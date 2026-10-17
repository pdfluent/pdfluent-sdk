# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L3: pure unit guards for page selection and chunk records.

These pin the SPEC G43..G49 page rules and the page-chunk record shape
without needing the native module, so they run anywhere.
"""

import pytest

from pdfluent_extract._chunks import (
    LayoutBox,
    PageLayout,
    ParsedDocument,
    _normalize_legacy_pages,
    _normalize_pages,
    _ocr_trigger_mode,
    _stored_use_ocr,
    make_page_chunk,
)
from pdfluent_extract.ocr import OCRMode


def test_layout_pages_dedup_sorted():
    assert _normalize_pages([2, 0, 2], 5) == [0, 2]


def test_layout_pages_negative_from_end():
    assert _normalize_pages([-1], 5) == [4]


def test_layout_pages_tuple_sorted():
    assert _normalize_pages((2, 0), 5) == [0, 2]


def test_layout_pages_int_refused():
    with pytest.raises(TypeError):
        _normalize_pages(1, 5)


def test_layout_pages_out_of_range():
    with pytest.raises(ValueError):
        _normalize_pages([99], 5)


def test_layout_pages_string_refused():
    with pytest.raises(ValueError):
        _normalize_pages("all", 5)


def test_layout_pages_empty_selects_none():
    assert _normalize_pages([], 5) == []


def test_legacy_pages_keep_order_and_repeats():
    assert _normalize_legacy_pages([2, 0, 2], 5) == [2, 0, 2]


def test_legacy_pages_range_accepted():
    assert _normalize_legacy_pages(range(3), 5) == [0, 1, 2]


def test_legacy_pages_tuple_kept():
    assert _normalize_legacy_pages((0, 2), 5) == [0, 2]


def test_legacy_pages_int_refused():
    with pytest.raises(TypeError):
        _normalize_legacy_pages(1, 5)


def test_legacy_pages_string_asserts():
    with pytest.raises(AssertionError):
        _normalize_legacy_pages("all", 5)


def test_stored_use_ocr_modes_and_fallback():
    assert _stored_use_ocr(True) is True
    assert _stored_use_ocr(OCRMode.NEVER) == 0
    assert _stored_use_ocr(OCRMode.FORCE_KEEP_OLD) == 4
    assert _stored_use_ocr("scan-everything") is True


def test_ocr_trigger_modes():
    assert _ocr_trigger_mode(True) == "select"
    assert _ocr_trigger_mode(0) == "never"
    assert _ocr_trigger_mode(2) == "select"
    assert _ocr_trigger_mode(3) == "force"
    assert _ocr_trigger_mode(4) == "force"


def _sample_doc():
    doc = ParsedDocument(filename="f.pdf", page_count=5, toc=[[1, "t", 2], [1, "u", 3]])
    page = PageLayout(
        page_number=2,
        width=100.0,
        height=200.0,
        boxes=[
            LayoutBox(0.0, 0.0, 10.0, 10.0, "text"),
            LayoutBox(0.0, 20.0, 10.0, 30.0, "picture"),
        ],
    )
    return doc, page


def test_make_page_chunk_offsets_and_keys():
    doc, page = _sample_doc()
    chunk = make_page_chunk(doc, page, "abcdefgh", [3, 5])
    assert set(chunk) == {"metadata", "toc_items", "page_boxes", "text"}
    assert chunk["text"] == "abcdefgh"
    assert chunk["metadata"]["file_path"] == "f.pdf"
    assert chunk["metadata"]["page_count"] == 5
    assert chunk["metadata"]["page_number"] == 2
    assert chunk["toc_items"] == [[1, "t", 2]]
    assert [box["pos"] for box in chunk["page_boxes"]] == [[0, 3], [3, 8]]
    assert [box["class"] for box in chunk["page_boxes"]] == ["text", "picture"]
    assert [box["index"] for box in chunk["page_boxes"]] == [0, 1]


def test_make_page_chunk_length_mismatch_asserts():
    doc, page = _sample_doc()
    with pytest.raises(AssertionError):
        make_page_chunk(doc, page, "abcdefgh", [8])
