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

import yaml

REPO = pathlib.Path(__file__).resolve().parents[2]
GATE = REPO / "scripts" / "ci" / "local_ci_gate.sh"
WORKFLOWS = REPO / ".github" / "workflows"
EXEMPTIONS = REPO / "docs" / "GATES_REACHABLE_FROM_A_PULL_REQUEST.toml"

SCRIPT = re.compile(r"scripts/ci/([a-z_0-9]+\.(?:py|sh))")
# `zwaar` is the heavy lane's wrapper around `run`: the gate is deferred on an
# ordinary push and runs on a push to master. Deferred is not dropped, so a
# `zwaar` gate is as much a gate as a `run` one -- and reading only `run` lines
# made the compiling gates invisible here, which turned their two exemption rows
# into "excused but nobody runs it" the day the lanes were introduced.
#
# `scoped` and `crate_gate` are the same argument one lane further (#343): they
# run on a push to master over the crates that landing touches. A gate that runs
# for some landings and not others is still a gate the pre-push hook can run, and
# this file asks whether a stranger's pull request would reach it -- a question
# whose answer does not depend on which crates today's landing changed.
#
# `crate_gate` takes the crate before the command, so the second word is skipped
# for that spelling and for that spelling only.
RUN_LINE = re.compile(
    r"^(?:run|zwaar|scoped)\s+(\S+)\s+(.*)$|^crate_gate\s+(\S+)\s+\S+\s+(.*)$", re.M)

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


# A job condition that is FALSE in this repository for as long as it is private.
# `.github/workflows/public-pull-request.yml` carries it on every one of its
# jobs: those two checks exist for the public repository this tree seeds, they
# are what a stranger's pull request there runs, and here they are skipped so no
# hosted minute is billed (#333, #233).
ALLEEN_PUBLIEK = "github.event.repository.private == false"


def _draait_hier(pad: pathlib.Path) -> bool:
    """True when at least one job of this workflow can actually run here.

    A workflow every job of which is gated on the repository being public
    declares no boundary in a private repository -- it declares one somewhere
    else. Without this, adding the first such workflow would flip the boundary
    below to "a pull request" and report 115 gates as unreachable, on the
    strength of two jobs that are skipped on every run.

    A file that does not parse is left out rather than counted either way;
    `every_workflow_can_start.py` owns that failure and says so in its own
    words.
    """
    try:
        doc = yaml.safe_load(pad.read_text(errors="replace")) or {}
    except yaml.YAMLError:
        return False
    jobs = (doc.get("jobs") or {}) if isinstance(doc, dict) else {}
    return any(ALLEEN_PUBLIEK not in str(job.get("if", ""))
               for job in jobs.values() if isinstance(job, dict))


def _start_bij(pad: pathlib.Path, trigger: str) -> bool:
    text = pad.read_text(errors="replace")
    head = text.split("jobs:", 1)[0]
    return bool(re.search(rf"^\s*{trigger}:", head, re.M))


def _genoemd_door(trigger: str) -> set[str]:
    """Scripts named by a workflow this trigger starts, here."""
    found: set[str] = set()
    for f in sorted(WORKFLOWS.glob("*.yml")):
        if _start_bij(f, trigger) and _draait_hier(f):
            found.update(SCRIPT.findall(f.read_text(errors="replace")))
    return found


def de_grens() -> tuple[str, set[str]]:
    """The boundary that exists today, and the scripts behind it.

    THE QUESTION THIS FILE ASKS IS NOT "IS THERE A PULL REQUEST".

    It is: when a change crosses into master, does every gate the pre-push gate
    ran also run somewhere the person pushing does not control? The pull request
    was that place, and while an outside contributor can open one it is the only
    place -- which is the whole of #232 and does not change.

    On 05-09-2026 #333 removed every `pull_request` trigger while this repository
    is private: the included Actions minutes were spent, the account had begun
    billing, and each guard was running twice -- once for the pull request and
    once for the push behind it -- over work `scripts/ci/local_ci_gate.sh` had
    already refused to let out. With one contributor and a master that only ever
    takes fast-forwards of heads that passed that gate, the pull-request run was
    a second opinion from the same machine.

    So the boundary is now the push to master: ci.yml and its siblings run on the
    merge, on the desktop runner, out of the code that landed. It is a weaker
    boundary than a pull request and it is the one that exists; saying which was
    measured is the difference between a check and a word.

    THE PUBLIC PHASE: the moment any workflow carries a `pull_request` trigger
    again, that is the boundary and this returns to it without an edit -- because
    it is chosen by looking, not by a flag somebody has to remember to flip.

    "Carries a trigger" is not enough on its own, and #233 is where that showed.
    `public-pull-request.yml` is triggered by a pull request and produces the
    checks the PUBLIC repository requires, and every one of its jobs is gated on
    the repository being public -- so here it starts and does nothing. Counting
    it would have moved the boundary on the strength of jobs that never run, and
    reported 115 gates as unreachable the day it was added. So the question is
    whether a job can run here, not whether a trigger is written down.
    """
    op_pr = _genoemd_door("pull_request")
    heeft_pr = any(_start_bij(f, "pull_request") and _draait_hier(f)
                   for f in sorted(WORKFLOWS.glob("*.yml")))
    if heeft_pr:
        return "a pull request", op_pr
    return "the push to master", _genoemd_door("push")


def main() -> int:
    if not GATE.is_file():
        print("every_gate_is_reachable_from_a_pull_request: SKIPPED (not a pass) -- "
              f"{GATE.relative_to(REPO)} is missing.", file=sys.stderr)
        return 1

    excused = exemptions()
    grens, reachable = de_grens()

    gates: dict[str, str] = {}
    for run_name, run_cmd, crate_name, crate_cmd in RUN_LINE.findall(
            GATE.read_text(encoding="utf-8")):
        name, command = (run_name, run_cmd) if run_name else (crate_name, crate_cmd)
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
            problems.append(f"`{gate_name}` runs {script}, which no workflow on "
                            f"{grens} reaches and no row excuses")
        elif not excused[script]:
            problems.append(f"{script} is excused with an empty reason")

    # An exemption for a gate that is no longer run, or that is reachable after
    # all, is a register outliving its subject.
    for script in sorted(excused):
        if script not in gates:
            problems.append(f"{script} is excused but the pre-push gate does not "
                            "run it any more")
        elif script in reachable:
            problems.append(f"{script} is excused but a workflow on {grens} does "
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
          f"{len(gates) - len(excused)} reachable from {grens}, "
          f"{len(excused)} excused with a reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
