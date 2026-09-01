#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
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

HEADER = """// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.
"""
MERK = "Innovation Trigger B.V."

# ONDERGRENS: onderzochte bestanden >= 200 -- er zijn er ruim driehonderd in de
# eigen crates. Minder betekent dat het zoeken stuk is en niet dat het werk af is.
MIN_BESTANDEN = 200


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

    for crate in sorted(p for p in CRATES.iterdir() if p.is_dir()):
        if is_geforkt(crate):
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
            if MERK in "".join(tekst.splitlines(keepends=True)[:60]):
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
