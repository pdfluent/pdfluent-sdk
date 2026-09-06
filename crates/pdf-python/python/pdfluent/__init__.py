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
    ├── PdfluentRenderError       — rendering and XFA flatten failures
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

try:
    from importlib.metadata import version as _pkg_version
    __version__ = _pkg_version("pdfluent")
except Exception:
    __version__ = "1.0.0b7"
