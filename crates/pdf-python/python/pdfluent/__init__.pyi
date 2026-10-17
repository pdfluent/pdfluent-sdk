"""Type stubs for the pdfluent public API.

These stubs cover every symbol re-exported from ``pdfluent.__init__`` and are
designed to pass ``mypy --strict``.  They are hand-written (not auto-generated)
to ensure G1 font-metadata fields and the exception hierarchy are typed
precisely.
"""

from __future__ import annotations

from dataclasses import dataclass

__version__: str

# ---------------------------------------------------------------------------
# Exception hierarchy
# ---------------------------------------------------------------------------

class PdfluentError(Exception):
    """Base exception for all PDFluent errors.

    Catch this class to handle any library-specific error::

        try:
            doc = Document("broken.pdf")
        except PdfluentError as exc:
            print(f"PDF error: {exc}")
    """

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
    """Open a PDF from a file path, returning a Document.

    Parameters
    ----------
    path:
        File-system path to the PDF.
    password:
        Password for encrypted PDFs.

    Raises
    ------
    PdfluentParseError
        If the file is not a valid PDF.
    PdfluentEncryptedError
        If the PDF is encrypted and no password is provided.
    PdfluentIoError
        If the file cannot be read.
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

    Raises
    ------
    PdfluentValidationError
        If ``input_paths`` is empty.
    PdfluentError
        On merge failures.
    PdfluentIoError
        If any input cannot be read or the output cannot be written.
    """
    ...

def validate_pdfa(path: str) -> ComplianceReport:
    """Validate a PDF file against PDF/A conformance requirements.

    Auto-detects the declared PDF/A level from XMP metadata.
    Falls back to PDF/A-2B if no level is declared.

    Parameters
    ----------
    path:
        Path to the PDF file to validate.

    Raises
    ------
    PdfluentParseError
        If the file is not a valid PDF.
    PdfluentIoError
        If the file cannot be read.
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

    Raises
    ------
    PdfluentEncryptedError
        If the password is incorrect.
    PdfluentIoError
        On read/write failures.
    """
    ...

def extract_office(
    source: Union[str, bytes], format: str = "markdown", name: str = "document"
) -> str:
    """Extract an Office file (``.docx``, ``.pptx``, ``.xlsx``) to Markdown or Docling JSON.

    The reverse of :meth:`Document.to_docx` and friends: an Office file in,
    the same structured output the engine produces for PDFs out. Routing is
    by the container's content types, not by the file extension.

    Parameters
    ----------
    source:
        File path (str) or raw file bytes.
    format:
        ``"markdown"`` (default) for the engine's Markdown, ``"docling"``
        for a Docling ``DoclingDocument`` as JSON.
    name:
        ``DoclingDocument.name`` for ``format="docling"``; ignored otherwise.

    Raises
    ------
    PdfluentParseError
        If the bytes are not an Office file, or the Office file is broken.
    PdfluentValidationError
        If ``format`` is neither ``"markdown"`` nor ``"docling"``.
    PdfluentIoError
        If a file path is given and the file cannot be read.
    """
    ...

def render_markdown(blocks: List[Block]) -> str:
    """Render blocks as markdown, the module-level renderer.

    Takes the blocks :meth:`Document.extract_blocks` hands back and
    returns the same markdown :meth:`Document.extract_markdown` would
    write for them.
    """
    ...

# ---------------------------------------------------------------------------
# Document
# ---------------------------------------------------------------------------

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

    Raises
    ------
    PdfluentParseError
        If ``source`` is not a valid PDF.
    PdfluentEncryptedError
        If the PDF is encrypted and no password is provided.
    PdfluentIoError
        If a file path is given and the file cannot be read.

    Examples
    --------
    >>> with Document("invoice.pdf") as doc:
    ...     print(doc.page_count)
    ...     img = doc[0].render()
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
    def page_count(self) -> int:
        """Number of pages."""
        ...

    @property
    def metadata(self) -> DocumentInfo:
        """Document metadata (title, author, …)."""
        ...

    @property
    def bookmarks(self) -> List[Bookmark]:
        """Document outline / bookmarks."""
        ...

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

    def render_all(self, dpi: float = 150.0) -> List[RenderedImage]:
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
    ) -> str:
        """The document as markdown, in reading order.

        Headings become ``#`` to ``###``, list items ``-`` and ``1.``, a table
        a pipe table or an HTML one where markdown cannot hold its shape, and a
        change of page an HTML comment. The running heads a document repeats
        are not in it unless ``include_headers`` or ``include_footers`` is
        set; they are in :meth:`extract_blocks`.

        Parameters
        ----------
        first_page:
            First page of the window, counted from zero.
        page_count:
            How many pages from ``first_page``; ``None`` for the rest of the
            document.
        pages:
            An explicit list of 0-based page indices instead of the window.
            Blocks come back in reading order whatever order lists them.
        include_headers:
            Write the running heads as plain paragraphs at their
            reading-order position. Default ``False``.
        include_footers:
            Write the page numbers and running footers as plain paragraphs
            at their reading-order position. Default ``False``.
        page_separators:
            ``"comment"`` (default: ``<!-- page N -->`` where the page
            changes) or ``"none"``: no marker, paragraphs separated by a
            blank line.
        quality:
            ``"fast"`` (default: the shipping engine) or ``"balanced"``
            (the layout model tier); anything else raises
            :class:`PdfluentValidationError` naming the value.
        model_dir:
            Where the balanced tier looks for its weights; ``None``
            resolves through ``PDFLUENT_MODEL_DIR`` to the per-user model
            dir. Read only with ``quality="balanced"``.
        table_model_dir:
            Where the TATR table model lives: a directory opts the
            balanced tier's table regions into TATR structure first,
            ``None`` (default) leaves TATR off.
        keep_furniture:
            Keep the balanced tier's running heads and feet as
            paragraphs instead of dropping them. Default ``False``.
            Read only with ``quality="balanced"``.
        """
        ...

    def extract_docling(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        name: str = "document",
        pages: Optional[List[int]] = None,
    ) -> str:
        """The document as a Docling ``DoclingDocument``, as JSON.

        Load it with ``DoclingDocument.model_validate_json(...)`` from
        docling-core; the schema version is pinned by the engine. ``name`` is
        ``DoclingDocument.name``, which Docling fills with the file's stem.
        """
        ...

    def extract_blocks(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        pages: Optional[List[int]] = None,
        quality: str = "fast",
        model_dir: Optional[str] = None,
        table_model_dir: Optional[str] = None,
    ) -> List[Block]:
        """The document's blocks, in reading order.

        One :class:`Block` per paragraph, heading, list item, table cell and
        running head, each with the page and the box it was drawn in.

        Takes the same ``quality``, ``model_dir`` and ``table_model_dir``
        as :meth:`extract_markdown`, but the balanced tier assembles
        markdown: ``quality="balanced"`` raises the engine's markdown-only
        error.
        """
        ...

    def extract_words(
        self,
        first_page: int = 0,
        page_count: Optional[int] = None,
        pages: Optional[List[int]] = None,
    ) -> List[Word]:
        """Every word of the document, with the box it was drawn in.

        Preview: output quality is still improving and the API may change in a minor release.

        One :class:`Word` per word, in reading order, block by block and line
        by line. Built from the blocks :meth:`extract_blocks` returns under
        """
        ...

    def vision_advice(self) -> List[VisionAdvice]:
        """Which pages are worth sending to a vision model, and why.

        One :class:`VisionAdvice` per page, in page order. ``needs_vision`` is
        the flag to route on. Only ``no_text_layer`` is measured today -- a page
        that draws ink and yields no text, a scan; ``low_confidence_table`` and
        ``figure`` are always ``False`` until the engine can measure them.
        """
        ...

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
    ) -> None:
        """Save the PDF to a file path.

        If the document has been mutated (form fill, annotations, redactions)
        the mutated state is written; otherwise the original bytes are copied.

        Raises
        ------
        PdfluentIoError
            If the file cannot be written.
        """
        ...

    def validate_signatures(self) -> List[SignatureResult]:
        """Cryptographically validate every digital signature in the document.

        Each signature field is returned as a :class:`SignatureResult` with its
        validation ``status`` (``"valid"``, ``"invalid"``, or ``"unknown"``), an
        optional ``reason``, the ``field_name``, and — when present — the
        ``signer`` common name and signing ``timestamp``.

        A document with no signatures returns an empty list (never raises).
        """
        ...

    def verify_signatures(self) -> List[SignatureResult]:
        """Alias for :meth:`validate_signatures`.

        Mirrors the Rust core ``PdfDocument::verify_signatures`` method name.
        """
        ...

    def signatures(self) -> List[SignatureResult]:
        """Lightweight list of signatures present in the document.

        Metadata only — **no cryptographic validation**. Each entry's
        ``status`` is ``"unknown"`` and ``reason`` is ``None``; use
        :meth:`validate_signatures` for the validated report. Returns an empty
        list when the document has no signatures.
        """
        ...

    def get_form_fields(self) -> List[FormField]:
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

    def set_multi_select(self, name: str, values: List[str]) -> bool:
        """Select multiple options on a multi-select list box.

        Writes ``/V`` as an array of text strings and rebuilds ``/I`` (the
        sorted selected-index cache) to match Adobe Acrobat. Pass an empty
        list to clear the selection.

        Parameters
        ----------
        name:
            Fully-qualified field name of a multi-select list box.
        values:
            Export (or display) values of the options to select; for a
            non-editable list box every value must be in ``/Opt``.

        Returns
        -------
        bool
            ``True`` if the field was found and updated.
        """
        ...

    def set_form_field_multi(self, name: str, values: List[str]) -> bool:
        """Deprecated alias for :meth:`set_multi_select`.

        .. deprecated::
            Use :meth:`set_multi_select` — the canonical cross-language name
            (``setMultiSelect`` in JS/Java/WASM). Kept for backward
            compatibility; will be removed in 1.0.0.
        """
        ...

    def get_annotations(self, page: int) -> List[Annotation]:
        """Return all annotations on the given page (0-based).

        Raises
        ------
        PdfluentPageRangeError
            If ``page`` is out of range.
        """
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

        Raises
        ------
        PdfluentValidationError
            If ``annot_type`` is not supported.
        PdfluentPageRangeError
            If ``page`` is out of range.
        """
        ...

    def redact_text(
        self, search_term: str, page: Optional[int] = None
    ) -> RedactReport:
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

        Raises
        ------
        PdfluentError
            On encryption failures.
        PdfluentIoError
            If the file cannot be written.
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

        Raises
        ------
        PdfluentEncryptedError
            If the password is incorrect.
        PdfluentIoError
            If the file cannot be written.
        """
        ...

# ---------------------------------------------------------------------------
# Page
# ---------------------------------------------------------------------------

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
        """Page width in points.

        Raises
        ------
        PdfluentGeometryError
            If the page geometry is invalid.
        """
        ...

    @property
    def height(self) -> float:
        """Page height in points.

        Raises
        ------
        PdfluentGeometryError
            If the page geometry is invalid.
        """
        ...

    @property
    def rotation(self) -> int:
        """Page rotation in degrees (0, 90, 180, or 270)."""
        ...

    @property
    def geometry(self) -> PageGeometry:
        """Full page geometry (media box, crop box, rotation).

        Raises
        ------
        PdfluentGeometryError
            If the page geometry is invalid.
        """
        ...

    def render(
        self,
        dpi: float = 150.0,
        width: Optional[int] = None,
        height: Optional[int] = None,
        background: Optional[Tuple[float, float, float, float]] = None,
    ) -> RenderedImage:
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

        Raises
        ------
        PdfluentRenderError
            If rendering fails.
        """
        ...

    def thumbnail(self, max_dimension: int = 256) -> RenderedImage:
        """Generate a thumbnail (longest side ≤ max_dimension pixels).

        Raises
        ------
        PdfluentRenderError
            If rendering fails.
        """
        ...

    def extract_text(self) -> str:
        """Extract all text from this page as a string."""
        ...

    def extract_text_blocks(self) -> List[TextBlock]:
        """Extract structured text blocks with position information."""
        ...

    def words(self) -> List[Word]:
        """This page's words, with the box each one was drawn in."""
        ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# RenderedImage
# ---------------------------------------------------------------------------

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

    def to_pil(self) -> object:
        """Convert to a PIL/Pillow Image (requires Pillow).

        Returns
        -------
        PIL.Image.Image
            RGBA image.
        """
        ...

    def to_numpy(self) -> object:
        """Convert to a NumPy array, shape (H, W, 4) uint8 (requires numpy).

        Returns
        -------
        numpy.ndarray
            Shape ``(height, width, 4)``, dtype uint8.
        """
        ...

    def save(self, path: str) -> None:
        """Save to a file via PIL (requires Pillow)."""
        ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Block
# ---------------------------------------------------------------------------

class VisionAdvice:
    """Why one page may be worth sending to a vision model (#624).

    ``needs_vision`` is true when any reason holds.
    """

    @property
    def page(self) -> int:
        """The page, 0-based."""
        ...

    @property
    def needs_vision(self) -> bool:
        """Whether any reason holds."""
        ...

    @property
    def no_text_layer(self) -> bool:
        """The page draws ink but yields no text: a scan, or text drawn as curves."""
        ...

    @property
    def low_confidence_table(self) -> bool:
        """A table the table model is unsure of. Always ``False`` today."""
        ...

    @property
    def figure(self) -> bool:
        """A chart or figure whose content is not text. Always ``False`` today."""
        ...


class Block:
    """One block of a document's structure (#573).

    A paragraph, a heading, a list item, a table cell or a running head. The
    attributes that belong to the block's own ``kind`` carry a value; the rest
    are ``None``.
    """

    @property
    def kind(self) -> str:
        """``paragraph``, ``heading``, ``list_item``, ``table_cell``,
        ``header`` or ``footer``."""
        ...

    @property
    def text(self) -> str:
        """The block's text, the line breaks of the measure taken out."""
        ...

    @property
    def page(self) -> int:
        """0-based page index the block was drawn on."""
        ...

    @property
    def bbox(self) -> List[float]:
        """``[x0, y0, x1, y1]`` in PDF user space, y up."""
        ...

    @property
    def index(self) -> int:
        """Place in the document's reading order, counted from the first block
        of the first page."""
        ...

    @property
    def continues_previous(self) -> bool:
        """Whether the block continues the one before it, because the page
        broke a paragraph in two."""
        ...

    @property
    def confidence(self) -> Optional[float]:
        """How sure the learned model is of ``kind``, between 0 and 1, or None
        where no model spoke about this block.

        None is not low confidence: it means nobody measured this block, so
        code that routes the doubtful cases must treat it as unknown rather
        than as zero."""
        ...

    @property
    def grid(self) -> Optional[Tuple[int, float, int, int, int]]:
        """What the builder had to go on when it found this cell's table, as
        (thin_rows, fill, retried_from, full_rows, columns), or None for a block
        that is not a table cell.

        Five facts and not a score, so each can be weighed on its own. A property
        of the table: every cell of one table returns the same tuple."""
        ...

    @property
    def level(self) -> Optional[int]:
        """A heading's level (1, 2 or 3) or a list item's nesting depth."""
        ...

    @property
    def ordinal(self) -> Optional[int]:
        """The number the document wrote on a list item, or ``None`` for a
        bullet."""
        ...

    @property
    def row(self) -> Optional[int]:
        """A table cell's 0-based row."""
        ...

    @property
    def column(self) -> Optional[int]:
        """A table cell's 0-based column, counted across the whole table."""
        ...

    @property
    def row_span(self) -> Optional[int]:
        """How many rows a table cell covers."""
        ...

    @property
    def column_span(self) -> Optional[int]:
        """How many columns a table cell covers."""
        ...

    @property
    def header(self) -> Optional[bool]:
        """Whether a table cell is in the table's header row."""
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...


class Word:
    """One word of a document's text, with the box it was drawn in.

    ``block`` is the position in the list :meth:`Document.extract_blocks`
    returns under the same window -- the window's numbering, not the
    document's -- and ``line`` the 0-based line within that block.
    """

    @property
    def text(self) -> str:
        """The word, without surrounding whitespace."""
        ...

    @property
    def page(self) -> int:
        """0-based page index the word was drawn on."""
        ...

    @property
    def bbox(self) -> List[float]:
        """``[x0, y0, x1, y1]`` in PDF user space, y up."""
        ...

    @property
    def block(self) -> int:
        """Position of the word's block in the ``extract_blocks`` answer."""
        ...

    @property
    def line(self) -> int:
        """0-based line within the word's block."""
        ...

    @property
    def word(self) -> int:
        """0-based word within the word's line, in the order the line reads."""
        ...

    @property
    def dir(self) -> float:
        """Baseline angle in degrees, counter-clockwise from +x; 0 upright."""
        ...

    @property
    def bold(self) -> bool:
        """Whether the word's span is set bold."""
        ...

    @property
    def italic(self) -> bool:
        """Whether the word's span is set italic."""
        ...

    @property
    def font_size(self) -> float:
        """Font size of the word's span, in points."""
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

# ---------------------------------------------------------------------------
# TextBlock / TextSpan
# ---------------------------------------------------------------------------

class TextBlock:
    """A block of text from a page (grouped by vertical proximity)."""

    @property
    def text(self) -> str:
        """Concatenated text of all spans in this block."""
        ...

    @property
    def spans(self) -> List[TextSpan]:
        """Individual text spans with position data."""
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class TextSpan:
    """A single text span at a specific position.

    """

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
    def page(self) -> Optional[int]:
        """Target page index (0-based), or None."""
        ...

    @property
    def children(self) -> List[Bookmark]:
        """Child bookmarks."""
        ...

    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# PageGeometry
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# ComplianceIssue / ComplianceReport
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# FormField
# ---------------------------------------------------------------------------

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

# ---------------------------------------------------------------------------
# Annotation
# ---------------------------------------------------------------------------

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
    """The validation result for a single digital signature.

    Returned by :meth:`Document.validate_signatures` (full cryptographic
    validation) and :meth:`Document.signatures` (metadata only, ``status`` is
    always ``"unknown"``).
    """

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
