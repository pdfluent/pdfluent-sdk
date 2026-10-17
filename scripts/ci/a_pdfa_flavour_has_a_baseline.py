#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A PDF/A flavour we say we convert to has a measured baseline behind it
(#443).

WHAT WENT WRONG WITHOUT IT

`pdfa_conformance_gate.py` converted to PDF/A-2b and graded PDF/A-2b, and every
conformance number this product has ever published came out of that one run.
At the same time:

  * `crates/pdf-capi/include/pdfluent.h` told a C caller that PDF/A-1B and
    PDF/A-3B "Converts and validates";
  * the website's published Rust snippet calls
    `doc.convert_to_pdfa(PdfAProfile::A1b)`;
  * `docs/CAPABILITY_REGISTER.md` said "Convert PDF to PDF/A" with no level
    named at all, which reads as "all of them".

Three conversion claims, one measurement. When the first flavour beyond 2b was
finally run, 1b scored 20 out of 30 — the pipeline does several things for part
2 that part 1 forbids. Nobody had lied; nobody had looked.

WHAT THIS CHECKS

1. Every flavour named as a **conversion target** in the capability register,
   the C header or the website's published snippets is a flavour
   `pdfa_flavours.py` declares, and has its baseline file on disk.
2. The C header says, for every PDF/A level it lists, which of the two it is:
   `Converts and validates` or `Validates only`. A level whose comment says
   neither is the ambiguity this whole guard exists to remove, so it is red and
   the fix is one word, not a baseline.
3. Every `conformance_baseline*.json` under `benchmarks/pdfa/` belongs to a
   declared flavour. A stray file is either a flavour somebody measured and
   forgot to declare, or a rename that left a corpse; both are worth a sentence.

THE REGISTER HALF IS ALMOST FREE, AND THAT IS FINE

`capability_register.py` now lists the levels *from the baseline files*, so a
register that names a level without one cannot normally be generated. Checking
it anyway costs nothing and catches the case that generation does not: a
hand-edited register, and the day somebody adds a level to the prose above the
table. The load-bearing half is the other two -- the C header and the website
are written by people, about what we sell.

WHAT IT DELIBERATELY DOES NOT CHECK

Whether a *validation* claim is measured. Validation is graded by
`pdf-compliance`'s own suite against veraPDF, document by document; it does not
need a corpus baseline, and demanding one would make PDF/A-4 support
unmentionable until somebody ran a round that proves nothing about it.

Exit codes:
    0  every conversion claim has a baseline
    1  a claim has none, a header comment does not say which it is, or a
       baseline file belongs to no declared flavour
    2  the guard could not run (a source file it reads is missing)
"""

from __future__ import annotations

import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
sys.path.insert(0, str(HERE))
from pdfa_flavours import (  # noqa: E402  one declaration of what a flavour is
    ALL_NAMES,
    FLAVOURS,
    VALIDATED_NOT_CONVERTED,
    spelling_variants,
)

REGISTER = REPO / "docs" / "CAPABILITY_REGISTER.md"
C_HEADER = REPO / "crates" / "pdf-capi" / "include" / "pdfluent.h"
SITE_SOURCES = (
    REPO / "docs" / "site-rust-blocks.json",
    REPO / "docs" / "site" / "snippets.json",
)
BASELINE_DIR = REPO / "benchmarks" / "pdfa"

# The two sentences the C header may use about a PDF/A level. Free text would
# put the guard in the business of reading English; two fixed phrases put the
# author in the business of deciding which one is true.
CONVERTS = "Converts and validates"
VALIDATES = "Validates only"

# A conversion call on the website, in the four spellings the published
# snippets actually use. Matching the *call* and not the level is what keeps a
# validation snippet -- `validate_pdfa(PdfAProfile::A1b)` -- from being read as
# a promise to produce one.
SITE_CONVERSION_CALL = re.compile(
    r"(?:convert_to_pdfa(?:_options)?|convert_to_pdf_a|PdfAFixOptions::for_level)"
    r"\s*\(\s*(?:[A-Za-z_]+::)?(A\d[abuABU])"
)


def shown(path: pathlib.Path) -> str:
    """A path as a reader wants it: repo-relative where that is possible.

    The fallback is not cosmetic. `test_a_pdfa_flavour_has_a_baseline.py` points
    the guard at a temporary tree, and `relative_to` raises on a path outside the
    repository -- so without this the guard's red path crashed instead of
    reporting, and only under test, which is the worst place to be wrong.
    """
    try:
        return str(path.relative_to(REPO))
    except ValueError:
        return str(path)


def flavour_of(token: str) -> str | None:
    """The declared flavour name for a spelling like `PDF/A-2b` or `A2B`."""
    for name in ALL_NAMES:
        if token in spelling_variants(name):
            return name
    return None


def claims_from_header(text: str, problems: list[str]) -> set[str]:
    """Levels the C header says it converts to, and a complaint per level whose
    comment says neither."""
    claimed: set[str] = set()
    for line in text.splitlines():
        tokens = re.findall(r"PDF/A-\d[ABUEFabuef]?", line)
        if not tokens:
            continue
        found = {name for token in tokens for name in [flavour_of(token)] if name}
        # A TOKEN NOBODY DECLARED IS THE CASE THIS GUARD EXISTS FOR, and it used
        # to be the one case it dropped (codex, #443): the comprehension threw
        # unmatched spellings away, so a header line adding `PDF/A-5B ... Converts
        # and validates` left `found` empty and the line was skipped -- the guard
        # passing exactly the undeclared conversion claim it is here to refuse.
        unknown = sorted({token for token in tokens if flavour_of(token) is None})
        if CONVERTS in line:
            claimed |= found
            for token in unknown:
                problems.append(
                    f"{shown(C_HEADER)}: the comment promises conversion to {token}, which "
                    "scripts/ci/pdfa_flavours.py does not declare at all. Declare it there "
                    "with a baseline, or list it as validate-only with the reason: "
                    + line.strip()
                )
        elif VALIDATES in line:
            continue
        else:
            named = ", ".join(sorted(found) + unknown)
            problems.append(
                f"{shown(C_HEADER)}: the comment for "
                f"{named} says neither {CONVERTS!r} nor {VALIDATES!r}, "
                "so a C caller cannot tell whether we produce it: " + line.strip()
            )
    return claimed


def claims_from_register(text: str) -> set[str]:
    """Levels the capability register names under PDF to PDF/A.

    Only that section: the register mentions PDF/A elsewhere (the gap list, the
    method table) in sentences that are not conversion claims.
    """
    section = []
    inside = False
    for line in text.splitlines():
        if line.startswith("### "):
            inside = line.startswith("### PDF to PDF/A")
        if inside:
            section.append(line)
    claimed = set()
    for token in re.findall(r"PDF/A-\d[ABUEFabuef]?", "\n".join(section)):
        name = flavour_of(token)
        if name:
            claimed.add(name)
    return claimed


def claims_from_site(problems: list[str]) -> set[str]:
    claimed: set[str] = set()
    for source in SITE_SOURCES:
        if not source.is_file():
            problems.append(f"{shown(source)} is missing; the website claims cannot be read")
            continue
        for token in SITE_CONVERSION_CALL.findall(source.read_text(errors="replace")):
            name = flavour_of(token)
            if name:
                claimed.add(name)
            else:
                # The same hole the C header had, on the side that customers
                # read (codex, #443): a published snippet calling
                # `convert_to_pdfa(PdfAProfile::A5b)` matched the call, resolved
                # to no declared flavour, and was dropped -- a conversion promise
                # with neither a declaration nor a baseline, passing in silence.
                problems.append(
                    f"{shown(source)} promises conversion to {token}, which "
                    "scripts/ci/pdfa_flavours.py does not declare at all. Declare it there "
                    "with a baseline, or take the call out of the published snippet."
                )
    return claimed


def main() -> int:
    for path in (REGISTER, C_HEADER):
        if not path.is_file():
            print(f"a_pdfa_flavour_has_a_baseline: {path} is missing", file=sys.stderr)
            return 2

    problems: list[str] = []

    claims: dict[str, set[str]] = {
        "the capability register": claims_from_register(REGISTER.read_text(errors="replace")),
        "the C header": claims_from_header(C_HEADER.read_text(errors="replace"), problems),
        "the website's published snippets": claims_from_site(problems),
    }

    for where, named in sorted(claims.items()):
        for name in sorted(named):
            if name in VALIDATED_NOT_CONVERTED:
                problems.append(
                    f"{where} promises conversion to PDF/A-{name}, and this converter does not "
                    f"produce it: {VALIDATED_NOT_CONVERTED[name]}. Withdraw the claim or build it."
                )
                continue
            if not FLAVOURS[name].is_measured():
                problems.append(
                    f"{where} promises conversion to {FLAVOURS[name].iso}, and "
                    f"{shown(FLAVOURS[name].baseline)} does not exist. "
                    "A conformance level nothing has measured is a claim, not a capability: "
                    "run `pdfa_conformance_gate.py --flavour "
                    f"{name} --update-baseline` on the measuring host, or withdraw the claim."
                )

    declared = {f.baseline.name for f in FLAVOURS.values()}
    for found in sorted(BASELINE_DIR.glob("conformance_baseline*.json")):
        if found.name not in declared:
            problems.append(
                f"{shown(found)} is a conformance baseline for no declared flavour. "
                "Declare it in scripts/ci/pdfa_flavours.py or delete it — a baseline nothing "
                "reads is a number nobody judges."
            )

    if problems:
        print("a_pdfa_flavour_has_a_baseline: RED", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    measured = [FLAVOURS[n].iso for n in FLAVOURS if FLAVOURS[n].is_measured()]
    named = sorted({n for s in claims.values() for n in s})
    print(
        f"a_pdfa_flavour_has_a_baseline: {len(named)} flavour(s) claimed "
        f"({', '.join(named) or 'none'}), {len(measured)} measured "
        f"({', '.join(measured) or 'none'})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
