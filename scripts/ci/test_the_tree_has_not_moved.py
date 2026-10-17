#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository.
"""The gate refuses a run whose worktree was checked out from under it.

THE CASE THAT MATTERS IS THE ONE THAT COMES BACK

On 06-09-2026 a second terminal moved the shared landing worktree to master and
put it back twenty-six seconds later, six minutes into a landing gate. Start and
end were the same commit on the same branch, so the obvious check -- compare
`git rev-parse HEAD` before and after -- would have passed and the push would
have carried unjudged commits to master.

So the first case here is not "HEAD is different". It is a round trip that
leaves HEAD exactly where it was, driven through a dummy gate that does its
checkout in the middle, the way the real one was interrupted. Take the reflog
field out of the guard and this test fails while every other case still passes.

The last case reads local_ci_gate.sh itself: a guard that is not called is not a
guard, and this is the wiring the incident was actually missing.
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "_the_tree_has_not_moved.sh"
GATE = REPO / "scripts" / "ci" / "local_ci_gate.sh"


def git(cwd: pathlib.Path, *args: str) -> str:
    """Every call sealed off from the repository this test lives in.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, so a fixture that
    inherits them builds its temporary repository inside the real one -- which
    is how a test once wrote `core.bare = true` onto this repository and stopped
    every terminal on the machine. `sealed_env` drops those and pins the config
    surface as well, because dropping GIT_* stops a fixture READING the real
    repository and not writing to the real config. `cwd=` adds the runtime
    check on top: it refuses before the first write when the repository git
    would actually act on is not under this directory, which sealing alone
    cannot see, because git finds that repository by walking up.
    """
    return subprocess.run(["git", *args], cwd=cwd, check=True, env=sealed_env(cwd=cwd),
                          capture_output=True, text=True).stdout.strip()


def a_repo_with_two_branches(root: pathlib.Path) -> pathlib.Path:
    """A worktree standing on a branch, with somewhere else to be moved to."""
    work = root / "work"
    work.mkdir()
    git(work, "init", "-q", "-b", "main")
    git(work, "config", "user.name", "gate test")
    git(work, "config", "user.email", "gate@test.invalid")
    (work / "file.txt").write_text("one\n", encoding="utf-8")
    git(work, "add", "file.txt")
    git(work, "commit", "-q", "-m", "one")
    git(work, "branch", "elsewhere")
    (work / "file.txt").write_text("two\n", encoding="utf-8")
    git(work, "add", "file.txt")
    git(work, "commit", "-q", "-m", "two")
    return work


def dummy_gate(work: pathlib.Path, interruption: str) -> subprocess.CompletedProcess:
    """Record, be interrupted, check -- the shape of the real gate, in six lines.

    `interruption` is shell run between the two, so each case differs only in
    what happens to the worktree while the gate believes it is measuring.
    """
    script = (
        "set -uo pipefail\n"
        f'baseline="$(bash {GUARD} --record)"\n'
        f"{interruption}\n"
        f'bash {GUARD} --check "$baseline" || exit 4\n'
        "echo GATE_VERDICT_PASS\n"
    )
    return subprocess.run(["bash", "-c", script], cwd=work, env=sealed_env(cwd=work),
                          capture_output=True, text=True)


def main() -> int:
    broken: list[str] = []

    def case(name: str, interruption: str, *, must_refuse: bool):
        with tempfile.TemporaryDirectory() as tmp:
            work = a_repo_with_two_branches(pathlib.Path(tmp))
            before = git(work, "rev-parse", "HEAD")
            done = dummy_gate(work, interruption)
            after = git(work, "rev-parse", "HEAD")
            refused = done.returncode == 4
            if refused != must_refuse:
                broken.append(
                    f"{name}: expected the gate to "
                    f"{'refuse' if must_refuse else 'pass'} and it "
                    f"{'refused' if refused else 'passed'} "
                    f"(exit {done.returncode}); HEAD {before[:8]} -> {after[:8]})")
                return
            if must_refuse and "TREE MOVED DURING GATE" not in done.stderr:
                broken.append(f"{name}: refused without saying TREE MOVED DURING "
                              f"GATE, so the reader cannot tell this from a real "
                              f"failure: {done.stderr.strip()[:200]}")
            if not must_refuse and "GATE_VERDICT_PASS" not in done.stdout:
                broken.append(f"{name}: the gate did not reach its verdict at all: "
                              f"{done.stderr.strip()[:200]}")

    # THE INCIDENT. Out to another commit and back; HEAD ends where it began.
    case("a checkout that comes back to the same commit",
         "git checkout -q --detach HEAD && git checkout -q main",
         must_refuse=True)

    # The same trip via a named branch, which is what `checkout -B` in the
    # landing script does when two landings share one worktree.
    case("a checkout onto another branch and back",
         "git checkout -q elsewhere && git checkout -q main",
         must_refuse=True)

    # Left somewhere else entirely: the easy case, and still a case.
    case("a checkout that stays away", "git checkout -q elsewhere",
         must_refuse=True)

    # Detached on the very commit it was already on. HEAD is the same object,
    # the ref is not, and the run was interrupted either way.
    case("a detach onto the same commit", "git checkout -q --detach HEAD",
         must_refuse=True)

    # A gate that was left alone must reach its verdict. Without this the guard
    # could refuse everything and every case above would still be green.
    case("an undisturbed run", "true", must_refuse=False)

    # Work in the tree is not a move: the gate is run on dirty worktrees all day
    # and refusing those would make this guard the thing that blocks everyone.
    case("an edit in the working tree, with HEAD untouched",
         "echo three > file.txt", must_refuse=False)

    # A commit made by the gate itself would be a move, but nothing does that;
    # the case is here so that if something ever starts, this says so rather
    # than the push doing it quietly.
    case("a commit made while the gate runs",
         "echo three > file.txt && git add file.txt && git commit -q -m three",
         must_refuse=True)

    # AND THE WIRING. The incident was not a missing guard, it was a missing
    # call: local_ci_gate.sh has to record a baseline and check it, or all of
    # the above is a script nobody runs.
    gate = GATE.read_text(encoding="utf-8")
    if "_the_tree_has_not_moved.sh --record" not in gate:
        broken.append("local_ci_gate.sh records no baseline, so there is nothing "
                      "to compare a moved worktree against")
    if "tree_check" not in gate or gate.count("tree_check") < 3:
        broken.append("local_ci_gate.sh calls tree_check fewer than three times; "
                      "the expensive phases and the verdict each need one, or a "
                      "move between them goes unreported")
    if "exit 4" not in gate:
        broken.append("local_ci_gate.sh does not leave with exit 4 on a moved "
                      "tree, so the hook cannot tell it apart from a real failure")
    # The hook belongs to another terminal and prints "local CI gate failed" for
    # every non-zero exit, so the sentence that keeps a pusher from hunting a
    # defect that does not exist has to come from the gate itself, above it.
    if "NOTHING WAS CHECKED" not in gate:
        broken.append("local_ci_gate.sh leaves on exit 4 without saying that "
                      "nothing was checked, so a moved tree reads to the pusher "
                      "as a gate that judged their code")

    if broken:
        for line in broken:
            print(f"FAIL: {line}", file=sys.stderr)
        return 1
    print("[tree-moved-test] OK: a worktree checked out from under the gate is "
          "refused -- including the round trip that ends where it started -- and "
          "an undisturbed run still reaches its verdict.")
    return 0


if __name__ == "__main__":
    if shutil.which("git") is None:
        print("FAIL: git is not on PATH", file=sys.stderr)
        sys.exit(1)
    sys.exit(main())
