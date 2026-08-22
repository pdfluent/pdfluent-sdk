#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for the retention gate's distribution report.

Run: python3 scripts/ci/test_retention_report.py

WAAROM HIER EEN TEST OP ZIT

Deze functie print het cijfer dat naar buiten gaat. Tot 22-08 meldde de poort
alleen wát er achteruitging, dus een geslaagde run leverde geen getal op — je
wist dat niets slechter werd, niet hoe goed het was. Dat getal werd dan alsnog
met de hand ergens vandaan gehaald, en zo blijft een verkeerd cijfer maanden op
een website staan.

Een printfunctie die stil niets doet is daarom niet onschuldig: dan is het cijfer
weg en merkt niemand het, want de poort zegt gewoon OK.
"""

from __future__ import annotations

import io
import contextlib
import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
GATE = HERE.parent / "pdfa" / "text_retention_gate.py"

mislukt: list[str] = []


def controleer(naam: str, voorwaarde: bool, toelichting: str = "") -> None:
    if voorwaarde:
        print(f"  ok    {naam}")
    else:
        print(f"  FOUT  {naam}  {toelichting}")
        mislukt.append(naam)


def laad():
    """Laadt de poort opnieuw, met de bytecode-cache uitgeschakeld.

    Zonder dit is deze test onbetrouwbaar op precies het moment dat hij nodig is.
    Python beoordeelt een `.pyc` op tijdstempel én bestandsgrootte, en een
    mutatie die `<` in `>` verandert laat allebei gelijk. Gebeuren de wijziging
    en de test binnen dezelfde seconde -- wat bij een mutatietest altijd zo is --
    dan draait de oude bytecode en blijft de test groen op gebroken code.

    Dat is hier één keer gebeurd en het kostte een kwartier om te herkennen,
    omdat de broncode klaarblijkelijk klopte terwijl de uitvoer iets anders zei.
    """
    sys.dont_write_bytecode = True
    importlib.invalidate_caches()
    spec = importlib.util.spec_from_file_location("retentiepoort", GATE)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def uitvoer_van(mod, gegevens: dict) -> str:
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        mod.print_distribution(gegevens)
    return buf.getvalue()


def test_de_verdeling_wordt_gerapporteerd() -> None:
    mod = laad()
    # negen waarden: mediaan 100.0, laagste 40.0, drie onder 95, één onder 50
    gegevens = {f"d{i}.pdf": v for i, v in enumerate(
        [40.0, 80.0, 94.0, 99.0, 100.0, 101.0, 102.0, 103.0, 120.0])}
    uit = uitvoer_van(mod, gegevens)
    controleer("er komt iets uit", bool(uit.strip()), "leeg")
    controleer("mediaan klopt", "median              100.0%" in uit, uit)
    controleer("laagste klopt", "lowest              40.0%" in uit, uit)
    controleer("telling onder 95 klopt", "below 95%           3" in uit, uit)
    controleer("telling onder 50 klopt", "below 50%           1" in uit, uit)
    controleer("telling vanaf 100 klopt", "at or above 100%    5/9" in uit, uit)


def test_een_lege_meting_zwijgt_niet_stiekem() -> None:
    """Bij niets gemeten hoort er niets te staan — maar dan moet de poort er ook
    niet op omvallen. Een uitzondering hier zou de hele run laten falen op het
    rapporteren in plaats van op het meten."""
    mod = laad()
    uit = uitvoer_van(mod, {})
    controleer("lege meting geeft geen regels", uit.strip() == "", repr(uit))


def test_niet_numerieke_waarden_worden_overgeslagen() -> None:
    mod = laad()
    uit = uitvoer_van(mod, {"a.pdf": 100.0, "b.pdf": None, "c.pdf": "kapot", "d.pdf": 50.0})
    controleer("telt alleen getallen", "at or above 100%    1/2" in uit, uit)


def main() -> int:
    if not GATE.is_file():
        print(f"SKIPPED (not a pass): {GATE} bestaat niet", file=sys.stderr)
        return 1
    print(f"retention-report tests ({GATE.name})")
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
