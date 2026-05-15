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
"""

from pdfluent._native import (
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
    LicenseStatus,
    open_pdf,
    merge_pdfs,
    validate_pdfa,
    decrypt_pdf,
    activate_license_key,
    activate_license_file,
    license_status,
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
    "LicenseStatus",
    "open_pdf",
    "merge_pdfs",
    "validate_pdfa",
    "decrypt_pdf",
    "activate_license_key",
    "activate_license_file",
    "license_status",
]

try:
    from importlib.metadata import version as _pkg_version
    __version__ = _pkg_version("pdfluent")
except Exception:
    __version__ = "1.0.0b5"
