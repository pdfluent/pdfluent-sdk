#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Een test achter een cargo-feature moet ergens in CI draaien.

`cargo test -p x` compileert alleen wat met de standaardfeatures aanstaat. Een
`#[cfg(feature = "tsa")] #[test]` bestaat dan letterlijk niet: geen fout, geen
overslag, geen regel in de uitvoer. Dat is de stilste vorm van de fout waar dit
project al vier keer op is gestruikeld -- de test bestond, en niets riep hem aan.

Op 25-08-2026 vonden we zo `extract_cms_from_signed_roundtrip` in
`crates/pdf-sign`. Hij stond er, hij was rood, en hij was rood omdat
`extract_cms_from_signed` zocht op `"/Contents <"` terwijl lopdf `/Contents<`
schrijft (`Writer::need_separator` geeft geen spatie voor een hexstring). De hele
PAdES-B-LT-route viel daarop om. Geen enkele pipelinejob draaide
`-p pdfluent-sign --features tsa`, dus niemand zag het.

Deze controle vergelijkt twee lijsten: welke (pakket, feature)-paren gated tests
dragen, en welke paren een pipelinejob daadwerkelijk aanzet.
"""
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / ".gitlab-ci.yml"

# ONDERGRENS: gescande crates >= 30 — de workspace heeft er ruim veertig; vindt
# deze controle er minder, dan is de boomwandeling stuk en niet de codebase leeg.
MIN_CRATES = 30

# Paren die met opzet niet in CI draaien, mét reden. Een naam hier zonder reden
# hoort niet te bestaan; een lege lijst is het doel.
TOEGESTAAN: dict[tuple[str, str], str] = {
    # The thirteen gaps as measured on master, 30-08-2026. Recorded rather than
    # left failing, because closing them means adding CI jobs -- and for `tsa`,
    # fixing a defect first: extract_cms_from_signed cannot read back a document
    # sign_pdf just produced (#285).
    #
    # Named individually so the list cannot grow quietly, and checked in both
    # directions: a pair that starts running and stays listed here fails too.
    ("pdfluent-lopdf", "chrono"): "#285 -- no job enables it",
    ("pdfluent-lopdf", "embed_image"): "#285 -- no job enables it",
    ("pdfluent-lopdf", "jiff"): "#285 -- no job enables it",
    ("pdfluent-lopdf", "time"): "#285 -- no job enables it",
    ("pdf-engine", "ocr-aws"): "#285 -- cloud OCR, no job enables it",
    ("pdf-engine", "ocr-azure"): "#285 -- cloud OCR, no job enables it",
    ("pdf-engine", "ocr-google"): "#285 -- cloud OCR, no job enables it",
    ("pdf-engine", "ocr-mistral"): "#285 -- cloud OCR, no job enables it",
    ("pdf-manip", "serde"): "#285 -- 279 tests pass locally, no job runs them",
    ("pdfluent-sign", "tsa"): "#285 -- one test FAILS here; fix before enabling",
    ("pdf-xfa", "xfa-js-sandboxed"): "#285 -- no job enables it",
    ("pdfluent", "pdfa"): "#285 -- 24 tests pass locally, no job runs them",
    ("xfa-license", "signing"): "#285 -- 26 tests pass locally, no job runs them",
    ("pdf-ocr", "tesseract"): "#244 -- libtesseract is not on the runner",
    # Found only after the parsing was widened to see crate-level gates,
    # compound cfgs and non-plain test attributes (Codex, #1583). The guard had
    # been reporting a subset and calling it the total -- which is the shape it
    # exists to catch, in itself.
    ("pdfluent-lopdf", "async"): "#285 -- no job enables it",
    ("pdf-annot", "write"): "#285 -- 38 tests pass locally, no job runs them",
    ("pdf-font", "embed-cmaps"): "#285 -- 117 tests pass locally, no job runs them",
}

# Matches an inner attribute too (`#![cfg(...)]`), which is how an integration
# test gates its whole file -- and the previous pattern required `#[`, so a
# crate-level gate was invisible. Also matches a feature named anywhere inside
# the cfg, so `cfg(all(test, feature = "x"))` counts; requiring `feature` to
# come first missed every compound condition (Codex, #1583).
CFG_FEATURE = re.compile(r'#!?\[cfg\([^)]*?feature\s*=\s*"([^"]+)"')
TEST_ATTRIBUUT = re.compile(r"#\[(?:[\w:]+::)?test\b")
CFG_NOT_FEATURE = re.compile(r'#\[cfg\(\s*not\(\s*feature\s*=\s*"([^"]+)"')


def pakketnaam(cargo_toml: pathlib.Path) -> str | None:
    for regel in cargo_toml.read_text(errors="replace").splitlines():
        m = re.match(r'\s*name\s*=\s*"([^"]+)"', regel)
        if m:
            return m.group(1)
        if regel.strip().startswith("[") and not regel.strip().startswith("[package"):
            continue
    return None


def gated_features_met_tests(tekst: str) -> set[str]:
    """Features die minstens één `#[test]` afschermen.

    Twee vormen tellen: het attribuut vlak boven de test zelf, en het attribuut
    boven een `mod` waar tests in zitten. Meer vormen zijn er in deze codebase
    niet, en een derde vorm die we missen komt vanzelf boven als een test die
    hoort te draaien nergens in de telling opduikt.
    """
    regels = tekst.splitlines()
    gevonden: set[str] = set()

    # Vorm 1: attribuutblok direct boven `#[test]`.
    for i, regel in enumerate(regels):
        # `#[tokio::test]`, `#[async_std::test]` and friends are tests too, and
        # a gate above one of those was skipped entirely.
        if not TEST_ATTRIBUUT.match(regel.strip()):
            continue
        j = i - 1
        while j >= 0 and regels[j].lstrip().startswith("#["):
            if CFG_FEATURE.search(regels[j]) and not CFG_NOT_FEATURE.search(regels[j]):
                gevonden.add(CFG_FEATURE.search(regels[j]).group(1))
            j -= 1
        # En het attribuut kán er ook ónder staan (`#[test]` eerst).
        j = i + 1
        while j < len(regels) and regels[j].lstrip().startswith("#["):
            if CFG_FEATURE.search(regels[j]) and not CFG_NOT_FEATURE.search(regels[j]):
                gevonden.add(CFG_FEATURE.search(regels[j]).group(1))
            j += 1

    # Vorm 2: `#[cfg(feature = "x")] mod tests {` — geldt voor alles erbinnen.
    for i, regel in enumerate(regels):
        if not re.match(r"\s*(pub\s+)?mod\s+\w+\s*\{", regel):
            continue
        j = i - 1
        feature = None
        while j >= 0 and regels[j].lstrip().startswith("#["):
            m = CFG_FEATURE.search(regels[j])
            if m and not CFG_NOT_FEATURE.search(regels[j]):
                feature = m.group(1)
            j -= 1
        if not feature:
            continue
        # Bevat het blok een test? Tel accolades tot de sluiter.
        diepte = 0
        for k in range(i, len(regels)):
            diepte += regels[k].count("{") - regels[k].count("}")
            if regels[k].strip() == "#[test]":
                gevonden.add(feature)
                break
            if diepte <= 0 and k > i:
                break

    return gevonden


def ci_dekking(tekst: str) -> tuple[set[tuple[str, str]], bool]:
    """(pakket, feature)-paren die een job aanzet, en of iets --all-features draait."""
    paren: set[tuple[str, str]] = set()
    alles = False
    for regel in tekst.splitlines():
        if "cargo test" not in regel and "cargo nextest" not in regel:
            continue
        if "--all-features" in regel:
            alles = True
        pakketten = re.findall(r"-p\s+([A-Za-z0-9_-]+)", regel)
        features: list[str] = []
        for m in re.finditer(r"--features[= ]([A-Za-z0-9_,\-]+)", regel):
            features.extend(f for f in m.group(1).split(",") if f)
        for p in pakketten:
            for f in features:
                paren.add((p, f))
    return paren, alles


def main() -> int:
    if not CI.exists():
        print(f"SKIPPED (not a pass): {CI} ontbreekt", file=sys.stderr)
        return 0
    ci_tekst = CI.read_text(errors="replace")
    gedekt, all_features = ci_dekking(ci_tekst)

    crates = sorted((REPO / "crates").glob("*/Cargo.toml"))
    if len(crates) < MIN_CRATES:
        print(
            f"ONDERGRENS: {len(crates)} crates gevonden, verwacht >= {MIN_CRATES}. "
            "De boomwandeling is stuk -- dit is geen groen.",
            file=sys.stderr,
        )
        return 1

    gaten: list[str] = []
    for cargo in crates:
        pkg = pakketnaam(cargo)
        if not pkg:
            continue
        features: set[str] = set()
        for bron in list((cargo.parent / "src").rglob("*.rs")) + list(
            (cargo.parent / "tests").rglob("*.rs")
        ):
            features |= gated_features_met_tests(bron.read_text(errors="replace"))
        for f in sorted(features):
            if (pkg, f) in TOEGESTAAN:
                continue
            if (pkg, f) in gedekt:
                continue
            gaten.append(
                f"  {pkg} --features {f}: tests achter deze vlag, geen job die hem aanzet"
            )

    if gaten:
        print(
            "Feature-gated tests die in geen enkele pipelinejob draaien.\n"
            "Een test die niet compileert is niet te onderscheiden van een test die slaagt:\n",
            file=sys.stderr,
        )
        print("\n".join(gaten), file=sys.stderr)
        if all_features:
            print(
                "\n(Er draait wel iets met --all-features, maar niet voor deze pakketten.)",
                file=sys.stderr,
            )
        return 1

    print(f"OK: {len(crates)} crates gescand, elke feature-gated test heeft een job.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
