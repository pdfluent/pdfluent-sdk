"""Compatible value types (lane P0): every name this lane registers in _native.

The lane appends each new name (e.g. a geometry class) to _COMPAT_NAMES;
``pdfluent.__init__`` re-exports the combined list in fixed lane order.
"""

from __future__ import annotations

_COMPAT_NAMES: list[str] = [
    "Rect",
    "IRect",
    "Point",
    "Matrix",
    "Quad",
    "Identity",
    "FileDataError",
    "EmptyFileError",
    "FileNotFoundError",
    "FitzDeprecation",
    "FzErrorArgument",
    "FzErrorSystem",
    "FzErrorFormat",
]
