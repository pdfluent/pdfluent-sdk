"""xfa-pdf — Enterprise PDF SDK for Python.

Built on a pure-Rust PDF stack via PyO3. Zero system dependencies.

Usage
-----
>>> from xfa_pdf import Document
>>> with Document("invoice.pdf") as doc:
...     print(f"{doc.page_count} pages")
...     for page in doc:
...         img = page.render(dpi=150)
...         img.save(f"page_{page.index}.png")
"""

from xfa_pdf._native import (
    Document,
    Page,
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
    open_pdf,
    merge_pdfs,
    validate_pdfa,
    decrypt_pdf,
)

__all__ = [
    "Document",
    "Page",
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
    "open_pdf",
    "merge_pdfs",
    "validate_pdfa",
    "decrypt_pdf",
]

__version__ = "0.1.0"
