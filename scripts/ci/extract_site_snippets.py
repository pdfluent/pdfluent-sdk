#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Haalt de codevoorbeelden voor pdfluent.com uit code die compileert.

De documentatiepagina droeg vier Rust-voorbeelden die niet bouwden: een
`pdfluent::Sdk` die niet bestaat, `import_xfa_data` dat nergens voorkomt,
`PdfaLevel` waar het type `PdfAProfile` heet. Wie het eerste voorbeeld
kopieerde kreeg vijf compilatiefouten -- als eerste indruk van de SDK. Niemand
had ze ooit gebouwd (#164).

`crates/pdfluent/examples/site_snippets.rs` is nu de bron: hij compileert in
CI, en de blokken tussen `// site:<naam>` en `// site:end` zijn wat de site
toont. Dit script haalt ze eruit.

Zonder argument schrijft het naar stdout. Met `--check <bestand>` vergelijkt
het met een eerder weggeschreven JSON en faalt bij verschil -- dat is de vorm
waarin de website-repo hem gebruikt.

# ONDERGRENS: blokken >= 3 -- vindt dit script er bijna geen, dan zijn de
# markeringen veranderd en levert het een lege site op. Zie #235.
"""

import argparse
import json
import re
import sys
from pathlib import Path

WORTEL = Path(__file__).resolve().parent.parent.parent
BRON = WORTEL / "crates" / "pdfluent" / "examples" / "site_snippets.rs"

BLOK = re.compile(
    r"^[ \t]*// site:(?P<naam>[\w-]+)[ \t]*\n(?P<code>.*?)^[ \t]*// site:end[ \t]*$",
    re.M | re.S,
)


def ontleed() -> dict[str, str]:
    if not BRON.is_file():
        print(f"[site-snippets] {BRON} bestaat niet", file=sys.stderr)
        return {}
    tekst = BRON.read_text(encoding="utf-8")
    uit: dict[str, str] = {}
    for m in BLOK.finditer(tekst):
        regels = m.group("code").rstrip("\n").split("\n")
        # De inspringing van de functie eraf, zodat het voorbeeld op zichzelf
        # leesbaar is.
        inspringing = min(
            (len(r) - len(r.lstrip()) for r in regels if r.strip()), default=0
        )
        uit[m.group("naam")] = "\n".join(r[inspringing:] if r.strip() else "" for r in regels)
    return uit


# Woorden die in Nederlandse tekst vrijwel altijd voorkomen en in Rust-code of in
# Engelse tekst vrijwel nooit. Grof, en met opzet: fijnzinnige taaldetectie op drie
# regels commentaar is niet betrouwbaar, deze lijst wel.
#
# Waarom dit er is: de blokken hieronder gaan naar de Engelstalige site. Op
# 25-08-2026 stond er `let tekst`, `--- pagina {} ---`, `for veld in` en
# `form1.naam` in, plus vier Nederlandse commentaarregels. Dat compileert prima en
# valt bij het schrijven niet op -- de rest van het bestand is ook Nederlands.
# `per` stond hier eerst en moest eruit: het is ook Engels ("one row per field").
# Een woord dat in beide talen bestaat, maakt de controle onbruikbaar -- hij meldt
# dan correcte Engelse tekst, en daar leert een lezer hem te negeren.
NEDERLANDSE_WOORDEN = (
    "de", "het", "een", "van", "voor", "naam", "regel", "pagina",
    "tekst", "veld", "volledige", "invullen", "leesvolgorde", "waarde",
    "bestand", "wordt", "niet", "met", "zonder", "elke", "geen",
)


def controleer_taal(blokken: dict[str, str]) -> list[str]:
    """Meldt Nederlandse woorden in blokken die de Engelstalige site toont."""
    import re

    patroon = re.compile(
        r"\b(" + "|".join(NEDERLANDSE_WOORDEN) + r")\b", re.IGNORECASE
    )
    klachten = []
    for naam, tekst in blokken.items():
        for nummer, regel in enumerate(tekst.splitlines(), 1):
            m = patroon.search(regel)
            if m:
                klachten.append(f"  {naam}:{nummer}: {regel.strip()[:70]}  ({m.group(1)!r})")
    return klachten


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", metavar="BESTAND", help="vergelijk met deze JSON")
    p.add_argument("--write", metavar="BESTAND", help="schrijf de JSON hierheen")
    a = p.parse_args()

    blokken = ontleed()
    if len(blokken) < 3:
        print(
            f"[site-snippets] maar {len(blokken)} blok(ken) gevonden in "
            f"{BRON.name}. Dat is onwaarschijnlijk weinig -- waarschijnlijker "
            f"is dat de `// site:`-markeringen veranderd zijn, en dan levert "
            f"dit script een lege site op.",
            file=sys.stderr,
        )
        return 1

    klachten = controleer_taal(blokken)
    if klachten:
        print(
            "[site-snippets] Nederlandse woorden in code die de Engelstalige site toont:\n"
            + "\n".join(klachten)
            + "\n\nDe rest van dit bestand is Nederlands, dus dit valt bij het schrijven "
            "niet op -- en het compileert ook gewoon.",
            file=sys.stderr,
        )
        return 1

    if a.check:
        pad = Path(a.check)
        if not pad.is_file():
            print(f"[site-snippets] {pad} bestaat niet", file=sys.stderr)
            return 1
        eerder = json.loads(pad.read_text(encoding="utf-8"))
        if eerder != blokken:
            verschil = sorted(set(eerder) ^ set(blokken)) or [
                k for k in blokken if eerder.get(k) != blokken[k]
            ]
            print(
                f"[site-snippets] de voorbeelden op de site lopen uit de pas met "
                f"de code:\n  {', '.join(verschil)}\n\n"
                f"  De bron is {BRON.relative_to(WORTEL)} -- die compileert. "
                f"Draai dit script\n  met --write en neem het resultaat over.",
                file=sys.stderr,
            )
            return 1
        print(f"[site-snippets] {len(blokken)} voorbeelden gelijk aan de bron")
        return 0

    uitvoer = json.dumps(blokken, indent=2, ensure_ascii=False) + "\n"
    if a.write:
        Path(a.write).write_text(uitvoer, encoding="utf-8")
        print(f"[site-snippets] {len(blokken)} voorbeelden geschreven naar {a.write}")
    else:
        sys.stdout.write(uitvoer)
    return 0


if __name__ == "__main__":
    sys.exit(main())
