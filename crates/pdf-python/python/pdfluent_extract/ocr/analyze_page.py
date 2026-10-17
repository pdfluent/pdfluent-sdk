# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Page scoring for recogniser use (lanes L6 for scoring, L2 for geometry)."""

from .._geometry import bbox_is_empty, intersect_rects, join_rect_pair as join_rects
from .._inputs import (
    ANALYZE_FLAGS as FLAGS,
    BAD_CHAR_THRESHOLD,
    BLOCK_IMAGE,
    BLOCK_TEXT,
    BLOCK_VECTOR,
    FEATURE_NAMES,
    GRAY,
    OCR_MODEL_THRESHOLD,
    TESSERACT_FONT_NAME,
    TEXT_FILLED,
    TEXT_STROKED,
    analyze_page,
    check_images,
    compute_features,
    get_layout_locked,
    is_ocr_span,
    predict_ocr_probability,
)
from .._markdown import REPLACEMENT_CHARACTER, TYPE3_FONT_NAME

__all__ = [
    "BAD_CHAR_THRESHOLD",
    "BLOCK_IMAGE",
    "BLOCK_TEXT",
    "BLOCK_VECTOR",
    "FEATURE_NAMES",
    "FLAGS",
    "GRAY",
    "OCR_MODEL_THRESHOLD",
    "REPLACEMENT_CHARACTER",
    "TESSERACT_FONT_NAME",
    "TEXT_FILLED",
    "TEXT_STROKED",
    "TYPE3_FONT_NAME",
    "analyze_page",
    "bbox_is_empty",
    "check_images",
    "compute_features",
    "get_layout_locked",
    "intersect_rects",
    "is_ocr_span",
    "join_rects",
    "predict_ocr_probability",
]
