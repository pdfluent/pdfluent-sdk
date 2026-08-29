#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

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
import re
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET
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


# --- scanners ---------------------------------------------------------------

def scan_cargo(_: dict) -> list[tuple[str, str]]:
    """Every third-party crate in the resolved graph, build and dev included."""
    r = subprocess.run(["cargo", "metadata", "--format-version", "1"],
                       capture_output=True, text=True, cwd=REPO,
                       env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
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
    for pad in (REPO / "crates/pdf-node/package.json",):
        if not pad.is_file():
            raise Onleesbaar(f"{pad.relative_to(REPO)} is missing")
        try:
            d = json.loads(pad.read_text())
        except json.JSONDecodeError as e:
            raise Onleesbaar(f"{pad.relative_to(REPO)}: {e}") from e
        rel = str(pad.relative_to(REPO))
        eigen = pol.get("own_packages", {}).get(rel)
        uit.append((rel, eigen or d.get("license", "UNKNOWN")))
        for naam, _v in (d.get("dependencies") or {}).items():
            uit.append((f"npm {naam}", "UNKNOWN (runtime dependency, licence unread)"))

    # The wasm package, from the manifest wasm-pack reads rather than the one it
    # writes.
    wasm = REPO / "crates/xfa-wasm/Cargo.toml"
    if not wasm.is_file():
        raise Onleesbaar("crates/xfa-wasm/Cargo.toml is missing")
    eigen = pol.get("own_packages", {}).get("crates/xfa-wasm/Cargo.toml")
    uit.append(("crates/xfa-wasm/Cargo.toml (npm: @pdfluent/sdk-wasm)",
                eigen or "UNKNOWN"))
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
        coord = f"{g}:{a}"
        keuze = keuzes.get(coord)
        if keuze:
            uit.append((f"maven {coord} (election: {keuze['take']})", keuze["take"]))
        else:
            uit.append((f"maven {coord}", "UNKNOWN (no election recorded)"))
    return uit


def scan_nuget(_: dict) -> list[tuple[str, str]]:
    projecten = list((REPO / "bindings/dotnet/src").glob("*/*.csproj"))
    if not projecten:
        raise Onleesbaar("no .csproj found under bindings/dotnet/src")
    uit = []
    for p in projecten:
        uit.append((f"dotnet {p.stem}", "LicenseRef-PDFluent-Commercial"))
        for m in re.finditer(r'<PackageReference\s+Include="([^"]+)"', p.read_text()):
            uit.append((f"nuget {m.group(1)}", "UNKNOWN (licence unread)"))
    return uit


def scan_python(_: dict) -> list[tuple[str, str]]:
    pad = REPO / "crates/pdf-python/pyproject.toml"
    if not pad.is_file():
        raise Onleesbaar("crates/pdf-python/pyproject.toml is missing")
    try:
        d = tomllib.loads(pad.read_text())
    except tomllib.TOMLDecodeError as e:
        raise Onleesbaar(f"pyproject.toml: {e}") from e
    uit = [("python pdfluent (wheel)", "LicenseRef-PDFluent-Commercial")]
    for spec in d.get("project", {}).get("dependencies", []):
        uit.append((f"pypi {spec}", "UNKNOWN (runtime dependency, licence unread)"))
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

    problemen: list[str] = []
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
        for wat, expr in gevonden:
            goed, reden = toegestaan(expr, ok, zwak, verboden, zwak_per_scanner[naam])
            if not goed:
                slecht.append(f"{wat}: {expr}  ({reden})")
        print(f"  {naam:8} {'ok  ' if not slecht else 'FAIL'}        "
              f"{len(gevonden)} item(s), {len(slecht)} problem(s)")
        for s in slecht[:10]:
            print(f"      {s}")
        problemen += [f"{naam}: {s}" for s in slecht]

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
