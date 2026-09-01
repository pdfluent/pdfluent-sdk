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

# FLOOR: free gigabytes on the volume holding the repository >= 25 -- a cold
# Rust build of this workspace peaks around 15 GB of transient target/ output,
# and the 31-08 corruption happened with less headroom than that. One-way by
# design: free space rising is the outcome we want, not a drift to catch.
DEFAULT_FLOOR_GB = 25

HEARTBEAT_DEFAULT = Path.home() / "Library/Application Support/pdfluent/disk_headroom.json"
GB = 1024**3


def volume_free_bytes(path: Path) -> tuple[int, int]:
    usage = shutil.disk_usage(path)
    return usage.free, usage.total


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
    roots = [repo_root]
    worktrees = repo_root / ".worktrees"
    if worktrees.is_dir():
        roots.extend(p for p in worktrees.iterdir() if p.is_dir())
    for root in roots:
        target = root / "target"
        if target.is_dir():
            size = directory_size(target)
            if size:
                found.append((size, target))
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

    free, total = volume_free_bytes(repo_root)
    write_heartbeat(heartbeat, free, total)

    free_gb = free / GB
    print(f"Volume holding {repo_root}: {free_gb:.1f} GB free of {total / GB:.1f} GB")

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
        print("  No target/ directories found -- the space is going somewhere else.", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
