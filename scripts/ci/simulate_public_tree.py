#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Assemble the tree that would be published, and prove it still builds.

WHY THIS EXISTS

`docs/PUBLIC_TREE.toml` says which paths stay in-house. Leaving a directory out
is easy to write down and easy to get wrong: `crates/xfa-golden-tests` is an
explicit entry in the workspace `members` list, so dropping the directory
without dropping that line makes `cargo metadata` fail on the first command,
before anything else gets a chance to run. A list nobody has executed is a plan,
not a boundary.

So this builds the tree from the same manifest the guard reads, rewrites the
workspace members, and runs cargo in it. What it proves is exactly what it ran;
see --help for the stages.

Usage:
    simulate_public_tree.py [--keep] [--stage metadata|check|test]

Exit codes:
    0  the assembled tree passed the requested stage
    1  it did not, or the tree could not be assembled
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
MANIFEST = REPO / "docs" / "PUBLIC_TREE.toml"


def _git_omgeving() -> dict:
    """git without the caller's GIT_* variables.

    Inside a git hook GIT_DIR and GIT_WORK_TREE point at the real repository,
    and git then ignores the directory you point it at. On 25-08-2026 a test set
    `core.bare = true` on the real repository that way and everything stopped.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def tracked() -> list[str]:
    """Tracked paths, NUL-separated.

    Without `-z`, git C-quotes any path holding a non-ASCII byte, and the quoted
    string then matches no prefix in [internal].paths and names no file on disk.
    The exporter's reaction is to skip it: `src.is_file()` is false, `continue`.

    So a file lands outside the published tree because its name has an accent in
    it, not because anybody decided it should. Today the two files in that state
    are internal anyway and the omission is harmless -- which is exactly why it
    went unnoticed. The same accident on a source file removes it from the
    published tree and the build breaks for a stranger and for nobody here.
    """
    out = subprocess.run(["git", "ls-files", "-z"], capture_output=True, text=True,
                         cwd=REPO, env=_git_omgeving())
    return [f for f in out.stdout.split("\0") if f]


def ledenregel(lid: str) -> "re.Pattern":
    """De `members`-regel van één crate.

    Eén bron, net als `wordt_gepubliceerd` hierboven, en om dezelfde reden. De
    zaai (`scripts/release/seed_history_filter.py`) moet deze regel over de HELE
    historie toepassen, want een gepubliceerde workspace die een crate noemt die
    er niet is, parseert niet -- en tot 06-09-2026 deed de zaai dat niet, terwijl
    dit bestand het al jaren wel deed. Twee mechanismen, één vraag, twee
    antwoorden: precies wat een gedeelde functie onmogelijk maakt.
    """
    return re.compile(rf'^\s*"{re.escape(lid)}",\s*\n', re.M)


def zonder_interne_leden(tekst: str, leden) -> str:
    """Het hoofdmanifest zonder de leden die niet meereizen.

    Weigert hier niets: dit is de bewerking, niet het oordeel. `assemble` eist dat
    elk gedeclareerd lid ook echt in de lijst staat, en de zaai kan dat niet eisen
    omdat een manifest van vóór de crate hem niet noemt.
    """
    for lid in leden:
        tekst = ledenregel(lid).sub("", tekst)
    return tekst


def wordt_gepubliceerd(pad: str, m: dict) -> bool:
    """Of dit pad in de publieke boom terechtkomt.

    Eén bron voor de vraag "gaat dit mee". `geen_interne_zaken --boom` stelt
    dezelfde vraag, en twee kopieën van deze twee regels zouden uit elkaar
    lopen zonder dat iets dat merkt.
    """
    return not (pad.startswith(tuple(m["internal"]["paths"]))
                or pad in set(m["internal"]["files"]))


def assemble(dest: Path, m: dict) -> int:
    n = 0
    for f in tracked():
        if not wordt_gepubliceerd(f, m):
            continue
        src = REPO / f
        if not src.is_file():
            continue
        doel = dest / f
        doel.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, doel)
        n += 1

    # Drop the internal crates from the workspace members list.
    cargo = dest / "Cargo.toml"
    tekst = cargo.read_text()
    for lid in m["internal"]["workspace_members"]:
        if not ledenregel(lid).search(tekst):
            print(f"[simulate] FATAL: workspace member {lid!r} is declared internal but "
                  f"is not in the members list; the manifest and Cargo.toml disagree",
                  file=sys.stderr)
            return -1
    cargo.write_text(zonder_interne_leden(tekst, m["internal"]["workspace_members"]))
    return n


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--keep", action="store_true", help="leave the tree on disk")
    p.add_argument("--stage", default="metadata", choices=["metadata", "check", "test"])
    a = p.parse_args()

    m = tomllib.loads(MANIFEST.read_text())
    tmp = Path(tempfile.mkdtemp(prefix="publieke-boom-"))
    try:
        n = assemble(tmp, m)
        if n < 0:
            return 1
        print(f"[simulate] assembled {n} file(s) at {tmp}")

        env = dict(os.environ)
        # Its own target dir: the point is to compile what a stranger would get,
        # and a warm cache from the full tree hides a missing file.
        env["CARGO_TARGET_DIR"] = str(tmp / "target")
        env["CARGO_TERM_COLOR"] = "never"

        stappen = {"metadata": ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                   "check": ["cargo", "check", "--workspace", "--locked"],
                   "test": ["cargo", "test", "--workspace", "--locked", "--no-run"]}
        volgorde = ["metadata", "check", "test"]
        for stap in volgorde[: volgorde.index(a.stage) + 1]:
            print(f"[simulate] {stap} ...", flush=True)
            r = subprocess.run(stappen[stap], cwd=tmp, env=env,
                               capture_output=True, text=True)
            if r.returncode != 0:
                print(f"[simulate] FAIL at {stap} (exit {r.returncode})")
                for regel in (r.stderr or r.stdout).splitlines()[-25:]:
                    print(f"  {regel[:150]}")
                return 1
            print(f"[simulate] {stap} ok")
        print(f"[simulate] the assembled tree passed: {a.stage}")
        return 0
    finally:
        if a.keep:
            print(f"[simulate] kept at {tmp}")
        else:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
