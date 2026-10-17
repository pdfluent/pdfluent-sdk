# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Determinism of the known-images fixture generator (stdlib only).

Running the generator twice yields identical bytes for the PDF and the
expected JSON, and the committed outputs match a fresh generation.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import known_images

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(HERE, ".."))
PDF = os.path.join(FIXTURES, "known-images.pdf")
JSON = os.path.join(FIXTURES, "known-images.expected.json")


class DeterminismTest(unittest.TestCase):
    def test_two_runs_identical(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            pdf1 = os.path.join(tmp, "a.pdf")
            json1 = os.path.join(tmp, "a.json")
            pdf2 = os.path.join(tmp, "b.pdf")
            json2 = os.path.join(tmp, "b.json")
            self.assertEqual(known_images.main(["--pdf", pdf1, "--json", json1]), 0)
            self.assertEqual(known_images.main(["--pdf", pdf2, "--json", json2]), 0)
            with open(pdf1, "rb") as a, open(pdf2, "rb") as b:
                self.assertEqual(a.read(), b.read())
            with open(json1, "rb") as a, open(json2, "rb") as b:
                self.assertEqual(a.read(), b.read())

    def test_committed_outputs_fresh(self) -> None:
        raw_a = known_images.pixels_a()
        raw_b = known_images.pixels_b()
        with open(PDF, "rb") as handle:
            self.assertEqual(handle.read(), known_images.build_pdf(raw_a, raw_b))
        import json

        with open(JSON, encoding="utf-8") as handle:
            self.assertEqual(
                json.load(handle), known_images.build_expected(raw_a, raw_b)
            )


if __name__ == "__main__":
    unittest.main()
