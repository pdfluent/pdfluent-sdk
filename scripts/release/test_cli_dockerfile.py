#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for the CLI Dockerfile and its build script (T6 §4).

The image is alpine:3.20 with the static musl binary staged in — ENTRYPOINT
pdfluent, WORKDIR /data, licence files under /usr/share/doc/pdfluent/ — and
nothing is compiled inside it, so the image holds exactly the binaries the
gate checked. Each test names the line it guards: remove that line and the
test turns red.
"""
from __future__ import annotations

import pathlib
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
DOCKERFILE = REPO / "crates" / "pdfluent-cli" / "Dockerfile"
BUILD_SCRIPT = HERE / "cli_docker_build.sh"

MINIMUM_CASES = 8  # FLOOR


class DockerfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = DOCKERFILE.read_text()

    def test_base_is_alpine_320(self):
        self.assertIn("FROM alpine:3.20", self.text)

    def test_entrypoint_is_pdfluent(self):
        self.assertIn('ENTRYPOINT ["pdfluent"]', self.text)

    def test_workdir_is_data(self):
        self.assertIn("WORKDIR /data", self.text)

    def test_binary_is_staged_per_arch(self):
        # TARGETARCH selects the musl binary the build script staged; the
        # Dockerfile never compiles (see next test).
        self.assertIn("ARG TARGETARCH", self.text)
        self.assertIn("COPY pdfluent-${TARGETARCH} /usr/local/bin/pdfluent",
                      self.text)

    def test_nothing_is_compiled_inside(self):
        lowered = self.text.lower()
        for word in ("cargo", "rustc", "rustup", "zigbuild", "apk add"):
            self.assertNotIn(word, lowered)

    def test_licence_files_land_in_doc_dir(self):
        self.assertIn("/usr/share/doc/pdfluent/", self.text)
        for name in ("LICENSE", "LICENSE-COMMERCIAL", "NOTICE",
                     "THIRD_PARTY_LICENSES.txt"):
            self.assertIn(name, self.text)

    def test_licenses_label_names_the_dual_licence(self):
        # The image label used to name AGPL-3.0-only alone, contradicting the
        # commercial half staged two lines below it.
        self.assertIn(
            'org.opencontainers.image.licenses="AGPL-3.0-only OR '
            'LicenseRef-PDFluent-Commercial"',
            self.text,
        )

    def test_build_script_builds_both_arches_without_pushing(self):
        script = BUILD_SCRIPT.read_text()
        self.assertIn("--platform linux/amd64,linux/arm64", script)
        # No push unless the operator passes --push, by hand.
        self.assertIn("--push", script)
        self.assertNotIn("docker push", script)

    def test_build_script_stages_from_the_musl_archives(self):
        script = BUILD_SCRIPT.read_text()
        self.assertIn("unknown-linux-musl.tar.gz", script)
        self.assertIn("SHA256SUMS", script)


if __name__ == "__main__":
    unittest.main()
