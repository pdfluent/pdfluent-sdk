#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The "does this already exist" question has to be answered, not merely asked.

The tool it tests (#237) reports "nothing open touches this" in three different
situations: nothing is open, nothing matches, and it could not read anything.
The first two are answers; the third is the absence of one, and the whole point
of the tool is that the answer can be trusted before half a day is spent.

So this pins the three apart, over a fake `gh` rather than the real API: a test
that needs the network answers a question about the network.
"""
from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import textwrap
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
TOOL = REPO / "scripts" / "ci" / "does_this_already_exist.py"

PULLS = [{"number": 44, "title": "binary_mark and indirect_object now have tests"}]
FILES = [
    {"filename": "crates/lopdf/src/writer.rs", "patch": "+fn binary_mark() {}"},
    {"filename": "docs/UPSTREAM_FORKS.toml", "patch": "+forkpunt = 'abc'"},
]


def fake_gh(tmp: pathlib.Path, pulls: object, files: object, exit_code: int = 0) -> pathlib.Path:
    """A `gh` on PATH that answers from fixtures instead of from GitHub."""
    binary = tmp / "gh"
    binary.write_text(textwrap.dedent(f"""\
        #!{sys.executable}
        import json, sys
        if {exit_code} != 0:
            print("HTTP 403: API rate limit exceeded", file=sys.stderr)
            sys.exit({exit_code})
        path = sys.argv[-1]
        print(json.dumps({files!r} if "/files" in path else {pulls!r}))
        """), encoding="utf-8")
    binary.chmod(0o755)
    return binary


def run(tmp: pathlib.Path, *args: str) -> tuple[int, str]:
    env = dict(os.environ, PATH=f"{tmp}:{os.environ['PATH']}")
    done = subprocess.run([sys.executable, str(TOOL), *args],
                          capture_output=True, text=True, env=env, check=False)
    return done.returncode, done.stdout + done.stderr


class DoesThisAlreadyExist(unittest.TestCase):
    def test_a_symbol_in_an_open_pull_request_is_found(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = pathlib.Path(td)
            fake_gh(tmp, PULLS, FILES)
            code, out = run(tmp, "--symbol", "binary_mark")
        self.assertEqual(code, 0, out)
        self.assertIn("#44", out)
        self.assertIn("crates/lopdf/src/writer.rs", out)

    def test_a_symbol_nobody_touches_is_reported_as_free(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = pathlib.Path(td)
            fake_gh(tmp, PULLS, FILES)
            code, out = run(tmp, "--symbol", "a_name_nobody_has_written")
        self.assertEqual(code, 0, out)
        self.assertIn("You can start", out)

    def test_a_path_is_matched_in_both_directions(self) -> None:
        """Asking about a directory finds a file in it, and the other way round."""
        with tempfile.TemporaryDirectory() as td:
            tmp = pathlib.Path(td)
            fake_gh(tmp, PULLS, FILES)
            code, out = run(tmp, "crates/lopdf")
        self.assertEqual(code, 0, out)
        self.assertIn("#44", out)

    def test_no_access_says_so_instead_of_saying_nothing_is_open(self) -> None:
        """The case the whole tool rests on.

        A silent "nothing found" when the API could not be reached is worse than
        no tool at all: it is the same sentence as the trustworthy answer, and it
        is the sentence somebody acts on by spending the day rebuilding what
        already exists.
        """
        with tempfile.TemporaryDirectory() as td:
            tmp = pathlib.Path(td)
            fake_gh(tmp, PULLS, FILES, exit_code=1)
            code, out = run(tmp, "--symbol", "binary_mark")
        self.assertEqual(code, 0, out)
        self.assertIn("SKIPPED (not a pass)", out)
        self.assertNotIn("You can start", out)


if __name__ == "__main__":
    unittest.main()
