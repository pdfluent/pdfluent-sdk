#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which PDF/A flavours this product converts to, and where each one's
measured baseline lives (#443).

WHY THIS IS ONE FILE AND NOT FIVE LITERALS

Until now "PDF/A" meant PDF/A-2b everywhere it was measured, and something
wider everywhere it was claimed. `pdfa_conformance_gate.py` converted to 2b and
graded 2b; the C header advertised conversion to 1b, 2b and 3b; the website
shows `convert_to_pdfa(PdfAProfile::A1b)` in a published snippet. Three
statements about the same capability, one of them measured.

So the flavour is data, in one place: the gate reads it to know what to convert
to and which baseline file to judge against, the capability register reads it to
list what has been measured, and `a_pdfa_flavour_has_a_baseline.py` reads it to
refuse a claim with no baseline behind it. Adding a sixth flavour is an entry
here plus a measured baseline -- and without the baseline the guard says so,
which is the whole point.

WHY 2b's BASELINE FILE KEEPS ITS OLD NAME

`benchmarks/pdfa/conformance_baseline.json` predates the other four and is
referenced from `.gitlab-ci.yml`, the round protocol and two handover
documents. Renaming it would have been tidier and would have broken every
reference for the sake of a suffix, so the mapping is explicit rather than
mechanical.

WHAT IS DELIBERATELY ABSENT

  1a, 2a, 3a  need a structure tree the converter does not build. Level A is a
              tagging problem, not a conformance-fixup problem, and a baseline
              of zeroes measured against a target we do not aim at teaches
              nobody anything.
  4, 4e, 4f   ISO 19005-4 is a validation target here and not a conversion
              target: `pdf_compliance::PdfALevel` knows the three, and
              `pdf_manip::pdfa_xmp::PdfAConformance` -- what `convert_bytes`
              takes -- has no variant for any of them. There is nothing to
              measure until the converter can emit one.

Both lists are in `VALIDATED_NOT_CONVERTED`, with the reason, so the guard can
tell "we do not convert to this" apart from "we forgot to measure this".
"""

from __future__ import annotations

import pathlib
from dataclasses import dataclass

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
BASELINE_DIR = REPO / "benchmarks" / "pdfa"


@dataclass(frozen=True)
class Flavour:
    """One conversion target: how it is spelled, and where its evidence is."""

    name: str
    """veraPDF's spelling, and the one `--flavour` takes: `1b`, `2b`, …"""

    iso: str
    """How the standard writes it, for anything a person reads."""

    part: int
    """ISO 19005 part number."""

    letter: str
    """Conformance level letter, upper case as the standard writes it."""

    baseline_name: str
    """File name under `benchmarks/pdfa/`."""

    @property
    def baseline(self) -> pathlib.Path:
        return BASELINE_DIR / self.baseline_name

    def is_measured(self) -> bool:
        """A flavour is measured when its baseline file exists.

        Not "when it is listed here": listing it is a plan, the file is the
        evidence, and the difference between the two is exactly what went
        unnoticed for the four flavours nobody had ever run.
        """
        return self.baseline.is_file()


def _baseline_name(name: str) -> str:
    return "conformance_baseline.json" if name == "2b" else f"conformance_baseline_{name}.json"


FLAVOURS: dict[str, Flavour] = {
    name: Flavour(
        name=name,
        iso=f"PDF/A-{name[0]}{name[1]}",
        part=int(name[0]),
        letter=name[1].upper(),
        baseline_name=_baseline_name(name),
    )
    for name in ("1b", "2b", "2u", "3b", "3u")
}

DEFAULT_FLAVOUR = "2b"
"""The flavour every published conformance number was measured at before #443.

It stays the default so that a caller who names no flavour gets the number the
baseline, the CI jobs and the round protocol already talk about, rather than a
different one under the same name.
"""

CALLER_REACHABLE: frozenset[str] = frozenset({"1b", "2b", "3b"})
"""The levels a customer can actually ask for today (codex, #443).

`pdfluent::PdfAProfile` has three variants (`crates/pdfluent/src/compliance.rs`)
and the C ABI's `convert_conformance` returns `None` for 2u and 3u on purpose
(`crates/pdf-capi/src/types.rs`): a caller asking for 2u used to get a file whose
XMP said 2b, and an accepted level that is quietly something else is worse than a
refusal, because the caller archives it.

So 2u and 3u are measured against `pdf_manip::pdfa::convert_bytes` -- the
shipping pipeline -- through a level no binding exposes yet. That is worth
measuring: #445 is the work of getting those two right, and a number is how it
will be judged. It is not worth *implying*, so the capability register marks
them, and this set is what it reads.
"""

VALIDATED_NOT_CONVERTED: dict[str, str] = {
    "1a": "level A needs a structure tree the converter does not build",
    "2a": "level A needs a structure tree the converter does not build",
    "3a": "level A needs a structure tree the converter does not build",
    "4": "ISO 19005-4 is a validation target; PdfAConformance has no variant to convert to",
    "4e": "ISO 19005-4 is a validation target; PdfAConformance has no variant to convert to",
    "4f": "ISO 19005-4 is a validation target; PdfAConformance has no variant to convert to",
}

ALL_NAMES: tuple[str, ...] = tuple(FLAVOURS) + tuple(VALIDATED_NOT_CONVERTED)


def baseline_path(name: str) -> pathlib.Path:
    """Where `name`'s baseline lives, whether or not it has been measured yet."""
    if name not in FLAVOURS:
        raise KeyError(f"{name!r} is not a conversion target ({', '.join(FLAVOURS)})")
    return FLAVOURS[name].baseline


def measured_flavours() -> list[str]:
    """Flavours with a baseline file on disk, in the order they are declared."""
    return [name for name, f in FLAVOURS.items() if f.is_measured()]


def unmeasured_flavours() -> list[str]:
    """Conversion targets declared here that nothing has measured yet."""
    return [name for name, f in FLAVOURS.items() if not f.is_measured()]


def spelling_variants(name: str) -> tuple[str, ...]:
    """Every way a document or a header spells this flavour.

    `PDF/A-2b`, `PDF/A-2B` and the Rust/C identifier `A2b`/`A2B` are the same
    claim; a guard that only knew one of them would pass the other three.
    Plain `4` has no letter, so it has two spellings rather than four.
    """
    part, letter = name[0], name[1:]
    return tuple(
        dict.fromkeys(
            (
                f"PDF/A-{part}{letter.lower()}",
                f"PDF/A-{part}{letter.upper()}",
                f"A{part}{letter.lower()}",
                f"A{part}{letter.upper()}",
            )
        )
    )
