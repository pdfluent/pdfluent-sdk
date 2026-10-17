#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The four ways `license_gate.py` must fail, on fixtures rather than on us.

Testing a gate by breaking the real lockfile proves it once and leaves the
repository in a state someone has to remember to undo. Everything here runs
against strings and throwaway directories.

The judgements below are the ones that decide whether a copyleft dependency
gets in, so each is pinned rather than left to a reading of the code:

  * a disjunction needs one acceptable option, a conjunction needs all,
  * an unclassified licence is refused and reported as unclassified, which is a
    different instruction to the reader than "forbidden",
  * MPL passes for the editor and fails for the SDK, from the same policy,
  * an unreadable manifest fails; skipping it would turn a broken scanner into
    a green tick.

Exit codes:
    0  every judgement holds
    1  one does not
"""

from __future__ import annotations

# FLOOR: cases >= 16 -- this file is the specification of the gate's judgement,
# and a shortened list is a quietly widened policy.
import importlib.util
import copy
import json
import os
import sys
import tempfile
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Load the gate from source, never from a cached .pyc.
#
# This file is used to check the gate by breaking it on purpose and watching the
# test go red, then restoring the source. On 01-09-2026 a restore looked like it
# had failed: the source was byte-identical to the backup and the test still
# reported the mutation. Python was serving
# scripts/ci/__pycache__/license_gate.cpython-311.pyc, written while the file was
# mutated, because spec_from_file_location honours the bytecode cache.
#
# The direction it happened in was harmless -- a restored gate looking broken.
# The other direction is not: a mutation applied and then *not* recompiled would
# leave the working module running, the test would pass, and the result would
# read as "the gate caught it" when nothing was checked at all. That is the exact
# class of false green this whole test exists to prevent, sitting in the harness
# rather than in the thing under test.
# `dont_write_bytecode` only stops WRITING. A cache that is already there and
# timestamp-valid is still READ, and mtime has one-second granularity on some
# filesystems -- so a same-length mutation and restore inside one second can
# execute the cached mutated gate and report that the test caught something it
# never ran. That is the false green this block exists to prevent, one layer
# deeper than the fix I first wrote. Codex found it; my version was incomplete.
#
# Compiling the source text directly cannot consult a cache at all.
sys.dont_write_bytecode = True
importlib.invalidate_caches()
_bron = (HERE / "license_gate.py").read_text(encoding="utf-8")
lg = types.ModuleType("lg")
lg.__file__ = str(HERE / "license_gate.py")
exec(compile(_bron, lg.__file__, "exec"), lg.__dict__)

OK = {"MIT", "Apache-2.0", "BSD-3-Clause", "LicenseRef-PDFluent-Commercial"}
ZWAK = {"MPL-2.0", "LGPL-2.1-or-later"}
VERBODEN = {"GPL-3.0-only", "AGPL-3.0-only", "AGPL-3.0-or-later", "SSPL-1.0"}

FLOOR_CASES = 16

# (expression, weak-copyleft permitted here, expected verdict, what it pins)
CASES: list[tuple[str, set, bool, str]] = [
    ("MIT", set(), True, "a plain allowed licence"),
    ("Apache-2.0", set(), True, "another"),
    ("GPL-3.0-only", set(), False, "a forbidden licence, alone"),
    ("AGPL-3.0-or-later", set(), False, "the one that would end the commercial offer"),
    ("MIT OR Apache-2.0", set(), True, "a disjunction where both are fine"),
    ("MIT OR GPL-3.0-only", set(), True, "a disjunction: one good option is enough"),
    ("GPL-3.0-only OR SSPL-1.0", set(), False, "a disjunction with no good option"),
    ("MIT/Apache-2.0", set(), True, "the older slash spelling of a disjunction"),
    ("Apache-2.0 AND MIT", set(), True, "a conjunction where both are fine"),
    ("MIT AND GPL-3.0-only", set(), False, "a conjunction: one bad part poisons it"),
    ("WTFPL", set(), False, "an unclassified licence is refused"),
    ("", set(), False, "an empty expression is refused, not waved through"),
    # The same expression, two surfaces, one policy.
    ("MPL-2.0", {"MPL-2.0"}, True, "MPL where the surface permits it (editor)"),
    ("MPL-2.0", set(), False, "MPL where it does not (SDK)"),
    ("LGPL-2.1-or-later", set(), False, "LGPL is classified and permitted nowhere"),
    ("MIT OR Apache-2.0 OR LGPL-2.1-or-later", set(), True,
     "JNA's shape: an LGPL option we decline, next to one we take"),
    ("Apache-2.0 WITH LLVM-exception", set(), False,
     "a WITH-exception not in this test's allow set stays unclassified"),
]


def zin(expr: str, zwak_ok: set) -> bool:
    goed, _ = lg.toegestaan(expr, OK, ZWAK, VERBODEN, zwak_ok)
    return goed


def reden(expr: str, zwak_ok: set) -> str:
    return lg.toegestaan(expr, OK, ZWAK, VERBODEN, zwak_ok)[1]


def main() -> int:
    if len(CASES) < FLOOR_CASES:  # FLOOR
        print(f"[test_license_gate] FATAL: {len(CASES)} cases, floor is {FLOOR_CASES}. "
              f"A shortened list is a quietly widened policy.", file=sys.stderr)
        return 1

    fouten = []
    for expr, zwak_ok, verwacht, wat in CASES:
        got = zin(expr, zwak_ok)
        if got != verwacht:
            fouten.append(f"{expr!r} (weak={sorted(zwak_ok)}): expected {verwacht}, got {got} — {wat}")

    # Unclassified must say so, not merely fail: the reader has to know whether
    # to look a licence up or to take it out.
    if "not classified" not in reden("WTFPL", set()):
        fouten.append("an unclassified licence must be reported as unclassified")
    if "forbidden" not in reden("GPL-3.0-only", set()):
        fouten.append("a forbidden licence must be reported as forbidden")

    # An unreadable manifest fails; it is never skipped.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text("{ this is not json")
        echt, lg.REPO = lg.REPO, bed
        try:
            lg.scan_npm({})
            fouten.append("a package.json that will not parse was skipped instead of failing")
        except lg.Onleesbaar:
            pass
        finally:
            lg.REPO = echt

    # A manifest that parses but declares a dependency we have not classified.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text(json.dumps(
            {"name": "x", "license": "MIT", "dependencies": {"left-pad": "^1.0.0"}}))
        # The scanner reads two manifests; a fixture with one makes it raise
        # Onleesbaar and the case below would never run.
        (bed / "crates/xfa-wasm").mkdir(parents=True)
        (bed / "crates/xfa-wasm/Cargo.toml").write_text('[package]\nname = "xfa-wasm"\n')
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_npm({"own_packages":
                                     {"crates/xfa-wasm/Cargo.toml": "MIT"}})
        finally:
            lg.REPO = echt
        if not any("left-pad" in w for w, _ in gevonden):
            fouten.append("a declared npm runtime dependency was not reported at all")
        elif all(zin(e, set()) for _, e in gevonden):
            fouten.append("an npm dependency whose licence was never read counted as allowed")

    # A manifest that is simply absent must fail too. This is the case CI found
    # and the local run did not: pkg/package.json is generated and gitignored,
    # so it exists on a developer machine and nowhere else. The scanner used to
    # skip a missing file, which made it see two manifests here and one there.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text(json.dumps({"license": "MIT"}))
        echt, lg.REPO = lg.REPO, bed
        try:
            lg.scan_npm({})
            fouten.append("a missing second manifest was skipped instead of failing")
        except lg.Onleesbaar:
            pass
        finally:
            lg.REPO = echt

    # A toolchain that is absent must fail as a verdict, not as a traceback.
    # The GitHub runner has no cargo, so scan_cargo raised FileNotFoundError
    # straight out of subprocess: the step went red with a stack trace and no
    # statement about licences at all. Exit 1 for "a crate is GPL" and exit 1
    # for "nothing was read" are not the same result, and the message is the
    # only thing that distinguishes them.
    # Genuinely absent means absent everywhere the gate looks, not just off
    # PATH: _cargo() also checks $CARGO_HOME/bin and ~/.cargo/bin, because the
    # runner has rustup installed without putting it on PATH. Stripping PATH
    # alone made this case pass by finding the developer's own cargo, and the
    # test then reported the opposite of what it meant to.
    bewaard = {k: os.environ.get(k) for k in ("PATH", "CARGO_HOME", "HOME")}
    with tempfile.TemporaryDirectory() as leeg:
        os.environ["PATH"] = leeg
        os.environ["CARGO_HOME"] = leeg
        os.environ["HOME"] = leeg
        try:
            lg.scan_cargo({})
            fouten.append("a missing cargo was not reported at all")
        except lg.Onleesbaar as e:
            if "SKIPPED (not a pass)" not in str(e):
                fouten.append("a missing cargo failed without announcing itself "
                              "as a skip rather than a licence verdict")
        except FileNotFoundError:
            fouten.append("a missing cargo escaped as a traceback instead of a verdict")
        finally:
            for k, v in bewaard.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    # Every [channel_representation] entry is validated, INCLUDING the one whose
    # manifest is not in the repository.
    #
    # `weergave_klopt` was reachable only through `eigen_verklaring`, which runs
    # per manifest read from disk, so the entry for the gitignored, wasm-pack
    # generated `crates/xfa-wasm/pkg/package.json` was never checked at all.
    # Measured 02-09-2026 by mutation: `canonical = "MIT"` on that one entry
    # passed all five licence gates, while the same mutation on either of the
    # other two turned license_gate red. The escape had been closed for the
    # entries that happen to be tracked.
    echte = lg.policy()
    if lg.weergaven_kloppen(echte):
        fouten.append("the policy as committed does not satisfy its own "
                      "representation check")
    for veld, wat in (("canonical", "MIT"), ("reason", ""), ("file", "no/such/file")):
        kapot = copy.deepcopy(echte)
        kapot["channel_representation"]["crates/xfa-wasm/pkg/package.json"][veld] = wat
        if not lg.weergaven_kloppen(kapot):
            fouten.append(f"a channel_representation entry with {veld}={wat!r} passed, "
                          "and that entry's manifest is the one absent from the repo")
    leeg = copy.deepcopy(echte)
    leeg["channel_representation"] = dict(
        list(echte["channel_representation"].items())[:lg.WEERGAVE_VLOER - 1])
    if not lg.weergaven_kloppen(leeg):
        fouten.append("dropping a channel_representation entry passed; the table "
                      "can be emptied and the comparison disappears with it")

    # THE PUBLISH PATH MAY DELEGATE, BUT ONLY TO THE TRANSFORM (#541). wasm.yml
    # no longer assigns the licence itself -- it calls transform-wasm-pkg.sh,
    # which the same loop reads. A workflow that stopped assigning AND does not
    # call the transform is the case that must still be UNKNOWN, because then
    # nobody sets it.
    uit = lg.scan_npm(echte)
    if any("wasm.yml" in naam for naam, _ in uit):
        fouten.append("wasm.yml is reported although it delegates to the transform, "
                      "which the next entry in that loop reads")
    stroom = (lg.REPO / ".github/workflows/wasm.yml").read_text()
    if "transform-wasm-pkg.sh" not in stroom:
        fouten.append("wasm.yml no longer calls the transform, so the delegation this "
                      "check allows is not the one that is happening")

    def echt_zin(expr: str) -> bool:
        """The verdict under the committed policy on the SDK surface.

        The module-level OK set above is the test's own mirror for the SPDX
        expression cases; the rows below carry licences like our own dual
        offer, which only the real allow list knows.
        """
        goed, _ = lg.toegestaan(
            expr,
            set(echte["licenses"]["allowed"]),
            set(echte["licenses"]["weak_copyleft"]),
            set(echte["licenses"]["forbidden"]),
            set(echte["surfaces"]["sdk"].get("weak_copyleft", [])))
        return goed

    # AN EXTRA UNDER AN UNKNOWN LICENCE IS RED. scan_python used to read only
    # [project].dependencies, so an extra under any licence passed; every
    # [project.optional-dependencies] entry is now a row of class "extra",
    # judged by the same policy. Without that row this case fails, which is
    # the hole re-opened.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-python").mkdir(parents=True)
        (bed / "crates/pdf-python/pyproject.toml").write_text(
            "[project]\n"
            'name = "pdfluent"\n'
            'license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"\n'
            "[project.optional-dependencies]\n"
            'mine = ["definitely-not-an-installed-distribution-xyz"]\n')
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_python(echte)
        finally:
            lg.REPO = echt
        rij = [(w, e) for w, e in gevonden if "[extra mine]" in w]
        if not rij:
            fouten.append("an [project.optional-dependencies] entry produced no row at all")
        elif all(echt_zin(e) for _, e in rij):
            fouten.append("an extra whose distribution is not installed counted as allowed")

    # Without the resolvers the two cases below would crash on a missing
    # attribute instead of failing on a missing row. Name the absence once so
    # a reverted gate fails clean -- and skip only the cases that touch it.
    ontbreekt = [s for s in ("pypi_licentie", "distributie_naam", "npm_licentie",
                             "importlib_metadata") if not hasattr(lg, s)]
    if ontbreekt:
        fouten.append("the extras resolvers are gone from license_gate.py: "
                      + ", ".join(ontbreekt))

    # THE SAME ROW UNDER AN ALLOWED LICENCE IS GREEN. The resolver is the
    # installed distribution's own metadata; here it is stubbed at the store
    # boundary, so the case does not depend on what this machine installed.
    if not ontbreekt:
        with tempfile.TemporaryDirectory() as tmp:
            bed = Path(tmp)
            (bed / "crates/pdf-python").mkdir(parents=True)
            (bed / "crates/pdf-python/pyproject.toml").write_text(
                "[project]\n"
                'name = "pdfluent"\n'
                'license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"\n'
                "[project.optional-dependencies]\n"
                'vriend = ["some-allowed-thing"]\n')

            class _Winkel:
                class PackageNotFoundError(Exception):
                    pass

                @staticmethod
                def metadata(_naam):
                    return {"License-Expression": "MIT"}

            bewaard, lg.importlib_metadata = lg.importlib_metadata, _Winkel
            echt, lg.REPO = lg.REPO, bed
            try:
                gevonden = lg.scan_python(echte)
            finally:
                lg.REPO = echt
                lg.importlib_metadata = bewaard
            rij = [(w, e) for w, e in gevonden if "[extra vriend]" in w]
            if not rij:
                fouten.append("an extra under an allowed licence produced no row at all")
            elif not all(echt_zin(e) for _, e in rij):
                fouten.append("an extra under an allowed licence did not judge green")

    # THE RESOLVER READS WHAT THE METADATA STATES. License-Expression first,
    # then the License field; a field carrying the full text states no
    # identifier and an absent distribution states nothing at all. The store
    # is stubbed, the parsing is real.
    class _Pakhuis:
        class PackageNotFoundError(Exception):
            pass

        VOORRAAD = {
            "met-spdx": {"License-Expression": "MIT"},
            "met-veld": {"License": "Apache-2.0"},
            "met-tekst": {"License": "Copyright (c) 2005-2023, NumPy Developers.\n"
                                     "All rights reserved."},
            "met-lang": {"License": "x" * 121},
            "zonder": {},
        }

        @classmethod
        def metadata(cls, naam):
            if naam not in cls.VOORRAAD:
                raise cls.PackageNotFoundError(naam)
            return cls.VOORRAAD[naam]

    if not ontbreekt:
        bewaard, lg.importlib_metadata = lg.importlib_metadata, _Pakhuis
        try:
            if lg.pypi_licentie("met-spdx") != "MIT":
                fouten.append("the resolver did not read License-Expression")
            if lg.pypi_licentie("met-veld") != "Apache-2.0":
                fouten.append("the resolver did not fall back to the License field")
            if not lg.pypi_licentie("met-tekst").startswith("UNKNOWN"):
                fouten.append("a full licence text counted as an identifier")
            if not lg.pypi_licentie("met-lang").startswith("UNKNOWN"):
                fouten.append("a licence field carrying prose counted as an identifier")
            if not lg.pypi_licentie("zonder").startswith("UNKNOWN"):
                fouten.append("a distribution declaring no licence counted as allowed")
            if not lg.pypi_licentie("niet-hier").startswith("UNKNOWN"):
                fouten.append("a distribution that is not installed counted as allowed")
            if not echt_zin(lg.pypi_licentie("met-spdx")):
                fouten.append("a resolved allowed licence did not judge green")
            if lg.distributie_naam("pytest>=7.0") != "pytest":
                fouten.append("a version constraint leaked into the distribution name")
            if lg.distributie_naam("requests[security]>=2.8") != "requests":
                fouten.append("an extras clause leaked into the distribution name")
            if lg.distributie_naam("x; python_version > '3.8'") != "x":
                fouten.append("a marker leaked into the distribution name")
        finally:
            lg.importlib_metadata = bewaard

    # NPM OPTIONAL AND PEER DEPENDENCIES ARE ROWS. scan_npm used to read only
    # "dependencies"; the seven platform packages sat in optionalDependencies
    # and passed unread. Unknown stays red, first-party stays green, and the
    # dev table stays out -- the table name is what tells it apart.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text(json.dumps(
            {"name": "x", "license": "MIT",
             "optionalDependencies": {"mystery-opt": "^1.0.0",
                                      "@pdfluent/node-test-xyz": "1.1.0"},
             "peerDependencies": {"mystery-peer": "^2.0.0"},
             "devDependencies": {"jest": "^30.0.0"}}))
        (bed / "crates/xfa-wasm").mkdir(parents=True)
        (bed / "crates/xfa-wasm/Cargo.toml").write_text('[package]\nname = "xfa-wasm"\n')
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_npm({"own_packages":
                                    {"crates/xfa-wasm/Cargo.toml": "MIT"}})
        finally:
            lg.REPO = echt
        opt = [(w, e) for w, e in gevonden if "mystery-opt" in w]
        peer = [(w, e) for w, e in gevonden if "mystery-peer" in w]
        eigen = [(w, e) for w, e in gevonden if "node-test-xyz" in w]
        if not opt or "[optional]" not in opt[0][0]:
            fouten.append("an npm optionalDependency produced no [optional] row")
        elif all(echt_zin(e) for _, e in opt):
            fouten.append("an npm optionalDependency whose licence was never read "
                          "counted as allowed")
        if not peer or "[peer]" not in peer[0][0]:
            fouten.append("an npm peerDependency produced no [peer] row")
        elif all(echt_zin(e) for _, e in peer):
            fouten.append("an npm peerDependency whose licence was never read "
                          "counted as allowed")
        if not eigen:
            fouten.append("a first-party optionalDependency produced no row at all")
        elif not all(echt_zin(e) for _, e in eigen):
            fouten.append("a first-party optionalDependency did not judge green")
        if any("jest" in w for w, _ in gevonden):
            fouten.append("a devDependency reached the rows; the dev table must stay out")

    # MAVEN OPTIONAL AND PROVIDED DEPENDENCIES CARRY THEIR CLASS. The scanner
    # saw every non-test dependency but labelled none of them; an
    # `<optional>true</optional>` travelled without its class. Unknown stays
    # red, a recorded election still greens, and `<scope>test</scope>` stays
    # out -- the scope is what tells it apart.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "bindings/java").mkdir(parents=True)
        (bed / "bindings/java/pom.xml").write_text(
            '<project xmlns="http://maven.apache.org/POM/4.0.0">'
            "<modelVersion>4.0.0</modelVersion><dependencies>"
            '<dependency><groupId>com.example</groupId><artifactId>mystery</artifactId>'
            "<version>1.0</version><optional>true</optional></dependency>"
            '<dependency><groupId>com.example</groupId><artifactId>provided-thing</artifactId>'
            "<version>1.0</version><scope>provided</scope></dependency>"
            '<dependency><groupId>net.java.dev.jna</groupId><artifactId>jna</artifactId>'
            "<version>5.14.0</version><optional>true</optional></dependency>"
            '<dependency><groupId>org.junit</groupId><artifactId>jug</artifactId>'
            "<version>1.0</version><scope>test</scope></dependency>"
            "</dependencies></project>")
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_maven(echte)
        finally:
            lg.REPO = echt
        opt = [(w, e) for w, e in gevonden if "com.example:mystery" in w]
        provided = [(w, e) for w, e in gevonden if "provided-thing" in w]
        gekozen = [(w, e) for w, e in gevonden if "jna" in w]
        if not opt or "[optional]" not in opt[0][0]:
            fouten.append("an <optional>true</optional> dependency produced no [optional] row")
        elif all(echt_zin(e) for _, e in opt):
            fouten.append("an optional dependency with no election recorded counted as allowed")
        if not provided or "[provided]" not in provided[0][0]:
            fouten.append("a <scope>provided</scope> dependency produced no [provided] row")
        elif all(echt_zin(e) for _, e in provided):
            fouten.append("a provided dependency with no election recorded counted as allowed")
        if not gekozen or "[optional]" not in gekozen[0][0]:
            fouten.append("an elected optional dependency produced no [optional] row")
        elif not all(echt_zin(e) for _, e in gekozen):
            fouten.append("an elected optional dependency did not judge green")
        if any("org.junit:jug" in w for w, _ in gevonden):
            fouten.append("a test-scoped dependency reached the rows; test scope must stay out")

    # NUGET PRIVATE REFERENCES ARE ROWS OF CLASS "private". A PackageReference
    # carrying PrivateAssets or IncludeAssets still builds what ships, so it
    # meets the same policy; the class only says why it is there. Attribute
    # and child-element spellings both count. Test projects stay out by the
    # src-only glob, which never reaches bindings/dotnet/test.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "bindings/dotnet/src/Fake").mkdir(parents=True)
        (bed / "bindings/dotnet/src/Fake/Fake.csproj").write_text(
            '<Project Sdk="Microsoft.NET.Sdk"><ItemGroup>'
            '<PackageReference Include="Mystery.Pkg" Version="1.0.0">'
            "<PrivateAssets>all</PrivateAssets></PackageReference>"
            '<PackageReference Include="Flow.Pkg" Version="2.0.0" />'
            '<PackageReference Include="Build.Only" Version="3.0.0" PrivateAssets="all" />'
            "</ItemGroup></Project>")
        (bed / "bindings/dotnet/test/Fake.Tests").mkdir(parents=True)
        (bed / "bindings/dotnet/test/Fake.Tests/Fake.Tests.csproj").write_text(
            '<Project Sdk="Microsoft.NET.Sdk"><ItemGroup>'
            '<PackageReference Include="Test.Only" Version="1.0.0" />'
            "</ItemGroup></Project>")
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_nuget({})
        finally:
            lg.REPO = echt
        privaat = [(w, e) for w, e in gevonden if "Mystery.Pkg" in w]
        attribuut = [(w, e) for w, e in gevonden if "Build.Only" in w]
        gewoon = [(w, e) for w, e in gevonden if "Flow.Pkg" in w]
        if not privaat or "[private]" not in privaat[0][0]:
            fouten.append("a PrivateAssets child element produced no [private] row")
        elif all(echt_zin(e) for _, e in privaat):
            fouten.append("a private NuGet reference whose licence was never read "
                          "counted as allowed")
        if not attribuut or "[private]" not in attribuut[0][0]:
            fouten.append('a PrivateAssets="all" attribute produced no [private] row')
        if not gewoon or "[private]" in gewoon[0][0]:
            fouten.append("a plain PackageReference went missing or was mislabelled [private]")
        if any("Test.Only" in w for w, _ in gevonden):
            fouten.append("a test-project reference reached the rows; test projects must stay out")
        # A NuGet reference carries no licence and the policy holds no
        # per-package table, so no private row can be green end to end today;
        # what is pinned is that it meets the same judge: allowed greens it.
        if not echt_zin("MIT"):
            fouten.append("class private: an allowed licence did not judge green")

    # THE TABLE DECIDES, THE MACHINE ONLY WARNS. Extras/optional/peer/private
    # rows take their SPDX from scripts/ci/extra_licences.toml; installed
    # metadata disagreeing with it is a WARNING line, never the verdict. A
    # package missing from the table is red with the add-hint. Without the
    # table the rows below go red or silent, which is the machine-dependent
    # gate re-opened.
    if not hasattr(lg, "extra_licentie") or not hasattr(lg, "WAARSCHUWINGEN"):
        fouten.append("the extras table resolvers are gone from license_gate.py")
    else:
        def _schrijf_tabel(bed, regels):
            t = bed / "scripts" / "ci"
            t.mkdir(parents=True)
            (t / "extra_licences.toml").write_text(regels)

        # TABLE HIT: verdict from the table although installed disagrees.
        with tempfile.TemporaryDirectory() as tmp:
            bed = Path(tmp)
            (bed / "crates/pdf-python").mkdir(parents=True)
            (bed / "crates/pdf-python/pyproject.toml").write_text(
                "[project]\n"
                'name = "pdfluent"\n'
                'license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"\n'
                "[project.optional-dependencies]\n"
                'vriend = ["hit-pkg"]\n')
            _schrijf_tabel(bed,
                '[[extra]]\npackage = "hit-pkg"\necosystem = "pypi"\n'
                'version = "1"\nspdx = "MIT"\n'
                'source = "fixture"\nchecked = "2026-10-11"\n')

            class _Oneens:
                class PackageNotFoundError(Exception):
                    pass

                @staticmethod
                def metadata(_naam):
                    return {"License-Expression": "GPL-3.0-only"}

            bewaard, lg.importlib_metadata = lg.importlib_metadata, _Oneens
            echt, lg.REPO = lg.REPO, bed
            lg.WAARSCHUWINGEN.clear()
            try:
                gevonden = lg.scan_python(echte)
            finally:
                lg.REPO = echt
                lg.importlib_metadata = bewaard
            rij = [(w, e) for w, e in gevonden if "[extra vriend]" in w]
            if not rij:
                fouten.append("table hit produced no extra row at all")
            elif not all(echt_zin(e) for _, e in rij):
                fouten.append("table hit (MIT) did not judge green although "
                              "installed says GPL; the verdict must come from the table")
            if not any("hit-pkg" in w for w in lg.WAARSCHUWINGEN):
                fouten.append("installed metadata disagreeing with the table "
                              "produced no warning")

        # TABLE MISS: red with the add-hint, never the machine's answer.
        with tempfile.TemporaryDirectory() as tmp:
            bed = Path(tmp)
            (bed / "crates/pdf-python").mkdir(parents=True)
            (bed / "crates/pdf-python/pyproject.toml").write_text(
                "[project]\n"
                'name = "pdfluent"\n'
                'license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"\n'
                "[project.optional-dependencies]\n"
                'vreemd = ["miss-pkg"]\n')
            _schrijf_tabel(bed, "# an empty table\n")

            class _Meegaand:
                class PackageNotFoundError(Exception):
                    pass

                @staticmethod
                def metadata(_naam):
                    return {"License-Expression": "MIT"}

            bewaard, lg.importlib_metadata = lg.importlib_metadata, _Meegaand
            echt, lg.REPO = lg.REPO, bed
            try:
                gevonden = lg.scan_python(echte)
            finally:
                lg.REPO = echt
                lg.importlib_metadata = bewaard
            rij = [(w, e) for w, e in gevonden if "[extra vreemd]" in w]
            if not rij:
                fouten.append("table miss produced no extra row at all")
            elif all(echt_zin(e) for _, e in rij):
                fouten.append("table miss judged green on installed MIT; it must "
                              "be red with the add-hint")
            elif not any("add miss-pkg to scripts/ci/extra_licences.toml" in e
                         for _, e in rij):
                fouten.append("table miss did not name the add-hint for the package")

        # THE FRONT DOORS ARE SCANNED, on the real tree they publish from.
        lg.WAARSCHUWINGEN.clear()
        py_gevonden = lg.scan_python(echte)
        if not any("pdfluent-extract" in w or "langchain-pdfluent" in w
                   or "llama-index-readers-pdfluent" in w for w, _ in py_gevonden):
            fouten.append("integrations/pdfluent-extract/pyproject.toml produced no rows")
        npm_gevonden = lg.scan_npm(echte)
        if not any("pdfluent-extract-js" in w for w, _ in npm_gevonden):
            fouten.append("integrations/pdfluent-extract-js/package.json produced no rows")

    # PILLOW (D34) IS A SCOPED EXCEPTION: package AND range AND surface.
    # MIT-CMU stays out of `allowed`; the gate accepts it only for Pillow
    # inside >=11.2.1,<13 arriving as a python extra. Three reds pin the
    # three sides of the scope, one green pins the acceptance -- and the
    # green row still carries the table's MIT-CMU statement, so the
    # exception cannot be mistaken for a reclassification.
    if not hasattr(lg, "binnen_bereik") or not hasattr(lg, "SCOPED_WAT"):
        fouten.append("the scoped-exception resolvers are gone from license_gate.py")
    else:
        for decl, want in [
            (">=11.2.1,<13", True),
            ("==12.3.0", True),
            ("~=12.1", True),
            (">=11.2.1,<13,!=12.0", True),
            (">=10", False),
            (">=11.2.1", False),
            ("<13", False),
            ("", False),
            (">=13,<14", False),
            ("~=11.2", False),
            ("*", False),
        ]:
            if lg.binnen_bereik(decl, ">=11.2.1,<13") != want:
                fouten.append(f"binnen_bereik({decl!r}) judged {not want}, "
                              f"expected {want}")

        PILLOW_TABEL = (
            '[[extra]]\npackage = "Pillow"\necosystem = "pypi"\n'
            'version = "11.2.1"\nversions = ">=11.2.1,<13"\n'
            'surface = "python-extra"\ndecision = "D34"\nspdx = "MIT-CMU"\n'
            'source = "fixture"\nchecked = "2026-10-11"\n')

        def _scan_pillow(vereisten, afhankelijkheden=(), tabel=None):
            with tempfile.TemporaryDirectory() as tmp:
                bed = Path(tmp)
                (bed / "crates/pdf-python").mkdir(parents=True)
                extra = "".join(f'"{v}", ' for v in vereisten)
                deps = "".join(f'"{v}", ' for v in afhankelijkheden)
                (bed / "crates/pdf-python/pyproject.toml").write_text(
                    "[project]\n"
                    'name = "pdfluent"\n'
                    'license = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"\n'
                    f"dependencies = [{deps}]\n"
                    "[project.optional-dependencies]\n"
                    f"pillow = [{extra}]\n")
                t = bed / "scripts" / "ci"
                t.mkdir(parents=True)
                (t / "extra_licences.toml").write_text(
                    PILLOW_TABEL if tabel is None else tabel)
                lg.SCOPED_WAT.clear()
                lg.WAARSCHUWINGEN.clear()
                echt, lg.REPO = lg.REPO, bed
                try:
                    return lg.scan_python(echte)
                finally:
                    lg.REPO = echt

        # GREEN: Pillow inside the reviewed range, as an extra.
        rij = [(w, e) for w, e in _scan_pillow(["Pillow>=11.2.1,<13"])
               if "[extra pillow]" in w]
        if not rij:
            fouten.append("Pillow>=11.2.1,<13 as an extra produced no row at all")
        elif not all(w in lg.SCOPED_WAT and "D34" in lg.SCOPED_WAT[w]
                     for w, _ in rij):
            fouten.append("Pillow inside the reviewed range as an extra was "
                          "not scoped-accepted under D34")
        elif any(e != "MIT-CMU" for _, e in rij):
            fouten.append("the scoped Pillow row no longer carries the table's "
                          "MIT-CMU statement")

        # RED: Pillow outside the reviewed range, and Pillow unpinned.
        for vereiste in ("Pillow>=13", "Pillow"):
            rij = [(w, e) for w, e in _scan_pillow([vereiste])
                   if "[extra pillow]" in w]
            if not rij:
                fouten.append(f"{vereiste} as an extra produced no row at all")
            elif any(w in lg.SCOPED_WAT for w, _ in rij):
                fouten.append(f"{vereiste} as an extra was scoped-accepted "
                              f"outside the reviewed range")
            elif all(echt_zin(e) for _, e in rij):
                fouten.append(f"{vereiste} as an extra judged green outside "
                              f"the reviewed range")
            elif not any("outside the reviewed" in e for _, e in rij):
                fouten.append(f"{vereiste} as an extra did not name the range "
                              f"it fell outside of")

        # RED: another MIT-CMU package does not ride Pillow's exception.
        tabel = (PILLOW_TABEL + '[[extra]]\npackage = "other-pkg"\n'
                 'ecosystem = "pypi"\nversion = "1"\nspdx = "MIT-CMU"\n'
                 'source = "fixture"\nchecked = "2026-10-11"\n')
        rij = [(w, e) for w, e in _scan_pillow(["other-pkg"], tabel=tabel)
               if "[extra pillow]" in w]
        if not rij:
            fouten.append("an extra under MIT-CMU produced no row at all")
        elif any(w in lg.SCOPED_WAT for w, _ in rij):
            fouten.append("a non-Pillow MIT-CMU extra was scoped-accepted")
        elif all(echt_zin(e) for _, e in rij):
            fouten.append("a non-Pillow MIT-CMU extra judged green")

        # RED: Pillow as a runtime dependency is not the reviewed surface.
        rij = [(w, e) for w, e in _scan_pillow([], ["Pillow>=11.2.1,<13"])
               if "pypi Pillow" in w and "[extra" not in w]
        if not rij:
            fouten.append("Pillow as a runtime dependency produced no row at all")
        elif any(w in lg.SCOPED_WAT for w, _ in rij):
            fouten.append("Pillow as a runtime dependency was scoped-accepted; "
                          "the exception covers the extra surface only")
        elif all(echt_zin(e) for _, e in rij):
            fouten.append("Pillow as a runtime dependency judged green")

    print(f"[test_license_gate] {len(CASES)} expression case(s) + 10 scanner case(s) + 7 representation case(s)")
    if not fouten:
        print("[test_license_gate] every judgement holds")
        return 0
    print(f"[test_license_gate] {len(fouten)} wrong:")
    for f in fouten:
        print(f"  {f}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
