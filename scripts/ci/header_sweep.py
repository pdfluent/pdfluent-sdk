#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Elke eigen bronregel draagt de proprietary header. Geforkte code niet.

LC1 (#213). Headers zijn hoe je bij een audit per bestand herkomst aantoont: een
bestand zonder header is een bestand waarvan je moet uitleggen van wie het is.

DE SWEEP IS HERKOMSTBEWUST, EN DAT IS HET HELE PUNT
De negen geforkte crates -- hayro, lopdf, cff-parser -- houden hun upstream-notice
en krijgen onze header **niet**. Een sweep die alles aandoet, claimt andermans
werk, en dat is een ernstiger fout dan een ontbrekende header. Welke crate wat is,
staat in `scripts/ci/herkomsttabel.py`; deze leest die lijst zodat er één bron is.

Zonder argumenten controleert het en faalt bij een gat. Met `--write` zet het de
headers erin.
"""
import os
import pathlib
import sys

# De wortel is overschrijfbaar zodat een test op een eigen boom kan meten.
#
# Zonder dat kan een test alleen de losse functies aanroepen, en dan blijft de
# bedrading -- welke bestanden gekozen worden, in welke categorie ze vallen, wat
# er gemeld wordt -- ongetest. Dat is net het deel waar de fouten in zaten.
REPO = pathlib.Path(os.environ.get("PDFLUENT_HEADER_SWEEP_ROOT")
                    or pathlib.Path(__file__).resolve().parents[2])
CRATES = REPO / "crates"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
from herkomsttabel import UPSTREAM  # noqa: E402  -- één bron voor wat geforkt is

HEADER = """// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
"""
# Dezelfde tekst in `#`-commentaar. Python, shell en TOML dragen hem ook: 107
# gepubliceerde bestanden zeiden "This software is proprietary" naast een LICENSE
# die de AGPL aanbiedt, en 89 daarvan waren scripts. (#301)
HEADER_HASH = "\n".join("#" + r[2:] if r.startswith("//") else r
                        for r in HEADER.rstrip("\n").split("\n")) + "\n"

# Per extensie: het commentaarteken en de kop die erbij hoort.
STIJL = {".rs": ("//", HEADER), ".py": ("#", HEADER_HASH),
         ".sh": ("#", HEADER_HASH), ".toml": ("#", HEADER_HASH)}

MERK = "Innovation Trigger B.V."

# WAT DE KOP MOET ZEGGEN, NIET ALLEEN DAT ER EEN KOP STAAT
#
# Tot 01-09-2026 controleerde dit bestand alleen of MERK in de eerste zestig
# regels stond. Een kop met willekeurige inhoud kwam er dus door zolang de
# bedrijfsnaam erin voorkwam -- en dat is precies wat er gebeurde: 800 bestanden
# droegen "This software is proprietary ... See https://pdfluent.com/license",
# het model dat op 24-08 was afgeschaft, en deze poort stond al die tijd op
# groen. Een poort die aanwezigheid toetst in plaats van inhoud, keurt de fout
# goed die hij moet vangen (#301).
LICENTIEREGEL = "PDFluent is available under two licences"

# En andersom: onze kop hoort NIET in een geforkte crate. Twee bestanden in
# hayro-jbig2 en lopdf droegen hem wel, tegenover een NOTICE die publiek belooft
# dat die crates zonder commerciële licentie van PDFluent te gebruiken zijn.
# Twee onverenigbare claims in één bestand.

# ONDERGRENS: onderzochte bestanden >= 200 -- er zijn er ruim driehonderd in de
# eigen crates. Minder betekent dat het zoeken stuk is en niet dat het werk af is.
MIN_BESTANDEN = 200


def crate_licentie(crate: pathlib.Path) -> str | None:
    """De SPDX-expressie uit de Cargo.toml van de crate, of None."""
    toml = crate / "Cargo.toml"
    if not toml.is_file():
        return None
    import re as _re
    m = _re.search(r'^\s*license\s*=\s*"([^"]+)"', toml.read_text(errors="replace"), _re.M)
    return m.group(1) if m else None


def is_geforkt(crate: pathlib.Path) -> bool:
    if crate.name in UPSTREAM:
        return True
    toml = (crate / "Cargo.toml")
    if not toml.is_file():
        return False
    import re

    m = re.search(r'^\s*name\s*=\s*"([^"]+)"', toml.read_text(errors="replace"), re.M)
    return bool(m and m.group(1) in UPSTREAM)


PROPRIETAIR = "This software is proprietary"

# Waar `#`-bestanden vandaan komen. Niet elke .py of .sh: 281 van de 370 onder
# scripts/ dragen helemaal geen kop, en die er een opleggen is een NIEUWE eis,
# geen reparatie. Deze pas kijkt alleen naar bestanden die AL een kop dragen en
# daarin het verkeerde zeggen -- de tegenspraak uit #301. Een bestand zonder kop
# is een aparte keuze, niet deze.
HASH_MAPPEN = ("scripts", "crates", "benchmarks")


def hash_bestanden() -> list[pathlib.Path]:
    uit = []
    for map_ in HASH_MAPPEN:
        wortel = REPO / map_
        if not wortel.is_dir():
            continue
        for suffix in (".py", ".sh", ".toml"):
            for bron in wortel.rglob("*" + suffix):
                if "target" in bron.parts or ".git" in bron.parts:
                    continue
                uit.append(bron)
    return sorted(set(uit))


def hash_kopblok(tekst: str) -> str:
    """Het kopblok van een `#`-bestand: shebang plus het aaneengesloten commentaar.

    Niet "de eerste zestig regels". Deze wachter noemt zijn eigen bron een
    bestand met een propriëtaire kop, omdat de term daar als stringliteral staat
    -- twintig regels onder de kop, in de code die eropzoek is. Dat is dezelfde
    fout als een probe die zijn eigen proza meetelt: hij vindt zichzelf.

    Een kop is een kop door zijn plaats, niet doordat de woorden ergens bovenin
    voorkomen.
    """
    regels = tekst.splitlines(keepends=True)
    i = 1 if regels and regels[0].startswith("#!") else 0
    eind = i
    while eind < len(regels) and (regels[eind].lstrip().startswith("#")
                                  or not regels[eind].strip()):
        if not regels[eind].strip() and eind > i:
            break
        eind += 1
    return "".join(regels[i:eind])


def vervang_hash_kop(tekst: str) -> str | None:
    """De propriëtaire kop vervangen door de duale, of None als er niets staat.

    De shebang blijft op regel 1, zoals een `#![...]`-attribuut bij Rust: er mag
    niets vóór staan. Het blok dat verdwijnt is het aaneengesloten `#`-commentaar
    dat met de copyrightregel begint.
    """
    regels = tekst.splitlines(keepends=True)
    begin = next((i for i, r in enumerate(regels[:60]) if MERK in r), None)
    if begin is None:
        return None
    eind = begin
    while eind < len(regels) and regels[eind].lstrip().startswith("#"):
        eind += 1
    return "".join(regels[:begin]) + HEADER_HASH + "".join(regels[eind:])


def main() -> int:
    schrijven = "--write" in sys.argv
    hersteld = 0
    zonder, gezien = [], 0

    verkeerd, in_fork = [], []

    for crate in sorted(p for p in CRATES.iterdir() if p.is_dir()):
        if is_geforkt(crate):
            # De kop van een bestand moet passen bij de licentie-expressie van de
            # crate waarin het staat. `license = "Apache-2.0 OR MIT"` in de
            # Cargo.toml is een belofte over ELKE file erin, dus een proprietary
            # kop daarbinnen is een tegenspraak die elke SCA-scanner ziet.
            #
            # Een bestand dat wij aan zo'n crate toevoegen mag onze copyrightregel
            # dragen -- maar dan onder de licentie van de crate, en die moet er dan
            # ook staan. Alleen een copyrightregel laat in het midden waaronder het
            # valt, en dat is precies wat hier niet mag blijven staan.
            expr = crate_licentie(crate)
            for bron in sorted(crate.rglob("*.rs")):
                if "target" in bron.parts:
                    continue
                kop = "".join(bron.read_text(errors="replace").splitlines(keepends=True)[:60])
                if LICENTIEREGEL in kop or "This software is proprietary" in kop:
                    in_fork.append(str(bron.relative_to(REPO)))
                elif MERK in kop and expr and expr not in kop:
                    in_fork.append(f"{bron.relative_to(REPO)} (our copyright without "
                                   f"naming the crate's licence {expr!r})")
            continue
        for bron in sorted(crate.rglob("*.rs")):
            if "target" in bron.parts:
                continue
            gezien += 1
            tekst = bron.read_text(errors="replace")
            # De eerste zestig regels, niet de eerste 400 tekens.
            #
            # Een crate-doccomment mag lang zijn: `formcalc-interpreter/src/lib.rs`
            # draagt negen `//!`-regels vóór de header, en die passen samen niet
            # in 400 tekens. De controle meldde toen 176 ontbrekende headers die
            # er gewoon stonden -- en de sweep die je dan draait, zet ze er een
            # tweede keer in.
            regels60 = tekst.splitlines(keepends=True)[:60]
            kop = "".join(regels60)
            if MERK in kop:
                # De kop staat er. Zegt hij ook het juiste?
                #
                # Het venster wordt aan de kop zelf verankerd en niet blind
                # opgerekt: `regex_guard.rs` draagt 58 regels `//!` en dan de
                # header, zodat MERK op regel 59 net binnen de zestig valt en de
                # licentieregel op 61 er net buiten. Dat als "verkeerde kop"
                # melden zou een vals alarm zijn van het meetvenster, niet een
                # bevinding over het bestand.
                i = next(j for j, r in enumerate(regels60) if MERK in r)
                blok = "".join(tekst.splitlines(keepends=True)[i:i + 8])
                if LICENTIEREGEL not in blok:
                    if schrijven:
                        # De oude kop VERVANGEN, niet ernaast schrijven. Zonder
                        # dit pad kon `--write` alleen aanvullen wat ontbrak en
                        # niets repareren wat fout stond -- en meldde intussen
                        # "N bestanden voorzien van de header" over bestanden die
                        # het niet had aangeraakt. (#301)
                        regels = tekst.splitlines(keepends=True)
                        eind = i
                        while eind < len(regels) and regels[eind].lstrip().startswith("//"):
                            eind += 1
                        bron.write_text("".join(regels[:i]) + HEADER
                                        + "".join(regels[eind:]), encoding="utf-8")
                        hersteld += 1
                    else:
                        verkeerd.append(str(bron.relative_to(REPO)))
                continue
            if schrijven:
                # Een `#![…]`-attribuut hoort bovenaan te blijven staan; de
                # header gaat er dan onder, want een crate-attribuut mag niets
                # anders dan commentaar boven zich hebben.
                regels = tekst.splitlines(keepends=True)
                i = 0
                while i < len(regels) and (
                    regels[i].startswith("#!") or regels[i].startswith("//!")
                ):
                    i += 1
                if i:
                    # Een lege regel na een `//!`-doccomment leest beter, maar na
                    # een `#![…]`-attribuut haalt rustfmt hem weg -- en dan
                    # produceert deze sweep zelf de diff waarop de fmt-poort rood
                    # gaat. Een reparatie die de volgende poort breekt is geen
                    # reparatie.
                    wit = "\n" if regels[i - 1].startswith("//!") else ""
                    bron.write_text("".join(regels[:i]) + wit + HEADER + "".join(regels[i:]))
                else:
                    bron.write_text(HEADER + "\n" + tekst)
            else:
                zonder.append(str(bron.relative_to(REPO)))


    if gezien < MIN_BESTANDEN:
        print(
            f"ONDERGRENS: {gezien} bestanden onderzocht, verwacht >= {MIN_BESTANDEN}. "
            "Het zoeken is stuk -- dit is geen groen.",
            file=sys.stderr,
        )
        return 1

    # De `#`-bestanden worden hier gemeten, niet na de rapportage.
    #
    # Ze stonden eerst achter de laatste `return 1`. Dat betekende dat ze
    # onbereikbaar waren zolang er iets anders rood stond -- en dat de melding
    # "OK: N eigen bronbestanden, allemaal met de header" al gedrukt was voordat
    # de scan die alsnog rood kon maken uberhaupt liep. `--write` keerde zelfs
    # terug voordat hij eraan toekwam: hij schreef de `.rs`-koppen, meldde dat
    # alles voorzien was, en liet de 88 tegensprekende `#`-koppen staan.
    #
    # Dat is dezelfde vorm als de reden dat #301 bestaat: een uitslag die iets
    # bevestigt over het deel dat hij bekeek en zwijgt over het deel dat hij
    # oversloeg. Een wachter met meerdere categorieen moet ze alle verzamelen en
    # daarna een keer oordelen, anders vertelt de eerste rode categorie je nooit
    # dat er een tweede is.
    hash_fout: list[str] = []
    hash_hersteld = 0
    for bron in hash_bestanden():
        tekst = bron.read_text(encoding="utf-8", errors="replace")
        if PROPRIETAIR not in hash_kopblok(tekst):
            continue
        if schrijven:
            nieuw_ = vervang_hash_kop(tekst)
            if nieuw_ is not None:
                bron.write_text(nieuw_, encoding="utf-8")
                hash_hersteld += 1
        else:
            hash_fout.append(str(bron.relative_to(REPO)))

    if schrijven:
        print(f"OK: {gezien} `.rs`-bestanden gezien, {hersteld} kop vervangen; "
              f"{hash_hersteld} `#`-kop vervangen.")
        return 0

    # Alle categorieen, daarna een oordeel.
    secties: list[tuple[str, list[str]]] = [
        (f"{len(in_fork)} bestand(en) in een GEFORKTE crate dragen onze licentiekop.\n"
         "Die crates staan onder hun upstream-licentie en NOTICE belooft publiek dat\n"
         "ze zonder commerciele licentie van PDFluent te gebruiken zijn. Onze kop\n"
         "erbovenop zijn twee onverenigbare claims in een bestand.", in_fork),
        (f"{len(verkeerd)} van {gezien} eigen bronbestanden dragen een kop die niet\n"
         "het duale model beschrijft. Aanwezigheid is niet genoeg: een kop die zegt\n"
         "dat de software propriëtair is, spreekt LICENSE tegen in het bestand\n"
         "ernaast (#301).", verkeerd),
        (f"{len(hash_fout)} bestand(en) met een `#`-kop zeggen dat de software\n"
         "propriëtair is, naast een LICENSE die de AGPL aanbiedt. Deze gaan publiek\n"
         "mee; de lezer ziet de kop eerder dan het licentiebestand.", hash_fout),
        (f"{len(zonder)} van {gezien} eigen bronbestanden dragen helemaal geen kop.\n"
         "Bij een audit is dat per bestand uitleggen van wie het is.", zonder),
    ]
    rood = False
    for kop_regel, lijst in secties:
        if not lijst:
            continue
        rood = True
        print(kop_regel + "\n", file=sys.stderr)
        for q in lijst[:25]:
            print(f"  {q}", file=sys.stderr)
        if len(lijst) > 25:
            print(f"  ... en nog {len(lijst) - 25}", file=sys.stderr)
        print("", file=sys.stderr)
    if rood:
        print("Herstellen: python3 scripts/ci/header_sweep.py --write", file=sys.stderr)
        return 1

    print(f"OK: {gezien} eigen bronbestanden, allemaal met de duale kop.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
