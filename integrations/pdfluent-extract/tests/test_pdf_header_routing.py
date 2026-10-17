# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Routing tolerates a PDF header anywhere in the first 1024 bytes.

The PDF spec allows leading garbage before `%PDF-`; requiring it at offset 0
refused real-world files. The engine is stubbed out of `sys.modules`, so this
tests the real routing (`_kind` plus the `to_markdown` dispatch) with no
pdfluent install: a header at offset 100 must reach the PDF side, garbage and
a header past the window must be refused, and a ZIP must still route Office.
"""
from __future__ import annotations

import io
import pathlib
import sys
import types
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))


class PdfluentParseError(Exception):
    pass


class _Document:
    """A Document that records which side the routing picked."""

    opened: list[bytes] = []

    def __init__(self, data: bytes):
        type(self).opened.append(data)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def extract_markdown(self, pages=None):
        return "# stub\n"


def _stub_pdfluent() -> types.ModuleType:
    stub = types.ModuleType("pdfluent")
    stub.PdfluentParseError = PdfluentParseError
    stub.Document = _Document
    stub.__version__ = "0.0-stub"
    chunks = types.ModuleType("pdfluent.chunks")
    chunks.Chunk = dict
    chunks.iter_chunks = lambda text, **kw: iter(())
    sys.modules.setdefault("pdfluent.chunks", chunks)
    return stub


# Hermetic when pdfluent is missing, harmless when it is installed: setdefault
# leaves a real engine alone, and each test swaps the stub in and back out, so
# this file shares a pytest run with the engine-backed suite either way.
sys.modules.setdefault("pdfluent", _stub_pdfluent())

import pdfluent_extract  # noqa: E402


class PdfHeaderWindowTests(unittest.TestCase):
    def setUp(self):
        self._real_pdfluent = pdfluent_extract.pdfluent
        pdfluent_extract.pdfluent = sys.modules["pdfluent"]
        if pdfluent_extract.pdfluent.__version__ != "0.0-stub":
            stub = _stub_pdfluent()
            sys.modules["pdfluent"] = stub
            pdfluent_extract.pdfluent = stub
        _Document.opened.clear()

    def tearDown(self):
        pdfluent_extract.pdfluent = self._real_pdfluent
        if self._real_pdfluent.__version__ != "0.0-stub":
            sys.modules["pdfluent"] = self._real_pdfluent
    def test_header_at_offset_zero_is_pdf(self):
        self.assertEqual(pdfluent_extract._kind(b"%PDF-1.7\n"), "pdf")

    def test_header_within_the_first_1024_bytes_is_pdf(self):
        data = b"\n" * 100 + b"%PDF-1.7\n" + b"body"
        self.assertEqual(pdfluent_extract._kind(data), "pdf")
        markdown = pdfluent_extract.to_markdown(data)
        self.assertIn("stub", markdown)
        self.assertIn(data, sys.modules["pdfluent"].Document.opened)

    def test_header_at_the_edge_of_the_window_is_pdf(self):
        data = b" " * 1019 + b"%PDF-"
        self.assertEqual(pdfluent_extract._kind(data), "pdf")

    def test_header_past_the_window_is_refused(self):
        data = b" " * 1024 + b"%PDF-1.7\n"
        self.assertEqual(pdfluent_extract._kind(data), "unknown")
        with self.assertRaises(pdfluent_extract.pdfluent.PdfluentParseError):
            pdfluent_extract.to_markdown(data)

    def test_garbage_is_still_refused(self):
        self.assertEqual(pdfluent_extract._kind(b"not a document"), "unknown")
        with self.assertRaises(pdfluent_extract.pdfluent.PdfluentParseError):
            pdfluent_extract.to_markdown(b"not a document")

    def test_zip_still_routes_office(self):
        self.assertEqual(pdfluent_extract._kind(b"PK\x03\x04rest"), "office")

    def test_pdf_beats_a_zip_prefix_with_a_header_inside(self):
        # A ZIP whose first kilobyte happens to contain the header bytes is
        # still read as a PDF: the header match runs first, by design.
        data = b"PK\x03\x04" + b"\0" * 100 + b"%PDF-1.7"
        self.assertEqual(pdfluent_extract._kind(data), "pdf")

    def test_file_like_sources_route_the_same_way(self):
        data = b"\n" * 50 + b"%PDF-1.7\n"
        markdown = pdfluent_extract.to_markdown(io.BytesIO(data))
        self.assertIn("stub", markdown)


if __name__ == "__main__":
    unittest.main()
