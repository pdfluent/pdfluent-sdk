#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests voor de woordgebaseerde tekstbehoudmaat.

Run: python3 scripts/ci/test_text_fidelity.py

WAAROM DEZE TESTS BESTAAN

De maat die hiervoor gebruikt werd telde tekens, en gaf een vernield document
80,1%: elke letter was door een 'x' vervangen, dus tachtig procent van de tékens
stond er nog. Deze maat moet dat geval laag scoren en een ongeschonden document
hoog — en die twee gevallen staan hieronder als eerste, want dat is waar hij voor
gemaakt is.

Geen PDF's nodig: de kern is een vergelijking van twee stukken tekst.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
MAAT = HERE.parent / "pdfa" / "text_fidelity.py"

mislukt: list[str] = []


def controleer(naam: str, voorwaarde: bool, toelichting: str = "") -> None:
    if voorwaarde:
        print(f"  ok    {naam}")
    else:
        print(f"  FOUT  {naam}  {toelichting}")
        mislukt.append(naam)


def laad():
    # Bytecode-cache uit: een mutatie die even groot is en in dezelfde seconde
    # valt, wordt anders niet opgemerkt en blijft de test groen op kapotte code.
    sys.dont_write_bytecode = True
    importlib.invalidate_caches()
    spec = importlib.util.spec_from_file_location("fidelity", MAAT)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


BRON = "Physics H7C Fall 1999 Solutions to Problem Set 9 Derek Kimball quantum mechanics"


def test_elke_letter_vervangen_scoort_laag() -> None:
    """Het geval dat de tekentelling 80% gaf."""
    mod = laad()
    kapot = "C F 1 Pr m xxxxx xxx xxlxs xx qxxxxxx xxxxxxxxs"
    score, _, _ = mod.fidelity(BRON, kapot)
    controleer(
        "vernielde tekst scoort onder 20%",
        score < 20,
        f"kreeg {score:.1f}%",
    )


def test_ongeschonden_tekst_scoort_vol() -> None:
    mod = laad()
    score, _, _ = mod.fidelity(BRON, BRON)
    controleer("identieke tekst is 100%", abs(score - 100.0) < 0.01, f"kreeg {score:.1f}%")


def test_bijgekomen_ruis_verhoogt_de_score_niet() -> None:
    """Zonder /ToUnicode leverde een omzetting drie keer zoveel tekens op,
    allemaal vervangingstekens. Op tellen was dat 345%; hier hoort het niets
    toe te voegen."""
    mod = laad()
    met_ruis = BRON + " " + ("�" * 500) + " qqqq wwww eeee"
    score, _, _ = mod.fidelity(BRON, met_ruis)
    controleer("ruis erbij houdt de score op 100%", abs(score - 100.0) < 0.01, f"kreeg {score:.1f}%")


def test_de_helft_weggooien_geeft_ongeveer_de_helft() -> None:
    mod = laad()
    woorden = BRON.split()
    helft = " ".join(woorden[: len(woorden) // 2])
    score, _, _ = mod.fidelity(BRON, helft)
    controleer("de helft bewaard geeft 40-60%", 40 <= score <= 60, f"kreeg {score:.1f}%")


def test_volgorde_doet_niet_ter_zake() -> None:
    """Een gewijzigde leesvolgorde is een ander probleem dan tekstverlies, en
    hoort deze maat niet te verlagen."""
    mod = laad()
    omgekeerd = " ".join(reversed(BRON.split()))
    score, _, _ = mod.fidelity(BRON, omgekeerd)
    controleer("omgekeerde volgorde blijft 100%", abs(score - 100.0) < 0.01, f"kreeg {score:.1f}%")


def test_veelvoud_telt_mee() -> None:
    """Een woord dat drie keer in de bron staat en één keer in de uitvoer telt
    voor een derde — anders verbergt een maat op unieke woorden dat herhaalde
    inhoud verdwenen is."""
    mod = laad()
    score, behouden, totaal = mod.fidelity("test test test", "test")
    controleer("3 keer bron, 1 keer uit = 33%", abs(score - 100.0/3) < 0.5,
               f"kreeg {score:.1f}% ({behouden}/{totaal})")


def test_hoofdletters_tellen_niet_mee() -> None:
    mod = laad()
    score, _, _ = mod.fidelity("Quantum Mechanics", "quantum mechanics")
    controleer("hoofdlettergebruik verandert niets", abs(score - 100.0) < 0.01, f"kreeg {score:.1f}%")


def test_lege_bron_geeft_geen_getal() -> None:
    """Zonder bronwoorden valt er niets te behouden; dan hoort er NaN uit te
    komen in plaats van 0% of 100%, want beide zouden gelezen worden als een
    oordeel."""
    mod = laad()
    score, _, _ = mod.fidelity("", "van alles")
    controleer("lege bron geeft NaN", score != score, f"kreeg {score}")


def test_een_bron_met_bijna_geen_woorden_is_geen_bewijs() -> None:
    """De blinde vlek van deze maat, in cijfers.

    govdocs 076_076313.pdf heeft 5.865 tekens en twee woorden. De omzetting
    verloor 31% van de tekens en deze maat gaf hem 100,0%. Een percentage over
    twee woorden is geen meting; MIN_WOORDEN bestaat om zulke documenten apart
    te zetten in plaats van ze als een perfecte score mee te tellen.
    """
    mod = laad()
    score, behouden, totaal = mod.fidelity("ok ok", "ok ok 1 2 3 4 5 6 7 8 9")
    controleer(
        "twee woorden geven 100% terwijl er van alles bij kan zijn gekomen",
        score == 100.0 and totaal == 2,
        f"{score} {behouden}/{totaal}",
    )
    controleer(
        "en die 2 ligt onder de drempel",
        totaal < mod.MIN_WOORDEN,
        f"MIN_WOORDEN={mod.MIN_WOORDEN}",
    )


def test_de_twee_implementaties_delen_hun_regels() -> None:
    """De woordmaat bestaat twee keer: hier in Python en in het
    reproductiescript in JavaScript, zodat een buitenstaander hetzelfde cijfer
    kan uitrekenen zonder onze broncode.

    Twee kopieën lopen uit elkaar, en dan meten ze iets anders terwijl ze
    hetzelfde heten. Het woordpatroon en de drempel worden daarom hier
    vergeleken in plaats van met de hand.
    """
    import pathlib
    import re as _re

    wortel = pathlib.Path(__file__).resolve().parents[2]
    py = (wortel / "scripts/pdfa/text_fidelity.py").read_text(encoding="utf-8")
    js_pad = wortel / "benchmarks/pdfa/reproduce/reproduce.mjs"
    if not js_pad.is_file():
        print(f"SKIPPED (not a pass): {js_pad} ontbreekt", file=sys.stderr)
        return
    js = js_pad.read_text(encoding="utf-8")

    p_re = _re.search(r'^WOORD = re\.compile\(r"(.+)"\)', py, _re.M)
    j_re = _re.search(r"^const WOORD = /(.+)/gu;", js, _re.M)
    p_min = _re.search(r"^MIN_WOORDEN = (\d+)", py, _re.M)
    j_min = _re.search(r"^const MIN_WOORDEN = (\d+);", js, _re.M)

    controleer(
        "beide kanten zijn vindbaar",
        all([p_re, j_re, p_min, j_min]),
        f"py_re={bool(p_re)} js_re={bool(j_re)} py_min={bool(p_min)} js_min={bool(j_min)}",
    )
    if not all([p_re, j_re, p_min, j_min]):
        return
    controleer(
        "hetzelfde woordpatroon",
        p_re.group(1) == j_re.group(1),
        f"{p_re.group(1)!r} tegen {j_re.group(1)!r}",
    )
    controleer(
        "dezelfde drempel",
        p_min.group(1) == j_min.group(1),
        f"{p_min.group(1)} tegen {j_min.group(1)}",
    )


def main() -> int:
    if not MAAT.is_file():
        print(f"SKIPPED (not a pass): {MAAT} bestaat niet", file=sys.stderr)
        return 1
    print(f"text-fidelity tests ({MAAT.name})")
    for naam, fn in sorted(globals().items()):
        if naam.startswith("test_") and callable(fn):
            fn()
    if mislukt:
        print(f"\n{len(mislukt)} test(s) gefaald: {', '.join(mislukt)}")
        return 1
    print("\nalle tests geslaagd")
    return 0


if __name__ == "__main__":
    sys.exit(main())
