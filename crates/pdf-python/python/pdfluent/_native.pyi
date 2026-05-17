"""Type stubs for ``pdfluent._native`` (the compiled Rust extension).

Consumers should import from ``pdfluent`` directly — these stubs exist so that
mypy can resolve types when the native extension is not yet built.

All public symbols here are re-exported by ``pdfluent.__init__``.
"""

from __future__ import annotations

from typing import Iterator, List, Optional, Tuple, Type, Union

# ---------------------------------------------------------------------------
# Exception hierarchy
# ---------------------------------------------------------------------------

class PdfluentError(Exception):
    """Base exception for all PDFluent errors."""

class PdfluentParseError(PdfluentError):
    """Raised when a PDF cannot be parsed (corrupt, truncated, or not a PDF)."""

class PdfluentValidationError(PdfluentError):
    """Raised when a document fails schema or compliance validation."""

class PdfluentRenderError(PdfluentError):
    """Raised when page rendering or XFA flattening fails."""

class PdfluentEncryptedError(PdfluentError):
    """Raised when an operation is blocked by PDF encryption."""

class PdfluentPageRangeError(PdfluentError):
    """Raised when a page index is out of range."""

class PdfluentIoError(PdfluentError):
    """Raised on file-system I/O errors."""

class PdfluentLicenseError(PdfluentError):
    """Raised on license validation errors (invalid key, expired, quota exceeded)."""

class PdfluentGeometryError(PdfluentError):
    """Raised when a page has an invalid or unsupported geometry."""

class PdfluentLimitError(PdfluentError):
    """Raised when a processing limit (page count, file size, etc.) is exceeded."""

# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

class _NativeLicenseInfo:
    """Canonical license state snapshot from the Rust core.

    Returned by :func:`native_license_info`. Prefer importing
    :class:`pdfluent.LicenseInfo` from the top-level package.
    """

    @property
    def tier(self) -> str:
        """Canonical tier: ``"trial"``, ``"developer"``, ``"team"``,
        ``"business"``, or ``"enterprise"``."""
        ...

    @property
    def expires_at(self) -> Optional[str]:
        """Expiration in ISO 8601 format, or ``None`` (always ``None`` in 1.0)."""
        ...

    @property
    def output_is_marked(self) -> bool:
        """``True`` when Trial-tier output watermarking is active."""
        ...

    def __repr__(self) -> str: ...

def set_license_key(key: str) -> None:
    """Activate the process-global license key in the Rust core.

    Accepts the simple 1.0 evaluation format ``"tier:<name>"``.
    First call locks the tier; subsequent calls with the same tier are
    idempotent.  A different tier raises :exc:`PdfluentLicenseError`.

    Raises
    ------
    PdfluentLicenseError
        On invalid format or tier conflict.
    """
    ...

def native_license_info() -> _NativeLicenseInfo:
    """Return the current canonical license state from the Rust core."""
    ...

def open_pdf(path: str, password: Optional[str] = None) -> Document:
    """Open a PDF from a file path, returning a ``Document``."""
    ...

def merge_pdfs(input_paths: List[str], output_path: str) -> None:
    """Merge multiple PDF files into a single output file."""
    ...

def validate_pdfa(path: str) -> ComplianceReport:
    """Validate a PDF file against PDF/A conformance requirements."""
    ...

def decrypt_pdf(input_path: str, output_path: str, password: str) -> None:
    """Decrypt a password-protected PDF and write the decrypted copy."""
    ...

# ---------------------------------------------------------------------------
# Document
# ---------------------------------------------------------------------------

class Document:
    """A PDF document.

    Open from a file path or raw bytes. Supports context manager protocol.

    Parameters
    ----------
    source:
        File path (str) or raw PDF bytes.
    password:
        Password for encrypted PDFs.
    """

    def __init__(
        self, source: Union[str, bytes], password: Optional[str] = None
    ) -> None: ...

    @property
    def page_count(self) -> int: ...

    @property
    def metadata(self) -> DocumentInfo: ...

    @property
    def bookmarks(self) -> List[Bookmark]: ...

    def __getitem__(self, index: int) -> Page: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[Page]: ...
    def __enter__(self) -> Document: ...
    def __exit__(
        self,
        exc_type: Optional[Type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: object,
    ) -> bool: ...
    def __repr__(self) -> str: ...

    def render_all(self, dpi: float = 150.0) -> List[RenderedImage]: ...
    def search(self, query: str) -> List[int]: ...
    def extract_text(self, page_num: int) -> str: ...
    def save(self, path: str) -> None: ...
    def get_form_fields(self) -> List[FormField]: ...
    def set_form_field(self, name: str, value: str) -> bool: ...
    def get_annotations(self, page: int) -> List[Annotation]: ...

    def add_annotation(
        self,
        page: int,
        annot_type: str,
        rect: Tuple[float, float, float, float],
        content: Optional[str] = None,
    ) -> None: ...

    def redact_text(
        self, search_term: str, page: Optional[int] = None
    ) -> RedactReport: ...

    def encrypt(
        self,
        output_path: str,
        password: str,
        owner_password: Optional[str] = None,
    ) -> None: ...

    def decrypt(self, output_path: str, password: str) -> None: ...

# ---------------------------------------------------------------------------
# Page
# ---------------------------------------------------------------------------

class Page:
    """A single page in a PDF document."""

    @property
    def index(self) -> int: ...

    @property
    def width(self) -> float: ...

    @property
    def height(self) -> float: ...

    @property
    def rotation(self) -> int: ...

    @property
    def geometry(self) -> PageGeometry: ...

    def render(
        self,
        dpi: float = 150.0,
        width: Optional[int] = None,
        height: Optional[int] = None,
        background: Optional[Tuple[float, float, float, float]] = None,
    ) -> RenderedImage: ...

    def thumbnail(self, max_dimension: int = 256) -> RenderedImage: ...
    def extract_text(self) -> str: ...
    def extract_text_blocks(self) -> List[TextBlock]: ...
    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# RenderedImage
# ---------------------------------------------------------------------------

class RenderedImage:
    """A rendered page as RGBA pixel data."""

    @property
    def width(self) -> int: ...

    @property
    def height(self) -> int: ...

    @property
    def pixels(self) -> bytes: ...

    def to_pil(self) -> object: ...
    def to_numpy(self) -> object: ...
    def save(self, path: str) -> None: ...
    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# TextBlock / TextSpan
# ---------------------------------------------------------------------------

class TextBlock:
    """A block of text from a page (grouped by vertical proximity)."""

    @property
    def text(self) -> str: ...

    @property
    def spans(self) -> List[TextSpan]: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class TextSpan:
    """A single text span at a specific position.

    G1 font-metadata fields return ``None`` until the extraction pipeline is
    upgraded to emit font attributes.
    """

    @property
    def text(self) -> str: ...

    @property
    def x(self) -> float: ...

    @property
    def y(self) -> float: ...

    @property
    def font_size(self) -> float: ...

    @property
    def font_name(self) -> Optional[str]:
        """Font name, or ``None`` if not yet available (G1)."""
        ...

    @property
    def is_bold(self) -> Optional[bool]:
        """``True`` if bold, ``None`` if not yet available (G1)."""
        ...

    @property
    def is_italic(self) -> Optional[bool]:
        """``True`` if italic, ``None`` if not yet available (G1)."""
        ...

    @property
    def color(self) -> Optional[Tuple[float, float, float]]:
        """Foreground color (r, g, b) in 0.0–1.0, or ``None`` (G1)."""
        ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# DocumentInfo
# ---------------------------------------------------------------------------

class DocumentInfo:
    """Document metadata (title, author, subject, etc.)."""

    @property
    def title(self) -> Optional[str]: ...

    @property
    def author(self) -> Optional[str]: ...

    @property
    def subject(self) -> Optional[str]: ...

    @property
    def keywords(self) -> Optional[str]: ...

    @property
    def creator(self) -> Optional[str]: ...

    @property
    def producer(self) -> Optional[str]: ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Bookmark
# ---------------------------------------------------------------------------

class Bookmark:
    """A bookmark (outline item) in the document."""

    @property
    def title(self) -> str: ...

    @property
    def page(self) -> Optional[int]: ...

    @property
    def children(self) -> List[Bookmark]: ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# PageGeometry
# ---------------------------------------------------------------------------

class PageGeometry:
    """Full page geometry (boxes, rotation)."""

    @property
    def media_box(self) -> Tuple[float, float, float, float]: ...

    @property
    def crop_box(self) -> Tuple[float, float, float, float]: ...

    @property
    def rotation(self) -> int: ...

    @property
    def width(self) -> float: ...

    @property
    def height(self) -> float: ...

    def pixel_dimensions(self, dpi: float) -> Tuple[int, int]: ...
    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# ComplianceIssue / ComplianceReport
# ---------------------------------------------------------------------------

class ComplianceIssue:
    """A single compliance issue found during PDF/A validation."""

    @property
    def rule(self) -> str: ...

    @property
    def severity(self) -> str: ...

    @property
    def message(self) -> str: ...

    @property
    def location(self) -> Optional[str]: ...

    def __repr__(self) -> str: ...

class ComplianceReport:
    """Result of a PDF/A compliance validation."""

    @property
    def is_compliant(self) -> bool: ...

    @property
    def error_count(self) -> int: ...

    @property
    def warning_count(self) -> int: ...

    @property
    def issues(self) -> List[ComplianceIssue]: ...

    @property
    def pdfa_level(self) -> Optional[str]: ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# FormField
# ---------------------------------------------------------------------------

class FormField:
    """An interactive form field (AcroForm widget)."""

    @property
    def name(self) -> str: ...

    @property
    def field_type(self) -> str: ...

    @property
    def value(self) -> Optional[str]: ...

    @property
    def page(self) -> Optional[int]: ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Annotation
# ---------------------------------------------------------------------------

class Annotation:
    """A PDF annotation (highlight, freetext, etc.)."""

    @property
    def page(self) -> int: ...

    @property
    def annot_type(self) -> str: ...

    @property
    def rect(self) -> Tuple[float, float, float, float]: ...

    @property
    def contents(self) -> Optional[str]: ...

    @property
    def author(self) -> Optional[str]: ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# RedactReport
# ---------------------------------------------------------------------------

class RedactReport:
    """Result of a search-and-redact operation."""

    @property
    def matches_found(self) -> int: ...

    @property
    def areas_redacted(self) -> int: ...

    @property
    def pages_affected(self) -> int: ...

    def __repr__(self) -> str: ...
