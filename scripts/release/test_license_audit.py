#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the licence audit.

An artefact without a LICENSE file fails; one with an empty placeholder
fails; a POM whose <licenses> block names anything but the dual licence
fails; a csproj without its licence file fails. A complete fixture passes.
Each case builds its fixture here.
"""
from __future__ import annotations

import importlib.util
import pathlib
import sys
import tarfile
import tempfile
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
_spec = importlib.util.spec_from_file_location("license_audit", HERE / "license_audit.py")
audit = importlib.util.module_from_spec(_spec)
sys.modules["license_audit"] = audit
_spec.loader.exec_module(audit)

MINIMUM_CASES = 7  # FLOOR

AGPL_TEXT = "GNU AFFERO GENERAL PUBLIC LICENSE\n" + ("terms...\n" * 20)
COMMERCIAL_TEXT = "PDFluent Commercial Licence\n" + ("terms...\n" * 20)
OFFER_TEXT = (
    "SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-PDFluent-Commercial\n"
    + ("offer...\n" * 20)
)
DUAL = "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial"
METADATA_GOOD = (
    "Metadata-Version: 2.4\nName: pdfluent\nVersion: 1.0.1\n"
    f"License-Expression: {DUAL}\n"
).encode()


def _write_zip(path: pathlib.Path, entries: dict[str, bytes]) -> pathlib.Path:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_STORED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return path


def _write_tar(path: pathlib.Path, entries: dict[str, bytes]) -> pathlib.Path:
    import io

    with tarfile.open(path, "w:gz") as tf:
        for name, data in entries.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    return path


def _npm_entries(license_value: str | None, *, offer: bool = True) -> dict[str, bytes]:
    import json

    manifest: dict[str, object] = {"name": "@pdfluent/sdk-wasm", "version": "1.0.1"}
    if license_value is not None:
        manifest["license"] = license_value
    entries = {
        "package/package.json": json.dumps(manifest).encode(),
        "package/LICENSE": AGPL_TEXT.encode(),
        "package/LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
    }
    if offer:
        entries["package/LICENSE-OFFER"] = OFFER_TEXT.encode()
    return entries


GOOD_POM = """<project>
  <licenses>
    <license>
      <name>AGPL-3.0-only OR LicenseRef-PDFluent-Commercial</name>
    </license>
  </licenses>
</project>
"""


class LicenseAuditTests(unittest.TestCase):
    def test_wheel_with_licence_text_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {
                    "x/__init__.py": b"# x\n",
                    "x-0.1.dist-info/METADATA": METADATA_GOOD,
                    "x-0.1.dist-info/licenses/LICENSE": AGPL_TEXT.encode(),
                    "x-0.1.dist-info/licenses/LICENSE-COMMERCIAL": (
                        COMMERCIAL_TEXT.encode()
                    ),
                },
            )
            self.assertEqual(audit.check_archive(whl), [])

    def test_wheel_without_license_expression_fails(self):
        # The field the audit never read: licence files alone passed, while
        # the registry read a METADATA promising other terms.
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {
                    "x/__init__.py": b"# x\n",
                    "x-0.1.dist-info/METADATA": b"Name: x\nVersion: 0.1\n",
                    "x-0.1.dist-info/licenses/LICENSE": AGPL_TEXT.encode(),
                    "x-0.1.dist-info/licenses/LICENSE-COMMERCIAL": (
                        COMMERCIAL_TEXT.encode()
                    ),
                },
            )
            failures = audit.check_archive(whl)
            self.assertTrue(
                any("License-Expression" in f for f in failures), failures
            )

    def test_wheel_with_another_expression_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            meta = METADATA_GOOD.replace(
                DUAL.encode(), b"MIT", 1
            )
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {
                    "x/__init__.py": b"# x\n",
                    "x-0.1.dist-info/METADATA": meta,
                    "x-0.1.dist-info/licenses/LICENSE": AGPL_TEXT.encode(),
                    "x-0.1.dist-info/licenses/LICENSE-COMMERCIAL": (
                        COMMERCIAL_TEXT.encode()
                    ),
                },
            )
            failures = audit.check_archive(whl)
            self.assertTrue(
                any("MIT" in f and DUAL in f for f in failures), failures
            )

    def test_sdist_without_license_expression_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "x-0.1.tar.gz",
                {
                    "x-0.1/PKG-INFO": b"Name: x\nVersion: 0.1\n",
                    "x-0.1/LICENSE": AGPL_TEXT.encode(),
                    "x-0.1/LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
                },
            )
            failures = audit.check_archive(tgz)
            self.assertTrue(
                any("PKG-INFO" in f and "License-Expression" in f for f in failures),
                failures,
            )

    def test_sdist_with_license_expression_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "x-0.1.tar.gz",
                {
                    "x-0.1/PKG-INFO": METADATA_GOOD,
                    "x-0.1/LICENSE": AGPL_TEXT.encode(),
                    "x-0.1/LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
                },
            )
            self.assertEqual(audit.check_archive(tgz), [])

    def test_npm_tarball_with_the_pointer_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-sdk-wasm-1.0.1.tgz",
                _npm_entries("SEE LICENSE IN LICENSE-OFFER"),
            )
            self.assertEqual(audit.check_archive(tgz), [])

    def test_npm_tarball_with_another_license_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-sdk-wasm-1.0.1.tgz",
                _npm_entries("MIT"),
            )
            failures = audit.check_archive(tgz)
            self.assertTrue(
                any("package.json" in f and "MIT" in f for f in failures),
                failures,
            )

    def test_npm_tarball_without_the_offer_file_fails(self):
        # The pointer dangles: every sdk-wasm tarball up to 1.0.0-beta.17.5
        # shipped exactly this mismatch.
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-sdk-wasm-1.0.1.tgz",
                _npm_entries("SEE LICENSE IN LICENSE-OFFER", offer=False),
            )
            failures = audit.check_archive(tgz)
            self.assertTrue(
                any("LICENSE-OFFER" in f for f in failures), failures
            )

    def test_npm_tarball_with_a_nested_module_marker_passes(self):
        # The generated {"type": "commonjs"} marker under dist/ names no
        # package and carries no licence. It is listed FIRST, the way the
        # @pdfluent/extract tarball orders its entries: judging the first
        # manifest in tarball order failed a correct tarball on its own
        # marker file.
        import json

        with tempfile.TemporaryDirectory() as tmp:
            entries = {
                "package/dist/cjs/package.json": json.dumps(
                    {"type": "commonjs"}
                ).encode(),
            }
            entries.update(_npm_entries("SEE LICENSE IN LICENSE-OFFER"))
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-extract-0.1.0.tgz", entries
            )
            self.assertEqual(audit.check_archive(tgz), [])

    def test_npm_tarball_with_a_nested_named_package_without_licence_fails(
        self,
    ):
        # A nested manifest WITH a name is a nested package, and it
        # travels under the same licence route as the top one.
        import json

        with tempfile.TemporaryDirectory() as tmp:
            entries = _npm_entries("SEE LICENSE IN LICENSE-OFFER")
            entries["package/dist/cjs/package.json"] = json.dumps(
                {"name": "@pdfluent/extract-cjs", "version": "0.1.0"}
            ).encode()
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-extract-0.1.0.tgz", entries
            )
            failures = audit.check_archive(tgz)
            self.assertTrue(
                any("nested package" in f for f in failures), failures
            )

    def test_sdist_with_vendored_third_party_licences_passes(self):
        # The sdist vendors its path-dependency sources, and those carry
        # their own texts (MIT/Apache for third-party crates, font
        # licences under assets/). Only the top level of the distribution
        # is judged; the vendored texts are supposed to say something
        # else. Shaped like the real pdfluent-1.1.0.tar.gz.
        mit = b"MIT License\n\n" + b"permission hereby granted...\n" * 20
        apache = b"Apache License\nVersion 2.0\n" + b"terms...\n" * 30
        entries = {
            "pdfluent-1.1.0/LICENSE": AGPL_TEXT.encode(),
            "pdfluent-1.1.0/LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
            "pdfluent-1.1.0/PKG-INFO": (
                "Metadata-Version: 2.4\nName: pdfluent\nVersion: 1.1.0\n"
                "License-Expression: AGPL-3.0-only OR "
                "LicenseRef-PDFluent-Commercial\n"
            ).encode(),
            "pdfluent-1.1.0/cff-parser/LICENSE-APACHE": apache,
            "pdfluent-1.1.0/cff-parser/LICENSE-MIT": mit,
            "pdfluent-1.1.0/lopdf/LICENSE": mit,
            "pdfluent-1.1.0/pdf-font/assets/LICENSE.txt": b"Font licence\n"
            + b"x" * 200,
        }
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-1.1.0.tar.gz", entries
            )
            self.assertEqual(audit.check_archive(tgz), [])

    def test_nested_commercial_text_does_not_satisfy_the_top_level(self):
        # A vendored copy is not the distribution offering the text: the
        # top-level LICENSE-COMMERCIAL is required even when a nested
        # crate carries the same bytes.
        entries = {
            "pdfluent-1.1.0/LICENSE": AGPL_TEXT.encode(),
            "pdfluent-1.1.0/PKG-INFO": (
                "Metadata-Version: 2.4\nName: pdfluent\nVersion: 1.1.0\n"
                "License-Expression: AGPL-3.0-only OR "
                "LicenseRef-PDFluent-Commercial\n"
            ).encode(),
            "pdfluent-1.1.0/formcalc-interpreter/LICENSE": AGPL_TEXT.encode(),
            "pdfluent-1.1.0/formcalc-interpreter/LICENSE-COMMERCIAL": (
                COMMERCIAL_TEXT.encode()
            ),
        }
        with tempfile.TemporaryDirectory() as tmp:
            tgz = _write_tar(
                pathlib.Path(tmp) / "pdfluent-1.1.0.tar.gz", entries
            )
            failures = audit.check_archive(tgz)
            self.assertTrue(
                any("commercial" in f for f in failures), failures
            )

    def test_wasm_dry_run_audits_the_packed_tarball(self):
        # The wiring: wasm_dry_run.sh packs the exact tarball and hands it
        # to this audit, so the wasm channel cannot publish what the audit
        # never saw.
        text = (REPO / "scripts" / "release" / "wasm_dry_run.sh").read_text()
        self.assertIn("license_audit.py", text)

    def test_wasm_pkg_dir_is_audited_in_place(self):
        # The wasm package as a directory, before it is packed: the same
        # pointer and the same offer, read off the same names.
        with tempfile.TemporaryDirectory() as tmp:
            pkg = pathlib.Path(tmp) / "pkg"
            pkg.mkdir()
            for name, data in _npm_entries("SEE LICENSE IN LICENSE-OFFER").items():
                target = pkg / pathlib.Path(name).name
                target.write_bytes(data)
            self.assertEqual(audit.check_archive(pkg), [])
            (pkg / "package.json").write_text('{"name": "x", "license": "MIT"}')
            failures = audit.check_archive(pkg)
            self.assertTrue(
                any("package.json" in f for f in failures), failures
            )

    def test_tarball_with_licence_text_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "LICENSE").write_text(AGPL_TEXT)
            (root / "LICENSE-COMMERCIAL").write_text(COMMERCIAL_TEXT)
            tgz = root / "x.tgz"
            with tarfile.open(tgz, "w:gz") as tf:
                tf.add(root / "LICENSE", arcname="x-0.1/LICENSE")
                tf.add(
                    root / "LICENSE-COMMERCIAL",
                    arcname="x-0.1/LICENSE-COMMERCIAL",
                )
            self.assertEqual(audit.check_archive(tgz), [])

    def test_archive_with_only_the_commercial_half_fails_on_the_agpl_half(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {
                    "x/__init__.py": b"# x\n",
                    "LICENSE-COMMERCIAL": COMMERCIAL_TEXT.encode(),
                },
            )
            failures = audit.check_archive(whl)
            self.assertTrue(
                any("AGPL" in f for f in failures), failures
            )

    def test_archive_with_only_the_agpl_half_fails_on_the_commercial_half(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {
                    "x/__init__.py": b"# x\n",
                    "LICENSE": AGPL_TEXT.encode(),
                },
            )
            failures = audit.check_archive(whl)
            self.assertTrue(
                any("commercial" in f for f in failures), failures
            )

    def test_artefact_without_licence_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {"x/__init__.py": b"# x\n"},
            )
            failures = audit.check_archive(whl)
            self.assertEqual(len(failures), 1)
            self.assertIn("no LICENSE", failures[0])

    def test_empty_licence_placeholder_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            whl = _write_zip(
                pathlib.Path(tmp) / "x-0.1-py3-none-any.whl",
                {"LICENSE": b"TODO: licence\n"},
            )
            failures = audit.check_archive(whl)
            self.assertTrue(failures)
            self.assertTrue(
                any("does not carry the licence text" in f for f in failures),
                failures,
            )

    def test_pom_with_dual_licence_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            pom = pathlib.Path(tmp) / "pom.xml"
            pom.write_text(GOOD_POM)
            self.assertEqual(audit.check_pom(pom), [])

    def test_pom_with_extra_grant_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            pom = pathlib.Path(tmp) / "pom.xml"
            pom.write_text(GOOD_POM.replace("Commercial</name>", "Commercial OR MIT</name>"))
            failures = audit.check_pom(pom)
            self.assertEqual(len(failures), 1)
            self.assertIn("exactly", failures[0])

    def test_csproj_without_licence_file_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            csproj = pathlib.Path(tmp) / "X.csproj"
            csproj.write_text("<Project><PropertyGroup></PropertyGroup></Project>")
            failures = audit.check_csproj(csproj)
            self.assertEqual(len(failures), 1)
            self.assertIn("PackageLicenseFile", failures[0])

    def test_csproj_with_licence_file_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "LICENSE-OFFER").write_text(
                "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial\n" + "offer...\n" * 10
            )
            csproj = root / "X.csproj"
            csproj.write_text(
                "<Project><PropertyGroup>"
                "<PackageLicenseFile>LICENSE-OFFER</PackageLicenseFile>"
                "</PropertyGroup></Project>"
            )
            self.assertEqual(audit.check_csproj(csproj), [])


if __name__ == "__main__":
    unittest.main()
