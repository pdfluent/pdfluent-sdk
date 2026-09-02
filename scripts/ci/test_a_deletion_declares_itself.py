#!/usr/bin/env python3
"""Cases for a_deletion_declares_itself.py.

A guard that finds nothing on a clean branch cannot tell you whether it still
recognises anything -- the same shape as the workflows in #290 that were green
for 42 runs while comparing zero images. So every case here builds a scratch
repository that does the thing, and checks the guard says so.
"""
from __future__ import annotations
import os, subprocess, sys, tempfile
from pathlib import Path

GUARD = Path(__file__).with_name("a_deletion_declares_itself.py")
MINIMUM_CASES = 35  # FLOOR


def clean_env() -> dict[str, str]:
    """Without the caller's GIT_*.

    A hook hands GIT_DIR and GIT_WORK_TREE down as absolute paths, git then
    ignores the directory it was pointed at, and a test that builds scratch
    repositories starts editing the real one. On 25-08-2026 that set
    `core.bare = true` on this repository and stopped thirty worktrees.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def run(wd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=str(wd), capture_output=True,
                          text=True, env=clean_env())


def guard(wd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(GUARD), *args], cwd=str(wd),
                          capture_output=True, text=True, env=clean_env())


def repo(tmp: Path) -> Path:
    wd = tmp / "r"
    wd.mkdir()
    run(wd, "init", "-q", "-b", "master")
    run(wd, "config", "user.email", "t@t")
    run(wd, "config", "user.name", "t")
    (wd / "kept.txt").write_text("kept\n")
    (wd / "doomed.txt").write_text("doomed\n")
    run(wd, "add", "-A")
    run(wd, "commit", "-qm", "base")
    run(wd, "checkout", "-q", "-b", "topic")
    return wd


def main() -> int:
    failures: list[str] = []
    cases = 0

    def expect(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        if not ok:
            failures.append(f"{what}: {detail}")

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it")
        r = guard(wd, "--base", "master")
        expect("an undeclared deletion fails", r.returncode == 1, f"exit {r.returncode}")
        expect("and it names the file", "doomed.txt" in r.stderr, r.stderr[:120])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master")
        expect("a declared deletion passes", r.returncode == 0, f"exit {r.returncode} {r.stderr[:120]}")

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "new.txt").write_text("new\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "add one\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master")
        expect("a trailer for a file that is not deleted fails", r.returncode == 1,
               f"exit {r.returncode}")
        expect("and it says the trailer is the problem", "does not delete" in r.stderr,
               r.stderr[:140])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "new.txt").write_text("new\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "add one")
        r = guard(wd, "--base", "master")
        expect("a branch that deletes nothing passes", r.returncode == 0,
               f"exit {r.returncode} {r.stderr[:120]}")

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it")
        # declared in a LATER commit in the same range, not the deleting one
        (wd / "x.txt").write_text("x\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "later\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master")
        # This case used to assert the opposite, and asserting it is what made the
        # defect look intentional: a trailer in a LATER commit was accepted for a
        # deletion in an earlier one, so the deleting commit was never examined.
        # The trailer was chosen over a register file precisely because it travels
        # with the deletion through rebase and cherry-pick; accepting it anywhere
        # in the range lets the two come apart in exactly those operations.
        # (codex, #1635)
        expect("a trailer on a LATER commit does not count", r.returncode == 1,
               f"exit {r.returncode}")
        expect("and it says the trailer is on the wrong commit",
               "does not delete it" in r.stderr, r.stderr[:160])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it\n\nRemoves-deliberately: doomed.txt")
        (wd / "x.txt").write_text("x\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "unrelated work after it")
        r = guard(wd, "--base", "master")
        expect("a declared deletion still passes with later commits on top",
               r.returncode == 0, f"exit {r.returncode} {r.stderr[:120]}")

    # A `pull_request` run does not check out the PR head: actions/checkout
    # resolves the SYNTHETIC merge GitHub builds from base and head. Against its
    # first parent that merge removes every path the PR removes, and its
    # generated message carries no trailer -- so it became the last deleter of
    # everything and a correctly declared deletion failed. An ordinary merge on
    # master has the same shape. (codex, #1635)
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it\n\nRemoves-deliberately: doomed.txt")
        run(wd, "checkout", "-q", "master")
        run(wd, "merge", "-q", "--no-ff", "-m", "Merge pull request #1", "topic")
        r = guard(wd, "--base", "master~1" if False else "HEAD^1")
        expect("a declared deletion behind a merge commit passes",
               r.returncode == 0, f"exit {r.returncode} {r.stderr[:200]}")
        expect("and the merge is not blamed for it",
               "Merge pull request" not in r.stderr, r.stderr[:160])

    # Skipping merges must not buy that pass by ignoring them entirely.
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it with no trailer")
        run(wd, "checkout", "-q", "master")
        run(wd, "merge", "-q", "--no-ff", "-m", "Merge pull request #2", "topic")
        r = guard(wd, "--base", "HEAD^1")
        expect("an undeclared deletion behind a merge still fails",
               r.returncode == 1, f"exit {r.returncode}")
        expect("and it names the path", "doomed.txt" in r.stderr, r.stderr[:160])

    # core.quotePath is on by default, so git spells a path outside ASCII as
    # "runs/\316\262.md" while the trailer carries the literal one. The two
    # spellings never matched: a correct declaration was reported as undeclared
    # AND misplaced at once. This repo already tracks such paths. (codex, #1635)
    BETA = "runs/D\u03b2\u03b3.md"
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "runs").mkdir()
        (wd / BETA).write_text("data\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "add a path outside ASCII")
        run(wd, "rm", "-q", BETA)
        run(wd, "commit", "-qm", f"remove it\n\nRemoves-deliberately: {BETA}")
        r = guard(wd, "--base", "HEAD~1")
        expect("a declared non-ASCII path passes",
               r.returncode == 0, f"exit {r.returncode} {r.stderr[:200]}")
        expect("and it is not called misplaced",
               "does not delete it" not in r.stderr, r.stderr[:160])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "runs").mkdir()
        (wd / BETA).write_text("data\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "add a path outside ASCII")
        run(wd, "rm", "-q", BETA)
        run(wd, "commit", "-qm", "remove it silently")
        r = guard(wd, "--base", "HEAD~1")
        expect("an undeclared non-ASCII path fails",
               r.returncode == 1, f"exit {r.returncode}")
        expect("and the path is printed readably, not C-quoted",
               "\\316" not in r.stderr, r.stderr[:160])

    # A merge that performs its OWN deletion -- a conflict resolution that drops
    # a file both sides still had -- can and must declare it. Discarding every
    # merge deletion outright reported such a commit as undeclared AND misplaced
    # at once. The discriminator is the other parents: absent in any parent means
    # the merge inherited it; present in all of them means the merge did it.
    # (codex, #1635)
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "x.txt").write_text("topic\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "topic work")
        run(wd, "checkout", "-q", "master")
        (wd / "y.txt").write_text("master\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "master work")
        base = run(wd, "rev-parse", "HEAD").stdout.strip()
        run(wd, "merge", "-q", "--no-commit", "--no-ff", "topic")
        run(wd, "rm", "-q", "doomed.txt")   # neither side removed it; the merge does
        run(wd, "commit", "-qm", "merge and drop it\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", base)
        expect("a merge declaring its own deletion passes",
               r.returncode == 0, f"exit {r.returncode} {r.stderr[:200]}")
        expect("and it is not called misplaced",
               "does not delete it" not in r.stderr, r.stderr[:160])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "x.txt").write_text("topic\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "topic work")
        run(wd, "checkout", "-q", "master")
        (wd / "y.txt").write_text("master\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "master work")
        base = run(wd, "rev-parse", "HEAD").stdout.strip()
        run(wd, "merge", "-q", "--no-commit", "--no-ff", "topic")
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "merge and drop it silently")
        r = guard(wd, "--base", base)
        expect("a merge deleting silently still fails", r.returncode == 1,
               f"exit {r.returncode}")

    # A push reports where the branch WAS. Deriving a merge base from it loses
    # anything added between the common ancestor and that tip, so a force-push
    # that drops such a file reported nothing at all. (codex, #1635)
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "later.txt").write_text("added after the ancestor\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "add later.txt")
        before = run(wd, "rev-parse", "HEAD").stdout.strip()
        # Force the tip elsewhere: later.txt never existed on this line.
        run(wd, "reset", "-q", "--hard", "master")
        run(wd, "checkout", "-q", "-b", "forced")
        (wd / "z.txt").write_text("other work\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "unrelated work")
        r = guard(wd, "--base", before, "--exact-base")
        expect("a force-push that drops a file is seen with --exact-base",
               r.returncode == 1, f"exit {r.returncode} {r.stdout[:160]}")
        expect("and it names the lost file", "later.txt" in r.stderr, r.stderr[:200])
        r2 = guard(wd, "--base", before)
        # Not a bug to be fixed later: for a PULL REQUEST the merge base is the
        # right reading, because the base branch may have moved on and its
        # commits are not the PR's deletions. The two events want different
        # questions asked, which is why the flag exists rather than a new default.
        expect("and the merge-base reading deliberately does not see it",
               r2.returncode == 0, f"exit {r2.returncode}")

    # A force-push whose new history never had the file: nothing in base..head
    # can be its "last deleter", so the trailer had nowhere valid to go and a
    # deliberate removal could not be declared at all. (codex, #1635)
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "later.txt").write_text("added after the ancestor\n")
        run(wd, "add", "-A"); run(wd, "commit", "-qm", "add later.txt")
        before = run(wd, "rev-parse", "HEAD").stdout.strip()
        run(wd, "reset", "-q", "--hard", "master")
        run(wd, "checkout", "-q", "-b", "forced2")
        (wd / "z.txt").write_text("other\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "other work\n\nRemoves-deliberately: later.txt")
        r = guard(wd, "--base", before, "--exact-base")
        expect("a force-push removal declared anywhere in the range passes",
               r.returncode == 0, f"exit {r.returncode} {r.stderr[:200]}")
        expect("and it is not called misplaced",
               "does not delete it" not in r.stderr, r.stderr[:160])

    # The caller's config must not decide what a deletion IS. `git mv` with
    # diff.renames on was no deletion, with it off it deleted the old path -- the
    # same commit passing or failing depending on whose machine read it.
    # (T1 review, #1635)
    for renames in ("true", "false"):
        with tempfile.TemporaryDirectory() as d:
            wd = repo(Path(d))
            run(wd, "config", "diff.renames", renames)
            run(wd, "mv", "doomed.txt", "renamed.txt")
            run(wd, "commit", "-qm", "rename it")
            r = guard(wd, "--base", "master")
            expect(f"a rename is a deletion with diff.renames={renames}",
                   r.returncode == 1, f"exit {r.returncode}")

    # A git command that FAILS must not read as "nothing found". With a bad
    # --head the diff exited non-zero, stdout was empty, and the guard reported
    # OK having compared nothing. (codex, #1635)
    #
    # This case proves the PAIR of return-code checks, not either alone:
    # measured, disabling only the diff check leaves it green, because the log
    # check then catches the same bad --head. Disabling both turns it red. They
    # are defence in depth over one class of failure rather than two independent
    # ones, and saying otherwise would claim a coverage this case does not have.
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master", "--head", "no-such-revision",
                  "--exact-base")
        expect("a head that does not resolve is FATAL, not a pass",
               r.returncode == 2, f"exit {r.returncode} {r.stdout[:120]}")
        expect("and it says the comparison did not happen",
               "did not happen" in r.stderr or "could not be read" in r.stderr,
               r.stderr[:200])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it")
        r = guard(wd, "--base", "HEAD")
        expect("a base that is the head refuses, it does not pass",
               r.returncode == 2, f"exit {r.returncode}")

    # A declaration attached to a deletion that was later undone. The trailer is
    # real, the commit that carried it really did delete the file -- and none of
    # that is true of the deletion standing at the tip. (codex, #1635)
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "A: delete\n\nRemoves-deliberately: doomed.txt")
        (wd / "doomed.txt").write_text("doomed\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "B: restore it")
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "C: delete again, saying nothing")
        r = guard(wd, "--base", "master")
        expect("an earlier declaration does not excuse the last deletion",
               r.returncode == 1, f"exit {r.returncode}")

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "A: delete\n\nRemoves-deliberately: doomed.txt")
        (wd / "doomed.txt").write_text("doomed\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "B: restore it")
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "C: delete again\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master")
        expect("declaring the LAST deletion passes", r.returncode == 0,
               f"exit {r.returncode} {r.stderr[:140]}")

    # The misplaced-trailer path used to raise KeyError. The old case passed
    # because it asserted exit 1 and text printed BEFORE the exception -- a
    # traceback exits non-zero too, so the assertion could not tell the two
    # apart. This one refuses a traceback explicitly.
    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        (wd / "new.txt").write_text("new\n")
        run(wd, "add", "-A")
        run(wd, "commit", "-qm", "adds a file\n\nRemoves-deliberately: doomed.txt")
        r = guard(wd, "--base", "master")
        expect("a misplaced trailer fails", r.returncode == 1, f"exit {r.returncode}")
        expect("and does not crash", "Traceback" not in r.stderr, r.stderr[-160:])
        expect("and says how to fix it", "Move the trailer" in r.stderr,
               r.stderr[:160])

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        r = guard(wd, "--base", "no-such-branch")
        expect("an unreachable base is fatal, not a pass", r.returncode == 2,
               f"exit {r.returncode}")

    if cases < MINIMUM_CASES:  # FLOOR
        print(f"[test-deletions] FATAL: {cases} cases, floor is {MINIMUM_CASES}. "
              "A shrinking case list is how a guard stops covering its own reason.",
              file=sys.stderr)
        return 1
    if failures:
        print("[test-deletions] FATAL:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"[test-deletions] OK: {cases} cases, both directions and the blind case.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
