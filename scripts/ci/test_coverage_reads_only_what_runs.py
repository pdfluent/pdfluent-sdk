#!/usr/bin/env python3
"""Coverage is what a job RUNS, not what a file says.

`test_feature_gated_tests_run.py` decides whether a feature-gated test has a job
by looking for `cargo test ... --features x`. It used to look in the raw text of
the pipeline files, so anything containing that string counted -- a comment, a
job's `name`, a line in the prose explaining why the job exists.

That is the same defect the guard itself exists to name, one level up: the
epitaph of a deleted job kept the gate green. Measured 03-09-2026 on the real
tree, both shapes:

    run: true
    # was: cargo test -p pdf-ocr --features tesseract --locked   -> still green

    name: cargo test -p pdf-ocr --features tesseract --locked
    run: true                                                    -> still green

Both are red now, and this file is what keeps them red. (T3, #1685)
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys

HIER = pathlib.Path(__file__).resolve().parent
GUARD = HIER / "test_feature_gated_tests_run.py"

fouten: list[str] = []


def expect(wat: str, ok: bool, detail: str = "") -> None:
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:180]}"))
    if not ok:
        fouten.append(wat)


def laad():
    spec = importlib.util.spec_from_file_location("featgate", GUARD)
    mod = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(mod)
    except SystemExit:
        pass
    return mod


def paren(mod, tekst: str):
    """(pakket, feature) pairs the guard would count for this file."""
    regels = mod.uitvoerregels(tekst)
    if regels is None:
        return None
    return mod.ci_dekking("\n".join(regels))[0]


WERKELIJK = """
jobs:
  t:
    name: pdf-ocr feature tests
    steps:
      - run: cargo test -p pdf-ocr --features tesseract --locked
"""

COMMENTAAR = """
jobs:
  t:
    name: pdf-ocr feature tests
    steps:
      # was: cargo test -p pdf-ocr --features tesseract --locked
      - run: "true"
"""

JOBNAAM = """
jobs:
  t:
    name: cargo test -p pdf-ocr --features tesseract --locked
    steps:
      - run: "true"
"""

GITLAB = """
sanity:ocr:
  script:
    - cargo test -p pdf-ocr --features tesseract --locked
"""

# A workflow nobody can reach without typing the dispatch. The four corpus gates
# are this shape, and a `cargo test --features x` inside one of them would have
# counted as coverage for a job that has not run since May (#276).
OP_VERZOEK = """
on:
  workflow_dispatch:
jobs:
  t:
    steps:
      - run: cargo test -p pdf-ocr --features tesseract --locked
"""

BLOKKEREND = """
on:
  pull_request:
    branches: [master]
jobs:
  t:
    steps:
      - run: cargo test -p pdf-ocr --features tesseract --locked
"""

# The shape the fix is about: a whole test file gated at the top. Both walks in
# `gated_features_met_tests` read only attribute lines touching a `#[test]`, and
# a crate-level attribute sits thirty lines above the first one.
BESTANDSGATE = """
//! Constructing an engine must never fetch anything by itself.

#![cfg(feature = "paddle")]

use pdf_ocr::paddle::PaddleOcrConfig;

#[test]
fn the_default_source_is_local_only() {
    assert!(true);
}
"""

ALLES = """
jobs:
  t:
    steps:
      - run: cargo test --workspace --all-features
"""


def main() -> int:
    if not GUARD.is_file():
        print(f"SKIPPED (not a pass): {GUARD} is missing, so nothing was checked",
              file=sys.stderr)
        return 3
    mod = laad()
    if not hasattr(mod, "uitvoerregels"):
        print("  FAIL  the guard has no `uitvoerregels`, so it still judges coverage "
              "from raw text -- a comment or a job name counts as a job")
        print("\n  1 assertion(s) ran, 1 failure(s)")
        return 1

    print("coverage reads only what runs")
    expect("a real `run:` step counts",
           ("pdf-ocr", "tesseract") in (paren(mod, WERKELIJK) or set()))
    expect("a COMMENT naming the command does not",
           ("pdf-ocr", "tesseract") not in (paren(mod, COMMENTAAR) or set()),
           "a deleted job's epitaph kept the gate green")
    expect("a job NAME carrying the command does not",
           ("pdf-ocr", "tesseract") not in (paren(mod, JOBNAAM) or set()),
           "a name is a label, not a command")
    # About the reader, not the policy: `script:` is still an execution key, and
    # the mirror is simply no longer among the files handed to it.
    expect("a `script:` list is read as commands",
           ("pdf-ocr", "tesseract") in (paren(mod, GITLAB) or set()))
    expect("`--all-features` in a run is still seen",
           mod.ci_dekking("\n".join(mod.uitvoerregels(ALLES)))[1])
    expect("a file that is not YAML is refused, not guessed at",
           mod.uitvoerregels("jobs: [unclosed\n  - : :") is None,
           "returning the raw text on a parse error is how the hole got here")

    # --- and only where a change has to pass ---------------------------------
    #
    # The mirror was the first version of this defect and the reason this file
    # exists. The second is a workflow in the right repository that still never
    # runs against the change in front of it (#276).
    import yaml
    expect("a dispatch-only workflow is not coverage",
           not mod.blokkeert_een_wijziging(yaml.safe_load(OP_VERZOEK)),
           "a job somebody has to type is not a gate a change passes")
    expect("a pull_request workflow is",
           mod.blokkeert_een_wijziging(yaml.safe_load(BLOKKEREND)))
    expect("and the GitLab mirror is no longer read at all",
           not hasattr(mod, "CI"),
           "it is 204 commits behind and blocks nothing, so a job on it was a "
           "claim about a pipeline that never judged this change")

    # --- a whole file gated at the top ---------------------------------------
    #
    # Measured 05-09-2026 on the real tree: three test files gate themselves with
    # `#![cfg(feature = ...)]` and this function returned nothing for all three.
    # One of them, crates/pdf-engine/tests/text_span_info_contract.rs, no longer
    # COMPILES -- `TextSpan` grew five fields since anything last built it. That
    # is exactly the rot this guard was written to catch, sitting inside the
    # guard's own blind spot.
    expect("a crate-level `#![cfg(feature)]` gates every test in the file",
           mod.gated_features_met_tests(BESTANDSGATE) == {"paddle"},
           str(mod.gated_features_met_tests(BESTANDSGATE)))
    expect("a file with the gate and no test counts for nothing",
           mod.gated_features_met_tests(
               '#![cfg(feature = "paddle")]\nfn helper() {}\n') == set())

    # --- a default feature is not a gap --------------------------------------
    #
    # The other half of the same honesty: seven rows in feature_gaps.toml
    # excused features that are ON by default, so every workspace run had been
    # compiling them. A guard that reports gaps that are not there is one
    # somebody switches off.
    pakket = HIER.parent.parent / "crates" / "pdf-manip" / "Cargo.toml"
    if pakket.is_file():
        expect("a default feature is read as on",
               "pdfa-convert" in mod.standaardfeatures(pakket),
               str(sorted(mod.standaardfeatures(pakket))))
    expect("`--workspace` builds every package with its defaults",
           mod.standaardbouw("cargo test --workspace --no-fail-fast")[1])
    expect("`--no-default-features` does not",
           not mod.standaardbouw("cargo test --workspace --no-default-features")[1],
           "the flag is the whole point of the line")

    print(f"\n  {6 + 7 + (1 if pakket.is_file() else 0)} assertion(s) ran, "
          f"{len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
