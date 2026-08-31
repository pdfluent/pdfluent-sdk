#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for disk_headroom.py.

The floor cases are easy. The one that earns its place is the heartbeat: a
janitor that stops running looks exactly like a janitor with nothing to do, and
that is not a hypothetical here -- com.pdfluent.cleanup sat installed and dead
for months because launchd has no Full Disk Access and said nothing about it.

So these tests care most about the states where the check has NOT run. Every one
of them must be distinguishable from a pass.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import time

GUARD = pathlib.Path(__file__).resolve().parent / "disk_headroom.py"

failures: list[str] = []


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), *args], capture_output=True, text=True, check=False
    )


def expect(name: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name} {detail}")
        failures.append(name)


def main() -> int:
    if not GUARD.exists():
        print(f"SKIPPED (not a pass): {GUARD} is missing", file=sys.stderr)
        return 3

    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = pathlib.Path(tmp)
        beat = tmpdir / "beat.json"

        # --- the floor ---------------------------------------------------
        r = run("--repo", tmp, "--floor-gb", "0", "--heartbeat", str(beat))
        expect("a reachable floor passes", r.returncode == 0, r.stderr.strip()[:120])

        r = run("--repo", tmp, "--floor-gb", "999999", "--heartbeat", str(beat))
        expect("an unreachable floor fails with 2", r.returncode == 2, f"got {r.returncode}")
        expect(
            "the failure names the floor",
            "Below the floor" in r.stderr,
            r.stderr.strip()[:120],
        )

        # --- it records that it ran ---------------------------------------
        expect("running writes a heartbeat", beat.exists())
        if beat.exists():
            data = json.loads(beat.read_text())
            expect("the heartbeat carries a timestamp", isinstance(data.get("checked_at"), int))

        # --- the states where it has NOT run ------------------------------
        r = run("--check-heartbeat", "--heartbeat", str(tmpdir / "never-written.json"))
        expect("a missing heartbeat is not a pass", r.returncode == 3, f"got {r.returncode}")
        expect(
            "a missing heartbeat announces itself",
            "SKIPPED (not a pass)" in r.stderr,
            r.stderr.strip()[:120],
        )

        stale = tmpdir / "stale.json"
        stale.write_text(
            json.dumps({"checked_at": int(time.time()) - 200 * 3600, "free_bytes": 1, "total_bytes": 2})
        )
        r = run("--check-heartbeat", "--heartbeat", str(stale), "--max-age-hours", "36")
        expect("a stale heartbeat fails", r.returncode == 2, f"got {r.returncode}")
        expect(
            "a stale heartbeat explains the usual cause",
            "Full Disk Access" in r.stderr,
            r.stderr.strip()[:120],
        )

        corrupt = tmpdir / "corrupt.json"
        corrupt.write_text("this is not json")
        r = run("--check-heartbeat", "--heartbeat", str(corrupt))
        expect("an unreadable heartbeat is not a pass", r.returncode == 3, f"got {r.returncode}")

        fresh = tmpdir / "fresh.json"
        fresh.write_text(json.dumps({"checked_at": int(time.time()), "free_bytes": 1, "total_bytes": 2}))
        r = run("--check-heartbeat", "--heartbeat", str(fresh), "--max-age-hours", "36")
        expect("a fresh heartbeat passes", r.returncode == 0, r.stderr.strip()[:120])

        # --- it must not pass on a path it cannot measure ------------------
        r = run("--repo", str(tmpdir / "does-not-exist"))
        expect("an unmeasurable path is not a pass", r.returncode == 3, f"got {r.returncode}")

    if failures:
        print(f"\n{len(failures)} case(s) failed: {', '.join(failures)}", file=sys.stderr)
        return 1
    print("\ndisk_headroom: 12 cases, all as intended")
    return 0


if __name__ == "__main__":
    sys.exit(main())
