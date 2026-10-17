#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does the budget guard refuse, and does it refuse for the right reason? (#425)

A budget guard's whole failure mode is being green for the wrong reason: no
table, a table full of runs from a machine at load 100, a landing-lane run read
as a fast one. Each of those looks identical to "the lane is fine" unless the
guard is made to say which it is, so every case below asserts on the WORD as
well as the exit code -- SKIPPED and PASS are different answers and this file
exists to keep them different.

Since 09-09-2026 the guard REPORTS instead of judging: #474 moved the lane off
the shared Mac the 300 s was derived on, and #494 owes a measured number. Two
contracts now, and both are here -- that it reports while PREMISSE_HERZIEN is
True, and that the judging it will go back to still works, driven with the flag
patched off. A guard that was only tested in its reporting state would be a
guard nobody could turn back on with confidence.

Since 23-09-2026 the fast lane refuses only the push that puts it over: the
guard runs in a small repository of its own here, whose history adds the gates
the rows name, so every refusal below is one this push's diff causes.

FLOOR: cases run >= 19.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile

CI = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(CI))
from fixture_env import sealed_env  # noqa: E402
REAL_GUARD = CI / "the_fast_lane_stays_under_its_budget.py"
# Set by harness(): the guard copied into a repository whose history is known.
GUARD = REAL_GUARD
BASES: dict[str, str] = {}
FLOOR = 19

failures = 0
cases = 0


def fail(msg: str) -> None:
    global failures
    print(f"  FAIL: {msg}", file=sys.stderr)
    failures += 1


# The guard with the revision flag patched off, which is the state #494 puts it
# back into. Deliberately not an environment variable in the guard itself: that
# would be the budget raise the whole file argues against, one indirection
# further away. A test may reach into a module; a caller may not.
def oordeelt() -> str:
    return ("import importlib.util,sys;"
            f"spec=importlib.util.spec_from_file_location('g',{str(GUARD)!r});"
            "m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);"
            "m.PREMISSE_HERZIEN=False;sys.exit(m.main())")


def harness(work: pathlib.Path) -> None:
    """A repository of four commits, each a kind of push, with the guard in it.

      base    one cheap gate
      slow    adds seedtest and snapaudit, the two gates line() names by default
      frames  adds a gate the rows never list (T2's 0.1 s self-test)
      script  changes only the file seedtest runs
      helped  adds a gate whose script imports a sibling helper
      helper  changes only that helper

    LANDING_BASE picks which of those the push is; the tree stays at `helper`.
    """
    global GUARD
    repo = work / "repo"
    ci = repo / "scripts" / "ci"
    ci.mkdir(parents=True)
    (ci / REAL_GUARD.name).write_text(REAL_GUARD.read_text())
    env = sealed_env(identity=True, cwd=repo)

    def git(*a: str) -> str:
        return subprocess.run(["git", *a], cwd=repo,
                              env=env, check=True, capture_output=True, text=True).stdout.strip()

    gates = ["run cheap python3 scripts/ci/cheap.py\n"]

    def commit(name: str) -> None:
        (ci / "local_ci_gate.sh").write_text("".join(gates))
        git("add", "-A")
        git("commit", "-q", "-m", name)
        BASES[name] = git("rev-parse", "HEAD")

    git("init", "-q", ".")
    (ci / "seed.py").write_text("pass\n")
    commit("base")
    gates += ["run seedtest python3 scripts/ci/seed.py\n",
              "run snapaudit python3 \\\n  scripts/ci/snap.py\n"]
    commit("slow")
    gates += ["run frames python3 scripts/ci/frames.py\n"]
    commit("frames")
    (ci / "seed.py").write_text("pass  # changed\n")
    commit("script")
    (ci / "shared.py").write_text("X = 1\n")
    (ci / "helped.py").write_text("import shared\n")
    gates += ["run helped python3 scripts/ci/helped.py\n"]
    commit("helped")
    (ci / "shared.py").write_text("X = 2  # slower now\n")
    commit("helper")
    GUARD = ci / REAL_GUARD.name


def run_guard(table: pathlib.Path, budget: str = "300", idle: str = "2.0", judging: bool = False,
              lane: str = "fast", base: str = "base"):
    global cases
    cases += 1
    env = sealed_env(cwd=GUARD.parents[2])
    env["XFA_GATE_TIMINGS"] = str(table)
    env["XFA_FAST_LANE_BUDGET"] = budget
    env["XFA_FAST_LANE_IDLE_LOAD"] = idle
    env["LANDING_BASE"] = BASES.get(base, base)
    argv = ([sys.executable, "-c", oordeelt(), "--lane", lane] if judging
            else [sys.executable, str(GUARD), "--lane", lane])
    p = subprocess.run(argv, capture_output=True, text=True, env=env)
    return p.returncode, p.stdout + p.stderr


def lane_id() -> str:
    """This gate list's fingerprint, the way the gate writes it into the table."""
    import hashlib
    gate = GUARD.parent / "local_ci_gate.sh"
    return hashlib.sha256(gate.read_bytes()).hexdigest()[:12]


def line(lane: str, guard_s: int, wall_s: int, load: float,
         slowest: str = "seedtest=857;snapaudit=182",
         fingerprint: str | None = None, host: str | None = None) -> str:
    """A row of the table, of THIS gate list unless another is named (#454).

    Every row carries a fingerprint, because since #454 a row that names no
    gate list is not judged: on a pooled instance those are rows from before
    the column existed, outliving the tree that wrote them, and a time is a
    fact about a gate list. The cases that are about the budget itself must
    therefore claim this lane, or they would all measure the skip instead.
    """
    id_ = lane_id() if fingerprint is None else fingerprint
    import socket
    host_ = socket.gethostname() if host is None else host
    return (f"2026-09-07T12:00:00Z\t{lane}\t{guard_s}\t{wall_s}\t{load}\t{slowest}"
            + (f"\t{id_}" if id_ else "") + (f"\t{host_}" if id_ and host_ else "") + "\n")


def main() -> int:
    if not REAL_GUARD.exists():
        print(f"SKIPPED (not a pass): {REAL_GUARD} is gone; nothing to test.", file=sys.stderr)
        return 0

    with tempfile.TemporaryDirectory() as d:
        work = pathlib.Path(d)
        harness(work)

        # 1. NO TABLE IS NOT A PASS. A checkout that has never run the gate has
        # nothing to judge, and the one thing it must not do is say the lane is fine.
        rc, out = run_guard(work / "absent.tsv")
        if rc != 0:
            fail(f"1: exit {rc} for a missing table; a guard that cannot look must not refuse either")
        if "SKIPPED" not in out:
            fail("1: a missing table did not say SKIPPED, so it reads as a pass")

        # 2. A BUSY MACHINE IS NOT A MEASUREMENT. The same lane took 1152s and
        # 6342s on 07-09 depending on what else was running; judging those would
        # make the guard red all afternoon and believed on neither reading.
        busy = work / "busy.tsv"
        busy.write_text(line("fast", 4000, 4200, 98.5) + line("fast", 3800, 3900, 42.0))
        rc, out = run_guard(busy)
        if rc != 0:
            fail(f"2: exit {rc} on runs from a loaded machine; it judged what it cannot judge")
        if "SKIPPED" not in out or "idle" not in out:
            fail("2: a busy-machine table did not say why it skipped")

        # 3. OVER BUDGET ON AN IDLE MACHINE REFUSES. The case the ticket is about.
        over = work / "over.tsv"
        over.write_text(line("fast", 1141, 1380, 0.8))
        rc, out = run_guard(over, judging=True)
        if rc != 1:
            fail(f"3: exit {rc} for 1141s against a 300s budget; expected 1")
        if "over its budget" not in out:
            fail("3: the refusal does not say the lane is over budget")

        # 4. AND IT NAMES THE GATES. A budget that says only "too slow" sends the
        # reader back to the stopwatch this ticket was opened to retire.
        # Read off case 3's output rather than a run of its own: what is asserted
        # is that the refusal is actionable, not that the guard runs twice.
        if "seedtest" not in out or "857" not in out:
            fail("4: the refusal does not name the slowest gates from that run")

        # 5. UNDER BUDGET PASSES.
        under = work / "under.tsv"
        under.write_text(line("fast", 120, 140, 0.5))
        rc, out = run_guard(under)
        if rc != 0:
            fail(f"5: exit {rc} for 120s against a 300s budget")
        if "under the budget" not in out:
            fail("5: a passing run does not say it was under the budget")

        # 6. A LANDING RUN IS NOT A FAST RUN. The landing lane is minutes by
        # design -- it compiles. Reading one as a fast lane would make the guard
        # permanently red for the one reason that is not a defect.
        landing = work / "landing.tsv"
        landing.write_text(line("landing", 5000, 5200, 0.4))
        rc, out = run_guard(landing)
        if rc != 0:
            fail(f"6: exit {rc} with only a landing-lane run recorded; expected a skip")
        if "SKIPPED" not in out:
            fail("6: a landing-lane run was read as a fast-lane measurement")

        # 7. THE NEWEST IDLE RUN DECIDES, not the best one in the file. A median
        # or a minimum would hide today's regression behind last week's good runs.
        mixed = work / "mixed.tsv"
        mixed.write_text(line("fast", 100, 120, 0.5) + line("fast", 90, 100, 0.5) + line("fast", 900, 950, 0.6))
        rc, out = run_guard(mixed, judging=True)
        if rc != 1:
            fail(f"7: exit {rc}; the newest idle run is 900s and must decide, not the two good ones before it")

        # 7b. AND IT JUDGES AGAIN, WHICH IS WHAT #494 OWED. From #474 to #494
        # this guard printed its measurement and declined to judge it: the
        # budget had been derived on a shared Mac and the lane had moved to an
        # instance. The case that stood here asserted that clause and said it
        # would go red on the day the number came back. It has: the same
        # over-budget table now refuses AS THE GUARD SHIPS, without anything
        # being set in the test, and the report is not allowed to be a report.
        rc, out = run_guard(over)
        if rc != 1:
            fail(f"7b: exit {rc}; the premise was re-derived in #494, so an over-budget lane must refuse")
        if "SKIPPED" in out:
            fail("7b: it still says SKIPPED, so the suspension outlived the measurement that ended it")
        if "1141" not in out:
            fail("7b: the refusal does not carry the measurement it is refusing")
        if "seedtest" not in out:
            fail("7b: the refusal drops the slowest gates, which is the actionable half")
        # And the number it judges against is the measured one, not the Mac's.
        import re as _re
        bron = (pathlib.Path(__file__).resolve().parent / "the_fast_lane_stays_under_its_budget.py").read_text()
        if not _re.search(r'XFA_FAST_LANE_BUDGET", "240"', bron):
            fail("7b: the budget is not the 240 s #494 measured; a number nobody measured is not a budget")
        if "PREMISSE_HERZIEN = False" not in bron:
            fail("7b: the premise flag is not back to False, so the guard reports where it should judge")
        if "170 s of guard time" not in bron:
            fail("7b: the docstring does not carry the derivation, which is the whole of why 240 is not 300")

        # 7c. A ROW FROM ANOTHER LANE IS NOT THIS LANE'S TIME (#494). One pooled
        # instance serves every branch in turn and the table outlives them all.
        # Without the fingerprint the newest row is whichever tree used the
        # machine last: a lane measuring 165 s was refused on a 308 s row that
        # master's lane had written on the same instance twenty minutes earlier.
        vreemd = work / "vreemd.tsv"
        vreemd.write_text(line("fast", 900, 950, 0.5, fingerprint="deadbeefdead"))
        rc, out = run_guard(vreemd, judging=True)
        if rc != 0:
            fail(f"7c: exit {rc}; a 900 s row written by a different gate list must not refuse this one")
        if "none of THIS lane" not in out:
            fail("7c: it does not say why it is skipping, so a reader cannot tell it from 'no runs'")
        # And a row that DOES carry this lane's fingerprint is judged.
        eigen = work / "eigen.tsv"
        eigen.write_text(line("fast", 900, 950, 0.5))
        rc, out = run_guard(eigen, judging=True)
        if rc != 1:
            fail(f"7c: exit {rc}; a 900 s row from THIS gate list is exactly what the budget is for")

        # 7d. A ROW THAT NAMES NO GATE LIST NAMES NO LANE (#454). Every row
        # written before the fingerprint column existed looks like this, and on
        # a pooled instance they outlive the tree that wrote them: a lane of 22
        # gates was refused on a 313 s row from another tree whose only claim to
        # being this lane was an empty column.
        naamloos = work / "naamloos.tsv"
        naamloos.write_text(line("fast", 900, 950, 0.5, fingerprint=""))
        rc, out = run_guard(naamloos, judging=True)
        if rc != 0:
            fail(f"7d: exit {rc}; a row that names no gate list must not refuse this one")
        if "name no lane at all" not in out:
            fail("7d: the skip does not say the rows are nameless, which is the fixable half")

        # 7e. A LANDING IS NEVER BLOCKED BY A LANE TIME (23-09-2026). The same
        # over-budget row of this very lane, judged from the landing lane,
        # reports and passes: a time measures the machine, not the change, and
        # on 23-09 a 961 s row stopped landings that added no gate.
        rc, out = run_guard(eigen, judging=True, lane="landing")
        if rc != 0:
            fail(f"7e: exit {rc}; an over-budget row blocked a landing")
        if "not judged" not in out:
            fail("7e: the landing lane does not say it reports without judging")

        # 7f. NOR BY A ROW FROM ANOTHER MACHINE. A pooled instance's table holds
        # rows written on other hosts (and other instance types); in the fast
        # lane only a row from this machine is judged.
        elders = work / "elders.tsv"
        elders.write_text(line("fast", 900, 950, 0.5, host="gate-fast-somewhere-else"))
        rc, out = run_guard(elders, judging=True)
        if rc != 0:
            fail(f"7f: exit {rc}; a row written on another machine refused this one")
        if "SKIPPED" not in out or "gate-fast-somewhere-else" not in out:
            fail("7f: the skip does not name the machine the row came from")

        # 7g. A PASSING ROW FROM ANOTHER MACHINE DOES NOT HIDE THIS ONE'S
        # (codex, #1955): the newest row is under budget but from elsewhere,
        # the older one is this machine's and over. The verdict is this machine's.
        gemengd = work / "gemengd.tsv"
        gemengd.write_text(line("fast", 900, 950, 0.5)
                           + line("fast", 100, 120, 0.5, host="gate-fast-somewhere-else"))
        rc, out = run_guard(gemengd, judging=True)
        if rc != 1:
            fail(f"7g: exit {rc}; another machine's passing row hid this machine's over-budget one")

        # 9. AN OVERSHOOT THIS PUSH DID NOT CAUSE WARNS (COS, 23-09-2026). The
        # same over-budget row, from a push whose diff touches no fast-lane gate:
        # without this rule it is case 3's refusal, and #596 was refused for it.
        rc, out = run_guard(over, base="helper")
        if rc != 0:
            fail(f"9: exit {rc}; a push that adds no gate was refused for a lane it inherited")
        if "WARN" not in out or "not because of this push" not in out:
            fail("9: an inherited overshoot does not say it is inherited")

        # 10. NOR A PUSH WHOSE OWN GATE IS SMALLER THAN THE OVERSHOOT (T2, #596):
        # 268 s against 240, its one new gate absent from a full top-eight whose
        # smallest is 3 s. Without that gate the lane is still over: not the cause.
        full8 = ";".join(f"g{i}={s}" for i, s in enumerate([19, 12, 9, 7, 6, 5, 4, 3]))
        t2 = work / "t2.tsv"
        t2.write_text(line("fast", 268, 290, 0.5, slowest=full8))
        rc, out = run_guard(t2, budget="240", base="slow")
        if rc != 0:
            fail(f"10: exit {rc}; a gate of at most 3 s was blamed for 28 s over")
        if "frames" not in out:
            fail("10: the warning does not name the push's own gate")

        # 11. A PUSH THAT CHANGES ONLY THE FILE A GATE RUNS OWNS THAT GATE: the
        # gate list is identical, seed.py is not, and seedtest's 857 s is the cause.
        rc, out = run_guard(over, base="frames")
        if rc != 1:
            fail(f"11: exit {rc}; a push that made seedtest slow through its script was not refused")
        if "seedtest" not in out or "without them it fits" not in out:
            fail("11: the refusal does not name the push's own gate as the cause")

        # 13. A HELPER A GATE IMPORTS IS THAT GATE'S (codex, #1962): the push
        # changes only shared.py, which no command line names, and helped.py --
        # which imports it -- is the cause of the overshoot.
        helped = work / "helped.tsv"
        helped.write_text(line("fast", 1000, 1100, 0.5, slowest="helped=900;seedtest=20"))
        rc, out = run_guard(helped, base="helped")
        if rc != 1 or "helped" not in out:
            fail(f"13: exit {rc}; a push that slowed a gate through its imported helper was not refused")

        # 14. AND THE PUSH IS JUDGED ON ITS OWN ROW (codex, #1962): the lane's
        # first reading is the previous run's row, so the gate script asks
        # again after it has appended this run's. Read off the real script.
        gate = (CI / "local_ci_gate.sh").read_text()
        appended = gate.find('>> "$TIMINGS"')
        judged = gate.find("the_fast_lane_stays_under_its_budget.py --lane fast", appended)
        if appended < 0 or judged < 0:
            fail("14: the gate does not judge the row it has just written; a slower lane "
                 "passes on the row before it and lands before anyone is told")

        # 12. NO BASE, NO VERDICT: a push whose diff cannot be read warns.
        rc, out = run_guard(over, base="0" * 40)
        if rc != 0 or "base could not be found" not in out:
            fail(f"12: exit {rc}; with no base the guard must warn and say why: {out[-200:]}")

        # 8. A RAGGED LINE DOES NOT KILL IT. The table is appended by a shell
        # script on a machine that can be killed mid-write.
        ragged = work / "ragged.tsv"
        ragged.write_text("not a row\n" + "\t".join(["a", "b"]) + "\n" + line("fast", 100, 120, 0.5))
        rc, out = run_guard(ragged)
        if rc != 0:
            fail(f"8: exit {rc} on a table with a half-written line; it should skip the line, not die")

    if cases < FLOOR:
        print(f"FAIL: ran {cases} cases, floor is {FLOOR} -- the suite stopped short.", file=sys.stderr)
        return 1
    if failures:
        print(f"FAIL: {failures} of {cases} cases failed.", file=sys.stderr)
        return 1
    print(f"PASS: {cases} cases.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
