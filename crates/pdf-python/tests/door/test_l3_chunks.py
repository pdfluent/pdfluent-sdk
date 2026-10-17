# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L3: SPEC sections 6, 14, 22. Flip markers as sections land."""

import glob
import json
import os
from contextlib import contextmanager

import pytest
from conftest import (
    FIX,
    FIXR,
    SKIP_REASON,
    SPEC_DIR,
    SPEC_MISSING,
    assert_data_equal,
    read_json,
)
from conftest import read_text as _read_text

import pdfluent_extract as pe
from pdfluent_extract.helpers import utils as du
from pdfluent_extract.ocr import OCRMode

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]

needs_w1 = pytest.mark.xfail(strict=True, reason="W1")


@contextmanager
def _doc(fixture):
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )
    with pdfluent.Document(str(FIX(fixture))) as doc:
        yield doc


def _legacy():
    pe.use_layout(False)


@pytest.fixture(autouse=True)
def _spec_dir_cwd(monkeypatch):
    # Chunk and JSON outputs echo their input path; the recordings were
    # made with spec-relative inputs, so this module runs there.
    monkeypatch.chdir(SPEC_DIR)


def _chunks(fixture, **kwargs):
    out = pe.to_markdown(FIXR(fixture), page_chunks=True, **kwargs)
    assert isinstance(out, list)
    return out


def test_g50_layout_chunk_keys():
    chunks = _chunks("chunk-classes.pdf")
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks.json"))
    for chunk in chunks:
        assert set(chunk) == {"metadata", "toc_items", "page_boxes", "text"}


def test_g51_legacy_chunk_keys():
    _legacy()
    chunks = _chunks("chunk-classes.pdf")
    assert_data_equal(chunks, read_json("chunks/top-legacy-chunks.json"))
    for chunk in chunks:
        assert set(chunk) == {
            "metadata",
            "toc_items",
            "tables",
            "images",
            "graphics",
            "text",
            "words",
        }


def test_g52_chunks_truthy_int():
    chunks = pe.to_markdown(FIXR("chunk-classes.pdf"), page_chunks=1)
    assert isinstance(chunks, list)
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks-1.json"))


def test_g53_chunks_page_subset():
    chunks = pe.to_markdown(FIXR("multi-page.pdf"), page_chunks=True, pages=[1, 3])
    assert len(chunks) == 2
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks-pages.json"))


def test_g54_layout_metadata():
    chunks = _chunks("multi-page.pdf")
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks-multi.json"))
    for pos, chunk in enumerate(chunks, start=1):
        meta = chunk["metadata"]
        assert meta["page_count"] == 5 and meta["page_number"] == pos
        assert "file_path" in meta


def test_g55_legacy_metadata():
    _legacy()
    chunks = _chunks("compat-frame.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-compat.json"))
    for chunk in chunks:
        assert "page" in chunk["metadata"] and "page_number" not in chunk["metadata"]


def test_g56_toc_per_page():
    chunks = pe.to_markdown(FIXR("toc-meta.pdf"), page_chunks=True)
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks-toc.json"))
    _legacy()
    chunks = _chunks("toc-meta.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-toc.json"))
    chunks = _chunks("named-dests.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-named.json"))


def test_g57_page_box_entries():
    chunks = _chunks("chunk-classes.pdf")
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks.json"))
    for chunk in chunks:
        for box in chunk["page_boxes"]:
            assert set(box) == {"index", "class", "bbox", "pos"}


def test_g58_picture_gaps():
    chunks = _chunks("images.pdf")
    assert_data_equal(chunks, read_json("chunks/top-layout-chunks-images.json"))


def test_g59_legacy_table_chunks():
    _legacy()
    chunks = _chunks("ruled-table.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-ruled.json"))
    table = chunks[0]["tables"][0]
    assert set(table) == {"bbox", "rows", "columns"}
    assert len(table["bbox"]) == 4
    assert "|" in chunks[0]["text"]


@needs_w1
def test_g60_words_on_request():
    _legacy()
    chunks = _chunks("chunk-classes.pdf")
    assert_data_equal(chunks, read_json("chunks/rag-chunks.json"))
    assert chunks[0]["words"] == []
    chunks = _chunks("chunk-classes.pdf", extract_words=True)
    assert_data_equal(chunks, read_json("chunks/rag-chunks-words.json"))
    for word in chunks[0]["words"]:
        assert len(word) == 8


@needs_w1
def test_g61_words_imply_chunks():
    _legacy()
    out = pe.to_markdown(FIXR("multi-page.pdf"), extract_words=True)
    assert isinstance(out, list)
    assert_data_equal(out, read_json("misc2/extractwords-md.json"))
    assert out[0]["words"] != []


def test_g62_words_ignored_layout():
    assert pe.to_markdown(FIXR("multi-page.pdf"), extract_words=True) == _read_text(
        "unit/layout-extractwords.json"
    )


def test_g63_legacy_image_dicts():
    _legacy()
    chunks = _chunks("images.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-images.json"))
    for image in chunks[0]["images"]:
        assert set(image) == {
            "bbox",
            "bpc",
            "colorspace",
            "cs-name",
            "has-mask",
            "height",
            "number",
            "size",
            "transform",
            "width",
            "xres",
            "yres",
        }
        assert image["bbox"] is not None
    tables = _chunks("ruled-table.pdf")[0]["tables"]
    assert isinstance(tables[0]["bbox"], list)


def test_g64_graphics_always_empty():
    _legacy()
    chunks = _chunks("vector-graphics.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-vector.json"))
    assert chunks[0]["graphics"] == []
    chunks = _chunks("vector-graphics.pdf", ignore_graphics=True)
    assert_data_equal(chunks, read_json("misc2/leg-chunks-ignore-graphics.json"))
    assert chunks[0]["graphics"] == []
    chunks = _chunks("vector-graphics.pdf", graphics_limit=0)
    assert_data_equal(chunks, read_json("misc2/leg-chunks-graphics0.json"))
    assert chunks[0]["graphics"] == []
    assert _chunks("compat-frame.pdf")[0]["graphics"] == []


def test_g65_no_links_in_chunks():
    _legacy()
    chunks = _chunks("links.pdf")
    assert_data_equal(chunks, read_json("chunks/leg-links.json"))
    pe.use_layout(True)
    for chunk in _chunks("links.pdf"):
        assert not [k for k in chunk if "link" in k.lower()]
        assert "://" not in chunk["text"]


def test_g125_json_top_keys():
    got = json.loads(pe.to_json(FIXR("multi-page.pdf")))
    assert set(got) == {
        "filename",
        "page_count",
        "toc",
        "pages",
        "metadata",
        "from_bytes",
        "image_dpi",
        "image_format",
        "image_path",
        "use_ocr",
        "form_fields",
        "force_text",
        "embed_images",
        "write_images",
    }
    assert_data_equal(got, read_json("jsonsweep/json-multi-page.pdf.json"))


def test_g126_json_toc_metadata():
    got = json.loads(pe.to_json(FIXR("toc-meta.pdf")))
    assert_data_equal(got, read_json("basics/json-tocmeta.json"))
    assert got["toc"] and all(len(e) == 3 and e[2] >= 1 for e in got["toc"])


def test_g127_json_unresolved_zero():
    got = json.loads(pe.to_json(FIXR("named-dests.pdf")))
    assert_data_equal(got, read_json("jsonsweep/json-named-dests.pdf.json"))
    assert any(e[2] == 0 for e in got["toc"])


def test_g128_nine_box_classes():
    found = set()
    for path in sorted(glob.glob(os.path.join(SPEC_DIR, "fixtures", "*.pdf"))):
        if os.path.basename(path) == "encrypted.pdf":
            continue
        doc = json.loads(pe.to_json(path))
        for page in doc["pages"]:
            for box in page["boxes"]:
                found.add(box["boxclass"])
    assert found == {
        "caption",
        "footnote",
        "list-item",
        "page-footer",
        "page-header",
        "picture",
        "section-header",
        "table",
        "text",
    }


def test_g129_box_keys():
    got = json.loads(pe.to_json(FIXR("multi-page.pdf")))
    want = read_json("jsonsweep/json-multi-page.pdf.json")
    assert_data_equal(got, want)
    keys = {
        "x0",
        "y0",
        "x1",
        "y1",
        "boxclass",
        "image",
        "table",
        "max_fontsize",
        "header_level",
        "textlines",
    }
    for page in got["pages"]:
        for box in page["boxes"]:
            assert set(box) == keys


def test_g130_table_box_shape():
    got = json.loads(pe.to_json(FIXR("ruled-table.pdf")))
    assert_data_equal(got, read_json("jsonsweep/json-ruled-table.pdf.json"))
    table = got["pages"][0]["boxes"][0]["table"]
    assert set(table) == {
        "bbox",
        "cells",
        "col_count",
        "extract",
        "markdown",
        "row_count",
    }
    assert "|" in table["markdown"]


def test_g131_span_keys():
    from pdfluent_extract.helpers import legacy as _leg

    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for page input"
    )
    with pdfluent.Document(str(FIX("multi-page.pdf"))) as doc:
        pairs = _leg.get_raw_lines(textpage=doc[0])
    recorded = read_json("unit/grl-tp.json")[0][1][0]
    for key in (
        "size",
        "flags",
        "bidi",
        "char_flags",
        "font",
        "color",
        "alpha",
        "ascender",
        "descender",
        "text",
        "origin",
        "bbox",
        "line",
        "block",
        "dir",
    ):
        assert key in recorded and key in pairs[0][1][0]


def test_g132_use_ocr_stored_raw():
    cases = [
        (OCRMode.SELECT_DROP_OLD, "misc2/json-useocr-sdo.json"),
        (OCRMode.FORCE_KEEP_OLD, "misc2/json-useocr-fko.json"),
        (OCRMode.SELECT_KEEP_OLD, "misc2/json-useocr-sko.json"),
        (OCRMode.NEVER, "ocr/json-scan-never.json"),
    ]
    for mode, ref in cases:
        got = json.loads(pe.to_json(FIXR("scanned-page.pdf"), use_ocr=mode))
        assert_data_equal(got, read_json(ref))
        assert got["use_ocr"] == int(mode)
    got = json.loads(pe.to_json(FIXR("scanned-page.pdf")))
    assert_data_equal(got, read_json("ocr/json-scan-default.json"))
    assert got["use_ocr"] is True


def test_g133_ocr_words_not_listed():
    got = json.loads(pe.to_json(FIXR("scanned-words.pdf")))
    assert_data_equal(got, read_json("ocrwords/words-default-json.json"))
    assert got["pages"][0]["words"] == [] and got["pages"][0]["full_ocred"] is False


def test_g134_json_form_fields():
    got = json.loads(pe.to_json(FIXR("form-fields.pdf")))
    assert_data_equal(got, read_json("jsonsweep/json-form-fields.pdf.json"))
    assert got["form_fields"]["given_name"]["pages"] == [0]


def test_g194_key_values():
    with _doc("form-fields.pdf") as doc:
        assert_data_equal(pe.get_key_values(doc), read_json("kv/kv-doc.json"))
    assert_data_equal(
        pe.get_key_values(FIXR("form-fields.pdf")), read_json("kv/kv-path.json")
    )


def test_g195_key_values_xrefs():
    with _doc("form-fields.pdf") as doc:
        got = pe.get_key_values(doc, xrefs=True)
    assert_data_equal(got, read_json("kv/kv-doc-xrefs.json"))
    assert all(isinstance(f["xref"], int) for f in got.values())


def test_g196_no_fields_empty():
    assert (
        pe.get_key_values(FIXR("multi-page.pdf"))
        == read_json("kv/kv-plain.json")
        == {}
    )


def test_g197_form_fields_twin():
    with _doc("form-fields.pdf") as doc:
        twin = du.extract_form_fields_with_pages(doc)
        assert_data_equal(twin, read_json("kv/effw-doc.json"))
    with _doc("form-fields.pdf") as doc:
        assert du.extract_form_fields_with_pages(doc) == pe.get_key_values(doc)
    with _doc("form-fields.pdf") as doc:
        assert du.extract_form_fields_with_pages(doc, xrefs=True) == pe.get_key_values(
            doc, xrefs=True
        )
