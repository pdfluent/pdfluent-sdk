# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Document.convert_to_pdf through the public path (#646).

A page span becomes fresh PDF bytes that re-open with the same pages and
content, rotated as asked. A wrongly typed rotation is a TypeError (as the
l2-document-convert-to-pdf case pins); a bad value is a ValueError.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_document_convert.py -v
"""

from __future__ import annotations

import os

import pytest

pdfluent = pytest.importorskip("pdfluent")

HERE = os.path.dirname(__file__)
SAMPLE = os.path.normpath(
    os.path.join(HERE, "..", "..", "..", "fixtures", "sample.pdf")
)
MULTI = os.path.normpath(
    os.path.join(HERE, "..", "..", "..", "fixtures", "multi-page.pdf")
)


def test_convert_defaults_roundtrip() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        blob = doc.convert_to_pdf()
    finally:
        doc.close()
    assert isinstance(blob, bytes)
    back = pdfluent.open(stream=blob, filetype="pdf")
    try:
        assert back.page_count == 1
        assert back[0].get_text() == "Test page\n"
    finally:
        back.close()


def test_convert_rotated() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        blob = doc.convert_to_pdf(rotate=90)
        assert doc[0].rotation == 0
    finally:
        doc.close()
    back = pdfluent.open(stream=blob, filetype="pdf")
    try:
        assert back.page_count == 1
        assert back[0].rotation == 90
        assert back[0].get_text() == "Test page\n"
    finally:
        back.close()


def test_convert_span_selects_pages() -> None:
    doc = pdfluent.open(MULTI)
    try:
        total = doc.page_count
        assert total >= 3
        blob = doc.convert_to_pdf(1, 2)
    finally:
        doc.close()
    back = pdfluent.open(stream=blob, filetype="pdf")
    try:
        assert back.page_count == 2
    finally:
        back.close()


def test_convert_bad_rotate_type_is_type_error() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        with pytest.raises(TypeError):
            doc.convert_to_pdf(rotate="x")
    finally:
        doc.close()


def test_convert_bad_rotate_value_is_value_error() -> None:
    doc = pdfluent.open(SAMPLE)
    try:
        with pytest.raises(ValueError):
            doc.convert_to_pdf(rotate=45)
    finally:
        doc.close()


def test_convert_reversed_span_is_value_error() -> None:
    doc = pdfluent.open(MULTI)
    try:
        with pytest.raises(ValueError):
            doc.convert_to_pdf(1, 0)
    finally:
        doc.close()
