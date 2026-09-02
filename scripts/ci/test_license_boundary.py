#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The ways `license_boundary.py` must go red, proved on a copy of the tree (#295).

The gate was green while seven first-party crates sat outside the one glob it
used and while no binding manifest was read at all. A guard that has only ever
passed cannot be told apart from one that checks nothing, so every case here
MUTATES a throwaway copy of the real map and the real manifests and demands the
verdict flip -- and names the package it flipped on, because "1 problem" without
a name sends the reader back to the diff.

The fixture is a real git repository, because the gate enumerates the tree with
`git ls-files`: what is tracked is what can be pushed. It is sealed with
fixture_env.sealed_env so that a hook's GIT_DIR cannot point either the fixture
or the gate at a real repository.

Exit codes:
    0  every case flipped as expected
    1  one did not
"""
from __future__ import annotations
import pathlib, re, shutil, subprocess, sys, tempfile, tomllib

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
GUARD = "scripts/ci/license_boundary.py"
KAART = "docs/licensing/boundary.toml"
GIT = shutil.which("git") or "git"

sys.path.insert(0, str(HERE))
from fixture_env import sealed_env  # noqa: E402

# FLOOR: cases >= 14 -- this file is the specification of what the boundary
# refuses, and a shortened list is a quietly narrowed boundary.
FLOOR_CASES = 14

fails: list[str] = []
ran = 0


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'} {what}")
    if not ok:
        fails.append(f"{what}: {detail}")


# What the gate reads besides the manifests. Copied, never referenced in place:
# a case that mutates a manifest must not be able to reach the real one.
VASTE_BESTANDEN = [
    GUARD, KAART,
    "docs/UPSTREAM_FORKS.toml",
    "docs/LICENSE_POLICY.toml",
    "docs/release/canonical_licenses.toml",
    "LICENSE", "LICENSE-AGPL", "LICENSE-COMMERCIAL",
    "Cargo.toml",  # the virtual workspace root: no [package], must be skipped
]


def booked_manifests() -> list[str]:
    """Every manifest the real map books, so the fixture holds the same tree."""
    d = tomllib.loads((REPO / KAART).read_text(encoding="utf-8"))
    uit = [f"{r['dir']}/Cargo.toml" for r in d.get("crate", [])]
    uit += [r["manifest"] for r in d.get("package", [])]
    return uit


def git(root: pathlib.Path, *args: str) -> None:
    env = sealed_env(identity=True, cwd=root)
    r = subprocess.run([GIT, *args], cwd=root, env=env, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed in the fixture: {r.stderr}")


def bouw_basis(td: pathlib.Path) -> pathlib.Path:
    root = td / "basis"
    for rel in VASTE_BESTANDEN + booked_manifests():
        bron = REPO / rel
        if not bron.is_file():
            continue  # the unmutated run below reports it if the gate minds
        doel = root / rel
        doel.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(bron, doel)
    git(root, "init", "-q")
    git(root, "add", ".")
    return root


def geval(td: pathlib.Path, basis: pathlib.Path, naam: str,
          muteer) -> subprocess.CompletedProcess:
    """Copy the base fixture, apply `muteer(root)`, re-stage, run the gate."""
    root = td / re.sub(r"[^\w]+", "_", naam)
    shutil.copytree(basis, root)
    muteer(root)
    git(root, "add", "-A", ".")
    env = sealed_env(cwd=root)
    return subprocess.run([sys.executable, str(root / GUARD)], cwd=root, env=env,
                          capture_output=True, text=True)


def herschrijf(rel: str, oud: str, nieuw: str, count: int = 1):
    def f(root: pathlib.Path) -> None:
        p = root / rel
        t = p.read_text(encoding="utf-8")
        assert t.count(oud) == count, f"{rel}: {oud!r} occurs {t.count(oud)}x, not {count}x"
        p.write_text(t.replace(oud, nieuw), encoding="utf-8")
    return f


def rood(what: str, r: subprocess.CompletedProcess, *noemt: str) -> None:
    expect(f"{what} -> red", r.returncode == 1, f"exit={r.returncode} {r.stderr[-300:]}")
    for n in noemt:
        expect(f"{what} -> names {n}", n in r.stderr, r.stderr[-300:])


with tempfile.TemporaryDirectory(prefix="boundary-") as _td:
    td = pathlib.Path(_td)
    basis = bouw_basis(td)

    # --- 0. the copy of the real tree passes, so every red below is the mutation
    r = geval(td, basis, "unmutated", lambda root: None)
    expect("the unmutated fixture passes", r.returncode == 0, r.stderr[-400:] or r.stdout)

    # --- 1. the mutation #295 was opened about: one `publish = false` removed
    # from a licence-less workspace member outside crates/*/. Both licence gates
    # stayed green on master.
    r = geval(td, basis, "snippet-extract publishable",
              herschrijf("tools/pdfluent-snippet-extract/Cargo.toml", "publish = false\n", ""))
    rood("tools/pdfluent-snippet-extract loses publish = false", r, "pdfluent-snippet-extract")

    # --- 2. the same for a fuzz crate that lives inside a fork's directory
    r = geval(td, basis, "fuzz publishable",
              herschrijf("crates/lopdf/fuzz/Cargo.toml", "publish = false\n", ""))
    rood("crates/lopdf/fuzz loses publish = false", r, "lopdf-fuzz")

    # --- 3. the example changes the licence it was booked with
    r = geval(td, basis, "example relicensed",
              herschrijf("pdfluent-examples/rust/Cargo.toml",
                         'license = "MIT OR Apache-2.0"', 'license = "MIT"'))
    rood("pdfluent-examples/rust declares something other than recorded", r,
         "pdfluent-example-golden-path", "MIT OR Apache-2.0")

    # --- 4..7. the four binding manifests, one per registry
    r = geval(td, basis, "pom MIT",
              herschrijf("bindings/java/pom.xml",
                         "<name>AGPL-3.0-only OR LicenseRef-PDFluent-Commercial</name>",
                         "<name>MIT</name>"))
    rood("bindings/java/pom.xml declares MIT", r, "bindings/java/pom.xml")

    r = geval(td, basis, "pyproject no licence",
              herschrijf("crates/pdf-python/pyproject.toml",
                         'license = { text = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial" }\n', ""))
    rood("crates/pdf-python/pyproject.toml declares no licence", r,
         "crates/pdf-python/pyproject.toml", "nothing")

    r = geval(td, basis, "npm MIT",
              herschrijf("crates/pdf-node/package.json",
                         '"license": "SEE LICENSE IN LICENSE"', '"license": "MIT"'))
    rood("crates/pdf-node/package.json declares MIT", r, "crates/pdf-node/package.json")

    r = geval(td, basis, "csproj expression",
              herschrijf("bindings/dotnet/src/PDFluent/PDFluent.csproj",
                         "<PackageLicenseFile>LICENSE</PackageLicenseFile>",
                         "<PackageLicenseExpression>MIT</PackageLicenseExpression>"))
    rood("PDFluent.csproj swaps the licence file for MIT", r,
         "bindings/dotnet/src/PDFluent/PDFluent.csproj")

    # --- 8. an internal package that becomes publishable: the deprecated pom
    # gets somewhere to deploy to and its deploy skip goes, and it still says MIT
    def legacy_pom_deployable(root: pathlib.Path) -> None:
        herschrijf("crates/pdf-java/pom.xml", "<skip>true</skip>", "<skip>false</skip>")(root)
        herschrijf("crates/pdf-java/pom.xml", "</project>",
                   "<distributionManagement><repository><id>x</id>"
                   "<url>https://repo.invalid/</url></repository></distributionManagement>"
                   "</project>")(root)
    r = geval(td, basis, "legacy pom deployable", legacy_pom_deployable)
    rood("crates/pdf-java/pom.xml gains a deploy target and loses its skip", r,
         "crates/pdf-java/pom.xml")

    # --- 8b. the skip wins over a deploy target: this is the shape the file
    # itself documents, so it must stay green
    r = geval(td, basis, "legacy pom target but skipped",
              herschrijf("crates/pdf-java/pom.xml", "</project>",
                         "<distributionManagement><repository><id>x</id>"
                         "<url>https://repo.invalid/</url></repository>"
                         "</distributionManagement></project>"))
    expect("crates/pdf-java/pom.xml with a deploy target but skip=true stays green",
           r.returncode == 0, r.stderr[-300:])

    # --- 9. an example executable gets a PackageId, which is all `dotnet pack`
    # needs to publish it under a name
    r = geval(td, basis, "example csproj packable",
              herschrijf("pdfluent-examples/dotnet/StrictApi/StrictApi.csproj",
                         "<OutputType>Exe</OutputType>",
                         "<OutputType>Exe</OutputType><PackageId>PDFluent.Example</PackageId>"))
    rood("StrictApi.csproj gains a PackageId", r,
         "pdfluent-examples/dotnet/StrictApi/StrictApi.csproj")

    # --- 10..12. a manifest nobody classified, in each of three ecosystems
    def nieuwe_crate(root: pathlib.Path) -> None:
        d = root / "tools" / "zzz-probe"
        d.mkdir(parents=True)
        (d / "Cargo.toml").write_text('[package]\nname = "zzz-probe"\nversion = "0.0.1"\n')
    r = geval(td, basis, "unbooked crate", nieuwe_crate)
    rood("a new Cargo.toml outside crates/*/ that no row books", r, "tools/zzz-probe")

    def nieuw_npm(root: pathlib.Path) -> None:
        d = root / "tools" / "zzz-web"
        d.mkdir(parents=True)
        (d / "package.json").write_text('{"name": "zzz-web", "version": "0.0.1"}\n')
    r = geval(td, basis, "unbooked npm", nieuw_npm)
    rood("a new package.json that no row books", r, "tools/zzz-web/package.json")

    def nieuw_csproj(root: pathlib.Path) -> None:
        d = root / "tools" / "zzz-net"
        d.mkdir(parents=True)
        (d / "Zzz.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>'
                                      '<TargetFramework>net8.0</TargetFramework>'
                                      '</PropertyGroup></Project>\n')
    r = geval(td, basis, "unbooked csproj", nieuw_csproj)
    rood("a new .csproj that no row books", r, "tools/zzz-net/Zzz.csproj")

    # --- 13. a booked manifest disappears: the row outlives the file
    r = geval(td, basis, "booked manifest gone",
              lambda root: (root / "fuzz" / "Cargo.toml").unlink())
    rood("fuzz/Cargo.toml is deleted while still booked", r, "xfa-fuzz")

    # --- 14..15. the floors: a truncated map is FATAL before anything is judged
    def zonder_pakketten(root: pathlib.Path) -> None:
        p = root / KAART
        t = p.read_text(encoding="utf-8")
        # The header mentions `[[package]]` in prose; cut at the first ROW.
        i = t.index("\n[[package]]\n")
        p.write_text(t[:i], encoding="utf-8")
    r = geval(td, basis, "package rows gone", zonder_pakketten)
    rood("every [[package]] row removed", r, "floor")

    def helft_van_de_crates(root: pathlib.Path) -> None:
        # Crate rows only; the package rows stay, so the package floor cannot
        # be the one that fires.
        p = root / KAART
        t = p.read_text(encoding="utf-8")
        i = t.index("\n[[package]]\n")
        crates, pakketten = t[:i], t[i:]
        blokken = crates.split("\n[[crate]]\n")
        p.write_text("\n[[crate]]\n".join(blokken[: len(blokken) // 2]) + pakketten,
                     encoding="utf-8")
    r = geval(td, basis, "crate rows halved", helft_van_de_crates)
    rood("half the [[crate]] rows removed", r, "floor")

    # --- 16. a side the gate does not know is a row that means nothing
    r = geval(td, basis, "unknown side",
              herschrijf(KAART, 'manifest = "crates/pdf-desktop/package.json"\nside = "internal"',
                         'manifest = "crates/pdf-desktop/package.json"\nside = "whatever"'))
    rood("a [[package]] row with an unknown side", r, "crates/pdf-desktop/package.json")

    # --- 17. an unreadable manifest is a finding, not a skip
    r = geval(td, basis, "broken json",
              lambda root: (root / "crates/pdf-desktop/package.json").write_text("{ not json\n"))
    rood("a package.json that does not parse", r, "crates/pdf-desktop/package.json")

    # --- 18. the policy calls a manifest ours; the map must agree
    r = geval(td, basis, "policy disagrees",
              herschrijf(KAART, 'manifest = "crates/pdf-node/package.json"\nside = "ours"',
                         'manifest = "crates/pdf-node/package.json"\nside = "internal"'))
    rood("pdf-node booked internal while the policy lists it as ours", r,
         "own_packages", "crates/pdf-node/package.json")

# The real tree, in place: the gate is green on master, and this file has to
# know that too -- a suite that only ever sees fixtures cannot tell a repository
# regression from a fixture that drifted.
r = subprocess.run([sys.executable, str(REPO / GUARD)], cwd=REPO,
                   env=sealed_env(), capture_output=True, text=True)
expect("the real repository passes", r.returncode == 0, r.stderr[-400:])

if ran < FLOOR_CASES:
    print(f"FLOOR: {ran} case(s) ran, expected >= {FLOOR_CASES}. The specification "
          "shrank; that is a change to the boundary, not a green.", file=sys.stderr)
    sys.exit(1)
if fails:
    print(f"\n{len(fails)} of {ran} case(s) failed:", file=sys.stderr)
    for f in fails:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)
print(f"[test_license_boundary] {ran} case(s); the boundary goes red where it must")
