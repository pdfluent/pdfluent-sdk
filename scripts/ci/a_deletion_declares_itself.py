#!/usr/bin/env python3
"""A branch that deletes a file master has must say so in the commit that does it.

WHY (#297)

Three times on 01-09-2026 a stale branch would have silently reverted work on
master. The third was a pull request undoing files another had added twenty
minutes earlier, by the same author. Nothing conflicted, nothing was red,
GitHub reported MERGEABLE throughout.

That is not a gap in the merge machinery. A branch cut before some work exists
carries the ABSENCE of that work as content, and an absence never produces a
textual conflict -- so `MERGEABLE` is true precisely when it is least
informative. **A branch is dangerous in proportion to how long it has been open,
and that danger is invisible to every signal the interface shows.**

WHY A TRAILER AND NOT A FILE

Deliberate deletions are ordinary: #1610 removed three workflows on purpose,
#1617 three more. So the guard needs "explained" to be machine-checkable rather
than a comment somebody wrote in a description. A separate register would drift
-- the deletion moves between rebases and the entry does not follow it. A trailer
lives in the commit that performs the deletion, travels with it through every
rebase and cherry-pick, and cannot be true of a commit that no longer deletes
anything:

    Removes-deliberately: .github/workflows/avrt.yml

Both directions, because that is what makes a declaration worth reading:

  * a deletion with no trailer naming it FAILS -- the thing this exists for;
  * a trailer naming a path this branch does not delete FAILS too. An
    explanation that outlives its subject reads as current and quietly covers
    whatever next occupies that name.

Usage: a_deletion_declares_itself.py [--base github/master] [--head HEAD]
"""
from __future__ import annotations
import argparse, subprocess, sys


def git(*args: str) -> tuple[int, str]:
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    return r.returncode, r.stdout


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="github/master")
    ap.add_argument("--head", default="HEAD")
    args = ap.parse_args(argv[1:])

    rc, base_sha = git("merge-base", args.base, args.head)
    base_sha = base_sha.strip()
    if rc != 0 or not base_sha:
        # Never a silent pass. Without a base there is no question to answer, and
        # answering "no deletions" would be a verdict about a comparison that did
        # not happen.
        print(f"[deletions] FATAL: no merge base between {args.base} and "
              f"{args.head}. Fetch the base branch first; refusing to report a "
              "clean result for a comparison that never ran.", file=sys.stderr)
        return 2

    # If the base resolves to the head, there is no range and every branch looks
    # clean. That is not a pass, it is a comparison that did not happen -- and it
    # is exactly what `--base origin/master` produced on a push to master, where
    # the ref already points at the revision being pushed. The caller was fixed;
    # this refuses too, because the next caller will be written by somebody who
    # has not read that fix. (codex, #1635)
    head_sha = git("rev-parse", args.head)[1].strip()
    if base_sha == head_sha:
        print(f"[deletions] FATAL: the base resolves to {args.head} itself, so the "
              "range is empty and no deletion could be found. Pass the revision "
              "this work started from, not the one it produced.", file=sys.stderr)
        return 2

    _, out = git("diff", "--diff-filter=D", "--name-only", f"{base_sha}..{args.head}")
    deleted = {p for p in out.splitlines() if p.strip()}

    _, log = git("log", "--format=%H%x00%B%x00", f"{base_sha}..{args.head}")
    declared: dict[str, str] = {}
    for entry in log.split("\x00\x00"):
        if not entry.strip():
            continue
        sha, _, body = entry.partition("\x00")
        for line in body.splitlines():
            if line.lower().startswith("removes-deliberately:"):
                path = line.split(":", 1)[1].strip()
                if path:
                    declared[path] = sha.strip()[:8]

    undeclared = sorted(deleted - set(declared))
    stale = sorted(set(declared) - deleted)

    if undeclared:
        print(f"[deletions] FAIL: {len(undeclared)} file(s) present on "
              f"{args.base} are deleted here and nothing says why:", file=sys.stderr)
        for p in undeclared:
            print(f"    {p}", file=sys.stderr)
        print("\n  Add a trailer to the commit that removes it:\n"
              "      Removes-deliberately: <path>\n"
              "  A deletion nobody declared is how a branch that predates the work\n"
              "  silently reverts it -- no conflict, no red, MERGEABLE true.",
              file=sys.stderr)
    if stale:
        print(f"[deletions] FAIL: {len(stale)} trailer(s) name a path this branch "
              "does not delete:", file=sys.stderr)
        for p in stale:
            print(f"    {p}  (declared in {declared[p]})", file=sys.stderr)
        print("\n  Remove the trailer. An explanation that outlives its subject\n"
              "  reads as current and covers whatever next takes that name.",
              file=sys.stderr)
    if undeclared or stale:
        return 1

    if deleted:
        print(f"[deletions] OK: {len(deleted)} deletion(s), each declared by the "
              "commit that performs it.")
    else:
        print(f"[deletions] OK: this branch deletes nothing that {args.base} has.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
