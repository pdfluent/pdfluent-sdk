#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A file in the tree that is not source is named in a licence register. (#493)

WHAT WAS MISSING

`crates/pdf-manip/resources/cmap/GB-EUC-H` is an Adobe CMap, BSD-3-Clause,
"Copyright 1990-2019 Adobe. All rights reserved.", and it ships to crates.io in
the packaged file set. The obligation is met inside the file -- Adobe's notice
travels in its own header -- and the register entry was missing from all three
places that would have told a reader: NOTICE, THIRD_PARTY_LICENSES.txt, and
`docs/release/canonical_licenses.toml`, which is per-crate and has no shape for
a file.

That is not a mistake about one file. Every register we have answers "which
crate", and nothing answered "which file in this tree is not ours". Asking the
question of all of them found a second: `crates/pdf-font/assets/cmaps.brotli`,
the Adobe CMap resources as one compressed blob, in neither document either.

WHAT THIS ASKS

Every file under a crate's `assets/` or `resources/` directory is named, by file
name, in NOTICE or in THIRD_PARTY_LICENSES.txt. Those two are what leaves the
building: NOTICE travels in the NuGet package and THIRD_PARTY_LICENSES.txt is
the document a buyer's lawyer reads.

By file name and not by path, because both documents already list the fonts that
way and say why: "listed by file name because that is what the licence gate
counts, and because a family name does not tell a reader which bytes are in the
build".

WHAT IT DOES NOT ASK

Whether the licence is right, or whether we may ship the file at all. Those are
`license_gate.py`'s question and the owner's. This one makes sure the question
was written down where it cannot be lost -- the same division
`every_fetched_asset_is_registered.py` makes for the assets we download.

Exit codes:
  0  every vendored asset is named
  1  one is not
  2  nothing could be read, or so few files that the scan is not looking at what
     it thinks it is
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]

# The directories a crate keeps files it did not compile in. `src/` is source and
# is judged by the licence-header guard; `tests/` fixtures we made ourselves.
ASSET_DIRS = ("assets", "resources")

# The documents that leave the building.
REGISTERS = ("NOTICE", "THIRD_PARTY_LICENSES.txt")

# A scan that finds nothing approves everything. There were 60 such files when
# this was written, across seven crates.
MIN_FILES = 30

# What a file name may hold, written as "everything except the delimiters": `+`,
# `@` and `~` are all legal in one, and a list of the characters this tree
# happens to use today would let the next one through. `/` is out of it because
# a directory is named with its trailing slash in the font tables. (codex, #1887)
BINNEN = r"[^\s,;:()\"'\[\]/]"


def vendored(root: pathlib.Path) -> list[pathlib.Path]:
    found: list[pathlib.Path] = []
    crates = root / "crates"
    if not crates.is_dir():
        return found
    for crate in sorted(crates.iterdir()):
        for name in ASSET_DIRS:
            directory = crate / name
            if not directory.is_dir():
                continue
            found.extend(sorted(p for p in directory.rglob("*") if p.is_file()))
    return found


def mentions(text: str, word: str) -> bool:
    """Whether the register names this word, and not a longer one containing it.

    `LICENSE` is a substring of `LICENSE.txt`, and a file called the first was
    covered by a register that names only the second. The boundary is any
    character a file name can hold, which is why `\\b` will not do: it treats
    `.` and `-` as boundaries, and those are inside names here. (codex, #1887)

    `/` is NOT one of them: the font tables head their columns with
    `crates/pdf-interpret/assets/`, trailing slash and all, and a directory
    named that way is named.

    Written as "everything except the delimiters" rather than as a list of the
    characters names are made of: `+`, `@`, `~` and the rest are all legal in a
    file name, and a list of the ones we happen to use today would let the next
    one through. (codex, #1887)
    """
    return re.search(rf"(?<!{BINNEN}){re.escape(word)}(?!{BINNEN})", text) is not None


def heading(text: str, directory: str) -> bool:
    """Whether the register names this DIRECTORY, rather than a file inside it.

    `crates/one/assets` occurs inside `crates/one/assets/registered.bin`, so a
    register that names one file by path would otherwise have blessed every
    other file in that directory: one entry, a whole folder covered. A heading
    ends where the directory ends -- optionally with the trailing slash the font
    tables write -- and nothing of a file name follows it. (codex, #1887)
    """
    return re.search(rf"(?<!{BINNEN}){re.escape(directory)}/?(?!{BINNEN}|/)",
                     text) is not None


def ambiguous(files: list[pathlib.Path]) -> set[str]:
    """File names that more than one vendored file carries."""
    seen: dict[str, int] = {}
    for path in files:
        seen[path.name] = seen.get(path.name, 0) + 1
    return {name for name, count in seen.items() if count > 1}


def named(path: pathlib.Path, root: pathlib.Path, text: str, shared: set[str]) -> bool:
    """Whether a register names THIS file, and not merely a word it contains.

    Three shapes, and the second is the one the registers actually use: the font
    tables list bare file names under a heading that names their directory, and
    rewriting sixty rows into full paths would make a legal document worse to
    read for no gain.

    So a bare name counts only when it is unique among the vendored files AND
    its directory is named in the same register. `README.md` is carried by four
    crates and `LAB.icc` by two, so those must be named by path -- otherwise one
    mention of the word covers every file that happens to share it, which is a
    register that answers a question nobody asked. (codex, #1887)
    """
    relative = path.relative_to(root).as_posix()
    if mentions(text, relative):
        return True
    if path.name in shared:
        return False
    return (mentions(text, path.name)
            and heading(text, path.parent.relative_to(root).as_posix()))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT),
                        help="the checkout to read (a fixture tree, in the test)")
    parser.add_argument("--floor", type=int, default=MIN_FILES,
                        help="how few files means the scan is broken")
    arguments = parser.parse_args()
    root = pathlib.Path(arguments.repo).resolve()

    # EACH REGISTER READ ON ITS OWN (codex, #1887). Concatenated, a name in one
    # document and its directory in the other combine into a mention neither
    # makes -- and a reader has one document in front of them, not the pair.
    registers: list[str] = []
    for name in REGISTERS:
        path = root / name
        if not path.is_file():
            print(f"every_vendored_asset_is_named: SKIPPED (not a pass) -- {name} is "
                  "not there, so nothing was checked.", file=sys.stderr)
            return 2
        registers.append(path.read_text(errors="replace"))

    files = vendored(root)
    if len(files) < arguments.floor:
        print(f"every_vendored_asset_is_named: only {len(files)} file(s) found under "
              f"crates/*/{{{','.join(ASSET_DIRS)}}}, and there should be at least "
              f"{arguments.floor}.")
        print("  The scan is not reading the tree it thinks it is. A scan that finds "
              "nothing approves everything.")
        return 2

    shared = ambiguous(files)
    missing = [p for p in files
               if not any(named(p, root, register, shared) for register in registers)]
    if missing:
        print(f"every_vendored_asset_is_named: {len(missing)} vendored file(s) of "
              f"{len(files)} are not named in NOTICE or THIRD_PARTY_LICENSES.txt:")
        for path in missing:
            print(f"  {path.relative_to(root)}")
        print("")
        print("These ship. Name each one where a reader will look -- what it is, whose")
        print("it is, under which licence, and where that licence text lives -- in")
        print("THIRD_PARTY_LICENSES.txt, and in NOTICE beside the crate that carries")
        print("it. A file we wrote ourselves is named too, and says so: the register")
        print("answers 'which bytes here are not ours', and silence is not an answer.")
        return 1

    print(f"every_vendored_asset_is_named: OK — {len(files)} vendored file(s) under "
          f"crates/*/{{{','.join(ASSET_DIRS)}}}, every one named in a register.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
