#!/usr/bin/env python3
"""The sign-off gate bites, in both directions, on real repositories.

Fixtures rather than assertions about the source, because the thing under test
is a decision about commits and the only honest way to ask it is to make some.

The cases below are the ones that were actually wrong before #316: a commit
after the cutoff without a trailer (must fail), the same commit with one (must
pass), a commit written BEFORE the cutoff (must pass -- no retroactive
certification), and a merge commit (must pass, because it cannot carry a trailer
and demanding one made the gate unsatisfiable on every pull request).
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WACHT = ROOT / "scripts" / "ci" / "every_commit_since_the_cutoff_is_signed.py"

gevallen: list[tuple[str, bool]] = []


def geval(naam: str, ok: bool, detail: str = "") -> None:
    gevallen.append((naam, ok))
    print(f"  {'PASS' if ok else 'FAIL'}  {naam}" + (f"  -- {detail}" if detail and not ok else ""))


def schone_omgeving(**extra: str) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update({
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "fixture@invalid",
        "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "fixture@invalid",
    })
    env.update(extra)
    return env


def git(*args: str, cwd: Path, **extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True,
                          env=schone_omgeving(**extra), check=False)


def cut_at() -> int:
    """The cutoff the guard compiles with."""
    bron = WACHT.read_text()
    m = re.search(r"^CUT_AT = (\d+)$", bron, re.M)
    if not m:
        raise SystemExit("[signoff-test] the guard has no literal CUT_AT to read")
    return int(m.group(1))


def bouw_repo(tmp: Path, cut: int) -> Path:
    r = tmp / "repo"
    r.mkdir()
    git("init", "-q", "-b", "main", ".", cwd=r)
    (r / "a.txt").write_text("een\n")
    git("add", "a.txt", cwd=r)
    # Before the cutoff, and unsigned: must never be demanded.
    git("commit", "-q", "-m", "voor de cutoff, ongetekend",
        cwd=r, GIT_AUTHOR_DATE=f"{cut - 3600} +0000")
    return r


def draai(repo: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(WACHT)], cwd=repo,
                          capture_output=True, text=True,
                          env=schone_omgeving(), check=False)


def main() -> int:
    cut = cut_at()
    print(f"[signoff-test] cutoff the guard compiles with: {cut}")

    # The two cutoffs must not drift apart. ci.yml derives the same moment, and
    # a guard that disagrees with the job of the same name is worse than either.
    ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text()
    m = re.search(r"^\s*CUT_AT=(\d+)$", ci, re.M)
    geval("ci.yml carries the same cutoff as the guard",
          bool(m) and int(m.group(1)) == cut,
          f"ci.yml={m.group(1) if m else 'absent'} guard={cut}")

    with tempfile.TemporaryDirectory(prefix="signoff-") as raw:
        tmp = Path(raw)
        repo = bouw_repo(tmp, cut)

        # Nothing after the cutoff yet: passes, and says so rather than being silent.
        r = draai(repo)
        geval("a branch with nothing after the cutoff passes", r.returncode == 0,
              r.stdout + r.stderr)

        # After the cutoff, unsigned: must fail.
        (repo / "b.txt").write_text("twee\n")
        git("add", "b.txt", cwd=repo)
        git("commit", "-q", "-m", "na de cutoff, ongetekend",
            cwd=repo, GIT_AUTHOR_DATE=f"{cut + 60} +0000")
        r = draai(repo)
        geval("an unsigned commit after the cutoff is refused", r.returncode == 1,
              f"exit={r.returncode} {r.stdout}{r.stderr}")
        geval("the refusal names the commit rather than a count only",
              "na de cutoff, ongetekend" in (r.stdout + r.stderr),
              (r.stdout + r.stderr)[:160])

        # The same commit, signed: must pass.
        git("commit", "-q", "--amend", "--no-edit", "-s", cwd=repo,
            GIT_AUTHOR_DATE=f"{cut + 60} +0000")
        r = draai(repo)
        geval("the same commit with a sign-off passes", r.returncode == 0,
              f"exit={r.returncode} {r.stdout}{r.stderr}")

        # A merge commit after the cutoff: must pass. It cannot carry a trailer,
        # and demanding one is what made the CI job unsatisfiable (#316).
        git("checkout", "-q", "-b", "zijtak", "HEAD~1", cwd=repo)
        (repo / "c.txt").write_text("drie\n")
        git("add", "c.txt", cwd=repo)
        git("commit", "-q", "-s", "-m", "zijtak, getekend",
            cwd=repo, GIT_AUTHOR_DATE=f"{cut + 120} +0000")
        git("checkout", "-q", "main", cwd=repo)
        git("merge", "-q", "--no-ff", "--no-edit", "zijtak", cwd=repo,
            GIT_AUTHOR_DATE=f"{cut + 180} +0000")
        r = draai(repo)
        geval("a merge commit after the cutoff does not fail the gate",
              r.returncode == 0, f"exit={r.returncode} {r.stdout}{r.stderr}")

    mislukt = [n for n, ok in gevallen if not ok]
    print(f"\n  {len(gevallen)} assertion(s) ran, {len(mislukt)} failure(s)")
    return 1 if mislukt else 0


if __name__ == "__main__":
    sys.exit(main())
