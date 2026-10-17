#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The default build stays pure Rust: no ort/ort-sys without asking (#641).

The shared ONNX Runtime layer (`pdf_ocr::runtime`) and the model tiers on top
(`ort`, `ort-layout`, `ort-tables`, `paddle`) are opt-in. The default build --
including the browser bundle -- must not link, load, or even resolve ONNX
Runtime. Cargo unifies features across the whole graph, so one dependant with
defaults on would switch it back on for everyone; that is exactly the drift
this holds.

Two halves, failing independently:

* `cargo tree -e features -p <pkg>` with DEFAULT features for `pdf-engine`
  and `pdfluent`, plus `cargo tree --target wasm32-unknown-unknown -p
  xfa-wasm`: none may mention the `ort` or `ort-sys` packages. This is the
  half that would catch a path the text scan cannot see.
* the manifests, which always run: every `ort`/`ort-sys` dependency
  declaration must be optional (dev-dependencies excepted -- they never reach
  another crate's tree), and no `default` feature list may name an
  ort-pulling feature. It reads text, needs no cargo and no network, so it
  answers the same on a hosted runner as it does here.
"""
import os
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(os.environ.get("PDFLUENT_ORT_GUARD_ROOT")
                    or pathlib.Path(__file__).resolve().parents[2])

# The packages that must stay out of the default and wasm graphs, and the
# feature names that would pull them in through a `default` list.
ORT_PACKAGES = ("ort", "ort-sys")
ORT_FEATURES = {"ort", "ort-sys", "onnx-runtime", "paddle", "ort-layout", "ort-tables"}

# `<pkg> v<version>`, as `cargo tree` prints a package node: whitespace (or
# start of line) before the name, so `effort v1.0` does not match, and a
# version after it, so `feature "ort"` does not match.
ORT_NODE = re.compile(r"(?:^|\s)ort(-sys)? v\d", re.MULTILINE)

DEFAULT_PACKAGES = ("pdf-engine", "pdfluent")
WASM_TARGET = "wasm32-unknown-unknown"
WASM_PACKAGE = "xfa-wasm"


def cargo() -> str:
    """Where cargo actually is; a hosted runner may not put it on PATH."""
    from shutil import which
    found = which("cargo")
    if found:
        return found
    for candidate in (
        pathlib.Path(os.environ.get("CARGO_HOME", "")) / "bin" / "cargo",
        pathlib.Path.home() / ".cargo" / "bin" / "cargo",
    ):
        if candidate.is_file():
            return str(candidate)
    return "cargo"


def every_ort_declaration_is_optional() -> list[str]:
    """No manifest may declare ort/ort-sys as a non-optional dependency.

    Text, not cargo: the edge is added by writing a line in a manifest, so
    the manifest is where the rule belongs. `[dev-dependencies]` is exempt --
    a dev-dependency never reaches another crate's `cargo tree`, and the
    `onnx-runtime` integration tests name `ort` there for that reason.
    """
    problems = []
    for manifest in sorted(REPO.glob("crates/*/Cargo.toml")):
        table = ""
        rows = manifest.read_text(encoding="utf-8", errors="ignore").split("\n")
        number = 0
        while number < len(rows):
            number += 1
            row = rows[number - 1]
            stripped = row.strip()
            if stripped.startswith("[") and not stripped.startswith("[["):
                table = stripped
                continue
            if stripped.startswith("#") or "=" not in stripped:
                continue
            name = stripped.split("=", 1)[0].strip().strip('"')
            if name not in ORT_PACKAGES:
                continue
            # Only dependency tables declare dependencies: [features] DEFINES
            # the `ort` feature (which must exist, off by default), and
            # [dev-dependencies] never reaches another crate's tree.
            if table == "[features]" or "dev-dependencies" in table:
                continue
            # An inline table may run over several lines; accumulate until
            # the braces balance before asking whether it is optional.
            declaration = stripped
            depth = declaration.count("{") - declaration.count("}")
            while depth > 0 and number < len(rows):
                declaration += " " + rows[number].strip()
                number += 1
                depth = declaration.count("{") - declaration.count("}")
            if "optional=true" in declaration.replace(" ", ""):
                continue
            problems.append(
                f"{manifest.relative_to(REPO)}:{number} declares {name} "
                f"under {table or '[dependencies]'} without `optional = true`, "
                f"so ONNX Runtime reaches every default build. The shared layer "
                f"is opt-in (#641): the declaration stays optional."
            )
    return problems


def no_default_feature_pulls_ort() -> list[str]:
    """No `default` list may name an ort-pulling feature.

    The other half of the cargo-free backstop: `default = ["ort"]` anywhere
    would put `ort` in the graphs below without touching a dependency line.
    Quoted names compare exactly, so `font-subset` does not trip on its
    substring and `pdf-engine/ort` still counts through its feature half.
    """
    problems = []
    for manifest in sorted(REPO.glob("crates/*/Cargo.toml")):
        rows = manifest.read_text(encoding="utf-8", errors="ignore").split("\n")
        number = 0
        while number < len(rows):
            number += 1
            row = rows[number - 1]
            stripped = row.strip()
            if stripped.startswith("#") or "=" not in stripped:
                continue
            name = stripped.split("=", 1)[0].strip()
            if name != "default":
                continue
            # Only the [features] table's `default` enables features; a
            # `default = ...` key anywhere else (a dependency's
            # `default-features`, a profile) is not a feature list.
            table = ""
            for back in range(number - 1, 0, -1):
                candidate = rows[back - 1].strip()
                if candidate.startswith("[") and not candidate.startswith("[["):
                    table = candidate
                    break
            if table != "[features]":
                continue
            listed = stripped.split("=", 1)[1]
            while "]" not in listed and number < len(rows):
                listed += " " + rows[number].strip()
                number += 1
            for token in re.findall(r'"([^"]+)"', listed):
                feature = token.split("/", 1)[-1].removeprefix("dep:")
                if feature in ORT_FEATURES:
                    problems.append(
                        f"{manifest.relative_to(REPO)}:{number} enables {token!r} "
                        f"by default, which pulls ONNX Runtime into every "
                        f"default build. The model tiers are opt-in (#641)."
                    )
    return problems


def tree_has_no_ort(package: str, extra: list[str]) -> list[str]:
    """`cargo tree` for `package` must not mention the ort packages."""
    try:
        done = subprocess.run(
            [cargo(), "tree", *extra, "-p", package],
            cwd=REPO, capture_output=True, text=True, timeout=300,
        )
    except FileNotFoundError:
        return ["SKIPPED (not a pass): cargo is not on PATH"]
    except subprocess.TimeoutExpired:
        return [f"cargo tree -p {package} did not finish within 300s"]

    if done.returncode != 0:
        tail = (done.stderr or "").strip().splitlines()[-1:] or ["no output"]
        return [f"cargo tree -p {package} failed ({done.returncode}): {tail}"]
    if ORT_NODE.search(done.stdout or ""):
        who = [r.strip() for r in (done.stdout or "").strip().split("\n")
               if ORT_NODE.search(r)][:12]
        where = " ".join(extra + ["-p", package])
        return [f"ort is in `cargo tree {where}`, so ONNX Runtime reaches a "
                f"build that did not ask for it (#641). Nodes:",
                *[f"    {r}" for r in who]]
    if f"{package} v" not in (done.stdout or ""):
        # Exit 0, no ort, no root node either: the shape changed under us and
        # a silent pass here is exactly the failure this file exists to
        # prevent.
        return [f"cargo tree -p {package} gave an answer this guard does not "
                f"recognise, so it cannot say whether ort is in the graph: "
                f"{(done.stdout or '').strip()[:200]!r}"]
    return []


def main() -> int:
    problems = every_ort_declaration_is_optional()
    problems += no_default_feature_pulls_ort()
    skipped: list[str] = []
    for package in DEFAULT_PACKAGES:
        found = tree_has_no_ort(package, ["-e", "features"])
        skipped += [p for p in found if p.startswith("SKIPPED")]
        problems += [p for p in found if not p.startswith("SKIPPED")]
    found = tree_has_no_ort(WASM_PACKAGE, ["--target", WASM_TARGET])
    skipped += [p for p in found if p.startswith("SKIPPED")]
    problems += [p for p in found if not p.startswith("SKIPPED")]
    for line in skipped:
        # The manifest halves above still ran and still count; only the cargo
        # half is unavailable, and saying which one is missing is the
        # difference between a guard that skipped and a guard that passed.
        print(line)
    if problems:
        print("[ortree] FAIL:")
        for entry in problems:
            print(f"  {entry}")
        return 1
    print("[ortree] OK: every ort declaration is optional, no default pulls "
          "it, and it is in neither default graph nor the wasm graph")
    return 0


if __name__ == "__main__":
    sys.exit(main())
