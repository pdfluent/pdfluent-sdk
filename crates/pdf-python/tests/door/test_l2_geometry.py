# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L2: SPEC sections 4, 5, 19. Flip markers as sections land."""

import json
from contextlib import contextmanager

import pytest
from conftest import FIX, SKIP_REASON, SPEC_MISSING, assert_data_equal, read_json
from conftest import read_text as _read_text

import pdfluent_extract as pe
from pdfluent_extract.helpers import document_layout as dl
from pdfluent_extract.helpers import get_text_lines as gtl
from pdfluent_extract.helpers import multi_column as mc

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]

needs_l1 = pytest.mark.xfail(strict=True, reason="needs L1 legacy conversion")
needs_l1_lines = pytest.mark.xfail(strict=True, reason="needs L1 get_text_lines")
needs_l13 = pytest.mark.xfail(strict=True, reason="needs L1+L3 layout conversion")
needs_l15 = pytest.mark.xfail(strict=True, reason="needs L1+L5 conversion")
needs_l3 = pytest.mark.xfail(strict=True, reason="needs L3 JSON")


@contextmanager
def _page(fixture, number=0):
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for page input"
    )
    with pdfluent.Document(str(FIX(fixture))) as doc:
        yield doc[number]


def _legacy():
    pe.use_layout(False)


def _box4(box):
    try:
        return (box[0], box[1], box[2], box[3])
    except (TypeError, KeyError):
        return (box.x0, box.y0, box.x1, box.y1)


def test_g28_margins_edge():
    _legacy()
    path = str(FIX("edge-margins.pdf"))
    assert pe.to_markdown(path, margins=50) == _read_text(
        "legacy-params/margins-50-edge.md"
    )
    assert pe.to_markdown(path, margins=200) == ""
    assert pe.to_markdown(path, margins=200) == _read_text(
        "legacy-params/margins-200-edge.md"
    )


def test_g29_margins_heads():
    _legacy()
    assert pe.to_markdown(str(FIX("header-footer.pdf")), margins=50) == _read_text(
        "legacy-params/margins-50-hf.md"
    )


def test_g30_margins_sequence():
    _legacy()
    assert pe.to_markdown(
        str(FIX("edge-margins.pdf")), margins=[10, 20, 30, 40]
    ) == _read_text("legacy-params/margins-tuple.md")


@needs_l13
def test_g31_margins_ignored_layout():
    assert pe.to_markdown(str(FIX("header-footer.pdf")), margins=50) == _read_text(
        "layout-params/margins-50.md"
    )


def test_g32_margins_string_refused():
    _legacy()
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("multi-page.pdf")), margins="wide")


def test_g33_two_columns():
    with _page("two-column.pdf") as page:
        boxes = mc.column_boxes(page)
    assert len(boxes) == 2
    first = _box4(boxes[0])
    second = _box4(boxes[1])
    assert_data_equal(
        [first[0], first[2]],
        [read_json("final/col-tc-0-x0.json"), read_json("final/col-tc-0-x1.json")],
    )
    assert_data_equal(
        [second[0], second[2]],
        [read_json("final/col-tc-1-x0.json"), read_json("final/col-tc-1-x1.json")],
    )


def test_g34_head_foot_single_box():
    with _page("header-footer.pdf") as page:
        boxes = mc.column_boxes(page)
    assert len(boxes) == 1
    assert_data_equal(
        list(_box4(boxes[0])),
        [
            read_json("final/col-hf-x0.json"),
            read_json("final/col-hf-y0.json"),
            read_json("final/col-hf-x1.json"),
            read_json("final/col-hf-y1.json"),
        ],
    )
    with _page("header-footer.pdf") as page:
        boxes = mc.column_boxes(page, header_margin=0, footer_margin=0)
    assert len(boxes) == 1


@needs_l1_lines
def test_g35_clip_top_left():
    with _page("edge-margins.pdf") as page:
        assert gtl.get_text_lines(page, clip=[0, 0, 595, 100]) == _read_text(
            "final/clip-top.json"
        )
    with _page("edge-margins.pdf") as page:
        assert gtl.get_text_lines(page, clip=[0, 100, 595, 260]) == _read_text(
            "final/clip-bottom.json"
        )
    with _page("edge-margins.pdf") as page:
        assert gtl.get_text_lines(page, clip=[0, 0, 595, 842]) == _read_text(
            "final/clip-full.json"
        )


@needs_l1_lines
def test_g36_clip_list_like_rect():
    assert _read_text("final/listclip.json") == _read_text("final/clip-bottom.json")
    with _page("edge-margins.pdf") as page:
        assert gtl.get_text_lines(page, clip=[0, 100, 595, 260]) == _read_text(
            "final/listclip.json"
        )


def test_g37_page_size_ignored():
    _legacy()
    path = str(FIX("multi-page.pdf"))
    assert pe.to_markdown(path, page_width=300) == _read_text(
        "legacy-params/pagewidth-300.md"
    )
    assert pe.to_markdown(path, page_height=400) == _read_text(
        "legacy-params/pageheight-400.md"
    )
    assert pe.to_markdown(path, page_width=300) == pe.to_markdown(path)


def test_g38_fontsize_limit_ignored():
    _legacy()
    path = str(FIX("font-sizes.pdf"))
    outs = [pe.to_markdown(path, fontsize_limit=n) for n in (0, 3, 12)]
    assert outs[0] == _read_text("legacy-params/fontsize-0.md")
    assert outs[1] == _read_text("legacy-params/fontsize-3.md")
    assert outs[2] == _read_text("legacy-params/fontsize-12.md")
    assert outs[0] == outs[1] == outs[2]


@needs_l1
def test_g39_dpi_ignored_legacy():
    _legacy()
    path = str(FIX("images.pdf"))
    out72 = pe.to_markdown(path, dpi=72)
    out300 = pe.to_markdown(path, dpi=300)
    assert out72 == _read_text("legacy-params/dpi-72.md")
    assert out300 == _read_text("legacy-params/dpi-300.md")
    assert out72 == out300


def test_g40_dpi_nonsense_ignored():
    path = str(FIX("multi-page.pdf"))
    assert pe.to_markdown(path, dpi="high") == _read_text("errors/dpi-str.md")
    assert pe.to_markdown(path, image_dpi=-5) == _read_text("errors/dpi-neg.md")


def test_g41_pages_subset():
    _legacy()
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[0, 2]) == _read_text(
        "legacy-params/pages-02.md"
    )
    pe.use_layout(True)
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[0, 2]) == _read_text(
        "layout-params/pages-02.md"
    )


def test_g42_pages_negative():
    _legacy()
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[-1]) == _read_text(
        "legacy-params/pages-neg.md"
    )
    pe.use_layout(True)
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[-1]) == _read_text(
        "layout-params/pages-neg.md"
    )


def test_g43_pages_dup_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[2, 0, 2]) == _read_text(
        "legacy-params/pages-dup.md"
    )


def test_g44_pages_dup_layout_sorted():
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[2, 0, 2]) == _read_text(
        "layout-params/pages-dup.md"
    )


@needs_l13
def test_g45_pages_int():
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=1) == _read_text(
        "layout-params/pages-int.md"
    )
    _legacy()
    with pytest.raises(TypeError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages=1)


def test_g46_pages_empty():
    _legacy()
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[]) == ""
    assert pe.to_markdown(str(FIX("multi-page.pdf")), pages=[]) == _read_text(
        "legacy-params/pages-empty.md"
    )
    pe.use_layout(True)
    with pytest.raises(IndexError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages=[])


def test_g47_pages_out_of_range():
    _legacy()
    with pytest.raises(IndexError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages=[99])
    pe.use_layout(True)
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages=[99])


def test_g48_pages_wrong_type():
    _legacy()
    with pytest.raises(AssertionError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages="all")
    pe.use_layout(True)
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages="all")
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("multi-page.pdf")), pages=1.5)


def test_g49_parse_pages_int_refused():
    with pytest.raises(TypeError):
        dl.parse_document(str(FIX("multi-page.pdf")), pages=2)


@needs_l15
def test_g169_links_plain():
    assert pe.to_markdown(str(FIX("links.pdf"))) == _read_text(
        "misc2/links-layout-md.md"
    )
    out = pe.to_text(str(FIX("links.pdf")))
    assert out == _read_text("misc2/links-layout-text.txt")
    assert "http" not in out
    _legacy()
    out = pe.to_markdown(str(FIX("links.pdf")))
    assert out == _read_text("misc2/links-legacy-md.md")
    assert "http" not in out


def test_g170_links_json():
    links = json.loads(pe.to_json(str(FIX("links.pdf"))))["pages"][0]["links"]
    assert len(links) == 1
    assert_data_equal(links, read_json("basics/json-links.json")["pages"][0]["links"])
    assert links[0]["kind"] == 2 and "uri" in links[0]


def test_g171_named_dests_plain():
    assert pe.to_markdown(str(FIX("named-dests.pdf"))) == _read_text(
        "misc2/named-layout-md.md"
    )
    assert pe.to_text(str(FIX("named-dests.pdf"))) == _read_text(
        "misc2/named-layout-text.txt"
    )
    _legacy()
    assert pe.to_markdown(str(FIX("named-dests.pdf"))) == _read_text(
        "misc2/named-legacy-md.md"
    )
