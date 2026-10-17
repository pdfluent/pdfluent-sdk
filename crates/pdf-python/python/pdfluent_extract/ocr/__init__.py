# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Recogniser selection and page scoring (implemented in lane L6)."""

from .._inputs import OCRMode
from . import analyze_page, compute_ocr_features

__all__ = ["OCRMode", "analyze_page", "compute_ocr_features"]
