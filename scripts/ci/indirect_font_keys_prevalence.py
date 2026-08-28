#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""How often are font-dictionary keys indirect references rather than direct values?

Run: python3 scripts/ci/indirect_font_keys_prevalence.py <map-met-pdfs> [--limit N]

De meting achter de /Differences-reparatie in sync_widths_from_embedded_fonts
is hiermee gedaan, op de VPS omdat de buildmachine bezet was:

    python3 indirect_font_keys_prevalence.py /mnt/storagebox/corpus/govdocs --limit 3000

Uitkomst 21-08-2026: 36322 lettertypen in 3000 documenten. /FirstChar en
/LastChar nul keer indirect, /FontDescriptor nul keer direct, /Widths 6,02%
indirect (ruis: de waarden kloppen), /Encoding 15,97% indirect. Op alleen
het codepad van de breedtesynchronisatie -- ingebed, simpel, geen Type0 --
draagt 20,08% een /Encoding-woordenboek met /Differences.

WAAROM DEZE METING BESTAAT

pdfa_fonts.rs leest een aantal sleutels alleen in hun directe vorm:

    match font.get(b"FirstChar").ok() { Some(Object::Integer(i)) => *i as u32, _ => 0 }
    match font.get(b"Widths").ok()    { Some(Object::Array(a))   => a.clone(), _ => vec![] }
    match dict.get(b"FontDescriptor").ok() { Some(Object::Reference(id)) => ..., _ => continue }

Alle drie zijn in ISO 32000 net zo goed indirect toegestaan. Wat er dan gebeurt
verschilt per sleutel, en dat is precies waarom tellen nodig is voordat er iets
verbouwd wordt:

  FirstChar/LastChar indirect  -> stilletjes terugvallen op 0 en 255, waarna er
                                  een /Widths van de verkeerde lengte op de
                                  verkeerde plek wordt weggeschreven. Foute uitvoer.
  Widths indirect              -> telt als leeg, dus altijd "changed"; de waarden
                                  die eruit komen kloppen wel. Ruis, geen fout.
  FontDescriptor direct        -> het hele lettertype wordt overgeslagen. Stille
                                  overslag, en dat is de ergste van de drie.

Zonder deze telling is niet te zeggen of dat een randgeval is of dagelijks werk,
en dus ook niet of de verbouwing van tien leesplekken de moeite waard is.

Vereist pikepdf. Leest alleen; schrijft geen enkel document.
"""

from __future__ import annotations

import argparse
import collections
import json
import random
import sys
from pathlib import Path

try:
    import pikepdf
except ImportError:
    print("SKIPPED (not a pass): pikepdf ontbreekt — pip install pikepdf", file=sys.stderr)
    raise SystemExit(1)

# De sleutels die pdfa_fonts.rs alleen direct leest, met de vorm die het
# verwacht. "direct" betekent: de code werkt alleen als de waarde inline staat.
VERWACHT_DIRECT = ["/FirstChar", "/LastChar", "/Widths", "/Encoding"]
# FontDescriptor is het spiegelbeeld: de code werkt alleen bij een verwijzing.
VERWACHT_INDIRECT = ["/FontDescriptor"]


def is_indirect(obj) -> bool:
    """pikepdf houdt de objgen vast; (0,0) betekent inline."""
    try:
        return obj.objgen != (0, 0)
    except AttributeError:
        return False


def onderzoek(pad: Path) -> dict[str, int] | None:
    telling: dict[str, int] = collections.Counter()
    try:
        with pikepdf.open(pad) as pdf:
            for obj in pdf.objects:
                try:
                    if not isinstance(obj, pikepdf.Dictionary):
                        continue
                    if obj.get("/Type") != "/Font":
                        continue
                except Exception:
                    continue
                telling["lettertypen"] += 1
                for sleutel in VERWACHT_DIRECT:
                    try:
                        waarde = obj.get(sleutel)
                    except Exception:
                        continue
                    if waarde is None:
                        continue
                    if is_indirect(waarde):
                        telling[f"indirect{sleutel}"] += 1
                for sleutel in VERWACHT_INDIRECT:
                    try:
                        waarde = obj.get(sleutel)
                    except Exception:
                        continue
                    if waarde is None:
                        continue
                    if not is_indirect(waarde):
                        telling[f"direct{sleutel}"] += 1
    except Exception:
        return None
    return dict(telling)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("map", type=Path)
    p.add_argument("--limit", type=int, default=0, help="hoogstens zoveel documenten")
    p.add_argument("--seed", type=int, default=20260821,
                   help="zaad voor de steekproef; met --limit wordt er willekeurig "
                        "getrokken in plaats van het eerste blok genomen, want "
                        "bestandsnamen in govdocs lopen op en de eerste N is dus "
                        "een aaneengesloten stuk en geen doorsnede")
    p.add_argument("--json", type=Path, help="schrijf de telling hierheen")
    a = p.parse_args()

    bestanden = sorted(a.map.rglob("*.pdf"))
    if a.limit and len(bestanden) > a.limit:
        random.Random(a.seed).shuffle(bestanden)
        bestanden = bestanden[: a.limit]
    if not bestanden:
        print(f"SKIPPED (not a pass): geen pdf's onder {a.map}", file=sys.stderr)
        return 1

    totaal: dict[str, int] = collections.Counter()
    gelezen = 0
    onleesbaar = 0
    getroffen: dict[str, list[str]] = collections.defaultdict(list)

    for f in bestanden:
        r = onderzoek(f)
        if r is None:
            onleesbaar += 1
            continue
        gelezen += 1
        for k, v in r.items():
            totaal[k] += v
            if k != "lettertypen" and len(getroffen[k]) < 5:
                getroffen[k].append(f.name)

    lettertypen = totaal.get("lettertypen", 0)
    print(f"documenten gelezen:  {gelezen}  (onleesbaar: {onleesbaar})")
    print(f"lettertypen gezien:  {lettertypen}")
    if lettertypen == 0:
        print("geen lettertypen aangetroffen — deze verzameling zegt niets")
        return 0

    print()
    print("sleutels die de code alleen DIRECT leest, maar indirect aantreft:")
    for sleutel in VERWACHT_DIRECT:
        n = totaal.get(f"indirect{sleutel}", 0)
        pct = 100.0 * n / lettertypen
        merk = "  <-- foute uitvoer" if sleutel in ("/FirstChar", "/LastChar") and n else ""
        print(f"  {sleutel:<16} {n:>6} van {lettertypen}  ({pct:5.2f}%){merk}")
        if n:
            print(f"                   bv. {', '.join(getroffen[f'indirect{sleutel}'])}")

    print()
    print("sleutels die de code alleen INDIRECT leest, maar direct aantreft:")
    for sleutel in VERWACHT_INDIRECT:
        n = totaal.get(f"direct{sleutel}", 0)
        pct = 100.0 * n / lettertypen
        merk = "  <-- lettertype stil overgeslagen" if n else ""
        print(f"  {sleutel:<16} {n:>6} van {lettertypen}  ({pct:5.2f}%){merk}")
        if n:
            print(f"                   bv. {', '.join(getroffen[f'direct{sleutel}'])}")

    if a.json:
        a.json.write_text(json.dumps(
            {"documenten": gelezen, "onleesbaar": onleesbaar, **totaal}, indent=2))
        print(f"\ngeschreven: {a.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
