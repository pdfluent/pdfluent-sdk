#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Cases for the browser-bundle guard (#383).

Every case here is a way of being GREEN FOR THE WRONG REASON, because that is
the only failure that matters in a size guard: a red one gets looked at, and a
green one that checked nothing gets believed. The cargo call is stubbed, so
these run in under a second and need no wasm toolchain.
"""
import os
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import the_browser_bundle_carries_no_fonts as guard  # noqa: E402

FAILURES: list[str] = []


def judge(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        FAILURES.append(f"{name}{': ' + detail if detail else ''}")
        print(f"  FAIL {name}{': ' + detail if detail else ''}")


class StubRun:
    """Stands in for subprocess.run so the cases pick cargo's answer."""

    def __init__(self, stdout: str = "", stderr: str = "", returncode: int = 0,
                 raises: BaseException | None = None):
        self.stdout, self.stderr, self.returncode = stdout, stderr, returncode
        self.raises = raises

    def __call__(self, *args, **kwargs):
        if self.raises:
            raise self.raises
        return subprocess.CompletedProcess(
            args=args[0], returncode=self.returncode,
            stdout=self.stdout, stderr=self.stderr)


def with_cargo(stub: StubRun):
    saved = guard.subprocess.run
    guard.subprocess.run = stub
    return saved


def restore(saved) -> None:
    guard.subprocess.run = saved


def manifest_cases() -> None:
    print("the manifests:")
    saved_root = guard.REPO
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        guard.REPO = root
        crate = root / "crates" / "pdf-manip"
        crate.mkdir(parents=True)
        manifest = crate / "Cargo.toml"

        manifest.write_text(
            "[dependencies]\nlopdf = \"1\"\n\n"
            "[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]\n"
            "pdf-substitute-fonts = { path = \"../pdf-substitute-fonts\" }\n")
        judge("a target-conditional declaration passes",
              guard.every_declaration_is_target_conditional() == [])

        # The case that carries the file: someone moves it back.
        manifest.write_text(
            "[dependencies]\n"
            "pdf-substitute-fonts = { path = \"../pdf-substitute-fonts\" }\n")
        problems = guard.every_declaration_is_target_conditional()
        judge("a plain [dependencies] declaration is refused", len(problems) == 1)
        judge("the refusal names the file and the line",
              problems and "Cargo.toml:2" in problems[0], f"got {problems}")

        # A cfg WITHOUT the `not(` is the subtle one: it reads as target-aware
        # and is exactly backwards.
        manifest.write_text(
            "[target.'cfg(target_arch = \"wasm32\")'.dependencies]\n"
            "pdf-substitute-fonts = { path = \"../pdf-substitute-fonts\" }\n")
        judge("a wasm32-only declaration is refused, not mistaken for the rule",
              len(guard.every_declaration_is_target_conditional()) == 1)

        # A comment mentioning the crate must not count as a declaration; a
        # guard that trips on prose gets switched off.
        manifest.write_text(
            "# pdf-substitute-fonts = ... was here once\n"
            "[dependencies]\nlopdf = \"1\"\n")
        judge("a commented-out line is not a declaration",
              guard.every_declaration_is_target_conditional() == [])

        # And a crate that simply does not use it is fine.
        manifest.write_text("[dependencies]\nlopdf = \"1\"\n")
        judge("a manifest that never names it passes",
              guard.every_declaration_is_target_conditional() == [])
    guard.REPO = saved_root


def graph_cases() -> None:
    print("the wasm graph:")

    saved = with_cargo(StubRun(stderr="warning: nothing to print.\n"))
    judge("an empty inverted tree is the pass",
          guard.the_font_crate_is_out_of_the_wasm_graph() == [])
    restore(saved)

    # The case that carries the file: the crate is back in the graph.
    saved = with_cargo(StubRun(stdout=(
        "pdf-substitute-fonts v1.0.0 (/x/crates/pdf-substitute-fonts)\n"
        "└── pdf-manip v1.0.0 (/x/crates/pdf-manip)\n"
        "    └── xfa-wasm v1.0.0 (/x/crates/xfa-wasm)\n")))
    problems = guard.the_font_crate_is_out_of_the_wasm_graph()
    judge("a crate in the graph is refused", len(problems) > 0)
    judge("the refusal names the path a reader has to follow",
          any("pdf-manip" in p for p in problems),
          f"got {problems}")
    judge("the refusal names the claim it breaks",
          any("B07" in p for p in problems), f"got {problems}")
    restore(saved)

    # Exit 0 with an answer we do not recognise must NOT read as clean. A guard
    # that treats "no match" as "all good" is how a renamed crate goes unnoticed.
    saved = with_cargo(StubRun(stdout="some future cargo output\n"))
    judge("an unrecognised answer is refused rather than passed",
          len(guard.the_font_crate_is_out_of_the_wasm_graph()) > 0)
    restore(saved)

    saved = with_cargo(StubRun(returncode=101, stderr="error: no such package\n"))
    judge("a failing cargo is refused",
          len(guard.the_font_crate_is_out_of_the_wasm_graph()) > 0)
    restore(saved)

    saved = with_cargo(StubRun(raises=FileNotFoundError()))
    problems = guard.the_font_crate_is_out_of_the_wasm_graph()
    judge("no cargo announces a skip, and says it is not a pass",
          problems and problems[0].startswith("SKIPPED (not a pass)"),
          f"got {problems}")
    restore(saved)


def size_cases() -> None:
    print("the built bundle:")
    saved_root = guard.REPO
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        guard.REPO = root

        judge("no bundle is not a failure", guard.a_built_bundle_is_under_the_ceiling() == [])

        pkg = root / "pkg"
        pkg.mkdir()
        bundle = pkg / "xfa_wasm_bg.wasm"

        # The size the 1.0.0 dry run actually measured, 09-09-2026. A fixture
        # written as "some number under the ceiling" tells a later reader
        # nothing about whether the ceiling fits the thing it governs.
        bundle.write_bytes(b"\0" * 14_619_146)
        judge("the measured 1.0.0 bundle passes",
              guard.a_built_bundle_is_under_the_ceiling() == [])

        bundle.write_bytes(b"\0" * 15_600_000)
        problems = guard.a_built_bundle_is_under_the_ceiling()
        judge("a bundle over the ceiling is refused", len(problems) == 1)
        judge("the refusal says how big it actually is",
              problems and "15.6 MB" in problems[0], f"got {problems}")
        judge("and names the claim, and which of the two may move first",
              problems and "B07" in problems[0] and "never the other way round" in problems[0],
              f"got {problems}")

        # Exactly at the ceiling is not over it; an off-by-one here would fail
        # every build for a byte.
        bundle.write_bytes(b"\0" * guard.CEILING_BYTES)
        judge("exactly at the ceiling is not over it",
              guard.a_built_bundle_is_under_the_ceiling() == [])

    guard.REPO = saved_root


def the_ceiling_matches_the_claim() -> None:
    print("the number:")
    # The ceiling exists to hold a published claim. If someone raises it without
    # the claim moving, the guard still passes and the website is still wrong.
    # Megabytes are 10^6 here, the unit the claim is written in. This test
    # compared MiB against a figure written in MB until 10-09-2026, so "not by
    # a bundle's worth" allowed a fifth more slack than it read as (#519).
    judge("the ceiling is above B07 but not by a bundle's worth",
          guard.B07_UNPACKED_MB * 1_000_000 < guard.CEILING_BYTES
          <= (guard.B07_UNPACKED_MB + 1) * 1_000_000,
          f"B07 says {guard.B07_UNPACKED_MB} MB, ceiling is "
          f"{guard.CEILING_BYTES / 1_000_000:.1f} MB")


def main() -> int:
    manifest_cases()
    graph_cases()
    size_cases()
    the_ceiling_matches_the_claim()
    if FAILURES:
        print(f"[test_bundle] {len(FAILURES)} case(s) failed")
        return 1
    print("[test_bundle] every judgement holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
