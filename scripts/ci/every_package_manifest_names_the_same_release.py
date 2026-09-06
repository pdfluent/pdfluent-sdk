#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Homebrew, winget and Chocolatey name the release `packaging/` records (#192).

WHAT BREAKS WITHOUT THIS

Three package managers describe one desktop release, and each carries the
version, the download URL and the artefact checksum in its own spelling: Ruby in
the Homebrew cask, YAML in the winget manifests, XML and PowerShell in the
Chocolatey package. Bumping a release means changing the same three facts in
four files.

Getting one of them wrong is not a build error. It is a stranger's machine
refusing the download over a checksum mismatch -- or worse, succeeding: a URL
left at the previous version installs the OLD build while the package manager
reports the new one, and nothing on either side notices.

The checksum this repository shipped before #192 was the string `PLACEHOLDER`,
in a formula naming a release tarball that does not exist, pointing at a private
repository nobody can download from. It had been there since 1.0.0-beta.1.

WHAT IS CHECKED

`packaging/desktop-release.toml` holds each artefact once -- version, URL, size,
checksum, and for the MSI the identifiers read out of its own Property table.
Every manifest is read back and must agree with it:

  * the cask's version, sha256 and url, and that the url is built from the
    version rather than pasted (a literal version in the url is how the two
    drift apart on the next bump)
  * the three winget files: one PackageIdentifier, one PackageVersion, and an
    installer whose url and sha256 are the recorded ones. Case is not compared
    for the checksum -- winget wants uppercase hex, Homebrew lowercase, and they
    are the same number
  * the Chocolatey nuspec version, and the url and checksum in
    `chocolateyinstall.ps1`, and the ProductCode in `chocolateyuninstall.ps1`,
    which uninstalls the wrong software if it is wrong
  * that no field is a placeholder. `PLACEHOLDER`, `TODO`, `CHANGEME` and a
    checksum of sixty-four zeroes are refused by name, because the thing that
    made this guard necessary was a placeholder that read as a value

WHAT IS NOT CHECKED

Whether the artefact at that URL still hashes to that number. That is a
download, it needs the network, and a guard that fails when a CDN is having a
bad afternoon is a guard people learn to rerun until it passes.
`scripts/release/package_artifacts_still_match.sh` does it on demand, and
`verified` in the record says when it last held.
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tomllib

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - the gate installs PyYAML
    print("[packaging] FAIL: PyYAML is missing; this guard cannot read the "
          "winget manifests without it", file=sys.stderr)
    sys.exit(2)

PLACEHOLDERS = ("PLACEHOLDER", "TODO", "CHANGEME", "FIXME", "XXX", "0" * 64)

# A floor on what was examined. Three ecosystems and two artefacts: fewer means
# the walk found nothing, and reporting OK over nothing is the failure every
# guard in this directory exists to refuse.
MIN_ECOSYSTEMS = 3
MIN_ARTEFACTS = 2


class Bevinding(list):
    """Findings, collected rather than raised, so one run names every problem."""

    def add(self, path: pathlib.Path, root: pathlib.Path, message: str) -> None:
        try:
            where = path.relative_to(root)
        except ValueError:
            where = path
        self.append(f"{where}: {message}")


def _placeholder(value: str) -> bool:
    upper = value.upper()
    return any(p in upper for p in PLACEHOLDERS)


def _same_hash(left: str, right: str) -> bool:
    return left.strip().lower() == right.strip().lower()


def _artifact(record: dict, platform: str) -> dict | None:
    for row in record.get("artifact", []):
        if row.get("platform") == platform:
            return row
    return None


def check_cask(path: pathlib.Path, root: pathlib.Path, record: dict,
               mac: dict, uit: Bevinding) -> bool:
    if not path.is_file():
        uit.append(f"{path.name}: the Homebrew cask is missing")
        return False
    text = path.read_text(errors="replace")

    version = re.search(r'^\s*version\s+"([^"]+)"', text, re.M)
    sha = re.search(r'^\s*sha256\s+"([^"]+)"', text, re.M)
    url = re.search(r'^\s*url\s+"([^"]+)"', text, re.M)
    app = re.search(r'^\s*app\s+"([^"]+)"', text, re.M)
    if not (version and sha and url and app):
        uit.add(path, root, "no version, sha256, url and app stanza could be read")
        return False

    if _placeholder(sha.group(1)) or _placeholder(url.group(1)):
        uit.add(path, root, f"placeholder value: sha256 {sha.group(1)!r}")
    if version.group(1) != record["version"]:
        uit.add(path, root, f"version {version.group(1)} is not the recorded "
                            f"{record['version']}")
    if not _same_hash(sha.group(1), mac["sha256"]):
        uit.add(path, root, f"sha256 {sha.group(1)} is not the recorded "
                            f"{mac['sha256']}")
    # The url must interpolate the version. A literal one passes the comparison
    # below on the day it is written and is the reason the two drift on the next
    # bump, so the interpolated form is what is required.
    if "#{version}" not in url.group(1):
        uit.add(path, root, "the url does not interpolate #{version}; it will "
                            "be stale the next time the version moves")
    resolved = url.group(1).replace("#{version}", version.group(1))
    if resolved != mac["url"]:
        uit.add(path, root, f"url resolves to {resolved}, not the recorded "
                            f"{mac['url']}")
    if app.group(1) != mac["app_bundle"]:
        uit.add(path, root, f"app stanza {app.group(1)} is not the recorded "
                            f"{mac['app_bundle']}")
    return True


def check_winget(directory: pathlib.Path, root: pathlib.Path, record: dict,
                 win: dict, uit: Bevinding) -> bool:
    files = sorted(directory.rglob("*.yaml")) if directory.is_dir() else []
    if len(files) < 3:
        uit.append(f"{directory.name}: winget needs three manifest files, found "
                   f"{len(files)}")
        return False

    identifiers, versions, seen_installer = set(), set(), False
    for path in files:
        try:
            doc = yaml.safe_load(path.read_text(errors="replace"))
        except yaml.YAMLError as exc:
            uit.add(path, root, f"is not valid YAML: {exc}")
            continue
        if not isinstance(doc, dict):
            uit.add(path, root, "does not parse to a mapping")
            continue
        identifiers.add(doc.get("PackageIdentifier"))
        versions.add(doc.get("PackageVersion"))
        if doc.get("ManifestType") != "installer":
            continue
        seen_installer = True
        installers = doc.get("Installers") or []
        if not installers:
            uit.add(path, root, "the installer manifest lists no installer")
        for installer in installers:
            url = str(installer.get("InstallerUrl", ""))
            sha = str(installer.get("InstallerSha256", ""))
            if _placeholder(url) or _placeholder(sha):
                uit.add(path, root, f"placeholder value: {sha!r}")
            if url != win["url"]:
                uit.add(path, root, f"InstallerUrl {url} is not the recorded "
                                    f"{win['url']}")
            if not _same_hash(sha, win["sha256"]):
                uit.add(path, root, f"InstallerSha256 {sha} is not the recorded "
                                    f"{win['sha256']}")
        scope = doc.get("Scope")
        if scope and scope != win["scope"]:
            uit.add(path, root, f"Scope {scope} is not the recorded "
                                f"{win['scope']}; the MSI decides this, not the "
                                f"manifest")
        product = str(doc.get("ProductCode", "") or "")
        if product and product != win["product_code"]:
            uit.add(path, root, f"ProductCode {product} is not the recorded "
                                f"{win['product_code']}")

    if not seen_installer:
        uit.append(f"{directory.name}: no manifest of type `installer`")
    if len(identifiers) != 1:
        uit.append(f"{directory.name}: the three manifests disagree on "
                   f"PackageIdentifier: {sorted(map(str, identifiers))}")
    if versions != {record["version"]}:
        uit.append(f"{directory.name}: PackageVersion is "
                   f"{sorted(map(str, versions))}, not the recorded "
                   f"{record['version']}")
    return True


def check_chocolatey(directory: pathlib.Path, root: pathlib.Path, record: dict,
                     win: dict, uit: Bevinding) -> bool:
    nuspec = directory / "pdfluent.nuspec"
    install = directory / "tools" / "chocolateyinstall.ps1"
    uninstall = directory / "tools" / "chocolateyuninstall.ps1"
    missing = [p.name for p in (nuspec, install, uninstall) if not p.is_file()]
    if missing:
        uit.append(f"chocolatey: missing {', '.join(missing)}")
        return False

    spec = nuspec.read_text(errors="replace")
    version = re.search(r"<version>([^<]+)</version>", spec)
    if not version:
        uit.add(nuspec, root, "carries no <version>")
    elif version.group(1).strip() != record["version"]:
        uit.add(nuspec, root, f"<version> {version.group(1)} is not the "
                              f"recorded {record['version']}")

    script = install.read_text(errors="replace")
    url = re.search(r"url64bit\s*=\s*'([^']+)'", script)
    sha = re.search(r"checksum64\s*=\s*'([^']+)'", script)
    if not (url and sha):
        uit.add(install, root, "no url64bit and checksum64 could be read")
        return True
    if _placeholder(url.group(1)) or _placeholder(sha.group(1)):
        uit.add(install, root, f"placeholder value: {sha.group(1)!r}")
    if url.group(1) != win["url"]:
        uit.add(install, root, f"url64bit {url.group(1)} is not the recorded "
                               f"{win['url']}")
    if not _same_hash(sha.group(1), win["sha256"]):
        uit.add(install, root, f"checksum64 {sha.group(1)} is not the recorded "
                               f"{win['sha256']}")

    removal = uninstall.read_text(errors="replace")
    if win["product_code"] not in removal:
        uit.add(uninstall, root, f"does not name the recorded ProductCode "
                                 f"{win['product_code']}; an uninstall that "
                                 f"matches on a display name removes whatever "
                                 f"happens to match")
    return True


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parents[2]),
                    help="repository root to read; the test measures a copy")
    args = ap.parse_args(argv[1:])
    root = pathlib.Path(args.root).resolve()
    packaging = root / "packaging"

    record_path = packaging / "desktop-release.toml"
    if not record_path.is_file():
        print(f"[packaging] FAIL: {record_path} is missing; the manifests have "
              f"nothing to agree with", file=sys.stderr)
        return 1
    try:
        record = tomllib.loads(record_path.read_text(errors="replace"))
    except tomllib.TOMLDecodeError as exc:
        print(f"[packaging] FAIL: {record_path} is not valid TOML: {exc}",
              file=sys.stderr)
        return 1

    artefacts = record.get("artifact", [])
    if len(artefacts) < MIN_ARTEFACTS:
        print(f"[packaging] FAIL: {len(artefacts)} artefact(s) recorded, "
              f"expected at least {MIN_ARTEFACTS}", file=sys.stderr)
        return 1
    for row in artefacts:
        for field in ("platform", "file", "url", "size", "sha256", "verified"):
            if not row.get(field):
                print(f"[packaging] FAIL: artefact {row.get('platform')!r} has "
                      f"no {field}", file=sys.stderr)
                return 1
        if _placeholder(str(row["sha256"])):
            print(f"[packaging] FAIL: artefact {row['platform']} records a "
                  f"placeholder checksum", file=sys.stderr)
            return 1

    mac = _artifact(record, "macos-universal")
    win = _artifact(record, "windows-x64")
    if not mac or not win:
        print("[packaging] FAIL: the record must carry a macos-universal and a "
              "windows-x64 artefact", file=sys.stderr)
        return 1

    uit = Bevinding()
    ecosystems = 0
    ecosystems += check_cask(packaging / "homebrew" / "Casks" / "pdfluent.rb",
                             root, record, mac, uit)
    ecosystems += check_winget(packaging / "winget" / "manifests", root, record,
                               win, uit)
    ecosystems += check_chocolatey(packaging / "chocolatey", root, record, win,
                                   uit)

    if ecosystems < MIN_ECOSYSTEMS:
        print(f"[packaging] FAIL: {ecosystems} of {MIN_ECOSYSTEMS} ecosystems "
              f"could be read at all", file=sys.stderr)
        for line in uit:
            print(f"  {line}", file=sys.stderr)
        return 1

    if uit:
        print(f"[packaging] FAIL: {len(uit)} disagreement(s) with "
              f"packaging/desktop-release.toml ({record['version']}):",
              file=sys.stderr)
        for line in uit:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(f"[packaging] OK: {ecosystems} package managers name "
          f"{record['version']}, over {len(artefacts)} artefact(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
