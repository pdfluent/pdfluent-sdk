# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""CheckFontInfo reports a font's identity instead of a silent None (#643).

The door used to validate the document and return None for every xref,
dropping the facade's probe on the floor (a silent no-op). It now reports
through the facade: [base_name, subtype] for a font carrying a descriptor,
None for a bare base-14 name (the recording pins null for the fixture's
Helvetica in tests/compat/expected/l1-module-checkfontinfo), for anything
that is not a font, and for a missing object; AttributeError without a
document.

Run:
    cd crates/pdf-python
    maturin develop -m Cargo.toml
    pytest tests/test_compat_check_font_info.py -v
"""

from __future__ import annotations

import os

import pytest

pdfluent_native = pytest.importorskip("pdfluent._native")

Document = pdfluent_native.Document
CheckFontInfo = pdfluent_native.CheckFontInfo

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "..")
SAMPLE = os.path.join(ROOT, "fixtures", "sample.pdf")
TRUETYPE = os.path.join(
    ROOT, "crates", "visual-regression", "fixtures", "text-truetype.pdf"
)
CFF = os.path.join(ROOT, "crates", "visual-regression", "fixtures", "text-cff.pdf")


def _first_font_xref(doc) -> int:
    fonts = doc[0].get_fonts()
    assert fonts, "page uses fonts"
    return fonts[0][0]


def test_embedded_font_reports_name_and_subtype() -> None:
    doc = Document(TRUETYPE)
    try:
        info = CheckFontInfo(doc, _first_font_xref(doc))
    finally:
        doc.close()
    assert info == ["VRPolygon", "TrueType"], info


def test_embedded_cff_helvetica_reports_list() -> None:
    # Same face as the sample's Helvetica, but carrying a descriptor, so it
    # reports instead of the base-14 None below.
    doc = Document(CFF)
    try:
        info = CheckFontInfo(doc, _first_font_xref(doc))
    finally:
        doc.close()
    assert info == ["Helvetica", "Type1"], info


def test_base14_without_descriptor_reports_none() -> None:
    # Object 1 of the fixture is a descriptor-less Helvetica; the recording
    # pins null for it.
    doc = Document(SAMPLE)
    try:
        assert CheckFontInfo(doc, 1) is None
        assert CheckFontInfo(doc, _first_font_xref(doc)) is None
    finally:
        doc.close()


def test_non_font_and_missing_xref_report_none() -> None:
    doc = Document(SAMPLE)
    try:
        assert CheckFontInfo(doc, 2) is None
        assert CheckFontInfo(doc, 9999) is None
    finally:
        doc.close()


def test_missing_document_reports_attribute_error() -> None:
    # The recording pins the exception class for a missing document.
    with pytest.raises(AttributeError):
        CheckFontInfo(None, 1)
