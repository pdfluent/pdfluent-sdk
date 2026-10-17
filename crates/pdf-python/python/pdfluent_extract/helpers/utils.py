# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Geometry, reading order, images and table helpers (lanes L1-L6)."""

from .._chunks import extract_form_fields_with_pages
from .._geometry import (
    almost_in_bbox,
    are_disjoint,
    bbox_in_bbox,
    bbox_is_empty,
    cluster_columns_in_stripe,
    cluster_stripes,
    compute_reading_order,
    expand_bbox_by_points,
    find_reading_order,
    find_virtual_lines,
    intersect_rects,
    iou,
    join_rects,
    simplify_vectors,
)
from .._images import add_image_orphans, clean_pictures, md_path
from .._inputs import TESSERACT_FONT_NAME, analyze_page, is_ocr_text
from .._markdown import (
    BULLETS,
    FLAGS,
    REPLACEMENT_CHARACTER,
    TYPE3_FONT_NAME,
    WHITE_CHARS,
    is_white,
    startswith_bullet,
)
from .._tables import extract_cells, table_to_markdown

__all__ = [
    "BULLETS",
    "FLAGS",
    "REPLACEMENT_CHARACTER",
    "TESSERACT_FONT_NAME",
    "TYPE3_FONT_NAME",
    "WHITE_CHARS",
    "add_image_orphans",
    "almost_in_bbox",
    "analyze_page",
    "are_disjoint",
    "bbox_in_bbox",
    "bbox_is_empty",
    "clean_pictures",
    "cluster_columns_in_stripe",
    "cluster_stripes",
    "compute_reading_order",
    "expand_bbox_by_points",
    "extract_cells",
    "extract_form_fields_with_pages",
    "find_reading_order",
    "find_virtual_lines",
    "intersect_rects",
    "iou",
    "is_ocr_text",
    "is_white",
    "join_rects",
    "md_path",
    "simplify_vectors",
    "startswith_bullet",
    "table_to_markdown",
]
