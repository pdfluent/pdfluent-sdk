#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Cases for the default-tree-carries-no-ort guard (#641).

Every case here is a way of being GREEN FOR THE WRONG REASON, because that is
the only failure that matters in a default-purity guard: a red one gets looked
at, and a green one that checked nothing gets believed. The cargo call is
stubbed, so these run in under a second and need no toolchain.
"""
import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import the_default_tree_carries_no_ort as guard  # noqa: E402

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
        self.calls: list[list[str]] = []

    def __call__(self, *args, **kwargs):
        self.calls.append(list(args[0]))
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
        crate = root / "crates" / "pdf-ocr"
        crate.mkdir(parents=True)
        manifest = crate / "Cargo.toml"

        manifest.write_text(
            "[features]\ndefault = []\n\n[dependencies]\n"
            "ort = { version = \"=2.0.0-rc.13\", optional = true }\n")
        judge("an optional ort declaration passes",
              guard.every_ort_declaration_is_optional() == [])
        judge("an empty default passes",
              guard.no_default_feature_pulls_ort() == [])

        # The case that carries the file: someone makes it unconditional.
        manifest.write_text("[dependencies]\nort = \"2.0.0\"\n")
        problems = guard.every_ort_declaration_is_optional()
        judge("a non-optional ort declaration is refused", len(problems) == 1,
              f"got {problems}")
        judge("the refusal names the crate and the rule",
              problems and "optional = true" in problems[0] and "641" in problems[0],
              f"got {problems}")

        manifest.write_text("[dependencies]\nort-sys = \"2.0.0\"\n")
        judge("a non-optional ort-sys declaration is refused",
              len(guard.every_ort_declaration_is_optional()) == 1)

        # Multi-line inline tables count too: `optional = true` on the third
        # line is still optional.
        manifest.write_text(
            "[dependencies]\nort = {\n version = \"=2.0.0-rc.13\",\n optional = true,\n}\n")
        judge("a multi-line optional declaration passes",
              guard.every_ort_declaration_is_optional() == [])

        # Dev-dependencies never reach another crate's tree; the onnx-runtime
        # integration tests name ort there.
        manifest.write_text("[dev-dependencies]\nort = \"=2.0.0-rc.13\"\n")
        judge("a dev-dependency on ort passes",
              guard.every_ort_declaration_is_optional() == [])

        # [features] DEFINES the `ort` feature; that definition is what the
        # opt-in is built on, not a dependency edge.
        manifest.write_text("[features]\nort = [\"dep:pdf-ocr\"]\n")
        judge("an `ort` feature definition passes",
              guard.every_ort_declaration_is_optional() == [])

        manifest.write_text("[features]\ndefault = [\"ort\"]\n")
        problems = guard.no_default_feature_pulls_ort()
        judge("a default that names ort is refused", len(problems) == 1,
              f"got {problems}")

        manifest.write_text("[features]\ndefault = [\n \"paddle\",\n]\n")
        judge("a multi-line default that names paddle is refused",
              len(guard.no_default_feature_pulls_ort()) == 1)

        manifest.write_text("[features]\ndefault = [\"pdf-engine/ort\"]\n")
        judge("a default that names pdf-engine/ort is refused",
              len(guard.no_default_feature_pulls_ort()) == 1)

        # Substrings are not names: font-subset must never trip this.
        manifest.write_text(
            "[features]\ndefault = [\"signing\", \"font-subset\"]\n")
        judge("font-subset in a default passes",
              guard.no_default_feature_pulls_ort() == [])

        # A `default` key outside [features] enables nothing.
        manifest.write_text(
            "[dependencies]\nort = { version = \"2\", optional = true, default-features = false }\n")
        judge("default-features on a dependency is not a default list",
              guard.no_default_feature_pulls_ort() == [])

    guard.REPO = saved_root


def graph_cases() -> None:
    print("the trees:")

    clean = ("pdfluent v1.1.0 (/x/crates/pdfluent)\n"
             "├── pdf-engine v1.1.0 (/x/crates/pdf-engine)\n"
             "│   └── lopdf v0.1.0 (/x/crates/lopdf)\n")
    saved = with_cargo(StubRun(stdout=clean))
    judge("a tree without ort passes",
          guard.tree_has_no_ort("pdfluent", ["-e", "features"]) == [])
    restore(saved)

    dirty = ("pdf-engine v1.1.0 (/x/crates/pdf-engine)\n"
             "└── pdf-ocr v1.1.0 (/x/crates/pdf-ocr)\n"
             "    └── ort v2.0.0-rc.13\n")
    saved = with_cargo(StubRun(stdout=dirty))
    problems = guard.tree_has_no_ort("pdf-engine", ["-e", "features"])
    judge("a tree with ort is refused", len(problems) > 0, f"got {problems}")
    judge("the refusal shows the node a reader has to follow",
          any("ort v2.0.0-rc.13" in p for p in problems), f"got {problems}")
    judge("the refusal names the rule",
          any("641" in p for p in problems), f"got {problems}")
    restore(saved)

    sys_dirty = ("pdf-engine v1.1.0 (/x/crates/pdf-engine)\n"
                 "└── ort-sys v2.0.0-rc.13\n")
    saved = with_cargo(StubRun(stdout=sys_dirty))
    judge("a tree with ort-sys is refused",
          len(guard.tree_has_no_ort("pdf-engine", ["-e", "features"])) > 0)
    restore(saved)

    # `-e features` prints feature names; a feature NAMED ort with no ort
    # package behind it is not the failure (and cannot happen with defaults
    # off, which the manifest half holds).
    features_only = ("pdf-engine v1.1.0 (/x/crates/pdf-engine)\n"
                     "└── pdf-ocr v1.1.0 (/x/crates/pdf-ocr) feature \"paddle\"\n")
    saved = with_cargo(StubRun(stdout=features_only))
    judge("a feature name without the package passes",
          guard.tree_has_no_ort("pdf-engine", ["-e", "features"]) == [])
    restore(saved)

    effort = ("pdfluent v1.1.0 (/x/crates/pdfluent)\n"
              "└── effort v1.2.3\n")
    saved = with_cargo(StubRun(stdout=effort))
    judge("a package merely ending in ort passes",
          guard.tree_has_no_ort("pdfluent", ["-e", "features"]) == [])
    restore(saved)

    # Exit 0 with an answer we do not recognise must NOT read as clean.
    saved = with_cargo(StubRun(stdout="some future cargo output\n"))
    judge("an unrecognised answer is refused rather than passed",
          len(guard.tree_has_no_ort("pdfluent", ["-e", "features"])) > 0)
    restore(saved)

    saved = with_cargo(StubRun(returncode=101, stderr="error: no such package\n"))
    judge("a failing cargo is refused",
          len(guard.tree_has_no_ort("pdfluent", ["-e", "features"])) > 0)
    restore(saved)

    saved = with_cargo(StubRun(raises=FileNotFoundError()))
    problems = guard.tree_has_no_ort("pdfluent", ["-e", "features"])
    judge("no cargo announces a skip, and says it is not a pass",
          problems and problems[0].startswith("SKIPPED (not a pass)"),
          f"got {problems}")
    restore(saved)

    # The guard asks for the exact trees the spec names.
    stub = StubRun(stdout=clean)
    saved = with_cargo(stub)
    guard.tree_has_no_ort("pdfluent", ["-e", "features"])
    asked = stub.calls[0] if stub.calls else []
    judge("the default query carries -e features",
          "-e" in asked and "features" in asked, f"asked {asked}")
    restore(saved)

    stub = StubRun(stdout="xfa-wasm v1.1.0 (/x/crates/xfa-wasm)\n")
    saved = with_cargo(stub)
    guard.tree_has_no_ort("xfa-wasm", ["--target", "wasm32-unknown-unknown"])
    asked = stub.calls[0] if stub.calls else []
    judge("the wasm query names the target and the crate",
          "--target" in asked and "wasm32-unknown-unknown" in asked
          and "xfa-wasm" in asked, f"asked {asked}")
    restore(saved)


def main() -> int:
    manifest_cases()
    graph_cases()
    if FAILURES:
        print(f"[test_ortree] {len(FAILURES)} case(s) failed")
        return 1
    print("[test_ortree] every judgement holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
