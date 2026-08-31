#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Elke fork noemt een forkpunt, of zegt hardop dat hij niet gemerged mag worden.

WAAROM DIT BESTAAT

Het register is zes keer fout geweest, elke keer dezelfde kant op: het beweerde
dat we actueler waren dan we waren. Twee daarvan wezen naar een commit *na* een
cherry-pick. Een drieweg-merge op zo'n basis leest upstream-werk dat wij niet
hebben als iets wat wij hebben verwijderd, en draait het terug -- terwijl het
rapport een upgrade meldt. Dat is de duurste vorm, want de uitkomst ziet er goed
uit.

Tegen een verkeerd forkpunt helpt `the_fork_register_is_verifiable.py`: die
controleert of de versie klopt bij de commit die genoemd wordt. Maar tegen een
*ontbrekend* forkpunt helpt niets. Op 31-08-2026 droegen vier van de negen
regels helemaal geen `forkpunt`, alleen een versienummer -- en een versienummer
kan een forkpunt niet zien. Er viel dus niets te controleren, en het gat zag er
identiek uit aan een gecontroleerd gat.

`pdf-render` is het geval waar dit niet met beter meten op te lossen is. Onze
`src/` is twee bestanden tegen upstreams zeven, geen enkele naam gedeeld, dus
geen enkel bestand is byte-identiek aan welke upstream-revisie dan ook en er
zijn geen vensters om te doorsnijden. Het forkpunt is niet onbekend bij gebrek
aan moeite; de methode reikt er niet.

WAT DEZE CONTROLE AFDWINGT

Elke `[[fork]]` draagt precies een van twee dingen:

  forkpunt = "<commit>"     -- hier komt onze code vandaan, controleerbaar
  niet_mergen = "<reden>"   -- geen forkpunt, dus deze crate wordt niet gemerged

Geen van beide is een fout. Allebei ook: dan spreekt de regel zichzelf tegen.

`niet_mergen` is geen ontsnapping maar een rem. Het staat in de uitvoer, het
staat in het register, en het maakt van een gat een besluit in plaats van een
omissie. Een crate met `niet_mergen` mag pas gemerged worden als het veld weg is
-- en het veld gaat pas weg als er een forkpunt voor in de plaats komt.

Exitcodes:
    0  elke fork noemt een forkpunt of een reden om niet te mergen
    1  een fork noemt geen van beide, of allebei
    2  het register ontbreekt of is onleesbaar
"""

from __future__ import annotations

import pathlib
import sys

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    print("SKIPPED (not a pass): geen tomllib, dus het register is niet gelezen",
          file=sys.stderr)
    raise SystemExit(2)

WORTEL = pathlib.Path(__file__).resolve().parents[2]
LIJST = WORTEL / "docs" / "UPSTREAM_FORKS.toml"

# ONDERGRENS: het register heeft er negen. Vindt deze controle er minder dan
# vijf, dan is het inlezen stuk en niet het bestand leeg -- en een controle die
# nul regels controleert en groen meldt is precies de fout die dit project al
# vier keer heeft gehad.
MINIMAAL_AANTAL_FORKS = 5


def main() -> int:
    if not LIJST.exists():
        print(f"FAIL: {LIJST.relative_to(WORTEL)} ontbreekt; er is niets dat vastlegt "
              "waar de forks vandaan komen.", file=sys.stderr)
        return 2

    try:
        forks = tomllib.load(LIJST.open("rb"))["fork"]
    except Exception as fout:  # noqa: BLE001 - elke leesfout is hier hetzelfde
        print(f"FAIL: {LIJST.relative_to(WORTEL)} is onleesbaar: {fout}", file=sys.stderr)
        return 2

    if len(forks) < MINIMAAL_AANTAL_FORKS:
        print(f"ONDERGRENS: {len(forks)} fork(s) gelezen, verwacht >= "
              f"{MINIMAAL_AANTAL_FORKS}. Het inlezen is stuk -- dit is geen groen.",
              file=sys.stderr)
        return 1

    fouten: list[str] = []
    geblokkeerd: list[tuple[str, str]] = []

    for f in forks:
        naam = f.get("onze_crate", "<naamloos>")
        forkpunt = f.get("forkpunt")
        niet_mergen = f.get("niet_mergen")

        if forkpunt and niet_mergen:
            fouten.append(
                f"  {naam}: draagt zowel `forkpunt` als `niet_mergen`. Als het "
                "forkpunt bekend is, is er geen reden hem niet te mergen; haal er "
                "een van weg."
            )
        elif not forkpunt and not niet_mergen:
            fouten.append(
                f"  {naam}: geen `forkpunt` en geen `niet_mergen`. Een versienummer "
                "kan een forkpunt niet zien, dus deze regel is niet te controleren "
                "en mergen ertegen is raden."
            )
        elif niet_mergen:
            geblokkeerd.append((naam, niet_mergen))

    for naam, reden in geblokkeerd:
        print(f"[forkpunt] NIET MERGEN: {naam} -- {reden}")

    if fouten:
        print(file=sys.stderr)
        print(f"[forkpunt] FAIL: {len(fouten)} regel(s) in "
              f"{LIJST.relative_to(WORTEL)} zonder controleerbaar forkpunt:",
              file=sys.stderr)
        for regel in fouten:
            print(regel, file=sys.stderr)
        print(
            "\nEen fork zonder forkpunt wordt niet gemerged. Stel het forkpunt vast "
            "op inhoud -- vensters van byte-identieke bestanden, niet regelafstand, "
            "die keert om zodra de bestandsverzamelingen verschillen -- of zet "
            "`niet_mergen = \"<reden>\"` zodat het gat een besluit is.",
            file=sys.stderr,
        )
        return 1

    print(f"[forkpunt] OK: {len(forks)} fork(s), elk met een forkpunt of een "
          f"reden om niet te mergen ({len(geblokkeerd)} geblokkeerd).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
