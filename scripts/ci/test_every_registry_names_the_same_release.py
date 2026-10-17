#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The one-release guard refuses what it should, and only that (#358).

Each case builds a scratch tree that disagrees with itself in one specific way
-- npm left a version behind, PyPI spelled the same release the way PEP 440
requires, the .NET property renamed, a manifest deleted -- and proves the guard
says so, by name. The last block runs it against the real tree, so letting the
six manifests drift apart turns this red rather than leaving a guard that
describes a repository nobody has.
"""
from __future__ import annotations

import pathlib
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import every_registry_names_the_same_release as guard  # noqa: E402

MINIMUM_CASES = 12  # FLOOR: fewer cases ran than were written, so something skipped


def build(root: pathlib.Path, release: str = "1.0.0", **override: str) -> pathlib.Path:
    """A six-manifest tree at one release, with named fields overridden."""
    version = {
        "crate": release,
        "python": release,
        "node": release,
        "npm": release,
        "dotnet": release,
        "maven": release,
    } | override
    classifier = override.get("classifier", guard.STABLE_CLASSIFIER)

    def write(path: str, text: str) -> None:
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    write("Cargo.toml",
          f'[workspace]\nmembers = ["crates/*"]\n\n'
          f'[workspace.package]\nedition = "2021"\nversion = "{release}"\n\n'
          f'[workspace.dependencies]\nserde = "1"\n')
    write("crates/pdfluent/Cargo.toml",
          f'[package]\nname = "pdfluent"\nversion = "{version["crate"]}"\n\n'
          f'[dependencies]\nserde = "1"\n')
    write("crates/pdf-python/pyproject.toml",
          f'[build-system]\nrequires = ["maturin"]\n\n'
          f'[project]\nname = "pdfluent"\nversion = "{version["python"]}"\n'
          f'classifiers = [\n    "{classifier}",\n]\n\n'
          f'[project.urls]\nHomepage = "https://pdfluent.com"\n')
    write("crates/pdf-node/package.json",
          '{\n  "name": "@pdfluent/node",\n  "version": "%s"\n}\n' % version["node"])
    write("crates/xfa-wasm/Cargo.toml",
          f'[package]\nname = "xfa-wasm"\nversion = "{version["crate"]}"\n\n'
          f'[package.metadata.npm]\nname = "@pdfluent/sdk-wasm"\n'
          f'version = "{version["npm"]}"\n')
    write("bindings/dotnet/src/PDFluent/PDFluent.csproj",
          f"<Project>\n  <PropertyGroup>\n"
          f"    <Version>{version['dotnet']}</Version>\n"
          f"  </PropertyGroup>\n</Project>\n")
    write("bindings/java/pom.xml",
          f"<project>\n  <groupId>com.pdfluent</groupId>\n"
          f"  <artifactId>pdfluent</artifactId>\n"
          f"  <version>{version['maven']}</version>\n"
          f"  <dependencies>\n    <dependency>\n      <groupId>org.junit</groupId>\n"
          f"      <artifactId>junit</artifactId>\n      <version>5.99.0</version>\n"
          f"    </dependency>\n  </dependencies>\n</project>\n")
    return root


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    with tempfile.TemporaryDirectory() as d:
        tmp = pathlib.Path(d)

        root = build(tmp / "agree")
        found = guard.judge(root)
        case("six manifests at one release pass", found == [], str(found))

        # The drift that was live on 09-09-2026: five channels moved, npm did
        # not, and every page kept saying it shipped the product.
        root = build(tmp / "npm-behind", npm="1.0.0-beta.17.5")
        found = guard.judge(root)
        case("an npm identity left behind is caught",
             any("sdk-wasm" in x and "beta.17.5" in x for x in found), str(found))

        root = build(tmp / "node-behind", node="1.0.0-beta.17.4.1")
        found = guard.judge(root)
        case("a node package.json left behind is caught",
             any("@pdfluent/node" in x for x in found), str(found))

        root = build(tmp / "nuget-behind", dotnet="1.0.0-beta.17.3")
        found = guard.judge(root)
        case("a NuGet <Version> left behind is caught",
             any("NuGet" in x for x in found), str(found))

        root = build(tmp / "maven-behind", maven="1.0.0-beta.17.2")
        found = guard.judge(root)
        case("a Maven <version> left behind is caught",
             any("Maven" in x for x in found), str(found))

        # PEP 440 refuses `1.0.0-beta.18`, so PyPI has to spell a pre-release
        # differently. That spelling is the same release and must pass; the
        # dotted spelling on PyPI is the mistake.
        root = build(tmp / "pep440", release="1.0.0-beta.18", python="1.0.0b18")
        found = guard.judge(root)
        case("PyPI may spell a pre-release the way PEP 440 requires",
             found == [], str(found))

        root = build(tmp / "pep440-wrong", release="1.0.0-beta.18",
                     python="1.0.0-beta.18")
        found = guard.judge(root)
        case("PyPI carrying the Cargo spelling of a pre-release is caught",
             any("PyPI" in x for x in found), str(found))

        # A GA release whose PyPI page still says beta. The version is right
        # and the page is wrong, which is why the version comparison alone
        # cannot catch it.
        root = build(tmp / "stale-classifier", classifier=guard.BETA_CLASSIFIER)
        found = guard.judge(root)
        case("a GA release still classified 4 - Beta on PyPI is caught",
             any("classifies" in x for x in found), str(found))

        root = build(tmp / "prerelease-classifier", release="1.0.0-beta.18",
                     python="1.0.0b18", classifier=guard.BETA_CLASSIFIER)
        found = guard.judge(root)
        case("a pre-release may keep the beta classifier", found == [], str(found))

        # The floor. Deleting a manifest is the cheapest way to make a
        # comparison agree with itself.
        root = build(tmp / "deleted")
        (root / "bindings/java/pom.xml").unlink()
        found = guard.judge(root)
        case("a deleted manifest is a failure, not one fewer comparison",
             any("floor" in x for x in found) and any("pom.xml" in x for x in found),
             str(found))

        # A pom's own version is the first outside <parent>; a dependency's
        # version further down must not be read as the project's.
        root = build(tmp / "pom-dependency")
        found = guard.judge(root)
        case("a dependency version in the pom is not read as the project's",
             found == [], str(found))

        root = build(tmp / "unparsable")
        (root / "crates/pdf-node/package.json").write_text("{ not json", encoding="utf-8")
        found = guard.judge(root)
        case("a manifest that cannot be parsed is a finding, not a crash",
             any("could not be parsed" in x for x in found), str(found))

    print("\n  -- against the real tree --")
    found = guard.judge(guard.REPO)
    case("this repository's six registry manifests name one release",
         found == [], str(found))

    if cases < MINIMUM_CASES:
        print(f"\nFAIL: {cases} cases ran, at least {MINIMUM_CASES} were written")
        return 1
    if failures:
        print(f"\nFAIL: {len(failures)} of {cases} cases")
        return 1
    print(f"\nOK: {cases} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
