#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every byte-compared golden dir checks out LF on every OS.

Build 6 reddened the Windows natives leg: the golden files under
crates/pdf-engine/tests/golden/ checked out with CRLF (Git for Windows
autocrlf) while the serializers produce LF, so all three byte comparisons
failed ("left: {\\n ... right: {\\r\\n"). The tests must keep comparing
byte-for-byte -- stripping CR there would hide a real CRLF bug in our own
output -- so the fix is in .gitattributes: eol=lf (or -text) on every
directory a test treats as golden.

This guard holds that: for every dir in GOLDEN_DIRS, each with the test
that compares its files byte-for-byte, every committed file must carry
eol=lf or -text, read off `git check-attr`. A new crates/*/tests/golden
dir fails here until it is listed deliberately, with its test.
"""
from __future__ import annotations

import glob
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

# Golden dir -> the test that compares its files byte-for-byte.
GOLDEN_DIRS = {
    "crates/pdf-engine/tests/golden":
        "crates/pdf-engine/tests/serializers_over_five_documents.rs",
}


def _git_env() -> dict[str, str]:
    """The environment for git calls, without the caller's GIT_* variables.

    Inside a git hook GIT_DIR and GIT_WORK_TREE point at the hook's repository,
    so a plain `git -C` would read that one instead of REPO.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def _git(*args: str) -> str:
    out = subprocess.run(
        ["git", "-C", str(REPO), *args],
        capture_output=True, text=True, check=False, env=_git_env(),
    )
    if out.returncode != 0:
        raise SystemExit(f"golden_dirs_check_out_lf: git {' '.join(args)} failed: "
                         f"{out.stderr.strip()[-300:]}")
    return out.stdout


def main() -> int:
    failures: list[str] = []
    listed = set(GOLDEN_DIRS)
    for path in sorted(glob.glob(str(REPO / "crates" / "*" / "tests" / "golden"))):
        rel = str(pathlib.Path(path).relative_to(REPO))
        if rel not in listed:
            failures.append(f"{rel}: a golden dir no test is listed for -- "
                            f"add it to GOLDEN_DIRS with its byte-comparing test, "
                            f"or move it")
    for rel, test in sorted(GOLDEN_DIRS.items()):
        if not (REPO / rel).is_dir():
            failures.append(f"{rel}: not a directory (listed for {test})")
            continue
        files = [line for line in _git("ls-files", rel).splitlines() if line]
        if not files:
            failures.append(f"{rel}: no committed files (listed for {test})")
            continue
        attrs: dict[str, dict[str, str]] = {}
        for line in _git("check-attr", "eol", "text", "--", *files).splitlines():
            head, _, value = line.rpartition(": ")
            path, _, attr = head.rpartition(": ")
            if attr in ("eol", "text"):
                attrs.setdefault(path, {})[attr] = value
        for name in files:
            seen = attrs.get(name, {})
            # Quoted when the name needs it; the attribute pair is the same.
            seen = seen or attrs.get(f'"{name}"', {})
            if seen.get("eol") == "lf" or seen.get("text") == "unset":
                continue
            failures.append(f"{name}: eol={seen.get('eol', '?')} "
                            f"text={seen.get('text', '?')} -- want eol=lf "
                            f"or -text in .gitattributes (byte-compared by {test})")
    if failures:
        print("FAIL: " + "; ".join(failures))
        return 1
    total = sum(1 for _ in _git("ls-files", *sorted(GOLDEN_DIRS)).splitlines())
    print(f"OK: {len(GOLDEN_DIRS)} golden dir(s), {total} committed file(s), "
          f"all eol=lf or -text")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
