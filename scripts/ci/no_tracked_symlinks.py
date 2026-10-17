#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No tracked file may be a symlink, except the ones listed below (#636).

WHAT BREAKS WITHOUT THIS

Git for Windows defaults to `core.symlinks=false`, so a checkout there writes
every tracked symlink as a small text file holding the link target instead of
the file it points at. `fixtures/sample.pdf` was such a link, and the
node-bindings smoke run on windows-latest read 31 bytes of target path where
it expected a PDF: 63 of 122 jest tests failed in `beforeAll` with
`invalid PDF: Invalid`. The same held for the other three fixture links. That
is not our runners' quirk alone -- any customer cloning on Windows gets the
same text files.

So: every mode-120000 entry in the index is a failure, except the allow-list
below. A fixture or source file that two paths must share is copied, not
linked; bytes are cheap and a checkout that silently degrades is not.

THE ALLOW-LIST

`.githooks/commit-msg` stays a link to `../scripts/git-hooks/commit-msg`. It is
not needed as one -- git would run a regular executable file there just the
same, and the link exists only so the hook has a single source. It is allowed
because hooks never run where the Windows rendering bites: a fresh clone has
`core.hooksPath` unset, so every hook in this repository is inert until
someone runs the installer, and no CI runner -- Windows or otherwise -- ever
does. Replacing it with a copy is a one-line change the day anyone wants zero
tracked links; the guard stays green either way.

Usage:
    python3 scripts/ci/no_tracked_symlinks.py [--root PATH]

Exit codes:
  0  no tracked symlink outside the allow-list
  1  a tracked symlink outside the allow-list is present
  2  the index could not be read (announced, never silent)
"""
from __future__ import annotations

import argparse
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]


def _clean_env() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    Inside a git hook GIT_DIR and GIT_WORK_TREE point at the real repository,
    and git then ignores the directory it is pointed at -- so the guard would
    read the wrong index. Same helper as mr_staleness.py's schone_omgeving.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

# Path -> the reason it may stay a link. Free text would approve every reason,
# including "later": each entry names why the Windows text-file rendering can
# never reach a run through it.
ALLOWED: dict[str, str] = {
    ".githooks/commit-msg": (
        "a git hook, inert in every fresh clone (core.hooksPath unset) and "
        "never installed on a CI runner, so no Windows checkout executes it"
    ),
}


def tracked_symlinks(root: pathlib.Path) -> list[str]:
    """Every mode-120000 path in the index, as index-relative strings."""
    proc = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-s", "-z"],
        env=_clean_env(),
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git ls-files failed: {proc.stderr.strip()}")
    links: list[str] = []
    entries = 0
    for record in proc.stdout.split("\0"):
        if not record:
            continue
        entries += 1
        # `100644 <sha> 0\t<path>`; -z keeps even odd paths on one record.
        head, path = record.split("\t", 1)
        mode = head.split(" ", 1)[0]
        if mode == "120000":
            links.append(path)
    if entries == 0:
        raise RuntimeError("the index lists no files at all; refusing to call an empty scan clean")
    return links


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(REPO))
    args = ap.parse_args(argv[1:])
    root = pathlib.Path(args.root)
    try:
        links = tracked_symlinks(root)
    except RuntimeError as e:
        print(f"[symlinks] FATAL: {e}.", file=sys.stderr)
        return 2
    offenders = [p for p in links if p not in ALLOWED]
    if offenders:
        print(
            f"[symlinks] FAIL: {len(offenders)} tracked symlink(s) outside the allow-list:",
            file=sys.stderr,
        )
        for p in sorted(offenders):
            print(f"    {p}", file=sys.stderr)
        print(
            "\n  A Windows checkout (core.symlinks=false) writes each of these as a\n"
            "  text file holding the link target. Replace the link with a regular\n"
            "  file carrying the target's bytes: git rm the link, copy, git add.",
            file=sys.stderr,
        )
        return 1
    print(
        f"[symlinks] OK: no tracked symlink outside the allow-list "
        f"({len(ALLOWED)} allowed)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
