#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The fork-notice guard has to bite, and to bite in the right place (#219).

The way #219 says to test it is to take the line out of one README and watch the
lint fall over. That is done here mechanically, on a copy of the tree, so the
proof runs on every pull request instead of once by hand.

The second case is the one that is easy to miss. LC7 is about what a reader sees
FIRST -- a fork line below the fold is the same as a line in NOTICE, which is
where it already was. A guard that accepts a mention anywhere in the file would
report the repository as compliant while changing nothing a reader sees.
"""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "every_fork_names_its_upstream.py"
CRATE = "crates/pdf-render/README.md"


def in_a_copy(edit) -> tuple[int, str]:
    """Run the guard over a copy of the tree with one README edited.

    A copy, because the guard rewrites READMEs with `--write` and a test that
    edits the working tree is a test that can lose somebody's work. Only the
    files the guard reads are copied: the crate READMEs, the provenance table
    it imports, and the guard itself.
    """
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "scripts" / "ci").mkdir(parents=True)
        for name in ("every_fork_names_its_upstream.py", "herkomsttabel.py"):
            shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
        for readme in REPO.glob("crates/*/README.md"):
            target = root / "crates" / readme.parent.name / "README.md"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(readme, target)

        edit(root / CRATE)
        done = subprocess.run(
            [sys.executable, str(root / "scripts/ci/every_fork_names_its_upstream.py")],
            capture_output=True, text=True, check=False,
            env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"),
        )
        return done.returncode, done.stdout + done.stderr


class EveryForkNamesItsUpstream(unittest.TestCase):
    def test_the_repository_as_it_stands_passes(self) -> None:
        done = subprocess.run([sys.executable, str(GUARD)],
                              capture_output=True, text=True, check=False)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)

    def test_removing_the_line_from_one_readme_turns_it_red(self) -> None:
        def strip(path: pathlib.Path) -> None:
            kept = [line for line in path.read_text().splitlines()
                    if not line.startswith(">")]
            path.write_text("\n".join(kept))

        code, output = in_a_copy(strip)
        self.assertEqual(code, 1, output)
        self.assertIn("pdf-render", output)

    def test_a_mention_below_the_fold_is_not_enough(self) -> None:
        """The whole of LC7 in one case.

        Moving the line to the bottom leaves the file saying it is a fork and
        leaves the reader none the wiser -- which is the state NOTICE was already
        in, and the reason this issue exists at all.
        """
        def move_to_the_bottom(path: pathlib.Path) -> None:
            lines = path.read_text().splitlines()
            quoted = [line for line in lines if line.startswith(">")]
            rest = [line for line in lines if not line.startswith(">")]
            path.write_text("\n".join(rest + [""] + quoted))

        code, output = in_a_copy(move_to_the_bottom)
        self.assertEqual(
            code, 1,
            "a fork line below the fold was accepted. That is the state NOTICE "
            f"was already in, and it changes nothing a reader sees.\n{output}",
        )


if __name__ == "__main__":
    unittest.main()
