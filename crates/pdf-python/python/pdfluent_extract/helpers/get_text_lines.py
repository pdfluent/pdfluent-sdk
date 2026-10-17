# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Text-line grouping for one page (lanes L1, L2)."""

from .._geometry import almost_in_bbox, are_disjoint, bbox_is_empty
from .._markdown import TYPE3_FONT_NAME, get_raw_lines, get_text_lines, is_white

__all__ = [
    "TYPE3_FONT_NAME",
    "almost_in_bbox",
    "are_disjoint",
    "bbox_is_empty",
    "get_raw_lines",
    "get_text_lines",
    "is_white",
]
