#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-PDFluent-Commercial
#
# This file is part of PDFluent. See LICENSE for the AGPL terms and
# LICENSE-COMMERCIAL.md for the commercial alternative.
"""Wat header_sweep moet doen, en waar hij het eerder niet deed.

De laatste drie gevallen zijn geen uitbreiding maar regressies: ze leggen vast
waarom de wachter maandenlang groen-genoeg leek terwijl 615 bestanden geen kop
droegen.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

CI = pathlib.Path(__file__).resolve().parent
SWEEP = CI / "header_sweep.py"
# Het merkteken van een goede kop is de wachter zijn eigen LICENTIEREGEL, niet
# een SPDX-expressie: `HEADER` draagt die niet. Een test die op een zelf
# verzonnen string meet, meet iets anders dan de wachter.
sys.path.insert(0, str(CI))
from header_sweep import (HEADER as HEADER_GOED, LICENTIEREGEL as DUAAL,  # noqa: E402
                          MIN_BESTANDEN)
PROP = "This software is proprietary"

KOP_PROP_RS = ("// Copyright (c) 2026 Innovation Trigger B.V.\n"
               "//\n// This software is proprietary. Unauthorised copying is prohibited.\n")
KOP_PROP_HASH = ("# Copyright (c) 2026 Innovation Trigger B.V.\n"
                 "#\n# This software is proprietary. Unauthorised copying is prohibited.\n")


def boom_zonder_vulling(tmp: pathlib.Path) -> pathlib.Path:
    """De kale structuur, zonder de bestanden die de ondergrens halen."""
    crate = tmp / "crates" / "eigen-crate"
    (crate / "src").mkdir(parents=True)
    (crate / "Cargo.toml").write_text('[package]\nname = "eigen-crate"\n'
                                      'license = "AGPL-3.0-only"\n', encoding="utf-8")
    (tmp / "scripts").mkdir(exist_ok=True)
    (tmp / "benchmarks").mkdir(exist_ok=True)
    return crate


def boom(tmp: pathlib.Path) -> pathlib.Path:
    """Een minimale repo: één eigen crate, plus scripts/ voor de `#`-kant."""
    crate = tmp / "crates" / "eigen-crate"
    (crate / "src").mkdir(parents=True)
    (crate / "Cargo.toml").write_text('[package]\nname = "eigen-crate"\n'
                                      'license = "AGPL-3.0-only"\n', encoding="utf-8")
    # Boven MIN_BESTANDEN uit komen: de wachter weigert terecht een boom te
    # beoordelen die te klein is om iets over te zeggen, en een fixture die
    # daaronder blijft test dus de weigering in plaats van het gedrag.
    for i in range(MIN_BESTANDEN + 10):
        (crate / "src" / f"vul{i}.rs").write_text(HEADER_GOED + "pub fn v() {}\n",
                                                  encoding="utf-8")
    (tmp / "scripts").mkdir(exist_ok=True)
    (tmp / "benchmarks").mkdir(exist_ok=True)
    return crate


def draai(tmp: pathlib.Path, *vlag: str) -> subprocess.CompletedProcess:
    omgeving = dict(os.environ, PDFLUENT_HEADER_SWEEP_ROOT=str(tmp))
    return subprocess.run([sys.executable, str(SWEEP), *vlag],
                          capture_output=True, text=True, env=omgeving)


def geval(naam: str, voorwaarde: bool, waarom: str) -> bool:
    print(f"  {'ok  ' if voorwaarde else 'FOUT'}  {naam}")
    if not voorwaarde:
        print(f"        {waarom}")
    return voorwaarde


def main() -> int:
    goed = True
    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)
        crate = boom(tmp)
        (crate / "src" / "kaal.rs").write_text("pub fn a() {}\n", encoding="utf-8")
        (crate / "src" / "prop.rs").write_text(KOP_PROP_RS + "pub fn b() {}\n", encoding="utf-8")
        (crate / "src" / "attr.rs").write_text("#![allow(dead_code)]\npub fn c() {}\n",
                                               encoding="utf-8")
        (tmp / "scripts" / "prop.py").write_text("#!/usr/bin/env python3\n" + KOP_PROP_HASH
                                                 + "print(1)\n", encoding="utf-8")
        (tmp / "scripts" / "prop.sh").write_text("#!/bin/sh\n" + KOP_PROP_HASH + "true\n",
                                                 encoding="utf-8")
        (tmp / "scripts" / "kaal.py").write_text("print(2)\n", encoding="utf-8")

        # De term als stringliteral, onder een goede kop. Dit is de wachter zijn
        # eigen bron: hij noemde zichzelf een bestand met een propriëtaire kop,
        # omdat hij de woorden zocht in "de eerste zestig regels" en zijn eigen
        # zoekterm daar staat. Een kop is een kop door zijn plaats.
        (tmp / "scripts" / "zoeker.py").write_text(
            "#!/usr/bin/env python3\n" + HEADER_GOED.replace("//", "#")
            + '\nTERM = "This software is proprietary"\nprint(TERM)\n', encoding="utf-8")

        r = draai(tmp)
        uit = r.stdout + r.stderr
        goed &= geval("een boom met fouten is rood", r.returncode == 1, uit[:200])

        # DE regressie: alle categorieen in een run, niet alleen de eerste.
        #
        # Zolang elke categorie zijn eigen `return 1` had, zag je bij 18 verkeerde
        # koppen nooit dat er ook 615 bestanden zonder kop waren -- de wachter
        # sprong eruit voordat hij daaraan toekwam. Een rapport dat stopt bij de
        # eerste bevinding vertelt je niet hoe groot het probleem is.
        goed &= geval("elke categorie wordt in dezelfde run gemeld",
                      "dragen een kop die niet" in uit and "geen kop" in uit
                      and "`#`-kop" in uit,
                      f"niet alle drie gemeld:\n{uit[:400]}")

        # Een `.py` zonder kop is geen bevinding. De `#`-kant repareert de
        # tegenspraak; hij legt geen kop op aan bestanden die er nooit een hadden.
        goed &= geval("de term als stringliteral is geen kop",
                      "zoeker.py" not in uit, uit[:300])
        goed &= geval("een `#`-bestand zonder kop is geen bevinding",
                      "kaal.py" not in uit, uit[:300])

        r = draai(tmp, "--write")
        goed &= geval("--write is groen", r.returncode == 0, (r.stdout + r.stderr)[:200])

        t_prop = (crate / "src" / "prop.rs").read_text(encoding="utf-8")
        # De vervanging, niet de toevoeging: master's --write liet een
        # propriëtaire kop staan omdat het merk er al stond, en meldde daarna
        # "voorzien van de header" over bestanden die hij niet had aangeraakt.
        goed &= geval("een propriëtaire `.rs`-kop wordt VERVANGEN",
                      DUAAL in t_prop and PROP not in t_prop, t_prop[:200])
        goed &= geval("een kale `.rs` krijgt de kop",
                      DUAAL in (crate / "src" / "kaal.rs").read_text(encoding="utf-8"), "")
        t_attr = (crate / "src" / "attr.rs").read_text(encoding="utf-8")
        goed &= geval("een `#![…]`-attribuut blijft op regel 1",
                      t_attr.splitlines()[0] == "#![allow(dead_code)]" and DUAAL in t_attr,
                      t_attr[:200])
        for naam in ("prop.py", "prop.sh"):
            t = (tmp / "scripts" / naam).read_text(encoding="utf-8")
            goed &= geval(f"{naam}: shebang blijft regel 1, kop vervangen",
                          t.splitlines()[0].startswith("#!") and DUAAL in t and PROP not in t,
                          t[:200])
        goed &= geval("een `#`-bestand zonder kop blijft ongemoeid",
                      (tmp / "scripts" / "kaal.py").read_text(encoding="utf-8") == "print(2)\n",
                      "")

        goed &= geval("na --write is de boom groen", draai(tmp).returncode == 0, "")

        # De mutatie: zet de propriëtaire kop terug en de wachter moet rood gaan.
        # Zonder dit geval overleeft een test het weghalen van juist het ding dat
        # hij bewaakt.
        (crate / "src" / "prop.rs").write_text(KOP_PROP_RS + "pub fn b() {}\n", encoding="utf-8")
        goed &= geval("een teruggezette propriëtaire kop maakt hem weer rood",
                      draai(tmp).returncode == 1, "")

    # De ondergrens zelf: een boom die te klein is om iets over te zeggen moet
    # geweigerd worden, niet groen genoemd. Zonder dit geval kan iemand de
    # ondergrens weghalen en blijft alles hierboven groen -- de test zou het
    # verdwijnen van juist die bescherming overleven.
    with tempfile.TemporaryDirectory() as d:
        klein = pathlib.Path(d)
        crate = boom_zonder_vulling(klein)
        (crate / "src" / "een.rs").write_text("pub fn a() {}\n", encoding="utf-8")
        r = draai(klein)
        goed &= geval("een te kleine boom wordt geweigerd, niet groen genoemd",
                      r.returncode != 0 and "ONDERGRENS" in (r.stdout + r.stderr),
                      (r.stdout + r.stderr)[:200])

    print("test_header_sweep: " + ("OK" if goed else "GEFAALD"))
    return 0 if goed else 1


if __name__ == "__main__":
    sys.exit(main())
