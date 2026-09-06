#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The packaging guard goes red when a manifest stops naming the release (#192).

Every case here breaks ONE fact in a COPY of `packaging/` and asserts the guard
refuses it. A guard for cross-file agreement that is only ever run against a
tree where the files agree has never demonstrated it can tell the difference --
and the specific failure it exists to catch, a checksum left at the previous
release, looks exactly like a correct file to everything else in this
repository.

The tree under test is a copy in a temporary directory. Nothing here writes to
the checkout it was started from.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "every_package_manifest_names_the_same_release.py"

CASK = "packaging/homebrew/Casks/pdfluent.rb"
RECORD = "packaging/desktop-release.toml"
WINGET = ("packaging/winget/manifests/i/InnovationTrigger/PDFluent/"
          "1.0.0-beta.21/InnovationTrigger.PDFluent.installer.yaml")
WINGET_VERSION = ("packaging/winget/manifests/i/InnovationTrigger/PDFluent/"
                  "1.0.0-beta.21/InnovationTrigger.PDFluent.yaml")
CHOCO_SPEC = "packaging/chocolatey/pdfluent.nuspec"
CHOCO_INSTALL = "packaging/chocolatey/tools/chocolateyinstall.ps1"
CHOCO_UNINSTALL = "packaging/chocolatey/tools/chocolateyuninstall.ps1"


def run(root: pathlib.Path) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(GUARD), "--root", str(root)],
                          capture_output=True, text=True)


def edit(root: pathlib.Path, rel: str, pattern: str, replacement: str) -> None:
    path = root / rel
    text = path.read_text()
    new, count = re.subn(pattern, replacement, text, count=1, flags=re.M)
    if count != 1:
        raise AssertionError(f"the case did not apply: {pattern!r} in {rel}")
    path.write_text(new)


def drop(root: pathlib.Path, rel: str) -> None:
    (root / rel).unlink()


CASES: list[tuple[str, object]] = [
    ("the cask keeps last release's checksum",
     lambda r: edit(r, CASK, r'sha256 "[0-9a-f]{64}"',
                    'sha256 "' + "b" * 64 + '"')),
    ("the cask's checksum is a placeholder",
     lambda r: edit(r, CASK, r'sha256 "[0-9a-f]{64}"', 'sha256 "PLACEHOLDER"')),
    ("the cask names a version the record does not",
     lambda r: edit(r, CASK, r'version "[^"]+"', 'version "1.0.0-beta.20"')),
    ("the cask pastes the version into the url instead of interpolating it",
     lambda r: edit(r, CASK, re.escape('#{version}/PDFluent_#{version}'),
                    "1.0.0-beta.21/PDFluent_1.0.0-beta.21")),
    ("the cask installs a bundle by another name",
     lambda r: edit(r, CASK, r'app "PDFluent\.app"', 'app "PDFluent 2.app"')),
    ("the cask is gone",
     lambda r: drop(r, CASK)),
    ("winget's installer url points at another release",
     lambda r: edit(r, WINGET, r"InstallerUrl: \S+",
                    "InstallerUrl: https://pdfluent.com/releases/1.0.0-beta.20/"
                    "PDFluent_1.0.0-beta.20_x64_en-US.msi")),
    ("winget's checksum is another number",
     lambda r: edit(r, WINGET, r"InstallerSha256: [0-9A-F]{64}",
                    "InstallerSha256: " + "C" * 64)),
    ("winget claims a user-scope install of a per-machine MSI",
     lambda r: edit(r, WINGET, r"^Scope: machine", "Scope: user")),
    ("winget names another ProductCode",
     lambda r: edit(r, WINGET, r"^ProductCode: '\{[0-9A-F-]+\}'",
                    "ProductCode: '{00000000-0000-0000-0000-000000000000}'")),
    ("winget's three files disagree on the version",
     lambda r: edit(r, WINGET_VERSION, r"PackageVersion: \S+",
                    "PackageVersion: 1.0.0-beta.20")),
    ("winget's installer manifest is not valid YAML",
     lambda r: edit(r, WINGET, r"^Installers:", "Installers: [oops")),
    ("chocolatey's nuspec names another version",
     lambda r: edit(r, CHOCO_SPEC, r"<version>[^<]+</version>",
                    "<version>1.0.0-beta.20</version>")),
    ("chocolatey downloads from another url",
     lambda r: edit(r, CHOCO_INSTALL, r"url64bit\s*=\s*'[^']+'",
                    "url64bit       = 'https://example.invalid/PDFluent.msi'")),
    ("chocolatey verifies against another checksum",
     lambda r: edit(r, CHOCO_INSTALL, r"checksum64\s*=\s*'[^']+'",
                    "checksum64     = '" + "D" * 64 + "'")),
    ("chocolatey uninstalls by name instead of by ProductCode",
     lambda r: edit(r, CHOCO_UNINSTALL, r"\{[0-9A-F-]+\}", "PDFluent")),
    ("chocolatey's install script is gone",
     lambda r: drop(r, CHOCO_INSTALL)),
    ("the record itself is gone",
     lambda r: drop(r, RECORD)),
    ("the record carries a placeholder checksum",
     lambda r: edit(r, RECORD, r'sha256 = "[0-9a-f]{64}"',
                    'sha256 = "PLACEHOLDER"')),
]


def main() -> int:
    bron = REPO / "packaging"
    if not bron.is_dir():
        print("FAIL: packaging/ is missing; there is nothing to measure",
              file=sys.stderr)
        return 1

    fouten: list[str] = []
    with tempfile.TemporaryDirectory(prefix="pkgmanifest.") as tmp:
        schoon = pathlib.Path(tmp) / "clean"
        shutil.copytree(bron, schoon / "packaging")

        # The unmutated copy first. A guard that fails on a correct tree would
        # make every case below pass for the wrong reason.
        basis = run(schoon)
        if basis.returncode != 0:
            print("FAIL: the guard refuses the tree as it stands, so no case "
                  "below proves anything:", file=sys.stderr)
            print(basis.stdout + basis.stderr, file=sys.stderr)
            return 1

        for n, (naam, breek) in enumerate(CASES):
            werk = pathlib.Path(tmp) / f"case{n}"
            shutil.copytree(schoon, werk)
            breek(werk)
            uitkomst = run(werk)
            if uitkomst.returncode == 0:
                fouten.append(f"the guard passed when {naam}")

    if fouten:
        print(f"FAIL: {len(fouten)} of {len(CASES)} case(s) went unnoticed:",
              file=sys.stderr)
        for regel in fouten:
            print(f"  {regel}", file=sys.stderr)
        return 1

    print(f"[packaging-test] OK: the guard refuses all {len(CASES)} broken "
          f"manifests and accepts the tree as it stands.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
