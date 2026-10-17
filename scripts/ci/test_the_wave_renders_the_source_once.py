#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The guard of #547 against the shapes it exists to refuse.

Each case is a workflow file the guard reads, not a mocked call: the thing that
went wrong was a workflow, and a fixture that is not one proves nothing about
it. The mutation is in the fixture -- put the second call back, take the second
arm out of the one call -- and the case requires the guard to go red on it.
"""
from __future__ import annotations

import pathlib
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import the_wave_renders_the_source_once as guard  # noqa: E402

FAILURES: list[str] = []

EVAL = "python3 crates/pdf-manip/examples/pdfa_round2_eval.py"

CONVERT = """      - name: Convert the set
        run: |
          node benchmarks/pdfa/reproduce/compare.mjs \\
            --name candidate --workdir "$PDFA_AFTER"
"""

SECOND_ARM = """      - name: The other way of keeping the geometry, for comparison
        env:
          PDFA_OTHER: ${{ runner.temp }}/pdfa-other
        run: |
          node benchmarks/pdfa/reproduce/compare.mjs \\
            --name candidate --workdir "$PDFA_OTHER"
"""


def workflow(*step_bodies: str) -> str:
    return ("on: workflow_dispatch\njobs:\n  pdfa-fidelity:\n"
            "    runs-on: ubuntu-latest\n    steps:\n" + "".join(step_bodies))


def measure(*extra: str) -> str:
    arguments = " ".join(extra)
    return f"""      - name: Measure the four axes and every page
        run: |
          {EVAL} \\
            --source "$PDFA_SOURCE" --after "$PDFA_AFTER" {arguments} \\
            --list "${{{{ runner.temp }}}}/set-documents.txt" \\
            --dpi 72 --workers 4 --out "$PDFA_AFTER/all-pages.json"
"""


def verdict(text: str | None) -> int:
    """The guard's exit code over a directory holding this one workflow."""
    with tempfile.TemporaryDirectory() as folder:
        if text is not None:
            (pathlib.Path(folder) / "wave.yml").write_text(text, encoding="utf-8")
        return guard.main(["--workflows", folder])


def check(label, got, want):
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, expected {want!r}")


def the_shapes_that_render_once_pass():
    check("one arm, one call",
          verdict(workflow(CONVERT, measure())), 0)
    check("two arms, one call, the second named",
          verdict(workflow(CONVERT, SECOND_ARM, measure(
              '--also-after "$PDFA_OTHER" --also-out "$PDFA_OTHER/all-pages.json"'))),
          0)
    check("a borrowed before is not a rendered one",
          verdict(workflow(CONVERT, measure('--baseline-result "$BASELINE"'))), 0)


def a_second_call_is_a_second_render():
    both = workflow(CONVERT, SECOND_ARM, measure(),
                    measure().replace("$PDFA_AFTER", "$PDFA_OTHER"))
    check("the two-call shape is refused", verdict(both), 1)


def a_converted_arm_that_is_not_measured_is_refused():
    check("a second candidate nobody named",
          verdict(workflow(CONVERT, SECOND_ARM, measure())), 1)


def one_directory_under_two_flags_is_refused():
    check("the #454 spelling stays refused",
          verdict(workflow(CONVERT, measure('--before "$PDFA_AFTER"'))), 1)
    check("and so does the source under a candidate flag",
          verdict(workflow(CONVERT, measure(
              '--also-after "$PDFA_SOURCE" --also-out "$PDFA_OTHER/all-pages.json"'))),
          1)


def a_scan_that_saw_nothing_reports_nothing():
    check("no workflow at all is not a pass", verdict(None), 2)
    check("a workflow with no call is not a pass",
          verdict(workflow(CONVERT)), 2)


def the_repository_itself_passes():
    check("the wave as it stands renders the source once", guard.main([]), 0)


def main() -> int:
    the_shapes_that_render_once_pass()
    a_second_call_is_a_second_render()
    a_converted_arm_that_is_not_measured_is_refused()
    one_directory_under_two_flags_is_refused()
    a_scan_that_saw_nothing_reports_nothing()
    the_repository_itself_passes()
    for failure in FAILURES:
        print(f"FAIL {failure}")
    if FAILURES:
        return 1
    print("OK a job renders the source once, and a second candidate is named "
          "to the call that already renders it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
