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
    ├── PdfluentLicenseError      — invalid / expired license
    ├── PdfluentGeometryError     — invalid page geometry
    └── PdfluentLimitError        — processing-limit exceeded

License activation
------------------
>>> from pdfluent import activate_license, LicenseInfo
>>> info = activate_license(open("my.license").read())
>>> print(info.tier, info.seats)
"""

from __future__ import annotations

import base64
import json
import os
from dataclasses import dataclass
from typing import Optional

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
    PdfluentLicenseError,
    PdfluentGeometryError,
    PdfluentLimitError,
)


@dataclass
class LicenseInfo:
    """Validated license information returned by :func:`activate_license`.

    Attributes
    ----------
    licensee:
        Name of the license holder.
    company:
        Company or organisation name.
    tier:
        License tier string: ``"trial"``, ``"basic"``, ``"professional"``,
        ``"enterprise"``, or ``"archival"``.
    expires_at:
        Unix timestamp (seconds) at which the license expires.
        ``0`` indicates no expiry (perpetual license).
    seats:
        Number of concurrent developer seats.
    """

    licensee: str
    company: str
    tier: str
    expires_at: int
    seats: int


def activate_license(license_key: str) -> LicenseInfo:
    """Activate a PDFluent license and return the validated license information.

    The key may be supplied as:

    - A JSON string (the raw license file contents).
    - A base64-encoded JSON string (as distributed in ``PDFLUENT_LICENSE_KEY``).
    - A file path — if ``license_key`` ends with ``.json`` or ``.license`` and
      the path exists, the file is read automatically.

    The environment variable ``PDFLUENT_LICENSE_KEY`` is checked first when
    this function is called without an argument (pass an empty string to skip
    the env check and raise immediately).

    Parameters
    ----------
    license_key:
        Raw license JSON, base64-encoded JSON, or a path to a license file.

    Returns
    -------
    LicenseInfo
        The parsed and (format-)validated license payload.

    Raises
    ------
    PdfluentLicenseError
        If the key is empty, malformed, or cannot be parsed.
    """
    if not license_key:
        # Fall back to environment variable
        env_key = os.environ.get("PDFLUENT_LICENSE_KEY", "")
        if not env_key:
            raise PdfluentLicenseError(
                "license_key is empty and PDFLUENT_LICENSE_KEY is not set"
            )
        license_key = env_key

    # File path shortcut
    if license_key.endswith((".json", ".license")) and os.path.isfile(license_key):
        try:
            with open(license_key, encoding="utf-8") as f:
                license_key = f.read()
        except OSError as exc:
            raise PdfluentLicenseError(f"cannot read license file: {exc}") from exc

    # Parse: JSON directly or base64-encoded JSON
    try:
        if license_key.lstrip().startswith("{"):
            payload: dict = json.loads(license_key)
        else:
            decoded = base64.b64decode(license_key.strip())
            payload = json.loads(decoded)
    except Exception as exc:
        raise PdfluentLicenseError(f"malformed license key: {exc}") from exc

    try:
        return LicenseInfo(
            licensee=str(payload.get("licensee", "")),
            company=str(payload.get("company", "")),
            tier=str(payload.get("tier", "trial")),
            expires_at=int(payload.get("expires_at", 0)),
            seats=int(payload.get("seats", 1)),
        )
    except (KeyError, TypeError, ValueError) as exc:
        raise PdfluentLicenseError(f"invalid license payload: {exc}") from exc


__all__ = [
    # Core classes
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
    # License
    "LicenseInfo",
    # Functions
    "open_pdf",
    "merge_pdfs",
    "validate_pdfa",
    "decrypt_pdf",
    "activate_license",
    # Exception hierarchy
    "PdfluentError",
    "PdfluentParseError",
    "PdfluentValidationError",
    "PdfluentRenderError",
    "PdfluentEncryptedError",
    "PdfluentPageRangeError",
    "PdfluentIoError",
    "PdfluentLicenseError",
    "PdfluentGeometryError",
    "PdfluentLimitError",
]

try:
    from importlib.metadata import version as _pkg_version
    __version__ = _pkg_version("pdfluent")
except Exception:
    __version__ = "1.0.0b5"
