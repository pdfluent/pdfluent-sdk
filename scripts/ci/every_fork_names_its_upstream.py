#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A forked crate says in its README that it is a fork (LC7, #219).

THE PROBLEM IS VISIBILITY, NOT COMPLIANCE

The attribution is complete and correct. `NOTICE` names every fork with its
upstream link, `THIRD_PARTY_LICENSES.txt` gives the full renaming table, and the
LICENSE files sit in every crate directory. This is done properly.

Only nobody reads those files. What crates.io and GitHub show is the README, and
that is the first thing a Rust reader checks. If they have to discover the fork
themselves, the publication becomes a story about what we did not mention.

WHY THE LINE GOES AT THE TOP AND NOT AT THE BOTTOM

The bottom of a README is the same as NOTICE: it is there, and nobody sees it.
The line sits directly under the title, above the description.

WHY THIS READS THE PROVENANCE TABLE RATHER THAN A LIST OF ITS OWN

Two lists drift apart, and then the question is which one is true.
`herkomsttabel.py` already knows the forks; this reads that same source. Add a
fork there and this check falls over without anybody having to update it.

    python3 scripts/ci/every_fork_names_its_upstream.py            # check
    python3 scripts/ci/every_fork_names_its_upstream.py --write    # fill in

Exit codes:
    0  every forked crate names its upstream
    1  one does not, or the provenance table could not be read
"""
from __future__ import annotations

import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from herkomsttabel import UPSTREAM  # noqa: E402

# FLOOR: forks >= 8. There are nine directories with upstream provenance. Find
# fewer and the provenance table is broken rather than a fork having vanished,
# and checking zero crates succeeds every time.
MIN_FORKS = 8

# Where upstream lives, per source named in the provenance table.
SOURCE_URL = {
    "hayro (Laurenz Stampfl)": "https://github.com/LaurenzV/hayro",
    "lopdf": "https://github.com/J-F-Liu/lopdf",
    "ttf-parser / pdf.js (Reizner, Muizelaar)": "https://github.com/RazrFalcon/ttf-parser",
}

# The upstream crate and the version we are level with, per directory name. The
# fork point itself is in docs/UPSTREAM_FORKS.toml with its evidence; a README
# reader needs the release, not the commit.
FORK = {
    "pdf-syntax": ("hayro-syntax", "0.5.0"),
    "pdf-interpret": ("hayro-interpret", "0.5.0"),
    "pdf-render": ("hayro", "0.5.0"),
    "pdf-font": ("hayro-font", "0.5.0"),
    "hayro-ccitt": ("hayro-ccitt", "0.3.0"),
    "hayro-jbig2": ("hayro-jbig2", "0.1.0"),
    "hayro-jpeg2000": ("hayro-jpeg2000", "0.4.0"),
    "lopdf": ("lopdf", "0.44.0"),
    # No version: our 0.2.1 is our own number, and no upstream release has been
    # matched to this code. docs/UPSTREAM_FORKS.toml records the fork point as
    # not established, and a version here would be a claim nothing supports.
    "cff-parser": ("ttf-parser", "—"),
}

# What counts as naming the upstream.
#
# Deliberately two conditions rather than one exact sentence: pdf-syntax already
# carried a fork line in its own words before this check existed, and a guard
# that demands its own phrasing would have rewritten a correct README to satisfy
# itself. The word and the link together cannot be satisfied by accident.
MARK = "fork"


def notice(directory: str) -> str:
    upstream, version = FORK[directory]
    source, licence = UPSTREAM[directory]
    url = SOURCE_URL[source]
    at = f" at version {version}" if version != "—" else ""
    return (
        f"> **This crate is a fork.** It began as [`{upstream}`]({url}){at} and is\n"
        f"> maintained separately here. Upstream is actively developed and is the better\n"
        f"> choice if you do not need the changes made for PDFluent. Licensed as upstream\n"
        f"> ({licence}); see `NOTICE` and `THIRD_PARTY_LICENSES.txt` for the full\n"
        f"> attribution.\n"
    )


def names_its_upstream(text: str, directory: str) -> bool:
    """The fork word and the upstream link, in the first part of the README.

    "The first part" is the whole point of LC7: a mention below the fold is the
    same as a mention in NOTICE. Twenty lines is generous -- a title, a badge row
    and a paragraph -- and still above where a reader stops.
    """
    head = "\n".join(text.splitlines()[:20])
    source, _ = UPSTREAM[directory]
    return MARK in head.lower() and SOURCE_URL[source] in head


def directories() -> list[str]:
    return sorted(n for n in FORK if (REPO / "crates" / n / "README.md").is_file())


def main() -> int:
    write = "--write" in sys.argv
    names = directories()
    if len(names) < MIN_FORKS:
        print(
            f"[forknotice] {len(names)} forked crate(s) with a README found, fewer "
            f"than {MIN_FORKS}. The provenance table is broken, not the fork gone.",
            file=sys.stderr,
        )
        return 1

    missing = []
    for name in names:
        path = REPO / "crates" / name / "README.md"
        text = path.read_text(encoding="utf-8")
        if names_its_upstream(text, name):
            continue
        if not write:
            missing.append(name)
            continue
        lines = text.split("\n")
        # directly under the title, above the description
        i = next((i for i, line in enumerate(lines) if line.startswith("# ")), -1) + 1
        while i < len(lines) and not lines[i].strip():
            i += 1
        lines.insert(i, notice(name).rstrip("\n") + "\n")
        path.write_text("\n".join(lines), encoding="utf-8")

    if missing:
        print(f"[forknotice] {len(missing)} forked crate(s) do not say in their "
              "README that they are a fork:", file=sys.stderr)
        for name in missing:
            print(f"  crates/{name}/README.md", file=sys.stderr)
        print(
            "\nWhat crates.io and GitHub show is the README. If it is not there the\n"
            "reader has to discover it, and then the publication becomes a story\n"
            "about what we did not mention.\n"
            "Fill in: python3 scripts/ci/every_fork_names_its_upstream.py --write",
            file=sys.stderr,
        )
        return 1

    print(f"[forknotice] OK: all {len(names)} forked crates name their upstream in "
          "their README, above the fold.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
