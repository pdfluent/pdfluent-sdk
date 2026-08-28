#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests voor de delegatie-herkenning in test_reachability.py.

Die herkenning bepaalt of een onbereikte publieke functie een uitleg krijgt.
Wordt hij te ruim, dan poetst hij echte gaten weg -- en dat is precies het
tegenovergestelde van wat het register moet doen. Daarom staat elk geval dat
GEEN delegatie mag zijn hier net zo goed in als de gevallen die het wel zijn.
"""
import importlib.util
import pathlib
import sys

HIER = pathlib.Path(__file__).resolve().parent


def laad():
    # Geen bytecodecache: een mutatietest van gelijke grootte in dezelfde
    # seconde draait anders de oude .pyc (zie mutation-test-bytecode-trap).
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("tr", HIER / "test_reachability.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    importlib.invalidate_caches()
    return mod


GEVALLEN_WEL = [
    (
        "enkele aanroep",
        "pub fn check_a(pdf: &Pdf, report: &mut R) {\n    check_a_cached(pdf, report);\n}",
        "check_a_cached",
    ),
    (
        "constructie plus aanroep",
        "pub fn check_b(pdf: &Pdf, report: &mut R) {\n"
        "    let cache = ObjectCache::new(pdf);\n"
        "    check_b_cached(&cache, report);\n}",
        "check_b_cached",
    ),
    (
        "commentaar telt niet mee",
        "pub fn check_c(pdf: &Pdf, report: &mut R) {\n"
        "    // waarom dan ook\n    check_c_cached(pdf, report);\n}",
        "check_c_cached",
    ),
]

GEVALLEN_NIET = [
    (
        "een lus is gedrag",
        "pub fn check_d(pdf: &Pdf, report: &mut R) {\n"
        "    for o in pdf.iter() {\n        check(o);\n    }\n}",
    ),
    (
        "een voorwaarde is gedrag",
        "pub fn check_e(pdf: &Pdf, report: &mut R) {\n"
        "    if pdf.is_empty() { return; }\n    check_e_cached(pdf, report);\n}",
    ),
    (
        "twee aanroepen is samenstelling, geen doorgeefluik",
        "pub fn check_f(pdf: &Pdf, report: &mut R) {\n"
        "    check_x(pdf, report);\n    check_y(pdf, report);\n}",
    ),
    (
        "eigen berekening voor het doorgeven",
        "pub fn check_g(pdf: &Pdf, report: &mut R) {\n"
        "    let n = pdf.len() * 2 + 1;\n    check_g_cached(n, report);\n}",
    ),
]


def bewaking(tr) -> list[str]:
    """Een delegatie verklaart alleen iets als het doel zelf getest wordt."""
    lichamen = {
        "omhulsel": ["pub fn omhulsel(p: &P) {\n    kern(p);\n}"],
        "kern": ["fn kern(p: &P) {\n    doe_iets(p);\n}"],
    }
    fout = []
    if tr.soort_van("omhulsel", lichamen, {"kern"}, {}) != "delegatie":
        fout.append("FAIL: doorgeven aan een getest doel hoort 'delegatie' te heten")
    if tr.soort_van("omhulsel", lichamen, set(), {}) is not None:
        fout.append(
            "FAIL: een omhulsel rond een ONgeteste functie mag zichzelf niet "
            "verklaren — dan verbergt het register precies het gat dat het moet tonen"
        )
    return fout


def main() -> int:
    tr = laad()
    fouten = 0
    for regel in bewaking(tr):
        print(regel)
        fouten += 1

    for naam, lichaam, verwacht in GEVALLEN_WEL:
        gekregen = tr.delegeert_naar(lichaam)
        if gekregen != verwacht:
            print(f"FAIL ({naam}): verwacht {verwacht!r}, kreeg {gekregen!r}")
            fouten += 1

    for naam, lichaam in GEVALLEN_NIET:
        gekregen = tr.delegeert_naar(lichaam)
        if gekregen is not None:
            print(f"FAIL ({naam}): had geen delegatie mogen zijn, kreeg {gekregen!r}")
            fouten += 1

    if fouten:
        print(f"[test_reachability_unit] {fouten} fout(en)")
        return 1
    print(f"[test_reachability_unit] {len(GEVALLEN_WEL) + len(GEVALLEN_NIET)} gevallen ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
