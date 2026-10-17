"""pdfluent — Enterprise PDF SDK for Python.

Built on a pure-Rust PDF stack via PyO3. Zero system dependencies.

Usage
-----
>>> from pdfluent import Document
>>> with Document("invoice.pdf") as doc:
...     print(f"{doc.page_count} pages")
...     for page in doc:
...         img = page.render(dpi=150)
...         img.save(f"page_{page.index}.png")

Exception hierarchy
-------------------
All pdfluent-specific errors derive from ``PdfluentError``::

    PdfluentError
    ├── PdfluentParseError        — corrupt / non-PDF bytes
    ├── PdfluentValidationError   — schema / compliance failures
    ├── PdfluentEncryptedError    — operation blocked by encryption
    ├── PdfluentPageRangeError    — page index out of range
    ├── PdfluentIoError           — file-system I/O errors
    ├── PdfluentGeometryError     — invalid page geometry
    └── PdfluentLimitError        — processing-limit exceeded
"""

from __future__ import annotations

from pdfluent._native import (
    Document,
    Page,
    TextEditor,
    RenderedImage,
    TextBlock,
    TextSpan,
    Block,
    Word,
    VisionAdvice,
    DocumentInfo,
    Bookmark,
    PageGeometry,
    ComplianceIssue,
    ComplianceReport,
    FormField,
    Annotation,
    RedactReport,
    SignatureResult,
    open_pdf,
    merge_pdfs,
    validate_pdfa,
    decrypt_pdf,
    extract_office,
    render_markdown,
    page_spans,
    page_images,
    # Door bridge (L3): pdfluent_extract reads these off the top level
    door_file_metadata,
    door_toc,
    door_form_details,
    door_page_drawings,
    door_page_links,
    door_page_tables,
    door_page_spans,
    door_page_size,
    door_page_span_geometry,
    # Exception hierarchy from Rust
    PdfluentError,
    PdfluentParseError,
    PdfluentValidationError,
    PdfluentRenderError,
    PdfluentEncryptedError,
    PdfluentPageRangeError,
    PdfluentIoError,
    PdfluentGeometryError,
    PdfluentLimitError,
)


__all__ = [
    # Core classes
    "Document",
    "Page",
    "TextEditor",
    "RenderedImage",
    "TextBlock",
    "TextSpan",
    "Block",
    "Word",
    "VisionAdvice",
    "DocumentInfo",
    "Bookmark",
    "PageGeometry",
    "ComplianceIssue",
    "ComplianceReport",
    "FormField",
    "Annotation",
    "RedactReport",
    "SignatureResult",
    # Functions
    "open_pdf",
    "merge_pdfs",
    "validate_pdfa",
    "decrypt_pdf",
    "extract_office",
    "render_markdown",
    "page_spans",
    "page_images",
    # Door bridge (L3)
    "door_file_metadata",
    "door_toc",
    "door_form_details",
    "door_page_drawings",
    "door_page_links",
    "door_page_tables",
    "door_page_spans",
    "door_page_size",
    "door_page_span_geometry",
    # Exception hierarchy
    "PdfluentError",
    "PdfluentParseError",
    "PdfluentValidationError",
    "PdfluentRenderError",
    "PdfluentEncryptedError",
    "PdfluentPageRangeError",
    "PdfluentIoError",
    "PdfluentGeometryError",
    "PdfluentLimitError",
]

# ---------------------------------------------------------------------------
# Compatible API names (#643). Each lane owns one _compat_<lane>.py file that
# lists the names it registers in _native; this block re-exports them in the
# fixed lane order below. `from ._native import *` is not allowed: every
# re-exported name is spelled out in its lane file.
# ---------------------------------------------------------------------------
from pdfluent import (
    _compat_geometry,
    _compat_text,
    _compat_document,
    _compat_pixmap,
    _compat_edit,
    _compat_functions,
)
from pdfluent import _native as _native_mod


def _reexport_compat() -> list[str]:
    """Bind every lane's _native names into this module; return the names."""
    names: list[str] = []
    for lane in (
        _compat_geometry,
        _compat_text,
        _compat_document,
        _compat_pixmap,
        _compat_edit,
        _compat_functions,
    ):
        for name in lane._COMPAT_NAMES:
            globals()[name] = getattr(_native_mod, name)
            names.append(name)
    return names


_COMPAT_NAMES: list[str] = _reexport_compat()
del _reexport_compat
__all__ += _COMPAT_NAMES


try:
    from importlib.metadata import version as _pkg_version
    __version__ = _pkg_version("pdfluent")
except Exception:
    __version__ = "1.0.0b7"
