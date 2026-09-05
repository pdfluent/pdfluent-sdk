#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for scripts/infra/mirror_to_gitlab.sh, against scratch repositories.

The script pushes to a remote, so it cannot be tried out on the real one: the
first thing worth knowing about it is what it does when pushing would destroy
something, and there is no safe way to ask that question of GitLab. These build
a source repository, a bare mirror, and a working clone with both as remotes,
and drive the four shapes:

  behind      the ordinary case -- the mirror ends up on the source's tip
  ahead       refused, and the mirror is left EXACTLY as it was
  ahead + --archive   the mirror's own commits survive as a tag, then it syncs
  unreachable a remote that cannot be fetched is a failure, never a quiet pass

The second shape is the one that matters. On 05-09-2026 the real mirror was 348
commits behind and 8 ahead at the same time; a sync without that check would have
deleted the eight. Remove the AHEAD refusal from the script and case 2 turns red.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import pathlib
import subprocess
import sys
import tempfile

SCRIPT = (pathlib.Path(__file__).resolve().parents[2]
          / "scripts" / "infra" / "mirror_to_gitlab.sh")


def git(where: pathlib.Path, *args: str, check: bool = True) -> str:
    r = subprocess.run(["git", *args], cwd=where, check=check,
                       env=sealed_env(cwd=where), capture_output=True, text=True)
    return r.stdout.strip()


def commit(where: pathlib.Path, name: str, text: str) -> None:
    (where / name).write_text(text)
    git(where, "add", name)
    git(where, "commit", "-qm", f"{name}: {text}")


def build(root: pathlib.Path, behind: int, ahead: int) -> pathlib.Path:
    """A source repo, a bare mirror, and a clone that can see both.

    The mirror is bare because a push to a checked-out branch is refused by git
    for reasons that have nothing to do with this script, and a test that passes
    because git said no somewhere else has measured nothing.
    """
    source = root / "source"
    source.mkdir()
    git(source, "init", "-q", "-b", "master")
    git(source, "config", "user.email", "t@example.invalid")
    git(source, "config", "user.name", "t")
    commit(source, "a", "start")

    mirror = root / "mirror.git"
    subprocess.run(["git", "clone", "-q", "--bare", str(source), str(mirror)],
                   check=True, env=sealed_env(cwd=root))

    for n in range(behind):
        commit(source, "a", f"source {n}")

    work = root / "work"
    subprocess.run(["git", "clone", "-q", "--origin", "github", str(source), str(work)],
                   check=True, env=sealed_env(cwd=root))
    git(work, "config", "user.email", "t@example.invalid")
    git(work, "config", "user.name", "t")
    git(work, "remote", "add", "origin", str(mirror))

    if ahead:
        # Commits that exist only on the mirror -- someone still working on the
        # backup, which is the state the refusal is about.
        side = root / "side"
        subprocess.run(["git", "clone", "-q", str(mirror), str(side)],
                       check=True, env=sealed_env(cwd=root))
        git(side, "config", "user.email", "t@example.invalid")
        git(side, "config", "user.name", "t")
        for n in range(ahead):
            commit(side, "b", f"mirror only {n}")
        git(side, "push", "-q", "origin", "master")

    git(work, "fetch", "-q", "--all")
    return work


def run(work: pathlib.Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = dict(sealed_env(cwd=work),
               MIRROR_SOURCE_REMOTE="github", MIRROR_TARGET_REMOTE="origin",
               MIRROR_BRANCH="master")
    return subprocess.run(["bash", str(SCRIPT), *args], cwd=work,
                          capture_output=True, text=True, env=env, check=False)


def mirror_tip(work: pathlib.Path) -> str:
    return git(work, "ls-remote", "origin", "refs/heads/master").split("\t")[0]


def main() -> int:
    failures: list[str] = []

    if not SCRIPT.is_file():
        print(f"[test-mirror-sync] SKIPPED (not a pass): {SCRIPT} is missing.",
              file=sys.stderr)
        return 1

    # 1. Behind: the ordinary case. The mirror ends on the source's tip, and the
    #    script says so only after reading it back off the remote.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=4, ahead=0)
        r = run(work)
        source_tip = git(work, "rev-parse", "github/master")
        if r.returncode != 0:
            failures.append(f"a mirror 4 behind was not synced: {r.stdout}{r.stderr}")
        elif mirror_tip(work) != source_tip:
            failures.append("the sync reported success and the mirror did not move")

    # 2. Ahead: refused, and nothing on the mirror is touched.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=3, ahead=2)
        before = mirror_tip(work)
        r = run(work)
        if r.returncode == 0:
            failures.append("a mirror 2 commits ahead was overwritten without asking")
        if "ahead" not in (r.stdout + r.stderr):
            failures.append(f"being ahead is not named: {(r.stdout + r.stderr)[:200]}")
        if mirror_tip(work) != before:
            failures.append("the refusal still moved the mirror -- work was destroyed")

    # 3. Ahead with --archive: the mirror's own commits survive the overwrite.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=3, ahead=2)
        before = mirror_tip(work)
        source_tip = git(work, "rev-parse", "github/master")
        r = run(work, "--archive")
        if r.returncode != 0:
            failures.append(f"--archive did not sync: {r.stdout}{r.stderr}")
        if mirror_tip(work) != source_tip:
            failures.append("--archive did not put the source tip on the mirror")
        tags = git(work, "ls-remote", "--tags", "origin")
        if before not in tags:
            failures.append("--archive overwrote the mirror without keeping its commits")
        if f"archive/mirror-master-{before[:8]}" not in tags:
            failures.append(f"the archive tag is not named after the tip it saves: {tags}")

    # 4. A remote that cannot be reached is a failure. A sync that cannot read
    #    both sides and returns 0 is the silence this whole issue is about.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=2, ahead=0)
        git(work, "remote", "set-url", "origin", str(pathlib.Path(d) / "gone.git"))
        r = run(work)
        if r.returncode == 0:
            failures.append("an unreachable mirror was reported as a successful sync")

    # 5. Already equal: no push, and it says so rather than claiming work.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=0, ahead=0)
        r = run(work)
        if r.returncode != 0:
            failures.append(f"an already-equal mirror was treated as a fault: {r.stderr}")
        if "already equal" not in r.stdout:
            failures.append(f"an equal mirror is not reported as such: {r.stdout[:200]}")

    # 6. Unreachable AND the refs on disk say the two agree. This is the shape
    #    the fetch exists for: without it the script reads a ref from the last
    #    time somebody fetched, finds nothing to do, and reports a healthy backup
    #    it never contacted. Silence that looks like a pass is the whole issue.
    with tempfile.TemporaryDirectory() as d:
        work = build(pathlib.Path(d), behind=0, ahead=0)
        git(work, "remote", "set-url", "origin", str(pathlib.Path(d) / "gone.git"))
        r = run(work)
        if r.returncode == 0:
            failures.append("a mirror it could not read was called equal and healthy")

    if failures:
        print("[test-mirror-sync] FATAL:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-mirror-sync] OK: behind, ahead refused, archived, unreachable, equal.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
