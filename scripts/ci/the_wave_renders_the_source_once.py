#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job renders the corpus for the page eval once, whatever it measures (#547).

`pdfa_round2_eval.py` renders the source and every candidate it is given. Two
arms of one experiment used to call it twice, and each call rendered the source:
four sets of the corpus for two candidates, where three answer for both. On
`pdfa-300` (8 293 pages) the redundant set is minutes of an instance hour, and
on `tuning1000` it is the difference between a fidelity run that fits in its
paid hour and one that starts a second.

The eval takes a second candidate itself (`--also-after`, `--also-out`), so one
call measures both arms and each keeps its own file. What this guard refuses is
the shape that made the waste:

  1. TWO CALLS IN ONE JOB. The second one renders the source again -- the same
     bytes, the same dpi, the same reader as the first.
  2. A SECOND ARM THAT IS NOT MEASURED BY THE ONE CALL. A job that converts a
     second candidate (`PDFA_OTHER`) and does not name it in the call either
     measures nothing with it, or measures it in a call this guard did not see.
  3. ONE DIRECTORY UNDER TWO FLAGS. `--before "$PDFA_AFTER" --after
     "$PDFA_AFTER"` was #454: a third set rendered for columns that came out
     copies, and an `output_pixels_identical` comparing a file with itself.

The arithmetic inside the eval is tested where it lives
(`crates/pdf-manip/examples/test_pdfa_round2_eval.py`). This guard is about the
caller, because that is where the second render came from: both calls were
correct on their own.
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
EVAL = "pdfa_round2_eval.py"
# The flags that name a directory the eval reads, and therefore renders.
DIRECTORY_FLAGS = ("--source", "--before", "--after", "--also-after")


def calls(script: str) -> list[str]:
    """Every invocation of the eval in one `run:` block, joined into one line.

    A shell command continued with backslashes is one command; read line by
    line its flags belong to nothing.
    """
    joined = re.sub(r"\\\n\s*", " ", script)
    found = []
    for line in joined.splitlines():
        if EVAL in line:
            found.append(" ".join(line.split()))
    return found


def flag(call: str, name: str) -> str | None:
    """The argument of one flag, quotes stripped, or None if it is absent."""
    match = re.search(rf"{re.escape(name)}\s+(\"[^\"]*\"|'[^']*'|\S+)", call)
    if match is None:
        return None
    value = match.group(1)
    if value[:1] in "\"'" and value[-1:] == value[:1]:
        value = value[1:-1]
    return value


def steps(job) -> list[dict]:
    return [s for s in (job or {}).get("steps") or [] if isinstance(s, dict)]


def inspect(path: pathlib.Path) -> tuple[int, list[str]]:
    """(invocations seen, complaints) for one workflow file."""
    document = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    seen, problems = 0, []
    for name, job in (document.get("jobs") or {}).items():
        shell = "\n".join(str(step.get("run", "")) for step in steps(job))
        found = [call for step in steps(job) for call in calls(str(step.get("run", "")))]
        seen += len(found)
        if len(found) > 1:
            problems.append(
                f"{path.name}:{name} calls {EVAL} {len(found)} times. Each call "
                "renders the source as well as its candidate, so the second one "
                "renders the corpus again for bytes the first already read. "
                "Measure the second candidate in the same call with "
                "`--also-after DIR --also-out FILE`; it gets its own file, with "
                "its own after_* columns (#547).")
        for call in found:
            named: dict[str, str] = {}
            for option in DIRECTORY_FLAGS:
                value = flag(call, option)
                if value is None:
                    continue
                if value in named:
                    problems.append(
                        f"{path.name}:{name} gives {named[value]} and {option} "
                        f"the same directory ({value}). The eval renders one set "
                        "per name, so that is a set rendered for a copy of "
                        "another, and a comparison of a file with itself (#454).")
                named[value] = option
        if found and "PDFA_OTHER" in shell and not flag(found[0], "--also-after"):
            problems.append(
                f"{path.name}:{name} converts a second candidate (PDFA_OTHER) and "
                f"does not name it to {EVAL}. Either it is measured in a call "
                "this guard cannot see, or it is not measured at all; the one "
                "call takes it as `--also-after` and shares the source render "
                "with it (#547).")
    return seen, problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workflows", type=pathlib.Path, default=WORKFLOWS,
                        help="Directory of workflow files to read")
    options = parser.parse_args(argv)
    seen, problems = 0, []
    # Both spellings: a workflow named .yaml runs exactly like one named .yml,
    # and a guard that reads only one extension is a guard with a way around it.
    for path in sorted(p for pattern in ("*.yml", "*.yaml")
                       for p in options.workflows.glob(pattern)):
        try:
            count, found = inspect(path)
        except yaml.YAMLError as error:
            print(f"[wave-source] FATAL: {path.name} does not parse: {error}",
                  file=sys.stderr)
            return 2
        seen += count
        problems += found
    if seen == 0:
        print(f"[wave-source] FATAL: no call to {EVAL} was found in "
              f"{options.workflows}. A scan that looked at nothing cannot "
              "report that the corpus is rendered once.", file=sys.stderr)
        return 2
    if problems:
        print(f"[wave-source] FAIL: {len(problems)} redundant render(s) of the "
              "corpus:", file=sys.stderr)
        for problem in problems:
            print(f"    {problem}", file=sys.stderr)
        return 1
    print(f"[wave-source] OK: {seen} call(s) to {EVAL}; each job renders the "
          "source once, and every candidate it converts is named to it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
