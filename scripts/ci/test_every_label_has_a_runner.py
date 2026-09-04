#!/usr/bin/env python3
"""Parked-ness is derived from the needs-graph, not read off a list.

`715e4121` answered "is this job queued behind a job that cannot pass?" by
walking the graph. The #1543 reconciliation dropped it, and the distinction
between "parked on purpose" and "blocked because something ahead of it broke"
came to rest on a list somebody maintains (#319).

Measured on 04-09-2026 before restoring it: the four entries in `BEKEND` are
exactly the four the graph derives. The list was right. Nothing was keeping it
right, and that is the whole of the defect.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

HIER = pathlib.Path(__file__).resolve().parent
WACHT = HIER / "every_label_has_a_runner.py"
sys.path.insert(0, str(HIER))
import every_label_has_a_runner as guard  # noqa: E402

fouten: list[str] = []
gedraaid = 0


def geval(wat: str, ok: bool, detail: str = "") -> None:
    global gedraaid
    gedraaid += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fouten.append(wat)


def job(**kw):
    return dict(kw)


def main() -> int:
    # --- a step that refuses -------------------------------------------------
    geval("`exit 1` as the last line refuses",
          guard._refuses({"run": "echo hi\nexit 1"}))
    geval("`exit 0` does not refuse",
          not guard._refuses({"run": "exit 0"}))
    geval("a trailing comment does not hide the exit",
          guard._refuses({"run": "exit 1\n# why this job is parked"}))
    geval("a step with an `if:` never refuses -- a false condition is a no-op",
          not guard._refuses({"if": "false", "run": "exit 1"}))
    geval("a step with no `run:` does not refuse",
          not guard._refuses({"uses": "actions/checkout@v4"}))

    # --- the graph -----------------------------------------------------------
    doc = {"jobs": {
        "blokkeur": job(steps=[{"run": "exit 1"}]),
        "zacht": job(steps=[{"run": "exit 1"}], **{"continue-on-error": True}),
        "schakelaar": job(steps=[{"run": "exit 1"}], **{"if": "github.event_name == 'push'"}),
        "gewoon": job(steps=[{"run": "true"}]),
    }}
    geval("a job behind a refuser is parked",
          guard.parked(doc, job(needs="blokkeur")))
    geval("a job behind a continue-on-error blocker is NOT parked",
          not guard.parked(doc, job(needs="zacht")),
          "continue-on-error lets the dependants run")
    geval("a job behind an `if:` blocker is NOT parked",
          not guard.parked(doc, job(needs="schakelaar")),
          "an if: is a switch somebody can flip without touching this file")
    geval("a job behind an ordinary job is not parked",
          not guard.parked(doc, job(needs="gewoon")))
    geval("a job with no needs is not parked", not guard.parked(doc, job()))
    geval("needs as a list is followed too",
          guard.parked(doc, job(needs=["gewoon", "blokkeur"])))
    geval("an unknown blocker name is not parked",
          not guard.parked(doc, job(needs="bestaat-niet")))

    # --- the guard on the real tree ------------------------------------------
    schoon = subprocess.run([sys.executable, str(WACHT)], capture_output=True, text=True)
    geval("the repository as it stands passes", schoon.returncode == 0,
          (schoon.stdout + schoon.stderr)[-300:])
    geval("and it says the graph derives the same set as the baseline",
          "needs-graph derives the same" in schoon.stdout,
          schoon.stdout[-200:])

    # --- THE MUTATION --------------------------------------------------------
    # Break `parked` and the cross-check must notice. Without this the agreement
    # above could hold because nothing is being compared.
    bron = WACHT.read_text()
    kapot = bron.replace("def parked(doc, job) -> bool:\n",
                         "def parked(doc, job) -> bool:\n    return False\n", 1)
    geval("the mutation could be applied", kapot != bron)
    tijdelijk = HIER / "_every_label_mutant.py"
    try:
        tijdelijk.write_text(kapot)
        rood = subprocess.run([sys.executable, str(tijdelijk)], capture_output=True, text=True)
        geval("with `parked` always False the guard goes red", rood.returncode == 1,
              f"exit={rood.returncode} {(rood.stdout + rood.stderr)[-200:]}")
        geval("and it names the disagreement rather than a count",
              "is excused by BEKEND but nothing ahead of it refuses" in rood.stderr,
              rood.stderr[-300:])
    finally:
        tijdelijk.unlink(missing_ok=True)

    print(f"\n  {gedraaid} assertion(s) ran, {len(fouten)} failure(s)")
    return 1 if fouten else 0


if __name__ == "__main__":
    sys.exit(main())
