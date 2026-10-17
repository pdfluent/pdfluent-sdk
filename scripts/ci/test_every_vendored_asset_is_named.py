#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Drives every_vendored_asset_is_named.py. (#493)

The ways this guard could be green and worthless:

  it reads only one of the two registers  -- and then a file named in
                                             THIRD_PARTY_LICENSES.txt but not in
                                             NOTICE is a finding, or the other
                                             way round
  it looks in the wrong place             -- `src/` is source, `assets/` is not
  it approves an empty scan               -- which is how a guard that stopped
                                             finding files reports a clean tree
"""
from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "every_vendored_asset_is_named.py"

failures: list[str] = []
done = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global done
    done += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        failures.append(what)


def tree(assets: dict[str, str], notice: str, third_party: str) -> tempfile.TemporaryDirectory:
    holder = tempfile.TemporaryDirectory()
    fake = pathlib.Path(holder.name)
    for path, body in assets.items():
        target = fake / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body)
    (fake / "NOTICE").write_text(notice)
    (fake / "THIRD_PARTY_LICENSES.txt").write_text(third_party)
    return holder


def judge(where: str, floor: int = 3) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(GUARD), "--repo", where, "--floor", str(floor)],
                          capture_output=True, text=True, timeout=60)


THREE = {
    "crates/one/assets/first.cff": "x",
    "crates/one/assets/second.ttf": "x",
    "crates/two/resources/cmap/THIRD-H": "x",
}

print("a file named in either register is enough")
with tree(THREE, notice="crates/one/assets holds first.cff and second.ttf\n",
          third_party="crates/two/resources/cmap/THIRD-H, Adobe, BSD-3-Clause\n") as where:
    good = judge(where)
expect("three files across two registers pass", good.returncode == 0, good.stdout[-300:])

print("and one named in neither is a finding")
with tree(THREE, notice="crates/one/assets: first.cff\n",
          third_party="crates/one/assets: second.ttf\n") as where:
    bad = judge(where)
expect("the unnamed file is refused", bad.returncode == 1, bad.stdout[-300:])
expect("and it is named in the finding", "THIRD-H" in bad.stdout, bad.stdout[-300:])

# THE NAME ON ITS OWN IS NOT AN ANSWER (codex, #1887). Four crates carry an
# `assets/README.md` and two an `LAB.icc`; one mention of the word would have
# covered every one of them.
print("a name more than one file carries must be named by path")
SHARED = {
    "crates/one/assets/README.md": "x",
    "crates/two/assets/README.md": "x",
    "crates/two/resources/cmap/THIRD-H": "x",
}
with tree(SHARED, notice="README.md\n", third_party="THIRD-H\n") as where:
    shared = judge(where)
expect("a bare shared name covers neither file", shared.returncode == 1,
       shared.stdout[-400:])
expect("and both are named in the finding",
       shared.stdout.count("README.md") >= 2, shared.stdout[-400:])
with tree(SHARED, notice="crates/one/assets/README.md\ncrates/two/assets/README.md\n",
          third_party="crates/two/resources/cmap/THIRD-H\n") as where:
    by_path = judge(where)
expect("naming both by path answers it", by_path.returncode == 0, by_path.stdout[-400:])

print("a unique name needs its directory named too")
with tree(THREE, notice="crates/one/assets holds first.cff and second.ttf\n",
          third_party="crates/two/resources/cmap/THIRD-H\n") as where:
    (pathlib.Path(where) / "crates/three/assets").mkdir(parents=True)
    (pathlib.Path(where) / "crates/three/assets/lonely.bin").write_text("x")
    lonely = judge(where)
expect("a file whose directory nothing names is refused", lonely.returncode == 1,
       lonely.stdout[-400:])

# A NAME IS NOT A PREFIX OF A LONGER ONE, AND THE TWO REGISTERS ARE TWO
# DOCUMENTS (codex, #1887).
print("a longer name does not cover a shorter one")
LICENCE_PAIR = {
    "crates/one/assets/LICENSE": "x",
    "crates/one/assets/first.cff": "x",
    "crates/one/assets/second.ttf": "x",
}
with tree(LICENCE_PAIR, notice="crates/one/assets holds LICENSE.txt, first.cff, second.ttf\n",
          third_party="x\n") as where:
    prefix = judge(where)
expect("`LICENSE` is not covered by a register that names `LICENSE.txt`",
       prefix.returncode == 1, prefix.stdout[-400:])
# Any legal file-name character, not only the ones this tree happens to use.
with tree(LICENCE_PAIR, notice="crates/one/assets holds LICENSE+extra, first.cff, second.ttf\n",
          third_party="x\n") as where:
    plus = judge(where)
expect("nor by one that names `LICENSE+extra`", plus.returncode == 1, plus.stdout[-400:])

print("each register is read on its own")
with tree(THREE, notice="first.cff second.ttf\n",
          third_party="crates/one/assets and crates/two/resources/cmap/THIRD-H\n") as where:
    split = judge(where)
expect("a name in one document and its directory in the other is not a mention",
       split.returncode == 1, split.stdout[-400:])

print("one file named by path does not bless its directory")
NEIGHBOURS = {
    "crates/one/assets/registered.bin": "x",
    "crates/one/assets/neighbour.bin": "x",
    "crates/two/resources/cmap/THIRD-H": "x",
}
with tree(NEIGHBOURS, notice="crates/one/assets/registered.bin, neighbour.bin\n",
          third_party="crates/two/resources/cmap/THIRD-H\n") as where:
    neighbour = judge(where)
expect("a neighbour is not covered by the path of the file beside it",
       neighbour.returncode == 1, neighbour.stdout[-400:])
with tree(NEIGHBOURS, notice="crates/one/assets/ holds registered.bin and neighbour.bin\n",
          third_party="crates/two/resources/cmap/THIRD-H\n") as where:
    real_heading = judge(where)
expect("a heading over the directory does cover it", real_heading.returncode == 0,
       real_heading.stdout[-400:])

print("source is not a vendored asset")
with tree({**THREE, "crates/one/src/lib.rs": "fn main() {}\n"},
          notice="crates/one/assets holds first.cff and second.ttf\n",
          third_party="crates/two/resources/cmap/THIRD-H\n") as where:
    source = judge(where)
expect("a file under src/ is not asked about", source.returncode == 0, source.stdout[-300:])

print("a register that is not there is not a pass")
with tree(THREE, notice="crates/one/assets first.cff second.ttf THIRD-H\n",
          third_party="x\n") as where:
    (pathlib.Path(where) / "THIRD_PARTY_LICENSES.txt").unlink()
    gone = judge(where)
expect("a missing register refuses", gone.returncode == 2, gone.stderr[-300:])
expect("and says nothing was checked", "nothing was checked" in gone.stderr, gone.stderr[-300:])

print("a scan that finds nothing approves nothing")
with tree(THREE, notice="crates/one/assets first.cff second.ttf THIRD-H\n",
          third_party="x\n") as where:
    thin = judge(where, floor=30)
expect("three files where thirty are expected is a refusal", thin.returncode == 2,
       thin.stdout[-300:])
expect("and it says so", "approves everything" in thin.stdout, thin.stdout[-300:])

print("and this repository obeys the rule")
real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True, timeout=120)
expect("every vendored file here is named in a register", real.returncode == 0,
       real.stdout[-600:])

print(f"test_every_vendored_asset_is_named: {done} case(s), {len(failures)} failure(s)")
sys.exit(1 if failures else 0)
