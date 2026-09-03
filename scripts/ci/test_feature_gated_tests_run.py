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
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
CI = REPO / ".gitlab-ci.yml"

# GitHub is the pipeline; GitLab is a nightly copy that was 204 commits behind on
# 03-09-2026 and blocks nothing. Reading only .gitlab-ci.yml meant the one way to
# satisfy this guard was a job on that mirror -- so "every feature-gated test has
# a job" could be true while no such job had ever run against a merge. The
# workflows are read too, and they are where the claim becomes checkable the same
# day. The mirror still counts; it is simply no longer the only thing that does.
WORKFLOWS = REPO / ".github" / "workflows"

# ONDERGRENS: gescande crates >= 30 — de workspace heeft er ruim veertig; vindt
# deze controle er minder, dan is de boomwandeling stuk en niet de codebase leeg.
MIN_CRATES = 30

# Paren die met opzet niet in CI draaien staan in scripts/ci/feature_gaps.toml,
# niet hier. Die tabel lag als dict in dit bestand, en dit bestand is t2-gebied:
# een gat verklaren betekende dus een t2-bestand aanraken. Op 01-09-2026 haalde
# T1 daarom een feature-vlag wég in plaats van hem te verklaren. De bewaker had
# gelijk en zijn enige uitweg zat achter andermans deur.
#
# Het TOML-bestand is in .claude/territories.toml uitgezonderd van t2, dus
# niemand claimt het en iedereen mag het bewerken.
GAPS = REPO / "scripts" / "ci" / "feature_gaps.toml"


def toegestaan() -> dict[tuple[str, str], str]:
    """De verklaarde gaten, of stoppen.

    Een ontbrekend of kapot bestand levert géén lege verzameling op. Leeg
    betekent "geen enkel gat is verklaard", en dan keurt deze controle elk gat
    af dat wél verklaard was -- of erger, bij een andere lezing keurt hij alles
    goed. Beide zijn een antwoord dat niemand heeft opgeschreven.
    """
    if not GAPS.is_file():
        print(f"[featgate] FATAL: {GAPS} ontbreekt. Zonder die tabel is niet te "
              "zeggen welk gat verklaard is en welk niet.", file=sys.stderr)
        raise SystemExit(2)
    try:
        doc = tomllib.loads(GAPS.read_text())
    except tomllib.TOMLDecodeError as fout:
        print(f"[featgate] FATAL: {GAPS} parseert niet: {fout}", file=sys.stderr)
        raise SystemExit(2) from None
    uit: dict[tuple[str, str], str] = {}
    for rij in doc.get("gap", []):
        pkt, feat, waarom = rij.get("pakket"), rij.get("feature"), rij.get("waarom")
        if not (pkt and feat and waarom):
            print(f"[featgate] FATAL: een regel in {GAPS.name} mist pakket, feature "
                  f"of waarom: {rij}", file=sys.stderr)
            raise SystemExit(2)
        uit[(pkt, feat)] = waarom
    return uit


TOEGESTAAN = toegestaan()

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
    bronnen = [CI.read_text(errors="replace")]
    if WORKFLOWS.is_dir():
        bronnen += [w.read_text(errors="replace")
                    for w in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))]
    gedekt, all_features = set(), False
    for tekst in bronnen:
        paren, alles = ci_dekking(tekst)
        gedekt |= paren
        all_features = all_features or alles

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
