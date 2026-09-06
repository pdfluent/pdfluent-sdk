#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every forked crate's README sends you upstream, and claims no terms the
crate does not carry (#255, #219).

WHAT THE LETTER PROMISES, AND WHICH HALF OF IT IS MEASURED HERE

The message to the upstream author (#256) says, in words a reader can check in
five minutes: "every forked crate says in the first lines of its README that it
is a fork of your work, and that upstream is the better choice for anyone who
does not need our changes".

The first clause is `every_fork_names_its_upstream.py` (#219): the word "fork"
and the upstream URL, above the fold. This file does not repeat it -- two guards
on one rule means the rule has two definitions and drifts. It measures the
promise's other half, which nothing else does: that a reader who does not need
our changes is actually SENT to the original, in a paragraph that names it the
better choice and links it. Naming a fork and recommending against yourself are
different promises, and the letter makes both.

THE SECOND HALF, WHICH IS THE LICENCE ONE

Those same two READMEs said "Free for evaluation. Production use requires a
valid license" and "PDFluent extensions are governed by the PDFluent Commercial
License", while `crates/pdf-render/Cargo.toml` and `crates/pdf-font/Cargo.toml`
both declare `Apache-2.0 OR MIT` and `docs/release/canonical_licenses.toml`
records "Relicensing prohibited" for both. They are derivative works of a
permissive original: extending them far does not move them across the licence
line, so our extensions carry the upstream terms too (#345 fixed the same claim
in NOTICE; this is the last register that still made it).

A README is not a licence, but it is the file a reader believes. Telling someone
they need to buy a licence for code they already have under Apache-2.0 OR MIT is
wrong in the direction that costs the goodwill this whole ticket exists to keep.

SO THE RULE IS, FOR EVERY CRATE IN THE FORK REGISTER

  1. the README exists;
  2. some paragraph calls upstream "the better choice" and links it, so a reader
     who does not need our changes is sent to the original;
  3. a crate whose Cargo.toml declares a permissive licence does not claim
     commercial terms in its README.

Rule 3 reads the manifest rather than a list of crate names: the failure is the
DISAGREEMENT between what the package declares and what its README says, so a
crate that legitimately becomes ours would move both and stay green, and one
that quietly grew a commercial paragraph would not.

WHAT THIS DOES NOT CHECK

That the README names the fork at all -- `every_fork_names_its_upstream.py` owns
that -- nor whether the upstream URL is reachable, nor whether the fork claim is
true. The second needs the network (this runs in the commit hook); the third is
what
`the_fork_register_is_verifiable.py` and `een_forkpunt_wordt_op_inhoud_gecontroleerd.py`
do by content.
"""
from __future__ import annotations

import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REGISTER = REPO / "docs" / "UPSTREAM_FORKS.toml"

# Where each upstream lives. Keyed by the register's `upstream` field, so a fork
# added there with an upstream nobody wrote down here fails loudly rather than
# being skipped -- the shape that let two READMEs go four months without the
# attribution they were assumed to have.
#
# cff-parser is the one where the README's history and the register's upstream
# are different repositories, and both are right: our code is the CFF1 parser
# jrmuizel extracted from ttf-parser, so the README credits RazrFalcon for the
# original and the recommendation sends you to the crate you could switch to.
UPSTREAM_URL = {
    "hayro": "https://github.com/LaurenzV/hayro",
    "hayro-syntax": "https://github.com/LaurenzV/hayro",
    "hayro-interpret": "https://github.com/LaurenzV/hayro",
    "hayro-jbig2": "https://github.com/LaurenzV/hayro",
    "hayro-jpeg2000": "https://github.com/LaurenzV/hayro",
    "hayro-ccitt": "https://github.com/LaurenzV/hayro",
    "lopdf": "https://github.com/J-F-Liu/lopdf",
    "cff-parser": "https://github.com/jrmuizel/cff-parser",
}

# A register that yields fewer forks than this is a register that stopped
# describing the tree, and reporting OK over one crate is the failure this
# family of guards exists to refuse. Nine on 06-09-2026; set at the population,
# not below it.
FLOOR = 9

# What upstream is called in the sentence that sends a reader there. One phrase,
# because a guard that accepts five paraphrases accepts the one that says
# nothing.
RECOMMENDATION = "the better choice"

# A permissive licence in the manifest. Deliberately the same expression family
# license_boundary.py uses.
PERMISSIVE = re.compile(r"\b(MIT|Apache-2\.0|BSD-[0-9]|ISC|Zlib|Unlicense|CC0)\b")

# Claims a permissive crate cannot make. Narrow on purpose: "the PDFluent
# commercial Rust PDF SDK" describes the product these crates are used by, which
# is true and stays; "requires a valid licence" describes THIS crate's terms,
# which is not.
COMMERCIAL_CLAIM = (
    (re.compile(r"PDFluent Commercial Licen[cs]e", re.I),
     "names the PDFluent Commercial Licence as governing this crate"),
    (re.compile(r"requires a valid\b[^.\n]*licen[cs]e", re.I),
     "says a licence must be bought for this crate"),
    (re.compile(r"Free for evaluation", re.I),
     "offers the crate as an evaluation, which a permissive licence is not"),
    (re.compile(r"OEM Redistribution", re.I),
     "makes redistribution conditional on an add-on"),
)


def paragraphs(readme: str) -> list[str]:
    """One string per paragraph, with the line breaks taken out.

    Normalised rather than raw, because the sentence this file looks for is a
    sentence and Markdown may wrap it anywhere: `every_fork_names_its_upstream.py`
    writes the fork notice as a blockquote, so "the better choice" arrives as
    "the better\n> choice". A guard that reads the source bytes would call that
    missing and demand a second copy of a line the README already carries.
    """
    out = []
    for block in re.split(r"\n\s*\n", readme):
        lines = [re.sub(r"^\s*>\s?", "", line) for line in block.split("\n")]
        out.append(re.sub(r"\s+", " ", " ".join(lines)).strip())
    return out


def faults(crate: str, upstream: str, readme: str, licence: str | None) -> list[str]:
    """Everything wrong with one README. Pure, so the test can drive it."""
    url = UPSTREAM_URL.get(upstream)
    if url is None:
        return [f"{crate}: upstream {upstream!r} has no repository URL in "
                f"{pathlib.Path(__file__).name}; add it there so the "
                "recommendation can be checked"]

    broken: list[str] = []

    sends_upstream = [p for p in paragraphs(readme) if RECOMMENDATION in p]
    if not sends_upstream:
        broken.append(f"{crate}: no paragraph calls upstream {RECOMMENDATION!r}, "
                      "so a reader who does not need our changes is not sent there")
    elif not any(url in p for p in sends_upstream):
        broken.append(f"{crate}: the {RECOMMENDATION!r} paragraph does not link "
                      f"{url}")

    if licence and PERMISSIVE.search(licence):
        for pattern, what in COMMERCIAL_CLAIM:
            hit = pattern.search(readme)
            if hit:
                broken.append(
                    f"{crate}: Cargo.toml declares {licence!r} but the README "
                    f"{what} ({hit.group(0)!r})")
    return broken


def licence_of(crate_dir: pathlib.Path) -> str | None:
    manifest = crate_dir / "Cargo.toml"
    if not manifest.is_file():
        return None
    package = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
    value = package.get("license")
    return value if isinstance(value, str) else None


def main() -> int:
    if not REGISTER.is_file():
        print(f"FAIL: {REGISTER.relative_to(REPO)} is missing; there is no list "
              "of forks to check READMEs against.", file=sys.stderr)
        return 1

    forks = tomllib.loads(REGISTER.read_text(encoding="utf-8")).get("fork", [])
    if len(forks) < FLOOR:
        print(f"FAIL: the fork register lists {len(forks)} forks, below the floor "
              f"of {FLOOR}. Either forks were removed -- lower the floor in the "
              "same commit -- or this file stopped reading the register.",
              file=sys.stderr)
        return 1

    broken: list[str] = []
    for fork in forks:
        crate = str(fork["onze_crate"])
        crate_dir = REPO / "crates" / crate
        readme = crate_dir / "README.md"
        if not readme.is_file():
            broken.append(f"{crate}: crates/{crate}/README.md is missing, so the "
                          "fork is attributed nowhere a reader looks first")
            continue
        broken.extend(faults(crate, str(fork.get("upstream", "")),
                             readme.read_text(encoding="utf-8"),
                             licence_of(crate_dir)))

    if broken:
        for line in broken:
            print(f"FAIL: {line}", file=sys.stderr)
        print("\nThe message to the upstream author says every forked crate does "
              "this. Fix the README rather than the sentence.", file=sys.stderr)
        return 1

    print(f"[fork-readme] OK: {len(forks)} forked crates send a reader who does "
          "not need our changes to upstream as the better choice, and claim no "
          "terms their manifest does not carry.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
