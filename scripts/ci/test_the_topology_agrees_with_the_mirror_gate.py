#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the_topology_agrees_with_the_mirror_gate.py, by mutation.

This repository normally agrees with itself, so running the guard here proves
only that it says so. Each case copies CLAUDE.md and the mirror gate into a
scratch tree, breaks the agreement in one specific way, and checks the guard
notices -- and then checks the restored pair passes, so a guard that simply
always failed could not sit here looking healthy.

The four ways it can come apart are the four this issue actually saw:

  the table calls the backup the source        (roles swapped in prose)
  GitLab written down as a CI executor again   (true until 28-08-2026)
  the gate's direction flipped, table untouched (the setting, not the decision)
  the topology section deleted                  (nothing left to disagree with)
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = pathlib.Path(__file__).with_name("the_topology_agrees_with_the_mirror_gate.py")
GATE = pathlib.Path(__file__).with_name("mirror_has_not_drifted.py")
TOPOLOGY = REPO / "CLAUDE.md"


def sandbox(root: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path, pathlib.Path]:
    """The guard resolves the repository as parents[2], so it needs the layout."""
    ci = root / "scripts" / "ci"
    ci.mkdir(parents=True)
    guard = ci / GUARD.name
    shutil.copy(GUARD, guard)
    shutil.copy(GATE, ci / GATE.name)
    topology = root / "CLAUDE.md"
    shutil.copy(TOPOLOGY, topology)
    return guard, topology, ci / GATE.name


def run(guard: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(guard)], capture_output=True,
                          text=True, check=False, env=sealed_env(cwd=guard.parent))


def case(name: str, mutate, expect_fail: bool, must_say: str = "") -> list[str]:
    with tempfile.TemporaryDirectory() as d:
        guard, topology, gate = sandbox(pathlib.Path(d))
        mutate(topology, gate)
        r = run(guard)
        out = r.stdout + r.stderr
        if expect_fail and r.returncode == 0:
            return [f"{name}: accepted (exit 0) -- {out.strip()[:200]}"]
        if not expect_fail and r.returncode != 0:
            return [f"{name}: refused a tree that agrees -- {out.strip()[:200]}"]
        if must_say and must_say.lower() not in out.lower():
            return [f"{name}: does not say why ('{must_say}' absent) -- {out.strip()[:200]}"]
    return []


def edit(text: str, old: str, new: str) -> str:
    """Replace, and refuse to do nothing.

    A mutation that no longer matches its target changes nothing, the guard
    passes the untouched tree, and the case reports "accepted (exit 0)" -- which
    reads as the guard being broken when it is the test that went stale. That
    happened here when the remotes were renamed for #291: three of the four
    mutations silently stopped mutating.
    """
    if old not in text:
        raise SystemExit(
            f"[test-topology] FATAL: this test no longer matches what it mutates -- "
            f"{old!r} is not in the file it was copied from. Nothing was tested.")
    return text.replace(old, new, 1)


def swap_roles(topology: pathlib.Path, _gate: pathlib.Path) -> None:
    t = topology.read_text()
    t = edit(t, "| `origin` | **source**", "| `origin` | **backup**")
    t = edit(t, "| `gitlab` | **backup**", "| `gitlab` | **source**")
    topology.write_text(t)


def gitlab_as_ci(topology: pathlib.Path, _gate: pathlib.Path) -> None:
    t = topology.read_text()
    t = edit(t, "| `gitlab` | **backup** — a copy of the source and nothing else |",
             "| `gitlab` | **backup** — CI executor for the heavy work, and a "
             "nightly backup |")
    topology.write_text(t)


def flip_the_gate(_topology: pathlib.Path, gate: pathlib.Path) -> None:
    t = gate.read_text()
    t = edit(t, 'os.environ.get("MIRROR_SOURCE", "origin/master")',
             'os.environ.get("MIRROR_SOURCE", "gitlab/master")')
    t = edit(t, 'os.environ.get("MIRROR_TARGET", "gitlab/master")',
             'os.environ.get("MIRROR_TARGET", "origin/master")')
    gate.write_text(t)


def delete_the_section(topology: pathlib.Path, _gate: pathlib.Path) -> None:
    t = topology.read_text()
    head, rest = t.split("## Repository topology", 1)
    topology.write_text(head + rest.split("\n## ", 1)[1])


def main() -> int:
    failures: list[str] = []
    failures += case("the table calls the backup the source", swap_roles, True, "source")
    failures += case("GitLab written down as a CI executor", gitlab_as_ci, True,
                     "28-08-2026")
    failures += case("the gate flipped instead of the table", flip_the_gate, True,
                     "backup")
    failures += case("the topology section deleted", delete_the_section, True, "row")
    failures += case("untouched", lambda *_: None, False)

    if failures:
        print("[test-topology] FATAL:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-topology] OK: swapped roles, CI executor, a flipped gate, a deleted "
          "section, and the tree as it stands.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
