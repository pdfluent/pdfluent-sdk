#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
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
SCHOON = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(map_: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=map_, check=True, env=SCHOON,
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


def draai(map_: pathlib.Path, bron: str, doel: str) -> subprocess.CompletedProcess[str]:
    omgeving = dict(SCHOON, MIRROR_SOURCE=bron, MIRROR_TARGET=doel)
    return subprocess.run([sys.executable, str(BEWAKER)], cwd=map_,
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

    # 2. Far behind is a snapshot of something else.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, bron_extra=60, spiegel_extra=0)
        r = draai(m, "main", "spiegel")
        if r.returncode == 0:
            fouten.append("60 commits behind is accepted")
        elif "behind" not in r.stderr:
            fouten.append(f"60 behind is not named as such: {r.stderr[:160]}")

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
    print("[test-mirror] OK: behind, far behind, ahead, and an unreadable ref.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
