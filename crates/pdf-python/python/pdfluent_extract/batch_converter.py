# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Batch conversion entries (implemented in lane L6)."""

from ._inputs import BatchResult, OCRMode, auto_workers, convert_batch, convert_document

__all__ = [
    "BatchResult",
    "OCRMode",
    "auto_workers",
    "convert_batch",
    "convert_document",
]
