#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Een fixture mag geen syntaxfout dragen die niemand bedoeld heeft.

Acht van de elf documenten in `tests/corpus-mini` droegen `<<//Length`. Een
dubbele slash: de sleutel heet daardoor `//Length` en niet `/Length`, dus de
lengte van de inhoudsstroom is onbekend en het object laadt niet.

Het gevolg was onzichtbaar omdat de engine toleranter leest dan lopdf. De
engine haalde er tekst uit, dus alles leek te werken -- maar alles wat de
inhoudsstroom via lopdf benadert, kreeg nul bytes. Redactie meldde daardoor
"nul treffers" op documenten waarin de term gewoon staat (#203), en niemand
merkte het, want een schoon rapport ziet er hetzelfde uit als een terecht
schoon rapport.

Deze lint kijkt naar de syntaxfouten die je met de hand maakt bij het schrijven
van een fixture, niet naar geldigheid in het algemeen. `malformed.pdf` moet
kapot blijven -- maar wel op de manier die zijn naam belooft.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

WORTEL = Path(__file__).resolve().parent.parent.parent
CORPUS = WORTEL / "tests" / "corpus-mini"

# Vormen die altijd fout zijn, met wat er misging toen ze er stonden.
PATRONEN = [
    (
        re.compile(rb"<<\s*//\w"),
        "een dictionary-sleutel met een dubbele slash (`<<//Length`). De sleutel "
        "heet dan niet wat je denkt, en het object laadt niet",
    ),
    (
        re.compile(rb"/Length\s+/"),
        "`/Length` met een naam als waarde in plaats van een getal",
    ),
    (
        # Alleen als de dictionary een lengte belooft die er niet is. Een
        # `/Length 0` met een lege stroom is consistent en komt echt voor:
        # acroform-multiselect.pdf heeft een lege pagina-inhoud, en die vlaggen
        # zou een lint zijn die wolf roept -- en zo'n lint wordt uitgezet.
        re.compile(rb"/Length\s+([1-9]\d*)\s*>>\s*stream\r?\n\r?\nendstream"),
        "een lege inhoudsstroom terwijl de dictionary een lengte boven nul belooft",
    ),
]


GENERATOR = WORTEL / "scripts" / "corpus-generate-mini.py"


def controleer(bestanden: list[Path], waar: str) -> list[str]:
    """De patronen over een verzameling documenten, met hun herkomst erbij."""
    fouten = []
    for pad in bestanden:
        data = pad.read_bytes()
        for patroon, uitleg in PATRONEN:
            treffers = patroon.findall(data)
            if treffers:
                fouten.append(f"{waar}{pad.name}: {len(treffers)}x {uitleg}")
    return fouten


def uit_de_generator() -> tuple[list[str], str | None]:
    """Dezelfde patronen over wat de generator VANDAAG schrijft.

    DE COMMITTED DOCUMENTEN WAREN GEREPAREERD EN DE GENERATOR NIET.

    Acht van de elf droegen `<<//Length`, en die vorm kwam niet uit een
    typefout in acht bestanden: `scripts/corpus-generate-mini.py` schreef hem,
    omdat elke aanroep de dictionary opende met `"<</"` en de generator er
    `/Length` achter plakte. De bestanden zijn in augustus met de hand hersteld.
    De generator niet -- dus wie het corpus opnieuw genereerde, schreef alle acht
    precies zo terug.

    Dat is de reden dat deze controle de generator draait in plaats van alleen de
    map te lezen: een document repareren zonder de bron ervan te repareren, laat
    de fout staan op de enige plek waar hij zichzelf herhaalt (#236).
    """
    if not GENERATOR.is_file():
        return [], f"{GENERATOR.name} bestaat niet"
    with tempfile.TemporaryDirectory() as td:
        uit = subprocess.run(
            [sys.executable, str(GENERATOR), td],
            capture_output=True, text=True, check=False,
        )
        if uit.returncode != 0:
            eerste = ((uit.stderr or uit.stdout).strip().splitlines() or ["geen uitvoer"])[-1]
            return [f"de generator zelf viel om: {eerste}"], None
        geschreven = sorted(Path(td).glob("*.pdf"))
        if len(geschreven) < 5:
            return [f"de generator schreef maar {len(geschreven)} document(en)"], None
        return controleer(geschreven, "generator "), None


def main() -> int:
    if not CORPUS.is_dir():
        print(f"SKIPPED (not a pass): {CORPUS} bestaat niet", file=sys.stderr)
        return 1

    bestanden = sorted(CORPUS.glob("*.pdf"))
    if len(bestanden) < 5:
        print(
            f"[fixtures] maar {len(bestanden)} documenten gevonden; leest deze "
            f"lint wel de goede map?",
            file=sys.stderr,
        )
        return 1

    fouten = controleer(bestanden, "")
    uit_generator, overgeslagen = uit_de_generator()
    fouten += uit_generator

    if fouten:
        print("[fixtures] documenten met een onbedoelde syntaxfout:\n", file=sys.stderr)
        for fout in fouten:
            print(f"  - {fout}", file=sys.stderr)
        print(
            "\n  Een fixture met een fout die niemand bedoeld heeft, meet minder dan\n"
            "  de test denkt -- en dat valt niet op, want de test slaagt gewoon.",
            file=sys.stderr,
        )
        return 1

    if overgeslagen:
        print(f"[fixtures] SKIPPED (not a pass) voor de generator: {overgeslagen}",
              file=sys.stderr)
    print(f"[fixtures] {len(bestanden)} documenten, geen onbedoelde syntaxfouten, "
          "en de generator schrijft er ook geen")
    return 0


if __name__ == "__main__":
    sys.exit(main())
