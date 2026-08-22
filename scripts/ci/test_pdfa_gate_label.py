#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Het conformiteitscijfer moet zeggen uit welk monster het komt.

`measured.json` van de holdoutrun van 22-08 schreef `govdocs (fixed
300-document sample)` terwijl `sample_size` 1000 was. Die 300 is het monster
waarop vijf ronden reparatiewerk zijn geoptimaliseerd; een holdoutcijfer dat
zichzelf zo etiketteert vertelt het tegenovergestelde van wat het waard is.
"""
import importlib.util
import pathlib
import sys

HIER = pathlib.Path(__file__).resolve().parent
GATE = HIER / "pdfa_conformance_gate.py"
mislukt: list[str] = []


def controleer(naam: str, voorwaarde: bool, toelichting: str = "") -> None:
    if voorwaarde:
        print(f"  ok    {naam}")
    else:
        mislukt.append(naam)
        print(f"  FAIL  {naam}" + (f" — {toelichting}" if toelichting else ""))


def laad():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("pdfa_gate", GATE)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    importlib.invalidate_caches()
    return mod


def main() -> int:
    if not GATE.exists():
        print(f"SKIPPED (not a pass): {GATE} bestaat niet", file=sys.stderr)
        return 1
    mod = laad()
    print(f"pdfa-gate label tests ({GATE.name})")

    holdout = mod.corpus_label(pathlib.Path("benchmarks/pdfa/govdocs_holdout_1000.txt"), 1000)
    controleer(
        "de holdout noemt zijn eigen lijst",
        "govdocs_holdout_1000.txt" in holdout,
        holdout,
    )
    controleer(
        "en niet het monster waarop is geoptimaliseerd",
        "300" not in holdout,
        holdout,
    )
    controleer("het aantal staat erbij", "1000 documents" in holdout, holdout)

    monster = mod.corpus_label(pathlib.Path("benchmarks/pdfa/govdocs_sample_300.txt"), 300)
    controleer(
        "en het 300-monster noemt zichzelf ook goed",
        "govdocs_sample_300.txt" in monster and "300 documents" in monster,
        monster,
    )
    controleer(
        "de twee zijn uit elkaar te houden",
        holdout != monster,
        f"{holdout!r} == {monster!r}",
    )

    if mislukt:
        print(f"\n{len(mislukt)} test(s) gefaald: {', '.join(mislukt)}")
        return 1
    print("\nalle tests geslaagd")
    return 0


if __name__ == "__main__":
    sys.exit(main())
