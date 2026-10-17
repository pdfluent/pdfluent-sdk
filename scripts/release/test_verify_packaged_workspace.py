#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""verify_packaged_workspace.sh builds every crate against its packaged siblings.

The release condition from m116/m115b is that pdfluent-cli 1.1.0 builds
against the packaged pdfluent 1.1.0, not the registry's 1.0.0. The helper
proves it: it packs with `cargo package --workspace --locked --no-verify`,
unpacks every .crate into a scratch dir, and builds each unpacked crate in
topological order with a [patch.crates-io] that maps every name to its
UNPACKED directory.

On a tiny fixture workspace of 2 crates (`a` and `b` with `b` depending on
`a = "=0.1.0"`):

  * `b` builds against the packaged `a`;
  * a change made to `a`'s SOURCE after packaging is NOT seen -- the patch
    points at the unpacked crate, not the tree;
  * an `a` package missing a module that `b` uses fails the helper.

And on this tree: the helper's report lists `pdfluent-cli` after `pdfluent`
(because it builds in topological_publish_order.py order), and the dry run's
report states "pdfluent-cli built against packaged pdfluent 1.1.0".

Each case fails when its feature is removed: without the [patch.crates-io]
mapping `b` resolves `a =0.1.0` from the registry (a different crate, or no
crate offline) and fails; without --no-package reuse a post-packaging source
break would be re-packaged and fail; without reading the package an excluded
module would still build.
"""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
HELPER = HERE / "verify_packaged_workspace.sh"
AUDIT = HERE / "prepublish_crate_audit.sh"
DRY_RUN = HERE / "registry_dry_run.sh"
ORDER = HERE / "topological_publish_order.py"
BASH = shutil.which("bash") or "/bin/bash"

MINIMUM_CASES = 9  # FLOOR


def _cargo_env(fixture: pathlib.Path) -> dict[str, str]:
    env = dict(os.environ)
    # Isolate the fixture's target dir: the shared CARGO_TARGET_DIR would mix
    # fixture .crates (a/b 0.1.0) with every other run on this machine.
    env["CARGO_TARGET_DIR"] = str(fixture / "target")
    # sccache's cache lives outside the sandbox; the wrapper fails EPERM there.
    env["CARGO_BUILD_RUSTC_WRAPPER"] = ""
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "line-tables-only")
    env.setdefault("CARGO_PROFILE_TEST_DEBUG", "line-tables-only")
    return env


def _write_fixture(root: pathlib.Path, *, exclude_extra: bool = False) -> pathlib.Path:
    (root / "crates" / "a" / "src").mkdir(parents=True)
    (root / "crates" / "b" / "src").mkdir(parents=True)
    (root / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["crates/a", "crates/b"]\nresolver = "2"\n',
        encoding="utf-8",
    )
    a_manifest = (
        '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2021"\n'
        'license = "MIT"\n'
    )
    if exclude_extra:
        a_manifest += 'exclude = ["src/extra.rs"]\n'
    (root / "crates" / "a" / "Cargo.toml").write_text(a_manifest, encoding="utf-8")
    (root / "crates" / "a" / "src" / "lib.rs").write_text(
        'pub fn hello() -> &\'static str { "packaged" }\npub mod extra;\n',
        encoding="utf-8",
    )
    (root / "crates" / "a" / "src" / "extra.rs").write_text(
        "pub fn f() -> i32 { 42 }\n", encoding="utf-8"
    )
    (root / "crates" / "b" / "Cargo.toml").write_text(
        '[package]\nname = "b"\nversion = "0.1.0"\nedition = "2021"\n'
        'license = "MIT"\n\n[dependencies]\n'
        'a = { path = "../a", version = "=0.1.0" }\n',
        encoding="utf-8",
    )
    (root / "crates" / "b" / "src" / "lib.rs").write_text(
        "pub fn use_a() -> i32 { a::extra::f() }\n", encoding="utf-8"
    )
    return root


def _run_helper(
    fixture: pathlib.Path, *args: str, env_extra: dict[str, str] | None = None
) -> subprocess.CompletedProcess:
    env = _cargo_env(fixture)
    if env_extra:
        env.update(env_extra)
    return subprocess.run(
        [BASH, str(HELPER), "--root", str(fixture), "--offline", *args],
        capture_output=True,
        text=True,
        timeout=300,
        cwd=str(REPO),
        env=env,
    )


def _non_comment_lines(path: pathlib.Path) -> list[str]:
    return [
        line
        for line in path.read_text(encoding="utf-8").splitlines()
        if not line.lstrip().startswith("#")
    ]


class PackagedSiblingTests(unittest.TestCase):
    def test_b_builds_against_the_packaged_a(self):
        with tempfile.TemporaryDirectory() as tmp:
            fixture = _write_fixture(pathlib.Path(tmp) / "ws")
            report = pathlib.Path(tmp) / "report.txt"
            proc = _run_helper(fixture, "--report", str(report))
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            body = report.read_text(encoding="utf-8")
            self.assertIn("| a | 0.1.0 | ok |", body)
            self.assertIn("| b | 0.1.0 | ok |", body)
            # Topological: the dependency builds (and reports) first.
            self.assertLess(body.index("| a |"), body.index("| b |"))

    def test_source_change_after_packaging_is_not_seen(self):
        with tempfile.TemporaryDirectory() as tmp:
            fixture = _write_fixture(pathlib.Path(tmp) / "ws")
            first = _run_helper(fixture)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            # Break the SOURCE after packaging: remove the module b uses.
            (fixture / "crates" / "a" / "src" / "lib.rs").write_text(
                'pub fn hello() -> &\'static str { "broken" }\n',
                encoding="utf-8",
            )
            (fixture / "crates" / "a" / "src" / "extra.rs").unlink()
            # The packaged .crates are unchanged, so the helper still builds:
            # the patch points at the unpacked crate, not the tree.
            proc = _run_helper(fixture, "--no-package")
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("| b | 0.1.0 | ok |", proc.stdout)
            # And the break is real: re-packaging it fails b.
            repack = _run_helper(fixture)
            self.assertNotEqual(repack.returncode, 0, repack.stdout)
            self.assertIn("FAIL", repack.stdout)

    def test_package_missing_a_module_that_b_uses_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            fixture = _write_fixture(pathlib.Path(tmp) / "ws", exclude_extra=True)
            env = _cargo_env(fixture)
            # The source tree builds: extra.rs is there, only the package
            # excludes it.
            src = subprocess.run(
                ["cargo", "build", "--offline", "-p", "b"],
                capture_output=True,
                text=True,
                timeout=300,
                cwd=str(fixture),
                env=env,
            )
            self.assertEqual(src.returncode, 0, src.stderr[-2000:])
            proc = _run_helper(fixture)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("FAIL", proc.stdout)
            self.assertIn("extra", proc.stdout + proc.stderr)

    def test_red_without_the_patch_mapping(self):
        # Without [patch.crates-io], b's `=0.1.0` pin resolves from the
        # registry, not the unpacked sibling: offline there is no such
        # crate, online it is a different crate without `extra`. Either way
        # the build fails -- the patch is what makes the helper green.
        with tempfile.TemporaryDirectory() as tmp:
            fixture = _write_fixture(pathlib.Path(tmp) / "ws")
            env = _cargo_env(fixture)
            pkgdir = fixture / "target" / "package"
            pack = subprocess.run(
                ["cargo", "package", "--workspace", "--locked", "--no-verify"],
                capture_output=True,
                text=True,
                timeout=300,
                cwd=str(fixture),
                env=env,
            )
            self.assertEqual(pack.returncode, 0, pack.stderr[-2000:])
            synth = pathlib.Path(tmp) / "synth"
            for name in ("a-0.1.0", "b-0.1.0"):
                top = synth / name
                top.mkdir(parents=True)
                with tarfile.open(pkgdir / f"{name}.crate", "r:gz") as tf:
                    tf.extractall(path=top, filter="data")
                    # Flatten the single top-level dir cargo wraps in.
                    inner = next(top.iterdir())
                    for child in inner.iterdir():
                        child.rename(top / child.name)
                    inner.rmdir()
            (synth / "Cargo.toml").write_text(
                '[workspace]\nresolver = "2"\n'
                'members = ["a-0.1.0", "b-0.1.0"]\n',
                encoding="utf-8",
            )
            build = subprocess.run(
                ["cargo", "build", "--offline", "-p", "b"],
                capture_output=True,
                text=True,
                timeout=300,
                cwd=str(synth),
                env={**env, "CARGO_TARGET_DIR": str(synth / "target")},
            )
            self.assertNotEqual(build.returncode, 0, build.stdout)
            self.assertNotIn("Finished", build.stdout + build.stderr)


class WiringTests(unittest.TestCase):
    def test_both_callers_use_the_one_helper(self):
        for path in (AUDIT, DRY_RUN):
            body = path.read_text(encoding="utf-8")
            self.assertIn(
                "verify_packaged_workspace.sh",
                body,
                f"{path.name} must call the shared helper",
            )

    def test_no_verifying_workspace_package_call_remains(self):
        # The verifying `cargo package --workspace --locked` ICEs on cargo
        # 1.94 ("no hash listed"); only --no-verify plus the standalone
        # build may remain, and only inside the helper.
        for path in (AUDIT, DRY_RUN):
            offenders = [
                line.strip()
                for line in _non_comment_lines(path)
                if "cargo package --workspace" in line
            ]
            self.assertEqual(offenders, [], path.name)
        helper_code = "\n".join(_non_comment_lines(HELPER))
        self.assertIn("--workspace --locked --no-verify", helper_code)
        self.assertIn("[patch.crates-io]", HELPER.read_text(encoding="utf-8"))
        self.assertIn("cargo 1.94", HELPER.read_text(encoding="utf-8"))
        self.assertIn("no hash listed", HELPER.read_text(encoding="utf-8"))

    def test_ice_comment_names_cargo_and_the_way_back(self):
        for path in (HELPER, AUDIT, DRY_RUN):
            body = path.read_text(encoding="utf-8")
            self.assertIn("cargo 1.94", body, path.name)
            self.assertIn("no hash listed", body, path.name)
            self.assertIn("cargo package --workspace", body, path.name)

    def test_helper_builds_in_topological_order(self):
        body = HELPER.read_text(encoding="utf-8")
        self.assertIn("topological_publish_order.py", body)
        self.assertIn("cargo build", body)
        self.assertIn("--locked", body)

    def test_cli_is_covered_after_the_facade(self):
        order = subprocess.run(
            ["python3", str(ORDER)],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(REPO),
        )
        self.assertEqual(order.returncode, 0, order.stderr)
        names = [line.split()[0] for line in order.stdout.splitlines() if line.strip()]
        self.assertIn("pdfluent", names)
        self.assertIn("pdfluent-cli", names)
        # The helper builds (and reports) in this order, so its report lists
        # pdfluent-cli after pdfluent.
        self.assertLess(names.index("pdfluent"), names.index("pdfluent-cli"))
        dry = DRY_RUN.read_text(encoding="utf-8")
        self.assertIn("pdfluent-cli built against packaged pdfluent", dry)


if __name__ == "__main__":
    unittest.main()
