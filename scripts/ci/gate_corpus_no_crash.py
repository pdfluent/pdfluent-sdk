#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Nothing in the gate corpus may panic, abort or hang the renderer.

This is the gate that used to be `crash-guard.yml`. That workflow asked for a
runner labelled `xfa-corpus`, which nobody ever registered, so every automatic
run of it sat in the queue until GitHub abandoned it about a day later -- never
red, never run (#276). It also read a corpus that lived on one external disk,
and that disk stopped answering reads.

So this one reads the gate corpus instead: five hundred files fetched from
public suites by URL at a pinned commit and verified against a SHA-256 each,
about 35 MB, roughly twenty seconds cold and nothing at all warm. Small enough
to run on every pull request, which is the only property that made the old one
worth having.

# FLOOR: PDFs rendered >= 500 -- two-way. Below it, files vanished from the
# corpus and a shrinking gate reports green while measuring less. Above it, the
# corpus grew without anyone choosing the new baseline, so every later
# comparison quietly means something different. Both directions are the same
# failure as #276: nothing red, nothing running.

An expected error is not a crash. A password-protected file, a truncated one, a
format we do not support -- the binary refusing those cleanly is the binary
working. What must never happen is a panic, a signal, or a hang.

Usage:
    python3 scripts/ci/gate_corpus_no_crash.py \
        --binary target/release/pdfluent \
        --corpus /tmp/gate-corpus \
        --known corpus/GATE_CORPUS_KNOWN_CRASHES.json
"""

from __future__ import annotations

import argparse
import concurrent.futures
import os
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time

VLOER = 500

# A panic message, an abort, a segfault, a kill. Not "Error: PasswordProtected".
CRASH = re.compile(
    r"panicked at|stack backtrace|SIGSEGV|SIGABRT|SIGBUS|Segmentation fault|"
    r"Bus error|Aborted|core dumped|memory allocation of \d+ bytes failed|"
    r"signal: \d+",
    re.IGNORECASE,
)

TIMEOUT_S = 30
# Five hundred files that each hang for TIMEOUT_S would outlast the job and be
# killed by GitHub with no output at all -- a gate that reports nothing looks
# exactly like a gate that has not got round to you yet, which is #276 again.
# The budget turns that into a result: too slow is a finding, not a timeout.
#
# Enforced WHILE running, not after. The first version compared the wall clock
# with the budget once the pool had drained, so a run where every file hung
# was detected 500 x 30 s / 4 workers = about an hour in -- long after the
# point the budget was meant to stop it (codex, #1627). Now every task gets
# the run's deadline: one that has not started by then does not start, and one
# still running is cut at the deadline instead of at its own 30 s.
BUDGET_S = 900


def een(binary: str, pdf: pathlib.Path, deadline: float | None = None,
        timeout_s: float = TIMEOUT_S) -> tuple[str, str, str]:
    """Return (name, verdict, detail). Verdict is ok | crash | hang | budget.

    `deadline` is a time.monotonic() value shared by the whole run. Past it,
    nothing starts; up to it, a task may run for at most the time left."""
    timeout = timeout_s
    if deadline is not None:
        left = deadline - time.monotonic()
        if left <= 0:
            return (pdf.name, "budget", "not started: the run's budget was spent "
                                        "before its turn")
        timeout = min(timeout_s, left)
    uit = tempfile.mkdtemp(prefix="gatepage-")
    try:
        r = subprocess.run(
            [binary, "render", str(pdf), "-o", uit, "-d", "72", "-p", "1"],
            capture_output=True, text=True, errors="replace",
            timeout=timeout,
            # Inherit the environment rather than replace it. A bare env drops
            # HOME and XDG_*, which is where fontconfig looks; every page that
            # needs a substituted font would then fail for a reason that has
            # nothing to do with the renderer.
            env={**os.environ, "RUST_BACKTRACE": "1"},
        )
    except subprocess.TimeoutExpired:
        if timeout < timeout_s:
            # Cut by the run's deadline, not by its own limit: a budget
            # finding, and it says so, rather than a hang it never got the
            # full 30 s to prove.
            return (pdf.name, "budget",
                    f"still running when the run's budget ran out, {timeout:.0f}s "
                    f"into its {timeout_s:.0f}s")
        return (pdf.name, "hang",
                f"still running after {timeout_s:.0f}s on page 1 at 72 dpi")
    except OSError as e:
        return (pdf.name, "crash", f"could not be executed: {e}")
    finally:
        shutil.rmtree(uit, ignore_errors=True)

    tekst = (r.stderr or "") + (r.stdout or "")
    if CRASH.search(tekst):
        regel = next((l for l in tekst.splitlines() if CRASH.search(l)), "")
        return (pdf.name, "crash", regel.strip()[:200])
    # A negative return code is a signal even when nothing was printed.
    if r.returncode < 0:
        return (pdf.name, "crash", f"killed by signal {-r.returncode}")
    return (pdf.name, "ok", "")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--binary", required=True)
    p.add_argument("--corpus", required=True)
    p.add_argument("--known", default="corpus/GATE_CORPUS_KNOWN_CRASHES.json")
    p.add_argument("--workers", type=int, default=4)
    p.add_argument("--budget", type=float, default=BUDGET_S,
                   help="seconds for the whole run; the test drives this down")
    p.add_argument("--timeout", type=float, default=TIMEOUT_S,
                   help="seconds per file before it counts as a hang")
    args = p.parse_args()

    binary = pathlib.Path(args.binary).resolve()
    if not binary.is_file():
        print(f"FATAL: no binary at {binary}. The build step did not produce "
              "one, which is a failure and not a reason to skip the gate.",
              file=sys.stderr)
        return 1

    corpus = pathlib.Path(args.corpus)
    pdfs = sorted(x for x in corpus.glob("*") if x.is_file())
    n = len(pdfs)

    if n < VLOER:  # FLOOR
        print(f"FATAL: {n} PDFs in {corpus}, floor is {VLOER}. The corpus is "
              "incomplete, and a gate that measures fewer files than it was "
              "set up with passes more easily every time it shrinks. Run "
              "scripts/ci/fetch_gate_corpus.py first. (#276)", file=sys.stderr)
        return 1
    if n > VLOER:  # FLOOR, upward half
        print(f"FATAL: {n} PDFs in {corpus}, floor is {VLOER}. The corpus grew "
              "without VLOER being edited here, so this run is not comparable "
              "with the last one. Raise the floor in the same commit as the "
              "manifest.", file=sys.stderr)
        return 1

    bekend: dict[str, str] = {}
    kpad = pathlib.Path(args.known)
    if kpad.is_file():
        bekend = json.loads(kpad.read_text()).get("known", {})

    crashes: list[tuple[str, str, str]] = []
    begonnen = time.monotonic()
    deadline = begonnen + args.budget
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=args.workers)
    futures = [pool.submit(een, str(binary), f, deadline, args.timeout) for f in pdfs]
    klaar = 0
    over_budget: tuple[str, str, str] | None = None
    # In submission order, so the count of finished files means "the first k".
    # Every future resolves by the deadline at the latest: a running task is
    # cut there and a waiting one returns at once.
    for fut in futures:
        naam, oordeel, detail = fut.result()
        if oordeel == "budget":
            over_budget = (naam, oordeel, detail)
            break
        klaar += 1
        if oordeel != "ok":
            crashes.append((naam, oordeel, detail))
    if over_budget is not None:
        # Nothing waiting starts; whatever is running sees the deadline and
        # returns. Not `wait=True`: the point is to report now.
        pool.shutdown(wait=False, cancel_futures=True)
        duur = time.monotonic() - begonnen
        print(f"[no-crash] {klaar} of {n} PDFs rendered in {duur:.0f}s, "
              f"{len(crashes)} crashed or hung, then the budget ran out")
        print(f"\n[no-crash] FATAL: {args.budget:.0f}s budget spent with {n - klaar} "
              f"of {n} files still to go (first not finished: {over_budget[0]}, "
              f"{over_budget[2]}). Page one of a file under 3 MB at 72 dpi does "
              "not take this long; something got much slower, and left to run "
              "this would overrun the job and report nothing at all.",
              file=sys.stderr)
        for naam, oordeel, detail in crashes:
            print(f"  {oordeel.upper():5} {naam}: {detail}", file=sys.stderr)
        return 1
    pool.shutdown(wait=True)
    duur = time.monotonic() - begonnen

    gevonden = {c[0] for c in crashes}
    nieuw = sorted(gevonden - set(bekend))
    genezen = sorted(set(bekend) - gevonden)

    print(f"[no-crash] {n} PDFs rendered in {duur:.0f}s, {len(crashes)} crashed "
          f"or hung, {len(bekend)} on the known list")

    if duur > args.budget:
        print(f"\n[no-crash] FATAL: {duur:.0f}s for {n} files, budget is "
              f"{args.budget:.0f}s. Page one of a file under 3 MB at 72 dpi does not "
              "take this long; something got much slower, and the next step is "
              "for this to overrun the job and report nothing at all.",
              file=sys.stderr)
        return 1

    if nieuw:
        print(f"\n[no-crash] FATAL: {len(nieuw)} file(s) newly crash or hang "
              "the renderer:", file=sys.stderr)
        for naam, oordeel, detail in crashes:
            if naam in nieuw:
                print(f"  {oordeel.upper():5} {naam}: {detail}", file=sys.stderr)
        print("\nA panic is not an error path. Fix it, or -- if it is a known "
              "limitation with an issue behind it -- add the file to "
              f"{kpad} with the reason, in a commit somebody reviews.",
              file=sys.stderr)
        return 1

    if genezen:
        # The upward half of the same ratchet. A known-crash list that only
        # ever grows is a list nobody prunes, and then it stops describing
        # anything.
        print(f"\n[no-crash] FAIL: {len(genezen)} file(s) on the known-crash "
              "list no longer crash. Good news, but the list now excuses "
              "something that is fixed, so the next regression on those files "
              "would pass unnoticed. Remove them:", file=sys.stderr)
        for g in genezen:
            print(f"  {g}  ({bekend[g]})", file=sys.stderr)
        return 1

    print(f"[no-crash] OK: {n} PDFs, no new crash, no stale entry on the "
          "known list.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
