#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Which public functions does no test in this workspace ever reach?

    python3 scripts/ci/test_reachability.py            schrijft docs/TEST_REACHABILITY.md
    python3 scripts/ci/test_reachability.py --check    faalt als het bestand niet klopt

WAAROM BEREIKBAARHEID EN NIET "STAAT ER EEN TEST"

Eis 1 en 2 van de Definition of Done zeggen: er is een test, en die draait in CI.
De derde eis -- dat de test faalt als de functie eruit gaat -- kan geen script
beoordelen. Wat een script wel kan zien is of er tijdens de tests ooit iets
gebeurt in een functie. Dat is een ondergrens: wordt een functie door niets
uitgevoerd, dan bewaakt niets haar, ook geen assertie die er niet is.

Zoeken op naam in de testmodule is daarvoor te grof. In `pdfa_fonts.rs` gaf dat
12 van de 62 publieke functies, terwijl er 55 daadwerkelijk worden uitgevoerd --
de rest via een hogere ingang. Andersom telt "wordt ergens genoemd" mee wat in
een commentaarregel staat. Daarom volgt dit de aanroepen transitief, vanaf alles
wat de tests rechtstreeks aanroepen.

DRIE DINGEN DIE DIT NIET ZIET, EN DIE HET GETAL DUS OPDRIJVEN

1. **Bindingtests** (Python, Node, Java, .NET, WASM) staan buiten de Rust-boom.
   Een functie die alleen daar wordt uitgeoefend telt hier als onbereikbaar.
2. **`pdf-desktop`-commando's** worden vanuit de frontend aangeroepen, niet
   vanuit Rust. Dat die hier staan is verwacht, geen bevinding.
3. **`examples/`** telt niet mee als test, want een voorbeeld dat draait bewijst
   niets over gedrag.

Omgekeerd is er één ding dat het getal drukt: aanroepherkenning gaat op naam, en
twee functies met dezelfde naam in verschillende modules worden als één gezien.
Bij twijfel meldt dit dus eerder te weinig dan te veel.

DE BEDOELING IS DE RATEL, NIET HET GETAL

`docs/TEST_REACHABILITY.md` legt vast wat er nu onbereikbaar is. `--check` faalt
zodra die lijst niet meer klopt -- in beide richtingen. Een nieuwe publieke
functie zonder test laat de poort omvallen; een functie die dekking krijgt ook,
en dan hoort het bestand in dezelfde wijziging mee te veranderen. Precies zoals
`capability_register.py` dat doet, en om dezelfde reden: een lijst die niemand
bijwerkt is na een maand een bewering waar niets meer achter zit.
"""

from __future__ import annotations

import argparse
import collections
import difflib
import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
CRATES = WORTEL / "crates"
UITVOER = WORTEL / "docs" / "TEST_REACHABILITY.md"

AANROEP = re.compile(r"\b([a-z_][a-z_0-9]{3,})\s*\(")
FUNCTIE = re.compile(r"^(?:pub(?:\([a-z()]+\))? )?fn ([a-z_0-9]+)")
PUBLIEK = re.compile(r"^pub fn ([a-z_0-9]+)", re.M)


def testmodule_start(lijnen: list[str]) -> int:
    """Waar de testmodule van een bestand begint, of het einde als die er niet is.

    Alleen een `#[cfg(test)]` die direct door een `mod` wordt gevolgd telt. De
    eerste `#[cfg(test)]` pakken is fout: bovenin `pdfa_fonts.rs` staan er twee
    op losse hulpjes, en dan geldt bijna het hele bestand als testcode en komt
    er nul onbereikbare functies uit. Die vergissing is bij het schrijven van
    dit script twee keer gemaakt.
    """
    for i, regel in enumerate(lijnen):
        if regel.startswith("#[cfg(test)]"):
            if i + 1 < len(lijnen) and lijnen[i + 1].lstrip().startswith("mod "):
                return i
    return len(lijnen)


# Woorden die een bewuste keuze aankondigen. Alleen doc-commentaar telt: een
# `//`-regel is een kanttekening, een `///`-regel staat in de gegenereerde
# documentatie en is dus geschreven om gelezen te worden.
REDEN_WOORDEN = (
    "not wired", "niet aangesloten", "deliberate", "on purpose", "bewust",
    "measured", "gemeten", "disabled", "uitgezet", "do not wire", "niet aansluiten",
)


# Attributen die zélf de reden zijn. Een functie met `#[tauri::command]` wordt
# per constructie vanuit de frontend aangeroepen en nooit vanuit Rust; hetzelfde
# geldt voor de bindingen naar WASM, Node, Python en Java. Dat is geen
# verzuim en ook geen keuze die iemand nog moet opschrijven -- het staat er al,
# machineleesbaar, en het kan niet verouderen zoals commentaar dat kan.
BUITEN_RUST = (
    "#[tauri::command", "#[wasm_bindgen", "#[napi", "#[pyfunction", "#[pymethods",
    "no_mangle", "extern \"C\"",
)


def heeft_reden(doc: str) -> str | None:
    """Geeft terug wélke soort uitleg er is, of None."""
    if any(a in doc for a in BUITEN_RUST):
        return "buiten Rust"
    laag = doc.lower()
    if any(w in laag for w in REDEN_WOORDEN):
        return "gemeten besluit"
    return None


def lees_workspace() -> tuple[dict[str, list[str]], str, dict[str, set[str]], dict[str, bool]]:
    lichamen: dict[str, list[str]] = {}
    testtekst: list[str] = []
    publiek: dict[str, set[str]] = {}
    redenen: dict[str, bool] = {}

    for crate in sorted(CRATES.iterdir()):
        if not (crate / "src").is_dir():
            continue
        pubs: set[str] = set()
        for bestand in sorted((crate / "src").rglob("*.rs")):
            # Een `mod tests;` die naar src/tests/ wijst is net zo goed testcode
            # als een inline `#[cfg(test)] mod tests`. Dat werd gemist, en het
            # gevolg was zichtbaar in het rapport: pdf-content-stream stond op 10
            # van de 11 publieke functies onbereikbaar, terwijl src/tests/
            # find_span elf keer aanroept. De tool telde die tests als
            # productiecode en zag ze dus nooit als test.
            if "tests" in bestand.relative_to(crate / "src").parts:
                testtekst.append(bestand.read_text(errors="replace"))
                continue

            lijnen = bestand.read_text(errors="replace").split("\n")
            knip = testmodule_start(lijnen)
            productie = lijnen[:knip]
            if knip < len(lijnen):
                testtekst.append("\n".join(lijnen[knip:]))

            grenzen = [
                (i, m.group(1))
                for i, regel in enumerate(productie)
                if (m := FUNCTIE.match(regel))
            ]
            grenzen.append((len(productie), None))
            for (start, naam), (eind, _) in zip(grenzen, grenzen[1:]):
                if naam:
                    lichamen.setdefault(naam, []).append("\n".join(productie[start:eind]))
            pubs |= set(PUBLIEK.findall("\n".join(productie)))

            # Het doc-commentaar direct boven een publieke functie: alles wat
            # vlak ervoor staat en met /// of #[ begint hoort er nog bij.
            for i, regel in enumerate(productie):
                m = PUBLIEK.match(regel)
                if not m:
                    continue
                doc: list[str] = []
                j = i - 1
                while j >= 0 and (productie[j].lstrip().startswith("///")
                                  or productie[j].lstrip().startswith("#[")
                                  or productie[j].lstrip().startswith("//!")):
                    doc.append(productie[j])
                    j -= 1
                redenen[m.group(1)] = heeft_reden("\n".join(doc))

        for map_ in ("tests", "benches"):
            if (crate / map_).is_dir():
                for bestand in sorted((crate / map_).rglob("*.rs")):
                    testtekst.append(bestand.read_text(errors="replace"))

        if pubs:
            publiek[crate.name] = pubs

    return lichamen, "\n".join(testtekst), publiek, redenen


def bereikbaar_vanaf_tests(lichamen: dict[str, list[str]], testtekst: str) -> set[str]:
    bereikt = AANROEP.findall(testtekst)
    bereik = {n for n in bereikt if n in lichamen}
    grens = set(bereik)
    while grens:
        nieuw: set[str] = set()
        for naam in grens:
            for lichaam in lichamen.get(naam, []):
                nieuw |= {n for n in AANROEP.findall(lichaam) if n in lichamen} - bereik
        bereik |= nieuw
        grens = nieuw
    return bereik


def maak_rapport() -> str:
    lichamen, testtekst, publiek, redenen = lees_workspace()
    bereik = bereikbaar_vanaf_tests(lichamen, testtekst)

    per_crate: list[tuple[str, int, list[str]]] = []
    totaal = onbereikt = 0
    for crate, pubs in sorted(publiek.items()):
        onb = sorted(p for p in pubs if p not in bereik)
        totaal += len(pubs)
        onbereikt += len(onb)
        if onb:
            per_crate.append((crate, len(pubs), onb))

    uit: list[str] = []
    uit.append("<!-- Gegenereerd door scripts/ci/test_reachability.py — niet met de hand bewerken. -->")
    uit.append("")
    uit.append("# Publieke functies die geen test bereikt")
    uit.append("")
    uit.append(
        "Een ondergrens, geen dekkingscijfer: dit zegt of een functie tijdens de "
        "tests ooit wordt uitgevoerd, niet of er iets over haar gedrag wordt "
        "beweerd. Wat hier staat wordt door niets uitgevoerd, en dus door niets "
        "bewaakt."
    )
    uit.append("")
    uit.append(
        "Niet meegeteld als test: bindingtests buiten de Rust-boom (Python, Node, "
        "Java, .NET, WASM), en `examples/`. `pdf-desktop`-commando's worden vanuit "
        "de frontend aangeroepen en horen hier dus thuis zonder dat het een "
        "bevinding is."
    )
    uit.append("")
    soorten = collections.Counter(
        redenen.get(n) for _, _, onb in per_crate for n in onb
    )
    met_reden = sum(v for k, v in soorten.items() if k)
    zonder = soorten.get(None, 0)
    uit.append(f"**{onbereikt} van {totaal} publieke functies** ({100 * onbereikt / totaal:.1f}%).")
    uit.append("")
    uit.append(
        f"Daarvan dragen er **{met_reden}** een uitleg: "
        f"{soorten.get('buiten Rust', 0)} worden per constructie van buiten Rust "
        f"aangeroepen (`#[tauri::command]`, een binding-attribuut, een C-ABI-export) "
        f"en {soorten.get('gemeten besluit', 0)} dragen een gemeten besluit in hun "
        "eigen doc-commentaar."
    )
    uit.append("")
    uit.append(
        f"De overige **{zonder}** staan hier zonder uitleg. Dat is het getal dat "
        "omlaag hoort, en het is geen verzameling besluiten maar een lijst gaten: "
        "niemand heeft ze getest en niemand heeft opgeschreven waarom niet. Een "
        "functie die hier bij komt verschijnt in de diff als "
        "`(geen reden opgegeven)` — precies waar een reviewer kijkt."
    )
    uit.append("")
    uit.append("| crate | onbereikt | publiek |")
    uit.append("|---|---:|---:|")
    for crate, aantal, onb in sorted(per_crate, key=lambda r: -len(r[2])):
        uit.append(f"| `{crate}` | {len(onb)} | {aantal} |")
    uit.append("")
    for crate, _, onb in sorted(per_crate):
        uit.append(f"## {crate}")
        uit.append("")
        for naam in onb:
            soort = redenen.get(naam)
            merk = f"  — *{soort}*" if soort else "  — *(geen reden opgegeven)*"
            uit.append(f"- `{naam}`{merk}")
        uit.append("")
    return "\n".join(uit).rstrip() + "\n"


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--check", action="store_true",
                   help="faal wanneer docs/TEST_REACHABILITY.md niet meer klopt")
    a = p.parse_args()

    nieuw = maak_rapport()
    if not a.check:
        UITVOER.parent.mkdir(parents=True, exist_ok=True)
        UITVOER.write_text(nieuw)
        print(f"[test_reachability] geschreven: {UITVOER.relative_to(WORTEL)}")
        return 0

    if not UITVOER.exists():
        print(f"[test_reachability] FAIL: {UITVOER.relative_to(WORTEL)} ontbreekt.", file=sys.stderr)
        print("[test_reachability] Draai: python3 scripts/ci/test_reachability.py", file=sys.stderr)
        return 1

    oud = UITVOER.read_text()
    if oud == nieuw:
        print("[test_reachability] up to date")
        return 0

    print("[test_reachability] FAIL: de lijst klopt niet meer met de code.")
    print("[test_reachability] Er is een publieke functie bijgekomen die geen test bereikt,")
    print("[test_reachability] of er is er een gedekt geraakt. Beide horen hier te blijken.")
    print("")
    for regel in difflib.unified_diff(
        oud.splitlines(), nieuw.splitlines(),
        fromfile="vastgelegd", tofile="gemeten", lineterm="", n=1,
    ):
        print(f"[test_reachability] {regel}")
    print("")
    print("[test_reachability] Draai: python3 scripts/ci/test_reachability.py")
    print("[test_reachability] en commit het resultaat in dezelfde wijziging.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
