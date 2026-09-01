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

The second group is newer and cost more. On the build machine the guard reported
876.1 GB free while the volume underneath had 9.5 GB, because inside WSL the root
filesystem is a sparse vhdx that reports its maximum rather than what the host
can still give it. A guard passing by a factor of thirty-five on the one host
where the failure happens is worse than no guard: it is a control that has been
switched off and still reports.

Those cases build a WSL machine out of two text files, because the real one
cannot be borrowed and the situation cannot be reached from a Mac.

FLOOR: cases run >= 20 -- this file has more, and a run that executes a handful
has lost its way through the script rather than found a clean tree.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import time

import os

GUARD = pathlib.Path(__file__).resolve().parent / "disk_headroom.py"

# FLOOR: cases run >= 20. A test that stops testing reports success in the same
# words as one that passed.
MINIMUM_CASES = 20

# TWO-WAY RATCHET. disk_headroom.DEFAULT_FLOOR_GB and this number must move
# together, in either direction. Lowering the floor silently switches the guard
# off while it goes on reporting; raising it silently turns every lane red for a
# reason unrelated to the change under test, and the usual repair for that is to
# stop running the guard at all. Both are one-line edits over there and neither
# is visible in a diff of that file alone.
PINNED_FLOOR_GB = 25

failures: list[str] = []
cases = 0


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), *args], capture_output=True, text=True, check=False
    )


def expect(name: str, condition: bool, detail: str = "") -> None:
    global cases
    cases += 1
    if condition:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name} {detail}")
        failures.append(name)


def wsl_machine(tmpdir: pathlib.Path, host_volumes: list[str], repo: pathlib.Path) -> dict[str, str]:
    """Two text files that make an ordinary machine answer as a WSL one.

    `/proc/version` is how the kernel says it is running under Windows, and
    `/proc/self/mountinfo` is the only place the answer to "which of these is the
    Windows volume" lives. Both are read as text, so both can be handed over.
    """
    version = tmpdir / f"proc-version-{len(host_volumes)}-{repo.name}"
    version.write_text("Linux version 5.15.0-microsoft-standard-WSL2\n")

    rows = ["1 0 8:1 / / rw - ext4 /dev/sdd rw"]
    for index, point in enumerate(host_volumes, start=2):
        pathlib.Path(point).mkdir(parents=True, exist_ok=True)
        rows.append(f"{index} 1 0:{index} / {point} rw - 9p C:\\ rw")
    info = tmpdir / f"mountinfo-{len(host_volumes)}-{repo.name}"
    info.write_text("\n".join(rows) + "\n")

    return {"DISK_HEADROOM_PROC_VERSION": str(version), "DISK_HEADROOM_MOUNTINFO": str(info)}


def run_env(env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    merged = dict(os.environ)
    merged.update(env)
    return subprocess.run(
        [sys.executable, str(GUARD), *args], capture_output=True, text=True, check=False, env=merged
    )


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

        # --- the vhdx that reports its maximum -----------------------------
        #
        # The repository sits on `/`, which under WSL is the sparse vhdx. The
        # answer must come from the Windows volume instead. These cases assert
        # WHICH volume was measured rather than how many bytes it had: the bytes
        # differ per machine, the choice of volume is the logic.
        repo = tmpdir / "repo"
        repo.mkdir()
        host = tmpdir / "hostvol"
        env = wsl_machine(tmpdir, [str(host)], repo)

        r = run_env(env, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect(
            "on WSL the Windows volume is what gets measured",
            f"measured: {host}" in r.stdout,
            r.stdout.strip()[:200],
        )
        expect("and the run still succeeds", r.returncode == 0, r.stderr.strip()[:160])

        # The control. Same mount table, same paths, only the kernel no longer
        # says Windows -- so the ordinary answer must come back. Without this the
        # case above would also pass if the script always reported the first
        # mount it found.
        not_wsl = dict(env)
        plain = tmpdir / "proc-version-plain"
        plain.write_text("Linux version 6.8.0-generic\n")
        not_wsl["DISK_HEADROOM_PROC_VERSION"] = str(plain)
        r = run_env(not_wsl, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect(
            "off WSL the path's own filesystem is what gets measured",
            f"measured: {repo}" in r.stdout,
            r.stdout.strip()[:200],
        )

        # C: by preference. WSL keeps the distro vhdx on the system drive unless
        # it was deliberately moved, so that is the volume it grows into.
        #
        # The other drive is deliberately named so that it wins on every
        # tie-break the code could fall back on -- it is first in the mount table
        # and first alphabetically. Without that, dropping the C: rule entirely
        # would still land on C: by accident and this case would pass over a
        # guard that no longer exists.
        drive_c = tmpdir / "drives" / "c"
        drive_a = tmpdir / "drives" / "a"
        env_two = wsl_machine(tmpdir, [str(drive_a), str(drive_c)], repo)
        r = run_env(env_two, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect(
            "with two host volumes the system drive is chosen",
            f"measured: {drive_c}" in r.stdout,
            r.stdout.strip()[:200],
        )

        # A path already on the Windows volume is measured where it lives; the
        # host lookup must not send it somewhere else.
        on_host = host / "checkout"
        on_host.mkdir(parents=True, exist_ok=True)
        r = run_env(env, "--repo", str(on_host), "--floor-gb", "0", "--heartbeat", str(beat))
        expect(
            "a path already on the Windows volume is measured there",
            f"measured: {host}" in r.stdout,
            r.stdout.strip()[:200],
        )

        # --- and it must never answer from the number it knows is wrong ----
        blind = wsl_machine(tmpdir, [], repo)
        r = run_env(blind, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect("WSL with no Windows volume is not a pass", r.returncode == 3, f"got {r.returncode}")
        expect(
            "and it announces that it could not measure",
            "SKIPPED (not a pass)" in r.stderr,
            r.stderr.strip()[:200],
        )

        gone = dict(blind)
        gone["DISK_HEADROOM_MOUNTINFO"] = str(tmpdir / "no-such-mountinfo")
        r = run_env(gone, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect("an unreadable mount table is not a pass", r.returncode == 3, f"got {r.returncode}")

        # --- the floor is pinned in both directions ------------------------
        declared = None
        for line in GUARD.read_text().splitlines():
            if line.startswith("DEFAULT_FLOOR_GB"):
                declared = int(line.split("=")[1].strip())
        expect(
            f"the floor is still {PINNED_FLOOR_GB} GB",
            declared == PINNED_FLOOR_GB,
            f"disk_headroom.py declares {declared}; change both or neither",
        )

        # And it is a floor, not a decoration: the verdict has to turn over at
        # it. Pinning the number without this would survive the guard being
        # rewritten to ignore it.
        r = run_env({}, "--repo", str(repo), "--floor-gb", "0", "--heartbeat", str(beat))
        expect("at a floor of 0 the verdict is a pass", r.returncode == 0, f"got {r.returncode}")
        r = run_env({}, "--repo", str(repo), "--floor-gb", "999999", "--heartbeat", str(beat))
        expect("at an unreachable floor the verdict is a failure", r.returncode == 2, f"got {r.returncode}")

    if cases < MINIMUM_CASES:  # FLOOR
        print(
            f"\n{cases} case(s) ran, floor is {MINIMUM_CASES}. The script stopped short,\n"
            "  and a test that stops testing reports success in the same words as one\n"
            "  that passed.",
            file=sys.stderr,
        )
        return 1

    if failures:
        print(f"\n{len(failures)} case(s) failed: {', '.join(failures)}", file=sys.stderr)
        return 1
    print(f"\ndisk_headroom: {cases} cases, all as intended")
    return 0


if __name__ == "__main__":
    sys.exit(main())
