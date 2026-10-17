#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The retired offer may not leave this repository again (claim B28, #479).

WHY A SECOND OFFER GUARD

`one_offer_in_every_document.py` holds the four documents that *state* the
offer -- `LICENSE-COMMERCIAL`, `docs/licensing.md`, the order form and
`NOTICE` -- against `docs/licensing/offer.toml`. It is deliberately narrow: it
compares prices, so it can only read documents that quote prices.

The offer leaves here by a second road, and nothing was watching it. A crate
README becomes the front page on crates.io, a `pyproject.toml` becomes the PyPI
page, a nuspec becomes the NuGet listing, and `docs/hero/hero_document.html`
becomes the image on the first screen of the public SDK repository. None of
those quote the price list, so none of them fail the offer guard -- and on
09-09-2026 an external inventory (#479) found all of them still selling the
model withdrawn on 06-09-2026: "Free for evaluation. Production use requires a
valid license" on eleven registry READMEs, `Licensing.ActivateKey("tier:
enterprise")` in the .NET README for a call that no longer exists, a 30-day
key on the org profile and in `pdfluent/examples`, and -- read by more people
than all of them together -- a hero image invoicing four PDFluent SDK
*developer seats* at EUR 2,400 a year, under our own company name.

A published version is immutable. crates.io will not take that README back, so
the cost of a slip is a permanent page that AI assistants then quote back at
buyers, which is exactly how #479 was found. The gate is cheap; the mistake is
not reversible.

WHAT IT CHECKS

  the vocabulary  no published file uses a word from the retired model:
                  a per-seat or perpetual licence, "pay once", an evaluation
                  period or evaluation licence, a licence key, an activation
                  call, a tier, or "production use requires a licence". Read
                  per sentence, because every one of these words is allowed --
                  and several of these documents exist -- to DENY it.
  the amounts     no published file states one of the retired prices in
                  currency. Compared on digits, so 1,099 and 1099 are one
                  number.
  the URL         the pricing link is `pdfluent.com/sdk/pricing`. The retired
                  model lived at `/pricing` and `/trial`; both still resolve,
                  so a stale link does not 404, it just sells the wrong thing.

FLOORS, because a scan that reads nothing approves everything

Every path in `PUBLISHED` must exist and be non-empty -- a renamed README must
break this guard rather than silently leave the scan. Every crate whose
`Cargo.toml` licence is the dual AGPL one must have a README that names the
current pricing page: the retired wording cannot be got rid of by deleting the
licence section instead of correcting it, which is the cheapest way to make a
vocabulary scan go green while telling a reader nothing.

Usage:
    python3 scripts/ci/no_retired_price_in_a_published_readme.py

Exit codes:
    0  nothing published states the retired offer
    1  something does
"""
from __future__ import annotations

import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

# The page every published surface must send a buyer to. The retired model sold
# from `/pricing` and `/trial`, and both of those still answer.
PRICING_URL = "pdfluent.com/sdk/pricing"

# Everything in this repository that becomes a page somebody else hosts: a
# registry listing, a public repository, or the README image. Directories are
# read whole; a glob is read as a glob. Anything matched must exist.
PUBLISHED: tuple[str, ...] = (
    "README.md",
    "crates/*/README.md",
    "bindings/*/README.md",
    "bindings/dotnet/src/PDFluent/README.md",
    "crates/*/Cargo.toml",
    "crates/pdf-node/package.json",
    "crates/pdf-python/pyproject.toml",
    "crates/pdf-java/pom.xml",
    "bindings/java/pom.xml",
    "bindings/dotnet/src/PDFluent/PDFluent.csproj",
    "packaging/chocolatey/pdfluent.nuspec",
    "docs/hero/hero_document.html",
    "docs/hero/hero_composite.html",
    "pdfluent-examples/**/*.md",
    "pdfluent-examples/**/*.rs",
    "pdfluent-examples/**/*.py",
    "pdfluent-examples/**/*.cs",
    "pdfluent-examples/**/*.ts",
    "docs/en/*.md",
    "docs/cookbook/*.md",
    # The CLI's own `--help` and `doctor` output. As published as a README and
    # read by more people: on 09-09-2026 both printed "PDFluent Commercial
    # License (evaluation use permitted)" while every README beside them had
    # said AGPL-or-commercial for three days.
    "crates/pdfluent-cli/src/*.rs",
)

# Files whose subject IS the retired model: the record of what changed, and the
# handover that quotes the old site copy so a reader can see what was replaced.
# Deleting them would delete the evidence; they are not published.
EXEMPT: frozenset[str] = frozenset({
    "docs/licensing/website-copy-handoff-t4.md",
})

# The vocabulary of the model withdrawn on 06-09-2026 (B02, retired by B28).
# Second element is why it may not be published.
RETIRED_WORDS: tuple[tuple[str, str], ...] = (
    (r"\bperpetual\b",
     "the commercial licence is yearly; only a sentence about what buyers "
     "before 06-09-2026 keep may call a licence perpetual"),
    (r"\bpay once\b",
     "nothing is sold as a one-off payment"),
    (r"\bper[- ]seat\b|\bdeveloper seats?\b|\bper[- ]developer\b",
     "the commercial licence is per organisation and OEM is per product; "
     "there are no seats"),
    (r"\bevaluation licen[cs]e\b|\bfree for evaluation\b"
     r"|\bevaluation (key|period|build|terms|use|copy)\b",
     "there is no evaluation: the AGPL build is the complete product"),
    (r"contact (us |sales )?(at \S+ )?for (a )?(quote|pricing|a price)"
     r"|\brequest a quote\b|\bcontact sales\b|\bpricing on request\b",
     "every price is published and every option is a button; there is no quote "
     "to ask for (B28: no seats, no keys, no quotes)"),
    (r"\b30[- ]day (evaluation|trial|licence|license|key)\b|\btrial mode\b",
     "there is no trial period"),
    (r"\blicen[cs]e key\b|\bproduct key\b|\bactivation code\b",
     "no code reads a key (#226); only a denial may mention one"),
    (r"\bActivateKey\b|\bactivateKey\b|\bactivate_license\b|\bset_license_key\b"
     r"|\bActivateFile\b|\bactivate_license_key\b",
     "the activation call was removed with the proprietary model (#226)"),
    (r"\btier:[a-z]+\b", "there are no tiers, so there is no tier string to pass"),
    (r"\b(paid|licen[cs]ed)[- ]tier\b|\bEffectiveTier\b|\bLicenseTier\b",
     "there are no tiers; every feature works in every build"),
    (r"requires a (valid )?(commercial )?licen[cs]e"
     r"|licen[cs]e is required for production"
     r"|production use requires",
     "production use of the AGPL build requires nothing but the AGPL"),
    (r"\bOEM Redistribution\b",
     "OEM is a licence of its own, not an add-on to another one"),
    (r"pdfluent\.com/trial",
     "there is no trial page to send anyone to"),
)

# Amounts the repository sold before 06-09-2026, plus the two subscription
# prices retired before those (B02's own predecessor, CLAIMS.md bronregel).
# Not "wrong numbers": prices that were true, which is exactly why a reader
# cannot spot a stale one.
RETIRED_AMOUNTS = ("699", "1099", "2199", "4399", "1999", "1499", "12000",
                   "1990", "5900")

AMOUNT = re.compile(r"(?:EUR|€|&euro;)\s?(\d[\d.,]*)")

# Every word above is allowed in a sentence that denies it, and several of these
# documents exist partly to deny them. What is forbidden is the offer, not the
# word. Same list as `one_offer_in_every_document.py`, for the same reason.
DENIAL = re.compile(
    r"\b(no|not|never|nothing|none|without|removed|withdrawn|retired|neither|"
    r"does not|do not|is not|are not|stopped|ceased|inert|read by nothing|"
    r"was|were|used to|until|former|formerly|previously|old|legacy)\b",
    re.I,
)

# A sentence about what buyers before 06-09-2026 keep is the one true use of
# "perpetual" left.
PERPETUAL_OK = ("were perpetual", "was perpetual", "stay perpetual",
                "sold as a perpetual licence")

problems: list[str] = []


def digits(amount: str) -> set[str]:
    """The digit forms an amount could be, so every writing of it is one number.

    A price is written four ways in this tree -- `699`, `EUR 699.00`,
    `1,099` and `1.099,00` -- and a scan that compares the written form lets
    three of them through. Compare on digits, and read a trailing two-digit
    fraction as a fraction: `699.00` is 699, not 69900. Measured 09-09-2026:
    the first draft of this guard passed `&euro; 699.00` in the README hero,
    which is the exact place the price came back.
    """
    bare = "".join(c for c in amount if c.isdigit())
    forms = {bare}
    if re.search(r"[.,]\d{2}$", amount) and len(bare) > 2:
        forms.add(bare[:-2])
    return forms


def sentences(text: str) -> list[str]:
    """Split on sentence and line ends both.

    A README is half prose and half table rows and code, and a table row has no
    full stop -- so a denial three rows up would otherwise cover a claim that is
    not denied anywhere.
    """
    return re.split(r"(?<=[.!?])\s+|\n", text)


def published_files() -> list[pathlib.Path]:
    found: list[pathlib.Path] = []
    for pattern in PUBLISHED:
        if any(ch in pattern for ch in "*?["):
            hits = sorted(REPO.glob(pattern))
            if not hits:
                problems.append(
                    f"{pattern} matches no file. A published surface that moved "
                    f"has to move this list with it, or it leaves the scan."
                )
            found.extend(hits)
        else:
            path = REPO / pattern
            if not path.is_file():
                problems.append(
                    f"{pattern} is not there. A published surface that moved has "
                    f"to move this list with it, or it leaves the scan."
                )
                continue
            found.append(path)
    return [p for p in found
            if p.relative_to(REPO).as_posix() not in EXEMPT and p.stat().st_size]


def scan(path: pathlib.Path) -> None:
    rel = path.relative_to(REPO).as_posix()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        problems.append(f"{rel} is not UTF-8, so it was not read")
        return

    for line in sentences(text):
        denied = DENIAL.search(line)
        for pattern, why in RETIRED_WORDS:
            hit = re.search(pattern, line, re.I)
            if not hit:
                continue
            if pattern == r"\bperpetual\b" and any(ok in line for ok in PERPETUAL_OK):
                continue
            if denied:
                continue
            problems.append(
                f"{rel} publishes {hit.group(0)!r}: {why}\n    {line.strip()[:140]}"
            )

        for match in AMOUNT.finditer(line):
            if digits(match.group(1)) & set(RETIRED_AMOUNTS) and not denied:
                problems.append(
                    f"{rel} publishes {match.group(0)!r}, a price retired on "
                    f"06-09-2026 (B02 -> B28)\n    {line.strip()[:140]}"
                )


def dual_licensed_crates() -> list[str]:
    """Crates whose licence field is the dual AGPL one.

    The forks (`lopdf`, `hayro-*`, `pdf-syntax` and the rest) are MIT/Apache
    upstream and carry the upstream licence text; they are not ours to price.
    """
    names = []
    for manifest in sorted(REPO.glob("crates/*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        if "LicenseRef-PDFluent-Commercial" not in text:
            continue
        if re.search(r"^publish\s*=\s*false", text, re.M):
            continue
        names.append(manifest.parent.name)
    return names


def main() -> int:
    files = published_files()
    for path in files:
        scan(path)

    # Floor 1: the scan read something.
    if len(files) < 40:
        problems.append(
            f"only {len(files)} published files were read; the list has thinned "
            f"out and the scan no longer covers the registries"
        )

    # Floor 2: the vocabulary list is not empty.
    if len(RETIRED_WORDS) < 10 or len(RETIRED_AMOUNTS) < 7:
        problems.append("the retired vocabulary has been emptied out")

    # Floor 3: every crate we price says where the price is. Removing the
    # licence section is the cheapest way to pass a vocabulary scan.
    crates = dual_licensed_crates()
    if len(crates) < 15:
        problems.append(
            f"only {len(crates)} dual-licensed crates found; the manifest scan "
            f"has stopped working"
        )
    for name in crates:
        readme = REPO / "crates" / name / "README.md"
        if not readme.is_file():
            problems.append(f"crates/{name} is published and dual-licensed but has no README")
        elif PRICING_URL not in readme.read_text(encoding="utf-8"):
            problems.append(
                f"crates/{name}/README.md does not name {PRICING_URL}; a registry "
                f"page that prices nothing sends a reader to the retired pages"
            )

    if problems:
        print(f"the retired offer is still published in {len(problems)} place(s):",
              file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"published surfaces state the current offer ({len(files)} files, "
          f"{len(crates)} priced crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
