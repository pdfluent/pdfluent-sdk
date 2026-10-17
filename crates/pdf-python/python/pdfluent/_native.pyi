"""Type stubs for ``pdfluent._native`` (the compiled Rust extension).

Consumers should import from ``pdfluent`` directly — these stubs exist so that
mypy can resolve types when the native extension is not yet built.

All public symbols here are re-exported by ``pdfluent.__init__``.
"""

from __future__ import annotations


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
    """Raised when a page cannot be rendered."""

class PdfluentEncryptedError(PdfluentError):
    """Raised when an operation is blocked by PDF encryption."""

class PdfluentPageRangeError(PdfluentError):
    """Raised when a page index is out of range."""

class PdfluentIoError(PdfluentError):
    """Raised on file-system I/O errors."""

class PdfluentGeometryError(PdfluentError):
    """Raised when a page has an invalid or unsupported geometry."""

class PdfluentLimitError(PdfluentError):
    """Raised when a processing limit (page count, file size, etc.) is exceeded."""

# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

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

def extract_office(
    source: Union[str, bytes], format: str = "markdown", name: str = "document"
) -> str:
    ...

def render_markdown(blocks: List[Block]) -> str:
    """Render blocks as markdown, the module-level renderer."""
    ...

def page_spans(doc: Document, page: int) -> List[TextSpan]:
    """Every span of one page, hidden ones included, in draw order."""
    ...

def page_images(doc: Document, page: int) -> List[List[float]]:
    """Image boxes of one page in PDF user space, y up, as ``[x0, y0, x1, y1]``."""
    ...

def ocr_engine_available() -> bool:
    """Whether an OCR engine is available to this build."""
    ...

# ---------------------------------------------------------------------------
# Door bridge (L3): read off the top level by pdfluent_extract
# ---------------------------------------------------------------------------

def door_file_metadata(doc: Document) -> dict[str, object>:
    """File-level metadata record for the door's JSON output."""
    ...

def door_toc(doc: Document) -> list[list[object]]:
    """Outline rows as [level, title, page] triples."""
    ...

def door_form_details(doc: Document) -> list[dict[str, object]]:
    """Key/value form-field records."""
    ...

def door_page_drawings(doc: Document, page: int) -> dict[str, object]:
    """One page's images, rulings, fills and graphic clusters."""
    ...

def door_page_links(doc: Document, page: int) -> list[dict[str, object]]:
    """One page's links as plain dicts."""
    ...

def door_page_tables(doc: Document, page: int) -> list[dict[str, object]]:
    """One page's table proposals as plain dicts."""
    ...

def door_page_spans(page: Page) -> list[tuple[str, float, float, float, list[list[float]], bool]]:
    """One page's spans with per-glyph advance boxes."""
    ...

def door_page_size(page: Page) -> tuple[float, float]:
    """One page's exact size in points as (width, height)."""
    ...

def door_page_span_geometry(page: Page) -> list[tuple[str, float, float, float, Optional[float], Optional[float], bool]]:
    """One page's spans with tight-geometry boxes in extraction order."""
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
        self,
        source: Union[str, bytes, None] = None,
        password: Optional[str] = None,
        *,
        stream: Optional[bytes] = None,
        filetype: Optional[str] = None,
        rect: Optional[object] = None,
        width: float = 0.0,
        height: float = 0.0,
    ) -> None: ...

    @property
    def page_count(self) -> int: ...

    @property
    def metadata(self) -> DocumentInfo: ...

    @property
    def bookmarks(self) -> List[Bookmark]: ...

    @overload
    def __getitem__(self, index: int) -> Page: ...
    @overload
    def __getitem__(self, index: slice) -> List[Page]: ...
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
    def extract_markdown(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        pages: Optional[List[int]] = None,
        include_headers: bool = False,
        include_footers: bool = False,
        page_separators: str = "comment",
        code: bool = True,
        quality: str = "fast",
        model_dir: Optional[str] = None,
        table_model_dir: Optional[str] = None,
        keep_furniture: bool = False,
        ocr: str = "never",
        ocr_language: str = "eng",
        ocr_dpi: int = 150,
        ocr_function: Optional[Callable[..., object]] = None,
    ) -> str: ...
    def extract_docling(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        name: str = "document",
        pages: Optional[List[int]] = None,
    ) -> str: ...
    def extract_blocks(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        pages: Optional[List[int]] = None,
        quality: str = "fast",
        model_dir: Optional[str] = None,
        table_model_dir: Optional[str] = None,
    ) -> List[Block]: ...
    def extract_words(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        pages: Optional[List[int]] = None,
    ) -> List[Word]: ...
    def vision_advice(self) -> List[VisionAdvice]: ...
    def save(
        self,
        filename: str,
        garbage: int = 3,
        clean: int = 0,
        deflate: int = 1,
        deflate_images: int = 1,
        deflate_fonts: int = 1,
        incremental: int = 0,
        ascii: int = 0,
        expand: int = 0,
        linear: int = 0,
        no_new_id: int = 0,
        appearance: int = 0,
        pretty: int = 0,
        encryption: int = 1,
        permissions: int = 4095,
        owner_pw: Optional[str] = None,
        user_pw: Optional[str] = None,
        preserve_metadata: int = 1,
        use_objstms: int = 0,
        compression_effort: int = 0,
        raise_on_repair: int = 0,
        reproducible: int = 0,
    ) -> None: ...
    def validate_signatures(self) -> List[SignatureResult]: ...
    def verify_signatures(self) -> List[SignatureResult]: ...
    def signatures(self) -> List[SignatureResult]: ...
    def get_form_fields(self) -> List[FormField]: ...
    def set_form_field(self, name: str, value: str) -> bool: ...
    def set_multi_select(self, name: str, values: List[str]) -> bool: ...
    # Deprecated alias for set_multi_select (removed in 1.0.0).
    def set_form_field_multi(self, name: str, values: List[str]) -> bool: ...
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

    def page_images(self, page: int) -> List[Dict[str, object]]:
        """Stored images on one page (0-based), as plain dicts."""
        ...

    def extract_image_data(self, image_id: int) -> Dict[str, object]:
        """One stored image's encoded bytes with its header facts."""
        ...

    def decode_image_pixels(self, image_id: int) -> Dict[str, object]:
        """One stored image decoded to raw samples, top row first."""
        ...

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
    def words(self) -> List[Word]: ...
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

class VisionAdvice:
    """Why one page may be worth sending to a vision model (#624)."""

    @property
    def page(self) -> int: ...

    @property
    def needs_vision(self) -> bool: ...

    @property
    def no_text_layer(self) -> bool: ...

    @property
    def low_confidence_table(self) -> bool: ...

    @property
    def figure(self) -> bool: ...

class Block:
    """One block of a document's structure: a paragraph, heading, list item,
    table cell or running head (#573).

    The attributes that belong to the block's own kind carry a value; the rest
    are ``None``.
    """

    @property
    def kind(self) -> str: ...

    @property
    def text(self) -> str: ...

    @property
    def page(self) -> int: ...

    @property
    def bbox(self) -> List[float]: ...

    @property
    def index(self) -> int: ...

    @property
    def continues_previous(self) -> bool: ...

    @property
    def confidence(self) -> Optional[float]: ...

    @property
    def grid(self) -> Optional[Tuple[int, float, int, int, int]]: ...

    @property
    def level(self) -> Optional[int]: ...

    @property
    def ordinal(self) -> Optional[int]: ...

    @property
    def row(self) -> Optional[int]: ...

    @property
    def column(self) -> Optional[int]: ...

    @property
    def row_span(self) -> Optional[int]: ...

    @property
    def column_span(self) -> Optional[int]: ...

    @property
    def header(self) -> Optional[bool]: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class Word:
    """One word of a document's text, with the box it was drawn in."""

    @property
    def text(self) -> str: ...

    @property
    def page(self) -> int: ...

    @property
    def bbox(self) -> List[float]: ...

    @property
    def block(self) -> int: ...

    @property
    def line(self) -> int: ...

    @property
    def word(self) -> int: ...

    @property
    def dir(self) -> float: ...

    @property
    def bold(self) -> bool: ...

    @property
    def italic(self) -> bool: ...

    @property
    def font_size(self) -> float: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class TextBlock:
    """A block of text from a page (grouped by vertical proximity)."""

    @property
    def text(self) -> str: ...

    @property
    def spans(self) -> List[TextSpan]: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class TextSpan:

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
        ...

    @property
    def is_bold(self) -> Optional[bool]:
        ...

    @property
    def is_italic(self) -> Optional[bool]:
        ...

    @property
    def color(self) -> Optional[Tuple[float, float, float]]:
        """Foreground color (r, g, b) in 0.0–1.0, or ``None`` (G1)."""
        ...

    @property
    def flags(self) -> int:
        """Style and layout bits: 16 bold, 2 italic, 8 monospace, 32 hidden, 64 non-horizontal."""
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

    @property
    def creation_date(self) -> Optional[str]: ...

    @property
    def mod_date(self) -> Optional[str]: ...

    @property
    def format(self) -> Optional[str]: ...

    @property
    def encryption(self) -> Optional[str]: ...

    @property
    def trapped(self) -> Optional[str]: ...

    def __getitem__(self, key: str) -> object: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[str]: ...
    def __contains__(self, key: str) -> bool: ...
    def keys(self) -> List[str]: ...
    def get(self, key: str, default: object = None) -> object: ...

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

# ---------------------------------------------------------------------------
# SignatureResult
# ---------------------------------------------------------------------------

class SignatureResult:
    """The validation result for a single digital signature."""

    @property
    def status(self) -> str:
        """Validation status: ``"valid"``, ``"invalid"``, or ``"unknown"``."""
        ...

    @property
    def reason(self) -> Optional[str]:
        """Reason for an ``"invalid"`` / ``"unknown"`` status, else ``None``."""
        ...

    @property
    def field_name(self) -> str:
        """Fully qualified signature field name."""
        ...

    @property
    def signer(self) -> Optional[str]:
        """Signer common name (from the certificate), if available."""
        ...

    @property
    def timestamp(self) -> Optional[str]:
        """Signing timestamp as a string, if available."""
        ...

    def __repr__(self) -> str: ...
