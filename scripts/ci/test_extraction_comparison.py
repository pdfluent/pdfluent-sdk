#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The extraction comparison is what it says it is (#390).

`a_published_number_names_its_run.py` checks that a published competitor figure
has a run file and a licence verdict behind it. This checks the half that lives
one level down: that the run files came from the list the repository publishes,
that the list is the rule and not a hand-picked sample, and that the arithmetic
inside the harness still does what its comments claim.

The four things that would each turn the comparison into a different
measurement while every file still looked right:

  1. The list drifts. Somebody adds a document to `gate-tail.txt`, or takes the
     awkward one out, and the table's denominators no longer come from a rule.
     Regenerating the list and comparing is the only check that catches a
     removal -- a rule you cannot rerun is a sample.

  2. A run was measured on a different list. The run files carry the SHA-256 of
     the list they read; if it is not the list in the tree, the numbers beside
     each other are not about the same documents. That is the failure that
     makes a comparison table wrong while every column is individually true.

  3. The columns were not measured together. Different machine classes, or a
     machine that was not idle, and the time axis stops meaning anything --
     `docs/PDFA_ROUND_PROTOCOL.md` step 3b, which is why the run files record
     the load average at all.

  4. The buckets stop partitioning the corpus. Assessed plus too-short plus
     no-source-text must be every document; the moment it is not, a median is
     being taken over a denominator nobody can name.

Exit codes:
    0  the comparison is reproducible and internally consistent
    1  it is not
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
EXTRACTION = REPO / "scripts" / "extraction"
COMPARISON = REPO / "benchmarks" / "extraction" / "comparison"
REGISTER = COMPARISON / "EXTRACTORS.toml"
LIST = COMPARISON / "gate-tail.txt"

SELF_TESTS = (
    EXTRACTION / "run_extractor_matrix.py",
    EXTRACTION / "build_comparison_list.py",
    EXTRACTION / "adapters" / "extract_text.py",
)

# FLOOR: the run files this test judges. Two columns and one document are a
# tree where every rule below is vacuously true, which reads exactly like a
# tree where they all hold.
MIN_RUNS = 3
MIN_DOCUMENTS = 50


def failures_of_self_tests() -> list[str]:
    problems = []
    for script in SELF_TESTS:
        if not script.is_file():
            problems.append(f"{script.relative_to(REPO)} is gone; the harness "
                            f"cannot be checked by running it")
            continue
        done = subprocess.run([sys.executable, str(script), "--self-test"],
                              capture_output=True, text=True, check=False)
        if done.returncode != 0:
            problems.append(f"{script.relative_to(REPO)} --self-test failed:\n"
                            + (done.stdout + done.stderr).strip())
    return problems


def the_list_is_still_the_rule() -> list[str]:
    """Regenerate it and compare. A rule you cannot rerun is a sample."""
    if not LIST.is_file():
        return [f"{LIST.relative_to(REPO)} is missing; the columns name no "
                f"document list"]
    with tempfile.TemporaryDirectory() as tmp:
        out = pathlib.Path(tmp) / "regenerated.txt"
        done = subprocess.run(
            [sys.executable, str(EXTRACTION / "build_comparison_list.py"),
             "--out", str(out)],
            capture_output=True, text=True, check=False)
        if done.returncode != 0:
            return [f"the list generator will not run: "
                    + (done.stdout + done.stderr).strip()]
        if out.read_text() != LIST.read_text():
            return [f"{LIST.relative_to(REPO)} is not what "
                    f"build_comparison_list.py produces. Either the rule "
                    f"changed and the list was not regenerated, or the list was "
                    f"edited by hand -- and a hand-edited list is a sample "
                    f"nobody can reproduce"]
    return []


def runs() -> list[tuple[pathlib.Path, dict]]:
    found = []
    for path in sorted(COMPARISON.glob("*.json")):
        try:
            found.append((path, json.loads(path.read_text())))
        except json.JSONDecodeError as exc:
            raise SystemExit(f"FAIL {path.name}: is not readable JSON ({exc})")
    return found


def the_runs_agree_with_each_other(found: list, digest: str) -> list[str]:
    problems = []
    if len(found) < MIN_RUNS:
        problems.append(f"{len(found)} comparison run(s), floor is {MIN_RUNS}. "
                        f"A comparison of two tools where one is ours is a "
                        f"measurement, not a comparison")
    shapes = set()
    for path, run in found:
        name = path.name
        if run.get("list_sha256") != digest:
            problems.append(
                f"{name}: was measured on a document list whose digest is "
                f"{str(run.get('list_sha256'))[:12]}, and the list in the tree "
                f"is {digest[:12]}. Columns measured on different documents do "
                f"not belong in one table")
        summary = run.get("summary", {})
        total = summary.get("documents", 0)
        if total < MIN_DOCUMENTS:
            problems.append(f"{name}: {total} document(s), floor is "
                            f"{MIN_DOCUMENTS}")
        buckets = (summary.get("recall_assessed", 0)
                   + summary.get("recall_too_short", 0)
                   + summary.get("recall_no_source_text", 0))
        if buckets != total:
            problems.append(
                f"{name}: the recall buckets hold {buckets} of {total} "
                f"documents. A median whose denominator does not add up is the "
                f"cheapest way to publish a flattering number")
        if not run.get("quiet_machine"):
            problems.append(
                f"{name}: the machine was not idle when the run began "
                f"(load {run.get('load_average_when_the_run_began')}), so its "
                f"time axis cannot be compared with the column beside it "
                f"(round protocol, step 3b)")
        shapes.add((run.get("machine_class"), run.get("date"),
                    run.get("timeout_s"), run.get("timing", {}).get("repeats")))
    if len(shapes) > 1:
        problems.append(
            f"the columns were not measured the same way: {sorted(shapes)}. "
            f"One machine, one day, one timeout, one repeat count -- otherwise "
            f"the table compares the rig with itself")
    return problems


def the_register_covers_every_run(found: list) -> list[str]:
    if not REGISTER.is_file():
        return [f"{REGISTER.relative_to(REPO)} is missing"]
    register = tomllib.loads(REGISTER.read_text())
    tools = register.get("extractors", {})
    problems = []
    on_the_rig = {name for name, entry in tools.items() if entry.get("on_the_rig")}
    measured = {run.get("extractor_id") for _, run in found}
    for missing in sorted(on_the_rig - measured):
        problems.append(
            f"{REGISTER.relative_to(REPO)}: {missing} is on the rig and no run "
            f"file measured it. A register that promises a column the table "
            f"does not have is the same defect as a column with no run")
    for extra in sorted(measured - on_the_rig):
        problems.append(f"a run measured {extra}, which the register does not "
                        f"put on the rig")
    if not any(tools.get(run.get("extractor_id"), {}).get("ours") for _, run in found):
        problems.append("no run is ours. A comparison we are not in is somebody "
                        "else's benchmark")
    return problems


def main() -> int:
    digest = hashlib.sha256(LIST.read_bytes()).hexdigest() if LIST.is_file() else ""
    found = runs()
    problems = (failures_of_self_tests()
                + the_list_is_still_the_rule()
                + the_runs_agree_with_each_other(found, digest)
                + the_register_covers_every_run(found))
    for problem in problems:
        print(f"FAIL: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"OK: {len(found)} column(s), one list, one machine, one session; the "
          f"list regenerates to itself.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
