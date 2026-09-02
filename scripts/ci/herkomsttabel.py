#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Per crate: wie is de rechthebbende, en waaruit blijkt dat.

LC1 (#213), blokkeert LC10. Herlicentiëren zonder deze tabel is weggeven wat
misschien niet van jou is -- en dat merk je pas als iemand het aankaart.

DE TABEL WORDT GEGENEREERD, NIET GESCHREVEN
Een tabel met de hand is op de dag van publicatie al oud. Deze leest de
werkelijkheid: het licentieveld uit elke `Cargo.toml`, de aanwezigheid van een
upstream-notice, de proprietary header in de broncode, en of de crate in
`docs/release/canonical_licenses.toml` staat.

GEGENEREERD IS NIET GENOEG -- ER MOET IETS OP LETTEN
Tot 30-08-2026 herschreef dit script het bestand en gaf altijd 0 terug. Niets in
CI vergeleek het resultaat met wat er gecommit stond, dus de tabel dreef af en
zei ondertussen 376 `.rs`-bestanden waar er 379 stonden. Hij veranderde pas als
iemand toevallig de lokale poort draaide, en landde dan als een vreemde diff in
een commit over iets heel anders. Een gegenereerd document zonder `--check` is
een document met de hand, alleen trager.

    herkomsttabel.py            # schrijf docs/HERKOMST.md
    herkomsttabel.py --check    # faal als het gecommitte bestand is afgedreven

Afloopcodes:
    0  geschreven, of bij de tijd
    1  --check en het gecommitte bestand is afgedreven, of de ondergrens spreekt

WAT ER AL VASTSTAAT (#213)
* Eigen code is schoon: 3.274 commits van één persoon plus een CI-bot, geen
  externe menselijke bijdrager. Herlicentiëren vereist niemands toestemming.
* De ~750 `Co-Authored-By`-trailers zijn geen auteursrechtclaim; er is geen
  aparte rechthebbende. Ze zijn inmiddels uit de historie geschreven (#229).
* De Foxit-fonts in `pdf-interpret/assets/*.pfb` dragen BSD-3-Clause van de
  PDFium Authors en zijn volledig herdistribueerbaar.
"""
import argparse
import difflib
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CRATES = REPO / "crates"
UIT = REPO / "docs" / "HERKOMST.md"

# ONDERGRENS: crates >= 30 -- er zijn er achtenveertig. Vindt dit script er
# minder, dan is de boomwandeling stuk en niet de workspace leeg; een lege tabel
# zou lezen als "niets van een ander".
MIN_CRATES = 30

# Waar een geforkte crate vandaan komt. Handmatig, want dit is een feit over de
# buitenwereld dat nergens in de code staat -- en het is het soort feit dat je
# één keer opzoekt en daarna nooit meer.
# LET OP: de mapnaam en de gepubliceerde naam verschillen. `crates/hayro-ccitt`
# publiceert als `pdfluent-ccitt`, `crates/lopdf` als `pdfluent-lopdf`,
# `crates/cff-parser` als `pdfluent-cff`. Matchen op alleen de map noemde vijf
# geforkte crates "eigen werk" -- precies de fout die LC1 moet voorkomen, en hij
# zat in het script dat hem moest opsporen.
UPSTREAM = {
    "hayro-ccitt": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "hayro-jbig2": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "hayro-jpeg2000": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "lopdf": ("lopdf", "MIT"),
    "cff-parser": ("ttf-parser / pdf.js (Reizner, Muizelaar)", "MIT OR Apache-2.0"),
    "pdf-syntax": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdf-interpret": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdf-font": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdf-render": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdfluent-ccitt": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdfluent-jbig2": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdfluent-jpeg2000": ("hayro (Laurenz Stampfl)", "Apache-2.0 OR MIT"),
    "pdfluent-lopdf": ("lopdf", "MIT"),
    "pdfluent-cff": ("ttf-parser / pdf.js (Reizner, Muizelaar)", "MIT OR Apache-2.0"),
}

HEADER = "Innovation Trigger B.V."


def leest(pad: pathlib.Path) -> str:
    return pad.read_text(errors="replace") if pad.is_file() else ""


def crate_info(crate: pathlib.Path) -> dict | None:
    toml = leest(crate / "Cargo.toml")
    if not toml:
        return None
    naam = re.search(r'^\s*name\s*=\s*"([^"]+)"', toml, re.M)
    if not naam:
        return None
    licentie = re.search(r'^\s*license\s*=\s*"([^"]+)"', toml, re.M)
    licentiebestand = re.search(r'^\s*license-file\s*=\s*"([^"]+)"', toml, re.M)

    rs = [p for p in (crate / "src").rglob("*.rs")] if (crate / "src").is_dir() else []
    # Zestig regels, niet 400 tekens: een lang crate-doccomment duwt de
    # header voorbij dat venster en dan lijkt hij te ontbreken.
    met_header = sum(
        1 for p in rs if HEADER in "".join(leest(p).splitlines(keepends=True)[:60])
    )

    # Zoek op mapnaam én op gepubliceerde naam; ze verschillen bij vijf crates.
    upstream = UPSTREAM.get(crate.name) or UPSTREAM.get(naam.group(1))
    return {
        "map": crate.name,
        "naam": naam.group(1),
        "licentie": licentie.group(1) if licentie else ("license-file" if licentiebestand else "—"),
        "bestanden": len(rs),
        "met_header": met_header,
        "upstream": upstream,
        "notice": (crate / "NOTICE").is_file() or (crate / "LICENSE").is_file(),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true",
                    help="schrijf niets; faal als het gecommitte bestand afwijkt")
    args = ap.parse_args()

    crates = sorted(p for p in CRATES.iterdir() if p.is_dir() and (p / "Cargo.toml").is_file())
    if len(crates) < MIN_CRATES:
        print(
            f"ONDERGRENS: {len(crates)} crates gevonden, verwacht >= {MIN_CRATES}. "
            "De boomwandeling is stuk -- dit is geen groen.",
            file=sys.stderr,
        )
        return 1

    rijen = [c for c in (crate_info(p) for p in crates) if c]
    eigen = [r for r in rijen if not r["upstream"]]
    geforkt = [r for r in rijen if r["upstream"]]

    tot_bestanden = sum(r["bestanden"] for r in eigen)
    tot_header = sum(r["met_header"] for r in eigen)

    uit = [
        "# Herkomst per crate",
        "",
        "**Gegenereerd** door `scripts/ci/herkomsttabel.py`. Niet met de hand bewerken —",
        "een tabel die je overschrijft is op de dag van publicatie al oud.",
        "",
        "Dit beantwoordt één vraag: *wie is de rechthebbende, en waaruit blijkt dat?*",
        "Zonder dat antwoord is herlicentiëren weggeven wat misschien niet van jou is.",
        "",
        "## Wat vaststaat",
        "",
        "**Eigen code is schoon.** 3.274 commits in deze repo, van één persoon plus een",
        "CI-bot. Geen enkele externe menselijke bijdrager, dus herlicentiëren vereist",
        "niemands toestemming.",
        "",
        "De `Co-Authored-By`-trailers waren geen auteursrechtclaim — een assistent is geen",
        "rechthebbende. Ze zijn uit de historie geschreven (#229) en kunnen er sinds",
        "26-08-2026 niet meer in komen: `scripts/ci/no_ai_attribution.py` plus een",
        "`commit-msg`-haak.",
        "",
        "**De Foxit-fonts zijn in orde.** `crates/pdf-interpret/assets/*.pfb` draagt",
        "BSD-3-Clause van de PDFium Authors, volledig herdistribueerbaar. Dat is het",
        "klassieke struikelpunt bij open source; hier is het al geregeld.",
        "",
        "## Eigen crates",
        "",
        f"{len(eigen)} crates, {tot_bestanden} `.rs`-bestanden, waarvan **{tot_header}**",
        "de proprietary header dragen.",
        "",
        "| crate | licentie | .rs | met header |",
        "|---|---|---:|---:|",
    ]
    for r in sorted(eigen, key=lambda x: x["map"]):
        uit.append(f"| `{r['map']}` | {r['licentie']} | {r['bestanden']} | {r['met_header']} |")

    uit += [
        "",
        "## Andermans werk",
        "",
        "Allemaal permissief en geattribueerd in `NOTICE` en `THIRD_PARTY_LICENSES.txt`.",
        "**Deze crates horen onze header níét te krijgen** — een sweep die dat wel doet,",
        "claimt andermans werk.",
        "",
        "| crate | upstream | licentie | .rs | met header |",
        "|---|---|---|---:|---:|",
    ]
    for r in sorted(geforkt, key=lambda x: x["map"]):
        bron, lic = r["upstream"]
        vlag = " ⚠️" if r["met_header"] else ""
        uit.append(f"| `{r['map']}` | {bron} | {lic} | {r['bestanden']} | {r['met_header']}{vlag} |")

    besmet = [r["map"] for r in geforkt if r["met_header"]]
    uit += [
        "",
        "## Gekopieerde fragmenten, met naam",
        "",
        "| plek | bron | licentie |",
        "|---|---|---|",
        "| `pdfluent-jpeg2000/src/lib.rs` | OpenJPEG | BSD-2-Clause |",
        "| `pdf-interpret/src/color.rs:551` | [pdf.js `colorspace.js#L846`]"
        "(https://github.com/mozilla/pdf.js/blob/06f44916/src/core/colorspace.js#L846) | Apache-2.0 |",
        "",
        "Die tweede stond in #213 als *\"dat 'there' heeft geen naam\"*. Hij heeft er wel",
        "een: de regel erbóven draagt de volledige pdf.js-permalink.",
        "",
        "## Eigen werk, geen fork",
        "",
        "`pdf-content-stream` en `pdf-diff` stonden in #213 als *\"eigen keuze of fork?\"*.",
        "Nagetrokken in de historie: allebei beginnen met een commit die *\"New crate\"*",
        "zegt en een eigen ontwerp beschrijft. Geen fork.",
        "",
    ]
    if besmet:
        uit += [
            "## ⚠️ Aandacht",
            "",
            f"Deze geforkte crates dragen onze header: {', '.join('`'+b+'`' for b in besmet)}.",
            "Dat is een claim op andermans werk en hoort teruggedraaid vóór LC10.",
            "",
        ]

    tekst = "\n".join(uit).rstrip() + "\n"

    if besmet:
        print(f"LET OP: {len(besmet)} geforkte crate(s) dragen onze header: {', '.join(besmet)}",
              file=sys.stderr)

    if not args.check:
        UIT.write_text(tekst)
        print(f"OK: {len(rijen)} crates ({len(eigen)} eigen, {len(geforkt)} geforkt) -> {UIT.relative_to(REPO)}")
        return 0

    if not UIT.is_file():
        print(f"{UIT.relative_to(REPO)} ontbreekt; draai dit script zonder --check "
              "en commit het resultaat.", file=sys.stderr)
        return 1

    gecommit = UIT.read_text()
    if gecommit != tekst:
        print(f"{UIT.relative_to(REPO)} is afgedreven van de boom.", file=sys.stderr)
        # Toon de diff hier. De eerste versie zei alleen "afgedreven" en liet de
        # lezer hem zelf zoeken -- op een runner betekent dat de hele run lokaal
        # overdoen.
        regels = list(difflib.unified_diff(
            gecommit.splitlines(), tekst.splitlines(),
            fromfile="gecommit", tofile="gegenereerd", lineterm="", n=1))
        for regel in regels[:80]:
            print(f"  {regel}", file=sys.stderr)
        if len(regels) > 80:
            print(f"  ... en nog {len(regels) - 80} diffregels", file=sys.stderr)
        print("Draai `python3 scripts/ci/herkomsttabel.py` en commit het resultaat "
              "in dezelfde wijziging.", file=sys.stderr)
        return 1

    print(f"OK: {len(rijen)} crates ({len(eigen)} eigen, {len(geforkt)} geforkt), tabel klopt.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
