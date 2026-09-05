#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for mirror_has_not_drifted.py, against scratch repositories.

The guard's own repository is normally in agreement, so running it there proves
only that it says so. These build the three shapes that matter and check it
notices each: a mirror far behind, a mirror that has gained commits of its own,
and a ref it cannot read at all.

The middle one is the shape that cost six days. A backup being behind is a
backup; a backup being *ahead* means someone is still working there, and
mirroring in the agreed direction would destroy it (#265, #231).
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env, inside_the_sandbox

import os
import pathlib
import subprocess
import sys
import tempfile

BEWAKER = pathlib.Path(__file__).with_name("mirror_has_not_drifted.py")


# GIT_DIR, GIT_INDEX_FILE and friends are set while a hook runs, and they follow
# a subprocess into a scratch repository -- where `git add` then writes into the
# real repository's index and exits 128. The local gate runs this from the
# pre-push hook, so it failed there and nowhere else.
# Sealed, not merely GIT_*-stripped: see fixture_env.py. (#297)
# Built per call now, from the directory being worked in: a module-level
# environment cannot carry a cwd, so the runtime sandbox check never ran.
def SCHOON_VOOR(map_):
    return sealed_env(cwd=map_)


def git(map_: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=map_, check=True, env=SCHOON_VOOR(map_),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def bouw(map_: pathlib.Path, bron_extra: int, spiegel_extra: int) -> None:
    git(map_, "init", "-q", "-b", "main")
    git(map_, "config", "user.email", "t@t")
    git(map_, "config", "user.name", "t")
    (map_ / "a").write_text("0")
    git(map_, "add", "a")
    git(map_, "commit", "-qm", "start")
    git(map_, "branch", "spiegel")
    for n in range(bron_extra):
        (map_ / "a").write_text(f"bron{n}")
        git(map_, "add", "a")
        git(map_, "commit", "-qm", f"bron{n}")
    if spiegel_extra:
        git(map_, "checkout", "-q", "spiegel")
        for n in range(spiegel_extra):
            (map_ / "b").write_text(f"spiegel{n}")
            git(map_, "add", "b")
            git(map_, "commit", "-qm", f"spiegel{n}")
        git(map_, "checkout", "-q", "main")


def draai(map_: pathlib.Path, bron: str, doel: str,
          *vlaggen: str) -> subprocess.CompletedProcess[str]:
    omgeving = dict(SCHOON_VOOR(map_), MIRROR_SOURCE=bron, MIRROR_TARGET=doel)
    return subprocess.run([sys.executable, str(BEWAKER), *vlaggen], cwd=map_,
                          capture_output=True, text=True, env=omgeving, check=False)


def main() -> int:
    fouten: list[str] = []

    # 1. A few commits behind is a backup, not a fault.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, bron_extra=3, spiegel_extra=0)
        r = draai(m, "main", "spiegel")
        if r.returncode != 0:
            fouten.append(f"3 behind should pass: {r.stdout}{r.stderr}")

    # 2. Far behind is a snapshot of something else -- and the same repository
    #    answers the two --fetch questions below, because building sixty commits
    #    three times over turned this file into two minutes of a gate that runs
    #    on every push.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, bron_extra=60, spiegel_extra=0)
        r = draai(m, "main", "spiegel")
        if r.returncode == 0:
            fouten.append("60 commits behind is accepted")
        elif "behind" not in r.stderr:
            fouten.append(f"60 behind is not named as such: {r.stderr[:160]}")

        # 2b. --fetch where the refs cannot be refreshed: these are plain branch
        #     names, so there is no remote to read them from. The drift is real
        #     and must still be printed, but it may not refuse -- a landing
        #     stopped by a network nobody at the keyboard can fix blames the
        #     wrong person, and that is how master closed four times in one day.
        r = draai(m, "main", "spiegel", "--fetch")
        if r.returncode != 0:
            fouten.append("an unrefreshable ref refused instead of warning")
        if "WARNING (not a verdict)" not in r.stderr:
            fouten.append(f"the downgrade is not announced: {r.stderr[:200]}")
        if "behind" not in r.stderr:
            fouten.append(f"the drift is not reported at all: {r.stderr[:200]}")

    # 3. Ahead at all: someone is working on the backup.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, bron_extra=1, spiegel_extra=2)
        r = draai(m, "main", "spiegel")
        if r.returncode == 0:
            fouten.append("a mirror two commits ahead is accepted")
        elif "AHEAD" not in r.stderr:
            fouten.append(f"being ahead is not named as such: {r.stderr[:160]}")

    # 4. A ref it cannot read must announce the skip, never agree by default.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, bron_extra=1, spiegel_extra=0)
        r = draai(m, "main", "bestaat/niet")
        if "SKIPPED (not a pass)" not in r.stderr:
            fouten.append(f"an unreadable ref does not announce a skip: {r.stderr[:160]}")

    if fouten:
        print("[test-mirror] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-mirror] OK: behind, far behind, ahead, an unreadable ref, "
          "and a refresh that could not happen.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
