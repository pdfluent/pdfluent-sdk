//! Python bindings for the PDF engine (via PyO3).
//!
//! Exposes `Document`, `Page`, `RenderedImage`, and supporting types
//! as a native Python module `pdfengine._native`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use lopdf::{Document as LopdfDocument, Object as LopdfObject, Permissions as LopdfPermissions};

use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};
use pdf_annot::Annotation;
use pdf_forms::{
    apply_choice_multi, apply_field_value, parse_acroform, FieldType, FieldValue, WriteOutcome,
    WriteValue, WritebackError,
};
use pdf_manip::encrypt::remove_encryption;
use pdf_redact::{search_and_redact, RedactSearchOptions};
use pdf_sign::{signature_fields, validate_signatures, ValidationStatus};

use pyo3::prelude::*;
use pyo3::types::PyBytes;

mod compat;
mod ocr_callback;
mod office;
mod text_edit;
// -- package slot modules (one line each; edit only your own line) --
mod door_chunks;
mod door_geometry;
mod door_images;
mod door_inputs;
mod door_markdown;
mod door_tables;
mod slot_compat;
mod slot_words;
// -- end package slot modules --

use pdf_compliance::{detect_pdfa_level, validate_pdfa as compliance_validate_pdfa, PdfALevel};
use pdf_manip::pages;
use pdf_syntax::Pdf;

use pdf_engine::model::ModelErrorKind;
use pdf_engine::{
    Block, BlockKind, BookmarkItem, EngineError, ExtractOptions, OcrMode, PageGeometry,
    PageSeparators, PdfDocument, Quality, RenderOptions, RenderedPage, TatrOptions, TextBlock,
    TextSpan, ThumbnailOptions, Word,
};

// ---------------------------------------------------------------------------
// Exception hierarchy
// ---------------------------------------------------------------------------

pyo3::create_exception!(
    pdfluent,
    PdfluentError,
    pyo3::exceptions::PyException,
    "Base exception for all PDFluent errors."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentParseError,
    PdfluentError,
    "Raised when a PDF cannot be parsed (corrupt, truncated, or not a PDF)."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentValidationError,
    PdfluentError,
    "Raised when a document fails schema or compliance validation."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentRenderError,
    PdfluentError,
    "Raised when page rendering or XFA flattening fails."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentEncryptedError,
    PdfluentError,
    "Raised when an operation is blocked by PDF encryption."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentPageRangeError,
    PdfluentError,
    "Raised when a page index is out of range."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentIoError,
    PdfluentError,
    "Raised on file-system I/O errors."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentGeometryError,
    PdfluentError,
    "Raised when a page has an invalid or unsupported geometry."
);
pyo3::create_exception!(
    pdfluent,
    PdfluentLimitError,
    PdfluentError,
    "Raised when a processing limit (page count, file size, etc.) is exceeded."
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn manip_err_to_py(e: pdf_manip::error::ManipError) -> PyErr {
    PdfluentError::new_err(e.to_string())
}

/// Apply a string value with type-aware dispatch, mirroring the CLI's
/// `fill_one` (crates/xfa-cli/src/cmd_fill.rs): try Text first (the common
/// case), then on a `/FT` type mismatch fall through to Radio, Choice, and
/// finally Checkbox with a bool-ish string.
///
/// All writes go through [`pdf_forms::apply_field_value`] — the single SDK
/// writeback chain — keeping `/V` encoding, per-widget `/AS`, and `/AP`
/// regeneration consistent.
fn apply_string_value(
    doc: &mut LopdfDocument,
    name: &str,
    value: &str,
) -> Result<WriteOutcome, WritebackError> {
    match apply_field_value(doc, name, WriteValue::Text(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Radio(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Choice(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    // Checkbox via bool-ish string ("true"/"Yes"/"Off"/"false").
    let on = !matches!(value, "false" | "Off" | "0" | "");
    apply_field_value(doc, name, WriteValue::Checkbox(on))
}

pub(crate) fn engine_err_to_py(e: EngineError) -> PyErr {
    // An OptionsError rides inside EngineError::Io: checked first, before
    // the generic Io mapping below, or a mistake in the call reads as I/O.
    if e.options().is_some() {
        return PdfluentValidationError::new_err(e.to_string());
    }
    // Recognition runs on a render, so its failures land here; the message
    // names the cause (the missing binary, the language, ...). OCR failures
    // ride as `Io` carrying a typed cause, read through `ocr_message`.
    if let Some(message) = e.ocr_message() {
        return PdfluentRenderError::new_err(format!("OCR error: {message}"));
    }
    // The table-structure weights are missing: the stage's own install text
    // names a manual route, but the wheel's users install through the
    // product command, so the message names it instead. Every other model
    // failure -- the layout tier included -- already names its own
    // installer and passes through below untouched.
    if let Some(message) = tatr_install_message(&e) {
        return PdfluentIoError::new_err(message);
    }
    match e {
        EngineError::InvalidPdf(msg) => PdfluentParseError::new_err(format!("invalid PDF: {msg}")),
        EngineError::PageOutOfRange { index, count } => {
            PdfluentPageRangeError::new_err(format!("page {index} out of range ({count} pages)"))
        }
        EngineError::RenderError(msg) => {
            PdfluentRenderError::new_err(format!("render error: {msg}"))
        }
        EngineError::Io(e) => PdfluentIoError::new_err(e.to_string()),
        EngineError::Encrypted(msg) => {
            PdfluentEncryptedError::new_err(format!("PDF is encrypted: {msg}"))
        }
        EngineError::InvalidPageGeometry { reason, .. } => {
            PdfluentGeometryError::new_err(format!("invalid page geometry: {reason}"))
        }
        EngineError::XfaFlattenFailed(msg) => {
            PdfluentRenderError::new_err(format!("XFA flatten failed: {msg}"))
        }
        EngineError::LimitExceeded(e) => {
            PdfluentLimitError::new_err(format!("processing limit exceeded: {e}"))
        }
    }
}

/// The missing table-structure weights, in the install command's words.
///
/// `Some` only for a [`ModelErrorKind::ModelNotAvailable`] naming the
/// `tatr-v1.1-all` package: the path looked at plus the product's tables
/// installer. Anything else -- another model, a checksum mismatch, an OS
/// error -- is `None` and keeps the engine's own message.
fn tatr_install_message(e: &EngineError) -> Option<String> {
    let model = e.model_error()?;
    if model.kind != ModelErrorKind::ModelNotAvailable
        || model.model.as_deref() != Some("tatr-v1.1-all")
    {
        return None;
    }
    let path = model
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "<model dir>/tatr-v1.1-all/tatr-v1.1-all.onnx".to_string());
    Some(format!(
        "model 'tatr-v1.1-all' is not available: {path} was not found. \
         Install it with: pdfluent models install tables"
    ))
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// Convert a `pdfluent::Error` into a typed Python exception and attach
/// canonical C8 metadata (`code`, `message`) to the resulting exception
/// instance.
///
/// Parity surface: Node, WASM, and .NET expose `.code` directly. Setting the
/// attributes on the `PyErr` value here matches that contract — users branch
/// on `e.code` without parsing the message string.
pub(crate) fn pdfluent_err_to_py(e: pdfluent::Error) -> PyErr {
    // Canonical C8 code from the Rust core (see crates/pdfluent/src/error.rs).
    let code = e.code();
    let message = e.to_string();
    let py_err = PdfluentError::new_err(message.clone());
    attach_code_attrs(&py_err, code, &message);
    py_err
}

/// Attach `code` and `message` attributes to a `PyErr` instance so the
/// raised Python exception carries the canonical C8 metadata directly,
/// matching the Node/WASM/.NET surface contract.
fn attach_code_attrs(err: &PyErr, code: &str, message: &str) {
    Python::with_gil(|py| {
        let value = err.value(py);
        // Best-effort: failures to set attributes (e.g. read-only base class)
        // are silently ignored — the message is still available via `args[0]`.
        let _ = value.setattr("code", code);
        let _ = value.setattr("message", message);
    });
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

/// A PDF document.
///
/// Open from a file path or bytes. Supports context manager protocol.
///
/// Examples
/// --------
/// >>> with Document("invoice.pdf") as doc:
/// ...     print(doc.page_count)
/// ...     img = doc[0].render()
#[pyclass(name = "Document")]
pub(crate) struct PyDocument {
    /// The parsed engine document, or `None` while the file still needs a
    /// password. `authenticate` fills it in; reads on a locked document
    /// fail with [`PdfluentEncryptedError`].
    inner: Mutex<Option<Arc<PdfDocument>>>,
    raw_bytes: Arc<Vec<u8>>,
    /// Source path for save-to-original-name flows; empty when opened from
    /// bytes. Read by the compatible save path (`compat_save`, `ez_save`).
    pub(crate) name: String,
    /// Shared with every `Page` (lane L4 mutation channel): page edits
    /// write back here so `save()` and `current_bytes()` observe them.
    pub(crate) lopdf: Arc<Mutex<Option<LopdfDocument>>>,
    /// Set by `close` / context exit; reads past it fail with `ValueError`.
    closed: AtomicBool,
    /// Set by the first mutation; cleared by a successful save.
    dirty: AtomicBool,
    /// Last password `authenticate` accepted, for re-encryption flows.
    password: Mutex<Option<String>>,
    /// Undo journal owned by the door: the facade is rebuilt from bytes
    /// on every call, so journal state cannot live on it.
    pub(crate) journal: pdfluent::compat::edit::Journal,
}

/// Plumbing for the Office exporters. Kept out of `#[pymethods]` so PyO3 does
/// not expose it to Python.
/// Bytes-like argument: bytes or bytearray, anything else is a type error.
fn bytes_arg(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    value
        .extract::<Vec<u8>>()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("stream must be bytes"))
}

/// One page by integer index, honouring negative positions.
fn page_at(slf: Py<PyDocument>, py: Python<'_>, index: isize) -> PyResult<PyPage> {
    let borrowed = slf.bind(py).borrow();
    borrowed.ensure_open()?;
    let engine = borrowed.engine()?;
    let count = engine.page_count() as isize;
    let idx = if index < 0 { count + index } else { index };
    if idx < 0 || idx >= count {
        return Err(pyo3::exceptions::PyIndexError::new_err(format!(
            "page index {index} out of range ({count} pages)"
        )));
    }
    Ok(PyPage {
        doc: engine,
        index: idx as usize,
        refreshed: AtomicBool::new(false),
        clip: Mutex::new(None),
        parent: slf.clone_ref(py),
        lopdf: Arc::clone(&borrowed.lopdf),
        snapshot: Arc::clone(&borrowed.raw_bytes),
    })
}

/// The pages a slice selects, in slice order.
fn slice_pages(
    slf: Py<PyDocument>,
    py: Python<'_>,
    slice: &Bound<'_, pyo3::types::PySlice>,
) -> PyResult<Vec<PyPage>> {
    let borrowed = slf.bind(py).borrow();
    borrowed.ensure_open()?;
    let engine = borrowed.engine()?;
    let span = slice.indices(engine.page_count() as isize)?;
    let mut out = Vec::with_capacity(span.slicelength.max(0));
    let mut at = span.start;
    for _ in 0..span.slicelength.max(0) {
        out.push(PyPage {
            doc: Arc::clone(&engine),
            index: at as usize,
            refreshed: AtomicBool::new(false),
            clip: Mutex::new(None),
            parent: slf.clone_ref(py),
            lopdf: Arc::clone(&borrowed.lopdf),
            snapshot: Arc::clone(&borrowed.raw_bytes),
        });
        at += span.step;
    }
    Ok(out)
}

impl PyDocument {
    /// A new empty document: zero pages, no backing file.
    fn empty(password: Option<String>) -> PyResult<Self> {
        let raw_bytes = Arc::new(compat::document::empty_pdf_bytes());
        let doc = match password.as_deref() {
            Some(pw) => PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw),
            None => PdfDocument::open(Arc::clone(&raw_bytes)),
        }
        .map_err(engine_err_to_py)?;
        Ok(Self {
            inner: Mutex::new(Some(Arc::new(doc))),
            raw_bytes,
            name: String::new(),
            lopdf: Arc::new(Mutex::new(None)),
            closed: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            password: Mutex::new(password),
            journal: pdfluent::compat::edit::Journal::default(),
        })
    }

    /// The bytes as they stand now, mutations included.
    ///
    /// Same rule as `save()`: once anything has written through the lopdf
    /// handle, the original bytes no longer describe the document, and an
    /// export that ignored that would silently convert the pre-edit version.
    pub(crate) fn current_bytes(&self) -> PyResult<Vec<u8>> {
        let mut guard = self.lopdf.lock().unwrap();
        match *guard {
            Some(ref mut doc) => {
                let mut buf = Vec::new();
                doc.save_to(&mut buf)
                    .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
                Ok(buf)
            }
            None => Ok(self.raw_bytes.as_ref().clone()),
        }
    }

    /// Route Office export through the facade rather than calling the
    /// converter crates directly. The capability gate lives on
    /// `pdfluent::PdfDocument`; duplicating it here would mean two places to
    /// keep in step, and the one that drifts is the one nobody tests.
    fn office_export<'py>(
        &self,
        py: Python<'py>,
        convert: fn(&pdfluent::PdfDocument) -> pdfluent::Result<Vec<u8>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let raw = self.current_bytes()?;
        let doc = pdfluent::PdfDocument::from_bytes(&raw).map_err(pdfluent_err_to_py)?;
        let out = convert(&doc).map_err(pdfluent_err_to_py)?;
        Ok(PyBytes::new(py, &out))
    }

    /// The engine document, failing on a closed or locked handle.
    pub(crate) fn engine(&self) -> PyResult<Arc<PdfDocument>> {
        self.ensure_open()?;
        self.inner
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| PdfluentEncryptedError::new_err("document needs a password"))
    }

    /// Fail with `ValueError` past `close`.
    pub(crate) fn ensure_open(&self) -> PyResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "document is closed",
            ));
        }
        Ok(())
    }

    /// Mark the document dirty after a mutation.
    pub(crate) fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Markdown through a caller-given recognizer: the OCR layer is laid by
    /// `engine` then extraction reads the copy with OCR off, the way the
    /// engine reads its own searchable copy.
    #[cfg(feature = "ocr-cli")]
    fn extract_markdown_with_ocr_engine(
        &self,
        options: &ExtractOptions,
        engine: &ocr_callback::OcrCallback,
    ) -> PyResult<String> {
        let copy = self
            .engine()?
            .searchable_copy(options, engine)
            .map_err(engine_err_to_py)?;
        let mut inner = options.clone();
        inner.ocr = OcrMode::Never;
        copy.extract_markdown(&inner).map_err(engine_err_to_py)
    }

    /// Without OCR support compiled in, a custom recognizer has nowhere
    /// to run.
    #[cfg(not(feature = "ocr-cli"))]
    fn extract_markdown_with_ocr_engine(
        &self,
        _options: &ExtractOptions,
        _engine: &ocr_callback::OcrCallback,
    ) -> PyResult<String> {
        Err(PdfluentRenderError::new_err(
            "ocr_function needs a build with OCR support",
        ))
    }
}

/// Engine access for the compat lanes (added by L1; other lanes reuse
/// these rather than adding siblings).
impl PyDocument {
    pub(crate) fn compat_raw_bytes(&self) -> &Arc<Vec<u8>> {
        &self.raw_bytes
    }
}

#[pymethods]
impl PyDocument {
    /// Open a PDF from a file path or raw bytes.
    ///
    /// An EPUB opens too: its chapters render as plain pages, one per
    /// chapter, so the extraction methods read it unchanged. So does an
    /// image (PNG or JPEG, as a one-page PDF sized to it) and HTML (via an
    /// external headless Chrome). A password is only read for PDFs.
    ///
    /// Parameters
    /// ----------
    /// source : str, bytes, or None
    ///     File path, raw file bytes, or omit for an empty document.
    /// password : str, optional
    ///     Password for encrypted PDFs.
    /// stream : bytes, optional
    ///     Raw file bytes; an alternative to passing them as `source`.
    /// filetype : str, optional
    ///     Hint for the stream layout ("pdf", "epub", "png", "html", …);
    ///     magic wins when the bytes identify themselves.
    /// rect : Rect, optional
    ///     Accepted and ignored beside a stream; alone it opens empty.
    /// width, height : float, optional
    ///     Accepted and ignored beside a stream; alone they open empty.
    ///
    /// Raises
    /// ------
    /// FileDataError
    ///     If the bytes are not a valid PDF.
    /// EmptyFileError
    ///     If the input carries no bytes at all.
    /// PdfluentError
    ///     If the format is named but unsupported, or HTML arrives with
    ///     no Chrome to render it.
    /// FileNotFoundError
    ///     If the named file does not exist.
    /// PdfluentEncryptedError
    ///     If the PDF is encrypted, the password is wrong, and a page is
    ///     read before authenticating. Opening itself defers the unlock so
    ///     `needs_pass` and `authenticate` can run first.
    /// PdfluentIoError
    ///     If the file cannot be read for another reason.
    #[new]
    #[pyo3(signature = (source=None, password=None, *, stream=None, filetype=None, rect=None, width=0.0, height=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        source: Option<&Bound<'_, PyAny>>,
        password: Option<&str>,
        stream: Option<&Bound<'_, PyAny>>,
        filetype: Option<String>,
        rect: Option<&Bound<'_, PyAny>>,
        width: f64,
        height: f64,
    ) -> PyResult<Self> {
        let _ = (rect, width, height);
        let have_source = source.is_some_and(|found| !found.is_none());
        let have_stream = stream.is_some_and(|found| !found.is_none());
        if have_source && have_stream {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "pass either a filename or a stream, not both",
            ));
        }
        if !have_source && !have_stream {
            return Self::empty(password.map(str::to_string));
        }
        let input = if have_stream { stream } else { source };
        let found = input.expect("one input is present");
        let mut name = String::new();
        let mut data: Vec<u8> = if have_stream {
            bytes_arg(found)?
        } else if let Ok(path_str) = found.extract::<String>() {
            let path = PathBuf::from(&path_str);
            let bytes = std::fs::read(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return compat::errors::file_not_found_error(format!("{path_str}: {e}"));
                }
                PdfluentIoError::new_err(format!("{path_str}: {e}"))
            })?;
            name = path_str;
            bytes
        } else if found.is_instance_of::<pyo3::types::PyBytes>()
            || found.is_instance_of::<pyo3::types::PyByteArray>()
        {
            bytes_arg(found)?
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "source must be a file path (str) or bytes",
            ));
        };
        if data.is_empty() {
            return Err(compat::errors::empty_file_error("stream is empty"));
        }
        let filename = if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        data = pdfluent::compat::open::input_to_pdf_bytes(&data, filetype.as_deref(), filename)
            .map_err(compat::errors::open_convert_error)?;

        let raw_bytes = Arc::new(data);
        let opened = match password {
            Some(pw) => PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw),
            None => PdfDocument::open(Arc::clone(&raw_bytes)),
        };
        let doc = match opened {
            Ok(doc) => Some(Arc::new(doc)),
            Err(EngineError::InvalidPdf(msg)) => {
                return Err(compat::errors::file_data_error(format!(
                    "invalid PDF: {msg}"
                )));
            }
            Err(EngineError::Encrypted(_)) if password.is_none() => None,
            Err(other) => return Err(engine_err_to_py(other)),
        };

        Ok(Self {
            inner: Mutex::new(doc),
            raw_bytes,
            name,
            lopdf: Arc::new(Mutex::new(None)),
            closed: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            password: Mutex::new(password.map(str::to_string)),
            journal: pdfluent::compat::edit::Journal::default(),
        })
    }

    /// Number of pages.
    #[getter]
    fn page_count(&self) -> PyResult<usize> {
        Ok(self.engine()?.page_count())
    }

    /// Document metadata.
    #[getter]
    fn metadata(&self) -> PyResult<PyDocumentInfo> {
        self.ensure_open()?;
        Ok(read_document_info(self))
    }

    /// Document outline / bookmarks.
    #[getter]
    fn bookmarks(&self) -> PyResult<Vec<PyBookmark>> {
        Ok(self
            .engine()?
            .bookmarks()
            .into_iter()
            .map(PyBookmark)
            .collect())
    }

    /// Get a page by index (supports negative indexing), or a page list
    /// for a slice.
    fn __getitem__(slf: Py<Self>, py: Python<'_>, index: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(slice) = index.downcast::<pyo3::types::PySlice>() {
            return Ok(slice_pages(slf, py, slice)?
                .into_pyobject(py)?
                .into_any()
                .unbind());
        }
        let position: isize = index.extract().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("page index must be an integer or slice")
        })?;
        Ok(page_at(slf, py, position)?
            .into_pyobject(py)?
            .into_any()
            .unbind())
    }

    /// Number of pages (for ``len()``).
    fn __len__(&self) -> PyResult<usize> {
        self.ensure_open()?;
        Ok(self.engine()?.page_count())
    }

    /// Iterate over pages.
    fn __iter__(slf: Py<Self>, py: Python<'_>) -> PyResult<PyPageIterator> {
        let borrowed = slf.bind(py).borrow();
        borrowed.ensure_open()?;
        let engine = borrowed.engine()?;
        Ok(PyPageIterator {
            count: engine.page_count(),
            doc: engine,
            index: 0,
            parent: slf.clone_ref(py),
            lopdf: Arc::clone(&borrowed.lopdf),
            snapshot: Arc::clone(&borrowed.raw_bytes),
        })
    }

    /// Context manager entry.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Context manager exit: leaving the block closes the document.
    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Render all pages in parallel.
    ///
    /// Parameters
    /// ----------
    /// dpi : float, optional
    ///     Resolution (default 150).
    ///
    /// Returns
    /// -------
    /// list[RenderedImage]
    #[pyo3(signature = (dpi=150.0))]
    fn render_all(&self, dpi: f64) -> PyResult<Vec<PyRenderedImage>> {
        let opts = RenderOptions {
            dpi,
            ..Default::default()
        };
        Ok(self
            .engine()?
            .render_all(&opts)
            .into_iter()
            .map(PyRenderedImage)
            .collect())
    }

    /// Search for text across all pages.
    ///
    /// Returns a list of 0-based page indices containing the query.
    fn search(&self, query: &str) -> PyResult<Vec<usize>> {
        Ok(self.engine()?.search_text(query))
    }

    /// The document as markdown, in reading order (#573).
    ///
    /// Headings become ``#`` to ``###``, list items ``-`` and ``1.``, a table
    /// a pipe table or an HTML one where markdown cannot hold its shape, and a
    /// change of page an HTML comment. The running heads a document repeats
    /// are not in it; they are in :meth:`extract_blocks`.
    ///
    /// ``first_page`` and ``page_count`` choose a window of pages;
    /// ``page_count=None`` means the rest of the document. The document is
    /// read whole whatever the window is, because what a block IS is decided
    /// against the rest of the document.
    ///
    /// ``pages`` is an explicit list of 0-based page indices instead of the
    /// window; blocks come back in reading order whatever order lists them.
    /// A page past the last one, or ``pages`` beside a window, is an error.
    ///
    /// ``include_headers`` (default ``False``) writes the running heads, and
    /// ``include_footers`` (default ``False``) the page numbers, as plain
    /// paragraphs at their reading-order position, for callers that need them
    /// in the text. ``page_separators`` is ``"comment"`` (default:
    /// ``<!-- page N -->`` where the page changes) or ``"none"``: no marker,
    /// paragraphs separated by a blank line.
    /// ``code`` (default ``True``) writes monospace runs as code: an inline
    /// span in a paragraph or a list item, a fenced block for a paragraph set
    ///
    /// ``quality`` is ``"fast"`` (default: the shipping engine) or
    /// ``"balanced"`` (the layout model tier); anything else is an error
    /// naming the value. ``model_dir`` points the balanced tier at its
    /// weights, ``table_model_dir`` opts its table regions into TATR
    /// structure; ``None`` resolves through ``PDFLUENT_MODEL_DIR`` to the
    /// per-user model dir, and ``table_model_dir`` unset leaves TATR off.
    /// ``keep_furniture`` (default ``False``) keeps the balanced tier's
    /// running heads and feet as paragraphs instead of dropping them; read
    /// only with ``quality="balanced"``.
    /// ``ocr`` (default ``"never"``) is ``"never"`` (read what the document
    /// draws), ``"auto"`` (recognize the pages that need it) or ``"force"``
    /// (recognize every page in the window, replacing a born-digital page's
    /// own text rather than doubling it). ``ocr_language`` (default
    /// ``"eng"``) is the language tag recognition runs in, ``ocr_dpi``
    /// (default 150) the resolution it renders pages at. ``ocr_function``
    /// (default ``None``) replaces the recognizer with a Python callable
    /// taking ``(image, width, height, dpi)`` and returning word dicts; it
    /// is read only when ``ocr`` is not ``"never"``.
    #[pyo3(signature = (first_page = 0, page_count = None, pages = None, include_headers = false, include_footers = false, page_separators = "comment", code = true, quality = "fast", model_dir = None, table_model_dir = None, keep_furniture = false, ocr = "never", ocr_language = "eng", ocr_dpi = 150, ocr_function = None))]
    #[allow(clippy::too_many_arguments)]
    fn extract_markdown(
        &self,
        py: Python<'_>,
        first_page: usize,
        page_count: Option<usize>,
        pages: Option<Vec<usize>>,
        include_headers: bool,
        include_footers: bool,
        page_separators: &str,
        code: bool,
        quality: &str,
        model_dir: Option<PathBuf>,
        table_model_dir: Option<PathBuf>,
        keep_furniture: bool,
        ocr: &str,
        ocr_language: &str,
        ocr_dpi: u32,
        ocr_function: Option<Py<PyAny>>,
    ) -> PyResult<String> {
        let options = markdown_options(
            first_page,
            page_count,
            pages,
            include_headers,
            include_footers,
            page_separators,
            code,
            quality,
            model_dir,
            table_model_dir,
            keep_furniture,
            ocr,
            ocr_language,
            ocr_dpi,
        )?;
        let callback = match ocr_function {
            Some(function) => {
                if !function.bind(py).is_callable() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "ocr_function must be callable or None",
                    ));
                }
                (options.ocr != OcrMode::Never).then(|| ocr_callback::OcrCallback::new(function))
            }
            None => None,
        };
        match callback {
            Some(engine) => self.extract_markdown_with_ocr_engine(&options, &engine),
            None => self
                .engine()?
                .extract_markdown(&options)
                .map_err(engine_err_to_py),
        }
    }

    /// The document as a Docling ``DoclingDocument``, as JSON (#629).
    ///
    /// The schema docling-core pins in ``pdf_engine::docling``; load it with
    /// ``DoclingDocument.model_validate_json(doc.extract_docling())`` and the
    /// pipeline downstream of Docling reads it unchanged. ``name`` is
    /// ``DoclingDocument.name``, which Docling fills with the file's stem.
    #[pyo3(signature = (first_page = 0, page_count = None, name = "document", pages = None))]
    fn extract_docling(
        &self,
        first_page: usize,
        page_count: Option<usize>,
        name: &str,
        pages: Option<Vec<usize>>,
    ) -> PyResult<String> {
        self.engine()?
            .extract_docling(name, &extract_options(first_page, page_count, pages))
            .map_err(engine_err_to_py)
    }

    /// The document's blocks, in reading order (#573).
    ///
    /// One :class:`Block` per paragraph, heading, list item, table cell and
    /// running head, each with the page and the box it was drawn in -- what a
    /// caller chunks, cites or embeds.
    ///
    /// Takes the same ``quality``, ``model_dir`` and ``table_model_dir`` as
    /// :meth:`extract_markdown`, but the balanced tier assembles markdown:
    /// ``quality="balanced"`` here raises the engine's markdown-only error.
    #[pyo3(signature = (first_page = 0, page_count = None, pages = None, quality = "fast", model_dir = None, table_model_dir = None))]
    fn extract_blocks(
        &self,
        first_page: usize,
        page_count: Option<usize>,
        pages: Option<Vec<usize>>,
        quality: &str,
        model_dir: Option<PathBuf>,
        table_model_dir: Option<PathBuf>,
    ) -> PyResult<Vec<PyBlock>> {
        let options = tier_options(
            extract_options(first_page, page_count, pages),
            quality,
            model_dir,
            table_model_dir,
        )?;
        Ok(self
            .engine()?
            .extract_blocks(&options)
            .map_err(engine_err_to_py)?
            .into_iter()
            .map(PyBlock)
            .collect())
    }

    /// Every word of the document, with the box it was drawn in.
    ///
    /// One :class:`Word` per word, in reading order, block by block and line
    /// by line -- what a caller cites, highlights or grounds with. Built from
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    #[pyo3(signature = (first_page = 0, page_count = None, pages = None))]
    fn extract_words(
        &self,
        first_page: usize,
        page_count: Option<usize>,
        pages: Option<Vec<usize>>,
    ) -> PyResult<Vec<PyWord>> {
        let options = extract_options(first_page, page_count, pages);
        Ok(self
            .engine()?
            .extract_words(&options)
            .map_err(engine_err_to_py)?
            .into_iter()
            .map(PyWord)
            .collect())
    }

    /// Which pages are worth sending to a vision model, and why (#624).
    ///
    /// One :class:`VisionAdvice` per page, in page order. ``needs_vision`` is
    /// the flag an agent routes on; the three reasons say what it would be
    /// routing. Today only ``no_text_layer`` is measured -- a page that draws
    /// ink and yields no text, a scan -- and the other two are always
    /// ``False``: the engine has no table model that gives a probability yet,
    /// and no figure detector, and a guessed flag would be worse than none.
    fn vision_advice(&self) -> PyResult<Vec<PyVisionAdvice>> {
        Ok(self
            .engine()?
            .vision_advice()
            .map_err(engine_err_to_py)?
            .into_iter()
            .enumerate()
            .map(|(page, advice)| PyVisionAdvice {
                page,
                no_text_layer: advice.no_text_layer,
                low_confidence_table: advice.low_confidence_table,
                figure: advice.figure,
            })
            .collect())
    }

    /// Save the PDF to a file path.
    ///
    /// If the document has been mutated (form fill, annotations, redactions,
    /// …) the mutated state is written. Otherwise the original bytes are
    /// copied verbatim.
    ///
    /// The keyword arguments tune the write: `garbage`/`clean`/`deflate`
    /// select the cleanup pass, `incremental` appends to the backing file,
    /// and `encryption`/`permissions`/`owner_pw`/`user_pw` encrypt the
    /// output. Cleanup passes beyond the default are accepted and ignored.
    #[pyo3(signature = (filename, garbage=3, clean=0, deflate=1, deflate_images=1, deflate_fonts=1, incremental=0, ascii=0, expand=0, linear=0, no_new_id=0, appearance=0, pretty=0, encryption=1, permissions=4095, owner_pw=None, user_pw=None, preserve_metadata=1, use_objstms=0, compression_effort=0, raise_on_repair=0, reproducible=0))]
    #[allow(clippy::too_many_arguments)]
    fn save(
        &self,
        filename: &str,
        garbage: i32,
        clean: i32,
        deflate: i32,
        deflate_images: i32,
        deflate_fonts: i32,
        incremental: i32,
        ascii: i32,
        expand: i32,
        linear: i32,
        no_new_id: i32,
        appearance: i32,
        pretty: i32,
        encryption: i32,
        permissions: i32,
        owner_pw: Option<String>,
        user_pw: Option<String>,
        preserve_metadata: i32,
        use_objstms: i32,
        compression_effort: i32,
        raise_on_repair: i32,
        reproducible: i32,
    ) -> PyResult<()> {
        let _ = (
            garbage,
            clean,
            deflate,
            deflate_images,
            deflate_fonts,
            ascii,
            expand,
            no_new_id,
            appearance,
            pretty,
            preserve_metadata,
            use_objstms,
            compression_effort,
            raise_on_repair,
            reproducible,
        );
        compat::document::compat_save(
            self,
            filename,
            incremental != 0,
            linear != 0,
            encryption,
            permissions,
            owner_pw,
            user_pw,
        )
    }

    // ------------------------------------------------------------------
    // Office export
    // ------------------------------------------------------------------

    /// Convert to a Word document (``.docx``) and return the bytes.
    fn to_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.office_export(py, |d| d.to_docx_bytes())
    }

    /// Convert to an Excel workbook (``.xlsx``) and return the bytes.
    fn to_xlsx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.office_export(py, |d| d.to_xlsx_bytes())
    }

    /// Convert to a PowerPoint deck (``.pptx``) and return the bytes.
    fn to_pptx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.office_export(py, |d| d.to_pptx_bytes())
    }

    // ------------------------------------------------------------------
    // Digital signatures
    // ------------------------------------------------------------------

    /// Cryptographically validate every digital signature in the document.
    ///
    /// Mirrors the Rust core
    /// [`PdfDocument::verify_signatures`](pdfluent::PdfDocument) and the
    /// Node ``validateSignatures()`` parity surface. Each signature field is
    /// returned as a :class:`SignatureResult` carrying its validation
    /// ``status`` (``"valid"``, ``"invalid"``, or ``"unknown"``), an optional
    /// ``reason``, the ``field_name``, and — when present — the ``signer``
    /// common name and signing ``timestamp``.
    ///
    /// A document with no signatures returns an empty list (never raises).
    ///
    /// Returns
    /// -------
    /// list[SignatureResult]
    ///     One entry per signature field found, in document order.
    fn validate_signatures(&self) -> PyResult<Vec<PySignatureResult>> {
        let engine = self.engine()?;
        Ok(validate_signatures(engine.pdf())
            .into_iter()
            .map(|r| {
                let (status, reason) = match r.status {
                    ValidationStatus::Valid => ("valid".to_string(), None),
                    ValidationStatus::Invalid(msg) => ("invalid".to_string(), Some(msg)),
                    ValidationStatus::Unknown(msg) => ("unknown".to_string(), Some(msg)),
                };
                PySignatureResult {
                    status,
                    reason,
                    field_name: r.field_name,
                    signer: r.signer,
                    timestamp: r.timestamp,
                }
            })
            .collect())
    }

    /// Alias for :meth:`validate_signatures`.
    ///
    /// Provided for parity with the Rust core
    /// [`PdfDocument::verify_signatures`](pdfluent::PdfDocument) method name.
    /// Returns the same :class:`SignatureResult` list with full cryptographic
    /// validation.
    fn verify_signatures(&self) -> PyResult<Vec<PySignatureResult>> {
        self.validate_signatures()
    }

    /// Lightweight list of signatures present in the document.
    ///
    /// Mirrors the Rust core
    /// [`PdfDocument::signatures`](pdfluent::PdfDocument): metadata only, with
    /// **no cryptographic validation**. Each entry's ``status`` is
    /// ``"unknown"`` and ``reason`` is ``None``; use
    /// :meth:`validate_signatures` for the validated report.
    ///
    /// A document with no signatures returns an empty list.
    ///
    /// Returns
    /// -------
    /// list[SignatureResult]
    ///     One entry per signature field found, in document order.
    fn signatures(&self) -> PyResult<Vec<PySignatureResult>> {
        let engine = self.engine()?;
        Ok(signature_fields(engine.pdf())
            .into_iter()
            .map(|f| PySignatureResult {
                status: "unknown".to_string(),
                reason: None,
                field_name: f.field_name,
                signer: f.sig.signer_name(),
                timestamp: f.sig.signing_time(),
            })
            .collect())
    }

    // ------------------------------------------------------------------
    // Form fields
    // ------------------------------------------------------------------

    /// Return all interactive form fields in the document.
    ///
    /// Returns
    /// -------
    /// list[FormField]
    fn get_form_fields(&self) -> PyResult<Vec<PyFormField>> {
        let engine = self.engine()?;
        let Some(tree) = parse_acroform(engine.pdf()) else {
            return Ok(vec![]);
        };
        Ok(tree
            .terminal_fields()
            .into_iter()
            .map(|id| {
                let name = tree.fully_qualified_name(id);
                let field_type = tree
                    .effective_field_type(id)
                    .map(|ft| match ft {
                        FieldType::Text => "text",
                        FieldType::Button => "button",
                        FieldType::Choice => "choice",
                        FieldType::Signature => "signature",
                    })
                    .unwrap_or("unknown")
                    .to_string();
                let value = tree.effective_value(id).map(|v| match v {
                    FieldValue::Text(s) => s.clone(),
                    FieldValue::StringArray(a) => a.join(", "),
                });
                let page = tree.get(id).page_index;
                PyFormField {
                    name,
                    field_type,
                    value,
                    page,
                }
            })
            .collect())
    }

    /// Set the value of a form field by its fully-qualified name.
    ///
    /// Routes through the single SDK writeback chain
    /// (``pdf_forms::apply_field_value``): correct ``/V`` encoding (ASCII
    /// literal else UTF-16BE+BOM), ``/V``-as-Name for buttons, per-widget
    /// ``/AS`` sync, ``/AP`` regeneration, recursive fully-qualified name
    /// lookup, and read-only rejection.
    ///
    /// Parameters
    /// ----------
    /// name : str
    ///     Fully-qualified field name (e.g. ``"Address.Street"``).
    /// value : str
    ///     New value, dispatched by the field's actual type: text, radio
    ///     export name, choice option, or bool-ish checkbox state
    ///     (``"true"``/``"false"``/``"Off"``/``"0"``).
    ///
    /// Returns
    /// -------
    /// bool
    ///     ``True`` if the field was found and updated; ``False`` when the
    ///     document has no form or no field matches ``name``.
    ///
    /// Raises
    /// ------
    /// PdfluentError
    ///     If the field is read-only, the value is not a valid option for
    ///     the field, or the form structure cannot be mutated (e.g. an
    ///     inline ``/AcroForm`` dictionary).
    fn set_form_field(&self, name: &str, value: &str) -> PyResult<bool> {
        // No form at all → field cannot be found (False), not an error.
        let engine = self.engine()?;
        if parse_acroform(engine.pdf()).is_none() {
            return Ok(false);
        }
        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();
        match apply_string_value(doc, name, value) {
            Ok(_) => Ok(true),
            Err(WritebackError::FieldNotFound(_)) => Ok(false),
            Err(e) => Err(PdfluentError::new_err(format!(
                "set_form_field '{name}': {e}"
            ))),
        }
    }

    /// Set multiple selected values on a multi-select list box.
    ///
    /// Routes through ``pdf_forms::apply_choice_multi``: writes ``/V`` as an
    /// array of text strings and rebuilds ``/I`` (the sorted selected-index
    /// cache) to match what Adobe Acrobat produces. Pass an empty list to
    /// clear the selection.
    ///
    /// Parameters
    /// ----------
    /// name : str
    ///     Fully-qualified field name of a multi-select list box
    ///     (``/Ff`` MultiSelect flag, bit 22).
    /// values : list[str]
    ///     Export (or display) values of the options to select. For a
    ///     non-editable list box every value must appear in ``/Opt``.
    ///
    /// Returns
    /// -------
    /// bool
    ///     ``True`` if the field was found and updated; ``False`` when the
    ///     document has no form or no field matches ``name``.
    ///
    /// Raises
    /// ------
    /// PdfluentError
    ///     If the field is not a multi-select list box, is read-only, or any
    ///     value is not a valid option (non-editable list boxes).
    fn set_multi_select(&self, name: &str, values: Vec<String>) -> PyResult<bool> {
        let engine = self.engine()?;
        if parse_acroform(engine.pdf()).is_none() {
            return Ok(false);
        }
        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();
        match apply_choice_multi(doc, name, &values) {
            Ok(_) => Ok(true),
            Err(WritebackError::FieldNotFound(_)) => Ok(false),
            Err(e) => Err(PdfluentError::new_err(format!(
                "set_multi_select '{name}': {e}"
            ))),
        }
    }

    /// Deprecated alias for :meth:`set_multi_select`.
    ///
    /// .. deprecated::
    ///    Use :meth:`set_multi_select` — the canonical cross-language name
    ///    (``setMultiSelect`` in JS/Java/WASM). Kept for backward
    ///    compatibility; will be removed in 1.0.0.
    fn set_form_field_multi(&self, name: &str, values: Vec<String>) -> PyResult<bool> {
        self.set_multi_select(name, values)
    }

    // ------------------------------------------------------------------
    // Annotations
    // ------------------------------------------------------------------

    /// Return all annotations on the given page (0-based).
    ///
    /// Parameters
    /// ----------
    /// page : int
    ///     0-based page index.
    ///
    /// Returns
    /// -------
    /// list[Annotation]
    fn get_annotations(&self, page: usize) -> PyResult<Vec<PyAnnotation>> {
        let engine = self.engine()?;
        let pages = engine.pdf().pages();
        if page >= pages.len() {
            return Err(PdfluentPageRangeError::new_err(format!(
                "page {page} out of range ({} pages)",
                pages.len()
            )));
        }
        let raw_annots = Annotation::from_page(&pages[page]);
        Ok(raw_annots
            .into_iter()
            .map(|a| {
                let annot_type = format!("{:?}", a.annotation_type());
                let rect = a
                    .rect()
                    .map(|r| (r.x0, r.y0, r.x1, r.y1))
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));
                PyAnnotation {
                    page,
                    annot_type,
                    rect,
                    contents: a.contents(),
                    author: a.author(),
                }
            })
            .collect())
    }

    /// Add an annotation to a page.
    ///
    /// Parameters
    /// ----------
    /// page : int
    ///     0-based page index.
    /// annot_type : str
    ///     ``"highlight"`` or ``"freetext"``.
    /// rect : tuple[float, float, float, float]
    ///     Bounding box as ``(x0, y0, x1, y1)`` in PDF user-space points.
    ///     Origin is bottom-left of the page.
    /// content : str, optional
    ///     Text content of the annotation.
    #[pyo3(signature = (page, annot_type, rect, content=None))]
    fn add_annotation(
        &self,
        page: usize,
        annot_type: &str,
        rect: (f64, f64, f64, f64),
        content: Option<&str>,
    ) -> PyResult<()> {
        let page_1based = (page + 1) as u32;
        let ar = AnnotRect::new(rect.0, rect.1, rect.2, rect.3);

        let builder = match annot_type.to_lowercase().as_str() {
            "highlight" => {
                let b = AnnotationBuilder::highlight(ar).quad_points_from_rect(&ar);
                if let Some(text) = content {
                    b.contents(text)
                } else {
                    b
                }
            }
            "freetext" | "free_text" => {
                let text = content.unwrap_or("");
                AnnotationBuilder::free_text(ar, text, 12.0)
            }
            other => {
                return Err(PdfluentValidationError::new_err(format!(
                    "unsupported annotation type {other:?}; use 'highlight' or 'freetext'"
                )));
            }
        };

        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();

        let annot_id = builder
            .build(doc)
            .map_err(|e| PdfluentRenderError::new_err(format!("annotation build failed: {e:?}")))?;

        add_annotation_to_page(doc, page_1based, annot_id)
            .map_err(|e| PdfluentRenderError::new_err(format!("add to page failed: {e:?}")))?;

        Ok(())
    }

    // ------------------------------------------------------------------
    // Redaction
    // ------------------------------------------------------------------

    /// Search for text and redact all occurrences.
    ///
    /// Parameters
    /// ----------
    /// search_term : str
    ///     Text to search for (literal match, case-insensitive).
    /// page : int, optional
    ///     0-based page index to limit search to. ``None`` searches all pages.
    ///
    /// Returns
    /// -------
    /// RedactReport
    #[pyo3(signature = (search_term, page=None))]
    fn redact_text(&self, search_term: &str, page: Option<usize>) -> PyResult<PyRedactReport> {
        let mut options = RedactSearchOptions::default();
        if let Some(p) = page {
            options = options.pages(vec![(p + 1) as u32]);
        }

        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();

        let report = search_and_redact(doc, search_term, &options)
            .map_err(|e| PdfluentError::new_err(e.to_string()))?;

        Ok(PyRedactReport {
            matches_found: report.matches_found,
            areas_redacted: report.areas_redacted,
            pages_affected: report.pages_affected,
        })
    }

    // ------------------------------------------------------------------
    // Encryption
    // ------------------------------------------------------------------

    /// Save an encrypted (password-protected) copy of the document.
    ///
    /// Parameters
    /// ----------
    /// output_path : str
    ///     Destination file path.
    /// password : str
    ///     User password (required to open).
    /// owner_password : str, optional
    ///     Owner password (for permissions). Defaults to ``password``.
    #[pyo3(signature = (output_path, password, owner_password=None))]
    fn encrypt(
        &self,
        output_path: &str,
        password: &str,
        owner_password: Option<&str>,
    ) -> PyResult<()> {
        let owner_pw = owner_password.unwrap_or(password);
        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();

        // AES-256 (PDF 2.0, V=5, R=6) — random key generated internally.
        let state = lopdf::aes256_encryption_state(owner_pw, password, LopdfPermissions::all())
            .map_err(|e| PdfluentError::new_err(format!("encryption setup failed: {e}")))?;

        doc.encrypt(&state)
            .map_err(|e| PdfluentError::new_err(format!("encryption failed: {e}")))?;

        doc.save(output_path)
            .map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
        Ok(())
    }

    /// Save a decrypted copy of an encrypted document.
    ///
    /// Parameters
    /// ----------
    /// output_path : str
    ///     Destination file path for the decrypted PDF.
    /// password : str
    ///     User or owner password.
    fn decrypt(&self, output_path: &str, password: &str) -> PyResult<()> {
        let mut doc = LopdfDocument::load_mem_with_options(
            self.raw_bytes.as_ref(),
            lopdf::LoadOptions::with_password(password),
        )
        .map_err(|e| {
            PdfluentEncryptedError::new_err(format!("failed to open with password: {e}"))
        })?;
        remove_encryption(&mut doc);
        doc.save(output_path)
            .map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        if self.closed.load(Ordering::SeqCst) {
            return "Document(closed)".to_string();
        }
        match self.inner.lock().unwrap().as_ref() {
            Some(engine) => format!("Document(pages={})", engine.page_count()),
            None => "Document(locked)".to_string(),
        }
    }
}

impl PyDocument {
    /// Lazily initialise the lopdf document from raw bytes.
    ///
    /// Returns a `MutexGuard` holding `Some(LopdfDocument)`.
    fn init_lopdf(&self) -> PyResult<std::sync::MutexGuard<'_, Option<LopdfDocument>>> {
        let mut guard = self.lopdf.lock().unwrap();
        if guard.is_none() {
            match LopdfDocument::load_mem(self.raw_bytes.as_ref()) {
                Ok(doc) => *guard = Some(doc),
                Err(e) => {
                    return Err(PdfluentError::new_err(format!(
                        "failed to load PDF for mutation: {e}"
                    )));
                }
            }
        }
        Ok(guard)
    }
}

// ---------------------------------------------------------------------------
// SignatureResult
// ---------------------------------------------------------------------------

/// The validation result for a single digital signature.
///
/// Returned by :meth:`Document.validate_signatures` (full cryptographic
/// validation) and :meth:`Document.signatures` (metadata only, ``status`` is
/// always ``"unknown"``). Mirrors the Node ``SignatureResult`` parity surface.
#[pyclass(name = "SignatureResult")]
struct PySignatureResult {
    /// Validation status: ``"valid"``, ``"invalid"``, or ``"unknown"``.
    #[pyo3(get)]
    status: String,
    /// Reason for an ``"invalid"`` / ``"unknown"`` status, else ``None``.
    #[pyo3(get)]
    reason: Option<String>,
    /// Fully qualified signature field name.
    #[pyo3(get)]
    field_name: String,
    /// Signer common name (from the certificate), if available.
    #[pyo3(get)]
    signer: Option<String>,
    /// Signing timestamp as a string, if available.
    #[pyo3(get)]
    timestamp: Option<String>,
}

#[pymethods]
impl PySignatureResult {
    fn __repr__(&self) -> String {
        format!(
            "SignatureResult(status={:?}, field_name={:?}, signer={:?})",
            self.status, self.field_name, self.signer
        )
    }
}

// ---------------------------------------------------------------------------
// Page iterator
// ---------------------------------------------------------------------------

#[pyclass]
struct PyPageIterator {
    doc: Arc<PdfDocument>,
    index: usize,
    count: usize,
    parent: Py<PyDocument>,
    lopdf: Arc<Mutex<Option<LopdfDocument>>>,
    snapshot: Arc<Vec<u8>>,
}

#[pymethods]
impl PyPageIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> Option<PyPage> {
        if self.index >= self.count {
            return None;
        }
        let page = PyPage {
            doc: self.doc.clone(),
            index: self.index,
            refreshed: AtomicBool::new(false),
            clip: Mutex::new(None),
            parent: self.parent.clone_ref(py),
            lopdf: Arc::clone(&self.lopdf),
            snapshot: Arc::clone(&self.snapshot),
        };
        self.index += 1;
        Some(page)
    }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// A single page in a PDF document.
///
/// Access via indexing: ``doc[0]`` or iteration: ``for page in doc``.
#[pyclass(name = "Page")]
pub(crate) struct PyPage {
    doc: Arc<PdfDocument>,
    index: usize,
    refreshed: AtomicBool,
    clip: Mutex<Option<[f64; 4]>>,
    parent: Py<PyDocument>,
    /// Mutation channel shared with the parent document (lane L4): page
    /// edits write the mutated bytes back here so `save()` observes them.
    lopdf: Arc<Mutex<Option<LopdfDocument>>>,
    /// Document bytes as of page creation, used when the channel is empty.
    snapshot: Arc<Vec<u8>>,
}

/// Engine access for the compat lanes (added by L1; other lanes reuse
/// these rather than adding siblings).
impl PyPage {
    pub(crate) fn compat_engine(&self) -> &Arc<PdfDocument> {
        &self.doc
    }

    pub(crate) fn compat_index(&self) -> usize {
        self.index
    }

    /// Clip `clip_to_rect` laid over this page, if any.
    pub(crate) fn compat_clip(&self) -> Option<[f64; 4]> {
        *self.clip.lock().unwrap()
    }

    /// Lay a clip over this page.
    pub(crate) fn set_compat_clip(&self, clip: [f64; 4]) {
        *self.clip.lock().unwrap() = Some(clip);
    }

    /// Whether `refresh` invalidated this page's number.
    pub(crate) fn compat_refreshed(&self) -> bool {
        self.refreshed.load(Ordering::SeqCst)
    }

    /// Invalidate this page's number.
    pub(crate) fn mark_compat_refreshed(&self) {
        self.refreshed.store(true, Ordering::SeqCst);
    }
}

#[pymethods]
impl PyPage {
    /// A page only comes from a document: direct construction raises.
    #[new]
    #[pyo3(signature = (page, document))]
    fn new(
        page: &Bound<'_, pyo3::types::PyAny>,
        document: Py<PyDocument>,
        py: Python<'_>,
    ) -> PyResult<Self> {
        let _ = (page, document, py);
        Err(crate::compat::errors::validation_error(
            crate::compat::errors::ValidationKind::Precondition,
            "Page() cannot wrap a page; use Document.load_page()",
        ))
    }

    /// Page index (0-based).
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Page width in points.
    #[getter]
    fn width(&self) -> PyResult<f64> {
        let geom = self
            .doc
            .page_geometry(self.index)
            .map_err(engine_err_to_py)?;
        Ok(geom.effective_dimensions().0)
    }

    /// Page height in points.
    #[getter]
    fn height(&self) -> PyResult<f64> {
        let geom = self
            .doc
            .page_geometry(self.index)
            .map_err(engine_err_to_py)?;
        Ok(geom.effective_dimensions().1)
    }

    /// Page rotation in degrees.
    #[getter]
    fn rotation(&self) -> PyResult<u32> {
        let geom = self
            .doc
            .page_geometry(self.index)
            .map_err(engine_err_to_py)?;
        Ok(geom.rotation.degrees())
    }

    /// Full page geometry.
    #[getter]
    fn geometry(&self) -> PyResult<PyPageGeometry> {
        let geom = self
            .doc
            .page_geometry(self.index)
            .map_err(engine_err_to_py)?;
        Ok(PyPageGeometry(geom))
    }

    /// Render this page to a ``RenderedImage``.
    ///
    /// Parameters
    /// ----------
    /// dpi : float, optional
    ///     Resolution (default 150).
    /// width : int, optional
    ///     Force output width in pixels.
    /// height : int, optional
    ///     Force output height in pixels.
    /// background : tuple[float, float, float, float], optional
    ///     RGBA background color (0.0-1.0). Default: opaque white.
    ///
    /// Returns
    /// -------
    /// RenderedImage
    #[pyo3(signature = (dpi=150.0, width=None, height=None, background=None))]
    fn render(
        &self,
        dpi: f64,
        width: Option<u16>,
        height: Option<u16>,
        background: Option<[f32; 4]>,
    ) -> PyResult<PyRenderedImage> {
        let opts = RenderOptions {
            dpi,
            width,
            height,
            background: background.unwrap_or([1.0, 1.0, 1.0, 1.0]),
            ..Default::default()
        };
        let rendered = self
            .doc
            .render_page(self.index, &opts)
            .map_err(engine_err_to_py)?;
        Ok(PyRenderedImage(rendered))
    }

    /// Generate a thumbnail for this page.
    ///
    /// Parameters
    /// ----------
    /// max_dimension : int, optional
    ///     Maximum pixel size on the longest side (default 256).
    #[pyo3(signature = (max_dimension=256))]
    fn thumbnail(&self, max_dimension: u32) -> PyResult<PyRenderedImage> {
        let opts = ThumbnailOptions { max_dimension };
        let rendered = self
            .doc
            .thumbnail(self.index, &opts)
            .map_err(engine_err_to_py)?;
        Ok(PyRenderedImage(rendered))
    }

    /// Extract all text from this page as a string.
    fn extract_text(&self) -> PyResult<String> {
        self.doc.extract_text(self.index).map_err(engine_err_to_py)
    }

    /// Extract structured text blocks from this page.
    fn extract_text_blocks(&self) -> PyResult<Vec<PyTextBlock>> {
        let blocks = self
            .doc
            .extract_text_blocks(self.index)
            .map_err(engine_err_to_py)?;
        Ok(blocks.into_iter().map(PyTextBlock).collect())
    }

    /// This page's words, with the box each one was drawn in.
    ///
    /// Blocks are numbered within the page: ``blocks[word.block]`` names the
    /// block in this page's own :meth:`Document.extract_blocks` window.
    fn words(&self) -> PyResult<Vec<PyWord>> {
        Ok(slot_words::page_words(&self.doc, self.index)
            .map_err(engine_err_to_py)?
            .into_iter()
            .map(PyWord)
            .collect())
    }

    fn __repr__(&self) -> PyResult<String> {
        let geom = self
            .doc
            .page_geometry(self.index)
            .map_err(engine_err_to_py)?;
        let (w, h) = geom.effective_dimensions();
        Ok(format!(
            "Page(index={}, width={w:.1}, height={h:.1})",
            self.index
        ))
    }
}

// ---------------------------------------------------------------------------
// RenderedImage
// ---------------------------------------------------------------------------

/// A rendered page as RGBA pixel data.
///
/// Convert to PIL Image via ``.to_pil()`` or NumPy array via ``.to_numpy()``.
#[pyclass(name = "RenderedImage")]
struct PyRenderedImage(RenderedPage);

#[pymethods]
impl PyRenderedImage {
    /// Image width in pixels.
    #[getter]
    fn width(&self) -> u32 {
        self.0.width
    }

    /// Image height in pixels.
    #[getter]
    fn height(&self) -> u32 {
        self.0.height
    }

    /// Raw RGBA pixel data as bytes (4 bytes per pixel, row-major).
    #[getter]
    fn pixels<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.0.pixels)
    }

    /// Convert to a PIL/Pillow Image.
    ///
    /// Requires ``Pillow`` to be installed.
    ///
    /// Returns
    /// -------
    /// PIL.Image.Image
    ///     RGBA image.
    fn to_pil<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pil = py.import("PIL.Image")?;
        let size = (self.0.width, self.0.height).into_pyobject(py)?;
        let bytes = PyBytes::new(py, &self.0.pixels);
        pil.call_method1("frombytes", ("RGBA", size, bytes))
    }

    /// Convert to a NumPy array (H x W x 4, dtype=uint8).
    ///
    /// Requires ``numpy`` to be installed.
    ///
    /// Returns
    /// -------
    /// numpy.ndarray
    ///     Shape (height, width, 4), dtype uint8.
    fn to_numpy<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let np = py.import("numpy")?;
        let bytes = PyBytes::new(py, &self.0.pixels);
        let arr = np.call_method1("frombuffer", (bytes, "uint8"))?;
        let shape = (self.0.height, self.0.width, 4u32).into_pyobject(py)?;
        arr.call_method1("reshape", (shape,))
    }

    /// Save to a file (PNG, JPEG, etc. via PIL).
    ///
    /// Requires ``Pillow`` to be installed.
    fn save(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let img = self.to_pil(py)?;
        img.call_method1("save", (path,))?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!(
            "RenderedImage(width={}, height={})",
            self.0.width, self.0.height
        )
    }
}

// ---------------------------------------------------------------------------
// TextBlock / TextSpan
// ---------------------------------------------------------------------------

/// A block of text from a page (grouped by vertical proximity).
#[pyclass(name = "TextBlock")]
struct PyTextBlock(TextBlock);

#[pymethods]
impl PyTextBlock {
    /// Concatenated text of all spans in this block.
    #[getter]
    fn text(&self) -> String {
        self.0.text()
    }

    /// Individual text spans.
    #[getter]
    fn spans(&self) -> Vec<PyTextSpan> {
        self.0.spans.iter().cloned().map(PyTextSpan).collect()
    }

    fn __repr__(&self) -> String {
        let t = self.0.text();
        let preview = if t.chars().count() > 50 {
            let end = t.char_indices().nth(50).map_or(t.len(), |(i, _)| i);
            format!("{}...", &t[..end])
        } else {
            t
        };
        format!("TextBlock({preview:?})")
    }

    fn __str__(&self) -> String {
        self.0.text()
    }
}

/// The window `extract_markdown` and `extract_blocks` were asked for.
///
/// One place, so the two methods cannot drift apart about what
/// `page_count=None` means.
fn extract_options(
    first_page: usize,
    page_count: Option<usize>,
    pages: Option<Vec<usize>>,
) -> ExtractOptions {
    // No struct literal: `ExtractOptions` is `#[non_exhaustive]`, so the
    // asked-for fields are assigned onto the default.
    let mut options = ExtractOptions::default();
    options.first_page = first_page;
    options.page_count = page_count;
    options.pages = pages;
    // AND WHATEVER THE ENGINE ADDS NEXT stays at its default rather than
    // left out: this is a binding, and a field it forgets is a feature this
    // language silently does not get. `learned` arrived that way (#599) and
    // broke the build here rather than in a test, which is the lucky version
    // of the same mistake.
    options
}

/// The window `extract_markdown` was asked for.
#[allow(clippy::too_many_arguments)]
fn markdown_options(
    first_page: usize,
    page_count: Option<usize>,
    pages: Option<Vec<usize>>,
    include_headers: bool,
    include_footers: bool,
    page_separators: &str,
    code: bool,
    quality: &str,
    model_dir: Option<PathBuf>,
    table_model_dir: Option<PathBuf>,
    keep_furniture: bool,
    ocr: &str,
    ocr_language: &str,
    ocr_dpi: u32,
) -> PyResult<ExtractOptions> {
    let separators = match page_separators.to_lowercase().as_str() {
        "comment" => PageSeparators::Comment,
        "none" => PageSeparators::None,
        other => {
            return Err(PdfluentValidationError::new_err(format!(
                "unknown page_separators {other:?}; use 'comment' or 'none'"
            )));
        }
    };
    let ocr = match ocr {
        "never" => OcrMode::Never,
        "auto" => OcrMode::Auto,
        "force" => OcrMode::Force,
        other => {
            return Err(PdfluentValidationError::new_err(format!(
                "unknown ocr option '{other}'; use 'never', 'auto' or 'force'"
            )));
        }
    };
    if ocr != OcrMode::Never && ocr_dpi == 0 {
        return Err(PdfluentValidationError::new_err("ocr_dpi must be positive"));
    }
    // No struct literal: `ExtractOptions` is `#[non_exhaustive]`.
    let mut options = ExtractOptions::default();
    options.first_page = first_page;
    options.page_count = page_count;
    options.pages = pages;
    options.include_headers = include_headers;
    options.include_footers = include_footers;
    options.page_separators = separators;
    options.code = code;
    options.keep_furniture = keep_furniture;
    options.ocr = ocr;
    options.ocr_language = ocr_language.to_string();
    options.ocr_dpi = ocr_dpi;
    tier_options(options, quality, model_dir, table_model_dir)
}

/// The tier `extract_markdown` and `extract_blocks` were asked for.
///
/// One place, so the two methods cannot drift apart about what `quality`
/// names: `fast` is the shipping engine, `balanced` the layout model tier,
/// and anything else a mistake in the call. `model_dir` points the tier at
/// its weights; `table_model_dir` sets the TATR opt-in over the library
/// defaults, exactly as the CLI's `--table-model-dir` maps it.
fn tier_options(
    mut options: ExtractOptions,
    quality: &str,
    model_dir: Option<PathBuf>,
    table_model_dir: Option<PathBuf>,
) -> PyResult<ExtractOptions> {
    options.quality = match quality {
        "fast" => Quality::Fast,
        "balanced" => Quality::Balanced,
        other => {
            return Err(PdfluentValidationError::new_err(format!(
                "unknown quality {other:?}; use 'fast' or 'balanced'"
            )));
        }
    };
    options.model_dir = model_dir;
    if let Some(dir) = table_model_dir {
        let mut tatr = TatrOptions::default();
        tatr.model_dir = Some(dir);
        options = options.with_tatr(tatr);
    }
    Ok(options)
}

/// One block of a document's structure: what a caller chunks, cites or embeds
/// (#573).
///
/// `kind` is one of ``paragraph``, ``heading``, ``list_item``, ``table_cell``,
/// a value while the rest are ``None`` -- ``level`` on a heading and a list
/// item, ``ordinal`` on a list item, ``row``/``column``/``row_span``/
/// ``column_span``/``header`` on a table cell. A reader in Python asks for the
/// attribute it wants rather than matching on a shape, which is why this is
/// flat and the Rust enum is not.
#[pyclass(name = "Block")]
#[derive(Clone)]
pub(crate) struct PyBlock(pub(crate) Block);

/// Why one page may be worth sending to a vision model (#624).
///
/// ``needs_vision`` is true when any reason holds. Only ``no_text_layer`` is
/// measured today; ``low_confidence_table`` and ``figure`` are always
/// ``False`` until the engine has a table probability and a figure detector.
#[pyclass(name = "VisionAdvice", frozen)]
#[derive(Clone)]
struct PyVisionAdvice {
    /// The page, 0-based.
    #[pyo3(get)]
    page: usize,
    /// The page draws ink but yields no text: a scan, or text drawn as curves.
    #[pyo3(get)]
    no_text_layer: bool,
    /// The page holds a table the table model is unsure of. Always ``False``
    /// today.
    #[pyo3(get)]
    low_confidence_table: bool,
    /// The page holds a chart or a figure whose content is not text. Always
    /// ``False`` today.
    #[pyo3(get)]
    figure: bool,
}

#[pymethods]
impl PyVisionAdvice {
    /// Whether any reason holds.
    #[getter]
    fn needs_vision(&self) -> bool {
        self.no_text_layer || self.low_confidence_table || self.figure
    }

    fn __repr__(&self) -> String {
        format!(
            "VisionAdvice(page={}, needs_vision={})",
            self.page,
            if self.needs_vision() { "True" } else { "False" }
        )
    }
}

#[pymethods]
impl PyBlock {
    /// ``paragraph``, ``heading``, ``list_item``, ``table_cell``, ``header``
    /// or ``footer``.
    #[getter]
    fn kind(&self) -> &'static str {
        match self.0.kind {
            BlockKind::Paragraph => "paragraph",
            BlockKind::Heading { .. } => "heading",
            BlockKind::ListItem { .. } => "list_item",
            BlockKind::TableCell { .. } => "table_cell",
            BlockKind::Header => "header",
            BlockKind::Footer => "footer",
            BlockKind::Caption => "caption",
            BlockKind::Figure => "figure",
            _ => "paragraph",
        }
    }

    /// The block's text, the line breaks of the measure taken out.
    #[getter]
    fn text(&self) -> String {
        self.0.text.clone()
    }

    /// 0-based page index the block was drawn on.
    #[getter]
    fn page(&self) -> usize {
        self.0.page
    }

    /// ``[x0, y0, x1, y1]`` in PDF user space, y up.
    #[getter]
    fn bbox(&self) -> [f64; 4] {
        self.0.bbox
    }

    /// Place in the document's reading order, counted from the first block of
    /// the first page -- the document's numbering, not the window's.
    #[getter]
    fn index(&self) -> usize {
        self.0.index
    }

    /// Whether the block continues the one before it, because the page broke a
    /// paragraph in two.
    #[getter]
    fn continues_previous(&self) -> bool {
        self.0.continues_previous
    }

    /// How sure the learned model is of :attr:`kind`, between 0 and 1, or
    /// ``None`` where no model spoke about this block.
    ///
    /// ``None`` IS NOT LOW CONFIDENCE. A block no model has an opinion about
    /// carries the heuristics' verdict, and the heuristics produce a decision
    /// rather than a probability -- so ``None`` means "nobody measured this".
    /// Code that routes the doubtful cases somewhere must treat it as unknown
    /// and not as zero.
    #[getter]
    fn confidence(&self) -> Option<f64> {
        self.0.confidence
    }

    /// What the heuristic builder had to go on when it found this cell's TABLE,
    /// as ``(thin_rows, fill, retried_from, full_rows, columns)``, or ``None`` for
    /// a block that is not a table cell.
    ///
    /// Five facts and not a score: a weighted single number would read as a
    /// measurement and could not be tested a part at a time. The value is a
    /// property of the table, so every cell of one table returns the same tuple.
    #[getter]
    fn grid(&self) -> Option<(usize, f64, usize, usize, usize)> {
        self.0
            .grid
            .map(|g| (g.thin_rows, g.fill, g.retried_from, g.full_rows, g.columns))
    }

    /// A heading's level (1, 2 or 3) or a list item's nesting depth (0 for the
    /// outermost); ``None`` for every other kind.
    #[getter]
    fn level(&self) -> Option<u8> {
        match self.0.kind {
            BlockKind::Heading { level } => Some(level),
            BlockKind::ListItem { level, .. } => Some(level),
            _ => None,
        }
    }

    /// The number the document wrote on a list item -- ``3.`` is 3, ``iv.`` is
    /// 4 -- or ``None`` for a bullet and for every other kind.
    #[getter]
    fn ordinal(&self) -> Option<u32> {
        match self.0.kind {
            BlockKind::ListItem { ordinal, .. } => ordinal,
            _ => None,
        }
    }

    /// A table cell's 0-based row.
    #[getter]
    fn row(&self) -> Option<u16> {
        match self.0.kind {
            BlockKind::TableCell { row, .. } => Some(row),
            _ => None,
        }
    }

    /// A table cell's 0-based column, counted across the whole table.
    #[getter]
    fn column(&self) -> Option<u16> {
        match self.0.kind {
            BlockKind::TableCell { column, .. } => Some(column),
            _ => None,
        }
    }

    /// How many rows a table cell covers.
    #[getter]
    fn row_span(&self) -> Option<u16> {
        match self.0.kind {
            BlockKind::TableCell { row_span, .. } => Some(row_span),
            _ => None,
        }
    }

    /// How many columns a table cell covers.
    #[getter]
    fn column_span(&self) -> Option<u16> {
        match self.0.kind {
            BlockKind::TableCell { column_span, .. } => Some(column_span),
            _ => None,
        }
    }

    /// Whether a table cell is in the table's header row.
    #[getter]
    fn header(&self) -> Option<bool> {
        match self.0.kind {
            BlockKind::TableCell { header, .. } => Some(header),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        let text = self.text();
        let preview = if text.chars().count() > 50 {
            let end = text.char_indices().nth(50).map_or(text.len(), |(i, _)| i);
            format!("{}...", &text[..end])
        } else {
            text
        };
        format!("Block({}, page={}, {preview:?})", self.kind(), self.0.page)
    }

    fn __str__(&self) -> String {
        self.text()
    }
}

/// One word of a document's text, with the box it was drawn in.
///
/// ``block`` is the position in the list :meth:`Document.extract_blocks`
/// returns under the same window -- the window's numbering, not the
/// document's -- and ``line`` the 0-based line within that block.
#[pyclass(name = "Word")]
#[derive(Clone)]
struct PyWord(Word);

#[pymethods]
impl PyWord {
    /// The word, without surrounding whitespace.
    #[getter]
    fn text(&self) -> String {
        self.0.text.clone()
    }

    /// 0-based page index the word was drawn on.
    #[getter]
    fn page(&self) -> usize {
        self.0.page
    }

    /// ``[x0, y0, x1, y1]`` in PDF user space, y up.
    #[getter]
    fn bbox(&self) -> [f64; 4] {
        self.0.bbox
    }

    /// Position of the word's block in the ``extract_blocks`` answer.
    #[getter]
    fn block(&self) -> usize {
        self.0.block
    }

    /// 0-based line within the word's block.
    #[getter]
    fn line(&self) -> usize {
        self.0.line
    }

    /// 0-based word within the word's line, in the order the line reads.
    #[getter]
    fn word(&self) -> usize {
        self.0.word
    }

    /// Baseline angle in degrees, counter-clockwise from +x; 0 upright.
    #[getter]
    fn dir(&self) -> f64 {
        self.0.dir
    }

    /// Whether the word's span is set bold.
    #[getter]
    fn bold(&self) -> bool {
        self.0.bold
    }

    /// Whether the word's span is set italic.
    #[getter]
    fn italic(&self) -> bool {
        self.0.italic
    }

    /// Font size of the word's span, in points.
    #[getter]
    fn font_size(&self) -> f64 {
        self.0.font_size
    }

    fn __repr__(&self) -> String {
        format!(
            "Word({:?}, page={}, block={}, line={})",
            self.0.text, self.0.page, self.0.block, self.0.line
        )
    }

    fn __str__(&self) -> String {
        self.0.text.clone()
    }
}

/// A single text span at a specific position.
///
#[pyclass(name = "TextSpan")]
#[derive(Clone)]
pub(crate) struct PyTextSpan(pub(crate) TextSpan);

#[pymethods]
impl PyTextSpan {
    /// The text content.
    #[getter]
    fn text(&self) -> &str {
        &self.0.text
    }

    /// X position in PDF user space.
    #[getter]
    fn x(&self) -> f64 {
        self.0.x
    }

    /// Y position in PDF user space.
    #[getter]
    fn y(&self) -> f64 {
        self.0.y
    }

    /// Approximate font size.
    #[getter]
    fn font_size(&self) -> f64 {
        self.0.font_size
    }

    /// Style and layout bits: 16 bold, 2 italic, 8 monospace, 32 hidden,
    /// 64 non-horizontal (lane L1).
    ///
    /// The first three mirror the front door's own span flags; hidden marks
    /// text the page hides (render mode 3 or zero alpha), which default
    /// extraction drops, and non-horizontal marks text the door's line
    /// grouping skips.
    #[getter]
    fn flags(&self) -> i32 {
        crate::door_markdown::span_flags(&self.0)
    }

    #[getter]
    fn font_name(&self) -> Option<String> {
        self.0.font_name.clone()
    }

    #[getter]
    fn is_bold(&self) -> Option<bool> {
        None
    }

    #[getter]
    fn is_italic(&self) -> Option<bool> {
        None
    }

    /// Foreground color as ``(r, g, b)`` floats in 0.0–1.0, or ``None`` if not yet available (G1).
    #[getter]
    fn color(&self) -> Option<(f32, f32, f32)> {
        None
    }

    fn __repr__(&self) -> String {
        format!(
            "TextSpan({:?}, x={:.1}, y={:.1}, size={:.1})",
            self.0.text, self.0.x, self.0.y, self.0.font_size
        )
    }
}

// ---------------------------------------------------------------------------
// DocumentInfo
// ---------------------------------------------------------------------------

/// Decode a PDF text string: UTF-16BE with BOM, else raw bytes as Latin-1.
fn info_text(obj: &LopdfObject) -> String {
    let LopdfObject::String(bytes, _) = obj else {
        return String::new();
    };
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|b| *b as char).collect()
}

/// One info-dict entry as text; names (such as `/Trapped`) read bare.
fn info_field(info: Option<&lopdf::Dictionary>, key: &[u8]) -> String {
    let Some(dict) = info else {
        return String::new();
    };
    match dict.get(key) {
        Ok(LopdfObject::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        Ok(obj) => info_text(obj),
        Err(_) => String::new(),
    }
}

/// The encryption note (`Standard V1 R2 40-bit RC4`), if the bytes carry one.
fn encryption_note(parsed: &LopdfDocument) -> Option<String> {
    if !parsed.is_encrypted() {
        return None;
    }
    let dict = match parsed.trailer.get(b"Encrypt") {
        Ok(LopdfObject::Reference(id)) => parsed.get_dictionary(*id).ok()?.clone(),
        Ok(LopdfObject::Dictionary(dict)) => dict.clone(),
        _ => return Some("Standard".to_string()),
    };
    let filter = dict
        .get(b"Filter")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_else(|| "Standard".to_string());
    let number = |key: &[u8], default: i64| {
        dict.get(key)
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(default)
    };
    let version = number(b"V", 1);
    let revision = number(b"R", 2);
    let bits = number(b"Length", 40);
    let algorithm = if version >= 5 {
        "AES-256"
    } else if version == 4 && uses_aes(&dict, parsed) {
        "AES-128"
    } else {
        "RC4"
    };
    Some(format!(
        "{filter} V{version} R{revision} {bits}-bit {algorithm}"
    ))
}

/// Whether a V4 encryption dict decodes streams with AES.
fn uses_aes(dict: &lopdf::Dictionary, parsed: &LopdfDocument) -> bool {
    let crypt = dict.get(b"CF").ok().and_then(|o| match o {
        LopdfObject::Reference(id) => parsed.get_dictionary(*id).ok().cloned(),
        LopdfObject::Dictionary(inner) => Some(inner.clone()),
        _ => None,
    });
    let Some(crypt) = crypt else {
        return false;
    };
    let entry = crypt
        .iter()
        .next()
        .map(|(_, o)| o.clone())
        .and_then(|o| match o {
            LopdfObject::Reference(id) => parsed.get_dictionary(id).ok().cloned(),
            LopdfObject::Dictionary(inner) => Some(inner),
            _ => None,
        });
    entry
        .as_ref()
        .and_then(|d| d.get(b"CFM").ok())
        .and_then(|o| o.as_name().ok())
        .is_some_and(|name| name == b"AESV2")
}

/// The `PDF x.y` format note, from the header or the parsed version.
fn format_note(bytes: &[u8], parsed: Option<&LopdfDocument>) -> String {
    if let Some(doc) = parsed {
        if !doc.version.is_empty() {
            return format!("PDF {}", doc.version);
        }
    }
    let line_end = bytes
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(bytes.len());
    let line = &bytes[..line_end.min(16)];
    if let Some(version) = line.strip_prefix(b"%PDF-") {
        return format!("PDF {}", String::from_utf8_lossy(version).trim());
    }
    String::new()
}

/// Read the info dict off the current bytes, decrypted when possible.
///
/// A locked file still reports its format and encryption note; its strings
/// stay empty until `authenticate` unlocks it.
fn read_document_info(doc: &PyDocument) -> PyDocumentInfo {
    let bytes = doc.current_bytes().unwrap_or_default();
    let mut parsed = lopdf::Document::load_mem(&bytes).ok();
    // The note reads the live `/Encrypt` entry; decryption below strips it.
    let encryption = parsed.as_ref().and_then(encryption_note);
    let mut trusted = true;
    if let Some(ref mut handle) = parsed {
        if handle.is_encrypted() {
            let password = doc.password.lock().unwrap().clone().unwrap_or_default();
            let verified = handle.authenticate_user_password(&password).is_ok()
                || handle.authenticate_owner_password(&password).is_ok();
            trusted = verified && handle.decrypt(&password).is_ok();
        }
    }
    let info: Option<lopdf::Dictionary> = if trusted {
        parsed.as_ref().and_then(|handle| {
            // The info dict is usually an indirect reference, but writers
            // may inline it in the trailer; both shapes read the same.
            match handle.trailer.get(b"Info") {
                Ok(LopdfObject::Reference(id)) => handle.get_dictionary(*id).ok().cloned(),
                Ok(LopdfObject::Dictionary(dict)) => Some(dict.clone()),
                _ => None,
            }
        })
    } else {
        None
    };
    let info = info.as_ref();
    PyDocumentInfo {
        title: info_field(info, b"Title"),
        author: info_field(info, b"Author"),
        subject: info_field(info, b"Subject"),
        keywords: info_field(info, b"Keywords"),
        creator: info_field(info, b"Creator"),
        producer: info_field(info, b"Producer"),
        creation_date: info_field(info, b"CreationDate"),
        mod_date: info_field(info, b"ModDate"),
        format: format_note(&bytes, parsed.as_ref()),
        encryption,
        trapped: info_field(info, b"Trapped"),
    }
}

/// Document metadata (title, author, subject, etc.).
///
/// Attribute access keeps the native shape (`None` when a field is empty);
/// the mapping protocol reads the compatible shape (empty strings, plus the
/// `format` and `encryption` notes), so `dict(doc.metadata)` matches the
/// recorded lane output.
#[pyclass(name = "DocumentInfo")]
struct PyDocumentInfo {
    title: String,
    author: String,
    subject: String,
    keywords: String,
    creator: String,
    producer: String,
    creation_date: String,
    mod_date: String,
    format: String,
    encryption: Option<String>,
    trapped: String,
}

#[pymethods]
impl PyDocumentInfo {
    #[getter]
    fn title(&self) -> Option<&str> {
        Self::present(&self.title)
    }

    #[getter]
    fn author(&self) -> Option<&str> {
        Self::present(&self.author)
    }

    #[getter]
    fn subject(&self) -> Option<&str> {
        Self::present(&self.subject)
    }

    #[getter]
    fn keywords(&self) -> Option<&str> {
        Self::present(&self.keywords)
    }

    #[getter]
    fn creator(&self) -> Option<&str> {
        Self::present(&self.creator)
    }

    #[getter]
    fn producer(&self) -> Option<&str> {
        Self::present(&self.producer)
    }

    #[getter]
    fn creation_date(&self) -> Option<&str> {
        Self::present(&self.creation_date)
    }

    #[getter]
    fn mod_date(&self) -> Option<&str> {
        Self::present(&self.mod_date)
    }

    #[getter]
    fn format(&self) -> Option<&str> {
        Self::present(&self.format)
    }

    #[getter]
    fn encryption(&self) -> Option<&str> {
        self.encryption.as_deref()
    }

    #[getter]
    fn trapped(&self) -> Option<&str> {
        Self::present(&self.trapped)
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        let value: PyObject = match key {
            "title" => self.title.clone().into_pyobject(py)?.into_any().unbind(),
            "author" => self.author.clone().into_pyobject(py)?.into_any().unbind(),
            "subject" => self.subject.clone().into_pyobject(py)?.into_any().unbind(),
            "keywords" => self.keywords.clone().into_pyobject(py)?.into_any().unbind(),
            "creator" => self.creator.clone().into_pyobject(py)?.into_any().unbind(),
            "producer" => self.producer.clone().into_pyobject(py)?.into_any().unbind(),
            "creationDate" => self
                .creation_date
                .clone()
                .into_pyobject(py)?
                .into_any()
                .unbind(),
            "modDate" => self.mod_date.clone().into_pyobject(py)?.into_any().unbind(),
            "format" => self.format.clone().into_pyobject(py)?.into_any().unbind(),
            "encryption" => self
                .encryption
                .clone()
                .into_pyobject(py)?
                .into_any()
                .unbind(),
            "trapped" => self.trapped.clone().into_pyobject(py)?.into_any().unbind(),
            _ => {
                return Err(pyo3::exceptions::PyKeyError::new_err(format!(
                    "unknown metadata key {key:?}"
                )));
            }
        };
        Ok(value)
    }

    fn __len__(&self) -> usize {
        Self::KEYS.len()
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        use pyo3::types::PyTuple;
        let tuple = PyTuple::new(py, Self::KEYS)?;
        Ok(tuple.call_method0("__iter__")?.unbind())
    }

    fn __contains__(&self, key: &str) -> bool {
        Self::KEYS.contains(&key)
    }

    fn keys(&self) -> Vec<&str> {
        Self::KEYS.to_vec()
    }

    fn get(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyResult<PyObject> {
        match self.__getitem__(py, key) {
            Ok(value) => Ok(value),
            Err(_) => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn __repr__(&self) -> String {
        let title = if self.title.is_empty() {
            "<untitled>"
        } else {
            &self.title
        };
        format!("DocumentInfo(title={title:?})")
    }
}

impl PyDocumentInfo {
    /// Mapping keys, in the recorded order.
    const KEYS: [&'static str; 11] = [
        "title",
        "author",
        "subject",
        "keywords",
        "creator",
        "producer",
        "creationDate",
        "modDate",
        "format",
        "encryption",
        "trapped",
    ];

    /// Attribute shape: `None` for an empty field.
    fn present(field: &str) -> Option<&str> {
        if field.is_empty() {
            None
        } else {
            Some(field)
        }
    }
}

// ---------------------------------------------------------------------------
// Bookmark
// ---------------------------------------------------------------------------

/// A bookmark (outline item) in the document.
#[pyclass(name = "Bookmark")]
struct PyBookmark(BookmarkItem);

#[pymethods]
impl PyBookmark {
    /// Bookmark title.
    #[getter]
    fn title(&self) -> &str {
        &self.0.title
    }

    /// Target page index (0-based), or None.
    #[getter]
    fn page(&self) -> Option<usize> {
        self.0.page
    }

    /// Child bookmarks.
    #[getter]
    fn children(&self) -> Vec<PyBookmark> {
        self.0.children.iter().cloned().map(PyBookmark).collect()
    }

    fn __repr__(&self) -> String {
        format!("Bookmark({:?})", self.0.title)
    }
}

// ---------------------------------------------------------------------------
// PageGeometry
// ---------------------------------------------------------------------------

/// Full page geometry (boxes, rotation).
#[pyclass(name = "PageGeometry")]
struct PyPageGeometry(PageGeometry);

#[pymethods]
impl PyPageGeometry {
    /// MediaBox as (x0, y0, x1, y1).
    #[getter]
    fn media_box(&self) -> (f64, f64, f64, f64) {
        let b = &self.0.media_box;
        (b.x0, b.y0, b.x1, b.y1)
    }

    /// CropBox as (x0, y0, x1, y1).
    #[getter]
    fn crop_box(&self) -> (f64, f64, f64, f64) {
        let b = &self.0.crop_box;
        (b.x0, b.y0, b.x1, b.y1)
    }

    /// Rotation in degrees.
    #[getter]
    fn rotation(&self) -> u32 {
        self.0.rotation.degrees()
    }

    /// Effective width in points (accounting for rotation).
    #[getter]
    fn width(&self) -> f64 {
        self.0.effective_dimensions().0
    }

    /// Effective height in points (accounting for rotation).
    #[getter]
    fn height(&self) -> f64 {
        self.0.effective_dimensions().1
    }

    /// Pixel dimensions at the given DPI.
    fn pixel_dimensions(&self, dpi: f64) -> (u32, u32) {
        self.0.pixel_dimensions(dpi)
    }

    fn __repr__(&self) -> String {
        let (w, h) = self.0.effective_dimensions();
        format!(
            "PageGeometry(width={w:.1}, height={h:.1}, rotation={})",
            self.0.rotation.degrees()
        )
    }
}

// ---------------------------------------------------------------------------
// ComplianceIssue / ComplianceReport
// ---------------------------------------------------------------------------

/// A single compliance issue found during PDF/A validation.
#[pyclass(name = "ComplianceIssue")]
struct PyComplianceIssue(pdf_compliance::ComplianceIssue);

#[pymethods]
impl PyComplianceIssue {
    /// Rule identifier (e.g. "6.1.2" for PDF/A clause).
    #[getter]
    fn rule(&self) -> &str {
        &self.0.rule
    }

    /// Severity: ``"error"``, ``"warning"``, or ``"info"``.
    #[getter]
    fn severity(&self) -> &'static str {
        match self.0.severity {
            pdf_compliance::Severity::Error => "error",
            pdf_compliance::Severity::Warning => "warning",
            pdf_compliance::Severity::Info => "info",
        }
    }

    /// Human-readable description of the issue.
    #[getter]
    fn message(&self) -> &str {
        &self.0.message
    }

    /// Location in the document (object number, page, etc.), or ``None``.
    #[getter]
    fn location(&self) -> Option<&str> {
        self.0.location.as_deref()
    }

    fn __repr__(&self) -> String {
        format!(
            "ComplianceIssue(severity={:?}, rule={:?}, message={:?})",
            self.severity(),
            self.0.rule,
            self.0.message,
        )
    }
}

/// Result of a PDF/A compliance validation.
///
/// Attributes
/// ----------
/// is_compliant : bool
///     True if no errors were found (warnings/info are allowed).
/// error_count : int
///     Number of conformance errors.
/// warning_count : int
///     Number of warnings.
/// issues : list[ComplianceIssue]
///     All issues found.
/// pdfa_level : str or None
///     The detected/validated PDF/A level (e.g. ``"PDF/A-2B"``), or ``None``.
#[pyclass(name = "ComplianceReport")]
struct PyComplianceReport(pdf_compliance::ComplianceReport);

#[pymethods]
impl PyComplianceReport {
    /// True if no conformance errors were found.
    #[getter]
    fn is_compliant(&self) -> bool {
        self.0.is_compliant()
    }

    /// Number of conformance errors.
    #[getter]
    fn error_count(&self) -> usize {
        self.0.error_count()
    }

    /// Number of warnings.
    #[getter]
    fn warning_count(&self) -> usize {
        self.0.warning_count()
    }

    /// All issues found during validation.
    #[getter]
    fn issues(&self) -> Vec<PyComplianceIssue> {
        self.0
            .issues
            .iter()
            .cloned()
            .map(PyComplianceIssue)
            .collect()
    }

    /// Detected PDF/A level string (e.g. ``"PDF/A-2B"``), or ``None``.
    #[getter]
    fn pdfa_level(&self) -> Option<String> {
        self.0
            .pdfa_level
            .map(|l| format!("PDF/A-{}{}", l.part(), l.conformance()))
    }

    fn __repr__(&self) -> String {
        format!(
            "ComplianceReport(compliant={}, errors={}, warnings={})",
            self.0.is_compliant(),
            self.0.error_count(),
            self.0.warning_count(),
        )
    }
}

// ---------------------------------------------------------------------------
// FormField
// ---------------------------------------------------------------------------

/// An interactive form field (AcroForm widget).
#[pyclass(name = "FormField")]
#[derive(Clone)]
struct PyFormField {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    field_type: String,
    #[pyo3(get)]
    value: Option<String>,
    #[pyo3(get)]
    page: Option<usize>,
}

#[pymethods]
impl PyFormField {
    fn __repr__(&self) -> String {
        format!(
            "FormField(name={:?}, type={:?}, value={:?})",
            self.name, self.field_type, self.value
        )
    }
}

// ---------------------------------------------------------------------------
// Annotation
// ---------------------------------------------------------------------------

/// A PDF annotation (highlight, freetext, etc.).
#[pyclass(name = "Annotation")]
#[derive(Clone)]
struct PyAnnotation {
    #[pyo3(get)]
    page: usize,
    #[pyo3(get)]
    annot_type: String,
    /// Bounding box as (x0, y0, x1, y1) in PDF user-space points.
    #[pyo3(get)]
    rect: (f64, f64, f64, f64),
    #[pyo3(get)]
    contents: Option<String>,
    #[pyo3(get)]
    author: Option<String>,
}

#[pymethods]
impl PyAnnotation {
    fn __repr__(&self) -> String {
        format!(
            "Annotation(page={}, type={:?}, contents={:?})",
            self.page, self.annot_type, self.contents
        )
    }
}

// ---------------------------------------------------------------------------
// RedactReport
// ---------------------------------------------------------------------------

/// Result of a search-and-redact operation.
#[pyclass(name = "RedactReport")]
struct PyRedactReport {
    #[pyo3(get)]
    matches_found: usize,
    #[pyo3(get)]
    areas_redacted: usize,
    #[pyo3(get)]
    pages_affected: usize,
}

#[pymethods]
impl PyRedactReport {
    fn __repr__(&self) -> String {
        format!(
            "RedactReport(matches={}, redacted={}, pages={})",
            self.matches_found, self.areas_redacted, self.pages_affected
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Open a PDF from a file path, returning a ``Document``.
///
/// Equivalent to ``Document(path)`` but reads more naturally in code.
///
/// Parameters
/// ----------
/// path : str
///     File-system path to the PDF.
/// password : str, optional
///     Password for encrypted PDFs.
///
/// Returns
/// -------
/// Document
///
/// Raises
/// ------
/// PdfluentParseError
///     If the file is not a valid PDF.
/// PdfluentIoError
///     If the file cannot be read.
#[pyfunction]
#[pyo3(signature = (path, password=None))]
fn open_pdf(path: &str, password: Option<&str>) -> PyResult<PyDocument> {
    let data = std::fs::read(path).map_err(|e| PdfluentIoError::new_err(format!("{path}: {e}")))?;
    let raw_bytes = Arc::new(data);
    let doc = match password {
        Some(pw) => {
            PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw).map_err(engine_err_to_py)?
        }
        None => PdfDocument::open(Arc::clone(&raw_bytes)).map_err(engine_err_to_py)?,
    };
    Ok(PyDocument {
        inner: Mutex::new(Some(Arc::new(doc))),
        raw_bytes,
        name: path.to_string(),
        lopdf: Arc::new(Mutex::new(None)),
        closed: AtomicBool::new(false),
        dirty: AtomicBool::new(false),
        password: Mutex::new(password.map(str::to_string)),
        journal: pdfluent::compat::edit::Journal::default(),
    })
}

/// Merge multiple PDF files into a single output file.
///
/// Parameters
/// ----------
/// input_paths : list[str]
///     Ordered list of PDF paths to merge.
/// output_path : str
///     Destination path for the merged PDF.
///
/// Examples
/// --------
/// >>> merge_pdfs(["a.pdf", "b.pdf"], "merged.pdf")
#[pyfunction]
fn merge_pdfs(input_paths: Vec<String>, output_path: &str) -> PyResult<()> {
    if input_paths.is_empty() {
        return Err(PdfluentValidationError::new_err(
            "input_paths must not be empty",
        ));
    }
    let mut doc = pages::merge(&input_paths).map_err(manip_err_to_py)?;
    doc.save(output_path)
        .map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
    Ok(())
}

/// Decrypt a password-protected PDF and write the decrypted copy to a file.
///
/// Parameters
/// ----------
/// input_path : str
///     Path to the encrypted PDF.
/// output_path : str
///     Destination path for the decrypted PDF.
/// password : str
///     User or owner password.
#[pyfunction]
fn decrypt_pdf(input_path: &str, output_path: &str, password: &str) -> PyResult<()> {
    let data = std::fs::read(input_path)
        .map_err(|e| PdfluentIoError::new_err(format!("{input_path}: {e}")))?;
    let mut doc =
        LopdfDocument::load_mem_with_options(&data, lopdf::LoadOptions::with_password(password))
            .map_err(|e| {
                PdfluentEncryptedError::new_err(format!("failed to open with password: {e}"))
            })?;
    remove_encryption(&mut doc);
    doc.save(output_path)
        .map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
    Ok(())
}

/// Validate a PDF file against PDF/A conformance requirements.
///
/// Auto-detects the declared PDF/A level from XMP metadata.
/// Falls back to PDF/A-2B if no level is declared.
///
/// Parameters
/// ----------
/// path : str
///     Path to the PDF file to validate.
///
/// Returns
/// -------
/// ComplianceReport
///
/// Examples
/// --------
/// >>> report = validate_pdfa("document.pdf")
/// >>> if report.is_compliant:
/// ...     print("PDF/A compliant")
/// ... else:
/// ...     for issue in report.issues:
/// ...         print(f"[{issue.severity}] {issue.rule}: {issue.message}")
#[pyfunction]
fn validate_pdfa(path: &str) -> PyResult<PyComplianceReport> {
    let data = std::fs::read(path).map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
    let pdf = Pdf::new(Arc::new(data))
        .map_err(|e| PdfluentParseError::new_err(format!("invalid PDF: {e:?}")))?;
    let level = detect_pdfa_level(&pdf).unwrap_or(PdfALevel::A2b);
    let report = compliance_validate_pdfa(&pdf, level);
    Ok(PyComplianceReport(report))
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

/// High-performance PDF engine — rendering, text extraction, forms, signatures.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exception hierarchy
    m.add("PdfluentError", m.py().get_type::<PdfluentError>())?;
    m.add(
        "PdfluentParseError",
        m.py().get_type::<PdfluentParseError>(),
    )?;
    m.add(
        "PdfluentValidationError",
        m.py().get_type::<PdfluentValidationError>(),
    )?;
    m.add(
        "PdfluentRenderError",
        m.py().get_type::<PdfluentRenderError>(),
    )?;
    m.add(
        "PdfluentEncryptedError",
        m.py().get_type::<PdfluentEncryptedError>(),
    )?;
    m.add(
        "PdfluentPageRangeError",
        m.py().get_type::<PdfluentPageRangeError>(),
    )?;
    m.add("PdfluentIoError", m.py().get_type::<PdfluentIoError>())?;
    m.add(
        "PdfluentGeometryError",
        m.py().get_type::<PdfluentGeometryError>(),
    )?;
    m.add(
        "PdfluentLimitError",
        m.py().get_type::<PdfluentLimitError>(),
    )?;
    // Classes
    m.add_class::<PyDocument>()?;
    m.add_class::<PyPage>()?;
    m.add_class::<PyRenderedImage>()?;
    m.add_class::<PyBlock>()?;
    m.add_class::<PyWord>()?;
    m.add_class::<PyVisionAdvice>()?;
    m.add_class::<PyTextBlock>()?;
    m.add_class::<PyTextSpan>()?;
    m.add_class::<PyDocumentInfo>()?;
    m.add_class::<PyBookmark>()?;
    m.add_class::<PyPageGeometry>()?;
    m.add_class::<PyComplianceIssue>()?;
    m.add_class::<PyComplianceReport>()?;
    m.add_class::<PyFormField>()?;
    m.add_class::<PyAnnotation>()?;
    m.add_class::<PyRedactReport>()?;
    m.add_class::<PySignatureResult>()?;
    m.add_class::<text_edit::PyTextEditor>()?;
    // Functions
    m.add_function(wrap_pyfunction!(open_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(merge_pdfs, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pdfa, m)?)?;
    m.add_function(wrap_pyfunction!(decrypt_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(office::extract_office, m)?)?;
    // -- package registration slots (one line each; edit only your own line) --
    door_markdown::register(m)?;
    door_geometry::register(m)?;
    door_chunks::register(m)?;
    door_images::register(m)?;
    door_tables::register(m)?;
    door_inputs::register(m)?;
    slot_words::register(m)?;
    slot_compat::register(m)?;
    // -- end package registration slots --
    Ok(())
}

#[cfg(test)]
mod writeback_dispatch_tests {
    //! Pin the new writeback behavior of `set_form_field`'s dispatch helper:
    //! correct /V encoding (ASCII literal else UTF-16BE+BOM), /V-as-Name +
    //! /AS sync for buttons, and read-only rejection — replacing the old
    //! raw-bytes /V write + bogus "NeedsAppearances" key (extra `s`).

    use super::apply_string_value;
    use lopdf::{dictionary, Document, Object, Stream};
    use pdf_forms::WritebackError;

    /// Minimal indirect-AcroForm document: text field, read-only text
    /// field, and a checkbox whose on-state (`On1`) lives on a kid widget.
    fn form_doc() -> Document {
        let mut doc = Document::with_version("1.4");
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let pages_id = doc.new_object_id();

        let text_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("first_name"),
            "V" => Object::string_literal(""),
        });
        // /Ff bit 1 = ReadOnly.
        let readonly_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "Ff" => 1i64,
            "T" => Object::string_literal("locked"),
            "V" => Object::string_literal("frozen"),
        });
        let checkbox_kid = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![100.into(), 700.into(), 115.into(), 715.into()],
            "AP" => dictionary! {
                "N" => dictionary! {
                    "Off" => Object::Null,
                    "On1" => Object::Null,
                },
            },
        });
        let checkbox_field = doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("subscribe"),
            "V" => Object::Name(b"Off".to_vec()),
            "Kids" => vec![checkbox_kid.into()],
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
            "Annots" => vec![checkbox_kid.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![
                text_field.into(),
                readonly_field.into(),
                checkbox_field.into(),
            ],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn field_v(doc: &Document, name: &str) -> Object {
        doc.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| {
                d.get(b"T")
                    .ok()
                    .and_then(|t| lopdf::decode_text_string(t).ok())
                    .as_deref()
                    == Some(name)
            })
            .and_then(|d| d.get(b"V").ok())
            .cloned()
            .unwrap_or_else(|| panic!("field '{name}' has no /V"))
    }

    #[test]
    fn ascii_text_stays_literal() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Jane").expect("apply ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert_eq!(v, b"Jane"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_text_writes_utf16be_bom() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Café").expect("apply non-ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert!(
                v.starts_with(&[0xFE, 0xFF]),
                "non-ASCII /V must be UTF-16BE with BOM, got {v:02X?}"
            ),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn checkbox_dispatch_sets_name_value_and_widget_as() {
        let mut doc = form_doc();
        let outcome = apply_string_value(&mut doc, "subscribe", "true").expect("apply checkbox");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"On1".to_vec()));
        assert!(
            outcome.appearance_states_set >= 1,
            "kid widget /AS must be synced"
        );
        let widget_synced = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.has(b"AP") && !d.has(b"T"))
            .any(|d| matches!(d.get(b"AS"), Ok(Object::Name(n)) if n == b"On1"));
        assert!(widget_synced, "kid widget /AS must be the on-state On1");
    }

    #[test]
    fn checkbox_dispatch_bool_ish_off_strings() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "subscribe", "true").expect("check");
        apply_string_value(&mut doc, "subscribe", "false").expect("uncheck");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"Off".to_vec()));
    }

    #[test]
    fn readonly_field_is_rejected() {
        let mut doc = form_doc();
        let err = apply_string_value(&mut doc, "locked", "new value")
            .expect_err("read-only field must be rejected");
        assert!(
            matches!(err, WritebackError::ReadOnly(ref n) if n == "locked"),
            "expected ReadOnly error, got {err:?}"
        );
        match field_v(&doc, "locked") {
            Object::String(v, _) => assert_eq!(v, b"frozen", "value must be unchanged"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }
}
