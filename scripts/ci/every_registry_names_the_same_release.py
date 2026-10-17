#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""One product, six registries, one version number (#358).

WHAT BREAKS WITHOUT THIS

PDFluent ships from one tree to six package pages -- crates.io, PyPI, npm
twice, NuGet and Maven Central -- and each of them takes its version from a
different file in a different language: a Cargo workspace key, a `[project]`
table, two `package.json`-shaped fields, an MSBuild property and an XML
element. Nothing reads them together, so nothing notices when they drift.

They had drifted. Read on 09-09-2026, the six live pages of one product were:

    crates.io  pdfluent               1.0.0-beta.18
    PyPI       pdfluent               1.0.0b17.post5
    npm        @pdfluent/sdk-wasm     1.0.0-beta.17.5
    npm        @pdfluent/node         1.0.0-beta.17.4.1
    NuGet      PDFluent               1.0.0-beta.17.3
    Maven      com.pdfluent:pdfluent  1.0.0-beta.17.2

Six numbers for one engine, spanning two months. A buyer comparing the Java
and the Rust page cannot tell whether they are looking at the same product,
and a bug fixed in the engine looks shipped when four of the six channels
never got it. A published version is immutable, so each of those pages is
permanent.

WHAT IS CHECKED

  one version   the six registry manifests name the same release, and that
                release is the one `[workspace.package] version` names. The
                Cargo workspace version is the anchor because the engine is
                what every binding wraps.

  no stale      when that release carries no pre-release segment -- a GA
  pre-release   release like 1.0.0 -- no registry manifest may still announce
  vocabulary    a beta. PyPI states its maturity in a trove classifier rather
                than in the version, so `Development Status :: 4 - Beta` on a
                1.0.0 wheel is not cosmetic: it is what pip's own listing and
                every package index that mirrors PyPI will show.

WHAT IS NOT CHECKED

Whether the registries actually carry that version. That needs the network
and six APIs, it fails when a CDN is having a bad afternoon, and the answer
changes the moment somebody publishes. `scripts/release/registry_dry_run.sh`
builds the six artefacts, and the runbook records the live pages by hand
after a publish.

FLOOR, because a scan that reads nothing approves everything

All six manifests must exist and must each yield a version. Five agreeing
manifests and one that could not be parsed is a failure, not a pass.

Usage:
    python3 scripts/ci/every_registry_names_the_same_release.py

Exit codes:
    0  the six registry manifests name one release
    1  they do not
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

# Every file that decides what a registry page says the version is. The key is
# how the channel is named in the runbook and on the ticket; changing one of
# these paths without changing this list is the drift the guard exists to stop.
MANIFESTS: dict[str, str] = {
    "crates.io pdfluent": "crates/pdfluent/Cargo.toml",
    "PyPI pdfluent": "crates/pdf-python/pyproject.toml",
    "npm @pdfluent/node": "crates/pdf-node/package.json",
    "npm @pdfluent/sdk-wasm": "crates/xfa-wasm/Cargo.toml",
    "NuGet PDFluent": "bindings/dotnet/src/PDFluent/PDFluent.csproj",
    "Maven com.pdfluent:pdfluent": "bindings/java/pom.xml",
}

# The trove classifier that states maturity on PyPI. A GA release that keeps
# it says "beta" on the page whatever the version says.
BETA_CLASSIFIER = "Development Status :: 4 - Beta"
STABLE_CLASSIFIER = "Development Status :: 5 - Production/Stable"


class Findings(list):
    """Collected rather than raised, so one run names every problem."""

    def note(self, where: str, what: str) -> None:
        self.append(f"{where}: {what}")


def workspace_version(root: pathlib.Path) -> str:
    """The release the whole tree is on: `[workspace.package] version`."""
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    after = text.split("[workspace.package]", 1)
    if len(after) != 2:
        raise ValueError("Cargo.toml has no [workspace.package] table")
    # Stop at the next table header so a later `version` cannot be picked up.
    block = re.split(r"^\[", after[1], maxsplit=1, flags=re.M)[0]
    found = re.search(r'^version\s*=\s*"([^"]+)"', block, re.M)
    if not found:
        raise ValueError("[workspace.package] has no version")
    return found.group(1)


def _table_version(text: str, table: str) -> str | None:
    """The `version` of one TOML table, ignoring every other table."""
    after = text.split(table, 1)
    if len(after) != 2:
        return None
    block = re.split(r"^\[", after[1], maxsplit=1, flags=re.M)[0]
    found = re.search(r'^version\s*=\s*"([^"]+)"', block, re.M)
    return found.group(1) if found else None


def cargo_package_version(text: str) -> str | None:
    return _table_version(text, "[package]")


def npm_metadata_version(text: str) -> str | None:
    """The npm identity carried in `[package.metadata.npm]`.

    wasm-pack builds the npm package from the crate, but an npm scope is not
    a legal Rust crate name, so the published identity lives in this table and
    `scripts/release/transform-wasm-pkg.sh` writes it into the emitted
    package.json. It is the field the npm page shows.
    """
    return _table_version(text, "[package.metadata.npm]")


def pyproject_version(text: str) -> str | None:
    return _table_version(text, "[project]")


def package_json_version(text: str) -> str | None:
    return json.loads(text).get("version")


def csproj_version(text: str) -> str | None:
    found = re.search(r"<Version>([^<]+)</Version>", text)
    return found.group(1).strip() if found else None


def pom_version(text: str) -> str | None:
    """The project's own version: the first one outside `<parent>`."""
    text = re.sub(r"<parent>.*?</parent>", "", text, flags=re.S)
    # Everything before <dependencies> is the project's own coordinates.
    head = text.split("<dependencies>", 1)[0]
    found = re.search(r"<version>([^<]+)</version>", head)
    return found.group(1).strip() if found else None


READERS = {
    "crates/pdfluent/Cargo.toml": cargo_package_version,
    "crates/pdf-python/pyproject.toml": pyproject_version,
    "crates/pdf-node/package.json": package_json_version,
    "crates/xfa-wasm/Cargo.toml": npm_metadata_version,
    "bindings/dotnet/src/PDFluent/PDFluent.csproj": csproj_version,
    "bindings/java/pom.xml": pom_version,
}


def python_version_of(release: str) -> str:
    """PEP 440 spelling of a release, so PyPI may differ where it must.

    `1.0.0-beta.18` is not a version PyPI accepts; `1.0.0b18` is the same
    release. A GA release like `1.0.0` is spelled identically either way,
    which is the only case this guard has to compare exactly.
    """
    match = re.fullmatch(r"(\d+\.\d+\.\d+)-(alpha|beta|rc)\.(\d+(?:\.\d+)*)", release)
    if not match:
        return release
    letter = {"beta": "b", "rc": "rc", "alpha": "a"}[match.group(2)]
    return f"{match.group(1)}{letter}{match.group(3)}"


def is_prerelease(release: str) -> bool:
    return "-" in release


def judge(root: pathlib.Path) -> list[str]:
    """Every way this tree's six registry manifests disagree about the release."""
    findings = Findings()

    try:
        release = workspace_version(root)
    except (OSError, ValueError) as why:
        findings.note("workspace", f"cannot read the release: {why}")
        return findings

    read_count = 0
    for channel, path in MANIFESTS.items():
        manifest = root / path
        if not manifest.exists():
            findings.note(channel, f"{path} does not exist; the walk cannot read it")
            continue
        try:
            found = READERS[path](manifest.read_text(encoding="utf-8"))
        except Exception as why:  # a malformed manifest is a finding, not a crash
            findings.note(channel, f"{path} could not be parsed: {why}")
            continue
        if not found:
            findings.note(channel, f"{path} carries no version this guard can find")
            continue
        read_count += 1
        expected = python_version_of(release) if channel.startswith("PyPI") else release
        if found != expected:
            findings.note(
                channel,
                f"{path} names {found}, the workspace names {release}"
                + (f" (PyPI spelling {expected})" if expected != release else ""),
            )

    if read_count < len(MANIFESTS):
        findings.note(
            "floor",
            f"only {read_count} of {len(MANIFESTS)} registry manifests were read; "
            "a version scan that reads nothing approves everything",
        )

    pyproject = root / "crates/pdf-python/pyproject.toml"
    if not is_prerelease(release) and pyproject.exists():
        if BETA_CLASSIFIER in pyproject.read_text(encoding="utf-8"):
            findings.note(
                "PyPI pdfluent",
                f"release {release} is not a pre-release, but pyproject.toml still "
                f'classifies it "{BETA_CLASSIFIER}"; PyPI shows the classifier, '
                f'not the version. Use "{STABLE_CLASSIFIER}"',
            )

    return findings


def main() -> int:
    findings = judge(REPO)
    if findings:
        print("[registry-version] FAIL: the six registry manifests do not name one release",
              file=sys.stderr)
        for line in findings:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print(f"[registry-version] OK: {len(MANIFESTS)} registry manifests "
          f"name {workspace_version(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
