#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the text-replacement corpus gate — specifically what it passes on.

Run: python3 scripts/ci/test_text_replace_gate.py

WAAROM DIT BESTAAT

`corpus:text-replace-capability` gaf `--fallback standard` mee aan dit script.
Het script kende die vlag niet, faalde binnen één seconde op het parsen, en de
meting die het moest opleveren — tilt fallback het vervangingspercentage? — is
daardoor nooit gedraaid. De job stond er wel. Er kwam alleen nooit iets uit.

Dat is exact het patroon uit de Definition of Done: het bestond, en niets voerde
het uit. Een test op de vlag zelf zou dat in seconden hebben gevangen.

De aanpak is een namaak-runner: een scriptje dat zijn eigen argumenten wegschrijft
in plaats van een PDF te bewerken. Daarmee is te zien wát de gate doorgeeft,
zonder corpus, zonder Rust en zonder pdftotext-afhankelijk gedrag. Dat is ook
precies de laag waar de fout zat — niet in het vervangen, maar in het doorgeven.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
GATE = HERE / "text_replace_corpus_gate.py"

mislukt: list[str] = []


def controleer(naam: str, voorwaarde: bool, toelichting: str = "") -> None:
    if voorwaarde:
        print(f"  ok    {naam}")
    else:
        print(f"  FOUT  {naam}  {toelichting}")
        mislukt.append(naam)


NAMAAK_RUNNER = r'''#!/usr/bin/env python3
"""Schrijft zijn argumenten weg en maakt een leeg uitvoerbestand."""
import json, os, pathlib, sys
log = pathlib.Path(os.environ["RUNNER_LOG"])
regels = json.loads(log.read_text()) if log.exists() else []
regels.append(sys.argv[1:])
log.write_text(json.dumps(regels))
argv = sys.argv[1:]
if "--output" in argv:
    uit = pathlib.Path(argv[argv.index("--output") + 1])
    uit.write_bytes(b"%PDF-1.4\n%stub\n")
'''


NAMAAK_PDFTOTEXT = r'''#!/usr/bin/env python3
"""Levert altijd dezelfde zin, zodat de gate een woord kan kiezen.

Nodig omdat de gate zijn zoekwoord uit de tekst van het document haalt: zonder
leesbare tekst komt hij nooit toe aan het aanroepen van de runner, en dan toetst
deze test niets. Een echt PDF maken zou een PDF-schrijver in de test halen om
iets te toetsen dat niets met PDF te maken heeft.
"""
import sys
print("Dit document bevat het woord VERVANGBAAR om te vinden.")
'''


def draai_gate(corpus: Path, runner: Path, log: Path, extra: list[str]) -> subprocess.CompletedProcess:
    sample = corpus / "sample.txt"
    sample.write_text("een.pdf\n")
    omgeving = dict(os.environ, RUNNER_LOG=str(log))
    return subprocess.run(
        [sys.executable, str(GATE),
         "--corpus-dir", str(corpus),
         "--runner", str(runner),
         "--pdftotext", str(corpus / "namaak_pdftotext.py"),
         "--sample-list", str(sample),
         "--baseline", str(corpus / "baseline.json"),
         "--workdir", str(corpus / "work"),
         "--write-baseline", *extra],
        capture_output=True, text=True, env=omgeving, timeout=120,
    )


def opstelling(tmp: str) -> tuple[Path, Path, Path]:
    corpus = Path(tmp)
    # Een document met genoeg tekst dat de gate er een woord van zes letters in
    # vindt; anders komt hij nooit tot het aanroepen van de runner.
    (corpus / "een.pdf").write_bytes(b"%PDF-1.4\n%stub\n")
    runner = corpus / "namaak_runner.py"
    runner.write_text(NAMAAK_RUNNER)
    runner.chmod(0o755)
    lezer = corpus / "namaak_pdftotext.py"
    lezer.write_text(NAMAAK_PDFTOTEXT)
    lezer.chmod(0o755)
    return corpus, runner, corpus / "runner.log"


def test_de_gate_accepteert_de_fallback_vlag() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        corpus, runner, log = opstelling(tmp)
        p = draai_gate(corpus, runner, log, ["--fallback", "standard"])
        controleer(
            "--fallback standard wordt geaccepteerd",
            "unrecognized arguments" not in p.stderr,
            p.stderr.strip()[-160:],
        )


def test_een_onbekende_fallback_wordt_geweigerd() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        corpus, runner, log = opstelling(tmp)
        p = draai_gate(corpus, runner, log, ["--fallback", "onzin"])
        controleer(
            "een onbekende fallback wordt geweigerd",
            p.returncode != 0 and "invalid choice" in p.stderr,
            f"code={p.returncode} {p.stderr.strip()[-120:]}",
        )


def test_de_vlag_bereikt_de_runner() -> None:
    """De assertie die de oorspronkelijke fout zou hebben gevangen.

    Accepteren is niet genoeg: het script kan de vlag netjes parsen en hem
    vervolgens nergens heen sturen. Dan meet de job iets anders dan zijn kop
    zegt, en dat is erger dan falen.
    """
    for keuze in ("deny", "standard"):
        with tempfile.TemporaryDirectory() as tmp:
            corpus, runner, log = opstelling(tmp)
            draai_gate(corpus, runner, log, ["--fallback", keuze])
            if not log.exists():
                controleer(
                    f"--fallback {keuze} bereikt de runner",
                    False,
                    "de runner is nooit aangeroepen; het document leverde geen woord op",
                )
                continue
            aanroepen = json.loads(log.read_text())
            goed = any("--fallback" in a and a[a.index("--fallback") + 1] == keuze
                       for a in aanroepen)
            controleer(
                f"--fallback {keuze} bereikt de runner",
                goed,
                f"argumenten: {aanroepen[:1]}",
            )


def test_een_basislijn_draagt_zijn_methode() -> None:
    """De geschreven basislijn zegt onder welke keuzeregel hij is gemaakt."""
    with tempfile.TemporaryDirectory() as tmp:
        corpus, runner, log = opstelling(tmp)
        draai_gate(corpus, runner, log, [])
        inhoud = json.loads((corpus / "baseline.json").read_text())
        controleer(
            "een basislijn draagt zijn methode",
            inhoud.get("_methodology"),
            "zonder markering is niet te zien of een vergelijking geldig is",
        )


def test_een_andere_methode_oordeelt_niet_maar_legt_opnieuw_vast() -> None:
    """Verandert de keuzeregel, dan zijn per-document-uitslagen onvergelijkbaar.

    Vergelijken zou verschillen melden die niets over de motor zeggen — op 22-08
    waren dat er veertien. Zwijgend doorgaan zou echte regressies verbergen. Dus
    opnieuw vastleggen, luid, en niets oordelen.
    """
    with tempfile.TemporaryDirectory() as tmp:
        corpus, runner, log = opstelling(tmp)
        # Een basislijn van een oudere regel, met een uitslag die anders als
        # regressie zou tellen.
        (corpus / "baseline.json").write_text(json.dumps({
            "_methodology": "needle-first-v1",
            "een.pdf": {"name": "een.pdf", "usable": True, "engine_found": True,
                        "replaced": True, "extractable": True, "note": "",
                        "needle": "", "replacement": ""},
        }))
        sample = corpus / "sample.txt"
        sample.write_text("een.pdf\n")
        r = subprocess.run(
            [sys.executable, str(GATE),
             "--corpus-dir", str(corpus), "--runner", str(runner),
             "--pdftotext", str(corpus / "namaak_pdftotext.py"),
             "--sample-list", str(sample),
             "--baseline", str(corpus / "baseline.json"),
             "--workdir", str(corpus / "work2")],
            capture_output=True, text=True,
            env=dict(os.environ, RUNNER_LOG=str(log)), timeout=120,
        )
        controleer(
            "een andere methode oordeelt niet",
            r.returncode == 0 and "METHODOLOGY CHANGED" in r.stdout,
            f"rc={r.returncode}, uitvoer: {r.stdout[-300:]}",
        )
        opnieuw = json.loads((corpus / "baseline.json").read_text())
        controleer(
            "en legt de nieuwe methode vast",
            opnieuw.get("_methodology") == "needle-unique-v2",
            f"kreeg {opnieuw.get('_methodology')!r}",
        )


def main() -> int:
    if not GATE.exists():
        print(f"SKIPPED (not a pass): {GATE} bestaat niet", file=sys.stderr)
        return 1
    print(f"text-replace-gate tests ({GATE.name})")
    for naam, fn in sorted(globals().items()):
        if naam.startswith("test_") and callable(fn):
            fn()
    if mislukt:
        print(f"\n{len(mislukt)} test(s) gefaald: {', '.join(mislukt)}")
        return 1
    print("\nalle tests geslaagd")
    return 0


if __name__ == "__main__":
    sys.exit(main())
