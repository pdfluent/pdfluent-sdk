"""Compatible text extraction (lane L1): every name this lane registers in _native.

The lane appends each new name to _COMPAT_NAMES; ``pdfluent.__init__``
re-exports the combined list in fixed lane order.
"""

from __future__ import annotations

_COMPAT_NAMES: list[str] = [
    # Owned classes.
    "TextPage",
    "TableFinder",
    "Table",
    "TableCell",
    # Module functions.
    "get_text",
    "get_text_length",
    "canon",
    "chartocanon",
    "glyph_name_to_unicode",
    "unicode_to_glyph_name",
    "make_table",
    "css_for_font",
    "get_tessdata",
    "on_highlight_char",
    "recover_bbox_quad",
    "recover_char_quad",
    "recover_line_quad",
    "recover_quad",
    "recover_span_quad",
    "CheckFont",
    "CheckFontInfo",
    "UpdateFontInfo",
    "repair_mono_font",
    "Page__add_text_marker",
    "css_for_pymupdf_font",
    # Extraction flags.
    "TEXT_DEHYPHENATE",
    "TEXT_INHIBIT_SPACES",
    "TEXTFLAGS_TEXT",
    "TEXTFLAGS_SEARCH",
]
