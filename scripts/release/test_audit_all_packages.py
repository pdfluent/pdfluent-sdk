#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""audit-all-packages.sh judges what the packages actually carry.

Three defects, each measured on the 1.1.0 lane artefacts:

- The Python wheel carries License-Expression plus License-File (PEP 639,
  metadata 2.4) and no legacy License: line, which was all the audit
  grepped for. The expression is accepted when it equals the one in
  pyproject.toml, with every License-File it names in the wheel; the
  legacy field stays as the fallback.
- `local crates_to_audit=("${CRATES[@]:-}")` yields ONE empty word when
  CRATES is empty, so the length check never loaded the publish list and
  the channel audited 0 crates and passed. An empty list now fails the
  channel instead of passing it.
- Each crate's audit rewrites benchmarks/runs/prepublish_audits/, and the
  next crate's Gate 0 failed on it. Within one train run the whole
  reports directory is excused; anything else dirty still fails.

- The licence check grepped LICENSE for "PDFluent Commercial License"
  (wasm, a failure) or for any of that, MIT or Apache (Python, a warning).
  After #349 LICENSE is the AGPL text, so the wasm gate refused the correct
  1.1.0 package. Both channels now judge the package against
  docs/LICENSE_POLICY.toml through licence_representation.py.
- A scoped package name (`@pdfluent/sdk-wasm`) put the report one directory
  down, and nothing created that directory: the scan crashed after it ran.

The cases call the script's own functions (sourced, DRY_RUN where the
channel would write), so a failure here is about the audit and not about
today's tree: Gate 0 is never invoked and nothing is written.
"""
from __future__ import annotations

import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
SCRIPT = HERE / "audit-all-packages.sh"
RULE = HERE / "tree_dirt.sh"
ORDER = HERE / "topological_publish_order.py"
BASH = shutil.which("bash") or "/bin/bash"

MINIMUM_CASES = 29  # FLOOR

AGPL_TEXT = "GNU AFFERO GENERAL PUBLIC LICENSE\n" + ("terms...\n" * 20)
COMMERCIAL_TEXT = "PDFluent Commercial Licence\n" + ("terms...\n" * 20)
WASM_ENTRY = "crates/xfa-wasm/pkg/package.json"


def _wheel_texts(prefix: str = "x-0.1.dist-info/licenses/") -> dict[str, bytes]:
    return {
        prefix + "LICENSE": AGPL_TEXT.encode(),
        prefix + "LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
    }


def _pyproject_expression() -> str:
    text = (REPO / "crates" / "pdf-python" / "pyproject.toml").read_text()
    return re.search(r'^license = "([^"]+)"', text, re.M).group(1)


def _run_bash(script: str, *, env_extra: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    env = dict(os.environ)
    if env_extra:
        env.update(env_extra)
    return subprocess.run(
        [BASH, "-c", script],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(REPO),
        env=env,
    )


def _sourced() -> str:
    return f'AUDIT_ALL_PACKAGES_SOURCED=1 source {shlex.quote(str(SCRIPT))}; '


def _write_wheel(path: pathlib.Path, metadata: bytes, extra: dict[str, bytes] | None = None) -> pathlib.Path:
    entries = {
        "x/__init__.py": b"# x\n",
        "x-0.1.dist-info/METADATA": metadata,
    }
    entries.update(extra or {})
    with zipfile.ZipFile(path, "w", zipfile.ZIP_STORED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return path


def _audit_python(wheel: pathlib.Path) -> subprocess.CompletedProcess:
    return _run_bash(
        _sourced()
        + f"WHEEL_PATH={shlex.quote(str(wheel))}; DRY_RUN=true; audit_python"
    )


def _git_env() -> dict[str, str]:
    # No GIT_* inherited: the pre-push hook exports GIT_DIR, and these
    # cases run git against a temporary repository.
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(
        GIT_AUTHOR_NAME="t",
        GIT_AUTHOR_EMAIL="t@t",
        GIT_COMMITTER_NAME="t",
        GIT_COMMITTER_EMAIL="t@t",
    )
    return env


def _dirt(repo: pathlib.Path, *, train: bool) -> str:
    script = f'source {shlex.quote(str(RULE))}; pdfluent_tree_dirt {shlex.quote(str(repo))}'
    env = _git_env()
    if train:
        env["PDFLUENT_AUDIT_TRAIN"] = "1"
    out = subprocess.run(
        [BASH, "-c", script],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(repo),
        env=env,
    )
    return out.stdout.strip()


class PythonChannelTests(unittest.TestCase):
    def test_wheel_with_only_expression_and_license_file_passes(self):
        expr = _pyproject_expression()
        meta = (
            "Metadata-Version: 2.4\nName: x\nVersion: 0.1\n"
            f"License-Expression: {expr}\nLicense-File: LICENSE\n"
        ).encode()
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                meta,
                _wheel_texts(),
            )
            proc = _audit_python(whl)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("License-Expression", proc.stdout)

    def test_wheel_with_neither_field_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                b"Metadata-Version: 2.4\nName: x\nVersion: 0.1\n",
                {"LICENSE": AGPL_TEXT.encode()},
            )
            proc = _audit_python(whl)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("No License field", proc.stdout + proc.stderr)

    def test_legacy_license_field_still_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                b"Name: x\nVersion: 0.1\nLicense: MIT\n",
                _wheel_texts(""),
            )
            proc = _audit_python(whl)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)

    def test_wrong_expression_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                b"Metadata-Version: 2.4\nName: x\nVersion: 0.1\n"
                b"License-Expression: MIT\nLicense-File: LICENSE\n",
                {"LICENSE": AGPL_TEXT.encode()},
            )
            proc = _audit_python(whl)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("pyproject.toml", proc.stdout + proc.stderr)

    def test_license_file_named_but_missing_fails(self):
        expr = _pyproject_expression()
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                f"Metadata-Version: 2.4\nName: x\nVersion: 0.1\n"
                f"License-Expression: {expr}\nLicense-File: LICENSE\n".encode(),
                _wheel_texts(""),
            )
            proc = _audit_python(whl)
            # The file IS in the wheel here (basename match); remove it
            # and the audit must name it.
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            bare = _write_wheel(
                pathlib.Path(tmp) / "y-0.1-py3-none-any.whl",
                f"Metadata-Version: 2.4\nName: y\nVersion: 0.1\n"
                f"License-Expression: {expr}\nLicense-File: LICENSE\n".encode(),
            )
            proc = _audit_python(bare)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE", proc.stdout + proc.stderr)

    def test_wheel_without_the_commercial_text_fails(self):
        expr = _pyproject_expression()
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                f"Metadata-Version: 2.4\nName: x\nVersion: 0.1\n"
                f"License-Expression: {expr}\nLicense-File: LICENSE\n".encode(),
                {"x-0.1.dist-info/licenses/LICENSE": AGPL_TEXT.encode()},
            )
            proc = _audit_python(whl)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-COMMERCIAL is missing", proc.stderr)

    def test_wheel_whose_license_is_not_the_agpl_text_fails(self):
        expr = _pyproject_expression()
        texts = _wheel_texts()
        texts["x-0.1.dist-info/licenses/LICENSE"] = b"MIT License\n"
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_wheel(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                f"Metadata-Version: 2.4\nName: x\nVersion: 0.1\n"
                f"License-Expression: {expr}\nLicense-File: LICENSE\n".encode(),
                texts,
            )
            proc = _audit_python(whl)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("does not carry the AGPL-3.0-only text", proc.stderr)


def _policy_rep() -> dict:
    import tomllib

    pol = tomllib.loads((REPO / "docs" / "LICENSE_POLICY.toml").read_text())
    return pol["channel_representation"][WASM_ENTRY]


def _write_wasm_pkg(root: pathlib.Path, *, drop: tuple[str, ...] = (), **over) -> pathlib.Path:
    """A wasm pkg dir shaped like the 1.1.0 lane output, licence files real.

    LICENSE, LICENSE-COMMERCIAL and LICENSE-OFFER are the repository's own,
    so the passing case is judged on the texts that actually ship.
    """
    pkg = root / "pkg"
    pkg.mkdir()
    files = ["xfa_wasm_bg.wasm", "xfa_wasm.js", "xfa_wasm.d.ts",
             "LICENSE", "LICENSE-COMMERCIAL", "LICENSE-OFFER"]
    manifest = {
        "name": "@pdfluent/sdk-wasm",
        "version": "9.9.9",
        "license": _policy_rep()["publishes"],
        "files": files,
        "main": "xfa_wasm.js",
        "homepage": "https://pdfluent.com",
    }
    manifest.update(over)
    import json

    (pkg / "package.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (pkg / "xfa_wasm.js").write_text("export {};\n")
    (pkg / "xfa_wasm.d.ts").write_text("export {};\n")
    (pkg / "xfa_wasm_bg.wasm").write_bytes(b"\0asm\1\0\0\0")
    for name in ("LICENSE", "LICENSE-COMMERCIAL", "LICENSE-OFFER"):
        if name not in drop:
            shutil.copyfile(REPO / name, pkg / name)
    return pkg


def _audit_wasm(pkg: pathlib.Path, *, reports: pathlib.Path | None = None) -> subprocess.CompletedProcess:
    if reports is None:
        tail = "DRY_RUN=true; audit_wasm"
    else:
        tail = f"REPORTS_DIR={shlex.quote(str(reports))}; DRY_RUN=false; audit_wasm"
    return _run_bash(_sourced() + f"WASM_PKG_DIR={shlex.quote(str(pkg))}; " + tail)


class WasmChannelTests(unittest.TestCase):
    def test_package_as_the_policy_declares_it_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp))
            proc = _audit_wasm(pkg)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertIn("Licence representation matches", proc.stdout)

    def test_missing_license_offer_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp), drop=("LICENSE-OFFER",))
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-OFFER is missing", proc.stderr)

    def test_wrong_spdx_line_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp))
            offer = pkg / "LICENSE-OFFER"
            body = offer.read_text().splitlines(keepends=True)
            body[0] = "SPDX-License-Identifier: AGPL-3.0-only\n"
            offer.write_text("".join(body))
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-OFFER line 1", proc.stderr)

    def test_spdx_line_not_first_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp))
            offer = pkg / "LICENSE-OFFER"
            offer.write_text("PDFluent licence offer\n" + offer.read_text())
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-OFFER line 1", proc.stderr)

    def test_wrong_package_json_license_fails(self):
        for wrong in (_policy_rep()["canonical"], "PDFluent Commercial License",
                      "SEE LICENSE IN LICENSE", None):
            with self.subTest(license=wrong), tempfile.TemporaryDirectory() as tmp:
                pkg = _write_wasm_pkg(pathlib.Path(tmp), license=wrong)
                proc = _audit_wasm(pkg)
                self.assertNotEqual(proc.returncode, 0, proc.stdout)
                self.assertIn("package.json license is", proc.stderr)

    def test_offer_left_out_of_files_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(
                pathlib.Path(tmp),
                files=["xfa_wasm_bg.wasm", "xfa_wasm.js", "LICENSE", "LICENSE-COMMERCIAL"],
            )
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("files omits LICENSE-OFFER", proc.stderr)

    def test_missing_commercial_text_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp), drop=("LICENSE-COMMERCIAL",))
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-COMMERCIAL is missing", proc.stderr)

    def test_license_that_is_not_the_agpl_text_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp))
            (pkg / "LICENSE").write_text("PDFluent Commercial License\n")
            proc = _audit_wasm(pkg)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("does not carry the AGPL-3.0-only text", proc.stderr)

    def test_scoped_name_report_is_written(self):
        # Not a dry run: the transform and the leakage scan both run, on the
        # temporary copy, and the report for `@pdfluent/sdk-wasm` must land in
        # a reports directory that has no `@pdfluent/` yet.
        with tempfile.TemporaryDirectory() as tmp:
            pkg = _write_wasm_pkg(pathlib.Path(tmp))
            reports = pathlib.Path(tmp) / "reports"
            proc = _audit_wasm(pkg, reports=reports)
            self.assertNotIn("FileNotFoundError", proc.stdout + proc.stderr)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            for suffix in (".audit.json", ".audit.md"):
                self.assertTrue(
                    (reports / "@pdfluent" / f"sdk-wasm-9.9.9{suffix}").is_file(),
                    sorted(str(p) for p in reports.rglob("*")),
                )


class ReportPathTests(unittest.TestCase):
    def _scan(self, name: str, out: pathlib.Path, tree: pathlib.Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["python3", str(HERE / "audit_package_tree.py"), "--tree", str(tree),
             "--out", str(out), "--package-name", name, "--package-version", "1.0.0"],
            capture_output=True, text=True, timeout=120,
        )

    def test_scoped_name_creates_the_scope_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = pathlib.Path(tmp) / "tree"
            tree.mkdir()
            (tree / "README.md").write_text("# x\n")
            out = pathlib.Path(tmp) / "out"
            proc = self._scan("@pdfluent/sdk-wasm", out, tree)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
            self.assertTrue((out / "@pdfluent" / "sdk-wasm-1.0.0.audit.json").is_file())
            self.assertTrue((out / "@pdfluent" / "sdk-wasm-1.0.0.audit.md").is_file())

    def test_name_that_leaves_the_out_dir_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = pathlib.Path(tmp) / "tree"
            tree.mkdir()
            (tree / "README.md").write_text("# x\n")
            # Two levels deep, so an escape still lands inside `tmp`.
            out = pathlib.Path(tmp) / "a" / "b" / "out"
            proc = self._scan("../../escaped", out, tree)
            self.assertEqual(proc.returncode, 2, proc.stdout + proc.stderr)
            self.assertEqual(list(pathlib.Path(tmp).rglob("escaped-1.0.0.audit.*")), [])


class CrateListTests(unittest.TestCase):
    def test_default_list_is_every_publishable_crate(self):
        want = subprocess.run(
            ["python3", str(ORDER)],
            capture_output=True,
            text=True,
            timeout=120,
            cwd=str(REPO),
        )
        self.assertEqual(want.returncode, 0, want.stderr)
        names = [line.split()[0] for line in want.stdout.splitlines() if line.strip()]
        self.assertGreater(len(names), 0)
        proc = _run_bash(_sourced() + "CRATES=(); resolve_crates_to_audit")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.split(), names)

    def test_explicit_crates_win_over_the_publish_list(self):
        proc = _run_bash(
            _sourced() + 'CRATES=(pdf-foo pdf-bar); resolve_crates_to_audit'
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.split(), ["pdf-foo", "pdf-bar"])

    def test_empty_list_exits_non_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake = pathlib.Path(tmp) / "python3"
            fake.write_text("#!/bin/sh\nexit 0\n")
            fake.chmod(0o755)
            env = {"PATH": f"{tmp}{os.pathsep}{os.environ.get('PATH', '')}"}
            proc = _run_bash(
                _sourced() + "CRATES=(); resolve_crates_to_audit",
                env_extra=env,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            proc = _run_bash(
                _sourced() + "CRATES=(); DRY_RUN=true; audit_rust",
                env_extra=env,
            )
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("vacuous", proc.stdout + proc.stderr)


class TrainDirtTests(unittest.TestCase):
    def _train_repo(self, root: pathlib.Path) -> pathlib.Path:
        repo = root / "train"
        reports = repo / "benchmarks" / "runs" / "prepublish_audits"
        reports.mkdir(parents=True)
        env = _git_env()
        subprocess.run(["git", "init", "-q"], cwd=repo, check=True, env=env)
        (reports / "crate-a-1.0.0.md").write_text("report a\n")
        (reports / "crate-b-1.0.0.md").write_text("report b\n")
        subprocess.run(["git", "add", "."], cwd=repo, check=True, env=env)
        subprocess.run(
            ["git", "commit", "-qm", "reports"], cwd=repo, check=True, env=env
        )
        # The train rewrites tracked reports with a fresh temporary path.
        (reports / "crate-a-1.0.0.md").write_text("report a\n/tmp/run-1\n")
        (reports / "crate-b-1.0.0.md").write_text("report b\n/tmp/run-2\n")
        return repo

    def test_two_rewritten_reports_pass_in_train_mode_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = self._train_repo(pathlib.Path(tmp))
            self.assertEqual(_dirt(repo, train=True), "")
            strict = _dirt(repo, train=False)
            self.assertIn("crate-a-1.0.0.md", strict)
            self.assertIn("crate-b-1.0.0.md", strict)

    def test_stray_file_outside_the_reports_dir_still_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = self._train_repo(pathlib.Path(tmp))
            (repo / "stray.txt").write_text("not a report\n")
            dirt = _dirt(repo, train=True)
            self.assertIn("stray.txt", dirt)
            self.assertNotIn("prepublish_audits", dirt)


def _cabi_version() -> str:
    text = (REPO / "crates" / "pdf-capi" / "Cargo.toml").read_text()
    return re.search(r'^version\s*=\s*"([^"]+)"', text, re.M).group(1)


def _write_cabi_tarball(
    path: pathlib.Path, *, commercial: bool, version: str | None = None
) -> pathlib.Path:
    import io
    import tarfile

    top = f"pdfluent-capi-{version or _cabi_version()}"
    entries = {
        f"{top}/VERSION": (_cabi_version() + "\n").encode(),
        f"{top}/include/pdfluent.h": b"// header\n",
        f"{top}/lib/libpdf_capi.so": b"\x7fELF" + b"\x00" * 100,
        f"{top}/README.md": b"# capi\n",
        f"{top}/LICENSE": AGPL_TEXT.encode(),
    }
    if commercial:
        entries[f"{top}/LICENSE-COMMERCIAL"] = (
            "PDFluent Commercial Licence\n" + ("terms...\n" * 20)
        ).encode()
    with tarfile.open(path, "w:gz") as tf:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    return path


def _audit_cabi(tarball: pathlib.Path) -> subprocess.CompletedProcess:
    return _run_bash(
        _sourced()
        + f"CABI_TARBALL={shlex.quote(str(tarball))}; DRY_RUN=true; audit_cabi"
    )


class CabiChannelTests(unittest.TestCase):
    def test_tarball_with_both_licence_texts_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            ball = _write_cabi_tarball(
                pathlib.Path(tmp) / "pdfluent-capi-9.9.9.tar.gz",
                commercial=True,
            )
            proc = _audit_cabi(ball)
            self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)

    def test_tarball_without_the_commercial_text_fails(self):
        # The 1.1.0 lane tarball shipped the AGPL LICENSE only.
        with tempfile.TemporaryDirectory() as tmp:
            ball = _write_cabi_tarball(
                pathlib.Path(tmp) / "pdfluent-capi-9.9.9.tar.gz",
                commercial=False,
            )
            proc = _audit_cabi(ball)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("LICENSE-COMMERCIAL", proc.stdout + proc.stderr)

    def test_tarball_without_the_agpl_text_fails(self):
        import io
        import tarfile

        with tempfile.TemporaryDirectory() as tmp:
            top = f"pdfluent-capi-{_cabi_version()}"
            ball = pathlib.Path(tmp) / "pdfluent-capi-9.9.9.tar.gz"
            with tarfile.open(ball, "w:gz") as tf:
                entries = {
                    f"{top}/VERSION": (_cabi_version() + "\n").encode(),
                    f"{top}/include/pdfluent.h": b"// header\n",
                    f"{top}/lib/libpdf_capi.so": b"\x7fELF",
                    f"{top}/README.md": b"# capi\n",
                    f"{top}/LICENSE": b"not the licence text\n",
                    f"{top}/LICENSE-COMMERCIAL": (
                        "PDFluent Commercial Licence\n" + ("t\n" * 20)
                    ).encode(),
                }
                for name, data in entries.items():
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    tf.addfile(info, io.BytesIO(data))
            proc = _audit_cabi(ball)
            self.assertNotEqual(proc.returncode, 0, proc.stdout)
            self.assertIn("AGPL", proc.stdout + proc.stderr)


class CrateAuditWiringTests(unittest.TestCase):
    def test_prepublish_falls_back_to_the_workspace(self):
        # While the release's own dependencies are unpublished, a per-crate
        # verify resolves them from the registry and fails. The audit then
        # packages through the shared helper (ONE cargo package --workspace
        # --locked --no-verify, cargo 1.94's local overlay) instead of
        # failing the crate as blocked.
        body = (HERE / "prepublish_crate_audit.sh").read_text()
        helper = (HERE / "verify_packaged_workspace.sh").read_text()
        self.assertIn("verify_packaged_workspace.sh", body)
        self.assertIn("cargo package --workspace --locked", helper)
        self.assertIn("--no-verify", helper)
        self.assertIn("[patch.crates-io]", helper)
        self.assertIn("failed to select a version", body)
        self.assertIn("no hash", body)
        self.assertIn("no hash", helper)

    def test_prepublish_never_allows_dirty(self):
        # The freshness manifest keys the packaged bytes by HEAD, which is
        # sound only because step 1 refuses a dirty tree. An --allow-dirty
        # on a cargo invocation would package uncommitted bytes under a
        # committed key. (The operator note telling the publisher to pass
        # no --allow-dirty is the one allowed spelling.)
        body = (HERE / "prepublish_crate_audit.sh").read_text()
        offenders = [
            line.strip()
            for line in body.splitlines()
            if "--allow-dirty" in line and "no `--allow-dirty`" not in line
        ]
        self.assertEqual(offenders, [])

    def test_engine_dev_deps_on_extractors_stay_versionless(self):
        # pdf-engine's dev-dependencies on the Office extractors must stay
        # path-only (the test-skip shape): cargo strips a path-only
        # dev-dependency when it packages, but a versioned one has to
        # resolve on the index -- where docx-extract and markup-extract
        # never land (publish = false) and the other three land only after
        # this engine. A version here makes pdf-engine 1.1.0 unpackagable,
        # before AND after the publish, and takes the workspace packaging
        # down with it.
        text = (REPO / "crates" / "pdf-engine" / "Cargo.toml").read_text()
        dev = text.split("[dev-dependencies]", 1)[1].split("\n[", 1)[0]
        offenders = []
        for crate in (
            "docx-extract",
            "pptx-extract",
            "xlsx-extract",
            "markup-extract",
            "epub-extract",
        ):
            for line in dev.splitlines():
                if line.startswith(crate + " ="):
                    if "version" in line:
                        offenders.append(line.strip())
                    break
            else:
                offenders.append(f"MISSING dev-dependency on {crate}")
        self.assertEqual(offenders, [])


if __name__ == "__main__":
    unittest.main()
