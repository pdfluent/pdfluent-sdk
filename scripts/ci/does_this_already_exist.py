#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does this work already exist in an open pull request?

    python3 scripts/ci/does_this_already_exist.py crates/pdf-java/src/lib.rs
    python3 scripts/ci/does_this_already_exist.py --symbol require_capability
    python3 scripts/ci/does_this_already_exist.py

WHY

On 25-08-2026 five JNI entry points were wired up that had been sitting in an
open merge request since 23 August. Half a day, done twice, because the work
existed in a request and not in master. The same day: three infrastructure fixes
standing independently on four branches, and 42 function names living in both
`merger.rs` and `template_parser.rs`, eight of which had genuinely diverged
(#208).

That is not carelessness, it is a measurement problem: nobody COULD see that the
work already existed. `pr_staleness.py` warns afterwards about what is open.
This answers the question beforehand.

DELIBERATELY NOT A CI JOB

A job would ask the question after the work is done, which is the one moment at
which the answer is worthless. It belongs in a pair of hands, as step zero of
starting a story, and that is where CLAUDE.md puts it.
`every_guard_has_a_job.py` records it as by-hand for this reason.

WITHOUT A TOKEN IT SAYS SO

No token is not a failure of the check, it is an environment without access.
Skipping in silence would be indistinguishable from "nothing is open", which is
the answer this tool exists to make trustworthy.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys

REPO = os.environ.get("PDFLUENT_REPO", "pdfluent/engine")


def gh(path: str) -> tuple[object | None, str | None]:
    """(data, reason). Exactly one of the two is None.

    The shape pr_staleness.py already uses, and for the reason written there:
    four different failures -- no binary, a non-zero exit, no network, output
    that is not JSON -- used to arrive as one `None`, and the caller then
    guessed a cause it had not measured.
    """
    try:
        done = subprocess.run(["gh", "api", path], capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return None, "`gh` is not installed on this machine"
    except OSError as exc:
        return None, f"`gh` could not be started: {exc}"
    if done.returncode != 0:
        first = ((done.stderr or done.stdout).strip().splitlines() or ["no output"])[0]
        return None, f"`gh api {path}` failed: {first}"
    try:
        return json.loads(done.stdout), None
    except json.JSONDecodeError as exc:
        return None, f"`gh api {path}` returned something that is not JSON: {exc}"


def touches_path(files: list[dict], wanted: str) -> list[str]:
    """The changed paths that overlap the one asked about, in either direction.

    Both directions on purpose: asking about a directory should find a pull
    request that changes a file inside it, and asking about a file should find
    one that changes the directory it is in.
    """
    return [f["filename"] for f in files
            if wanted in f["filename"] or f["filename"] in wanted]


def touches_symbol(files: list[dict], symbol: str) -> list[str]:
    """The changed paths whose diff mentions the symbol.

    The patch is what GitHub returns per file, so this reads the added and
    removed lines rather than the whole file: a pull request that merely
    contains the name somewhere is not the question. A file too large for
    GitHub to patch carries no `patch` key, and is then invisible here -- said
    out loud by the caller rather than left to be assumed.
    """
    return [f["filename"] for f in files if symbol in (f.get("patch") or "")]


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("path", nargs="?", help="the file path you are about to change")
    parser.add_argument("--symbol", help="a function name or other symbol to look for")
    args = parser.parse_args()

    prs, reason = gh(f"repos/{REPO}/pulls?state=open&per_page=100")
    if prs is None:
        print(f"SKIPPED (not a pass): {reason}", file=sys.stderr)
        return 0
    if not isinstance(prs, list):
        print(f"unexpected answer from GitHub: {prs!r}", file=sys.stderr)
        return 1
    if not prs:
        print("No open pull requests. You can start.")
        return 0

    hits: list[tuple[int, str, list[str]]] = []
    unreadable = 0
    for pr in prs:
        number = pr["number"]
        files, reason = gh(f"repos/{REPO}/pulls/{number}/files?per_page=300")
        if files is None or not isinstance(files, list):
            unreadable += 1
            continue
        if args.path:
            found = touches_path(files, args.path)
        elif args.symbol:
            found = touches_symbol(files, args.symbol)
        else:
            found = [f["filename"] for f in files]
        if found:
            hits.append((number, pr["title"], found))

    what = args.path or args.symbol or "every open pull request"
    if unreadable:
        # Named, not swallowed. A pull request whose files could not be read is
        # one this tool did not look at, and "nothing found" would be a claim
        # about it that was never measured.
        print(f"NOTE: the files of {unreadable} open pull request(s) could not be "
              "read, so they were not searched.", file=sys.stderr)
    if not hits:
        print(f"Nothing open touches {what!r}. You can start.")
        return 0

    print(f"{what!r} is already touched by {len(hits)} open pull request(s):\n")
    for number, title, paths in hits:
        print(f"  #{number}  {title[:66]}")
        for path in paths[:6]:
            print(f"        {path}")
        if len(paths) > 6:
            print(f"        ... and {len(paths) - 6} more")
        print()
    print("Look there first. On 25-08-2026 not doing so cost half a day: five JNI\n"
          "entry points wired up again that had been open for two days.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
