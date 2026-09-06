#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The divergence measurement has to say when it did not measure (LC5, #217).

The number this produces decides whether a crate goes back to upstream or stays
a declared fork, so the ways it can be wrong matter more than the ways it can be
right:

  * measured against a guessed fork point -- exactly what #262 had to undo, 90
    commits' worth for pdf-syntax, always in the flattering direction;
  * measured against nothing, and reported as if it had been.

Both of those come out as an ordinary-looking table. So this pins the three
outcomes apart: no clone says so, a crate without an established fork point is
listed as not measured, and a clone that carries almost nothing fails rather
than reporting a small table.

The measurement itself is checked on a real answer -- a file we changed against
a file upstream has -- over a two-commit repository built here, so the test needs
no network and no 900 MB clone.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "fork_divergence.py"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402


def run(**env_extra) -> tuple[int, str]:
    env = dict(os.environ, **env_extra)
    done = subprocess.run([sys.executable, str(GUARD), "--check"],
                          capture_output=True, text=True, env=env, check=False)
    return done.returncode, done.stdout + done.stderr


class ForkDivergence(unittest.TestCase):
    def test_without_a_clone_it_says_so(self) -> None:
        """No clone is not "no divergence"; it is no measurement."""
        env = {k: v for k, v in os.environ.items() if k != "HAYRO_CLONE"}
        done = subprocess.run([sys.executable, str(GUARD)],
                              capture_output=True, text=True, env=env, check=False)
        output = done.stdout + done.stderr
        self.assertIn("SKIPPED (not a pass)", output)
        # And it writes nothing: a run that measured nothing must not replace a
        # table that was measured.
        self.assertNotIn("measured ->", output)

    def test_a_clone_without_the_fork_points_fails(self) -> None:
        """An empty repository is a broken clone, not a fork that vanished.

        Without the floor this is the shape that hurts: every crate lands in
        "not measured", the table comes out with no rows, and a document saying
        nothing looks exactly like a document saying nothing is wrong.
        """
        with tempfile.TemporaryDirectory() as td:
            clone = pathlib.Path(td)
            # `sealed_env()`: without it this `init` inherits the caller's
            # GIT_DIR, and inside a hook that points at the real repository. A
            # fixture set `core.bare = true` on it that way on 25-08-2026 and
            # every worktree stopped working.
            subprocess.run(["git", "init", "-q", str(clone)], check=True,
                           env=sealed_env(cwd=clone))
            code, output = run(HAYRO_CLONE=str(clone))
        self.assertEqual(code, 1, output)
        self.assertIn("The clone is broken", output)

    def test_the_committed_table_matches_the_code(self) -> None:
        """With the real clone, `--check` has to be green.

        SKIPPED (not a pass) on a machine without the clone: this cannot be
        judged there, and a green tick for a check that read nothing is the
        failure this file is about.
        """
        clone = os.environ.get("HAYRO_CLONE", "")
        if not clone or not (pathlib.Path(clone) / ".git").exists():
            print("SKIPPED (not a pass): no HAYRO_CLONE, so the committed table "
                  "was not compared against the code", file=sys.stderr)
            return
        code, output = run(HAYRO_CLONE=clone)
        self.assertEqual(code, 0, output)


if __name__ == "__main__":
    unittest.main()
