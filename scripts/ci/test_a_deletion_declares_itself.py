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
MINIMUM_CASES = 9  # FLOOR


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

    with tempfile.TemporaryDirectory() as d:
        wd = repo(Path(d))
        run(wd, "rm", "-q", "doomed.txt")
        run(wd, "commit", "-qm", "remove it")
        r = guard(wd, "--base", "HEAD")
        expect("a base that is the head refuses, it does not pass",
               r.returncode == 2, f"exit {r.returncode}")

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
