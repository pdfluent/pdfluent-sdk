#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Weiger een verwijderde blob op INHOUD, over elke commit die een push toevoegt.

#260. Een document is een keer uit de publieke geschiedenis gehaald en de
geschiedenis is herschreven. `every_document_is_registered.py` vangt een nieuw
bestand in de werkboom, maar niet `git checkout <oude-sha> -- <pad>`: het bestand
komt dan binnen onder een naam die iemand die de geschiedenis niet kent gewoon
aan SOURCES.md toevoegt. Een naamgebaseerde wachter kan dat niet zien.

Deze kijkt daarom naar het object, niet naar de naam. Hernoemen helpt niet,
verplaatsen helpt niet, en het maakt niet uit in welke commit van de push het
zit -- `git rev-list --objects` somt alles op wat het bereik toevoegt, niet
alleen wat er in de top staat.

WAAROM DE SHA'S NIET IN DEZE REPO STAAN
=======================================
De lijst leeft buiten de boom, net als de klant- en partnernamen in
`geen_interne_zaken.py`. #260 zegt het met zoveel woorden: de exacte SHA's en de
bereikbare routes mogen niet herhaald worden in een publieke repo, commitbericht
of gegenereerd document. Een wachter die zijn eigen blokkeerlijst publiceert
vertelt precies waar het weggehaalde nog te halen is.

Om dezelfde reden noemt een treffer het pad wel en de SHA niet. Een CI-log is
geen besloten plek.

Ontbreekt de lijst, dan is dit SKIPPED (not a pass) en geen groen: een controle
die niet kon kijken is geen controle die slaagde. Er is bewust geen ingebouwde
terugval -- die zou de SHA hier alsnog opschrijven.
"""
from __future__ import annotations
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LIJST_PAD = os.environ.get(
    "PDFLUENT_VERBODEN_BLOBS",
    os.path.expanduser("~/.config/pdfluent/verboden-blobs.txt"),
)


def schone_omgeving() -> dict[str, str]:
    """De omgeving van de aanroeper zonder elke GIT_*-variabele.

    Binnen een pre-push-haak wijzen GIT_DIR en GIT_INDEX_FILE naar de echte
    repository, en dan negeert git de map waarin je hem wijst. Op 25-08-2026
    zette een test daardoor `core.bare = true` op de echte repo.

    Dit is de zesde kopie van deze functie in scripts/ci. Ze hoort er een te
    zijn; dat is een opruiming voor wie dit bestand niet toevallig schrijft, en
    geen reden om hier de onveilige variant te gebruiken.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True,
                          text=True, check=False, env=schone_omgeving())


def verboden() -> set[str]:
    """De blokkeerlijst, of SystemExit als hij er niet is."""
    try:
        with open(LIJST_PAD, encoding="utf-8") as f:
            regels = [r.strip().lower() for r in f]
    except OSError:
        raise SystemExit(
            "geen_verwijderde_blob: SKIPPED (not a pass) -- de lijst met "
            f"verwijderde objecten ontbreekt op {LIJST_PAD}.\nZet "
            "PDFLUENT_VERBODEN_BLOBS naar het pad, of maak het bestand aan met "
            "een SHA per regel (`#` is commentaar). De lijst hoort NIET in deze "
            "repo: dan zou hij publiceren wat hij moet tegenhouden."
        )
    return {r for r in regels if r and not r.startswith("#")}


def bereik() -> str:
    """Wat deze push toevoegt.

    Dezelfde redenering als in `every_commit_since_the_cutoff_is_signed.py`:
    de upstream als die er is, anders alles wat niet op master staat, anders de
    hele tak. Die logica staat nu op twee plaatsen en hoort er een te zijn; dat
    is een aparte opruiming, geen reden om hier iets anders te doen.
    """
    boven = git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
    if boven.returncode == 0 and boven.stdout.strip():
        return f"{boven.stdout.strip()}..HEAD"
    for ref in ("github/master", "origin/master"):
        basis = git("merge-base", "HEAD", ref)
        if basis.returncode == 0 and basis.stdout.strip():
            return f"{basis.stdout.strip()}..HEAD"
    return "HEAD"


def main() -> int:
    lijst = verboden()
    r = bereik()
    uit = git("rev-list", "--objects", r)
    if uit.returncode != 0:
        print(f"[verwijderde-blob] SKIPPED (not a pass): `git rev-list --objects "
              f"{r}` faalde:\n  {uit.stderr.strip()[:200]}", file=sys.stderr)
        return 1

    treffers: list[str] = []
    gezien = 0
    for regel in uit.stdout.splitlines():
        if not regel:
            continue
        gezien += 1
        stukken = regel.split(maxsplit=1)
        sha = stukken[0].lower()
        if sha in lijst:
            # Het pad wel, de SHA niet -- zie de moduledocstring.
            treffers.append(stukken[1] if len(stukken) > 1 else "<zonder pad>")

    if treffers:
        print(f"[verwijderde-blob] {len(treffers)} object(en) in {r} staan op de "
              "lijst van wat uit de publieke geschiedenis is gehaald.\n"
              "Ze zijn niet aan de naam te herkennen; dit is het object zelf. "
              "Waarschijnlijk via `git checkout <oude-sha> -- <pad>`.\n",
              file=sys.stderr)
        for pad in sorted(set(treffers))[:25]:
            print(f"  {pad}", file=sys.stderr)
        print("\nHaal de commit weg die het toevoegt. Zie #260 voor waarom dit "
              "niet met een hernoeming op te lossen is.", file=sys.stderr)
        return 1

    # Nul objecten over een bereik dat wel commits heeft, kan niet. Elke commit
    # brengt minstens zichzelf en zijn boom mee. Dat is dus geen lege push maar
    # een bereik dat iets anders meet dan het zou moeten -- en groen melden over
    # nul onderzochte objecten is precies de vorm die deze poorten moeten
    # weigeren.
    commits = git("rev-list", "--count", r)
    n = int(commits.stdout.strip()) if commits.returncode == 0 and commits.stdout.strip() else 0
    if n and not gezien:
        print(f"[verwijderde-blob] SKIPPED (not a pass): {r} bevat {n} commit(s) "
              "maar `rev-list --objects` gaf niets terug. Er is dan niets "
              "onderzocht, en dat is geen groen.", file=sys.stderr)
        return 1

    print(f"[verwijderde-blob] OK: {gezien} object(en) uit {n} commit(s) in {r}, "
          f"geen ervan staat op de lijst van {len(lijst)} verwijderd object(en).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
