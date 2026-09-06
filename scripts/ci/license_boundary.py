#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
"""The licence boundary holds: each package declares what its side says it does.

#220 asks for the boundary as data plus "een lint die faalt als een crate aan de
eigen kant van de grens een permissieve licentie declareert, of andersom".

FOUR SIDES

  ours      dual-licensed AGPLv3 + commercial. Declares
            `license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"` --
            the compound is canonical on all four channels, so one constant can
            be compared against four registers. LC9 replaced the older
            `license-file` form, which crates.io renders instead of an SPDX
            expression, and the check below now refuses it -- this paragraph
            described the rule the code had already stopped applying.
  forked    a fork of somebody else's open source. Keeps its upstream permissive
            licence, exactly as NOTICE promises. Never `license-file`.
  internal  tooling that inherits the workspace `license = "MIT"`, or declares
            no licence at all. Must be unpublishable -- `publish = false`,
            `"private": true`, a deploy plugin that skips, `IsPackable=false` --
            because that MIT is only harmless while nothing carrying it reaches
            a registry. A licence of its own is refused unless the row records
            it in `declares`, so a deprecated artifact can keep the MIT it was
            born with without that being an accident.
  example   sample code. Never published; whatever it declares is recorded in
            `declares` so that a change to it is a change to the map. #295 found
            pdfluent-examples/rust declaring `MIT OR Apache-2.0` with nothing
            saying whether that was chosen.

WHAT COUNTS AS A PACKAGE

Every tracked manifest: `Cargo.toml` with a `[package]` table wherever it
sits, `package.json`, `pyproject.toml`, `pom.xml`, `*.csproj`. The first
version of this gate globbed `crates/*/Cargo.toml`, one level deep, and seven
first-party crates lived outside that -- fuzz targets, the snippet extractor,
the Rust example -- every one MIT-inheriting and publishable the moment a
single `publish = false` line went missing, and this gate green throughout
(#295). The bindings' own manifests (npm, PyPI, Maven, NuGet) were never
looked at either. Now the tree is enumerated with `git ls-files`, so the set
this gate judges is the set that can be pushed, and a manifest on no side is
a failure rather than a blind spot.

THE TRAP THIS EXISTS FOR

The workspace root declares MIT. Six crates inherit it and all six happen to be
publish = false, so nothing has ever been published as MIT. Nothing enforces
that. Add a crate, forget `publish = false`, and the proprietary SDK goes to
crates.io declaring MIT -- a grant that cannot be withdrawn from anyone who
fetched it. It fails no test and no build; it is simply true afterwards.

WHAT IS NOT CHECKED HERE, AND WHY

Whether crates.io agrees is checked by scripts/ci/license_registry_check.py,
which needs the network. This gate is offline and deliberately so: it has to run
in the commit hook, where a network call would make it skippable.
"""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET

REPO = pathlib.Path(__file__).resolve().parents[2]
KAART = REPO / "docs" / "licensing" / "boundary.toml"

# The workspace default. A crate inheriting this must never be publishable.
WERKRUIMTE_LICENTIE = "MIT"

# A crate on our side of the line must not declare one of these.
PERMISSIEF = re.compile(r"\b(MIT|Apache-2\.0|BSD-[0-9]|ISC|Zlib|Unlicense|CC0)\b")

# FLOOR: a boundary with fewer rows than this is a boundary that stopped
# describing the tree. Set to the population as measured (02-09-2026: 55 crate
# rows, 14 package rows), not below it: a floor under the real count tolerates
# exactly the shrinkage it exists to catch. Removing a package legitimately is
# a deliberate act and lowers the number here in the same commit -- 55 to 53 on
# 06-09-2026, when xfa-license and xfa-license-gen were deleted with the key
# checks they existed to serve (#226).
VLOER = 53
PAKKET_VLOER = 14

# The manifests this gate knows how to read. A tracked file with one of these
# names is a package until the map says otherwise.
MANIFEST_NAMEN = ("Cargo.toml", "package.json", "pyproject.toml", "pom.xml")
MANIFEST_SUFFIX = ".csproj"

ZIJDEN_CRATE = ("ours", "forked", "internal", "example")
ZIJDEN_PAKKET = ("ours", "internal", "example")


# What a crate of ours declares after LC9 (#221). The commercial half is not an
# SPDX identifier and cannot be one, so the manifest names the AGPL and LICENSE
# names both. crates.io renders this field, so this is the string the world sees.
ONZE_LICENTIE = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

# The fork register is the authority on what is a fork. `forked` is the one side
# that permits a permissive licence, so a crate may not simply claim it.
FORKREGISTER = REPO / "docs" / "UPSTREAM_FORKS.toml"


def _forks() -> set[str]:
    if not FORKREGISTER.is_file():
        return set()
    d = tomllib.loads(FORKREGISTER.read_text(encoding="utf-8"))
    return {f["onze_crate"] for f in d.get("fork", [])}


FORKS = _forks()

# --- the AGPL text itself ----------------------------------------------------

AGPL = REPO / "LICENSE-AGPL"

# Pinned to the FSF's own publication at https://www.gnu.org/licenses/agpl-3.0.txt,
# fetched 31-08-2026. Cross-checked word for word against SPDX's stored
# AGPL-3.0-only text: 5535 words in both, and the only three differences are
# `http` -> `https` in FSF/GNU URLs, which is the FSF's own migration and not a
# change to the licence.
#
# WHY A HASH AND NOT A STRUCTURAL CHECK
#
# An altered GPL is not the GPL. Edit one sentence and the result is
# incompatible with every other GPL work and loses the case law that gives the
# text its meaning -- and it would not look wrong, because it would still read
# like a licence. Counting section headings cannot catch a changed sentence
# inside section 7. A hash can, and nothing else here can.
#
# If this ever fails legitimately, the FSF published a new text. Then fetch it,
# diff it deliberately, and change this constant in the same commit -- do not
# make the constant follow the file.
AGPL_SHA256 = "0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0"
AGPL_WOORDEN = 5535


# docs/release/canonical_licenses.toml is the release gate's registry and it is
# older than this file. It went unnoticed while boundary.toml was written, and
# LC9 then flipped 33 manifests past it -- the pre-push hook caught all 23
# publishable ones, which is the only reason this is a paragraph and not an
# incident.
#
# Two registries describing the same fact will drift; that is not a risk, it is
# a schedule. So they are compared here rather than left to agree by hand.
CANONIEK = REPO / "docs" / "release" / "canonical_licenses.toml"


def registers_agree() -> list[str]:
    if not CANONIEK.is_file():
        return [f"{CANONIEK.name} is missing; the release gate's registry and this "
                "boundary can no longer be compared"]
    canon = tomllib.loads(CANONIEK.read_text(encoding="utf-8")).get("crate", [])
    kant = {}
    for rij in tomllib.loads(KAART.read_text(encoding="utf-8")).get("crate", []):
        kant[rij["name"]] = rij["side"]
    uit = []
    for c in canon:
        naam, waarde = c["published_name"], c["license_value"]
        zijde = kant.get(naam)
        if zijde is None:
            uit.append(f"{naam} is in {CANONIEK.name} and on no side of the boundary")
        elif zijde == "ours" and waarde != ONZE_LICENTIE:
            uit.append(f"{naam} is ours on the boundary and {CANONIEK.name} says "
                       f"{waarde!r}")
        elif zijde == "forked" and waarde == ONZE_LICENTIE:
            uit.append(f"{naam} is a fork on the boundary and {CANONIEK.name} "
                       "licenses it as ours")
    return uit

def eigen_refs_zijn_gedefinieerd(cargo_paden: list[str]) -> list[str]:
    """No crate of ours may declare a LicenseRef the policy has never heard of.

    This is the hole #295 is about, narrowed to the part that can be closed
    today. `scan_cargo` skips packages with no `source` -- which is every crate
    we write -- so the ecosystem gate has never judged our own declarations. A
    crate on #1543 declared `LicenseRef-PDFluent-Proprietary`, an identifier
    that appears in no allowed list, no forbidden list and no deny.toml, and
    nothing anywhere said so.

    A LicenseRef is by construction a name we invent. An invented name that
    matches nothing is not a licence; it is a typo with legal shape.
    """
    pol = REPO / "docs" / "LICENSE_POLICY.toml"
    if not pol.is_file():
        return ["docs/LICENSE_POLICY.toml is missing; no declaration can be checked"]
    d = tomllib.loads(pol.read_text(encoding="utf-8"))
    bekend = set(d["licenses"].get("allowed", []))
    bekend |= set(d["licenses"].get("forbidden", []))
    bekend |= set(d["licenses"].get("weak_copyleft", []))
    uit = []
    for rel in cargo_paden:
        t = (REPO / rel).read_text(encoding="utf-8", errors="ignore")
        for m in re.finditer(r'^\s*license\s*=\s*"([^"]+)"', t, re.M):
            for stuk in re.split(r"\s+(?:OR|AND)\s+", m.group(1)):
                stuk = stuk.strip("() ")
                if stuk.startswith("LicenseRef-") and stuk not in bekend:
                    uit.append(f"{rel} declares {stuk!r}, which is "
                               "in no list in docs/LICENSE_POLICY.toml. A LicenseRef "
                               "is a name we invent; one that matches nothing is a "
                               "typo with legal shape")
    return uit

# Every shipped copy of a licence text, and where it must be identical to.
KOPIEEN = ("LICENSE", "LICENSE-AGPL", "LICENSE-COMMERCIAL")
KOPIE_MAPPEN = ("crates/*", "bindings/java", "bindings/dotnet/src/PDFluent")


def kopieen_zijn_gelijk() -> list[str]:
    """A shipped licence copy that has drifted from the root is a second licence.

    This is not hypothetical and it was mine. On 01-09-2026 I corrected the root
    LICENSE -- it had claimed there was no enforcement while the build still
    refuses capabilities -- and thirty per-crate copies kept the false sentence.
    A crates.io tarball would then have carried LICENSE saying no enforcement
    exists and LICENSE-COMMERCIAL saying it does, in one package.

    Copying legal text into thirty directories is the design; nothing here can
    change that today. What can change is whether a copy may quietly differ.
    """
    # Forks keep their upstream licence text. Ours must never be copied over it:
    # crates/lopdf carries the MIT notice naming its upstream author, and a
    # re-sync loop that globs crates/* will overwrite it. That is not a
    # hypothetical -- I did it, twice, and license_registry_check.py caught it
    # both times by insisting the file still names him.
    vorken = {c["dir"] for c in
              tomllib.loads(KAART.read_text(encoding="utf-8")).get("crate", [])
              if c["side"] == "forked"}
    uit = []
    for patroon in KOPIE_MAPPEN:
        for d in sorted(REPO.glob(patroon)):
            if not d.is_dir():
                continue
            if str(d.relative_to(REPO)) in vorken:
                continue
            for naam in KOPIEEN:
                f = d / naam
                if not f.is_file():
                    continue
                bron = REPO / naam
                if not bron.is_file():
                    uit.append(f"{f.relative_to(REPO)} exists and {naam} does not "
                               "at the repository root")
                elif f.read_bytes() != bron.read_bytes():
                    uit.append(f"{f.relative_to(REPO)} differs from the root {naam}. "
                               "A shipped copy that may differ is a second licence, "
                               "and the tarball would carry both")
    return uit

def agpl_is_onaangeroerd() -> list[str]:
    """The AGPL text is byte-for-byte the one we pinned."""
    if not AGPL.is_file():
        return [f"{AGPL.name} is missing; the AGPL half of the dual licence has no text"]
    rauw = AGPL.read_bytes()
    echt = hashlib.sha256(rauw).hexdigest()
    if echt == AGPL_SHA256:
        return []
    woorden = len(rauw.decode("utf-8", "ignore").split())
    return [f"{AGPL.name} hashes to {echt[:16]}…, pinned is {AGPL_SHA256[:16]}… "
            f"({woorden} words, expected {AGPL_WOORDEN}). An edited GPL is not the "
            "GPL: it loses compatibility with every other GPL work and the case "
            "law that interprets it, and it still reads like a licence. If the FSF "
            "published a new text, diff it deliberately and move the pin in the "
            "same commit"]


def lees(pad: pathlib.Path) -> str:
    return pad.read_text(encoding="utf-8", errors="ignore")


def veld(tekst: str, naam: str) -> str | None:
    m = re.search(rf'^\s*{re.escape(naam)}\s*=\s*"([^"]+)"', tekst, re.M)
    return m.group(1) if m else None


# --- the tree, and how each manifest is read -----------------------------------

def tracked_manifests() -> list[str] | None:
    """Every tracked manifest, repository-relative. None if git cannot say.

    `git ls-files`, not a directory walk: the checkout also holds `target/`, a
    generated `crates/xfa-wasm/pkg/package.json`, nested worktrees and whatever
    else a developer left behind, and none of that can be pushed. What is tracked
    is what can reach a registry, so that is the set to judge. GIT_* is dropped
    so a pre-push hook's GIT_DIR cannot point this at some other repository.
    """
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    r = subprocess.run(["git", "ls-files", "-z"], cwd=REPO, env=env,
                       capture_output=True, check=False)
    if r.returncode != 0:
        return None
    uit = []
    for rel in r.stdout.decode("utf-8", "surrogateescape").split("\0"):
        naam = rel.rsplit("/", 1)[-1]
        if naam in MANIFEST_NAMEN or naam.endswith(MANIFEST_SUFFIX):
            uit.append(rel)
    return sorted(uit)


def _lokaal(tag: str) -> str:
    """The element name without its namespace; pom.xml carries one."""
    return tag.rsplit("}", 1)[-1]


def _xml(pad: pathlib.Path) -> ET.Element:
    return ET.fromstring(pad.read_bytes())


def _tekst(el: ET.Element | None) -> str | None:
    if el is None or el.text is None:
        return None
    t = el.text.strip()
    return t or None


def _vind(root: ET.Element, *naam: str) -> list[ET.Element]:
    return [e for e in root.iter() if _lokaal(e.tag) in naam]


def verklaring(rel: str) -> tuple[str | None, bool]:
    """(what the manifest declares as its licence, whether it is unpublishable).

    One reader per ecosystem, each returning the declaration in the spelling
    that ecosystem's channel sees, so it can be compared with what the policy
    says that channel must publish. `None` means the manifest declares nothing.
    Unreadable raises; the caller turns that into a problem, because a manifest
    that cannot be parsed is not one that passed.
    """
    pad = REPO / rel
    naam = pad.name
    if naam == "package.json":
        d = json.loads(pad.read_text(encoding="utf-8"))
        lic = d.get("license")
        if isinstance(lic, dict):
            lic = lic.get("type")
        if lic is None and isinstance(d.get("licenses"), list):
            lic = " OR ".join(str(x.get("type", x)) if isinstance(x, dict) else str(x)
                              for x in d["licenses"]) or None
        return (str(lic) if lic else None), d.get("private") is True
    if naam == "pyproject.toml":
        d = tomllib.loads(pad.read_text(encoding="utf-8"))
        proj = d.get("project", {})
        lic = proj.get("license")
        if isinstance(lic, dict):
            lic = lic.get("text") or (f"file:{lic['file']}" if "file" in lic else None)
        prive = "Private :: Do Not Upload" in proj.get("classifiers", [])
        return (str(lic) if lic else None), prive
    if naam == "pom.xml":
        root = _xml(pad)
        namen = [_tekst(n) for lic in _vind(root, "licenses")
                 for l in lic if _lokaal(l.tag) == "license"
                 for n in l if _lokaal(n.tag) == "name"]
        namen = [n for n in namen if n]
        gedeclareerd = " AND ".join(namen) if namen else None
        # `mvn deploy` is refused by a deploy plugin told to skip. Without that,
        # it still fails when there is nowhere to deploy to: no
        # <distributionManagement> and no publishing plugin. Either shape is
        # unpublishable; a pom with a publishing plugin is not, whatever else
        # it says.
        skipt = False
        uitgevers = False
        for plugin in _vind(root, "plugin"):
            art = next((_tekst(c) for c in plugin if _lokaal(c.tag) == "artifactId"), None)
            if art == "maven-deploy-plugin":
                for conf in plugin:
                    if _lokaal(conf.tag) == "configuration":
                        for c in conf:
                            if _lokaal(c.tag) == "skip" and (_tekst(c) or "").lower() == "true":
                                skipt = True
            if art in ("central-publishing-maven-plugin", "nexus-staging-maven-plugin"):
                uitgevers = True
        heeft_bestemming = bool(_vind(root, "distributionManagement"))
        return gedeclareerd, skipt or not (uitgevers or heeft_bestemming)
    if naam.endswith(MANIFEST_SUFFIX):
        root = _xml(pad)
        def prop(n: str) -> str | None:
            for e in _vind(root, n):
                t = _tekst(e)
                if t:
                    return t
            return None
        expr = prop("PackageLicenseExpression")
        bestand = prop("PackageLicenseFile")
        gedeclareerd = expr or (f"PackageLicenseFile:{bestand}" if bestand else None)
        # `dotnet pack` packs anything, so the only hard refusal is
        # IsPackable=false. An executable with no PackageId and not packed as a
        # tool has no name to publish under; give it one and this flips.
        packable = prop("IsPackable")
        if packable is not None and packable.lower() == "false":
            return gedeclareerd, True
        exe = (prop("OutputType") or "").lower() == "exe"
        tool = (prop("PackAsTool") or "").lower() == "true"
        return gedeclareerd, exe and prop("PackageId") is None and not tool
    raise ValueError(f"{rel}: no reader for this manifest")


def policy_own_packages_are_booked(crate_kant: dict[str, str],
                                   pakket_kant: dict[str, str]) -> list[str]:
    """Every manifest the policy calls ours is booked ours here, too.

    docs/LICENSE_POLICY.toml [own_packages] is the third register naming our
    own manifests (after canonical_licenses.toml and this map). Three lists of
    one fact drift on a schedule, so this is compared rather than trusted.
    """
    pol = REPO / "docs" / "LICENSE_POLICY.toml"
    if not pol.is_file():
        return []  # eigen_refs_zijn_gedefinieerd already reports the missing file
    eigen = tomllib.loads(pol.read_text(encoding="utf-8")).get("own_packages", {})
    uit = []
    for rel in eigen:
        if rel.endswith("Cargo.toml"):
            zijde = crate_kant.get(rel.rsplit("/", 1)[0])
        else:
            zijde = pakket_kant.get(rel)
        if zijde != "ours":
            uit.append(f"{rel} is in docs/LICENSE_POLICY.toml [own_packages] and is "
                       f"{'on no side of the boundary' if zijde is None else 'booked ' + repr(zijde)}"
                       " here. The policy calls it ours; the map has to say the same")
    return uit


def main() -> int:
    if not KAART.is_file():
        print(f"[boundary] FATAL: {KAART.relative_to(REPO)} is missing. Without it "
              "there is no boundary to check and this gate would pass on anything.",
              file=sys.stderr)
        return 1
    kaart = tomllib.loads(lees(KAART))
    rijen = kaart.get("crate") or []
    pakketten = kaart.get("package") or []
    if len(rijen) < VLOER:
        print(f"[boundary] FATAL: {len(rijen)} crate(s) booked, floor is {VLOER}. "
              "A short map reads as a clean run and is not one.", file=sys.stderr)
        return 1
    if len(pakketten) < PAKKET_VLOER:
        print(f"[boundary] FATAL: {len(pakketten)} package(s) booked, floor is "
              f"{PAKKET_VLOER}. A short map reads as a clean run and is not one.",
              file=sys.stderr)
        return 1
    manifesten = tracked_manifests()
    if manifesten is None:
        print("[boundary] FATAL: `git ls-files` failed, so the tree cannot be "
              "enumerated. A sweep that cannot list the tree has swept nothing.",
              file=sys.stderr)
        return 1
    cargo_paden = [m for m in manifesten if m.rsplit("/", 1)[-1] == "Cargo.toml"
                   and re.search(r"^\s*\[package\]", lees(REPO / m), re.M)]

    crate_kant = {rij["dir"]: rij["side"] for rij in rijen}
    pakket_kant = {rij["manifest"]: rij["side"] for rij in pakketten}
    problemen: list[str] = (agpl_is_onaangeroerd() + registers_agree()
                            + eigen_refs_zijn_gedefinieerd(cargo_paden)
                            + kopieen_zijn_gelijk()
                            + policy_own_packages_are_booked(crate_kant, pakket_kant))
    gezien: set[str] = set()

    for rij in rijen:
        naam, kant = rij["name"], rij["side"]
        gezien.add(rij["dir"])
        pad = REPO / rij["dir"] / "Cargo.toml"
        if not pad.is_file():
            problemen.append(f"{naam}: booked at {rij['dir']}, which has no Cargo.toml")
            continue
        t = lees(pad)
        erft = bool(re.search(r"^\s*license\.workspace\s*=\s*true", t, re.M))
        bestand = veld(t, "license-file")
        spdx = None if erft else veld(t, "license")
        eigen = spdx or bestand
        publiceerbaar = not re.search(r"^\s*publish\s*=\s*false", t, re.M)

        if kant == "ours":
            if spdx != ONZE_LICENTIE:
                problemen.append(
                    f"{naam} is ours and declares "
                    f"{spdx or bestand or 'the workspace licence'!r}, not "
                    f"{ONZE_LICENTIE!r}. Our crates are dual-licensed AGPL plus "
                    "commercial; anything else offers the product under terms we "
                    "cannot withdraw from whoever fetched it")
            if bestand:
                problemen.append(
                    f"{naam} still declares `license-file`, which crates.io renders "
                    "instead of an SPDX expression. LC9 replaced that with "
                    f"`license = \"{ONZE_LICENTIE}\"`")
        elif kant == "forked":
            if rij["dir"].split("/")[-1] not in FORKS:
                problemen.append(
                    f"{naam} is booked as a fork and is not in "
                    f"{FORKREGISTER.name}. Four of our own crates were booked this "
                    "way on 31-08 -- pdf-java among them, declaring MIT with a "
                    "comment admitting it was stale -- and this gate approved every "
                    "one, because `forked` is exactly the label that permits a "
                    "permissive licence. Nothing shipped only because all four "
                    "happened to be publish = false")
            if bestand:
                problemen.append(
                    f"{naam} is a fork of somebody else's work and declares "
                    "`license-file`, which puts a PDFluent licence on code that is "
                    "not ours to relicense. NOTICE promises the opposite")
            if spdx and not PERMISSIEF.search(spdx):
                problemen.append(
                    f"{naam} is booked as a fork but declares {spdx!r}, which is not "
                    "the permissive licence NOTICE promises for it")
            if rij.get("declares") and spdx and rij["declares"] != spdx:
                problemen.append(
                    f"{naam} declares {spdx!r}; the boundary records "
                    f"{rij['declares']!r}. Upstream's licence changed, or ours did")
        elif kant in ("internal", "example"):
            if publiceerbaar:
                problemen.append(
                    f"{naam} is {kant} and is publishable. An {kant} crate carries "
                    f"{eigen or 'the workspace licence (' + WERKRUIMTE_LICENTIE + ')'} "
                    "and that is only harmless while nothing carrying it reaches "
                    "crates.io -- published, it is a grant nobody can take back. "
                    "Set `publish = false`, or move it to a side that is meant "
                    "to publish")
            if eigen != rij.get("declares"):
                problemen.append(
                    f"{naam} is {kant} and declares {eigen or 'nothing'!r}; the "
                    f"boundary records {rij.get('declares') or 'nothing'!r}. An "
                    f"{kant} crate inherits the workspace licence or declares none; "
                    "a licence of its own is a choice, and a choice is written "
                    "down in `declares` so that changing it changes the map")
        else:
            problemen.append(f"{naam}: side {kant!r} is not one of "
                             f"{'/'.join(ZIJDEN_CRATE)}")

    beleid = tomllib.loads(lees(REPO / "docs" / "LICENSE_POLICY.toml")) \
        if (REPO / "docs" / "LICENSE_POLICY.toml").is_file() else {}
    kanaal = beleid.get("channel_representation", {})
    gezien_pakket: set[str] = set()
    for rij in pakketten:
        rel, kant = rij["manifest"], rij["side"]
        gezien_pakket.add(rel)
        pad = REPO / rel
        if not pad.is_file():
            problemen.append(f"{rel} is booked on the boundary and does not exist")
            continue
        try:
            eigen, onpubliceerbaar = verklaring(rel)
        except Exception as e:  # noqa: BLE001 -- any parse failure is a finding
            problemen.append(f"{rel} cannot be read ({type(e).__name__}: {e}); an "
                             "unreadable manifest has not passed")
            continue
        if kant == "ours":
            # What this channel publishes, where it cannot carry the SPDX
            # expression itself (npm and nuget.org reject a LicenseRef). The
            # policy writes that spelling down per manifest; anything not
            # written down is expected to carry the canonical expression.
            verwacht = kanaal.get(rel, {}).get("publishes", ONZE_LICENTIE)
            if eigen != verwacht:
                problemen.append(
                    f"{rel} is ours and declares {eigen or 'nothing'!r}, not "
                    f"{verwacht!r}. This is the manifest a registry reads; "
                    "anything else offers the product under terms we cannot "
                    "withdraw from whoever fetched it")
        elif kant in ("internal", "example"):
            if not onpubliceerbaar:
                problemen.append(
                    f"{rel} is {kant} and nothing stops it being published: no "
                    "`private: true`, no deploy skip, no `IsPackable=false`, or a "
                    "PackageId on an executable. Mark it, or move it to a side "
                    "that is meant to publish")
            if eigen != rij.get("declares"):
                problemen.append(
                    f"{rel} is {kant} and declares {eigen or 'nothing'!r}; the "
                    f"boundary records {rij.get('declares') or 'nothing'!r}. A "
                    "licence on something we do not publish is still a choice, "
                    "and a choice is written down in `declares`")
        else:
            problemen.append(f"{rel}: side {kant!r} is not one of "
                             f"{'/'.join(ZIJDEN_PAKKET)}")

    # A package in the tree and not on the map is the actual failure mode: the
    # map is complete on the day it is written and silently stops being so.
    for rel in cargo_paden:
        d = rel.rsplit("/", 1)[0] if "/" in rel else "."
        if d in gezien:
            continue
        problemen.append(
            f"{d} exists and is on no side of the boundary. A new crate defaults "
            "to the workspace MIT, so an unbooked crate is one `publish = false` "
            "away from publishing the SDK permissively")
    for rel in manifesten:
        if rel.rsplit("/", 1)[-1] == "Cargo.toml" or rel in gezien_pakket:
            continue
        problemen.append(
            f"{rel} exists and is on no side of the boundary. Book it as ours, "
            "internal or example in docs/licensing/boundary.toml; a manifest "
            "nobody classified is one nobody is checking")

    if not problemen:
        print(f"[boundary] {len(rijen)} crate(s) and {len(pakketten)} other "
              f"package(s) over {len(manifesten)} tracked manifests; every one "
              "declares what its side says")
        return 0
    print(f"[boundary] {len(problemen)} problem(s) on the licence boundary:",
          file=sys.stderr)
    for p in problemen:
        print(f"  - {p}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
