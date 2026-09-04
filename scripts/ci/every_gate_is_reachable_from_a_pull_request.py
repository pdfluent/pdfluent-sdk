#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every gate the pre-push gate runs is reachable from a pull request (#232).

THE GAP THIS CLOSES

`scripts/ci/local_ci_gate.sh` runs on the machine of whoever pushes. An outside
contributor never touches it: they open a pull request, and whatever runs there
is the whole of what stands between their change and master.

So a guard that lives only in the pre-push gate exists and does not run at the
moment it counts -- with a stranger watching, which is when it is needed. That is
the Definition-of-Done pattern #232 names, and it was not hypothetical: when this
guard was written, 13 of the 90 gates were reachable from no pull-request
workflow at all.

WHAT IT CHECKS

Each `run <name> <command>` line in the pre-push gate names a script. That script
must appear in a workflow whose triggers include `pull_request`, or carry a row
in docs/GATES_REACHABLE_FROM_A_PULL_REQUEST.toml saying what it needs that a
pull-request runner does not have.

An exemption must give a reason, and the reason is prose a person reads. This
guard cannot judge whether a reason is good; it can make sure one was written,
which is the difference between a decision and a drift.

WHAT IT DOES NOT CHECK

Whether the workflow's job actually passes, or whether the trigger has a
`paths-ignore` that would skip it for some changes. Those are real questions and
this is not the file that answers them: `a_gate_that_never_went_green.py` looks
at outcomes, and this one at reachability.
"""
from __future__ import annotations
import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
GATE = REPO / "scripts" / "ci" / "local_ci_gate.sh"
WORKFLOWS = REPO / ".github" / "workflows"
EXEMPTIONS = REPO / "docs" / "GATES_REACHABLE_FROM_A_PULL_REQUEST.toml"

SCRIPT = re.compile(r"scripts/ci/([a-z_0-9]+\.(?:py|sh))")
RUN_LINE = re.compile(r"^run\s+(\S+)\s+(.*)$", re.M)

# A floor on what was examined. A gate file that yields nothing means the parse
# broke, and reporting OK over zero gates is the failure this whole family of
# guards exists to refuse.
MIN_GATES = 40


def exemptions() -> dict[str, str]:
    if not EXEMPTIONS.is_file():
        raise SystemExit(
            "every_gate_is_reachable_from_a_pull_request: SKIPPED (not a pass) -- "
            f"{EXEMPTIONS.relative_to(REPO)} is missing, so nothing was checked."
        )
    rows = tomllib.loads(EXEMPTIONS.read_text(encoding="utf-8")).get("exempt", [])
    return {r["gate"]: r.get("why", "").strip() for r in rows if r.get("gate")}


def on_pull_request() -> set[str]:
    """Scripts named by a workflow that a pull request triggers."""
    found: set[str] = set()
    for f in sorted(WORKFLOWS.glob("*.yml")):
        text = f.read_text(errors="replace")
        head = text.split("jobs:", 1)[0]
        if re.search(r"^\s*pull_request:", head, re.M):
            found.update(SCRIPT.findall(text))
    return found


def main() -> int:
    if not GATE.is_file():
        print("every_gate_is_reachable_from_a_pull_request: SKIPPED (not a pass) -- "
              f"{GATE.relative_to(REPO)} is missing.", file=sys.stderr)
        return 1

    excused = exemptions()
    reachable = on_pull_request()

    gates: dict[str, str] = {}
    for name, command in RUN_LINE.findall(GATE.read_text(encoding="utf-8")):
        for script in SCRIPT.findall(command):
            gates[script] = name

    if len(gates) < MIN_GATES:
        print(f"[pr-reachable] SKIPPED (not a pass): only {len(gates)} gate(s) "
              "parsed out of the pre-push gate, which cannot be right.",
              file=sys.stderr)
        return 1

    problems: list[str] = []
    for script, gate_name in sorted(gates.items()):
        if script in reachable:
            continue
        if script not in excused:
            problems.append(f"`{gate_name}` runs {script}, which no pull-request "
                            "workflow reaches and no row excuses")
        elif not excused[script]:
            problems.append(f"{script} is excused with an empty reason")

    # An exemption for a gate that is no longer run, or that is reachable after
    # all, is a register outliving its subject.
    for script in sorted(excused):
        if script not in gates:
            problems.append(f"{script} is excused but the pre-push gate does not "
                            "run it any more")
        elif script in reachable:
            problems.append(f"{script} is excused but a pull-request workflow does "
                            "reach it; drop the row")

    if problems:
        print(f"[pr-reachable] {len(problems)} problem(s):\n", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("\n  Either give the gate a job on a pull-request workflow, or add a\n"
              "  row to docs/GATES_REACHABLE_FROM_A_PULL_REQUEST.toml saying what\n"
              "  it needs that a pull-request runner does not have.",
              file=sys.stderr)
        return 1

    print(f"[pr-reachable] OK: {len(gates)} gate(s) in the pre-push gate, "
          f"{len(gates) - len(excused)} reachable from a pull request, "
          f"{len(excused)} excused with a reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
