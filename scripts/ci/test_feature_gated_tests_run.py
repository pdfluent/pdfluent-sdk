#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
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

try:
    import yaml
except ModuleNotFoundError:
    print("[featgate] FATAL: pyyaml is not installed, so which lines a job runs "
          "cannot be established. That is not a pass.", file=__import__('sys').stderr)
    raise SystemExit(2) from None
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]

# Coverage is a job a change cannot get past. Two things used to count that are
# not that, and both are gone (#276).
#
# `.gitlab-ci.yml` was the first, and for a while the only, source. It is a
# nightly copy that was 204 commits behind GitHub on 03-09-2026 and blocks
# nothing -- so "every feature-gated test has a job" could be true while no such
# job had ever run against a merge. An earlier pass added the workflows beside
# it and left the mirror counting; this removes it, which is what the mirror
# being a mirror has meant all along.
#
# The second is subtler and is why this sits on #276: a workflow that only runs
# when somebody types the dispatch is not a gate either. The four corpus
# workflows are exactly that shape -- manual by decision, queued behind a job
# that exits non-zero -- and a `cargo test --features x` line inside one of them
# would have satisfied this guard while never running at all. That is the same
# claim as the mirror's, made by a file in the right repository.
#
# So the sources are the workflows a change actually passes through.
WORKFLOWS = REPO / ".github" / "workflows"

# The events a change cannot avoid. `schedule` is deliberately absent: a nightly
# runs after the fact, on master, and tells a pull request nothing. `workflow_
# dispatch` is absent for the reason above.
BLOKKERENDE_GEBEURTENISSEN = {"push", "pull_request", "pull_request_target",
                              "merge_group"}

# ONDERGRENS: workflows read >= 10. This repository has thirty-odd; finding a
# handful means the glob broke, and a coverage set assembled from a broken glob
# reports gaps that are not there -- or, once the gaps are declared, no gaps at
# all.
MIN_WORKFLOWS = 10

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


def standaardfeatures(cargo_toml: pathlib.Path) -> set[str]:
    """The crate's own features that a plain `cargo test` turns on.

    A test behind a DEFAULT feature compiles in any ordinary run, so counting it
    as uncovered is a gap that does not exist -- and a guard that reports gaps
    that do not exist is one somebody switches off. `pdf-manip`'s `pdfa-convert`
    and `serde` are both defaults; the second was even carrying a declared
    excuse in feature_gaps.toml for tests that have been running all along.

    The closure, not the literal list: `default = ["a"]` with `a = ["b"]` turns
    on `b` as well. `dep:x` and `other-crate/x` are not this crate's features and
    are skipped -- a feature this guard cannot resolve is one it must not claim.
    """
    try:
        doc = tomllib.loads(cargo_toml.read_text(errors="replace"))
    except (tomllib.TOMLDecodeError, OSError):
        return set()
    tabel = doc.get("features")
    if not isinstance(tabel, dict):
        return set()
    uit: set[str] = set()
    werk = [f for f in tabel.get("default", []) if isinstance(f, str)]
    while werk:
        naam = werk.pop()
        if "/" in naam or naam.startswith("dep:") or naam in uit:
            continue
        uit.add(naam)
        werk += [f for f in tabel.get(naam, []) if isinstance(f, str)]
    return uit


def gated_features_met_tests(tekst: str) -> set[str]:
    """Features die minstens één `#[test]` afschermen.

    Twee vormen tellen: het attribuut vlak boven de test zelf, en het attribuut
    boven een `mod` waar tests in zitten. Meer vormen zijn er in deze codebase
    niet, en een derde vorm die we missen komt vanzelf boven als een test die
    hoort te draaien nergens in de telling opduikt.
    """
    regels = tekst.splitlines()
    gevonden: set[str] = set()

    # Vorm 0: `#![cfg(feature = "x")]` at the top of the file, which gates every
    # test in it.
    #
    # CFG_FEATURE was widened to match an inner attribute, but both walks below
    # only read attribute lines TOUCHING a `#[test]` -- and a crate-level gate
    # sits at the top of the file, thirty lines above the first test. So the
    # pattern matched something nothing ever showed it.
    #
    # Measured 05-09-2026: `crates/pdf-ocr/tests/model_fetch_is_opt_in.rs` gates
    # its whole file this way, and this function returned an empty set for it.
    # Its four tests prove that constructing an OCR engine never fetches 84 MB of
    # weights over the network unasked -- and the guard whose job is to notice a
    # test nobody runs could not see them. An inner attribute is only legal
    # before the first item, so scanning the file for one is the whole of it.
    # (#276)
    if TEST_ATTRIBUUT.search(tekst):
        for regel in regels:
            kaal = regel.lstrip()
            if not kaal.startswith("#!["):
                continue
            m = CFG_FEATURE.search(regel)
            if m and not CFG_NOT_FEATURE.search(regel):
                gevonden.add(m.group(1))

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


# The keys whose values a pipeline actually executes. Everything else in a
# workflow file is description: names, comments, `if` expressions, the prose in
# between.
UITVOERSLEUTELS = {"run", "script", "before_script", "after_script"}


def uitvoerregels(tekst: str) -> list[str] | None:
    """What this file RUNS, or None if it is not YAML.

    `ci_dekking` used to scan every line of the raw text, so anything containing
    `cargo test` and `--features x` counted -- including a comment and including
    a job's `name`. Measured 03-09-2026: replacing the real step with `run: true`
    and a `# was: cargo test -p pdf-ocr --features tesseract` line left the guard
    green, so a job could be deleted and its epitaph kept the gate satisfied.
    That is the defect this guard exists to name, in the guard itself. (T3, #1685)

    Returning None rather than falling back to the raw text is the point: a file
    that will not parse is a file whose jobs cannot be established, and guessing
    from its bytes is how the hole got here.
    """
    try:
        doc = yaml.safe_load(tekst)
    except yaml.YAMLError:
        return None
    uit: list[str] = []

    def loop(knoop, sleutel=None):
        if isinstance(knoop, dict):
            for k, v in knoop.items():
                loop(v, k)
        elif isinstance(knoop, list):
            for v in knoop:
                loop(v, sleutel)
        elif isinstance(knoop, str) and sleutel in UITVOERSLEUTELS:
            uit.append(knoop)

    loop(doc)
    return uit


def standaardbouw(tekst: str) -> tuple[set[str], bool]:
    """Which packages a job compiles with their default features on.

    Measured, not assumed. `cargo test --workspace` in a blocking workflow is
    what makes a default feature covered; if that job ever goes away, the
    defaults stop being covered and this returns the smaller set rather than
    keeping an answer that was true last month.
    """
    pakketten: set[str] = set()
    hele_workspace = False
    for regel in tekst.splitlines():
        if "cargo test" not in regel and "cargo nextest" not in regel:
            continue
        if "--no-default-features" in regel:
            continue
        if "--workspace" in regel or "--all" in regel:
            hele_workspace = True
        pakketten.update(re.findall(r"(?:-p|--package)[= ]([A-Za-z0-9_-]+)", regel))
    return pakketten, hele_workspace


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


def triggers(doc) -> set[str]:
    """The events this workflow answers to.

    PyYAML reads a bare `on:` key as the boolean True, which is why every reader
    in this repository asks for both.
    """
    on = doc.get(True) or doc.get("on") or {}
    if isinstance(on, dict):
        return set(on)
    if isinstance(on, (list, tuple, set)):
        return {str(x) for x in on}
    return {str(on)}


def blokkeert_een_wijziging(doc) -> bool:
    """Does a change have to pass this workflow?

    A dispatch-only workflow runs when somebody types it and not otherwise, so a
    test named in one is a test that has never been run against the change in
    front of it. That is the mirror's defect with a different address (#276).
    """
    return bool(triggers(doc) & BLOKKERENDE_GEBEURTENISSEN)


def main() -> int:
    if not WORKFLOWS.is_dir():
        print(f"[featgate] FATAL: {WORKFLOWS} ontbreekt, so no job can be read at "
              "all. That is not a pass.", file=sys.stderr)
        return 2
    alle = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if len(alle) < MIN_WORKFLOWS:
        print(f"ONDERGRENS: {len(alle)} workflows gevonden, verwacht >= "
              f"{MIN_WORKFLOWS}. De glob is stuk -- dit is geen groen.",
              file=sys.stderr)
        return 1
    gedekt, all_features = set(), False
    standaard_pakketten: set[str] = set()
    hele_workspace = False
    overgeslagen: list[str] = []
    for pad in alle:
        tekst = pad.read_text(errors="replace")
        try:
            doc = yaml.safe_load(tekst)
        except yaml.YAMLError:
            doc = None
        if not isinstance(doc, dict):
            print(f"[featgate] FATAL: {pad} is not valid YAML, so the commands it "
                  "runs cannot be read. Refusing to judge coverage from its raw "
                  "bytes.", file=sys.stderr)
            return 2
        if not blokkeert_een_wijziging(doc):
            overgeslagen.append(pad.name)
            continue
        regels = uitvoerregels(tekst)
        if regels is None:
            print(f"[featgate] FATAL: {pad} is not valid YAML, so the commands it "
                  "runs cannot be read. Refusing to judge coverage from its raw "
                  "bytes.", file=sys.stderr)
            return 2
        samen = "\n".join(regels)
        paren, alles = ci_dekking(samen)
        gedekt |= paren
        all_features = all_features or alles
        pkt, hele = standaardbouw(samen)
        standaard_pakketten |= pkt
        hele_workspace = hele_workspace or hele

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
        standaard = standaardfeatures(cargo)
        gebouwd = hele_workspace or pkg in standaard_pakketten
        for f in sorted(features):
            # On by default and something builds this package plainly, so these
            # tests compile in that job whether or not any line names the flag.
            # Established BEFORE the declared gaps are consulted, so that a row
            # excusing a default feature shows up as the stale excuse it is
            # rather than being quietly honoured.
            if gebouwd and f in standaard:
                gedekt.add((pkg, f))
                continue
            if (pkg, f) in TOEGESTAAN:
                continue
            if (pkg, f) in gedekt:
                continue
            gaten.append(
                f"  {pkg} --features {f}: tests achter deze vlag, geen job die hem aanzet"
            )

    # The other direction, which feature_gaps.toml has claimed in writing since
    # it was created and nothing enforced: a declared gap that starts running is
    # a stale excuse, and a table of stale excuses is where the next real gap
    # hides. Measured 05-09-2026: `pdf-manip --features serde` was listed as "279
    # tests pass locally, no job runs them" while `serde` is one of that crate's
    # DEFAULT features -- every workspace run had been compiling them.
    verlopen = []
    for (pkt, feat), waarom in sorted(TOEGESTAAN.items()):
        if (pkt, feat) in gedekt:
            verlopen.append(f"  {pkt} --features {feat}: a job runs this now, "
                            f"but the table still says {waarom!r}")
    if verlopen:
        print("Declared gaps that are no longer gaps. Remove them from "
              f"{GAPS.name}; a list of excuses that never shrinks stops being "
              "read.\n", file=sys.stderr)
        print("\n".join(verlopen), file=sys.stderr)
        return 1

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

    print(f"OK: {len(crates)} crates gescand tegen "
          f"{len(alle) - len(overgeslagen)} blokkerende workflow(s); elke "
          f"feature-gated test heeft een job. ({len(overgeslagen)} workflow(s) "
          "run only on request and count for nothing.)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
