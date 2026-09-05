#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The order form and the commercial licence say the same thing (#220).

WHY THIS IS A GATE AND NOT A PROOFREAD

`LICENSE-COMMERCIAL` §10 is unusual for a licence: it does not contain the
agreement. It says the agreement is this text plus "an order form signed by both
parties", and that where the two conflict **the order form wins**. So the form
is the document with the last word, and until 5 September 2026 it did not exist
-- §10 pointed at nothing, and §6 deferred support terms to the same nothing.

That is the failure this guard is built around. The two files are written months
apart, by different hands, and nothing connects them: a grant added to §2 with no
row on the form is a right the licensee never buys, and a row on the form with no
grant behind it is a right we sell and cannot deliver. Neither shows up anywhere.
There is no build to fail, and the first reader who notices is a lawyer holding
both documents on the day the money is already paid.

WHAT IT CHECKS

  the parties      the form names the licensor exactly as the licence heading
                   does. `LICENSE-MIT` in this repository said "PDFluent BV" --
                   an entity that does not exist -- while `LICENSE` beside it
                   named Innovation Trigger B.V. (#220). A signature under a
                   name that is not a legal person binds nobody.
  the version      the form is drawn against a named version and date of the
                   licence. A form quoting v1.0 of 2 May 2026 sells licence keys
                   and a 30-day expiry, neither of which exists.
  the scope        one row per grant in §2, each carrying that grant's own first
                   sentence, and no row for a grant that is not there. This is
                   the check the other two exist to protect.
  the precedence   the form states that it prevails, because §10 says it does.
                   Two documents and no rule between them is the ordinary way a
                   commercial dispute starts.
  the references   every repository path the form names in backticks exists. A
                   form that points a buyer at `LICENSE-COMMERCIAL` is worth
                   nothing if the file was renamed.

FLOORS, because agreement over nothing reads exactly like agreement

A §2 that parses to zero grants agrees with a form that has zero rows, and a
truncated form agrees with everything it no longer mentions. So the licence must
yield at least MIN_GRANTS grants, and the form must carry every heading in
VERPLICHTE_KOPPEN. The lesson is `license_boundary.py`'s: a map is complete on
the day it is written and stops being so in silence.

Usage:
    python3 scripts/ci/the_order_form_agrees_with_the_licence.py

Exit codes:
    0  the form and the licence agree
    1  they do not, or one of them fell under a floor
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LICENCE = "LICENSE-COMMERCIAL"
FORM = "docs/licensing/order-form.md"

# See the FLOOR paragraph above.
MIN_GRANTS = 3
VERPLICHTE_KOPPEN = (
    "## 1. Parties",
    "## 2. Licensed software and versions",
    "## 3. Licensed scope",
    "## 4. Fee",
    "## 5. Term",
    "## 6. Precedence",
    "## 9. Signatures",
)

# The heading of the licence: the line naming the version and its date, and the
# line naming the licensor. Both are read from the licence and demanded of the
# form, never written down twice -- a constant repeated here would be a second
# answer to a question the licence already answers.
VERSIE = re.compile(r"^Version\s+(\S+)\s+[-—]+\s+(.+?)\s*$", re.M)
ENTITEIT = re.compile(r"^(Innovation Trigger B\.V\..*?)\s*$", re.M)
# The form's own declaration of who is licensing. Read as a line rather than as
# a substring anywhere in the file: the entity is named several times on the
# form, so "it appears somewhere" stays true while the line above the signature
# block names somebody else entirely.
FORM_LICENSOR = re.compile(r"^Licensor:\s*(.+?)\s*$", re.M)

# `  a. Distribution of ...` under section 2, up to the blank line that ends it.
GRANT = re.compile(r"^ {2}([a-z])\.\s+(.+?)(?=^ {2}[a-z]\.\s|\Z)", re.M | re.S)

# A backticked token that looks like a path into this repository. `§2` and
# `Tier::Trial` do not match; `NOTICE`, `LICENSE-COMMERCIAL` and
# `docs/licensing/boundary.toml` do.
PAD = re.compile(r"`([A-Za-z0-9][A-Za-z0-9_./-]*(?:/[A-Za-z0-9_./-]+|\.[a-z]{2,4}|[A-Z-]{3,}))`")

fouten: list[str] = []


def klaag(regel: str) -> None:
    fouten.append(regel)


def genormaliseerd(tekst: str) -> str:
    """One space between words, so a rewrap is not a difference."""
    return " ".join(tekst.split())


def eerste_zin(tekst: str) -> str:
    """The first sentence of a grant, which is the part a form row can carry.

    §2(d) continues with a paragraph explaining that an offline deployment needs
    no permission. Demanding that on the form would make the row unreadable and
    the check brittle for no gain: what a row has to reproduce is the right, and
    the right is the first sentence.
    """
    zin = genormaliseerd(tekst).split(". ")[0]
    return zin.rstrip(".")


def lees(rel: str) -> str | None:
    p = REPO / rel
    if not p.is_file():
        klaag(f"{rel} is missing. LICENSE-COMMERCIAL §10 makes the order form "
              "half of the agreement; without it the licence points at nothing.")
        return None
    return p.read_text(encoding="utf-8")


def sectie(tekst: str, van: str, tot: str) -> str:
    """The body between two section headings of the licence."""
    begin = tekst.find(van)
    if begin < 0:
        return ""
    eind = tekst.find(tot, begin)
    return tekst[begin:eind if eind > 0 else len(tekst)]


def main() -> int:
    licentie = lees(LICENCE)
    formulier = lees(FORM)
    if licentie is None or formulier is None:
        return afsluiten()

    # --- what the licence says about itself
    v = VERSIE.search(licentie)
    if not v:
        klaag(f"{LICENCE} carries no `Version <n> — <date>` line, so there is "
              "nothing for the form to be drawn against.")
        return afsluiten()
    versie, datum = v.group(1), v.group(2)

    e = ENTITEIT.search(licentie)
    if not e:
        klaag(f"{LICENCE} names no licensor on a line of its own.")
        return afsluiten()
    entiteit = e.group(1).rstrip(".")

    grants = GRANT.findall(sectie(licentie, "2. What you get", "3. What is not covered"))
    if len(grants) < MIN_GRANTS:
        klaag(f"{LICENCE} §2 parsed to {len(grants)} grant(s), under the floor of "
              f"{MIN_GRANTS}. A section that reads as empty agrees with any form, "
              "which is the one verdict this guard must never print.")
        return afsluiten()

    # --- the form is drawn against THIS licence
    plat = genormaliseerd(formulier)
    licensor = FORM_LICENSOR.search(formulier)
    if not licensor:
        klaag(f"{FORM} carries no `Licensor:` line, so it does not say who is "
              "granting what the licence grants.")
    elif licensor.group(1).rstrip(".") != entiteit:
        klaag(f"{FORM} names the licensor as {licensor.group(1)!r} where "
              f"{LICENCE} names {entiteit!r}. A form signed by an entity the "
              "licence does not name binds a party that is not in the "
              "agreement -- and where that entity does not exist, nobody.")
    for wat, waarde in (("version", versie), ("date", datum)):
        if waarde not in plat:
            klaag(f"{FORM} does not name the licence {wat} {waarde!r}. An order "
                  "form that does not say which licence text it is drawn against "
                  "is signed against whichever one the reader has.")

    # --- one row per grant, carrying that grant's own words
    rijen = dict(re.findall(r"^\|\s*([a-z])\s*\|\s*(.+?)\s*\|", formulier, re.M))
    for letter, tekst in grants:
        zin = eerste_zin(tekst)
        if letter not in rijen:
            klaag(f"{FORM} has no scope row for §2({letter}) {zin!r}. A right the "
                  "licence grants and the form never asks about is sold by "
                  "accident or withheld by accident; the form cannot say which.")
        elif zin not in genormaliseerd(rijen[letter]):
            klaag(f"{FORM} row ({letter}) does not carry the grant it stands for.\n"
                  f"      licence: {zin}\n"
                  f"      form:    {genormaliseerd(rijen[letter])}")
    verweesd = sorted(set(rijen) - {letter for letter, _ in grants})
    if verweesd:
        klaag(f"{FORM} has scope row(s) {', '.join(verweesd)} that §2 of "
              f"{LICENCE} does not grant. The form is the document that "
              "prevails, so it would grant it -- out of a licence that cannot "
              "deliver it.")

    # --- the rule between the two documents, on both documents
    voorrang = "Where they conflict, the order form prevails"
    if voorrang not in genormaliseerd(licentie):
        klaag(f"{LICENCE} §10 no longer says the order form prevails. If that "
              "changed deliberately, the form's section 6 changes in the same "
              "commit; this guard exists so it cannot change in only one.")
    if voorrang not in plat:
        klaag(f"{FORM} does not state that it prevails over {LICENCE}. Two "
              "documents that are one agreement need the rule written where "
              "both signatories read it.")

    # --- everything the form points at is really there
    for pad in sorted(set(PAD.findall(formulier))):
        if not (REPO / pad).exists():
            klaag(f"{FORM} points at `{pad}`, which is not in the tree.")

    for kop in VERPLICHTE_KOPPEN:
        if kop not in formulier:
            klaag(f"{FORM} is missing `{kop}`. A form that has lost a section "
                  "agrees with the licence about everything it no longer says.")

    if not fouten:
        print(f"[order-form] {FORM} agrees with {LICENCE} v{versie} ({datum}) on "
              f"{len(grants)} grant(s), the licensor, the precedence rule and "
              f"{len(VERPLICHTE_KOPPEN)} section(s)")
    return afsluiten()


def afsluiten() -> int:
    if not fouten:
        return 0
    print(f"[order-form] {len(fouten)} problem(s):", file=sys.stderr)
    for f in fouten:
        print(f"  - {f}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
