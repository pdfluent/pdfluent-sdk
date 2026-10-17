#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""One licence policy, every ecosystem (#214).

`deny.toml` guards the Rust workspace and does it well -- it is why the dual
licence model is possible at all. It guards nothing else. A copyleft dependency
could arrive through npm, Maven, NuGet or a Python wheel without meeting a
single check.

This reads `docs/LICENSE_POLICY.toml` and applies it to all of them, including
the Rust tree, so that one list governs everything. It also checks that
`deny.toml` still agrees with that list, because two lists become two policies
within a month.

FOUR WAYS TO FAIL, AND WHY EACH ONE MATTERS

  forbidden   a named copyleft licence turned up.
  unknown     a licence nobody has classified. Not the same as forbidden: it
              means look it up, and it must not pass while nobody has.
  empty       an ecosystem reported fewer dependencies than its declared floor.
              A scanner that finds nothing reports success over nothing, which
              is the failure mode docs/KWALITEITSSPOOR.md records three times.
  unreadable  a manifest exists and could not be parsed. Skipping it silently
              turns a broken scanner into a green tick.

WHAT THIS IS NOT

It does not decide what goes in the attribution file. Inspection covers every
dependency, build and dev included; attribution covers only what a user
receives. `scripts/ci/generate_attribution.py` does that, from the same policy.

Exit codes:
    0  every ecosystem satisfies the policy
    1  a violation, an empty discovery, or an unreadable manifest
"""

from __future__ import annotations

# FLOOR: declared per ecosystem in docs/LICENSE_POLICY.toml [floors]. A scanner
# that reads nothing must fail rather than report a clean tree.
import json
import os
import pathlib
import re
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET
from importlib import metadata as importlib_metadata
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
POLICY = REPO / "docs" / "LICENSE_POLICY.toml"


class Onleesbaar(Exception):
    """A manifest is there and could not be read. Never silently skipped."""


def policy() -> dict:
    if not POLICY.is_file():
        print(f"[license_gate] FATAL: {POLICY.name} is missing; without it this gate has "
              f"no policy and would pass everything", file=sys.stderr)
        sys.exit(1)
    return tomllib.loads(POLICY.read_text())


# --- SPDX ------------------------------------------------------------------

SPLIT = re.compile(r"\s+(OR|AND)\s+", re.I)


def toegestaan(expr: str, ok: set[str], zwak: set[str], verboden: set[str],
               zwak_toegestaan: set[str]) -> tuple[bool, str]:
    """Judge an SPDX expression. Returns (verdict, reason).

    `A OR B` needs one acceptable option; `A AND B` needs all of them. Slash and
    comma spellings ("MIT/Apache-2.0") are older crates writing a disjunction.
    """
    e = expr.replace("/", " OR ").replace(" , ", " OR ").strip("() ")
    delen = [d.strip("() ") for d in SPLIT.split(e) if d.strip("() ").upper() not in ("OR", "AND")]
    if not delen:
        return False, "empty expression"
    disjunctie = " OR " in f" {e.upper()} " or "/" in expr

    def enkel(x: str) -> tuple[bool, str]:
        if x in verboden:
            return False, f"{x} is forbidden"
        if x in ok:
            return True, ""
        if x in zwak:
            return (True, "") if x in zwak_toegestaan else (
                False, f"{x} is file-level copyleft and no surface here permits it")
        return False, f"{x} is not classified"

    oordelen = [enkel(d) for d in delen]
    if disjunctie:
        if any(v for v, _ in oordelen):
            return True, ""
        return False, "; ".join(r for v, r in oordelen if r)
    slecht = [r for v, r in oordelen if not v]
    return (not slecht), "; ".join(slecht)


# --- attribution -------------------------------------------------------------

ATTRIBUTIE = REPO / "THIRD_PARTY_LICENSES.txt"


def elections_are_attributed(pol: dict) -> list[str]:
    """Every recorded election must also be stated in the file we ship.

    An election lives in two places on purpose. `[elections]` in the policy is
    what CI reads; THIRD_PARTY_LICENSES.txt is what a buyer's lawyer reads, and
    it is the only one of the two that leaves the building. JNA sat in neither
    for months, then in the policy alone -- which passes every automated check
    while the shipped attribution still implies the copyleft half of a dual
    licence. Recording the choice where nobody reads it is not recording it.
    """
    keuzes = pol.get("elections", {})
    if not keuzes:
        return []
    if not ATTRIBUTIE.is_file():
        return [f"{ATTRIBUTIE.name} is missing, so no election can be stated in it"]
    regels = ATTRIBUTIE.read_text(encoding="utf-8", errors="ignore").split("\n")
    uit = []
    for coord, keuze in keuzes.items():
        artefact = coord.split(":")[-1]
        raken = [i for i, r in enumerate(regels) if artefact in r]
        if not raken:
            uit.append(f"election for {coord} is in the policy but {artefact} "
                       f"is absent from {ATTRIBUTIE.name}")
            continue
        # Look for the elected licence NEAR the artefact, not anywhere in the
        # file. `Apache-2.0` occurs 23 times in the Rust crate list, so a
        # whole-file substring test passes even when JNA's own entry names the
        # copyleft half -- which is the one mistake this check exists to catch.
        # VENSTER: the entry plus its explanatory paragraph, measured against
        # the real file; the two are never further apart than this.
        VENSTER = 12
        dichtbij = any(
            keuze["take"] in r
            for i in raken
            for r in regels[max(0, i - 2):i + VENSTER]
        )
        if not dichtbij:
            uit.append(f"{artefact} is attributed in {ATTRIBUTIE.name} but the "
                       f"elected licence {keuze['take']} is not named within "
                       f"{VENSTER} lines of it")
    return uit


def embedded_assets_are_attributed() -> list[str]:
    """Every binary asset we ship inside a crate must be named in the attribution.

    A .icc carries someone else's rights and no manifest declares it, so no
    ecosystem scanner above will ever see it: cargo metadata lists crates, not
    the bytes baked into them. ProPhoto-v2-micro.icc sat in the tree for months,
    correctly attributed in its own crate's README and absent from the file that
    actually ships -- found only because someone counted the files by hand. This
    is that count, kept.

    Matched on filename, which is the string a reader greps for. It is a weaker
    check than a hash, and deliberately: the profiles are upstream artefacts we
    do not modify, so drift here is a file appearing or disappearing, not one
    changing underneath us.

    FONTS COUNT TOO, SINCE 08-09-2026 (#383)
    This walked `*.icc` only, so it looked at six files and ignored forty-two --
    fourteen Foxit Type1 programs, the same fourteen as bare CFF, and twelve
    Liberation faces under SIL OFL 1.1, a licence whose whole content is a
    condition on redistributing the bytes. A font carries rights in exactly the
    way this function exists to notice, and it was the one asset class the glob
    did not name. Attribution held by diligence for months; it holds by the
    count now.
    """
    if not ATTRIBUTIE.is_file():
        return [f"{ATTRIBUTIE.name} is missing"]
    tekst = ATTRIBUTIE.read_text(encoding="utf-8", errors="ignore")
    uit = []
    RECHTENDRAGEND = ("*.icc", "*.pfb", "*.cff", "*.ttf", "*.otf", "*.ttc",
                      "*.woff", "*.woff2")
    paden = sorted({p for patroon in RECHTENDRAGEND
                    for p in REPO.glob(f"crates/*/assets/{patroon}")})
    for pad in paden:
        if pad.name not in tekst:
            uit.append(f"{pad.relative_to(REPO)} is shipped but not named in "
                       f"{ATTRIBUTIE.name}")
    return uit


# --- scanners ---------------------------------------------------------------

def _cargo() -> str:
    """Where cargo actually is, not where PATH says it is.

    The GitHub runner has rustup installed and does not put ~/.cargo/bin on PATH
    for this job, so this gate reported `SKIPPED (not a pass): cargo is not on
    PATH` on every run -- honest, legible, and still a licence policy that had
    never once been evaluated. Announcing a skip clearly is better than a
    traceback and is not the same as doing the work.

    rustup's own layout is the answer: $CARGO_HOME/bin/cargo, else
    ~/.cargo/bin/cargo. If neither exists, fall back to the bare name so the
    FileNotFoundError below still produces the message rather than a crash.
    """
    from shutil import which
    gevonden = which("cargo")
    if gevonden:
        return gevonden
    for kandidaat in (
        pathlib.Path(os.environ.get("CARGO_HOME", "")) / "bin" / "cargo",
        pathlib.Path.home() / ".cargo" / "bin" / "cargo",
    ):
        if kandidaat.is_file() and os.access(kandidaat, os.X_OK):
            return str(kandidaat)
    return "cargo"

def scan_cargo(_: dict) -> list[tuple[str, str]]:
    """Every third-party crate in the resolved graph, build and dev included."""
    try:
        r = subprocess.run([_cargo(), "metadata", "--format-version", "1"],
                           capture_output=True, text=True, cwd=REPO,
                           env={k: v for k, v in os.environ.items()
                                if not k.startswith("GIT_")})
    except FileNotFoundError as e:
        # A runner without cargo on PATH used to escape as a bare traceback out
        # of subprocess, which is a crash and not a verdict: the step went red
        # with a stack trace that says nothing about licences, and the reader
        # has no way to tell "769 crates were checked and one is GPL" from
        # "nothing was checked at all". Both are exit 1. This is the second
        # kind, and it says so.
        raise Onleesbaar("SKIPPED (not a pass): cargo is not on PATH, so the "
                         "resolved crate graph was never read") from e
    if r.returncode != 0:
        raise Onleesbaar(f"cargo metadata failed: {r.stderr[-200:]}")
    m = json.loads(r.stdout)
    uit = []
    for p in m["packages"]:
        if not p.get("source"):
            continue  # our own crates; deny.toml clarifies those
        lic = p.get("license")
        uit.append((f"{p['name']} {p['version']}", lic or "UNKNOWN (no license field)"))
    return uit


# A declaration npm or cargo accepts that is not an SPDX expression, so the
# policy has to say what it means. Anything else is checkable and gets checked.
# The one canonical expression. Every [channel_representation] entry has to
# resolve to exactly this: the point of a declared representation is that it
# stands for the SAME statement in a spelling the channel accepts, and an entry
# free to name its own `canonical` moves the escape one field to the right
# instead of closing it. Measured 02-09-2026: `canonical = "MIT"` passed. (T2)
CANONIEK = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"

VERPLICHTE_VELDEN = ("publishes", "canonical", "reason", "file")


def weergave_klopt(rel: str, w: dict) -> str | None:
    """None if the representation entry is sound, else the complaint.

    Every field is checked, because every unchecked field was a way through:
    dropping `canonical`, `reason` or `file`, or pointing `file` at a name that
    does not exist, all left the gate green.
    """
    ontbreekt = [k for k in VERPLICHTE_VELDEN if not w.get(k)]
    if ontbreekt:
        return (f"docs/LICENSE_POLICY.toml [channel_representation] {rel!r} is "
                f"missing {', '.join(ontbreekt)}. A representation without all four "
                "cannot be compared with anything")
    if w["canonical"] != CANONIEK:
        return (f"[channel_representation] {rel!r} declares canonical "
                f"{w['canonical']!r}; the canonical expression is {CANONIEK!r}. "
                "A channel may spell the offer differently, not state a different one")
    pad = REPO / w["file"]
    if not pad.is_file():
        return (f"[channel_representation] {rel!r} names file {w['file']!r}, "
                "which is not in the repository. `SEE LICENSE IN <file>` is a "
                "pointer, and a pointer to nothing is not a licence statement")
    eerste = pad.read_text(encoding="utf-8", errors="replace").splitlines()
    if not eerste or CANONIEK not in eerste[0]:
        return (f"[channel_representation] {rel!r} points at {w['file']!r}, whose "
                f"first line is {(eerste[0] if eerste else '')!r}. It has to state "
                f"{CANONIEK!r}: that first line is the only place the pointer "
                "resolves to the canonical expression")
    return None


NIET_SPDX = re.compile(r"^\s*(SEE LICENSE IN|LicenseRef-|UNLICENSED|PackageLicenseFile:)", re.I)


def eigen_verklaring(rel: str, gedeclareerd: str | None, pol: dict) -> tuple[str, str | None]:
    """(the licence to judge, a complaint if the policy and the manifest disagree).

    `own_packages` exists because npm accepts `SEE LICENSE IN LICENSE`, which is
    not an SPDX expression and cannot be judged against a policy. It used to
    override the manifest unconditionally -- so when LC9 changed
    crates/xfa-wasm/Cargo.toml to AGPL-3.0-or-later while the policy still said
    LicenseRef-PDFluent-Commercial, this gate printed `npm ok` over two files
    that disagreed with each other. Two sources of truth, and the silent one won.

    Now the policy only speaks where the manifest cannot. Where the manifest
    gives a real SPDX expression, that is the answer, and a policy entry that
    contradicts it is itself the finding.
    """
    verwacht = pol.get("own_packages", {}).get(rel)
    if verwacht is None:
        return (gedeclareerd or "UNKNOWN"), None

    # Judged as our own dual licence, not as the AGPL. The allow-list answers
    # "may this licence be in our dependency graph", and AGPL is forbidden there
    # for a good reason: an AGPL DEPENDENCY would reach the whole product. Our
    # own crate declaring AGPL is the opposite -- it is the thing being licensed
    # out, and it is exactly what LC9 set out to do. Reading one rule as the
    # other would have made the licence switch fail its own gate.
    oordeel = "LicenseRef-PDFluent-Dual"
    # The non-SPDX escape is gone as of 01-09-2026.
    #
    # It existed because npm had no spelling for the offer: `SEE LICENSE IN
    # LICENSE` is not an SPDX expression and cannot be compared to a policy. But
    # skipping the comparison meant the `own_packages` entry READ as coverage
    # while checking nothing -- put `SEE LICENSE IN LICENSE` back and this gate
    # stayed green, which is how a channel could drift with an entry beside it.
    #
    # Now that every channel declares the canonical expression, there is a real
    # string to compare on all four, so a manifest that retreats to a non-SPDX
    # form is a finding rather than an exemption.
    if not gedeclareerd:
        return oordeel, (f"{rel} declares no licence at all, while "
                         f"docs/LICENSE_POLICY.toml [own_packages] expects "
                         f"{verwacht!r}")
    if NIET_SPDX.match(gedeclareerd):
        # "every channel can spell it now" was the reasoning here until 02-09-2026,
        # and npm's own validator falsifies it: validate-npm-package-license 3.0.4
        # rejects a LicenseRef expression outright. A channel that cannot carry the
        # canonical string needs a spelling of its own -- but a declared one, named
        # in [channel_representation] with the expression it stands for, matched
        # exactly. That is the difference between a representation and an escape:
        # the escape skipped the comparison, this one still makes it.
        weergave = pol.get("channel_representation", {}).get(rel)
        if weergave and gedeclareerd == weergave.get("publishes") == verwacht:
            klacht = weergave_klopt(rel, weergave)
            return (oordeel if klacht else weergave["canonical"]), klacht
        return oordeel, (f"{rel} declares {gedeclareerd!r}, which is not an SPDX "
                         f"expression. [own_packages] expects {verwacht!r} and no "
                         f"[channel_representation] entry declares it, so it cannot "
                         "be compared with anything")
    if gedeclareerd != verwacht:
        return oordeel, (f"{rel} declares {gedeclareerd!r}; "
                         f"docs/LICENSE_POLICY.toml [own_packages] expects "
                         f"{verwacht!r}. One of the two is out of date")
    return oordeel, None

# Our own npm scope. The platform sub-packages (@pdfluent/node-<tag>) are
# artefacts of this repository: generate-npm-dirs.cjs writes them at packaging
# time and copies the licence field from the main package.json
# (`license: base.license` in platform-matrix.cjs), so they publish under the
# same dual offer as the manifest that names them. Judging them as our own
# dual licence is first-party identification -- the same thing scan_cargo does
# when it skips a crate without a `source` -- not a verdict on a third party.
# A scope is access-controlled on the registry, so nothing foreign can arrive
# under it.
EIGEN_SCOPE = "@pdfluent/"


# First-party Python distributions: our own wheels, judged as our own dual
# licence like EIGEN_SCOPE for npm, never from installed metadata or the
# table. langchain-pdfluent and llama-index-readers-pdfluent are separate
# PyPI distributions but published by us from integrations/, so they are ours.
EIGEN_PYPI = frozenset({
    "pdfluent", "pdfluent-extract", "langchain-pdfluent",
    "llama-index-readers-pdfluent", "pdfluent-mcp",
})

# Cross-check warnings: installed metadata disagreeing with the reviewed
# table. The scanners collect them, main() prints them, and they never
# decide a verdict.
WAARSCHUWINGEN: list[str] = []

# Row labels the gate accepted on a scoped exception this run, with the
# decision behind the acceptance. Cleared at the start of main(), next to
# WAARSCHUWINGEN: the acceptance belongs to this run's table and manifests,
# not to the module.
SCOPED_WAT: dict[str, str] = {}


def npm_licentie(naam: str, soort: str) -> str:
    """The licence to judge an npm dependency by. First-party is ours."""
    if naam.startswith(EIGEN_SCOPE):
        return "LicenseRef-PDFluent-Dual"
    return f"UNKNOWN ({soort} dependency, licence unread)"


def scan_npm(pol: dict) -> list[tuple[str, str]]:
    """Declared runtime dependencies of the npm packages, from tracked sources.

    `crates/xfa-wasm/pkg/package.json` is deliberately not read: wasm-pack
    generates it and .gitignore excludes it, so it exists on a developer machine
    that has run a build and nowhere else. Scanning it made this gate see two
    manifests locally and one in CI -- the declared floor caught that, which is
    what it is for. The wasm package's licence comes from its Cargo.toml, which
    is what wasm-pack copies into the generated manifest.
    """
    uit = []
    for rel_pad in ("crates/pdf-node/package.json",
                    "integrations/pdfluent-extract-js/package.json"):
        pad = REPO / rel_pad
        if not pad.is_file():
            # The front door is optional in fixtures, which predate it; the
            # binding manifest is not. Production always has both, and the
            # front-door test pins that they are scanned there.
            if rel_pad.startswith("integrations/"):
                continue
            raise Onleesbaar(f"{pad.relative_to(REPO)} is missing")
        try:
            d = json.loads(pad.read_text())
        except json.JSONDecodeError as e:
            raise Onleesbaar(f"{pad.relative_to(REPO)}: {e}") from e
        rel = str(pad.relative_to(REPO))
        oordeel, klacht = eigen_verklaring(rel, d.get("license"), pol)
        if klacht:
            uit.append((f"{rel} [policy drift]", "UNKNOWN (" + klacht + ")"))
        uit.append((rel, oordeel))
        for naam, _v in (d.get("dependencies") or {}).items():
            uit.append((f"npm {naam}", npm_licentie(naam, "runtime")))
        # Optional and peer dependencies resolve into the user's install --
        # npm fetches optionalDependencies unless --no-optional, and warns
        # rather than refuses on an unmet peer -- so they are inspected like
        # runtime ones, with their class on the row. devDependencies stay out:
        # they never leave the build machine, and the table name is what tells
        # them apart. Reading no table at all would be the hole this closes.
        # Third-party optionals and peers judge from the reviewed table.
        for naam, _v in (d.get("optionalDependencies") or {}).items():
            wat = f"npm {naam} [optional]"
            res = extra_licentie(naam, "npm", oppervlak="npm-optional")
            if res is None:
                lic = npm_licentie(naam, "optional")
            else:
                lic, w = _extra_rij(wat, res)
                if w:
                    WAARSCHUWINGEN.append(w)
            uit.append((wat, lic))
        for naam, _v in (d.get("peerDependencies") or {}).items():
            wat = f"npm {naam} [peer]"
            res = extra_licentie(naam, "npm", oppervlak="npm-peer")
            if res is None:
                lic = npm_licentie(naam, "peer")
            else:
                lic, w = _extra_rij(wat, res)
                if w:
                    WAARSCHUWINGEN.append(w)
            uit.append((wat, lic))

    # What the publish pipeline WRITES into the generated manifest. The docstring
    # above used to argue that pkg/package.json need not be read because wasm-pack
    # copies the licence from Cargo.toml. Two post-build steps overwrite it before
    # `npm publish`, so that argument held for the file on disk and not for the
    # package on the registry -- the gate said "canonical" while the channel
    # published something else. The artefact is gitignored and absent in CI, so
    # what is checked here is the code that produces it.
    weergave = pol.get("channel_representation", {}).get("crates/xfa-wasm/pkg/package.json")
    if weergave:
        verwacht_str = weergave["publishes"]
        for bron, patroon in (
            (REPO / ".github/workflows/wasm.yml", r"""pkg\.license\s*=\s*['"]([^'"]+)['"]"""),
            (REPO / "scripts/release/transform-wasm-pkg.sh", r"""d\[.license.\]\s*=\s*['"]([^'"]+)['"]"""),
        ):
            if not bron.is_file():
                raise Onleesbaar(f"{bron.relative_to(REPO)} is missing")
            tekst = bron.read_text(encoding="utf-8")
            gevonden = re.findall(patroon, tekst)
            if not gevonden:
                # A FILE THAT DELEGATES IS NOT A FILE THAT FORGOT (#541). The
                # publish workflow used to set the licence itself, doing again
                # what transform-wasm-pkg.sh does; since it calls the transform
                # there is one assignment and it is in the transform, which is
                # the next entry in this loop. Anything else that stopped
                # assigning is still UNKNOWN, which is what this branch is for.
                # RUN, not mentioned: a comment naming the transform is not a
                # job that calls it, and this is the only thing standing between
                # "delegates" and "stopped saying what it publishes". (codex,
                # #1896)
                roept_aan = re.search(
                    r"^\s*(?:-\s*)?(?:run:\s*\|?\s*)?.*\b(?:bash|sh)\s+"
                    r"scripts/release/transform-wasm-pkg\.sh\b",
                    tekst, re.M)
                if bron.name == "wasm.yml" and roept_aan:
                    # Nothing is appended, which is what a matching value does
                    # below: this loop only reports a disagreement.
                    continue
                uit.append((f"{bron.relative_to(REPO)} [publish path]",
                            "UNKNOWN (no licence assignment found; the transform "
                            "changed shape and this check no longer reads it)"))
                continue
            for waarde in gevonden:
                if waarde != verwacht_str:
                    uit.append((f"{bron.relative_to(REPO)} [publish path]",
                                f"UNKNOWN (writes {waarde!r} into the published "
                                f"manifest; [channel_representation] declares "
                                f"{verwacht_str!r})"))

    # The wasm package, from the manifest wasm-pack reads rather than the one it
    # writes.
    wasm = REPO / "crates/xfa-wasm/Cargo.toml"
    if not wasm.is_file():
        raise Onleesbaar("crates/xfa-wasm/Cargo.toml is missing")
    m = re.search(r'^\s*license\s*=\s*"([^"]+)"', wasm.read_text(encoding="utf-8"), re.M)
    oordeel, klacht = eigen_verklaring("crates/xfa-wasm/Cargo.toml",
                                       m.group(1) if m else None, pol)
    if klacht:
        uit.append(("crates/xfa-wasm/Cargo.toml [policy drift]",
                    "UNKNOWN (" + klacht + ")"))
    uit.append(("crates/xfa-wasm/Cargo.toml (npm: @pdfluent/sdk-wasm)", oordeel))
    return uit


def scan_maven(pol: dict) -> list[tuple[str, str]]:
    """Non-test dependencies of the Java binding, honouring recorded elections."""
    pad = REPO / "bindings/java/pom.xml"
    if not pad.is_file():
        raise Onleesbaar("bindings/java/pom.xml is missing")
    try:
        boom = ET.fromstring(pad.read_text())
    except ET.ParseError as e:
        raise Onleesbaar(f"pom.xml: {e}") from e
    ns = {"m": "http://maven.apache.org/POM/4.0.0"}
    keuzes = pol.get("elections", {})
    uit = []
    for dep in boom.iter(f"{{{ns['m']}}}dependency"):
        g = dep.findtext(f"{{{ns['m']}}}groupId", "")
        a = dep.findtext(f"{{{ns['m']}}}artifactId", "")
        scope = dep.findtext(f"{{{ns['m']}}}scope", "compile")
        if scope == "test":
            continue
        # `<scope>test</scope>` is the only scope that stays out: it rides the
        # test classpath and never the published artefact. `<optional>true`
        # and `<scope>provided</scope>` both reach a consumer -- the first when
        # the consumer opts in, the second from the container at runtime -- so
        # they are rows like any other, with their class on the row.
        klassen = []
        optioneel = (dep.findtext(f"{{{ns['m']}}}optional", "false") or "")
        if optioneel.strip().lower() == "true":
            klassen.append("optional")
        if (scope or "").strip() == "provided":
            klassen.append("provided")
        coord = f"{g}:{a}"
        if klassen:
            coord += " [" + ", ".join(klassen) + "]"
        if klassen:
            # Optional and provided rows judge from the reviewed table.
            wat = f"maven {coord}"
            res = extra_licentie(f"{g}:{a}", "maven",
                                 oppervlak="maven-" + "+".join(klassen))
            if res is None:
                keuze = keuzes.get(f"{g}:{a}")
                if keuze:
                    uit.append((f"maven {coord} (election: {keuze['take']})",
                                keuze["take"]))
                else:
                    uit.append((f"maven {coord}", "UNKNOWN (no election recorded)"))
            else:
                lic, w = _extra_rij(wat, res)
                if w:
                    WAARSCHUWINGEN.append(w)
                uit.append((wat, lic))
            continue
        keuze = keuzes.get(f"{g}:{a}")
        if keuze:
            uit.append((f"maven {coord} (election: {keuze['take']})", keuze["take"]))
        else:
            uit.append((f"maven {coord}", "UNKNOWN (no election recorded)"))
    return uit


def scan_nuget(_: dict) -> list[tuple[str, str]]:
    projecten = list((REPO / "bindings/dotnet/src").glob("*/*.csproj"))
    uit_eigen: list[tuple[str, str]] = []
    # The project's OWN licence, which this scanner never read: it collected
    # PackageReference entries and nothing else, so removing
    # <PackageLicenseExpression> left the gate green (#214/#304).
    for p in projecten:
        rel = p.relative_to(REPO).as_posix()
        if rel not in _.get("own_packages", {}):
            continue
        tekst = p.read_text()
        m = re.search(r"<PackageLicenseExpression>([^<]+)</PackageLicenseExpression>", tekst)
        if m:
            gedeclareerd = m.group(1)
        else:
            # NuGet's other form. NU5033 forbids both at once, and NU5124 makes the
            # expression unusable here, so the file IS the declaration -- reading
            # only the expression element would see nothing and call that "no
            # licence", which is the opposite of what the manifest says.
            f = re.search(r"<PackageLicenseFile>([^<]+)</PackageLicenseFile>", tekst)
            gedeclareerd = f"PackageLicenseFile:{f.group(1)}" if f else None
        oordeel, klacht = eigen_verklaring(rel, gedeclareerd, _)
        if klacht:
            raise Onleesbaar(klacht)
        uit_eigen.append((f"nuget {p.stem} (package)", oordeel))
    if not projecten:
        raise Onleesbaar("no .csproj found under bindings/dotnet/src")
    # uit_eigen used to be built and then dropped: the function returned `uit`,
    # whose first entry was the constant "LicenseRef-PDFluent-Commercial". So the
    # validation above ran, its verdict went nowhere, and the gate reported a
    # hardcoded answer -- the same shape as the wheel scanner, and the comment at
    # the top of this block described a repair that never reached the output.
    uit = []
    for p in projecten:
        tekst = p.read_text()
        for m in re.finditer(r'<PackageReference\s+Include="([^"]+)"', tekst):
            # A reference that carries PrivateAssets or IncludeAssets does not
            # flow to consumers by default, but it still builds what ships --
            # and the inspection set is every dependency, not every flowing
            # one. So it is a row like any other, with its class on the row.
            # Test projects stay out by the glob above, which reads src only:
            # bindings/dotnet/test never reaches this loop.
            element = _package_reference_element(tekst, m.start())
            # An attribute (` PrivateAssets="all"`) or a child element
            # (`<PrivateAssets>all</PrivateAssets>`), and nothing looser: the
            # package's own name sits in this same text, so a bare substring
            # would let `Include="PrivateAssets.Foo"` label itself.
            privaat = re.search(
                r"[<\s](?:PrivateAssets|IncludeAssets)\s*[=>]", element) is not None
            naam = f"nuget {m.group(1)}" + (" [private]" if privaat else "")
            if privaat:
                res = extra_licentie(m.group(1), "nuget", oppervlak="nuget-private")
                if res is None:
                    lic = "UNKNOWN (licence unread)"
                else:
                    lic, w = _extra_rij(naam, res)
                    if w:
                        WAARSCHUWINGEN.append(w)
            else:
                lic = "UNKNOWN (licence unread)"
            uit.append((naam, lic))
    return uit_eigen + uit


def _package_reference_element(tekst: str, begin: int) -> str:
    """The full `<PackageReference ...>` element starting at `begin`.

    The Include attribute is matched by regex, but PrivateAssets and
    IncludeAssets may sit in the attribute list OR in child elements, so the
    element's own text is what is searched -- never the whole file, where a
    neighbouring reference would answer for this one.
    """
    rest = tekst[begin:]
    einde = rest.find(">")
    if einde == -1:
        return rest
    if rest[einde - 1] == "/":
        return rest[:einde + 1]
    sluit = rest.find("</PackageReference>")
    if sluit == -1:
        return rest[:einde + 1]
    return rest[:sluit + len("</PackageReference>")]


def distributie_naam(spec: str) -> str:
    """The distribution a PEP 508 requirement names ("pytest>=7.0" -> "pytest").

    Version constraints, extras (`requests[security]`) and markers (`; ...`)
    are install instructions; the licence question is about the distribution.
    """
    return re.split(r"[<>=!~\s;\[]", spec.strip(), maxsplit=1)[0].strip()


def pypi_licentie(naam: str) -> str:
    """The licence an installed distribution declares for itself, or UNKNOWN.

    The smallest honest resolver: the distribution's own metadata says what
    its licence is, and where it is absent -- not installed here, no licence
    field, or a field that carries the full text instead of an identifier --
    the row is unknown. Nothing is guessed and no verdict is hardcoded: an
    unclassified identifier still has to be looked up and added to the policy.
    """
    try:
        meta = importlib_metadata.metadata(naam)
    except importlib_metadata.PackageNotFoundError:
        return f"UNKNOWN ({naam} is not installed here, so no metadata states its licence)"
    except Exception as e:
        return f"UNKNOWN (metadata for {naam} is unreadable: {e})"
    verklaard = meta.get("License-Expression") or meta.get("License")
    if not verklaard or not verklaard.strip():
        return f"UNKNOWN ({naam} is installed but declares no licence)"
    verklaard = verklaard.strip()
    if "\n" in verklaard or len(verklaard) > 120:
        # A field carrying the full licence text (numpy's BSD text) states no
        # identifier. Feeding prose to the SPDX judge would split it on "and"
        # and "or" and report the pieces, so the finding names the absence.
        return f"UNKNOWN ({naam} declares no SPDX identifier)"
    return verklaard


def extra_tabel() -> dict[tuple[str, str], dict] | None:
    """The reviewed extra-licence table, or None if its file is absent.

    None is the fixture case: throwaway trees predate the table and carry
    none, so their scanners fall back to the resolvers they always used.
    Production always has the file; a present-but-unparsable file is a
    misconfigured gate and fails.
    """
    pad = REPO / "scripts" / "ci" / "extra_licences.toml"
    if not pad.is_file():
        return None
    try:
        d = tomllib.loads(pad.read_text())
    except tomllib.TOMLDecodeError as e:
        raise Onleesbaar(f"extra_licences.toml: {e}") from e
    uit: dict[tuple[str, str], dict] = {}
    for r in d.get("extra", []) or []:
        uit[(str(r.get("ecosystem", "")).lower(),
             str(r.get("package", "")).lower())] = r
    return uit


_VERSIE = re.compile(r"^\d+(\.\d+)*$")


def _versie(s: str) -> tuple[int, ...] | None:
    s = s.strip()
    if not _VERSIE.match(s):
        return None
    return tuple(int(x) for x in s.split("."))


def _eisen(spec: str) -> list[tuple[str, tuple[int, ...]]] | None:
    """The PEP 440 clauses of one requirement tail (">=11.2.1,<13").

    None when unparsable -- the gate fails closed, so unparsable is red,
    not guessed.
    """
    uit: list[tuple[str, tuple[int, ...]]] = []
    for deel in spec.split(","):
        deel = deel.strip()
        for op in ("===", "~=", "==", "!=", ">=", "<=", ">", "<"):
            if deel.startswith(op):
                v = _versie(deel[len(op):])
                if v is None:
                    return None
                uit.append((op, v))
                break
        else:
            return None
    return uit


def _cmp(a: tuple[int, ...], b: tuple[int, ...]) -> int:
    """Version order, zero-padded: (13,) counts as (13, 0, 0) next to
    (11, 2, 1), because a bare tuple comparison reads 1.0 as smaller
    than 1."""
    n = max(len(a), len(b))
    aa = a + (0,) * (n - len(a))
    bb = b + (0,) * (n - len(b))
    return (aa > bb) - (aa < bb)


def _bereik(eisen: list[tuple[str, tuple[int, ...]]]) -> tuple | None:
    """(lo, lo_incl, hi, hi_incl) of the clause set. None when the set is
    open on either side, unsatisfiable, or uses an operator containment
    cannot prove: all three fail closed. `!=` only removes versions, so it
    cannot widen the set and is ignored for containment."""
    lo: tuple[int, ...] | None = None
    lo_incl = False
    hi: tuple[int, ...] | None = None
    hi_incl = False
    for op, v in eisen:
        if op == "==":
            if lo is None or _cmp(v, lo) > 0:
                lo, lo_incl = v, True
            if hi is None or _cmp(v, hi) < 0:
                hi, hi_incl = v, True
        elif op == "~=":
            if len(v) < 2:
                return None
            if lo is None or _cmp(v, lo) > 0:
                lo, lo_incl = v, True
            top = (v[0] + 1,)
            if hi is None or _cmp(top, hi) < 0:
                hi, hi_incl = top, False
        elif op in (">=", ">"):
            incl = op == ">="
            if lo is None or _cmp(v, lo) > 0:
                lo, lo_incl = v, incl
            elif _cmp(v, lo) == 0:
                lo_incl = lo_incl and incl
        elif op in ("<=", "<"):
            incl = op == "<="
            if hi is None or _cmp(v, hi) < 0:
                hi, hi_incl = v, incl
            elif _cmp(v, hi) == 0:
                hi_incl = hi_incl and incl
        elif op == "!=":
            continue
        else:
            return None
    if lo is None or hi is None:
        return None
    c = _cmp(lo, hi)
    if c > 0 or (c == 0 and not (lo_incl and hi_incl)):
        return None
    return (lo, lo_incl, hi, hi_incl)


def binnen_bereik(declaratie: str, begrensd: str) -> bool:
    """True when every version `declaratie` admits also satisfies `begrensd`.

    Both are comma-separated PEP 440 clause sets. Either side unparsable, or
    the declared set open on a side the reviewed range closes, is False. An
    unbounded declaration (a bare package name with no pin) admits everything,
    so it is never inside a reviewed range.
    """
    d = _eisen(declaratie)
    a = _eisen(begrensd)
    if not d or not a:
        return False
    if any(op == "!=" for op, _ in a):
        return False
    db = _bereik(d)
    ab = _bereik(a)
    if db is None or ab is None:
        return False
    (dlo, dlo_incl, dhi, dhi_incl) = db
    (alo, alo_incl, ahi, ahi_incl) = ab
    if _cmp(dlo, alo) < 0:
        return False
    if _cmp(dlo, alo) == 0 and dlo_incl and not alo_incl:
        return False
    if _cmp(dhi, ahi) > 0:
        return False
    if _cmp(dhi, ahi) == 0 and dhi_incl and not ahi_incl:
        return False
    return True


def _eis_staart(eis: str, naam: str) -> str:
    """The version tail of a requirement ("Pillow>=11.2.1,<13" -> ">=11.2.1,<13").

    Extras ("Pillow[x]") and markers ("; python_version > '3.8'") are install
    instructions, not versions, so they are stripped before the range check.
    """
    staart = eis[len(naam):] if eis.lower().startswith(naam.lower()) else eis
    staart = staart.split(";")[0].strip()
    if staart.startswith("["):
        einde = staart.find("]")
        staart = staart[einde + 1:] if einde != -1 else ""
    return staart.strip()


def extra_licentie(naam: str, ecosysteem: str, eis: str = "",
                   oppervlak: str = "") -> tuple[str, str | None, str | None] | None:
    """(SPDX from the table, cross-check warning, scoped note) for an
    extra/optional row.

    None if the table file is absent (fixture fallback; the caller uses its
    old resolver). A miss IN the table is red with the add-hint, never a
    fallback: an unreviewed package must not pass on machine metadata.
    First-party distributions judge as our own dual licence without the table.

    A row carrying `versions` and `surface` is a scoped exception (Pillow,
    D34): its `spdx` is accepted ONLY when the manifest's declared
    requirement lies inside `versions` AND the row arrives on `surface`. The
    acceptance travels in the scoped note; the caller records it against the
    row label in SCOPED_WAT for main() to honour. Anything else -- another
    package, another range, another surface -- stays red, and `allowed` in
    docs/LICENSE_POLICY.toml never widens for it.
    """
    if ecosysteem == "pypi" and naam.lower() in EIGEN_PYPI:
        return ("LicenseRef-PDFluent-Dual", None, None)
    if ecosysteem == "npm" and naam.startswith(EIGEN_SCOPE):
        return ("LicenseRef-PDFluent-Dual", None, None)
    tabel = extra_tabel()
    if tabel is None:
        return None
    r = tabel.get((ecosysteem.lower(), naam.lower()))
    if r is None:
        return (f"UNKNOWN (add {naam} to scripts/ci/extra_licences.toml)", None, None)
    spdx = str(r.get("spdx", "")).strip()
    if not spdx:
        return (f"UNKNOWN ({naam} has no spdx in scripts/ci/extra_licences.toml)", None, None)
    begrensd = str(r.get("versions", "") or "").strip()
    vlak = str(r.get("surface", "") or "").strip()
    if begrensd or vlak:
        # A scoped exception is package AND range AND surface, or nothing.
        # `decision` and `source` are the review trail; an exception without
        # them is a row that forgot why it is there.
        besluit = str(r.get("decision", "") or "").strip()
        bron = str(r.get("source", "") or "").strip()
        if not besluit or not bron:
            return (f"UNKNOWN ({naam} is a scoped exception without its review "
                    f"trail; record decision and source in "
                    f"scripts/ci/extra_licences.toml)", None, None)
        if vlak and vlak.lower() != (oppervlak or "").lower():
            return (f"UNKNOWN ({naam} is accepted only as {vlak}, not as "
                    f"{oppervlak or 'this surface'}; see decision {besluit})",
                    None, None)
        if begrensd:
            staart = _eis_staart(eis, naam)
            if not staart or not binnen_bereik(staart, begrensd):
                return (f"UNKNOWN ({eis or naam} is outside the reviewed "
                        f"{begrensd}; see decision {besluit})", None, None)
        return (spdx, None, f"{besluit} {vlak} {begrensd}".strip())
    waarschuwing = None
    if ecosysteem == "pypi":
        # Cross-check only: installed metadata never decides the verdict.
        geinstalleerd = pypi_licentie(naam)
        if not geinstalleerd.startswith("UNKNOWN") and geinstalleerd != spdx:
            waarschuwing = (f"installed {naam} declares {geinstalleerd}, "
                            f"the table says {spdx}; verdict from the table")
    return (spdx, waarschuwing, None)


def _extra_rij(wat: str, res: tuple[str, str | None, str | None]
               ) -> tuple[str, str | None]:
    """(licence, warning) for a scanned extra row, recording a scoped
    acceptance against the row label for main() to honour."""
    lic, w, scoped = res
    if scoped:
        SCOPED_WAT[wat] = scoped
    return lic, w


def scan_python(_: dict) -> list[tuple[str, str]]:
    uit: list[tuple[str, str]] = []
    for rel in ("crates/pdf-python/pyproject.toml",
                "integrations/pdfluent-extract/pyproject.toml"):
        pad = REPO / rel
        if not pad.is_file():
            # The front door is optional in fixtures, which predate it; the
            # wheel manifest is not. Production always has both, and the
            # front-door test pins that they are scanned there.
            if rel.startswith("integrations/"):
                continue
            raise Onleesbaar(f"{rel} is missing")
        try:
            d = tomllib.loads(pad.read_text())
        except tomllib.TOMLDecodeError as e:
            raise Onleesbaar(f"{rel}: {e}") from e
        # WAT HET BESTAND ZEGT, NIET WAT WIJ ERVAN VONDEN
        #
        # Tot 01-09-2026 stond hier onvoorwaardelijk
        # `("python pdfluent (wheel)", "LicenseRef-PDFluent-Commercial")`. Het
        # bestand werd wel geparset en de licentie nooit gelezen, dus de wheel werd
        # als commercieel-only geoordeeld ongeacht wat er stond. Zet de declaratie
        # op iets anders -- of draai hem terug -- en deze poort bleef groen (#1620).
        #
        # Dat is dezelfde vorm als #300, #301 en #304: een controle die een
        # naastgelegen vraag beantwoordt en er compleet uitziet.
        #
        # PEP 621 kent twee vormen: `license = { text = "..." }` (oud) en
        # `license = "..."` (PEP 639). Allebei lezen; ontbreekt hij, dan is dat een
        # bevinding en geen aanname.
        lic = d.get("project", {}).get("license")
        if isinstance(lic, dict):
            spdx = lic.get("text") or lic.get("file")
        elif isinstance(lic, str):
            spdx = lic
        else:
            spdx = None
        if not spdx:
            raise Onleesbaar(
                f"{rel} declares no [project] license. "
                "A wheel without a licence is not a wheel we may publish."
            )
        # Judged like our other own packages: the allow-list answers "may this be in
        # our dependency graph", which is a different question from "is this what we
        # mean to publish". MIT is fine as a dependency and would be a disaster here.
        oordeel, klacht = eigen_verklaring(rel, spdx, _)
        if klacht:
            raise Onleesbaar(klacht)
        wiel = d.get("project", {}).get("name", rel)
        uit.append((f"python {wiel} (wheel)", oordeel))
        project = d.get("project", {})
        for spec in project.get("dependencies", []) or []:
            naam = distributie_naam(spec)
            if not naam:
                lic = "UNKNOWN (unparsable requirement)"
            elif naam.lower() in EIGEN_PYPI:
                lic = "LicenseRef-PDFluent-Dual"
            else:
                lic = pypi_licentie(naam)
            uit.append((f"pypi {spec}", lic))
        # Every optional extra is published wheel metadata -- `pip install
        # pdfluent[pillow]` resolves it -- so every entry is a row of class
        # "extra", judged by the same policy as a runtime dependency. This table
        # used to be unread entirely: an extra under any licence passed. The
        # SPDX comes from the reviewed table, never from this machine.
        extras = project.get("optional-dependencies", {}) or {}
        for extra, specs in extras.items():
            for spec in specs or []:
                naam = distributie_naam(spec)
                wat = f"pypi {spec} [extra {extra}]"
                if not naam:
                    lic = "UNKNOWN (unparsable requirement)"
                else:
                    res = extra_licentie(naam, "pypi", eis=spec,
                                         oppervlak="python-extra")
                    if res is None:
                        lic = pypi_licentie(naam)
                    else:
                        lic, w = _extra_rij(wat, res)
                        if w:
                            WAARSCHUWINGEN.append(w)
                uit.append((wat, lic))
    return uit


SCANNERS = {
    "cargo": (scan_cargo, "cargo_packages"),
    "npm": (scan_npm, "npm_manifests"),
    "maven": (scan_maven, "maven_dependencies"),
    "nuget": (scan_nuget, "dotnet_projects"),
    "python": (scan_python, "python_manifests"),
}


def deny_komt_overeen(pol: dict) -> str | None:
    """deny.toml must still be the same policy, not a second one."""
    pad = REPO / "deny.toml"
    if not pad.is_file():
        return "deny.toml is missing"
    allow = set(tomllib.loads(pad.read_text())["licenses"]["allow"])
    # cargo-deny spans the whole workspace: the SDK crates, the editor and the
    # internal tools in one graph. Its allow list is therefore the union of what
    # every surface permits, not what the SDK alone permits.
    zwak_ergens = set()
    for opp in pol["surfaces"].values():
        zwak_ergens |= set(opp.get("weak_copyleft", []))
    verwacht = set(pol["licenses"]["allowed"]) | zwak_ergens
    extra, mist = allow - verwacht, verwacht - allow
    if extra or mist:
        return (f"deny.toml and the policy disagree — only in deny.toml: "
                f"{sorted(extra) or '-'}; missing from deny.toml: {sorted(mist) or '-'}")
    return None


# How many channels are declared to spell the offer their own way. A table that
# can be emptied is a check that can be switched off by deletion, and the rest of
# this file already floors every scanner for exactly that reason.
WEERGAVE_VLOER = 3


def weergaven_kloppen(pol: dict) -> list[str]:
    """Every [channel_representation] entry, whether or not its manifest exists.

    `weergave_klopt` was reachable only through `eigen_verklaring`, which runs
    once per manifest READ FROM DISK. `crates/xfa-wasm/pkg/package.json` is
    generated by wasm-pack and gitignored, so it is absent everywhere except a
    machine that has just built -- and its entry was therefore never validated at
    all. Measured 02-09-2026: `canonical = "MIT"` on that one entry passed all
    five licence gates, which is precisely the escape the comment above CANONIEK
    says was closed. It was closed for the two entries whose manifest is tracked.

    The absent artefact is the REASON the entry exists, so its absence cannot be
    the reason not to check it. The entry is a claim in the policy file, the
    policy file is always present, and that is what is read here.
    """
    weergaven = pol.get("channel_representation", {})
    if len(weergaven) < WEERGAVE_VLOER:
        return [f"docs/LICENSE_POLICY.toml declares {len(weergaven)} "
                f"[channel_representation] entr(y/ies); the floor is "
                f"{WEERGAVE_VLOER}. Removing one removes the only comparison "
                "between what a channel publishes and what it means"]
    return [k for k in (weergave_klopt(rel, w) for rel, w in sorted(weergaven.items())) if k]


def main() -> int:
    pol = policy()
    ok = set(pol["licenses"]["allowed"])
    zwak = set(pol["licenses"]["weak_copyleft"])
    verboden = set(pol["licenses"]["forbidden"])
    vloeren = pol["floors"]

    only = None
    if "--only" in sys.argv:
        only = sys.argv[sys.argv.index("--only") + 1]

    # `cargo` covers the whole workspace, editor included, so weak copyleft is
    # judged there under the editor's rule. The SDK surface is checked
    # separately by scripts/ci/internal_stays_internal.py's sibling in #212.
    # Which surface each scanner speaks for. cargo spans the workspace, so it
    # judges by what any surface permits; the binding ecosystems are SDK only.
    zwak_per_scanner = {
        "cargo": set().union(*(set(o.get("weak_copyleft", [])) for o in pol["surfaces"].values())),
        "npm": set(pol["surfaces"]["sdk"].get("weak_copyleft", [])),
        "maven": set(pol["surfaces"]["sdk"].get("weak_copyleft", [])),
        "nuget": set(pol["surfaces"]["sdk"].get("weak_copyleft", [])),
        "python": set(pol["surfaces"]["sdk"].get("weak_copyleft", [])),
    }

    stuk = deny_komt_overeen(pol)
    if stuk:
        print(f"[license_gate] FAIL: {stuk}", file=sys.stderr)
        return 1
    print("[license_gate] deny.toml agrees with the policy")

    scheef = weergaven_kloppen(pol)
    if scheef:
        for s in scheef:
            print(f"[license_gate] FAIL: {s}", file=sys.stderr)
        return 1
    print(f"[license_gate] all {len(pol.get('channel_representation', {}))} channel "
          "representations resolve to the canonical expression")

    problemen: list[str] = []
    WAARSCHUWINGEN.clear()
    SCOPED_WAT.clear()
    for naam, (fn, vloer_sleutel) in SCANNERS.items():
        if only and naam != only:
            continue
        try:
            gevonden = fn(pol)
        except Onleesbaar as e:
            problemen.append(f"{naam}: unreadable — {e}")
            print(f"  {naam:8} UNREADABLE  {e}")
            continue
        vloer = vloeren[vloer_sleutel]
        if len(gevonden) < vloer:
            problemen.append(f"{naam}: found {len(gevonden)}, floor is {vloer}")
            print(f"  {naam:8} EMPTY       {len(gevonden)} < {vloer}")
            continue
        slecht = []
        # Rows a scoped exception accepted during this run's scans. The
        # acceptance is package AND range AND surface (SCOPED_WAT, recorded
        # against the exact row label), never the expression alone: MIT-CMU
        # anywhere else stays red.
        vastgesteld = [(w, SCOPED_WAT[w]) for w, _ in gevonden if w in SCOPED_WAT]
        for wat, expr in gevonden:
            if wat in SCOPED_WAT:
                continue
            goed, reden = toegestaan(expr, ok, zwak, verboden, zwak_per_scanner[naam])
            if not goed:
                slecht.append(f"{wat}: {expr}  ({reden})")
        print(f"  {naam:8} {'ok  ' if not slecht else 'FAIL'}        "
              f"{len(gevonden)} item(s), {len(slecht)} problem(s), "
              f"{len(vastgesteld)} scoped")
        for s in slecht[:10]:
            print(f"      {s}")
        for w, grond in vastgesteld:
            print(f"      {w}  (scoped accept: {grond})")
        problemen += [f"{naam}: {s}" for s in slecht]

    for w in WAARSCHUWINGEN:
        print(f"[license_gate] WARNING: {w}")

    for m in elections_are_attributed(pol) + embedded_assets_are_attributed():
        problemen.append(f"attribution: {m}")
        print(f"  {'attrib':8} FAIL        {m}")

    if not problemen:
        print("[license_gate] every ecosystem satisfies the policy")
        return 0
    print()
    print(f"[license_gate] {len(problemen)} problem(s). A licence that is not classified is")
    print("[license_gate] not the same as one that is allowed: add it to `allowed`,")
    print("[license_gate] `forbidden` or `weak_copyleft` in docs/LICENSE_POLICY.toml, or")
    print("[license_gate] record an election under [elections] if it is dual-licensed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
