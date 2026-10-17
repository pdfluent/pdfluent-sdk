# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L4 figure listing and image helpers (SPEC sections 7, 8, 9).

The G-line pipeline tests live in test_l4_images.py; the tests here pin the
lane's own surface: the native figure listing both document methods call,
the size rule, the file naming, the option refusals, and the orphan and
picture rules. Nothing here is expected to fail.
"""

import struct
from types import SimpleNamespace

import pytest
from conftest import FIX, SKIP_REASON, SPEC_MISSING

import pdfluent_extract as pe
from pdfluent_extract import _images as im

needs_spec = pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON)


def _native():
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for figure listing"
    )
    return pdfluent


@needs_spec
def test_extract_figures_lists_two_rendered_figures():
    pdfluent = _native()
    with pdfluent.Document(str(FIX("images.pdf"))) as doc:
        figures = doc.extract_figures()
    assert len(figures) == 2
    assert sorted(f.name for f in figures) == [
        "figure-0001-01.png",
        "figure-0001-02.png",
    ]
    for figure in figures:
        assert figure.page == 0
        x0, y0, x1, y1 = figure.bbox
        assert x1 > x0 and y1 > y0
        assert figure.png_bytes[:8] == b"\x89PNG\r\n\x1a\n"
        width, height = struct.unpack(">II", figure.png_bytes[16:24])
        assert width > 0 and height > 0


@needs_spec
def test_figure_boxes_scans_one_entry_per_page():
    pdfluent = _native()
    with pdfluent.Document(str(FIX("images.pdf"))) as doc:
        assert doc.page_count == 1
        boxes = doc.figure_boxes()
    assert len(boxes) == 1
    assert len(boxes[0]) == 2
    for box in boxes[0]:
        x0, y0, x1, y1 = box
        assert x1 > x0 and y1 > y0


@needs_spec
def test_page_image_boxes_lists_placed_images():
    pdfluent = _native()
    with pdfluent.Document(str(FIX("images.pdf"))) as doc:
        boxes = doc[0].image_boxes()
    assert len(boxes) == 2


def _a4_page():
    return SimpleNamespace(width=595.2756, height=841.8898)


def test_size_limit_sweep_keeps_wider_fractions():
    page = _a4_page()
    wide = {"bbox": [0, 0, 400, 20]}
    tall = {"bbox": [0, 0, 20, 400]}
    assert im.filter_small_images(page, [wide, tall], 0.02) == [wide, tall]
    assert im.filter_small_images(page, [wide, tall], 0.03) == [tall]
    assert im.filter_small_images(page, [wide, tall], 0.034) == []
    assert im.filter_small_images(page, [wide, tall], 0.1) == []


def test_size_limit_zero_keeps_all_and_default_drops_small():
    page = _a4_page()
    large = {"bbox": [0, 0, 400, 300]}
    tiny = {"bbox": [0, 0, 4, 4]}
    assert im.filter_small_images(page, [large, tiny], 0) == [large, tiny]
    assert im.filter_small_images(page, [large, tiny], 0.05) == [large]


def test_size_limit_marks_and_drops():
    page = _a4_page()
    wide = {"bbox": [0, 0, 400, 20], "number": 7}
    tall = {"bbox": [0, 0, 20, 400], "number": 8}
    marked = im.extract_images_on_page_simple(page, [wide, tall], 0.03)
    assert [m["keep"] for m in marked] == [False, True]
    assert marked[0]["number"] == 7
    dropped = im.extract_images_on_page_simple_drop(page, [wide, tall], 0.03)
    assert [d["number"] for d in dropped] == [8]


def test_size_limit_one_or_more_is_refused():
    page = _a4_page()
    records = [{"bbox": [0, 0, 400, 300]}]
    with pytest.raises(ValueError):
        im.filter_small_images(page, records, 1)
    with pytest.raises(ValueError):
        im.extract_images_on_page_simple(page, records, 2)
    assert im.filter_small_images(page, records, 0.99) == []


def test_filtering_never_mutates_its_input():
    page = _a4_page()
    records = [{"bbox": [0, 0, 400, 20]}, {"bbox": [0, 0, 20, 400]}]
    before = [dict(r) for r in records]
    im.filter_small_images(page, records, 0.03)
    im.extract_images_on_page_simple(page, records, 0.03)
    assert records == before


@needs_spec
def test_legacy_size_limit_one_is_refused_through_to_markdown():
    pe.use_layout(False)
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("tiny-images.pdf")), image_size_limit=1)


@needs_spec
def test_legacy_force_text_false_is_refused_through_to_markdown():
    pe.use_layout(False)
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("text-over-image.pdf")), force_text=False)


@needs_spec
def test_layout_force_text_false_is_not_refused():
    try:
        pe.to_markdown(str(FIX("text-over-image.pdf")), force_text=False)
    except ValueError:
        pytest.fail("layout must tolerate force_text=False")
    except NotImplementedError:
        pass


def test_md_path_joins_twice():
    assert im.md_path("results/x", "doc.pdf") == [
        "results/x/doc.pdf",
        "results/x/doc.pdf",
    ]
    assert im.md_path("results/x/", "doc.pdf") == [
        "results/x/doc.pdf",
        "results/x/doc.pdf",
    ]


def test_image_naming_matches_both_engines():
    assert im.layout_image_name("images.pdf", 0, 0) == "images.pdf-0001-01.png"
    assert im.layout_image_name("images.pdf", 0, 1) == "images.pdf-0001-02.png"
    assert im.legacy_image_name("images.pdf", 0, 0) == "images.pdf-0-0.png"
    assert im.legacy_image_name("images.pdf", 0, 1) == "images.pdf-0-1.png"
    assert im.image_base_name("/x/y/images.pdf") == "images.pdf"
    assert im.image_base_name("/x/y/images.pdf", "custom.pdf") == "custom.pdf"
    assert im.image_extension("jpg") == "jpg"
    assert im.image_extension("png") == "png"
    assert im.image_extension("zzz") == "png"


def test_orphans_attach_unclaimed_images_only():
    page = SimpleNamespace(
        image_boxes=[[0, 0, 100, 100], [500, 500, 600, 600]]
    )
    text = {"bbox": [0, 0, 100, 100], "boxclass": "text"}
    blocks = im.add_image_orphans(page, [text])
    assert blocks[0] is text
    orphans = [b for b in blocks[1:] if b["boxclass"] == "picture"]
    assert len(orphans) == 1
    assert orphans[0]["bbox"] == [500, 500, 600, 600]
    assert orphans[0]["image"] is None


def test_clean_pictures_drops_imageless_pictures_only():
    blocks = [
        {"bbox": [0, 0, 9, 9], "boxclass": "picture"},
        {"bbox": [0, 0, 9, 9], "boxclass": "picture", "image": b"png"},
        {"bbox": [0, 0, 9, 9], "boxclass": "text"},
    ]
    kept = im.clean_pictures(SimpleNamespace(), blocks)
    assert len(kept) == 2
    assert blocks[1] in kept and blocks[2] in kept
