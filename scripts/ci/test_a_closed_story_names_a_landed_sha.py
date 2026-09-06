#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The closure guard has to tell a landed sha from a sha (#348).

The failure it exists for is not a missing sha. All seven stories that were
closed on a written-off branch named one, and every sha was real; they were
simply not on master and never became so. So the case that matters here is the
one that looks right: a closure naming a commit that exists and is not an
ancestor of master must be reported.

Everything runs over a repository built in a temporary directory and a fake
`gh`, so this asks nothing of the network and judges no real tracker.
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "a_closed_story_names_a_landed_sha.py"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import sealed_env  # noqa: E402
SINCE = "2026-01-01T00:00:00Z"


GIT = shutil.which("git") or "git"


def git(*args: str, cwd: pathlib.Path) -> str:
    """git in a sealed environment.

    `sealed_env()` drops GIT_* and pins the config surface. Without it this
    `init` and `commit` inherit the caller's GIT_DIR, which inside a hook points
    at the real repository -- a fixture set `core.bare = true` on it that way on
    25-08-2026 and every worktree stopped working. Dropping GIT_* stops a
    fixture READING the real repository; pinning the config stops it WRITING
    there, and both incidents that night needed both halves.
    """
    done = subprocess.run([GIT, *args], cwd=cwd, capture_output=True, text=True,
                          check=True, env=sealed_env(identity=True, cwd=cwd))
    return done.stdout.strip()


def a_repository(tmp: pathlib.Path) -> tuple[pathlib.Path, str, str]:
    """A repository with one commit on master and one on a branch beside it.

    The second is the whole point: a real commit, in the repository, that master
    does not have -- which is what every one of the seven closures named.
    """
    root = tmp / "repo"
    root.mkdir()
    git("init", "-q", "-b", "master", ".", cwd=root)
    git("config", "user.email", "t@example.invalid", cwd=root)
    git("config", "user.name", "t", cwd=root)
    (root / "a").write_text("one")
    git("add", "-A", cwd=root)
    git("commit", "-q", "-m", "on master", cwd=root)
    landed = git("rev-parse", "HEAD", cwd=root)
    git("checkout", "-q", "-b", "written-off", cwd=root)
    (root / "b").write_text("two")
    git("add", "-A", cwd=root)
    git("commit", "-q", "-m", "never merged", cwd=root)
    stranded = git("rev-parse", "HEAD", cwd=root)
    git("checkout", "-q", "master", cwd=root)
    return root, landed, stranded


def a_fake_gh(tmp: pathlib.Path, body: str) -> None:
    """A `gh` that answers with one closed issue carrying `body`."""
    issue = {"number": 161, "title": "a story", "closed_at": "2026-06-01T00:00:00Z",
             "body": body}
    binary = tmp / "gh"
    binary.write_text(textwrap.dedent(f"""\
        #!{sys.executable}
        import json, sys
        path = sys.argv[-1]
        if "/comments" in path:
            print("[]")
        elif "/issues/161" in path:
            print(json.dumps({issue!r}))
        else:
            print(json.dumps([{issue!r}]))
        """), encoding="utf-8")
    binary.chmod(0o755)


def run(body: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        root, landed, stranded = a_repository(tmp)
        a_fake_gh(tmp, body.format(landed=landed, stranded=stranded))
        done = subprocess.run(
            [sys.executable, str(GUARD), "--since", SINCE],
            cwd=root, capture_output=True, text=True, check=False,
            env=dict(os.environ, PATH=f"{tmp}:{os.environ['PATH']}"),
        )
        return done.returncode, done.stdout + done.stderr


class AClosedStoryNamesALandedSha(unittest.TestCase):
    def test_a_sha_master_has_is_accepted(self) -> None:
        code, out = run("Done in {landed}.")
        self.assertEqual(code, 0, out)

    def test_a_real_sha_master_never_took_is_reported(self) -> None:
        """The whole of #348 in one case.

        The commit exists. It is in the repository. It is not on master, and the
        closing comment reads exactly like one that is.
        """
        code, out = run("Done in {stranded}.")
        self.assertEqual(code, 1, out)
        self.assertIn("#161", out)

    def test_no_sha_at_all_is_reported(self) -> None:
        code, out = run("Closed, this turned out to be fine.")
        self.assertEqual(code, 1, out)

    def test_a_written_no_code_reason_is_accepted(self) -> None:
        code, out = run("No code: superseded by the measurement above.")
        self.assertEqual(code, 0, out)

    def test_a_bare_declaration_is_not_a_reason(self) -> None:
        """`No code:` with nothing after it is the same silence with a label."""
        code, out = run("No code:")
        self.assertEqual(code, 1, out)

    def test_work_that_landed_in_another_repository_is_accepted_when_said(self) -> None:
        code, out = run("Landed in pdfluent-website: 1823b4ae")
        self.assertEqual(code, 0, out)

    def test_without_gh_it_says_so(self) -> None:
        """No access is not "every closure is sound"."""
        with tempfile.TemporaryDirectory() as td:
            tmp = pathlib.Path(td)
            root, _, _ = a_repository(tmp)
            done = subprocess.run(
                [sys.executable, str(GUARD), "--since", SINCE],
                cwd=root, capture_output=True, text=True, check=False,
                # A PATH with neither `gh` nor `git`. That is the shape of a
                # hosted runner as far as this guard is concerned, and the
                # answer must be "I could not look", not "all clear".
                env=dict(os.environ, PATH=str(tmp)),
            )
        output = done.stdout + done.stderr
        self.assertEqual(done.returncode, 0, output)
        self.assertIn("SKIPPED (not a pass)", output)


if __name__ == "__main__":
    unittest.main()
