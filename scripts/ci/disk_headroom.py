#!/usr/bin/env python3
"""Fail before the disk is full, and notice when the thing that checks stops running.

Two failures happened on 31-08-2026, hours apart, on two different machines.

On the Mac the data volume reached zero bytes free during a `cargo test`. A
mutation harness died between breaking a source file and restoring it, leaving a
deliberately broken parser on disk. Disk-full is not only an inconvenience; it
interrupts operations that are only safe as a pair.

On the Windows desktop `C:` reached 9.2 GB free of 222.6 GB, of which a single
WSL `ext4.vhdx` held 103.2 GB while reporting 81 GB used inside. A virtual disk
grows and never shrinks, so deleting files inside it reclaims nothing on the
host.

That second machine is the one this script could not see. Inside WSL the root
filesystem IS the sparse vhdx, and it reports the vhdx's MAXIMUM size, not what
the host can still give it. Measured on the build machine 31-08-2026, with no
job running:

    shutil.disk_usage("/")                    free = 876.1 GB   total = 1006.9 GB
    shutil.disk_usage("/var/cache/cargo-target")  free = 876.1 GB
    shutil.disk_usage("/mnt/c")               free =   9.5 GB   total =  222.6 GB

Nine and a half gigabytes of real headroom, reported as eight hundred and
seventy-six. A guard written to stop disk exhaustion would have passed by a
factor of thirty-five on the one host where the exhaustion happens, every time,
and gone on passing as the volume filled. `runner_disk_guard.sh` already knew
this trick; this file did not, and this file is the one wired into the pipeline
that blocks a merge.

So on WSL the question is not "how much does this filesystem claim" but "how
much can it still grow", and that is bounded by the Windows volume underneath.

There was already a cleanup agent for the Mac, `com.pdfluent.cleanup`. It had
not run for months: launchd has no Full Disk Access, so everything it touched
under ~/Documents failed, and it failed *quietly*. That is the part worth
designing against. A janitor that can die in silence is not a mechanism, it is a
memory of one -- so this script also records that it ran, and can be asked
whether those records have stopped arriving.

Exit codes:
  0  above the floor
  2  below the floor
  3  cannot measure, or the heartbeat is stale (announced, never silent)

FLOOR: the free-space floor is a TWO-WAY ratchet -- see DEFAULT_FLOOR_GB.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

# FLOOR: free gigabytes on the volume that can still grow.
#
# Expressed as the build it has to survive plus a margin, not as a bare number,
# because the bare number drifted away from the thing it was protecting and
# nobody could see it happen. The comment here used to say "a cold Rust build of
# this workspace peaks around 15 GB" against a floor of 25, which reads as ten
# gigabytes of headroom. Measured 01-09-2026: a full local gate run from a cold
# target (check, test --no-run, clippy, wasm, examples) leaves a **30 GB**
# target/ directory. So the floor was five gigabytes BELOW the build it was
# meant to permit, and the gate's verdict came out of how warm the previous run
# had left the cache -- it passed on a warm one and failed partway through a cold
# one, which is not a property of the change under test.
#
# Both numbers are pinned by test_disk_headroom.py, and so is the relationship:
# a floor at or below COLD_BUILD_GB is the defect this replaces, so the test
# refuses it rather than trusting whoever edits next to notice.
#
# TWO-WAY RATCHET, unchanged in spirit. Lowering it is the obvious danger -- a
# floor quietly dropped to 5 is a guard switched off while still reporting.
# Raising it is the less obvious one: a floor above what the machine can ever
# offer makes every lane red for a reason that has nothing to do with the change
# under test, and the usual repair for that is to stop running the guard. That is
# why the margin is small and the build cost is measured rather than padded.
COLD_BUILD_GB = 30
SAFETY_MARGIN_GB = 5
DEFAULT_FLOOR_GB = COLD_BUILD_GB + SAFETY_MARGIN_GB

HEARTBEAT_DEFAULT = Path.home() / "Library/Application Support/pdfluent/disk_headroom.json"
GB = 1024**3


# Test seams. Named, documented and defaulted to the real thing, so a case can
# build a WSL machine that does not exist rather than needing one. The same
# pattern as WNW_MOUNTINFO in why_not_writable.sh.
PROC_VERSION = Path(os.environ.get("DISK_HEADROOM_PROC_VERSION", "/proc/version"))
MOUNTINFO = Path(os.environ.get("DISK_HEADROOM_MOUNTINFO", "/proc/self/mountinfo"))

# Filesystem types that mean "this is the Windows volume, seen from inside the
# VM". Space reported on one of these is real: it is the host's own accounting.
HOST_VOLUME_FSTYPES = ("9p", "drvfs", "virtiofs", "cifs")

# ...but not every one of those is a candidate for "the volume the vhdx grows
# into". CIFS is a network share. On this runner /mnt/storagebox is a CIFS
# storage box with no relation to the Windows disk holding the distro image, so
# with /mnt/c unmounted the search below would have picked the storage box and
# reported its free space as the vhdx's headroom -- a full C: passing the guard
# on the strength of a remote share's spare room. (codex, #1616)
#
# It stays in HOST_VOLUME_FSTYPES because the check above it is a different
# question: if the repository itself lives on the share, then the share really
# is what bounds it and df is telling the truth. Being the carrier is evidence;
# merely being mounted is not.
NOT_A_BACKING_VOLUME = ("cifs",)


class CannotMeasure(Exception):
    """Refuse to answer rather than answer from the number known to be wrong."""


def running_under_wsl() -> bool:
    try:
        return any(m in PROC_VERSION.read_text().lower() for m in ("microsoft", "wsl"))
    except OSError:
        return False


def mounts() -> list[tuple[str, str]]:
    """(mount point, filesystem type) for every mount, longest point first.

    Pure text out of the kernel: this touches no filesystem, so it still answers
    while a device is failing every read. `ls` on the suspect path does not, and
    that is how a previous diagnosis joined the outage it was diagnosing (#264).
    """
    rows: list[tuple[str, str]] = []
    for line in MOUNTINFO.read_text().splitlines():
        fields = line.split()
        if "-" not in fields:
            continue
        sep = fields.index("-")
        if sep + 1 >= len(fields) or sep < 5:
            continue
        rows.append((fields[4], fields[sep + 1]))
    rows.sort(key=lambda r: len(r[0]), reverse=True)
    return rows


def carrying_mount(path: Path, rows: list[tuple[str, str]]) -> tuple[str, str] | None:
    target = str(path)
    for point, fstype in rows:
        if target == point or target.startswith(point.rstrip("/") + "/"):
            return point, fstype
    return None


def host_volume_for(path: Path) -> Path:
    """The Windows volume that bounds how far `path` can still grow.

    Raises CannotMeasure rather than falling back to the vhdx's own figure. The
    fallback is what makes this class of guard useless: it reports the number it
    knows to be wrong, in the words of a pass.
    """
    try:
        rows = mounts()
    except OSError as exc:
        raise CannotMeasure(f"{MOUNTINFO} is unreadable ({exc}), so the host volume is unknown") from exc

    carrier = carrying_mount(path, rows)
    if carrier and carrier[1] in HOST_VOLUME_FSTYPES:
        # Already on a host volume -- df is telling the truth here.
        return Path(carrier[0])

    host_mounts = [
        Path(point) for point, fstype in rows
        if fstype in HOST_VOLUME_FSTYPES and fstype not in NOT_A_BACKING_VOLUME
    ]
    if not host_mounts:
        genegeerd = sorted(
            {point for point, fstype in rows if fstype in NOT_A_BACKING_VOLUME}
        )
        extra = (
            f" ({', '.join(genegeerd)} is a network share, not the disk the image grows into)"
            if genegeerd else ""
        )
        raise CannotMeasure(
            "WSL is running but no Windows volume is mounted, so how much the root "
            f"filesystem can still grow cannot be determined{extra}"
        )
    # C: by preference: WSL keeps its distro vhdx on the system drive unless it
    # was deliberately moved, and that is the volume the vhdx grows into.
    for candidate in host_mounts:
        if candidate.name.lower() == "c":
            return candidate
    return sorted(host_mounts, key=lambda p: str(p))[0]


def volume_free_bytes(path: Path) -> tuple[int, int, str]:
    """Free and total bytes for `path`, plus what was actually measured.

    On anything but WSL that is the filesystem holding the path. On WSL the root
    filesystem is a sparse vhdx that reports its maximum size, so the binding
    constraint is the smaller of two numbers: what the Windows volume can still
    hand over, and what the filesystem itself has left. Taking the minimum is
    deliberately conservative -- the vhdx also holds space it has already claimed
    from the host and not yet used, which this ignores. Under-reporting costs a
    cleanup that was not strictly needed; over-reporting is what let the volume
    reach 96% with every check green.
    """
    own = shutil.disk_usage(path)
    if not running_under_wsl():
        return own.free, own.total, str(path)

    host = host_volume_for(path)
    if host == Path(str(path)) or str(path).startswith(str(host).rstrip("/") + "/"):
        host_usage = shutil.disk_usage(host)
        return host_usage.free, host_usage.total, str(host)

    host_usage = shutil.disk_usage(host)
    if host_usage.free <= own.free:
        return host_usage.free, host_usage.total, f"{host} (Windows volume under the WSL vhdx holding {path})"
    return own.free, own.total, str(path)


def directory_size(path: Path) -> int:
    """du -s, tolerating races and permission errors rather than dying on them."""
    try:
        out = subprocess.run(
            ["du", "-sk", str(path)],
            capture_output=True,
            text=True,
            timeout=180,
            check=False,
        )
        if out.returncode == 0 or out.stdout.strip():
            return int(out.stdout.split()[0]) * 1024
    except (subprocess.TimeoutExpired, ValueError, IndexError, OSError):
        pass
    return 0


def reclaimable(repo_root: Path) -> list[tuple[int, Path]]:
    """Build output that costs only rebuild time to delete.

    target/ is gitignored everywhere in this workspace, so every byte here is a
    cache. Worktrees are the pathology: each one carries its own, and nothing
    ever removes them.
    """
    found: list[tuple[int, Path]] = []
    seen: set[Path] = set()

    def note(path: Path) -> None:
        resolved = path.resolve()
        if resolved in seen or not path.is_dir():
            return
        seen.add(resolved)
        size = directory_size(path)
        if size:
            found.append((size, path))

    roots = [repo_root]
    worktrees = repo_root / ".worktrees"
    if worktrees.is_dir():
        roots.extend(p for p in worktrees.iterdir() if p.is_dir())
    for root in roots:
        note(root / "target")

    # The shared build directory, which on the CI host is where the space
    # actually goes and is nowhere near the checkout. Reporting only the
    # checkout's own target/ is how this guard first failed on the runner: it
    # said "no target/ directories found -- the space is going somewhere else"
    # while 23 GB sat in CARGO_TARGET_DIR two levels up from /.
    shared = os.environ.get("CARGO_TARGET_DIR")
    if shared:
        note(Path(shared))
        # Incremental state is listed separately because it is the cheapest
        # thing on the list to lose: cargo rebuilds it, nothing depends on it
        # between jobs, and CI has no use for it at all. Measured 31-08-2026
        # across four worktrees on the Mac: 23 GB, in a checkout that had 118 MB
        # of disk left.
        for profile in ("debug", "release"):
            note(Path(shared) / profile / "incremental")

    found.sort(reverse=True)
    return found


def write_heartbeat(path: Path, free: int, total: int) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(
                {
                    "checked_at": int(time.time()),
                    "free_bytes": free,
                    "total_bytes": total,
                    "host": os.uname().nodename if hasattr(os, "uname") else "unknown",
                },
                indent=2,
            )
            + "\n"
        )
    except OSError as exc:
        # Writing the record must not be the thing that fails the check, but a
        # silent failure here is how the last janitor died. Say so.
        print(f"SKIPPED (not a pass): could not write the heartbeat to {path}: {exc}", file=sys.stderr)


def check_heartbeat(path: Path, max_age_hours: float) -> int:
    if not path.exists():
        print(
            f"SKIPPED (not a pass): no disk-headroom heartbeat at {path}.\n"
            "  Nothing has recorded a disk check on this machine. That is the state\n"
            "  com.pdfluent.cleanup was in for months while appearing to be installed.",
            file=sys.stderr,
        )
        return 3
    try:
        data = json.loads(path.read_text())
        age_hours = (time.time() - float(data["checked_at"])) / 3600.0
    except (OSError, ValueError, KeyError) as exc:
        print(f"SKIPPED (not a pass): heartbeat at {path} is unreadable: {exc}", file=sys.stderr)
        return 3
    if age_hours > max_age_hours:
        print(
            f"The disk check has not run for {age_hours:.1f} hours "
            f"(allowed: {max_age_hours:.0f}).\n"
            "  The scheduler is installed and not running. On macOS the usual cause is\n"
            "  that launchd has no Full Disk Access, so anything under ~/Documents fails\n"
            "  without a word -- move the agent under ~/Library/Application Support.",
            file=sys.stderr,
        )
        return 2
    print(f"✓ disk check last ran {age_hours:.1f} hours ago")
    return 0


def elders_gemeten(measured, repo_root) -> bool:
    """Was the volume we measured a DIFFERENT place from the repository?

    A named function because the decision could not otherwise be tested. Through
    the CLI the note needs both this AND `inner.free > free`, and on any real
    machine a fixture's "host volume" is a directory on the same filesystem --
    so the second condition is False and covers for the first however the first
    is written. Two earlier attempts at a test proved nothing: an `or` chain
    that could not fail, and an assertion about Python's own `str != Path`
    rather than about this guard. (peer review, #1664)

    `measured` arrives as a str and `repo_root` as a Path, so comparing them
    directly was always True -- the explanation would have printed on a Mac,
    where there is no sparse image and the sentence is nonsense.
    """
    try:
        return Path(str(measured)).resolve() != Path(repo_root).resolve()
    except OSError:
        return False


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=None, help="repository root (default: this script's repository)")
    ap.add_argument("--floor-gb", type=float, default=DEFAULT_FLOOR_GB)
    ap.add_argument("--heartbeat", default=str(HEARTBEAT_DEFAULT))
    ap.add_argument("--check-heartbeat", action="store_true", help="only verify that the check is still running")
    ap.add_argument("--max-age-hours", type=float, default=36.0)
    args = ap.parse_args()

    heartbeat = Path(args.heartbeat).expanduser()
    if args.check_heartbeat:
        return check_heartbeat(heartbeat, args.max_age_hours)

    repo_root = Path(args.repo).expanduser() if args.repo else Path(__file__).resolve().parents[2]
    if not repo_root.is_dir():
        print(f"SKIPPED (not a pass): {repo_root} is not a directory", file=sys.stderr)
        return 3

    try:
        free, total, measured = volume_free_bytes(repo_root)
    except CannotMeasure as exc:
        print(f"SKIPPED (not a pass): {exc}", file=sys.stderr)
        return 3
    write_heartbeat(heartbeat, free, total)

    free_gb = free / GB
    # The measured path is printed on its own line and on every run, pass or
    # fail. A guard that does not say what it looked at cannot be caught looking
    # at the wrong thing, which is exactly how this one reported 876 GB free on a
    # machine with 9.5 GB.
    print(f"measured: {measured}")
    print(f"Volume holding {repo_root}: {free_gb:.1f} GB free of {total / GB:.1f} GB")

    # What the INNER filesystem claims, said out loud and never used to decide.
    #
    # On 02-09-2026 the desktop measurement showed ext4 reporting 875 GB free
    # while the host volume had 9.2 GB, and the honest reading of that pair --
    # writable space is min(inner, host) -- was nearly lost twice in one
    # morning: once when I summarised it as "no shortage inside the image", and
    # once when that summary was taken as a reason to make the inner number the
    # criterion. Both times the docstring above already had the answer.
    #
    # So the pair is printed together. A reader who sees only one of the two
    # numbers reaches for the wrong one, and this guard exists because somebody
    # did. It stays informational: the verdict below is unchanged.
    if elders_gemeten(measured, repo_root):
        try:
            inner = shutil.disk_usage(repo_root)
        except OSError:
            inner = None
        if inner is not None and inner.free > free:
            print(f"  (the filesystem at {repo_root} reports "
                  f"{inner.free / GB:.1f} GB free, which is the maximum size of "
                  f"a sparse image and NOT what the host can give it. Writable "
                  f"space is the smaller of the two: {free_gb:.1f} GB. Deleting "
                  f"inside frees blocks in the image and returns nothing to the "
                  f"host until it is compacted -- which is why deleting first is "
                  f"the precondition for compaction, not an alternative to it.)")

    if free_gb >= args.floor_gb:
        print(f"✓ disk_headroom: above the floor of {args.floor_gb:.0f} GB")
        return 0

    print(f"\nBelow the floor: {free_gb:.1f} GB free, floor is {args.floor_gb:.0f} GB.\n", file=sys.stderr)
    items = reclaimable(repo_root)
    if items:
        print("Build output that costs only rebuild time to delete:", file=sys.stderr)
        for size, path in items[:10]:
            print(f"  {size / GB:6.1f} GB  {path}", file=sys.stderr)
        print(f"\n  Reclaimable in total: {sum(s for s, _ in items) / GB:.1f} GB", file=sys.stderr)
    else:
        print(
            "  No target/ directories found -- the space is going somewhere else.\n"
            "  On the CI host, check CARGO_TARGET_DIR and the runner work directories.",
            file=sys.stderr,
        )
    return 2


if __name__ == "__main__":
    sys.exit(main())
