#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Een commitboodschap is een publicatie. Deze houdt tegen wat dat niet mag zijn.

Op 26-08-2026 bleek `pdfluent/pdfluent` publiek te staan, met 253 issues erin --
inclusief commerciële afwegingen, partnernamen en interne infrastructuur. De
repository is dichtgezet, maar het onderliggende gat zat niet in de instelling:
er was nergens een moment waarop iemand zich afvroeg of dít naar buiten mocht.

Commitboodschappen dragen hetzelfde risico en ze zijn moeilijker terug te nemen:
een issue sluit je, een commit staat in elke kloon. Deze repository gaat publiek
(LC10), dus elke boodschap die er nu bij komt, publiceert zichzelf later.

WAT WEL EN WAT NIET
Dit is geen woordenlijst, want de meeste treffers zijn techniek. `password` is
een functie van deze SDK. `Adobe` en `iText` zijn feiten over de wereld waar een
PDF-bibliotheek vrijuit over hoort te praten. Een controle die daarop afgaat,
roept zo vaak vals alarm dat hij binnen een week wordt uitgezet.

Wat wel tegengehouden wordt, is wat alleen intern betekenis heeft:

  commercieel   uitspraken over omzet, klantaantallen, marges, prijsstrategie
                -- "there are no customers yet" hoort in een bestuurskamer
  partners      namen van klanten en partners; die staan onder een afspraak
  infrastructuur hostnamen, privé-adressen, keychain-accounts, groepspaden
                -- geen geheimen op zichzelf, wel een plattegrond

WAAROM DIT IN DE HAAK ZIT EN NIET ALLEEN IN CI
Een commit die al gepusht is, is al gepubliceerd. De haak is het laatste moment
waarop terugnemen nog goedkoop is. CI is de vangnetlaag voor wie de haak niet
heeft geïnstalleerd -- dezelfde opzet als no_ai_attribution.py, en om dezelfde
reden: `core.hooksPath` maakt een haak op de verkeerde plek onzichtbaar.
"""
import os
import re
import subprocess
import sys

# Alleen wat intern betekenis heeft. Techniek staat er bewust niet in.
REGELS = [
    (
        "commercieel",
        re.compile(
            r"\b(no customers yet|geen klanten|customer count|klantaantal|"
            r"run ?rate|winstmarge|profit margin|"
            r"revenue (that|which) does not exist|omzetdoel|verdienmodel|"
            r"go-to-market|prijsstrategie|pricing strategy)\b",
            re.I,
        ),
    ),
    (
        # Hoofdlettergevoelig: `arr` is een gewoon woord en een variabelenaam,
        # `ARR` is een omzetbegrip. Zonder dat onderscheid sloeg dit aan op
        # `docs/archive/viewer-rust-phase-2-plan.md`, waar het niets te zoeken had.
        "commercieel",
        re.compile(r"\b(MRR|ARR)\b"),
    ),
    (
        "partner",
        # Namen van klanten en partners. Eentje per regel, want dit is een lijst
        # die groeit met de klantenlijst en niet met de taal.
        re.compile(r"\b(Langbly|Instantly\.ai|SnapTale)\b", re.I),
    ),
    (
        "infrastructuur",
        re.compile(
            # `10.x.x.x` stond hier ook. Dat is eruit: een Windows-SDK-versie
            # als `10.0.22621.0` is niet van een privé-adres te onderscheiden,
            # en die staat in elk buildscript. 192.168 is het bereik dat hier
            # werkelijk gebruikt wordt.
            #
            # `keychain` als los woord stond hier ook, en dat was fout: de
            # macOS Keychain is een API die de editor gewoon gebruikt
            # (`src-tauri/src/lib.rs`, `nativeServices.ts`). Gevoelig is niet
            # het woord maar de accountnaam, en die haal je op met deze twee
            # commando's.
            r"(\b192\.168\.\d+\.\d+\b|"
            r"find-internet-password|find-generic-password|"
            r"gitlab\.com/pdfluent-group)",
            re.I,
        ),
    ),
    (
        # Hoofdlettergevoelig, en met de lengte van een echte Windows-hostnaam.
        # Ongevoelig sloeg dit aan op `desktop-app`, `desktop-only` en
        # `desktop-uitleg` -- gewone woorden in een repository over een
        # desktoptoepassing, en precies het soort vals alarm waardoor een
        # controle binnen een week uitstaat.
        "infrastructuur",
        # Minstens één cijfer: Windows verzint namen als DESKTOP-HFR1SL1.
        # Zonder die eis sloeg dit aan op `DESKTOP-NATIVE`, een constante in de
        # editorcode.
        re.compile(r"\bDESKTOP-(?=[A-Z0-9]*[0-9])[A-Z0-9]{5,}\b"),
    ),
]


def overtredingen(tekst):
    uit = []
    for naam, rx in REGELS:
        for m in rx.finditer(tekst):
            begin = max(0, m.start() - 40)
            uit.append((naam, m.group(0), tekst[begin:m.end() + 30].replace("\n", " ").strip()))
    return uit


def uit_bestand(pad):
    with open(pad, encoding="utf-8", errors="ignore") as f:
        # Commentaarregels van git tellen niet mee.
        return "\n".join(r for r in f if not r.startswith("#"))


def _git_omgeving() -> dict[str, str]:
    """De omgeving voor git-aanroepen, zonder de GIT_*-variabelen.

    Dit script draait in de commit-msg-haak, en juist daar zet git `GIT_DIR` en
    `GIT_WORK_TREE` -- absoluut, en een subprocess dat ze erft werkt op de
    repository van de haak in plaats van op de map waarin je hem wijst.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


# ONDERGRENS: commits in het bereik >= 1 -- een leeg bereik betekent dat de
# verwijzing niet klopt (`origin/master` niet opgehaald, verkeerde tak), en dan
# slaagt deze controle zonder iets gelezen te hebben. Dat is niet te
# onderscheiden van een schone historie.
MIN_COMMITS = 1


def uit_git(bereik):
    n = subprocess.run(
        ["git", "rev-list", "--count", bereik],
        capture_output=True, text=True, check=True, env=_git_omgeving(),
    ).stdout.strip()
    if int(n or 0) < MIN_COMMITS:
        raise SystemExit(
            f"geen_interne_zaken: {bereik} bevat {n} commits. De verwijzing klopt "
            "niet -- een leeg bereik keurt alles goed zonder iets te lezen."
        )
    return subprocess.run(
        ["git", "log", "--format=%s%n%b", bereik],
        capture_output=True, text=True, check=True, env=_git_omgeving(),
    ).stdout


# ONDERGRENS: bestanden in de boom >= 1 -- dezelfde redenering als MIN_COMMITS.
# `git ls-files` in een lege of verkeerde map geeft nul bestanden terug, en een
# scan van nul bestanden is niet te onderscheiden van een schone boom.
MIN_BESTANDEN = 1

# Dit script draagt de klant- en partnernamen zelf, in REGELS. Het vindt zichzelf
# dus altijd, en zou zonder deze uitzondering nooit groen kunnen staan.
EIGEN_BESTANDEN = {"scripts/ci/geen_interne_zaken.py"}


def _is_tekst(pad):
    """Een nulbyte in de eerste 8 KiB betekent binair. Dekt de corpus-PDF's."""
    try:
        with open(pad, "rb") as f:
            return b"\0" not in f.read(8192)
    except OSError:
        return False


def uit_boom():
    """Doorzoekt de inhoud van elk getrackt tekstbestand.

    `uit_git` leest commitBOODSCHAPPEN. Een publieke repository lekt net zo goed
    via de bestanden zelf: een klantnaam in een testfixture staat na publicatie
    in elke kloon, ongeacht hoe schoon de boodschap was.
    """
    paden = [
        p for p in subprocess.run(
            ["git", "ls-files", "-z"],
            capture_output=True, text=True, check=True, env=_git_omgeving(),
        ).stdout.split("\0")
        if p and p not in EIGEN_BESTANDEN
    ]
    if len(paden) < MIN_BESTANDEN:
        raise SystemExit(
            f"geen_interne_zaken: de boom bevat {len(paden)} bestanden. De aanroep "
            "klopt niet -- een lege boom keurt alles goed zonder iets te lezen."
        )

    fouten, gelezen = [], 0
    for pad in paden:
        if not _is_tekst(pad):
            continue
        gelezen += 1
        with open(pad, encoding="utf-8", errors="ignore") as f:
            for nr, regel in enumerate(f, 1):
                for naam, rx in REGELS:
                    for m in rx.finditer(regel):
                        fouten.append((naam, m.group(0), f"{pad}:{nr}", regel.strip()[:90]))
    return fouten, gelezen


def _meld_boom(fouten, gelezen):
    if not fouten:
        print(f"OK: {gelezen} getrackte tekstbestanden bevatten geen interne zaken.")
        return 0
    print(
        f"geen_interne_zaken: {len(fouten)} plek(ken) in {gelezen} getrackte\n"
        "tekstbestanden horen niet in een publieke repository. Anders dan een\n"
        "commitboodschap is dit de inhoud zelf: die staat na publicatie in elke kloon.",
        file=sys.stderr,
    )
    for naam, wat, waar, context in fouten[:20]:
        print(f"  [{naam}] {wat}  --  {waar}: {context}", file=sys.stderr)
    if len(fouten) > 20:
        print(f"  ... en nog {len(fouten) - 20}", file=sys.stderr)
    return 1


def main(argv):
    if "--boom" in argv:
        return _meld_boom(*uit_boom())
    if len(argv) > 1 and not argv[1].startswith("-"):
        tekst, waar = uit_bestand(argv[1]), "deze boodschap"
    else:
        bereik = argv[2] if len(argv) > 2 else "origin/master..HEAD"
        tekst, waar = uit_git(bereik), bereik

    fouten = overtredingen(tekst)
    if not fouten:
        print(f"OK: {waar} bevat geen interne zaken.")
        return 0

    print(
        f"geen_interne_zaken: {len(fouten)} plek(ken) in {waar} horen niet in een\n"
        "commitboodschap. Deze repository gaat publiek; een commit staat daarna in\n"
        "elke kloon en is niet meer terug te nemen.",
        file=sys.stderr,
    )
    for naam, wat, context in fouten[:12]:
        print(f"  [{naam}] {wat}  --  …{context}…", file=sys.stderr)
    print(
        "\nHerschrijf de boodschap. Wat er technisch gebeurde mag er staan; waarom\n"
        "het commercieel uitkwam, voor welke klant, en op welke machine niet.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
