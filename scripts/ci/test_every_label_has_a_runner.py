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

    # --- the register carries reasons, and the reasons are measured ----------
    #
    # #276 asked for a decision, not a holding pattern: either the four corpus
    # gates get a runner, or they are manual and say why. They are manual. What
    # keeps that from ageing into a line nobody dares delete is that the reason
    # is a claim about a file -- "that manifest records no origin" -- and this
    # is where the claim is checked in both directions.
    case("every row in the register carries a reason",
         all(str(v or "").strip() for v in guard.REGISTER.values()),
         f"{sorted(k for k, v in guard.REGISTER.items() if not str(v or '').strip())}")
    case("and the reason names the condition that removes the row",
         all("Un-park" in v for v in guard.REGISTER.values()),
         "a reason without an exit is a permanent excuse")

    machinepaden = HERE.parent.parent / "corpus" / "CI_CORPUS_MANIFEST.json"
    geldt, hoezo = guard.de_corpusreden_geldt_nog(machinepaden)
    case("the tree still says what the register says it says", geldt is True, hoezo)

    import json as _json
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        machine = tmp / "machine.json"
        # A stand-in prefix rather than the real one. What is being exercised is
        # "the source is an absolute path on some machine", and the published
        # tree is audited for the actual mount name -- a fixture is not a reason
        # to put a pointer into somebody's filesystem in a file that ships.
        machine.write_text(_json.dumps({"entries": [
            {"file": "0052.pdf", "source": "/srv/corpus/curated-1k/0052.pdf"}]}))
        case("a manifest of machine paths keeps the exemption",
             guard.de_corpusreden_geldt_nog(machine)[0] is True)

        # THE POINT OF THE WHOLE ROW. The day somebody records where those 500
        # documents come from -- a suite and a commit, the way
        # GATE_CORPUS_MANIFEST.json does -- these four can run again, and this
        # is what says so instead of waiting for a reader to notice.
        herkomst = tmp / "herkomst.json"
        herkomst.write_text(_json.dumps({"entries": [
            {"file": "0052.pdf", "source": "https://example.invalid/0052.pdf"}]}))
        expired, waarom = guard.de_corpusreden_geldt_nog(herkomst)
        case("a manifest that records an origin expires the exemption",
             expired is False, waarom)

        # "I could not check" is not "still true", and reporting it as one is
        # how a register survives its own subject.
        case("an unreadable manifest is neither -- it refuses",
             guard.de_corpusreden_geldt_nog(tmp / "weg.json")[0] is None)
        stuk = tmp / "stuk.json"
        stuk.write_text("{ not json")
        case("and so does one that will not parse",
             guard.de_corpusreden_geldt_nog(stuk)[0] is None)
        leeg = tmp / "leeg.json"
        leeg.write_text(_json.dumps({"entries": []}))
        case("and so does one with no entries at all",
             guard.de_corpusreden_geldt_nog(leeg)[0] is None)

    # --- the guard on the real tree ------------------------------------------
    # THE GUARD CANNOT RUN EVERYWHERE, AND THIS TEST HAS TO SAY SO.
    #
    # `every_label_has_a_runner` reads the runner list through
    # `gh api /actions/runners`, which needs `administration: read` -- a scope
    # GITHUB_TOKEN cannot be granted. Its own docstring says so. Without that
    # list it prints `SKIPPED (not a pass)` and returns 0.
    #
    # The first version of this test asserted the comparison regardless. Wired
    # into a GitHub Actions workflow it failed three cases, turned `A deletion
    # declares itself` red on every pull request, and -- because
    # `a_gate_that_never_went_green.py` refuses a push while a changed workflow
    # has never been green -- closed master for all three terminals until #1719
    # reopened it. The test was right about the code and wrong about where it
    # could run, and the second mistake was the expensive one.
    #
    # So: one failure naming the cause and the remedy, not three describing
    # symptoms. A failure and not a silent pass, because this test is wired in
    # local_ci_gate.sh where `gh` is authenticated -- a skip there means the
    # environment is broken, and calling that green is the shape this
    # repository keeps finding.
    clean = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True)
    could_not_read = "could not read the runner list" in clean.stderr
    case("the guard could read the runner list", not could_not_read,
         "`gh api /actions/runners` answered nothing. This test belongs in "
         "local_ci_gate.sh, where gh is authenticated; it cannot run in GitHub "
         "Actions, because that endpoint needs `administration: read`.")
    if not could_not_read:
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
        # Same dependency as above: with no runner list the mutant skips before
        # it can disagree with anything, so asserting here would fail for the
        # environment rather than for the mutation.
        if not could_not_read:
            case("with `geparkeerd` always False the guard goes red", red.returncode == 1,
                 f"exit={red.returncode} {(red.stdout + red.stderr)[-200:]}")
            case("and it names the disagreement rather than a count",
                 "is excused by BEKEND but nothing ahead of it refuses" in red.stderr,
                 red.stderr[-300:])
    finally:
        tmp_path.unlink(missing_ok=True)

    # --- AND THE SECOND MUTATION ---------------------------------------------
    # The unit cases above prove the measurement. This proves the guard asks it.
    # A function that returns the right answer into nothing is the shape this
    # repository keeps finding, and it is indistinguishable from a working check
    # until the day the answer changes.
    #
    # Deliberately not behind `could_not_read`: the register is checked before
    # the runner list is fetched, precisely so this half still works in GitHub
    # Actions, where /actions/runners cannot be read at all.
    verlopen = source.replace(
        '    pad = pad or CORPUS_MANIFEST\n',
        '    return False, "an origin is recorded"\n', 1)
    case("the expiry mutation could be applied", verlopen != source)
    mutant = HERE / "_every_label_expired.py"
    try:
        mutant.write_text(verlopen)
        rood = subprocess.run([sys.executable, str(mutant)], capture_output=True, text=True)
        case("with the corpus reachable the guard refuses to keep the four parked",
             rood.returncode == 1, f"exit={rood.returncode} {(rood.stdout + rood.stderr)[-200:]}")
        case("and it names each workflow rather than the count",
             rood.stderr.count("Un-park it, or rewrite its reason") == len(guard.REGISTER),
             rood.stderr[-400:])
    finally:
        mutant.unlink(missing_ok=True)

    print(f"\n  {ran} assertion(s) ran, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
