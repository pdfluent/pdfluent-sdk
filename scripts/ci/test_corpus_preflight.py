#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for the corpus preflight guard.

Run: python3 scripts/ci/test_corpus_preflight.py

The test that matters here is `test_hangende_schijf_faalt_snel`. Every other case
would also pass against the check this script replaced -- a bare `test -d` -- and
that check is exactly what let a dead disk go unnoticed for five hours on
21-08-2026. A directory whose entries are readable while its contents block
forever is the failure this guard exists for, so a FIFO stands in for it: opening
it for reading blocks until someone writes, which is what an unresponsive disk
looks like from userspace.

If someone ever simplifies the guard back to a stat, that one test goes red and
the rest stay green. That is the whole point of it.

No real corpus and no mounts: temporary directories throughout, seconds to run.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
GUARD = HERE / "corpus_preflight.sh"

mislukt: list[str] = []


def draai(*args: str, deadline: str = "2") -> tuple[int, str, float]:
    """Roept de guard aan en geeft (exitcode, uitvoer, verstreken seconden)."""
    begin = time.monotonic()
    p = subprocess.run(
        ["bash", str(GUARD), "--deadline", deadline, *args],
        capture_output=True,
        text=True,
        timeout=120,
    )
    return p.returncode, p.stdout + p.stderr, time.monotonic() - begin


def controleer(naam: str, voorwaarde: bool, toelichting: str = "") -> None:
    if voorwaarde:
        print(f"  ok    {naam}")
    else:
        print(f"  FOUT  {naam}  {toelichting}")
        mislukt.append(naam)


def maak_corpus(map_: Path, inhoud: bytes = b"%PDF-1.4\n%fake\n") -> Path:
    doc = map_ / "000001.pdf"
    doc.write_bytes(inhoud)
    return doc


def test_gezond_corpus_slaagt() -> None:
    with tempfile.TemporaryDirectory() as d:
        maak_corpus(Path(d))
        code, uit, _ = draai(d)
        controleer("gezond corpus slaagt", code == 0, f"code={code} {uit.strip()}")
        controleer(
            "gezond corpus noemt het monster",
            "000001.pdf" in uit,
            uit.strip(),
        )


def test_ontbrekende_map_faalt() -> None:
    code, uit, _ = draai("/bestaat/echt/niet")
    controleer("ontbrekende map -> 2", code == 2, f"code={code} {uit.strip()}")


def test_lege_map_faalt() -> None:
    with tempfile.TemporaryDirectory() as d:
        code, uit, _ = draai(d)
        controleer("lege map -> 3", code == 3, f"code={code} {uit.strip()}")


def test_map_zonder_pdf_faalt() -> None:
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / "notities.txt").write_text("geen pdf")
        code, uit, _ = draai(d)
        controleer("map zonder pdf -> 3", code == 3, f"code={code} {uit.strip()}")


def test_leeg_bestand_faalt() -> None:
    with tempfile.TemporaryDirectory() as d:
        maak_corpus(Path(d), b"")
        code, uit, _ = draai(d)
        controleer("pdf zonder bytes -> 4", code == 4, f"code={code} {uit.strip()}")


def test_verkeerde_magic_faalt() -> None:
    with tempfile.TemporaryDirectory() as d:
        maak_corpus(Path(d), b"<html>dit is geen pdf</html>")
        code, uit, _ = draai(d)
        controleer("verkeerde magic -> 4", code == 4, f"code={code} {uit.strip()}")


def test_hangende_schijf_faalt_snel() -> None:
    """De reden dat dit bestand bestaat.

    De map is er, de bestandsnaam is er, `test -d` en `find` slagen allebei --
    en toch komt er geen byte uit. Precies wat een dode schijf laat zien.
    """
    with tempfile.TemporaryDirectory() as d:
        fifo = Path(d) / "000001.pdf"
        os.mkfifo(fifo)
        try:
            code, uit, duur = draai(d, deadline="2")
            controleer(
                "hangende schijf -> 5",
                code == 5,
                f"code={code} {uit.strip()}",
            )
            controleer(
                "hangende schijf faalt binnen 15s (niet na de jobtimeout)",
                duur < 15,
                f"duurde {duur:.1f}s",
            )
            controleer(
                "hangende schijf noemt het herstelpad",
                "PDFluent-WSL-CorpusDisk" in uit,
                uit.strip(),
            )
        finally:
            # Het achtergebleven leesproces losmaken, anders blijft de tijdelijke
            # map bezet: even openen om te schrijven geeft de lezer een EOF.
            try:
                fd = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
                os.close(fd)
            except OSError:
                pass


def test_stat_alleen_zou_dit_missen() -> None:
    """Toont expliciet dat de oude controle hier groen op zou staan.

    Zonder deze vergelijking is `test_hangende_schijf_faalt_snel` alleen maar een
    exitcode; met deze staat er zwart op wit dat het verschil in de controle zit
    en niet in de opstelling.
    """
    with tempfile.TemporaryDirectory() as d:
        fifo = Path(d) / "000001.pdf"
        os.mkfifo(fifo)
        try:
            oud = subprocess.run(["test", "-d", f"{d}"], capture_output=True)
            controleer(
                "de oude `test -d` slaagt op precies deze opstelling",
                oud.returncode == 0,
                f"code={oud.returncode}",
            )
        finally:
            fifo.unlink(missing_ok=True)


def _onder_koppelpunt(pad: Path) -> bool:
    """Ligt `pad` onder een koppelpunt anders dan de root?"""
    p = pad.resolve()
    while str(p) != "/":
        if subprocess.run(["mountpoint", "-q", str(p)]).returncode == 0:
            return True
        p = p.parent
    return False


def _map_op_de_rootschijf() -> Path | None:
    """Een schrijfbare map die onder geen enkel koppelpunt ligt.

    Nodig omdat dit niet overal hetzelfde ligt: op de CI-runner is `/tmp` een
    eigen tmpfs, en dan zou `tempfile` een map opleveren die wél onder een
    koppelpunt valt. Dat de test daar tot nu toe slaagde kwam doordat TMPDIR er
    ergens anders heen wees -- een uitkomst die van de omgeving afhing en niet
    van de code. Verandert TMPDIR, dan klapt de test om zonder dat er iets mis
    is met wat hij bewaakt.
    """
    for kandidaat in (Path("/var/tmp"), Path.cwd(), Path.home()):
        try:
            if kandidaat.is_dir() and os.access(kandidaat, os.W_OK) and not _onder_koppelpunt(kandidaat):
                return kandidaat
        except OSError:
            continue
    return None


def test_require_mount_weigert_rootschijf() -> None:
    basis = _map_op_de_rootschijf()
    if basis is None:
        print("  SKIPPED (not a pass): geen schrijfbare map gevonden die buiten "
              "elk koppelpunt ligt; --require-mount is hier niet te toetsen",
              file=sys.stderr)
        return
    with tempfile.TemporaryDirectory(dir=basis) as d:
        maak_corpus(Path(d))
        code, uit, _ = draai("--require-mount", d)
        controleer(
            "--require-mount op de rootschijf -> 6",
            code == 6,
            f"code={code} in {basis} — {uit.strip()}",
        )


def test_zonder_argument_weigert() -> None:
    p = subprocess.run(["bash", str(GUARD)], capture_output=True, text=True)
    controleer("zonder corpusmap -> 64", p.returncode == 64, f"code={p.returncode}")


def main() -> int:
    if not GUARD.exists():
        print(f"SKIPPED (not a pass): {GUARD} bestaat niet", file=sys.stderr)
        return 1
    if sys.platform == "darwin":
        # mountpoint(1) bestaat niet op macOS; de rest draait wel.
        print("let op: --require-mount wordt op macOS overgeslagen", file=sys.stderr)

    print(f"corpus-preflight tests ({GUARD.name})")
    for naam, fn in sorted(globals().items()):
        if naam.startswith("test_") and callable(fn):
            if naam == "test_require_mount_weigert_rootschijf" and sys.platform == "darwin":
                print(f"  SKIPPED (not a pass): {naam} — mountpoint(1) ontbreekt op macOS",
                      file=sys.stderr)
                continue
            fn()

    if mislukt:
        print(f"\n{len(mislukt)} test(s) gefaald: {', '.join(mislukt)}")
        return 1
    print("\nalle tests geslaagd")
    return 0


if __name__ == "__main__":
    sys.exit(main())
