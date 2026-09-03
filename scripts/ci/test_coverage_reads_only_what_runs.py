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
    expect("GitLab's `script:` list counts",
           ("pdf-ocr", "tesseract") in (paren(mod, GITLAB) or set()))
    expect("`--all-features` in a run is still seen",
           mod.ci_dekking("\n".join(mod.uitvoerregels(ALLES)))[1])
    expect("a file that is not YAML is refused, not guessed at",
           mod.uitvoerregels("jobs: [unclosed\n  - : :") is None,
           "returning the raw text on a parse error is how the hole got here")

    print(f"\n  6 assertion(s) ran, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
