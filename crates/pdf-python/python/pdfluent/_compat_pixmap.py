"""Compatible images and rasters (lane L3): every name this lane registers in _native.

The lane appends each new name to _COMPAT_NAMES; ``pdfluent.__init__``
re-exports the combined list in fixed lane order.
"""

from __future__ import annotations

_COMPAT_NAMES: list[str] = [
    "Pixmap",
    "Colorspace",
    "CS_CMYK",
    "CS_GRAY",
    "CS_RGB",
    "csCMYK",
    "csGRAY",
    "csRGB",
    "FZ_RECOMPRESS_FAX",
    "FZ_RECOMPRESS_J2K",
    "FZ_RECOMPRESS_JPEG",
    "FZ_RECOMPRESS_LOSSLESS",
    "FZ_RECOMPRESS_NEVER",
    "FZ_RECOMPRESS_SAME",
    "FZ_SUBSAMPLE_AVERAGE",
    "FZ_SUBSAMPLE_BICUBIC",
    "calc_image_matrix",
    "image_profile",
]
