#!/usr/bin/env python3
"""Reclaim target/ from worktrees whose work is already on master.

Three terminals each did this by hand on 01-09-2026 and together freed about
50 GB. Doing it by hand is why the disk-headroom floor kept being reached from
below: the space existed, nobody was told, and the gate could only report.

WHAT IS SAFE TO DELETE, and why the test is ancestry rather than age

A worktree's target/ is a cache: gitignored, rebuildable, and worth keeping only
while somebody is still building there. The question is not "is it old" -- an old
worktree may hold the only copy of work in progress -- but "is its work already
somewhere else". Two conditions, both required:

  * HEAD is an ancestor of github/master, so every commit in it is on master;
  * the worktree is clean, so nothing uncommitted would be lost.

A worktree failing either is left alone and said out loud. Deleting a cache from
under a colleague costs them an hour; leaving one costs disk that this reports.

A PATH TEST FOR "SOMEBODY ELSE'S DIRECTORY" WAS TRIED AND WITHDRAWN

The first version refused to sweep worktrees outside the repository's own
directory, on the reasoning that a merged, clean worktree can still be another
session's workspace and deleting it costs them a rebuild. Measured on 01-09: it
marked all three reclaimable worktrees, because in this workspace every worktree
lives under /tmp by convention. A safety test that blocks the entire case it was
written for is worse than none -- it makes the tool useless while looking
careful. Path is not evidence of ownership. What is left is the pair that holds:
merged means no work is lost, clean means nothing uncommitted is, and the cargo
check below means no build is interrupted.

WHAT IT WILL NOT DO

It never deletes a target/ that a build is using. `cargo` anywhere on the host
means nothing is swept -- the same rule cargo_target_health.sh applies, and for
the same reason: a file removed while rustc is mid-write reproduces the 25-08
corruption this workspace already paid for once.

Reporting by default; deleting takes --sweep. A tool that frees 30 GB the moment
you run it by accident is not a tool anybody runs.
"""
from __future__ import annotations
import argparse, shutil, subprocess, sys
from pathlib import Path

GB = 1024**3


def run(*args: str) -> str:
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""


def cargo_running() -> bool:
    r = subprocess.run(["ps", "-Ao", "args="], capture_output=True, text=True)
    for line in r.stdout.splitlines():
        head = line.split(" ", 1)[0].rsplit("/", 1)[-1]
        if head in ("cargo", "rustc", "sccache"):
            return True
    return False


def dir_size(path: Path) -> int:
    total = 0
    for p in path.rglob("*"):
        try:
            if p.is_file() and not p.is_symlink():
                total += p.stat().st_size
        except OSError:
            pass
    return total


def worktrees() -> list[Path]:
    out, paths = run("worktree", "list", "--porcelain"), []
    for line in out.splitlines():
        if line.startswith("worktree "):
            paths.append(Path(line.split(" ", 1)[1]))
    return paths


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sweep", action="store_true", help="delete; without it, report only")
    ap.add_argument("--base", default="github/master")
    args = ap.parse_args(argv[1:])

    repo_root = str(Path(run("rev-parse", "--show-toplevel").strip() or ".").resolve())
    trees = worktrees()
    if not trees:
        print("[sweep] FATAL: git reported no worktrees. Refusing to report a "
              "clean result for a scan that found nothing to look at.", file=sys.stderr)
        return 2

    freeable, kept = [], []
    for wt in trees:
        target = wt / "target"
        if not target.is_dir():
            continue
        head = run("-C", str(wt), "rev-parse", "HEAD").strip()
        if not head:
            kept.append((wt, "not a readable worktree")); continue
        merged = subprocess.run(
            ["git", "-C", str(wt), "merge-base", "--is-ancestor", head, args.base],
            capture_output=True).returncode == 0
        dirty = bool(run("-C", str(wt), "status", "--porcelain").strip())
        if not merged:
            kept.append((wt, "HEAD is not on " + args.base))
        elif dirty:
            kept.append((wt, "uncommitted changes"))
        else:
            freeable.append((dir_size(target), target))

    for wt, why in kept:
        print(f"[sweep] keeping {wt}/target -- {why}")
    total = sum(s for s, _ in freeable)
    if not freeable:
        print("[sweep] nothing reclaimable: every worktree is either unmerged or dirty.")
        return 0

    for size, target in sorted(freeable, reverse=True):
        print(f"[sweep] {size/GB:5.1f} GB  {target}")
    print(f"[sweep] {total/GB:.1f} GB reclaimable across {len(freeable)} worktree(s)")

    if not args.sweep:
        print("[sweep] reporting only; pass --sweep to delete.")
        return 0
    if cargo_running():
        print("[sweep] a build is running on this host -- nothing removed. "
              "Deleting a target/ from under rustc is the 25-08 corruption.", file=sys.stderr)
        return 0
    for size, target in freeable:
        shutil.rmtree(target, ignore_errors=True)
        print(f"[sweep] removed {target}")
    free = shutil.disk_usage(trees[0]).free
    print(f"[sweep] {total/GB:.1f} GB freed; {free/GB:.1f} GB now free")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
