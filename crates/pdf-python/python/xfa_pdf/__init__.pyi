"""Type stubs for xfa_pdf."""

from __future__ import annotations

from typing import Iterator, List, Optional, Tuple, Union

__version__: str

def open_pdf(path: str, password: Optional[str] = None) -> "Document":
    """Open a PDF from a file path, returning a Document.

    Parameters
    ----------
    path:
        File-system path to the PDF.
    password:
        Password for encrypted PDFs.
    """
    ...

def merge_pdfs(input_paths: List[str], output_path: str) -> None:
    """Merge multiple PDF files into a single output file.

    Parameters
    ----------
    input_paths:
        Ordered list of PDF paths to merge.
    output_path:
        Destination path for the merged PDF.
    """
    ...

def validate_pdfa(path: str) -> "ComplianceReport":
    """Validate a PDF file against PDF/A conformance requirements.

    Auto-detects the declared PDF/A level from XMP metadata.
    Falls back to PDF/A-2B if no level is declared.

    Parameters
    ----------
    path:
        Path to the PDF file to validate.
    """
    ...

def decrypt_pdf(input_path: str, output_path: str, password: str) -> None:
    """Decrypt a password-protected PDF and write the decrypted copy.

    Parameters
    ----------
    input_path:
        Path to the encrypted PDF.
    output_path:
        Destination path for the decrypted PDF.
    password:
        User or owner password.
    """
    ...

class Document:
    """A PDF document.

    Open from a file path or raw bytes. Supports context manager protocol,
    iteration over pages, and integer indexing.

    Parameters
    ----------
    source:
        File path (str) or raw PDF bytes.
    password:
        Password for encrypted PDFs.

    Examples
    --------
    >>> with Document("invoice.pdf") as doc:
    ...     print(doc.page_count)
    ...     img = doc[0].render()
    """

    def __init__(
        self, source: Union[str, bytes], password: Optional[str] = None
    ) -> None: ...
    @property
    def page_count(self) -> int:
        """Number of pages."""
        ...
    @property
    def metadata(self) -> "DocumentInfo":
        """Document metadata (title, author, …)."""
        ...
    @property
    def bookmarks(self) -> List["Bookmark"]:
        """Document outline / bookmarks."""
        ...
    def __getitem__(self, index: int) -> "Page": ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator["Page"]: ...
    def __enter__(self) -> "Document": ...
    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> bool: ...
    def __repr__(self) -> str: ...
    def render_all(self, dpi: float = 150.0) -> List["RenderedImage"]:
        """Render all pages in parallel.

        Parameters
        ----------
        dpi:
            Resolution (default 150).
        """
        ...
    def search(self, query: str) -> List[int]:
        """Search for text across all pages.

        Returns a list of 0-based page indices containing the query.
        """
        ...
    def extract_text(self, page_num: int) -> str:
        """Extract all text from a specific page (0-based index)."""
        ...
    def save(self, path: str) -> None:
        """Save the PDF to a file path.

        If the document has been mutated (form fill, annotations, redactions)
        the mutated state is written; otherwise the original bytes are copied.
        """
        ...
    def get_form_fields(self) -> List["FormField"]:
        """Return all interactive form fields in the document."""
        ...
    def set_form_field(self, name: str, value: str) -> bool:
        """Set the value of a form field by its fully-qualified name.

        Parameters
        ----------
        name:
            Fully-qualified field name (e.g. ``"Address.Street"``).
        value:
            New text value.

        Returns
        -------
        bool
            ``True`` if the field was found and updated.
        """
        ...
    def get_annotations(self, page: int) -> List["Annotation"]:
        """Return all annotations on the given page (0-based)."""
        ...
    def add_annotation(
        self,
        page: int,
        annot_type: str,
        rect: Tuple[float, float, float, float],
        content: Optional[str] = None,
    ) -> None:
        """Add an annotation to a page.

        Parameters
        ----------
        page:
            0-based page index.
        annot_type:
            ``"highlight"`` or ``"freetext"``.
        rect:
            Bounding box as ``(x0, y0, x1, y1)`` in PDF user-space points.
        content:
            Text content of the annotation.
        """
        ...
    def redact_text(
        self, search_term: str, page: Optional[int] = None
    ) -> "RedactReport":
        """Search for text and redact all occurrences.

        Parameters
        ----------
        search_term:
            Text to search for (literal match, case-insensitive).
        page:
            0-based page index to limit search to. ``None`` searches all pages.
        """
        ...
    def encrypt(
        self, output_path: str, password: str, owner_password: Optional[str] = None
    ) -> None:
        """Save an AES-256 encrypted copy of the document.

        Parameters
        ----------
        output_path:
            Destination file path.
        password:
            User password (required to open).
        owner_password:
            Owner password (for permissions). Defaults to ``password``.
        """
        ...
    def decrypt(self, output_path: str, password: str) -> None:
        """Save a decrypted copy of an encrypted document.

        Parameters
        ----------
        output_path:
            Destination file path for the decrypted PDF.
        password:
            User or owner password.
        """
        ...

class Page:
    """A single page in a PDF document.

    Access via indexing (``doc[0]``) or iteration (``for page in doc``).
    """

    @property
    def index(self) -> int:
        """Page index (0-based)."""
        ...
    @property
    def width(self) -> float:
        """Page width in points."""
        ...
    @property
    def height(self) -> float:
        """Page height in points."""
        ...
    @property
    def rotation(self) -> int:
        """Page rotation in degrees (0, 90, 180, or 270)."""
        ...
    @property
    def geometry(self) -> "PageGeometry":
        """Full page geometry (media box, crop box, rotation)."""
        ...
    def render(
        self,
        dpi: float = 150.0,
        width: Optional[int] = None,
        height: Optional[int] = None,
        background: Optional[Tuple[float, float, float, float]] = None,
    ) -> "RenderedImage":
        """Render this page to a RenderedImage.

        Parameters
        ----------
        dpi:
            Resolution (default 150).
        width:
            Force output width in pixels.
        height:
            Force output height in pixels.
        background:
            RGBA background color (0.0–1.0). Default: opaque white.
        """
        ...
    def thumbnail(self, max_dimension: int = 256) -> "RenderedImage":
        """Generate a thumbnail (longest side ≤ max_dimension pixels)."""
        ...
    def extract_text(self) -> str:
        """Extract all text from this page as a string."""
        ...
    def extract_text_blocks(self) -> List["TextBlock"]:
        """Extract structured text blocks with position information."""
        ...
    def __repr__(self) -> str: ...

class RenderedImage:
    """A rendered page as RGBA pixel data.

    Convert to PIL Image via ``.to_pil()`` or NumPy array via ``.to_numpy()``.
    """

    @property
    def width(self) -> int:
        """Image width in pixels."""
        ...
    @property
    def height(self) -> int:
        """Image height in pixels."""
        ...
    @property
    def pixels(self) -> bytes:
        """Raw RGBA pixel data (4 bytes per pixel, row-major)."""
        ...
    def to_pil(self) -> "PIL.Image.Image":  # type: ignore[name-defined]
        """Convert to a PIL/Pillow Image (requires Pillow)."""
        ...
    def to_numpy(self) -> "numpy.ndarray":  # type: ignore[name-defined]
        """Convert to a NumPy array, shape (H, W, 4) uint8 (requires numpy)."""
        ...
    def save(self, path: str) -> None:
        """Save to a file via PIL (requires Pillow)."""
        ...
    def __repr__(self) -> str: ...

class TextBlock:
    """A block of text from a page (grouped by vertical proximity)."""

    @property
    def text(self) -> str:
        """Concatenated text of all spans in this block."""
        ...
    @property
    def spans(self) -> List["TextSpan"]:
        """Individual text spans with position data."""
        ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class TextSpan:
    """A single text span at a specific position."""

    @property
    def text(self) -> str:
        """The text content."""
        ...
    @property
    def x(self) -> float:
        """X position in PDF user space."""
        ...
    @property
    def y(self) -> float:
        """Y position in PDF user space."""
        ...
    @property
    def font_size(self) -> float:
        """Approximate font size."""
        ...
    def __repr__(self) -> str: ...

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

class Bookmark:
    """A bookmark (outline item) in the document."""

    @property
    def title(self) -> str: ...
    @property
    def page(self) -> Optional[int]:
        """Target page index (0-based), or None."""
        ...
    @property
    def children(self) -> List["Bookmark"]:
        """Child bookmarks."""
        ...
    def __repr__(self) -> str: ...

class PageGeometry:
    """Full page geometry (boxes, rotation)."""

    @property
    def media_box(self) -> Tuple[float, float, float, float]:
        """MediaBox as (x0, y0, x1, y1)."""
        ...
    @property
    def crop_box(self) -> Tuple[float, float, float, float]:
        """CropBox as (x0, y0, x1, y1)."""
        ...
    @property
    def rotation(self) -> int:
        """Rotation in degrees."""
        ...
    @property
    def width(self) -> float:
        """Effective width in points (accounting for rotation)."""
        ...
    @property
    def height(self) -> float:
        """Effective height in points (accounting for rotation)."""
        ...
    def pixel_dimensions(self, dpi: float) -> Tuple[int, int]:
        """Pixel dimensions (width, height) at the given DPI."""
        ...
    def __repr__(self) -> str: ...

class ComplianceIssue:
    """A single compliance issue found during PDF/A validation."""

    @property
    def rule(self) -> str:
        """Rule identifier (e.g. ``"6.1.2"`` for PDF/A clause)."""
        ...
    @property
    def severity(self) -> str:
        """Severity: ``"error"``, ``"warning"``, or ``"info"``."""
        ...
    @property
    def message(self) -> str:
        """Human-readable description of the issue."""
        ...
    @property
    def location(self) -> Optional[str]:
        """Location in the document (object number, page, etc.)."""
        ...
    def __repr__(self) -> str: ...

class ComplianceReport:
    """Result of a PDF/A compliance validation."""

    @property
    def is_compliant(self) -> bool:
        """True if no conformance errors were found."""
        ...
    @property
    def error_count(self) -> int: ...
    @property
    def warning_count(self) -> int: ...
    @property
    def issues(self) -> List[ComplianceIssue]:
        """All issues found during validation."""
        ...
    @property
    def pdfa_level(self) -> Optional[str]:
        """Detected PDF/A level string (e.g. ``"PDF/A-2B"``), or None."""
        ...
    def __repr__(self) -> str: ...

class FormField:
    """An interactive form field (AcroForm widget)."""

    @property
    def name(self) -> str:
        """Fully-qualified field name."""
        ...
    @property
    def field_type(self) -> str:
        """Field type: ``"text"``, ``"button"``, ``"choice"``, or ``"signature"``."""
        ...
    @property
    def value(self) -> Optional[str]:
        """Current field value, or None if empty."""
        ...
    @property
    def page(self) -> Optional[int]:
        """0-based page index the field appears on, or None."""
        ...
    def __repr__(self) -> str: ...

class Annotation:
    """A PDF annotation (highlight, freetext, etc.)."""

    @property
    def page(self) -> int:
        """0-based page index."""
        ...
    @property
    def annot_type(self) -> str:
        """Annotation type string."""
        ...
    @property
    def rect(self) -> Tuple[float, float, float, float]:
        """Bounding box as (x0, y0, x1, y1) in PDF user-space points."""
        ...
    @property
    def contents(self) -> Optional[str]: ...
    @property
    def author(self) -> Optional[str]: ...
    def __repr__(self) -> str: ...

class RedactReport:
    """Result of a search-and-redact operation."""

    @property
    def matches_found(self) -> int: ...
    @property
    def areas_redacted(self) -> int: ...
    @property
    def pages_affected(self) -> int: ...
    def __repr__(self) -> str: ...
