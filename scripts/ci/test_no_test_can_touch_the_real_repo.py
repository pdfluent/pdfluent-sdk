#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Een test die repository's aanmaakt, mag de echte niet kunnen raken.

Op 25-08-2026 zette `test_mr_staleness.py` `core.bare = true` op
`Documents/XFA/.git`. Alle dertig worktrees antwoordden daarna met "fatal: this
operation must be run in a work tree" -- geen status, geen commit, niets.

Hoe dat kon: de test bouwt zijn fixtures met `git init` in een tijdelijke map.
Draaide hij binnen de pre-push-haak, dan stonden `GIT_DIR`, `GIT_INDEX_FILE` en
`GIT_WORK_TREE` in de omgeving, en die zijn absoluut. Git negeert dan de map
waarin je hem wijst. De `git init` liep op de echte repository.

Dat is een andere categorie dan verkeerd meten. Een test die verkeerd meet
geeft een verkeerd antwoord; deze richtte schade aan buiten zichzelf.

Deze lint eist dat elke test die git aanroept, dat doet met de GIT_*-variabelen
uit de omgeving gestript. Niet omdat elke test repository's aanmaakt, maar
omdat je aan de aanroep niet ziet welke dat wel doet -- en de goedkope regel is
dan: geen enkele erft die omgeving.

# ONDERGRENS: onderzochte bestanden >= 5 -- vindt deze lint er bijna geen, dan
# is het patroon veranderd en bewaakt hij niets. Zie #235.
"""

import re
import sys
from pathlib import Path

WORTEL = Path(__file__).resolve().parent.parent.parent
MAP = WORTEL / "scripts" / "ci"

# Een aanroep van git via subprocess.
# Ook met een absoluut pad: `["/usr/bin/git", ...]` is dezelfde aanroep en
# dezelfde schade. the_fork_register_is_verifiable.py schreef het zo en kwam
# er daardoor jarenlang doorheen, terwijl juist die aanroep een force-fetch
# over alle takken doet. (#1642)
GIT_AANROEP = re.compile(r"subprocess\.\w+\(\s*\[\s*[\"'](?:[\w./-]*/)?git[\"']")


def aanroepen_zonder_schone_omgeving(tekst: str) -> list[int]:
    """Regelnummers van git-aanroepen die geen `env=` meekrijgen.

    Per aanroep, niet per bestand. De eerste versie van deze lint keek of het
    bestand ergens een schone omgeving noemde -- en de definitie van de helper
    telde daar zelf al voor mee, dus een aanroep die hem niet gebruikte kwam er
    gewoon doorheen. Dat is dezelfde vorm als #235: de meter keek naar iets dat
    er altijd staat.
    """
    uit = []
    for m in GIT_AANROEP.finditer(tekst):
        # De argumentenlijst van deze ene aanroep uitlezen, op haakjesdiepte.
        i = tekst.index("(", m.start())
        diepte = 0
        for k in range(i, len(tekst)):
            if tekst[k] == "(":
                diepte += 1
            elif tekst[k] == ")":
                diepte -= 1
                if diepte == 0:
                    argumenten = tekst[i : k + 1]
                    break
        else:
            argumenten = tekst[i:]
        if "env=" not in argumenten:
            uit.append(tekst.count("\n", 0, m.start()) + 1)
    return uit


def main() -> int:
    bestanden = sorted(MAP.glob("*.py"))
    if len(bestanden) < 5:
        print(
            f"[geen-echte-repo] maar {len(bestanden)} scripts gevonden; deze lint "
            f"kijkt dan naar bijna niets.",
            file=sys.stderr,
        )
        return 1

    fouten = []
    gecontroleerd = 0
    for pad in bestanden:
        tekst = pad.read_text(encoding="utf-8", errors="replace")
        if not GIT_AANROEP.search(tekst):
            continue
        gecontroleerd += 1
        for regel in aanroepen_zonder_schone_omgeving(tekst):
            fouten.append(
                f"{pad.name}:{regel} roept git aan zonder `env=`, dus met de "
                f"GIT_*-variabelen van de aanroeper"
            )

    print(f"[geen-echte-repo] {gecontroleerd} script(s) die git aanroepen, alle met een schone omgeving")

    if fouten:
        print(
            "\n[geen-echte-repo] deze scripts erven de git-omgeving van hun "
            "aanroeper:\n",
            file=sys.stderr,
        )
        for regel in fouten:
            print(f"  - {regel}", file=sys.stderr)
        print(
            "\n  Binnen een git-haak wijzen GIT_DIR en GIT_WORK_TREE naar de échte\n"
            "  repository, en git negeert dan de map waarin je hem wijst. Op\n"
            "  25-08-2026 zette een test daardoor `core.bare = true` op de echte\n"
            "  repository en lag alles stil.\n"
            "\n  Voeg een helper toe die de GIT_*-variabelen weglaat, en geef die\n"
            "  mee als `env=`. Zie scripts/ci/mr_staleness.py.",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
