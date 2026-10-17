# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Column detection for multi-column pages (lane L2, blanks from L1)."""

from .._geometry import are_disjoint, bbox_is_empty, column_boxes, intersect_rects
from .._markdown import WHITE_CHARS

__all__ = [
    "WHITE_CHARS",
    "are_disjoint",
    "bbox_is_empty",
    "column_boxes",
    "intersect_rects",
]
