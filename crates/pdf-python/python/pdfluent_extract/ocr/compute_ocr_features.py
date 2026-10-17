# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Edge-based page features for recogniser scoring (lane L6)."""

from .._inputs import (
    FEATURE_NAMES,
    SOBEL_X,
    SOBEL_Y,
    as_strided,
    compute_features,
    conv2d_fast,
    make_ocr_decision,
    select_ocr_function,
    sobel_features,
    sobel_features_page,
)

__all__ = [
    "FEATURE_NAMES",
    "SOBEL_X",
    "SOBEL_Y",
    "as_strided",
    "compute_features",
    "conv2d_fast",
    "make_ocr_decision",
    "select_ocr_function",
    "sobel_features",
    "sobel_features_page",
]
