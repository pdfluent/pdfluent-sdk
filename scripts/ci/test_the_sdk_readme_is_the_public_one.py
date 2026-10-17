#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The one-README guard bites on any difference, and stays quiet on none.

Every case copies the guard into a scratch tree at the same depth, writes a
README there and hands the guard a `--remote` file, so no case touches the
network. The real README is the input, because a guard tested only on toy pages
has not been tested on the page it guards.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
GUARD = HERE / "the_sdk_readme_is_the_public_one.py"
REAL_README = HERE.parent.parent / "README.md"

MINIMUM_CASES = 7  # FLOOR: fewer means a case went missing, not that all is well


def run(tmp: pathlib.Path, local: str | None, remote: str | None,
        remote_arg: bool = True) -> subprocess.CompletedProcess[str]:
    root = tmp / "r"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / "scripts" / "ci" / GUARD.name).write_text(GUARD.read_text(encoding="utf-8"),
                                                      encoding="utf-8")
    if local is not None:
        (root / "README.md").write_text(local, encoding="utf-8")
    cmd = [sys.executable, str(root / "scripts" / "ci" / GUARD.name)]
    if remote_arg:
        public = tmp / "public.md"
        if remote is not None:
            public.write_text(remote, encoding="utf-8")
        cmd += ["--remote", str(public)]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=60)


def main() -> int:
    if not GUARD.is_file() or not REAL_README.is_file():
        print(f"FATAL: {GUARD.name} or README.md is missing", file=sys.stderr)
        return 1
    page = REAL_README.read_text(encoding="utf-8")

    cases = [
        ("identical files pass", page, page, 0),
        ("one changed word fails", page, page.replace("Install", "Installation", 1), 1),
        ("a trailing newline fails", page, page + "\n", 1),
        ("a section only the public side has fails", page,
         page + "\n## Extra\n\nOnly on one side.\n", 1),
        ("two identical short files fail on the floor", "a\n" * 5, "a\n" * 5, 1),
        ("a missing local README fails", None, page, 1),
        ("a missing --remote file fails", page, None, 1),
    ]

    failures = 0
    for name, local, remote, want in cases:
        with tempfile.TemporaryDirectory() as tmp:
            got = run(pathlib.Path(tmp), local, remote).returncode
        ok = (got == 0) == (want == 0)
        failures += not ok
        print(f"  {'ok  ' if ok else 'FAIL'}  {name} (exit {got}, want {'0' if want == 0 else 'non-zero'})")

    if len(cases) < MINIMUM_CASES:
        print(f"FATAL: {len(cases)} case(s), below the floor of {MINIMUM_CASES}",
              file=sys.stderr)
        return 1
    print(f"{len(cases) - failures} passed, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
