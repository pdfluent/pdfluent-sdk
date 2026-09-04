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

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "every_label_has_a_runner.py"
sys.path.insert(0, str(HERE))
import every_label_has_a_runner as guard  # noqa: E402

failures: list[str] = []
ran = 0


def case(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        failures.append(what)


def job(**kw):
    return dict(kw)


def main() -> int:
    # --- a step that refuses -------------------------------------------------
    case("`exit 1` as the last line refuses",
          guard._weigert({"run": "echo hi\nexit 1"}))
    case("`exit 0` does not refuse",
          not guard._weigert({"run": "exit 0"}))
    case("a trailing comment does not hide the exit",
          guard._weigert({"run": "exit 1\n# why this job is parked"}))
    case("a step with an `if:` never refuses -- a false condition is a no-op",
          not guard._weigert({"if": "false", "run": "exit 1"}))
    case("a step with no `run:` does not refuse",
          not guard._weigert({"uses": "actions/checkout@v4"}))

    # --- the graph -----------------------------------------------------------
    doc = {"jobs": {
        "blocker": job(steps=[{"run": "exit 1"}]),
        "soft": job(steps=[{"run": "exit 1"}], **{"continue-on-error": True}),
        "switch": job(steps=[{"run": "exit 1"}], **{"if": "github.event_name == 'push'"}),
        "plain": job(steps=[{"run": "true"}]),
    }}
    case("a job behind a refuser is parked",
          guard.geparkeerd(doc, job(needs="blocker")))
    case("a job behind a continue-on-error blocker is NOT parked",
          not guard.geparkeerd(doc, job(needs="soft")),
          "continue-on-error lets the dependants run")
    case("a job behind an `if:` blocker is NOT parked",
          not guard.geparkeerd(doc, job(needs="switch")),
          "an if: is a switch somebody can flip without touching this file")
    case("a job behind an ordinary job is not parked",
          not guard.geparkeerd(doc, job(needs="plain")))
    case("a job with no needs is not parked", not guard.geparkeerd(doc, job()))
    case("needs as a list is followed too",
          guard.geparkeerd(doc, job(needs=["plain", "blocker"])))
    case("an unknown blocker name is not parked",
          not guard.geparkeerd(doc, job(needs="bestaat-niet")))

    # --- the guard on the real tree ------------------------------------------
    clean = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True)
    case("the repository as it stands passes", clean.returncode == 0,
          (clean.stdout + clean.stderr)[-300:])
    case("and it says the graph derives the same set as the baseline",
          "needs-graph derives the same" in clean.stdout,
          clean.stdout[-200:])

    # --- THE MUTATION --------------------------------------------------------
    # Break `parked` and the cross-check must notice. Without this the agreement
    # above could hold because nothing is being compared.
    source = GUARD.read_text()
    broken = source.replace("def geparkeerd(doc, job) -> bool:\n",
                         "def geparkeerd(doc, job) -> bool:\n    return False\n", 1)
    case("the mutation could be applied", broken != source)
    tmp_path = HERE / "_every_label_mutant.py"
    try:
        tmp_path.write_text(broken)
        red = subprocess.run([sys.executable, str(tmp_path)], capture_output=True, text=True)
        case("with `parked` always False the guard goes red", red.returncode == 1,
              f"exit={red.returncode} {(red.stdout + red.stderr)[-200:]}")
        case("and it names the disagreement rather than a count",
              "is excused by BEKEND but nothing ahead of it refuses" in red.stderr,
              red.stderr[-300:])
    finally:
        tmp_path.unlink(missing_ok=True)

    print(f"\n  {ran} assertion(s) ran, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
