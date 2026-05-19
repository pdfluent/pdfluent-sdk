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
from typing import NoReturn, Optional

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
    # License activation — wired to the Rust core
    set_license_key as _native_set_license_key,
    native_license_info as _native_license_info,
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


# JSON tier name → canonical Rust tier name used by set_license_key.
#
# The 1.0 evaluation format accepted by the Rust core is "tier:<name>".
# Python license JSON files use a different tier taxonomy, so we map here.
_TIER_MAP: dict[str, str] = {
    "trial": "trial",
    "basic": "developer",
    "professional": "team",
    "enterprise": "enterprise",
    "archival": "business",
}


# Canonical C8 license error codes — mirror the Rust `Error::code()` strings
# defined in `crates/pdfluent/src/error.rs`. The Python wrapper layer
# pre-validation raise-sites map to `E-LICENSE-INVALID` since they cover the
# format / parse / IO failure class that the Rust core would also code as
# `E-LICENSE-INVALID`. Errors that originate inside the Rust core
# (`set_license_key`, capability checks) already carry the correct code via
# `pdfluent_license_err_to_py` in src/lib.rs.
LICENSE_CODE_INVALID = "E-LICENSE-INVALID"
LICENSE_CODE_FEATURE_NOT_IN_TIER = "E-LICENSE-FEATURE-NOT-IN-TIER"
LICENSE_CODE_CAPABILITY_NOT_COMPILED = "E-LICENSE-CAPABILITY-NOT-COMPILED"


def _raise_license_error(
    message: str,
    code: str = LICENSE_CODE_INVALID,
    cause: Optional[BaseException] = None,
) -> NoReturn:
    """Raise ``PdfluentLicenseError`` with a canonical C8 ``code`` attribute attached.

    Sets ``code`` and ``message`` on the exception instance so callers can
    branch on the canonical error class without parsing the human-readable
    message string — matching the Node, WASM, and .NET parity surfaces.
    """
    err = PdfluentLicenseError(message)
    # Set on the instance so ``except PdfluentLicenseError as e: e.code`` works.
    err.code = code  # type: ignore[attr-defined]
    err.message = message  # type: ignore[attr-defined]
    if cause is not None:
        raise err from cause
    raise err


@dataclass
class LicenseInfo:
    """Validated license information returned by :func:`activate_license`.

    Attributes
    ----------
    tier:
        Canonical license tier as reported by the Rust core:
        ``"trial"``, ``"developer"``, ``"team"``, ``"business"``,
        or ``"enterprise"``.
    expires_at:
        Expiration date in ISO 8601 format, or ``None`` for perpetual
        licenses.  Always ``None`` in 1.0 — time-bound keys require the
        signed-payload format shipping in 1.1.
    output_is_marked:
        ``True`` when the Rust core marks output via the ``/Producer``
        metadata field (Trial tier only).
    licensee:
        Name of the license holder (from the JSON payload).
    company:
        Company or organisation name (from the JSON payload).
    seats:
        Number of concurrent developer seats (from the JSON payload).

    Migration note (1.0 → post-1.0)
    --------------------------------
    ``tier`` now reflects the **canonical Rust tier** (e.g. ``"team"``)
    rather than the raw JSON value (e.g. ``"professional"``).
    ``expires_at`` changed from ``int`` (Unix timestamp) to
    ``Optional[str]`` (ISO 8601 / ``None``).
    """

    tier: str
    expires_at: Optional[str]
    output_is_marked: bool
    licensee: str
    company: str
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

    On success the Rust core's process-global tier is set. Re-activating with
    the same tier is idempotent; re-activating with a different tier raises
    :exc:`PdfluentLicenseError` — restart the process to switch tiers.

    Parameters
    ----------
    license_key:
        Raw license JSON, base64-encoded JSON, or a path to a license file.

    Returns
    -------
    LicenseInfo
        License information reflecting the canonical Rust core state.

    Raises
    ------
    PdfluentLicenseError
        If the key is empty, malformed, has an unknown tier, or the Rust
        core rejects it (e.g. conflicts with an already-set tier). The
        raised exception carries a ``code`` attribute holding the canonical
        C8 error code (e.g. ``"E-LICENSE-INVALID"``,
        ``"E-LICENSE-FEATURE-NOT-IN-TIER"``,
        ``"E-LICENSE-CAPABILITY-NOT-COMPILED"``) and a ``message``
        attribute mirroring the human-readable detail.  This matches the
        Node, WASM, and .NET surfaces — callers can branch on the canonical
        failure class without parsing the message string.
    """
    if not license_key:
        # Fall back to environment variable
        env_key = os.environ.get("PDFLUENT_LICENSE_KEY", "")
        if not env_key:
            _raise_license_error(
                "license_key is empty and PDFLUENT_LICENSE_KEY is not set"
            )
        license_key = env_key

    # File path shortcut — if the key looks like a file path, read it.
    # Any OSError (file not found, permission denied, etc.) is reported as
    # "cannot read" rather than falling through to JSON/base64 parsing.
    if license_key.endswith((".json", ".license")):
        try:
            with open(license_key, encoding="utf-8") as f:
                license_key = f.read()
        except OSError as exc:
            _raise_license_error(
                f"cannot read license file: {exc}", cause=exc
            )

    # Parse: JSON directly or base64-encoded JSON
    try:
        if license_key.lstrip().startswith("{"):
            payload: dict = json.loads(license_key)
        else:
            decoded = base64.b64decode(license_key.strip())
            payload = json.loads(decoded)
    except Exception as exc:
        _raise_license_error(f"malformed license key: {exc}", cause=exc)

    # Map JSON tier to canonical Rust tier format
    json_tier = str(payload.get("tier", "trial")).lower()
    rust_tier = _TIER_MAP.get(json_tier)
    if rust_tier is None:
        _raise_license_error(
            f"unknown license tier {json_tier!r}; "
            f"expected one of: {', '.join(_TIER_MAP)}"
        )

    # Activate in the Rust core — raises PdfluentLicenseError on failure.
    # Rust-originated errors already carry .code via pdfluent_license_err_to_py.
    _native_set_license_key(f"tier:{rust_tier}")

    # Read back the canonical state from the Rust core
    native_info = _native_license_info()

    try:
        return LicenseInfo(
            tier=native_info.tier,
            expires_at=native_info.expires_at,
            output_is_marked=native_info.output_is_marked,
            licensee=str(payload.get("licensee", "")),
            company=str(payload.get("company", "")),
            seats=int(payload.get("seats", 1)),
        )
    except (KeyError, TypeError, ValueError) as exc:
        _raise_license_error(f"invalid license payload: {exc}", cause=exc)


def license_status() -> str:
    """Return the current canonical license tier as reported by the Rust core.

    Returns
    -------
    str
        One of ``"trial"``, ``"developer"``, ``"team"``, ``"business"``,
        or ``"enterprise"``. Returns ``"trial"`` when no key has been set.
    """
    return _native_license_info().tier


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
    "license_status",
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
    __version__ = "1.0.0b7"
