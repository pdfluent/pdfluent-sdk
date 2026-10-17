# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Legacy converter and retrieval helpers (lanes L1, L2, L4, L6)."""

from .._geometry import (
    almost_in_bbox,
    are_disjoint,
    bbox_in_bbox,
    bbox_is_empty,
    column_boxes,
    intersect_rects,
    is_significant,
    refine_boxes,
)
from .._images import (
    extract_images_on_page_simple,
    extract_images_on_page_simple_drop,
    filter_small_images,
)
from .._inputs import ProgressBar, is_ocr_text
from .._markdown import (
    BULLETS,
    GRAPHICS_TEXT,
    REPLACEMENT_CHARACTER,
    IdentifyHeaders,
    Parameters,
    TocHeaders,
    get_raw_lines,
    is_white,
    legacy_to_json as to_json,
    legacy_to_markdown as to_markdown,
    legacy_to_text as to_text,
    startswith_bullet,
)

__all__ = [
    "BULLETS",
    "GRAPHICS_TEXT",
    "REPLACEMENT_CHARACTER",
    "IdentifyHeaders",
    "Parameters",
    "ProgressBar",
    "TocHeaders",
    "almost_in_bbox",
    "are_disjoint",
    "bbox_in_bbox",
    "bbox_is_empty",
    "column_boxes",
    "extract_images_on_page_simple",
    "extract_images_on_page_simple_drop",
    "filter_small_images",
    "get_raw_lines",
    "intersect_rects",
    "is_ocr_text",
    "is_significant",
    "is_white",
    "refine_boxes",
    "startswith_bullet",
    "to_json",
    "to_markdown",
    "to_text",
]
