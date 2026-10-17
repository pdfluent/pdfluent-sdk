#!/usr/bin/env python3
"""Het patroon van docs_references_resolve.py, tegen wat het moet en niet moet vinden.

Deze wachter heeft maanden gedraaid zonder dat iemand hem heeft zien falen op
iets anders dan een echte kapotte verwijzing -- en hij had een gat: `\\b` grijpt
ook NA een schuine streep, dus elk pad onder `scripts/docs/` werd gelezen alsof
de staart erachter op zichzelf stond. Dat is de dure kant van een vals alarm:
de melding klopt niet, de lezer leert eroverheen te kijken, en de volgende
echte kapotte verwijzing gaat mee in dezelfde beweging.
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from docs_references_resolve import VERWIJZING  # noqa: E402


def gevonden(tekst: str) -> list[str]:
    return VERWIJZING.findall(tekst)


GEVALLEN = [
    # (omschrijving, tekst, wat het patroon moet vinden)
    ("een gewone aanhaling", "zie `docs/error_catalogue.md` hierboven",
     ["docs/error_catalogue.md"]),
    ("een pad met mappen", "docs/archive/oud-plan.md", ["docs/archive/oud-plan.md"]),
    ("aan het begin van de regel", "docs/README.md is het begin", ["docs/README.md"]),
    # Het gat waar dit bestand voor bestaat.
    ("een script onder scripts/docs/", "draai scripts/docs/generate_error_catalogue.py", []),
    ("hetzelfde, in backticks", "`scripts/docs/iets.py` bestaat", []),
    ("een diepere map die op docs eindigt", "a/b/docs/x.md", []),
    ("een woord dat op docs eindigt", "mydocs/x.md", []),
    # De extensie-eis, die er al was.
    ("een map zonder bestandsnaam", "alles onder docs/ is markdown", []),
    ("geen extensie", "docs/naamzonderpunt", []),
]

mislukt = 0
for naam, tekst, verwacht in GEVALLEN:
    uit = gevonden(tekst)
    if uit != verwacht:
        print(f"  FOUT: {naam}: verwacht {verwacht}, kreeg {uit}", file=sys.stderr)
        mislukt += 1
    else:
        print(f"  ok: {naam}")

# En de wachter zelf, op deze repository. Een patroontest die slaagt terwijl de
# wachter omvalt, is geen bewijs van iets.
wachter = pathlib.Path(__file__).resolve().parent / "docs_references_resolve.py"
loop = subprocess.run([sys.executable, str(wachter)], capture_output=True, text=True)
if loop.returncode != 0:
    print(f"  FOUT: de wachter faalt op deze repository:\n{loop.stdout}{loop.stderr}",
          file=sys.stderr)
    mislukt += 1
else:
    print("  ok: de wachter slaagt op deze repository")

if mislukt:
    print(f"\n{mislukt} geval(len) mislukt", file=sys.stderr)
    sys.exit(1)
print(f"[docsrefs] {len(GEVALLEN) + 1} geval(len) goed")
