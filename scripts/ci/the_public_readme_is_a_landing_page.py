#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The README a stranger lands on still answers the three questions.

WHY A GUARD AND NOT A REVIEW

`README.md` in this tree is the front page of `pdfluent/pdfluent-sdk`: the
seeding publishes it unchanged, and it is the first and for most visitors the
only page they read. #205 settled what the first screen owes them -- what this
does, how to install it in their language, and how it compares -- and the way
that decays is not somebody deleting the file. It is a rewrite that keeps the
prose and drops a row, and nobody notices because the page still reads well.

WHAT IS CHECKED, AND WHAT IS DELIBERATELY NOT

Three things, each one a row that has to survive a rewrite:

  1. the badge row: one badge per registry we publish to, plus the licence and
     the checks. A registry with no badge is a language a reader cannot tell we
     support.
  2. the install lines: one per binding, the literal command. These are the
     lines people copy, and a stale one costs a first impression.
  3. the star reminder and the note that says where its image comes from.
     #205 put the reminder below the fold on purpose; the note exists so nobody
     later finds a third-party host in our README and has to guess.
  4. the hero image: referenced from the first screen, present in the tree, a
     real PNG, and inside the byte budget. #205 asked for real output on the
     first screen, and the three ways that quietly stops being true are a moved
     file, a placeholder written in its place, and an image so heavy that the
     reader has scrolled past before it paints.

Not checked: wording, order, length, or anything about the prose. A guard that
insists on sentences is a guard that gets switched off the first time somebody
writes a better one.

Exit codes:
    0  the landing page carries all three rows
    1  a row is missing -- and it says which
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
README = REPO / "README.md"

# The registries this repository publishes to. Each one needs a badge that
# points at its own package page: a badge is how a reader in that language
# learns this is for them too.
BADGES = {
    "crates.io": "crates.io/crates/pdfluent",
    "PyPI": "pypi.org/project/pdfluent",
    "npm": "npmjs.com/package/@pdfluent/node",
    "NuGet": "nuget.org/packages/PDFluent",
    "Maven": "central.sonatype.com/artifact/com.pdfluent/pdfluent",
    "licence": "LICENSE",
    "checks": "actions/workflows/public-pull-request.yml",
}

# The install lines, literally. The command as a reader copies it -- not a
# pattern that a paraphrase would still satisfy.
INSTALL = [
    "cargo add pdfluent",
    "pip install pdfluent",
    "npm install @pdfluent/node",
    "npm install @pdfluent/sdk-wasm",
    "dotnet add package PDFluent",
    "<artifactId>pdfluent</artifactId>",
]

# The note that accounts for the third-party hosts serving images into this
# page. `img.shields.io` is the only one left and it stays accounted for: a
# host in our README that nobody wrote down is the state #205 asked us not to
# leave behind.
ASSET_NOTE = "External assets in this README"

# The star reminder that used to sit here (#205, starme.dev's free tier) is
# gone: it displayed a zero and a badge that advertises how few stars a project
# has is worse than no badge. Named so it cannot come back by habit -- matched
# by host and by the service's name, because the same widget under a new
# hostname is the same widget.
BANNED_ASSETS = ("star-prompt-worker.lnmput.workers.dev", "starme.dev")

# The image on the first screen, and what it may cost. A landing page that makes
# a reader wait for its own picture has spent the attention it was there to win;
# 400 kB is the ceiling #205 set, and `cargo run --example readme_hero` refuses
# to write a file over it, so the two agree by construction.
HERO = "docs/readme-hero.png"
HERO_MAX_BYTES = 400 * 1024
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def main() -> int:
    if not README.is_file():
        print(f"[readme] FATAL: {README} is missing, and that is not a pass",
              file=sys.stderr)
        return 1

    text = README.read_text(encoding="utf-8")

    # FLOOR. A guard that reads an empty file finds nothing missing in it, and
    # reports the same green as a guard reading the real page. The published
    # README has never been under 60 lines; 40 is the floor because it is the
    # number below which the page cannot carry three rows at all.
    lines = text.splitlines()
    if len(lines) < 40:
        print(f"[readme] FATAL: README.md is {len(lines)} line(s), below the floor "
              "of 40. A page that short is not a landing page, and checking rows "
              "inside it would report on nothing.", file=sys.stderr)
        return 1

    missing: list[str] = []

    for name, target in BADGES.items():
        if target not in text:
            missing.append(f"the {name} badge (nothing points at {target})")

    # The badges have to stand above the fold, where the first screen is. The
    # first 30 lines is that screen on a normal window; a badge row pushed under
    # the features is a badge row nobody sees.
    head = "\n".join(lines[:30])
    if not re.search(r"!\[[^\]]*\]\(https://img\.shields\.io/", head):
        missing.append("a shields.io badge in the first 30 lines -- the row "
                         "exists somewhere but not on the first screen")

    for line in INSTALL:
        if line not in text:
            missing.append(f"the install line `{line}`")

    # The hero, in four steps: named on the first screen, on disk, actually a
    # PNG, and within budget. Each step is a separate way for the picture to
    # stop being there while the page still reads as though it is.
    if HERO not in head:
        missing.append(f"a reference to {HERO} in the first 30 lines -- #205 asked "
                        "for real output on the first screen")
    else:
        hero = REPO / HERO
        if not hero.is_file():
            missing.append(f"{HERO} itself: the README points at it and the tree "
                            "does not have it")
        else:
            raw = hero.read_bytes()
            if not raw.startswith(PNG_MAGIC):
                missing.append(f"{HERO} is not a PNG -- the first eight bytes are "
                                f"{raw[:8]!r}, so whatever replaced it will not render")
            elif len(raw) > HERO_MAX_BYTES:
                missing.append(f"a hero within budget: {HERO} is {len(raw) // 1024} kB, "
                                f"over the {HERO_MAX_BYTES // 1024} kB ceiling")

    if ASSET_NOTE not in text:
        # A third-party host in our README that nobody wrote down.
        missing.append(f"the note that accounts for its external assets "
                       f"(\"{ASSET_NOTE}\")")

    for banned in BANNED_ASSETS:
        if banned in text:
            missing.append(f"no star badge: {banned} is back in the README. It "
                           f"showed a zero, which advertises the opposite of "
                           f"what a badge is for, and it was removed on "
                           f"07-09-2026")

    if missing:
        print(f"[readme] {len(missing)} thing(s) the landing page owes a reader "
              "are missing:", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        print("  #205 decided what the first screen answers: what this does, how "
              "to install it, how it compares.", file=sys.stderr)
        return 1

    print(f"[readme] OK: {len(BADGES)} badge(s), {len(INSTALL)} install line(s), "
          f"a {(REPO / HERO).stat().st_size // 1024} kB hero on the first screen, "
          "the note that accounts for its external assets, and no star badge.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
