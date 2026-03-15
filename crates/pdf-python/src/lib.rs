//! Python bindings for the PDF engine (via PyO3).
//!
//! Exposes `Document`, `Page`, `RenderedImage`, and supporting types
//! as a native Python module `pdfengine._native`.

use std::path::PathBuf;
use std::sync::Arc;

use pyo3::exceptions::{PyIOError, PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pdf_compliance::{
    detect_pdfa_level, validate_pdfa as compliance_validate_pdfa, PdfALevel,
};
use pdf_manip::pages;
use pdf_syntax::Pdf;

use pdf_engine::{
    BookmarkItem, DocumentInfo, EngineError, PageGeometry, PdfDocument, RenderOptions,
    RenderedPage, TextBlock, TextSpan, ThumbnailOptions,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn manip_err_to_py(e: pdf_manip::error::ManipError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

fn engine_err_to_py(e: EngineError) -> PyErr {
    match e {
        EngineError::InvalidPdf(msg) => PyValueError::new_err(format!("invalid PDF: {msg}")),
        EngineError::PageOutOfRange { index, count } => {
            PyIndexError::new_err(format!("page {index} out of range ({count} pages)"))
        }
        EngineError::RenderError(msg) => PyRuntimeError::new_err(format!("render error: {msg}")),
        EngineError::Io(e) => PyIOError::new_err(e.to_string()),
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
    #[new]
    #[pyo3(signature = (source, password=None))]
    fn new(source: &Bound<'_, PyAny>, password: Option<&str>) -> PyResult<Self> {
        let data: Vec<u8> = if let Ok(path_str) = source.extract::<String>() {
            let path = PathBuf::from(&path_str);
            std::fs::read(&path).map_err(|e| PyIOError::new_err(format!("{path_str}: {e}")))?
        } else if let Ok(bytes) = source.extract::<Vec<u8>>() {
            bytes
        } else {
            return Err(PyValueError::new_err(
                "source must be a file path (str) or bytes",
            ));
        };

        let raw_bytes = Arc::new(data);
        let doc = match password {
            Some(pw) => {
                PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw)
                    .map_err(engine_err_to_py)?
            }
            None => PdfDocument::open(Arc::clone(&raw_bytes)).map_err(engine_err_to_py)?,
        };

        Ok(Self {
            inner: Arc::new(doc),
            raw_bytes,
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
            return Err(PyIndexError::new_err(format!(
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
    /// Writes the original PDF bytes to the given path. For merged documents
    /// created with ``merge_pdfs()``, use that function's ``output_path``
    /// parameter directly.
    fn save(&self, path: &str) -> PyResult<()> {
        std::fs::write(path, self.raw_bytes.as_ref())
            .map_err(|e| PyIOError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("Document(pages={})", self.inner.page_count())
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
#[pyfunction]
#[pyo3(signature = (path, password=None))]
fn open_pdf(path: &str, password: Option<&str>) -> PyResult<PyDocument> {
    let data =
        std::fs::read(path).map_err(|e| PyIOError::new_err(format!("{path}: {e}")))?;
    let raw_bytes = Arc::new(data);
    let doc = match password {
        Some(pw) => {
            PdfDocument::open_with_password(Arc::clone(&raw_bytes), pw)
                .map_err(engine_err_to_py)?
        }
        None => PdfDocument::open(Arc::clone(&raw_bytes)).map_err(engine_err_to_py)?,
    };
    Ok(PyDocument {
        inner: Arc::new(doc),
        raw_bytes,
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
        return Err(PyValueError::new_err("input_paths must not be empty"));
    }
    let mut doc = pages::merge(&input_paths).map_err(manip_err_to_py)?;
    doc.save(output_path)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
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
    let data =
        std::fs::read(path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    let pdf = Pdf::new(Arc::new(data))
        .map_err(|e| PyValueError::new_err(format!("invalid PDF: {e:?}")))?;
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
    m.add_function(wrap_pyfunction!(open_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(merge_pdfs, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pdfa, m)?)?;
    Ok(())
}
