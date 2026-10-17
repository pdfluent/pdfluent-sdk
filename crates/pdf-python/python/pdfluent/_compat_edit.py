"""Compatible page editing (lane L4): every name this lane registers in _native.

The lane appends each new name to _COMPAT_NAMES; ``pdfluent.__init__``
re-exports the combined list in fixed lane order.
"""

from __future__ import annotations

_COMPAT_NAMES: list[str] = [
    "Annot",
    "Font",
    "Shape",
    "TextWriter",
    "Widget",
]
