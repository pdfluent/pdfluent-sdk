#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Staat er iets in de gedeelde git-config dat elke worktree breekt?

WAT ER GEBEURDE

Op 31-08-2026 stond `core.bare = true` in `.git/config`. Dat bestand wordt door
alle worktrees gedeeld, dus alle worktrees braken tegelijk: `git commit`, `git
status` en `git rev-parse --show-toplevel` gaven alledrie

    fatal: this operation must be run in a work tree

terwijl `git rev-parse --git-dir` het gewoon deed. Vijf minuten eerder werkte
alles nog. Er draaien meerdere sessies in deze repo; wie het zette is niet te
achterhalen, en dat is precies het punt -- het komt terug.

De foutmelding wijst naar de worktree, niet naar de gedeelde config, dus je zoekt
op de verkeerde plek. Deze controle zegt in één regel waar het staat en hoe je
het terugzet.

WAT WORDT GECONTROLEERD

`core.bare` -- moet `false` zijn zolang er een working tree is. Een echte bare
repo heeft geen `.git/config` in een checkout, dus als dit bestand bestaat en
`core.bare = true` zegt, is dat een fout en geen configuratie.

`core.worktree` in de gedeelde config -- wijst elke worktree naar hetzelfde
pad, wat betekent dat je in de ene worktree de bestanden van de andere bewerkt.
Hoort per worktree in `config.worktree` te staan, nooit gedeeld.

Alleen de gedeelde config wordt gelezen, niet `--global` en niet
`config.worktree`: dit gaat over wat andere sessies stukmaakt, niet over wat
iemand voor zichzelf instelt.

Exitcodes:
    0  de gedeelde config breekt geen worktrees
    1  hij doet dat wel, of hij is niet te vinden -- allebei een fout, want een
       onvindbare gedeelde config is zelf een van de vormen van kapot
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]


def gedeelde_config() -> pathlib.Path | None:
    """Het config-bestand dat alle worktrees delen, gevonden zonder git.

    Niet via `git rev-parse --git-common-dir`, hoe voor de hand liggend dat ook
    is. De eerste versie hiervan deed dat, en die faalde precies in het geval
    waarvoor hij bestaat: met `core.worktree` in de gedeelde config weigert
    rev-parse zelf, de controle vond geen config, en meldde een overslag. Groen,
    terwijl elke worktree stuk stond.

    Dus met de hand, zoals git het zelf opschrijft: in een worktree is `.git` een
    bestand met `gitdir: <pad>`, en in die map staat `commondir` met het pad naar
    de gedeelde map. In de hoofdcheckout is `.git` gewoon een map.
    """
    punt = WORTEL / ".git"
    if punt.is_dir():
        pad = punt / "config"
        return pad if pad.exists() else None
    if not punt.is_file():
        return None

    inhoud = punt.read_text(errors="replace").strip()
    if not inhoud.startswith("gitdir:"):
        return None
    gitdir = pathlib.Path(inhoud.split(":", 1)[1].strip())
    if not gitdir.is_absolute():
        gitdir = (WORTEL / gitdir).resolve()

    commondir = gitdir / "commondir"
    gemeenschappelijk = gitdir
    if commondir.exists():
        rel = pathlib.Path(commondir.read_text(errors="replace").strip())
        gemeenschappelijk = rel if rel.is_absolute() else (gitdir / rel).resolve()

    pad = gemeenschappelijk / "config"
    return pad if pad.exists() else None


def waarde(tekst: str, sectie: str, sleutel: str) -> str | None:
    """Lees één sleutel uit één sectie, zoals git hem zou lezen.

    Met git zelf lezen zou de global en de per-worktree config meenemen, en juist
    het onderscheid tussen die drie is hier de vraag. Dus met de hand -- en dan
    ook mét de twee eigenaardigheden die git wél heeft en een naïeve lezer niet
    (allebei aangewezen door Codex op #1609):

    * **De laatste toekenning wint.** Bij `bare = false` gevolgd door
      `bare = true` gebruikt git `true`. De eerste versie hiervan gaf de eerste
      terug en meldde OK terwijl elke worktree stuk stond -- de exacte fout die
      deze controle moet vinden.
    * **Een sleutel zonder `=` is `true`.** `[core]` met een kale regel `bare`
      is geldige git-syntax en betekent waar. De eerste versie zag daar geen
      toekenning en gaf None terug, dus ook groen.
    """
    huidige = None
    gevonden: str | None = None
    for regel in tekst.splitlines():
        kaal = regel.split("#", 1)[0].split(";", 1)[0].strip()
        if not kaal:
            continue
        kop = re.match(r"\[([^\]\s]+)", kaal)
        if kop:
            huidige = kop.group(1).lower()
            continue
        if huidige != sectie:
            continue
        m = re.match(r"([A-Za-z0-9_-]+)\s*(?:=\s*(.*))?$", kaal)
        if not m or m.group(1).lower() != sleutel:
            continue
        # Geen `=`: git's impliciete booleaanse waar.
        rauw = m.group(2)
        gevonden = "true" if rauw is None else rauw.strip().strip('"')
    return gevonden


def main() -> int:
    pad = gedeelde_config()
    if pad is None:
        # Geen overslag. Deze controle bestaat om te merken dat de gedeelde
        # config stuk is, en "ik kan hem niet vinden" is een van de vormen die
        # dat aanneemt -- niet iets om groen over te zijn.
        print(f"FAIL: geen gedeelde git-config gevonden vanaf {WORTEL}. Dit is "
              "geen overslag: als de config onvindbaar is, is er iets mis met de "
              "repo en niet met deze controle.", file=sys.stderr)
        return 1

    tekst = pad.read_text(errors="replace")
    klachten: list[tuple[str, str]] = []

    bare = waarde(tekst, "core", "bare")
    if bare is not None and bare.lower() in ("true", "yes", "on", "1"):
        klachten.append((
            "core.bare = true",
            "Elke worktree geeft nu `fatal: this operation must be run in a work "
            "tree` op commit, status en rev-parse --show-toplevel, terwijl "
            "--git-dir het wel doet. Terugzetten met:\n"
            "    /usr/bin/git config --replace-all core.bare false",
        ))

    wt = waarde(tekst, "core", "worktree")
    if wt is not None:
        klachten.append((
            f"core.worktree = {wt}",
            "Dit staat in de gedeelde config, dus elke worktree wijst naar "
            "hetzelfde pad -- je bewerkt dan in de ene worktree de bestanden van "
            "de andere. Hoort per worktree in config.worktree. Weghalen met:\n"
            "    /usr/bin/git config --unset core.worktree",
        ))

    if klachten:
        print(file=sys.stderr)
        print(f"[gitconfig] FAIL: {pad} draagt {len(klachten)} instelling(en) die "
              "elke worktree in deze repo breken:", file=sys.stderr)
        for wat, hoe in klachten:
            print(f"\n  {wat}\n    {hoe}", file=sys.stderr)
        print("\nDeze repo draait meerdere sessies tegelijk. Een instelling in de "
              "gedeelde config raakt ze allemaal, en de foutmelding wijst naar de "
              "worktree in plaats van hierheen.", file=sys.stderr)
        return 1

    print(f"[gitconfig] OK: {pad.name} breekt geen worktrees.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
