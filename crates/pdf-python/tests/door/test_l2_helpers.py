# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L2 helper pins: rectangle math, page/margin rules, line geometry."""

import pytest

import pdfluent_extract as pe
from pdfluent_extract import _geometry as geo
from pdfluent_extract.helpers import document_layout as dl
from pdfluent_extract.helpers import utils as du

try:
    import pdfluent._native  # noqa: F401

    _NATIVE = True
except ImportError:
    _NATIVE = False

needs_no_native = pytest.mark.skipif(
    _NATIVE, reason="native wheel present; ImportError path unreachable"
)


def test_rect_reads_by_index_and_attribute():
    box = geo.Rect(1, 2, 30, 40)
    assert (box[0], box[1], box[2], box[3]) == (1, 2, 30, 40)
    assert (box.x0, box.y0, box.x1, box.y1) == (1, 2, 30, 40)
    assert list(box) == [1, 2, 30, 40]
    assert len(box) == 4
    assert box == [1, 2, 30, 40]
    assert box != [1, 2, 30, 41]
    assert box != "nope"


def test_iou_known_fractions():
    assert du.iou([0, 0, 10, 10], [5, 5, 15, 15]) == pytest.approx(1 / 7)
    assert du.iou([0, 0, 10, 10], [0, 0, 10, 10]) == 1.0
    assert du.iou([0, 0, 5, 5], [6, 6, 10, 10]) == 0.0
    assert du.iou([0, 0, 0, 0], [0, 0, 0, 0]) == 0.0


def test_bbox_in_bbox_edges_count_as_inside():
    assert du.bbox_in_bbox([2, 2, 5, 5], [0, 0, 10, 10]) is True
    assert du.bbox_in_bbox([0, 0, 10, 10], [0, 0, 10, 10]) is True
    assert du.bbox_in_bbox([0, 0, 20, 20], [0, 0, 10, 10]) is False


def test_are_disjoint_touching_and_strict():
    assert du.are_disjoint([0, 0, 5, 5], [6, 6, 10, 10]) is True
    assert du.are_disjoint([0, 0, 5, 5], [4, 4, 10, 10]) is False
    assert du.are_disjoint([0, 0, 5, 5], [5, 5, 10, 10]) is True
    assert du.are_disjoint([0, 0, 5, 5], [5, 5, 10, 10], strict=True) is False


def test_bbox_is_empty_none_and_degenerate():
    with pytest.raises(TypeError):
        du.bbox_is_empty(None)
    assert du.bbox_is_empty([0, 0, 0, 0]) is True
    assert du.bbox_is_empty([0, 0, 5, 5]) is False
    assert du.bbox_is_empty([5, 5, 0, 0]) is True
    assert du.bbox_is_empty([0, 0, 0, 5]) is True


def test_almost_in_bbox_fraction_of_box():
    assert du.almost_in_bbox([50, 50, 60, 60], [0, 0, 10, 10]) is False
    assert du.almost_in_bbox([0, 0, 4, 4], [0, 0, 10, 10], 0.9) is True
    assert du.almost_in_bbox([0, 0, 0, 0], [0, 0, 10, 10]) is False


def test_join_rects_union_and_plain_option():
    got = du.join_rects([[0, 0, 5, 5], [4, 4, 10, 10]])
    assert [got[0], got[1], got[2], got[3]] == [0, 0, 10, 10]
    assert (got.x0, got.y0, got.x1, got.y1) == (0, 0, 10, 10)
    assert du.join_rects([[0, 0, 5, 5], [4, 4, 10, 10]], bbox_only=True) == [
        0,
        0,
        10,
        10,
    ]
    assert du.join_rects([[2, 3, 4, 5]]) == [2, 3, 4, 5]
    with pytest.raises(ValueError):
        du.join_rects([])


def test_join_rect_pair():
    assert geo.join_rect_pair([0, 0, 5, 5], [4, 4, 10, 10]) == [0, 0, 10, 10]


def test_intersect_rects_overlap_and_miss():
    assert geo.intersect_rects([0, 0, 10, 10], [5, 5, 15, 15]) == [5, 5, 10, 10]
    missed = geo.intersect_rects([0, 0, 5, 5], [6, 6, 10, 10])
    assert du.bbox_is_empty(missed) is True
    assert geo.intersect_rects([0, 0, 10, 10], [5, 5, 15, 15], bbox_only=True) == [
        5,
        5,
        10,
        10,
    ]


def test_expand_bbox_by_points_plain_list():
    assert du.expand_bbox_by_points([0, 0, 5, 5], [[9, 9]]) == [0, 0, 9, 9]
    assert du.expand_bbox_by_points([0, 0, 5, 5], [9, 9]) == [0, 0, 9, 9]
    assert du.expand_bbox_by_points([0, 0, 5, 5], []) == [0, 0, 5, 5]


def test_refine_boxes_fuses_overlaps_and_pads():
    fused = geo.refine_boxes([[0, 0, 5, 5], [4, 4, 10, 10], [20, 20, 30, 30]])
    assert len(fused) == 2
    assert fused[0] == [0, 0, 10, 10]
    assert geo.refine_boxes([[0, 0, 0, 0], [1, 1, 2, 2]]) == [[1, 1, 2, 2]]
    assert geo.refine_boxes([[1, 1, 2, 2]], enlarge=1) == [[0, 0, 3, 3]]


def test_is_significant_overlapping_paths():
    assert geo.is_significant([0, 0, 10, 10], [[5, 5, 15, 15]]) is True
    assert geo.is_significant([0, 0, 10, 10], [[20, 20, 30, 30]]) is False
    assert geo.is_significant([0, 0, 10, 10], []) is False


def test_reading_order_helpers_row_major():
    left = [0, 0, 10, 10]
    right = [20, 0, 30, 10]
    below = [0, 40, 10, 50]
    assert geo.compute_reading_order([right, below, left], [], []) == [2, 0, 1]
    assert len(geo.cluster_stripes([left, right, below], [], [])) == 2
    assert len(geo.cluster_columns_in_stripe([left, right])) == 2
    assert len(geo.cluster_columns_in_stripe([left, [5, 0, 8, 10]])) == 1
    assert geo.find_reading_order([0, 0, 100, 100], [right, left], [left, right]) == [
        1,
        0,
    ]


def test_simplify_vectors_drops_degenerate():
    assert geo.simplify_vectors([[0, 0, 0, 0], [3, 1, 4, 5], [0, 0, 9, 9]]) == [
        [0, 0, 9, 9],
        [3, 1, 4, 5],
    ]


def test_find_virtual_lines_empty_and_gapped():
    assert geo.find_virtual_lines(None, [0, 0, 10, 10], [], [], []) == []
    bounds = geo.find_virtual_lines(
        None, [0, 0, 100, 100], [[0, 0, 10, 10], [40, 0, 50, 10]], [], []
    )
    assert len(bounds) == 1
    assert bounds[0][0] == bounds[0][2] == 25.0


def test_resolve_pages_layout():
    assert geo.resolve_pages(None, 5, layout=True) == [0, 1, 2, 3, 4]
    assert geo.resolve_pages(1, 5, layout=True) == [1]
    assert geo.resolve_pages([-1], 5, layout=True) == [4]
    assert geo.resolve_pages([2, 0, 2], 5, layout=True) == [0, 2]
    assert geo.resolve_pages((0, 2), 5, layout=True) == [0, 2]
    assert geo.resolve_pages(range(3), 5, layout=True) == [0, 1, 2]
    with pytest.raises(IndexError):
        geo.resolve_pages([], 5, layout=True)
    with pytest.raises(ValueError):
        geo.resolve_pages([99], 5, layout=True)
    with pytest.raises(ValueError):
        geo.resolve_pages([-99], 5, layout=True)
    with pytest.raises(ValueError):
        geo.resolve_pages("all", 5, layout=True)
    with pytest.raises(ValueError):
        geo.resolve_pages(1.5, 5, layout=True)
    with pytest.raises(ValueError):
        geo.resolve_pages([1.5], 5, layout=True)


def test_resolve_pages_legacy():
    assert geo.resolve_pages(None, 5, layout=False) == [0, 1, 2, 3, 4]
    assert geo.resolve_pages([2, 0, 2], 5, layout=False) == [2, 0, 2]
    assert geo.resolve_pages([-1], 5, layout=False) == [4]
    assert geo.resolve_pages([], 5, layout=False) == []
    assert geo.resolve_pages((0, 2), 5, layout=False) == [0, 2]
    assert geo.resolve_pages(range(0, 3, 2), 5, layout=False) == [0, 2]
    with pytest.raises(IndexError):
        geo.resolve_pages([99], 5, layout=False)
    with pytest.raises(TypeError):
        geo.resolve_pages(1, 5, layout=False)
    with pytest.raises(TypeError):
        geo.resolve_pages([1.5], 5, layout=False)
    with pytest.raises(AssertionError):
        geo.resolve_pages("all", 5, layout=False)


def test_resolve_margins_layout_ignores():
    assert geo.resolve_margins(50, layout=True) is None
    assert geo.resolve_margins("wide", layout=True) is None
    assert geo.resolve_margins(None, layout=True) is None


def test_resolve_margins_legacy():
    assert geo.resolve_margins(None, layout=False) is None
    assert geo.resolve_margins(0, layout=False) == [0, 0, 0, 0]
    assert geo.resolve_margins(50, layout=False) == [50, 50, 50, 50]
    assert geo.resolve_margins([10, 20, 30, 40], layout=False) == [10, 20, 30, 40]
    with pytest.raises(ValueError):
        geo.resolve_margins("wide", layout=False)
    with pytest.raises(ValueError):
        geo.resolve_margins(-1, layout=False)
    with pytest.raises(ValueError):
        geo.resolve_margins([10, 20], layout=False)
    with pytest.raises(ValueError):
        geo.resolve_margins(float("nan"), layout=False)


def test_line_edges_match_recorded_boxes():
    y0, y1 = geo._line_edges(761.8898, 12, 841.8898, True)
    assert (y0, y1) == (71.21600341796875, 82.61599731445312)
    y0, y1 = geo._line_edges(701.8898, 11, 841.8898, False)
    assert (y0, y1) == (131.947998046875, 140.2530059814453)
    y0, y1 = geo._line_edges(701.8898, 11, 841.8898, True)
    assert y1 == 142.3979949951172


def test_accumulate_matches_recorded_line_width():
    widths = [
        833, 556, 222, 556, 278, 278, 556, 500, 278, 278,
        556, 278, 278, 556, 556, 556, 556, 278, 556, 278,
    ]
    advances = [width * 12 / 1000 for width in widths]
    assert geo._accumulate(50.0, advances) == 155.39596557617188


MINIMAL_PDF = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\ntrailer\n<<>>\n"


@pytest.fixture
def pdf_path(tmp_path):
    target = tmp_path / "tiny.pdf"
    target.write_bytes(MINIMAL_PDF)
    return str(target)


def _legacy():
    pe.use_layout(False)


def test_rule_legacy_margins_string_refused(pdf_path):
    _legacy()
    with pytest.raises(ValueError):
        pe.to_markdown(pdf_path, margins="wide")


def test_rule_legacy_pages_mistyped(pdf_path):
    _legacy()
    with pytest.raises(TypeError):
        pe.to_markdown(pdf_path, pages=1)
    with pytest.raises(AssertionError):
        pe.to_markdown(pdf_path, pages="all")


def test_rule_legacy_pages_empty_is_empty(pdf_path):
    _legacy()
    assert pe.to_markdown(pdf_path, pages=[]) == ""


def test_rule_layout_pages_mistyped(pdf_path):
    with pytest.raises(ValueError):
        pe.to_markdown(pdf_path, pages="all")
    with pytest.raises(ValueError):
        pe.to_markdown(pdf_path, pages=1.5)
    with pytest.raises(IndexError):
        pe.to_markdown(pdf_path, pages=[])


def test_rule_parse_document_pages_int_refused(pdf_path):
    with pytest.raises(TypeError):
        dl.parse_document(pdf_path, pages=2)


def test_column_boxes_none_page():
    with pytest.raises(TypeError):
        geo.column_boxes(None)


@needs_no_native
def test_column_boxes_needs_native():
    class _Page:
        height = 842.0

    with pytest.raises(ImportError):
        geo.column_boxes(_Page())
