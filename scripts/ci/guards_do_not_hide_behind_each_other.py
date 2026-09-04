#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A guard job must run all of its guards, not stop at the first one that fails.

`orchestration-guard` had 37 steps in sequence. Step four failed, and had failed
on all thirty of the preceding runs on master -- twenty-one failures, nine
cancellations, no successes. The thirty-three steps below it never executed:
the licence gate, the fork-drift gates, the capability matrix, the
shell-injection guard, PR staleness, and about twenty more. Every one of them
read as an installed control while measuring nothing, and one of them was
discovered only because an agent added a guard, watched it go green, and then
found it had never run.

A job that stops at the first failure reports one problem and hides the rest, so
each fix reveals the next one a day later. Worse, a step that never ran looks
the same in the summary as a step that passed.

The remedy is one line per step: `if: ${{ !cancelled() }}`. Every guard still
runs, every failure is still a failure, and the job still goes red -- you simply
see all of them at once.

This check keeps that true for steps added later, which is the part a convention
cannot do on its own.

There is a second way to un-arm a guard job, and it is one line as well:
`continue-on-error: true` leaves the step running, the log red and the job
green. A gate that cannot fail is the shape #286 found in front of the merge --
a PDF/A check validating its own input fixtures, unable to go red, standing
where the real gate was not. So this file refuses that too, on the step and on
the job.

Exit codes:
  0  every guard step runs regardless of its predecessors, and can still fail
  1  a guard step would be skipped when an earlier one fails, or cannot fail
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("SKIPPED (not a pass): pyyaml is not installed, so the workflows cannot be read", file=sys.stderr)
    sys.exit(3)

ROOT = Path(__file__).resolve().parents[2]

# Jobs whose purpose is to run a series of independent checks. A failure in one
# says nothing about whether the next one would pass, so the next one must run.
GUARD_JOBS = {
    ("ci.yml", "orchestration-guard"),
    # #288 ported thirteen guards off the GitLab mirror into `promise-guard`
    # and `measurement-guard`. #328 merged those two, and four more light jobs,
    # into a single `guards` job: eight checkouts and eight minimum-minute
    # roundings for work that is seconds of file scanning. The shape did not
    # change -- a series of independent checks, where one failure would hide the
    # rest and a step that never ran looks exactly like a step that passed -- so
    # the requirement follows the steps into their new job, and now covers the
    # four that were never named here.
    ("ci.yml", "guards"),
}

# Steps that genuinely are prerequisites: if the checkout or the interpreter is
# missing, every step below is meaningless rather than unmeasured. Aborting
# there is correct, so they are exempt.
#
# Split by WHERE the evidence lives, because the two are different questions.
# An action is identified by its `uses:` id; a command by a line it actually
# runs. Neither is identified by what a step is CALLED.
PREREQUISITE_ACTIONS = (
    "actions/checkout",
    "actions/setup-python",
    "actions/setup-node",
)
PREREQUISITE_COMMANDS = (
    "pip install",
    "npm ci",
)

RUNS_ANYWAY = ("!cancelled()", "always()", "success() || failure()")


def zonder_commentaar(regel: str) -> str:
    """A shell line with its trailing comment removed, quotes respected.

    `#` only starts a comment at the start of a word, so `sha256#deadbeef` and
    `echo "# not a comment"` survive. Cutting on every `#` would silently
    shorten real commands, which is the same class of error as reading them
    from the wrong field.
    """
    uit: list[str] = []
    quote: str | None = None
    vorige_was_spatie = True
    ontsnapt = False
    for teken in regel:
        if ontsnapt:
            # The character after a backslash is literal: `\"` does not close
            # a double-quoted string and `\#` does not start a comment.
            uit.append(teken)
            ontsnapt = False
        elif teken == "\\" and quote != "'":
            uit.append(teken)
            ontsnapt = True
        elif quote:
            uit.append(teken)
            if teken == quote:
                quote = None
        elif teken in ("'", '"'):
            quote = teken
            uit.append(teken)
        elif teken == "#" and vorige_was_spatie:
            break
        else:
            uit.append(teken)
        vorige_was_spatie = teken.isspace()
    return "".join(uit)


def is_prerequisite(step: dict) -> bool:
    """Whether this step is a genuine prerequisite, judged by what it DOES.

    It used to concatenate `uses`, `run` AND `name` and look for substrings, so
    a step CALLED "check that actions/checkout is pinned" exempted itself from
    the rule this file exists to enforce, and so did a `# pip install ...`
    comment inside an unrelated command. A guard that reads a label as if it
    were behaviour cannot tell a check from the thing it checks -- the same
    defect `ci_dekking` had before #1685, in the file that polices exactly this.
    (#321)

    So: an action is matched on its `uses:` id with the version stripped, a
    command on a line it really runs, and `name:` is never consulted.
    """
    uses = (step.get("uses") or "").split("@", 1)[0].strip()
    if uses and uses in PREREQUISITE_ACTIONS:
        return True

    for regel in (step.get("run") or "").splitlines():
        code = zonder_commentaar(regel).strip()
        if code and any(marker in code for marker in PREREQUISITE_COMMANDS):
            return True
    return False


def main() -> int:
    problems: list[str] = []
    checked = 0

    for filename, job_name in sorted(GUARD_JOBS):
        path = ROOT / ".github" / "workflows" / filename
        if not path.exists():
            print(f"SKIPPED (not a pass): {path} is missing", file=sys.stderr)
            return 3
        workflow = yaml.safe_load(path.read_text())
        job = (workflow.get("jobs") or {}).get(job_name)
        if job is None:
            problems.append(f"{filename}: job `{job_name}` no longer exists. Update GUARD_JOBS or restore it.")
            continue

        if str(job.get("continue-on-error", "")).lower() == "true":
            problems.append(
                f"{filename}:{job_name} is continue-on-error, so every guard in it "
                "reports green whatever it finds. Remove it."
            )

        steps = job.get("steps") or []
        seen_a_guard = False
        for index, step in enumerate(steps, start=1):
            if is_prerequisite(step):
                if seen_a_guard:
                    problems.append(
                        f"{filename}:{job_name} step {index} ({step.get('name') or step.get('uses')}) "
                        "looks like setup but sits below a guard, so a guard failure would skip it."
                    )
                continue
            seen_a_guard = True
            checked += 1
            if str(step.get("continue-on-error", "")).lower() == "true":
                label = step.get("name") or str(step.get("run", ""))[:60]
                problems.append(
                    f"{filename}:{job_name} step {index} ({label}) is continue-on-error. "
                    "It runs, it goes red in the log and the job stays green -- which is "
                    "the soft gate this repository has already been caught by."
                )

            condition = str(step.get("if", ""))
            if not any(marker in condition for marker in RUNS_ANYWAY):
                label = step.get("name") or str(step.get("run", ""))[:60]
                problems.append(
                    f"{filename}:{job_name} step {index} ({label}) has no `if:` that survives an "
                    "earlier failure. Add `if: ${{ !cancelled() }}` so it still runs and still fails."
                )

    if checked == 0 and not problems:
        print(
            "SKIPPED (not a pass): no guard steps were examined. Either GUARD_JOBS is empty\n"
            "  or every step matched a prerequisite marker -- both mean this check measured nothing.",
            file=sys.stderr,
        )
        return 3

    if problems:
        print(f"Guard steps that hide behind an earlier failure: {len(problems)}\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    print(
        f"✓ {checked} guard step(s) across {len(GUARD_JOBS)} job(s) run regardless of "
        "their predecessors, and none of them is allowed to fail softly"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
