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
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CRATES = REPO / "crates"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
from herkomsttabel import UPSTREAM  # noqa: E402  -- één bron voor wat geforkt is

HEADER = """// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
"""
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


def main() -> int:
    schrijven = "--write" in sys.argv
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
                    bron.write_text("".join(regels[:i]) + "\n" + HEADER + "".join(regels[i:]))
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

    if schrijven:
        print(f"OK: {gezien} eigen bronbestanden voorzien van de header.")
        return 0

    if in_fork:
        print(
            f"{len(in_fork)} bestand(en) in een GEFORKTE crate dragen onze licentiekop.\n"
            "Die crates staan onder hun upstream-licentie en NOTICE belooft publiek dat\n"
            "ze zonder commerciele licentie van PDFluent te gebruiken zijn. Onze kop\n"
            "erbovenop zijn twee onverenigbare claims in een bestand.\n",
            file=sys.stderr,
        )
        for q in in_fork[:25]:
            print(f"  {q}", file=sys.stderr)
        return 1

    if verkeerd:
        print(
            f"{len(verkeerd)} van {gezien} eigen bronbestanden dragen een kop die niet\n"
            "het duale model beschrijft. Aanwezigheid is niet genoeg: een kop die zegt\n"
            "dat de software propriëtair is, spreekt LICENSE tegen in het bestand\n"
            "ernaast (#301).\n",
            file=sys.stderr,
        )
        for q in verkeerd[:25]:
            print(f"  {q}", file=sys.stderr)
        if len(verkeerd) > 25:
            print(f"  ... en nog {len(verkeerd) - 25}", file=sys.stderr)
        return 1

    if zonder:
        print(
            f"{len(zonder)} van {gezien} eigen bronbestanden missen de proprietary header.\n"
            "Bij een audit is dat per bestand uitleggen van wie het is.\n",
            file=sys.stderr,
        )
        for q in zonder[:25]:
            print(f"  {q}", file=sys.stderr)
        if len(zonder) > 25:
            print(f"  ... en nog {len(zonder) - 25}", file=sys.stderr)
        print("\nHerstellen: python3 scripts/ci/header_sweep.py --write", file=sys.stderr)
        return 1

    print(f"OK: {gezien} eigen bronbestanden, allemaal met de header.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
