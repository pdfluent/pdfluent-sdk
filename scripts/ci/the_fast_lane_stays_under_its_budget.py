#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Has the fast lane crept back over its budget? (#425)

WHAT HAPPENED

The fast lane is meant to be about three minutes. On 07-09-2026 it was nineteen
to twenty-seven, and nobody knew: a gate run prints a verdict per gate and no
times, so a guard that gained two minutes looked exactly like one that gained
two seconds. Two of a hundred and forty-six gates turned out to be 38% of the
lane. That is not a thing anyone would have noticed by reading; it took a
stopwatch around every gate and one full run.

So local_ci_gate.sh now times every gate and appends each run to a table, and
this reads that table.

WHY A LOAD FILTER, AND WHY IT IS THE WHOLE DESIGN

A lane time is only a fact about the lane if the machine was not doing anything
else. On the machine this was measured on, five terminals and a landing gate ran
at once and the same lane took anywhere from 1152 to 6342 seconds. A budget
guard without a load filter would be red all afternoon and green at night, would
be believed on neither, and would be switched off within a week -- which is how
a repository ends up with guards nobody reads.

So: only runs recorded below IDLE_LOAD are judged. If no such run exists, this
says so and does not pass. "I could not look" is not "I looked and it was fine";
that distinction is the reason several measurements in this repository had to be
withdrawn.

WHY THE NEWEST AND NOT THE MEDIAN

The question is "is the lane over budget NOW", and a median over a week hides
the commit that doubled it behind six days of good runs. The newest qualifying
run answers the question that is being asked; the table keeps the rest so a
person can see the trend.

WHERE THE NUMBER COMES FROM, SINCE 09-09-2026 (#494)

240 seconds, and the derivation is three measurements rather than a preference.

The lane was measured on a quiet cpx42 -- the machine a push actually runs on
since #474 -- at 395 s, with two compiling gates (73 s) and two sleep-driven
self-tests of the lock and the instance protocol (141 s) in it. #473 moved the
compilers to the landing lane and this change moved the two sleeps, and the same
lane on the same instance then measured **170 s of guard time over 188 gates**,
with no gate over 19 s and a flat tail behind it.

240 s is that measurement plus a margin of about 40%, which is what the spread
between runs on one machine has actually been (302-355 s before the moves, on
the same commit). It is a ceiling and not a target: a lane that grows past it
has gained a minute somewhere, and a minute is a gate somebody should have put
on the landing lane.

And it answers the question the pool asks rather than the one a shared Mac asked:
a fast lane at 170 s fits twenty times into a billed hour of the one machine both
products share (#494), so the budget is a fraction of 3600 s and not a fraction
of anybody's patience.

WHY IT DID NOT JUDGE, BETWEEN #474 AND #494

The budget is 300 seconds because that is what a push cost the shared Mac in
#425: one machine, every terminal on it, a lane that took five minutes took
five minutes away from everybody. Since #474 a push does not run there. It runs
on an instance bought for it, billed as one started hour whether the lane takes
four minutes or forty, and since #491 that hour is shared with the next gate. A
number derived from contention on a shared machine is not a number about a
machine nobody else is on.

Nobody noticed, because nobody could: every gate got a fresh instance with an
empty table, so this guard answered "no run to judge" on every remote lane and
was, in effect, switched off. The pool gave it a real idle measurement for the
first time -- 395 s on a cpx42, load 0.75 -- and the honest reading of that
number is that the premise moved, not that the lane doubled overnight.

So it reports and does not judge until #494 re-derives the budget from three
runs on a quiet instance. Deliberately NOT done: raising XFA_FAST_LANE_BUDGET
to whatever today happens to measure. That is how a ratchet becomes a
rubber stamp -- the number would then mean "the lane is as slow as it is". The
guard stays wired, keeps printing the measurement on every run, and starts
judging again the moment RUIMTE_HERZIEN is set back to False with a budget
somebody measured.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import re
import socket
import subprocess
import sys

# Seconds of guard time, not wall time: wall time includes the queue wait for the
# machine-wide lock (#359), which is not this lane's cost to carry. Five minutes
# is the ticket's number, and it is a ceiling and not a target -- the lane was
# about three minutes before it crept.
BUDGET = int(os.environ.get("XFA_FAST_LANE_BUDGET", "240"))

# THE RETURN CONDITION, and it has returned. This was True from #474 to #494:
# the budget had been derived on a shared Mac and the lane had moved to an
# instance, so the guard printed its measurement and declined to judge it.
# #494 re-derived the number from the machine the lane runs on, and this goes
# back to False in the same commit -- which is what the clause said it was for.
# Nothing may flip it back without a measurement; an environment variable here
# would be the budget raise this whole file argues against, one indirection
# further away.
PREMISSE_HERZIEN = False

# One-minute load average below which a measurement is worth judging. Two on a
# ten-core machine is a machine doing this run and very little else.
IDLE_LOAD = float(os.environ.get("XFA_FAST_LANE_IDLE_LOAD", "2.0"))

TIMINGS = pathlib.Path(
    os.environ.get("XFA_GATE_TIMINGS", os.path.join(os.environ.get("TMPDIR", "/tmp"), "xfa-gate-timings.tsv"))
)


def rows(path: pathlib.Path) -> list[dict]:
    """The recorded runs, oldest first. A line this cannot read is skipped.

    Skipped and not fatal: the table is written by a shell script on a machine
    that may have been rebooted, killed or filled up mid-append, and a guard that
    dies on one ragged line would be a guard that reports nothing exactly when
    the machine has been having a bad day.
    """
    out: list[dict] = []
    if not path.exists():
        return out
    for line in path.read_text(errors="replace").splitlines():
        parts = line.split("\t")
        if len(parts) < 5:
            continue
        when, lane, guard_s, wall_s, load = parts[0], parts[1], parts[2], parts[3], parts[4]
        slowest = parts[5] if len(parts) > 5 else ""
        # The seventh field arrived with #494; a row without one is from before
        # that and is judged as it always was, which is what keeps a table
        # written across an upgrade readable.
        lane_id = parts[6].strip() if len(parts) > 6 else ""
        host = parts[7].strip() if len(parts) > 7 else ""
        try:
            out.append(
                {
                    "when": when,
                    "lane": lane,
                    "guard": int(guard_s),
                    "wall": int(wall_s),
                    "load": float(load),
                    "slowest": slowest,
                    "lane_id": lane_id,
                    "host": host,
                }
            )
        except ValueError:
            continue
    return out


def lane_fingerprint() -> str:
    """The first twelve hex digits of local_ci_gate.sh's sha256.

    The same twelve the gate itself writes into the table. Read from the file
    rather than passed in: a guard that took the lane's identity as an argument
    could be told it was judging a lane it was not.
    """
    gate = pathlib.Path(__file__).resolve().parent / "local_ci_gate.sh"
    try:
        return hashlib.sha256(gate.read_bytes()).hexdigest()[:12]
    except OSError:
        return ""


# A gate in the fast lane is a `run` line; the other helpers defer to a landing.
GATE_LINE = re.compile(r"^\s*run\s+([A-Za-z0-9_]\S*)(.*)$")


def fast_gates(text: str) -> dict[str, str]:
    """Name -> command of every gate the fast lane runs, continuation lines joined."""
    joined = re.sub(r"\\\n\s*", " ", text)
    out: dict[str, str] = {}
    for line in joined.splitlines():
        m = GATE_LINE.match(line)
        if m:
            out[m.group(1)] = " ".join(m.group(2).split())
    return out


def own_gates(root: pathlib.Path) -> set[str] | None:
    """The fast-lane gates this push adds or changes, or None when there is no base.

    Added: a `run` name the base does not have (a `zwaar` promoted to `run` is
    one). Changed: its command line differs, or a file it names differs from the
    base in the working tree. Base as in development_gate_scope.py: LANDING_BASE,
    else the merge-base with github/master; GIT_* is dropped so a hook's GIT_DIR
    does not answer for another tree.
    """
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

    def git(*args: str) -> str:
        return subprocess.run(["git", *args], cwd=root, env=env, capture_output=True,
                              text=True, check=True).stdout

    try:
        base = os.environ.get("LANDING_BASE") or git(
            "merge-base", "HEAD", "refs/remotes/github/master").strip()
        before = fast_gates(git("show", f"{base}:scripts/ci/local_ci_gate.sh"))
        changed = set(git("diff", "--no-renames", "--name-only", base).split())
        changed |= set(git("ls-files", "--others", "--exclude-standard").split())
    except (subprocess.CalledProcessError, OSError):
        return None
    now = fast_gates((root / "scripts" / "ci" / "local_ci_gate.sh").read_text())
    return {n for n, cmd in now.items()
            if before.get(n) != cmd or any(t in changed for t in cmd.split())
            or changed & helpers_of(root, cmd)}


IMPORT = re.compile(r"^\s*(?:from\s+([A-Za-z_]\w*)\s+import|import\s+([A-Za-z_]\w*))", re.M)


def helpers_of(root: pathlib.Path, cmd: str) -> set[str]:
    """The sibling modules a gate's Python script imports, as repository paths.

    A change to `fixture_env.py` changes every gate that imports it, although no
    command line names it (codex, #1962). One level deep: a gate is charged for
    what its own script imports, which is where the time of a helper is spent.
    """
    out: set[str] = set()
    for token in cmd.split():
        if not token.endswith(".py"):
            continue
        script = root / token
        try:
            text = script.read_text(errors="replace")
        except OSError:
            continue
        for a, b in IMPORT.findall(text):
            sibling = script.parent / f"{a or b}.py"
            if sibling.is_file():
                out.add(sibling.relative_to(root).as_posix())
    return out


def cost_of(names: set[str], slowest: str) -> int:
    """Seconds those gates took in the row, as far as its top-eight list tells.

    A gate outside a full list of eight took at most the eighth's time, so that
    is what it is charged: the bound that can blame a push, never the one that
    lets a cause go.
    """
    listed = {}
    for item in slowest.split(";"):
        if "=" in item:
            n, secs = item.split("=", 1)
            try:
                listed[n] = int(secs)
            except ValueError:
                continue
    floor = min(listed.values()) if len(listed) >= 8 else 0
    return sum(listed.get(n, floor) for n in names)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--lane", default="fast",
                    help="the lane running this guard; only the fast lane is judged")
    args = ap.parse_args()
    runs = rows(TIMINGS)
    fast = [r for r in runs if r["lane"] == "fast"]
    # ONLY RUNS OF THIS LANE (#494). One pooled instance serves every branch in
    # turn and the table outlives them all: without this the newest row is
    # whichever branch used the machine last, and a lane that measures 165 s was
    # refused on a 308 s row written by another tree twenty minutes earlier.
    # AND A ROW OF UNKNOWN PROVENANCE IS NOT THIS LANE EITHER (#454). The filter
    # above admitted a row whose lane_id column was empty, which is every row
    # written before that column existed -- and on a POOLED instance those rows
    # outlive the tree that wrote them. On 09-09-2026 a lane of 22 gates was
    # refused on a 313 s row from 22:17Z that no longer had a lane to belong to:
    # remotegate 74 s and gatelock 71 s, two gates this lane defers. A time is a
    # fact about a gate list, and a row that cannot name its gate list states no
    # fact at all -- admitting it is the exact thing this filter was added to
    # stop, spelled with an empty string.
    mijn = lane_fingerprint()
    if mijn:
        eigen = [r for r in fast if r["lane_id"] == mijn]
        if fast and not eigen:
            naamloos = sum(1 for r in fast if not r["lane_id"])
            print(f"SKIPPED (not a pass): {len(fast)} fast-lane run(s) recorded, none of THIS lane"
                  + (f" ({naamloos} of them name no lane at all)." if naamloos else "."))
            print(f"  This gate list hashes to {mijn}; every row in {TIMINGS} was written by")
            print("  another one. A time is a fact about a gate list, and a lane that has just")
            print("  been edited has not been measured yet -- run the lane once and this judges again.")
            return 0
        fast = eigen
    # ONLY THIS MACHINE'S ROWS, before anything is picked (codex, #1955): a
    # passing row from another host must not read as this lane under budget,
    # and must not hide an older over-budget row of this one. The landing and
    # full lanes only report, so they read the whole table.
    if args.lane == "fast" and fast:
        host = socket.gethostname()
        hier = [r for r in fast if r["host"] == host]
        if not hier:
            anderen = sorted({r["host"] or "an unnamed machine" for r in fast})
            print(f"SKIPPED (not a pass): {len(fast)} fast-lane run(s) of this lane recorded, none on "
                  f"this machine ({host}); they were written on {', '.join(anderen)}. A time is a fact "
                  "about the machine that took it.")
            return 0
        fast = hier
    if not fast:
        print(f"SKIPPED (not a pass): no fast-lane run recorded in {TIMINGS}.")
        print("  The table is written by scripts/ci/local_ci_gate.sh. A checkout that")
        print("  has never run the gate has nothing to judge.")
        return 0

    idle = [r for r in fast if r["load"] <= IDLE_LOAD]
    if not idle:
        busiest = min(r["load"] for r in fast)
        print(f"SKIPPED (not a pass): {len(fast)} fast-lane run(s) recorded, none on an idle machine.")
        print(f"  Lowest load seen was {busiest:.2f}; the budget is only meaningful at or below {IDLE_LOAD}.")
        print("  A lane time measured beside four other builds says nothing about the lane.")
        return 0

    newest = idle[-1]
    print(f"[fast-lane-budget] newest idle run {newest['when']}: "
          f"{newest['guard']}s of guard time, {newest['wall']}s wall, load {newest['load']:.2f}"
          + (f", on {newest['host']}" if newest["host"] else ""))
    if newest["guard"] <= BUDGET:
        print(f"[fast-lane-budget] under the budget of {BUDGET}s.")
        return 0
    # A TIME MEASURES THE MACHINE, NOT THE CHANGE (owner via COS, 23-09-2026).
    # On a pooled instance the table holds rows from every branch it served, on
    # whatever type the pool could buy that hour; on 23-09 a 961 s row stopped
    # landings whose own diff added no gate. So a landing is never blocked by
    # it -- it reports -- and the fast lane refuses only on a row it wrote on
    # this same machine.
    if args.lane != "fast":
        print(f"[fast-lane-budget] over the budget of {BUDGET}s, reported and not judged: "
              f"this is the {args.lane} lane, and a lane time says nothing about the change being landed.")
        return 0


    if PREMISSE_HERZIEN:
        print(f"SKIPPED (not a pass): {newest['guard']}s of guard time against a budget of "
              f"{BUDGET}s, and this guard is not judging that.")
        print("  The budget premise changed with #474: a push no longer costs the shared")
        print("  Mac five minutes of everybody's machine, it costs one started hour of an")
        print("  instance bought for it, shared with the next gate since #491. #494 re-derives")
        print("  the number from three runs on a quiet instance; until then this reports.")
        if newest["slowest"]:
            print("  slowest gates in that run: " + ", ".join(
                f"{n}={s}s" for n, s in (i.split("=", 1) for i in newest["slowest"].split(";") if "=" in i)))
        return 0

    # ONLY THE PUSH THAT PUTS THE LANE OVER IS REFUSED (COS, 23-09-2026). T2's
    # #596 was refused at 268 s against 240 s for gates it did not touch, and
    # would have been again for the 0.1 s gate it does add. The question is
    # whether this push is the cause: without its own gates' time, is the lane
    # back under? If so, refuse; otherwise it inherited the overshoot, and warns.
    root = pathlib.Path(__file__).resolve().parents[2]
    eigen = own_gates(root)
    if eigen is None:
        print(f"WARN: {newest['guard']}s of guard time, over the budget of {BUDGET}s; not judged, "
              "because this push's base could not be found (no LANDING_BASE, no github/master).")
        return 0
    kosten = cost_of(eigen, newest["slowest"])
    if newest["guard"] - kosten > BUDGET:
        print(f"WARN: the fast lane is {newest['guard']}s of guard time, over its budget of {BUDGET}s,"
              " but not because of this push: "
              + (f"its own gates ({', '.join(sorted(eigen))}) cost at most {kosten}s of that."
                 if eigen else "it adds or changes no fast-lane gate."))
        if newest["slowest"]:
            print("  slowest gates in that run: " + ", ".join(
                f"{n}={s}s" for n, s in (i.split("=", 1) for i in newest["slowest"].split(";") if "=" in i)))
        return 0

    print(f"FAIL: the fast lane is {newest['guard']}s of guard time, over its budget of {BUDGET}s.")
    print(f"  This push's own gates ({', '.join(sorted(eigen))}) took {kosten}s of that; without them it fits.")
    if len(eigen) > 1 or any(f"{n}=" not in newest["slowest"] for n in eigen):
        print("  A gate outside the row's top eight is charged the eighth's time, an upper bound:")
        print("  with many small new gates that can overstate this push's share (T2, #596).")
    if newest["slowest"]:
        print("  slowest gates in that run:")
        for item in newest["slowest"].split(";"):
            if "=" in item:
                name, secs = item.split("=", 1)
                print(f"    {name:<16} {secs}s")
    print("  Move a gate to the landing lane with `zwaar` if what it watches only")
    print("  matters at a landing, or make it cheaper. Raising XFA_FAST_LANE_BUDGET")
    print("  is not a fix: the budget is what every push on this machine pays.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
