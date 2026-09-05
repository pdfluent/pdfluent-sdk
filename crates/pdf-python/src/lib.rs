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
use std::sync::{Arc, Mutex};

use lopdf::{Document as LopdfDocument, Permissions as LopdfPermissions};

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

mod text_edit;
use pyo3::types::PyBytes;

use pdf_compliance::{detect_pdfa_level, validate_pdfa as compliance_validate_pdfa, PdfALevel};
use pdf_manip::pages;
use pdf_syntax::Pdf;

use pdf_engine::{
    BookmarkItem, DocumentInfo, EngineError, PageGeometry, PdfDocument, RenderOptions,
    RenderedPage, TextBlock, TextSpan, ThumbnailOptions,
};

use pdfluent::{
    license_info as pdfl_license_info, set_license_key as pdfl_set_license_key,
    set_license_payload as pdfl_set_license_payload,
    set_license_public_key as pdfl_set_license_public_key, Tier,
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
    PdfluentLicenseError,
    PdfluentError,
    "Raised on license validation errors (invalid key, expired, quota exceeded)."
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

fn engine_err_to_py(e: EngineError) -> PyErr {
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

// ---------------------------------------------------------------------------
// License helpers
// ---------------------------------------------------------------------------

fn tier_to_str(tier: Tier) -> &'static str {
    match tier {
        Tier::Trial => "trial",
        Tier::Developer => "developer",
        Tier::Team => "team",
        Tier::Business => "business",
        Tier::Enterprise => "enterprise",
        _ => "unknown",
    }
}

/// Convert a `pdfluent::Error` into a typed Python exception and attach
/// canonical C8 metadata (`code`, `message`) to the resulting exception
/// instance.
///
/// Parity surface: Node, WASM, and .NET expose `.code` directly on
/// license errors. By setting attributes on the `PyErr` value here we
/// match that contract — users can branch on
/// `e.code == "E-LICENSE-INVALID"` without parsing the message string.
fn pdfluent_license_err_to_py(e: pdfluent::Error) -> PyErr {
    // Canonical C8 code from the Rust core (see crates/pdfluent/src/error.rs).
    let code = e.code();
    // The core's Display text, captured before the match moves `e`. Some arms
    // reuse it rather than paraphrase: see FeatureNotInTier below.
    let display = e.to_string();
    let (py_err, message) = match e {
        pdfluent::Error::InvalidLicense { reason } => {
            let msg = format!("invalid license: {reason}");
            (PdfluentLicenseError::new_err(msg.clone()), msg)
        }
        // Reuse the core's own text here instead of paraphrasing it. That text
        // carries the route to a key -- a free evaluation link on Trial, the
        // pricing page otherwise -- and the paraphrase dropped it, so a Python
        // caller was told what they could not do and nothing about how to fix
        // it. Every other binding forwards the Display text; this one did not.
        pdfluent::Error::FeatureNotInTier { .. } => (
            PdfluentLicenseError::new_err(display.clone()),
            display.clone(),
        ),
        pdfluent::Error::CapabilityNotCompiled {
            capability,
            feature_flag,
        } => {
            let msg = format!(
                "capability {capability:?} is not compiled into this build \
                 (enable the {feature_flag:?} cargo feature)"
            );
            (PdfluentLicenseError::new_err(msg.clone()), msg)
        }
        pdfluent::Error::LicenseExpired { expires_at } => {
            let msg = format!("license expired at unix timestamp {expires_at}");
            (PdfluentLicenseError::new_err(msg.clone()), msg)
        }
        pdfluent::Error::LicenseInvalidSignature => {
            let msg =
                "license signature does not verify against the configured public key".to_string();
            (PdfluentLicenseError::new_err(msg.clone()), msg)
        }
        pdfluent::Error::LicenseRateLimited {
            resource,
            used,
            limit,
        } => {
            let msg = format!("rate limit exceeded: {used}/{limit} {resource}");
            (PdfluentLicenseError::new_err(msg.clone()), msg)
        }
        other => {
            let msg = other.to_string();
            // Non-license errors keep the canonical code on the base class so
            // every typed PdfluentError carries a `code` if the Rust side has
            // one — but only license errors are exposed via this helper.
            (PdfluentError::new_err(msg.clone()), msg)
        }
    };
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

/// Canonical license state snapshot from the Rust core.
///
/// Returned by :func:`native_license_info`. Consumers should access the
/// higher-level :class:`pdfluent.LicenseInfo` returned by
/// :func:`pdfluent.activate_license` instead.
#[pyclass(name = "_NativeLicenseInfo")]
struct PyNativeLicenseInfo {
    #[pyo3(get)]
    tier: String,
    #[pyo3(get)]
    expires_at: Option<String>,
    #[pyo3(get)]
    output_is_marked: bool,
}

#[pymethods]
impl PyNativeLicenseInfo {
    fn __repr__(&self) -> String {
        format!(
            "_NativeLicenseInfo(tier={:?}, output_is_marked={})",
            self.tier, self.output_is_marked
        )
    }
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
struct PyDocument {
    inner: Arc<PdfDocument>,
    raw_bytes: Arc<Vec<u8>>,
    /// Lazily-initialised mutable document for write operations.
    /// None until the first mutation (form fill, annotation, redact, …).
    lopdf: Mutex<Option<LopdfDocument>>,
}

/// Plumbing for the Office exporters. Kept out of `#[pymethods]` so PyO3 does
/// not expose it to Python.
impl PyDocument {
    /// The bytes as they stand now, mutations included.
    ///
    /// Same rule as `save()`: once anything has written through the lopdf
    /// handle, the original bytes no longer describe the document, and an
    /// export that ignored that would silently convert the pre-edit version.
    fn current_bytes(&self) -> PyResult<Vec<u8>> {
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
        let doc = pdfluent::PdfDocument::from_bytes(&raw).map_err(pdfluent_license_err_to_py)?;
        let out = convert(&doc).map_err(pdfluent_license_err_to_py)?;
        Ok(PyBytes::new(py, &out))
    }
}

#[pymethods]
impl PyDocument {
    /// Open a PDF from a file path or raw bytes.
    ///
    /// Parameters
    /// ----------
    /// source : str or bytes
    ///     File path or raw PDF bytes.
    /// password : str, optional
    ///     Password for encrypted PDFs.
    ///
    /// Raises
    /// ------
    /// PdfluentParseError
    ///     If the bytes are not a valid PDF.
    /// PdfluentEncryptedError
    ///     If the PDF is encrypted and no password is provided.
    /// PdfluentIoError
    ///     If the file cannot be read.
    #[new]
    #[pyo3(signature = (source, password=None))]
    fn new(source: &Bound<'_, PyAny>, password: Option<&str>) -> PyResult<Self> {
        let data: Vec<u8> = if let Ok(path_str) = source.extract::<String>() {
            let path = PathBuf::from(&path_str);
            std::fs::read(&path)
                .map_err(|e| PdfluentIoError::new_err(format!("{path_str}: {e}")))?
        } else if let Ok(bytes) = source.extract::<Vec<u8>>() {
            bytes
        } else {
            return Err(PdfluentParseError::new_err(
                "source must be a file path (str) or bytes",
            ));
        };

        let raw_bytes = Arc::new(data);
        let doc = match password {
            Some(pw) => PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw)
                .map_err(engine_err_to_py)?,
            None => PdfDocument::open(Arc::clone(&raw_bytes)).map_err(engine_err_to_py)?,
        };

        Ok(Self {
            inner: Arc::new(doc),
            raw_bytes,
            lopdf: Mutex::new(None),
        })
    }

    /// Number of pages.
    #[getter]
    fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    /// Document metadata.
    #[getter]
    fn metadata(&self) -> PyDocumentInfo {
        PyDocumentInfo(self.inner.info())
    }

    /// Document outline / bookmarks.
    #[getter]
    fn bookmarks(&self) -> Vec<PyBookmark> {
        self.inner.bookmarks().into_iter().map(PyBookmark).collect()
    }

    /// Get a page by index (supports negative indexing).
    fn __getitem__(&self, index: isize) -> PyResult<PyPage> {
        let count = self.inner.page_count() as isize;
        let idx = if index < 0 { count + index } else { index };
        if idx < 0 || idx >= count {
            return Err(PdfluentPageRangeError::new_err(format!(
                "page index {index} out of range ({count} pages)"
            )));
        }
        Ok(PyPage {
            doc: self.inner.clone(),
            index: idx as usize,
        })
    }

    /// Number of pages (for ``len()``).
    fn __len__(&self) -> usize {
        self.inner.page_count()
    }

    /// Iterate over pages.
    fn __iter__(slf: PyRef<'_, Self>) -> PyPageIterator {
        PyPageIterator {
            doc: slf.inner.clone(),
            index: 0,
            count: slf.inner.page_count(),
        }
    }

    /// Context manager entry.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Context manager exit.
    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> bool {
        false
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
    fn render_all(&self, dpi: f64) -> Vec<PyRenderedImage> {
        let opts = RenderOptions {
            dpi,
            ..Default::default()
        };
        self.inner
            .render_all(&opts)
            .into_iter()
            .map(PyRenderedImage)
            .collect()
    }

    /// Search for text across all pages.
    ///
    /// Returns a list of 0-based page indices containing the query.
    fn search(&self, query: &str) -> Vec<usize> {
        self.inner.search_text(query)
    }

    /// Extract all text from a specific page (0-based index).
    fn extract_text(&self, page_num: usize) -> PyResult<String> {
        self.inner.extract_text(page_num).map_err(engine_err_to_py)
    }

    /// Save the PDF to a file path.
    ///
    /// If the document has been mutated (form fill, annotations, redactions,
    /// …) the mutated state is written. Otherwise the original bytes are
    /// copied verbatim.
    fn save(&self, path: &str) -> PyResult<()> {
        let mut guard = self.lopdf.lock().unwrap();
        if let Some(ref mut doc) = *guard {
            let mut buf = Vec::new();
            doc.save_to(&mut buf)
                .map_err(|e| PdfluentIoError::new_err(format!("save failed: {e}")))?;
            std::fs::write(path, &buf).map_err(|e| PdfluentIoError::new_err(e.to_string()))
        } else {
            std::fs::write(path, self.raw_bytes.as_ref())
                .map_err(|e| PdfluentIoError::new_err(e.to_string()))
        }
    }

    // ------------------------------------------------------------------
    // Office export
    // ------------------------------------------------------------------

    /// Convert to a Word document (``.docx``) and return the bytes.
    ///
    /// Requires a Business licence or higher. Without one this raises
    /// ``PdfluentLicenseError`` carrying the link to a free evaluation key.
    fn to_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.office_export(py, |d| d.to_docx_bytes())
    }

    /// Convert to an Excel workbook (``.xlsx``) and return the bytes.
    ///
    /// Requires a Business licence or higher.
    fn to_xlsx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.office_export(py, |d| d.to_xlsx_bytes())
    }

    /// Convert to a PowerPoint deck (``.pptx``) and return the bytes.
    ///
    /// Requires a Business licence or higher.
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
    fn validate_signatures(&self) -> Vec<PySignatureResult> {
        validate_signatures(self.inner.pdf())
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
            .collect()
    }

    /// Alias for :meth:`validate_signatures`.
    ///
    /// Provided for parity with the Rust core
    /// [`PdfDocument::verify_signatures`](pdfluent::PdfDocument) method name.
    /// Returns the same :class:`SignatureResult` list with full cryptographic
    /// validation.
    fn verify_signatures(&self) -> Vec<PySignatureResult> {
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
    fn signatures(&self) -> Vec<PySignatureResult> {
        signature_fields(self.inner.pdf())
            .into_iter()
            .map(|f| PySignatureResult {
                status: "unknown".to_string(),
                reason: None,
                field_name: f.field_name,
                signer: f.sig.signer_name(),
                timestamp: f.sig.signing_time(),
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Form fields
    // ------------------------------------------------------------------

    /// Return all interactive form fields in the document.
    ///
    /// Returns
    /// -------
    /// list[FormField]
    fn get_form_fields(&self) -> Vec<PyFormField> {
        let Some(tree) = parse_acroform(self.inner.pdf()) else {
            return vec![];
        };
        tree.terminal_fields()
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
            .collect()
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
        if parse_acroform(self.inner.pdf()).is_none() {
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
        if parse_acroform(self.inner.pdf()).is_none() {
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
        let pages = self.inner.pdf().pages();
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
        let mut doc = LopdfDocument::load_mem_with_options(self.raw_bytes.as_ref(), lopdf::LoadOptions::with_password(password))
            .map_err(|e| {
                PdfluentEncryptedError::new_err(format!("failed to open with password: {e}"))
            })?;
        remove_encryption(&mut doc);
        doc.save(output_path)
            .map_err(|e| PdfluentIoError::new_err(e.to_string()))?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("Document(pages={})", self.inner.page_count())
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
                    )))
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
}

#[pymethods]
impl PyPageIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<PyPage> {
        if self.index >= self.count {
            return None;
        }
        let page = PyPage {
            doc: self.doc.clone(),
            index: self.index,
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
struct PyPage {
    doc: Arc<PdfDocument>,
    index: usize,
}

#[pymethods]
impl PyPage {
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

/// A single text span at a specific position.
///
/// G1 font-metadata fields (``font_name``, ``is_bold``, ``is_italic``, ``color``)
/// are ``None`` until the text-extraction pipeline is upgraded to emit font
/// attributes (G1 milestone). Check for ``None`` before using.
#[pyclass(name = "TextSpan")]
#[derive(Clone)]
struct PyTextSpan(TextSpan);

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

    /// Font name (e.g. ``"Helvetica"``), or ``None`` if not yet available (G1).
    #[getter]
    fn font_name(&self) -> Option<String> {
        None
    }

    /// ``True`` if the span is bold, ``None`` if not yet available (G1).
    #[getter]
    fn is_bold(&self) -> Option<bool> {
        None
    }

    /// ``True`` if the span is italic, ``None`` if not yet available (G1).
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

/// Document metadata (title, author, subject, etc.).
#[pyclass(name = "DocumentInfo")]
struct PyDocumentInfo(DocumentInfo);

#[pymethods]
impl PyDocumentInfo {
    #[getter]
    fn title(&self) -> Option<&str> {
        self.0.title.as_deref()
    }

    #[getter]
    fn author(&self) -> Option<&str> {
        self.0.author.as_deref()
    }

    #[getter]
    fn subject(&self) -> Option<&str> {
        self.0.subject.as_deref()
    }

    #[getter]
    fn keywords(&self) -> Option<&str> {
        self.0.keywords.as_deref()
    }

    #[getter]
    fn creator(&self) -> Option<&str> {
        self.0.creator.as_deref()
    }

    #[getter]
    fn producer(&self) -> Option<&str> {
        self.0.producer.as_deref()
    }

    fn __repr__(&self) -> String {
        let title = self.0.title.as_deref().unwrap_or("<untitled>");
        format!("DocumentInfo(title={title:?})")
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
        inner: Arc::new(doc),
        raw_bytes,
        lopdf: Mutex::new(None),
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
    let mut doc = LopdfDocument::load_mem_with_options(&data, lopdf::LoadOptions::with_password(password)).map_err(|e| {
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
// License functions
// ---------------------------------------------------------------------------

/// Activate the process-global license key in the Rust core.
///
/// Accepts the simple 1.0 evaluation format: ``"tier:<name>"`` where
/// ``<name>`` is one of ``trial``, ``developer``, ``team``, ``business``,
/// or ``enterprise``.
///
/// The first call locks the resolved tier for the process lifetime. Subsequent
/// calls with the **same** tier are idempotent no-ops. Calls with a
/// **different** tier raise :exc:`PdfluentLicenseError`.
///
/// Raises
/// ------
/// PdfluentLicenseError
///     If the key format is invalid or a conflicting tier is already set.
#[pyfunction]
fn set_license_key(key: &str) -> PyResult<()> {
    pdfl_set_license_key(key).map_err(pdfluent_license_err_to_py)
}

/// Configure the Ed25519 public key used to verify signed JSON license
/// payloads (SDK 1.1+).
///
/// Must be called **once**, before :func:`set_license_payload` or before
/// passing a JSON payload to :func:`set_license_key`. The key must be
/// exactly 32 raw bytes (not base64, not PEM, not PKCS#8).
///
/// Subsequent calls with the **same** key are idempotent no-ops. Calls with a
/// **different** key raise :exc:`PdfluentLicenseError`.
///
/// Parameters
/// ----------
/// key : bytes
///     32-byte raw Ed25519 public key.
///
/// Raises
/// ------
/// PdfluentLicenseError
///     If the key is not 32 bytes, or a different key was already configured.
#[pyfunction]
fn set_license_public_key(key: &[u8]) -> PyResult<()> {
    pdfl_set_license_public_key(key).map_err(pdfluent_license_err_to_py)
}

/// Activate a cryptographically-signed JSON license payload (SDK 1.1+).
///
/// The public key must be configured first via :func:`set_license_public_key`.
/// This is the explicit 1.1 entry point; :func:`set_license_key` also accepts
/// signed JSON payloads automatically when the string starts with ``{``.
///
/// Parameters
/// ----------
/// payload_json : str
///     JSON string produced by the PDFluent licence-generator tool.
///
/// Raises
/// ------
/// PdfluentLicenseError
///     If the JSON is malformed, no public key is configured, or a conflicting
///     tier is already set (``E-LICENSE-INVALID``); if the signature does not
///     verify (``E-LICENSE-INVALID-SIGNATURE``); or if the payload is past its
///     expiry (``E-LICENSE-EXPIRED``).
#[pyfunction]
fn set_license_payload(payload_json: &str) -> PyResult<()> {
    pdfl_set_license_payload(payload_json).map_err(pdfluent_license_err_to_py)
}

/// Return the current canonical license state from the Rust core.
///
/// Returns
/// -------
/// _NativeLicenseInfo
///     Snapshot of the active tier, expiry, and output-marking flag.
#[pyfunction]
fn native_license_info() -> PyNativeLicenseInfo {
    let info = pdfl_license_info();
    PyNativeLicenseInfo {
        tier: tier_to_str(info.tier).to_owned(),
        expires_at: info.expires_at,
        output_is_marked: info.output_is_marked,
    }
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
        "PdfluentLicenseError",
        m.py().get_type::<PdfluentLicenseError>(),
    )?;
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
    m.add_class::<PyNativeLicenseInfo>()?;
    m.add_class::<text_edit::PyTextEditor>()?;
    // Functions
    m.add_function(wrap_pyfunction!(open_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(merge_pdfs, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pdfa, m)?)?;
    m.add_function(wrap_pyfunction!(decrypt_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(set_license_key, m)?)?;
    m.add_function(wrap_pyfunction!(set_license_public_key, m)?)?;
    m.add_function(wrap_pyfunction!(set_license_payload, m)?)?;
    m.add_function(wrap_pyfunction!(native_license_info, m)?)?;
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
