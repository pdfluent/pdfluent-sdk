#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tekstbehoud gemeten op woorden in plaats van op tekens.

    python3 scripts/pdfa/text_fidelity.py --before BRON.pdf --after UIT.pdf
    python3 scripts/pdfa/text_fidelity.py --pairs BESTAND.tsv

WAAROM DIT NAAST DE TELLING BESTAAT

`text_retention_gate.py` telt tekens: uitvoer gedeeld door bron. Dat meet
hoeveelheid, geen betekenis, en op 22-08 bleek hoe ver dat uiteen kan lopen.
Voor `170_170298.pdf` gaf de telling **80,1%**, terwijl de tekst dit was:

    bron       Physics H7C Fall 1999 Solutions to Problem Set 9 Derek Kimball
    uitvoer    C F 1 Pr m xxxxx xxx xxlxs xx qxxxxxx xxxxxxxxs

"xxxxx xxx xxlxs xx qxxxxxx" hoort "quantum mechanics" te zijn. Tachtig procent
van de tékens staat er nog en het document is vernield. Een omzetting die elke
letter door dezelfde vervangt scoort op tellen bijna vol.

Andersom is het net zo scheef: zonder `/ToUnicode` produceerde dezelfde omzetting
drie keer zoveel tekens (345%), allemaal vervangingstekens. Op tellen een
verbetering, in werkelijkheid onleesbaar.

WAT DIT WEL MEET

Welk deel van de woorden uit de bron in de uitvoer terugkomt, met veelvoud. Een
woord dat drie keer in de bron staat en één keer in de uitvoer telt voor een
derde. Ruis die erbij komt telt niet mee — dat is opzet: bijgekomen onzin hoort
de score niet te verhogen, en verlaagt hem ook niet, want dat is een andere
vraag.

Volgorde doet niet ter zake. Een omzetting die de leesvolgorde verandert maar
alle woorden bewaart is niet hetzelfde probleem als een die tekst weggooit, en
door op multiset te vergelijken blijft dit onderscheid staan.
"""

from __future__ import annotations

import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

# Woorden: letters en cijfers, minstens twee tekens. Losse leestekens zeggen
# niets over behoud en zouden de score opblazen met ruis die altijd matcht.
WOORD = re.compile(r"[0-9A-Za-zÀ-ɏ]{2,}")


def extract(pdf: Path, tool: str = "mutool") -> str | None:
    try:
        uit = subprocess.run(
            [tool, "draw", "-F", "txt", str(pdf)],
            capture_output=True, timeout=180,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return uit.stdout.decode("utf-8", "replace")


def woorden(tekst: str) -> collections.Counter:
    return collections.Counter(w.lower() for w in WOORD.findall(tekst))


def fidelity(bron: str, uit: str) -> tuple[float, int, int]:
    """Welk deel van de bronwoorden komt terug, met veelvoud."""
    b, u = woorden(bron), woorden(uit)
    if not b:
        return (float("nan"), 0, 0)
    behouden = sum(min(n, u.get(w, 0)) for w, n in b.items())
    totaal = sum(b.values())
    return (100.0 * behouden / totaal, behouden, totaal)


def meet(voor: Path, na: Path, tool: str) -> tuple[float, int, int] | None:
    b = extract(voor, tool)
    u = extract(na, tool)
    if b is None or u is None:
        return None
    return fidelity(b, u)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--before", type=Path)
    p.add_argument("--after", type=Path)
    p.add_argument("--pairs", type=Path,
                   help="TSV met per regel: naam <tab> bronpad <tab> uitvoerpad")
    p.add_argument("--mutool", default="mutool")
    a = p.parse_args()

    if a.pairs:
        scores = []
        for regel in a.pairs.read_text().splitlines():
            if not regel.strip():
                continue
            deel = regel.split("\t")
            if len(deel) < 3:
                continue
            naam, voor, na = deel[0], Path(deel[1]), Path(deel[2])
            r = meet(voor, na, a.mutool)
            if r is None or r[2] == 0:
                continue
            scores.append((naam, r[0]))
        if not scores:
            print("SKIPPED (not a pass): geen paar opgeleverd", file=sys.stderr)
            return 1
        w = sorted(s for _, s in scores)
        def pct(q: float) -> float:
            return w[min(int(len(w) * q), len(w) - 1)]
        print(f"documenten          {len(w)}")
        print(f"  mediaan           {w[len(w)//2]:.1f}%")
        print(f"  5e percentiel     {pct(0.05):.1f}%")
        print(f"  laagste           {w[0]:.1f}%")
        print(f"  99% of hoger      {sum(1 for x in w if x >= 99)}/{len(w)}")
        print(f"  onder 95%         {sum(1 for x in w if x < 95)}")
        print(f"  onder 50%         {sum(1 for x in w if x < 50)}")
        print()
        print("laagste tien:")
        for naam, s in sorted(scores, key=lambda x: x[1])[:10]:
            print(f"  {naam:<24} {s:6.1f}%")
        return 0

    if not (a.before and a.after):
        print("geef --before en --after, of --pairs", file=sys.stderr)
        return 64
    r = meet(a.before, a.after, a.mutool)
    if r is None:
        print("SKIPPED (not a pass): extractie mislukt", file=sys.stderr)
        return 1
    score, behouden, totaal = r
    print(f"{score:.1f}%  ({behouden} van {totaal} bronwoorden terug)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
