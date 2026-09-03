#!/usr/bin/env python3
"""Een verwijzing vanuit scripts/ci/ naar docs/ moet een bestaand bestand zijn.

Drie keer deze week haalde een script een document aan dat er niet was:
`docs/HERKOMST.md`, de registerverwijzing in `notice_matches_the_registry.py`,
en `docs/KWALITEITSSPOOR.md` — die laatste leefde byte-identiek op drie
nooit-gelande takken en op geen enkele die telt.

Zo'n verwijzing kost niets zolang niemand hem volgt, en dat is precies waarom
hij blijft staan: de auteur weet wat er had moeten staan en leest eroverheen.
Wie het wél opzoekt is meestal iemand van buiten, op het moment dat het uitmaakt.

WAT DIT WEL EN NIET DOET

Het leest paden die als tekst in de bron staan (`docs/...` gevolgd door een
bestandsextensie). Het volgt geen variabelen: een pad dat uit os.path.join of een
f-string komt, ziet het niet. Dat is bewust smal gehouden -- de drie gevallen die
dit moet vangen waren alle drie letterlijke strings in commentaar of docstring,
en een controle die probeert paden te reconstrueren levert vals alarm op waar
niemand meer naar kijkt.
"""
from __future__ import annotations
import pathlib, re, sys

REPO = pathlib.Path(__file__).resolve().parents[2]
MAP = REPO / "scripts" / "ci"

# `docs/<iets>.<ext>` -- de extensie is de eis, anders matcht dit elke zin die
# toevallig met "docs/" begint.
VERWIJZING = re.compile(r"\bdocs/[A-Za-z0-9_./-]+\.[A-Za-z0-9]{2,5}\b")

# ONDERGRENS: onderzochte bestanden >= 40. Er staan er ruim honderd in scripts/ci.
# Minder betekent dat het zoeken stuk is, niet dat het werk af is.
MIN_BESTANDEN = 40

# Testbestanden bouwen fixtures: `docs/big.md` bestaat niet en hoort niet te
# bestaan, want de test maakt hem in een tijdelijke repository. Ze uitsluiten is
# geen gemak maar een inhoudelijk onderscheid -- een test die naar een
# niet-bestaand pad verwijst, doet zijn werk.
IS_TEST = lambda naam: naam.startswith("test_")

# Namen die in commentaar staan als VOORBEELD, niet als aanhaling. Elk met de
# reden erbij, want een lijst zonder redenen groeit tot ze alles dekt.
VOORBEELDEN = {
    "docs/ZZQBETA-notes.md":
        "verzonnen naam in een comment over hoofdlettergevoeligheid",
    "docs/archive/viewer-rust-phase-2-plan.md":
        "beschrijft een vals alarm van vroeger; het bestand is sindsdien weg",
    "docs/big.md":
        "voorbeeld in de comment hierboven; dit bestand vond zichzelf",
}


def main() -> int:
    bestanden = sorted(p for p in MAP.iterdir()
                       if p.is_file() and p.suffix in {".py", ".sh"}
                       and not IS_TEST(p.name))
    if len(bestanden) < MIN_BESTANDEN:
        print(f"[docsrefs] FLOOR: {len(bestanden)} bestanden in "
              f"{MAP.relative_to(REPO)}, verwacht >= {MIN_BESTANDEN}. "
              "Het zoeken is stuk -- dit is geen groen.", file=sys.stderr)
        return 1

    kapot: list[tuple[str, str]] = []
    gezien = 0
    for bron in bestanden:
        tekst = bron.read_text(encoding="utf-8", errors="replace")
        for pad in sorted(set(VERWIJZING.findall(tekst))):
            if pad in VOORBEELDEN:
                continue
            gezien += 1
            if not (REPO / pad).is_file():
                kapot.append((bron.name, pad))

    if kapot:
        print(f"[docsrefs] {len(kapot)} verwijzing(en) naar een document dat er "
              "niet is:\n", file=sys.stderr)
        for bron, pad in kapot:
            print(f"  scripts/ci/{bron}  ->  {pad}", file=sys.stderr)
        print("\nMaak het bestand, of haal de verwijzing weg. Een aanhaling van "
              "een document\ndat niet bestaat, kost niets tot iemand hem volgt -- "
              "en dat is meestal\niemand van buiten, op het moment dat het "
              "uitmaakt.", file=sys.stderr)
        return 1

    print(f"[docsrefs] OK: {gezien} verwijzing(en) in {len(bestanden)} bestanden "
          "wijzen alle naar een bestaand document.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
