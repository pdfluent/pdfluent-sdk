//! Layout-aware text replacement for the Python binding.
//!
//! `TextEditor` owns a [`pdfluent::PdfDocument`] rather than a raw lopdf
//! document, so licensing and the Trial-tier notice apply the same way they do
//! in the Rust SDK and the WASM binding. The rest of this crate predates that
//! rule and talks to the engine crates directly; new surfaces go through the
//! facade.
//!
//! Structured values cross into Python as dicts/lists parsed from the report
//! JSON, and match ids are opaque strings that survive being written to a
//! queue or a file between the find and the apply (the asynchronous
//! translate-then-replace workflow).

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use pdfluent::unicode_font::UnicodeFont;
use pdfluent::text_edit::FitPolicy;
use pdfluent::text_edit::{
    CommitPolicy, FontFallback, MatchId, RegionRelation, ReplaceOptions, SignaturePolicy, TextQuery,
};

fn runtime_err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

/// Convert a serde_json value into the equivalent Python object.
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any().unbind(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any().unbind()
            } else {
                n.as_f64()
                    .unwrap_or(f64::NAN)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind()
            }
        }
        Value::String(s) => s.into_pyobject(py)?.into_any().unbind(),
        Value::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            list.into_any().unbind()
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            dict.into_any().unbind()
        }
    })
}

fn to_py<T: serde::Serialize>(py: Python<'_>, value: &T) -> PyResult<PyObject> {
    let json = serde_json::to_value(value).map_err(runtime_err)?;
    json_to_py(py, &json)
}

/// Build a query from the keyword arguments.
#[allow(clippy::too_many_arguments)]
fn build_query(
    text: &str,
    case_insensitive: bool,
    pages: Option<(u32, u32)>,
    region: Option<(u32, [f64; 4])>,
    region_relation: Option<&str>,
    limit: Option<usize>,
    regex: bool,
) -> PyResult<TextQuery> {
    // A regex that does not compile, or one that matches the empty string, is
    // rejected here rather than surfacing later from inside the search. The
    // engine has no backtracking, so a pattern from an end user cannot hang
    // the call.
    let mut query = if regex {
        TextQuery::regex(text)
            .map_err(|e| PyValueError::new_err(format!("regex=True: {e}")))?
            .case_insensitive(case_insensitive)
    } else {
        TextQuery::exact(text).case_insensitive(case_insensitive)
    };
    if let Some((lo, hi)) = pages {
        query = query.pages(lo..=hi);
    }
    if let Some((page, rect)) = region {
        let relation = match region_relation {
            None | Some("intersects") => RegionRelation::Intersects,
            Some("contained") => RegionRelation::Contained,
            Some(other) => {
                return Err(PyValueError::new_err(format!(
                    "region_relation must be 'intersects' or 'contained', got {other:?}"
                )))
            }
        };
        query = query.region_with(page, rect, relation);
    }
    if let Some(n) = limit {
        query = query.limit(n);
    }
    Ok(query)
}

fn build_options(
    font_fallback: Option<&str>,
    fallback_font_name: Option<&str>,
    unicode_font: Option<&[u8]>,
    fit: Option<&str>,
    signature_policy: Option<&str>,
    commit_policy: Option<&str>,
) -> PyResult<ReplaceOptions> {
    let mut options = ReplaceOptions::default();
    match font_fallback {
        None | Some("deny") => {}
        Some("inject_standard") => options.font_fallback = FontFallback::InjectStandard,
        Some("explicit") => {
            let name = fallback_font_name.ok_or_else(|| {
                PyValueError::new_err(
                    "font_fallback='explicit' requires fallback_font_name=<resource name>",
                )
            })?;
            options.font_fallback = FontFallback::Explicit(name.to_string());
        }
        Some("embed_unicode") => {
            // The only route that can write scripts the document never had.
            // The caller supplies the font because embedding redistributes it,
            // and the licence to do so is theirs to hold.
            let data = unicode_font.ok_or_else(|| {
                PyValueError::new_err(
                    "font_fallback='embed_unicode' requires unicode_font=<TrueType font bytes>",
                )
            })?;
            let font = UnicodeFont::from_bytes(data.to_vec()).map_err(|e| {
                PyValueError::new_err(format!("unicode_font is not usable: {e}"))
            })?;
            options.font_fallback = FontFallback::EmbedUnicode(font);
        }
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "font_fallback must be 'deny', 'inject_standard', 'explicit' or \
                 'embed_unicode', got {other:?}"
            )))
        }
    }
    match fit {
        None | Some("exact") => {}
        // Measures the replacement against the space the original occupied
        // and scales the font down, to a 50% floor. Reports what it did in
        // the per-edit diagnostics.
        Some("shrink_to_fit") => options.fit = FitPolicy::ShrinkToFit,
        // Acrobat's behaviour: rewrap onto more lines at the original size.
        // The natural choice for translations; shrink_to_fit is for headings
        // and table cells where an extra line is not an option.
        Some("reflow") => options.fit = FitPolicy::ReflowInBounds,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "fit must be 'exact', 'shrink_to_fit' or 'reflow', got {other:?}"
            )))
        }
    }
    match signature_policy {
        None | Some("reject") => {}
        Some("allow_post_signature_change") => {
            options.signature_policy = SignaturePolicy::AllowPostSignatureChange;
        }
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "signature_policy must be 'reject' or 'allow_post_signature_change', got {other:?}"
            )))
        }
    }
    match commit_policy {
        None | Some("all_or_nothing") => {}
        Some("best_effort") => options.commit_policy = CommitPolicy::BestEffort,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "commit_policy must be 'all_or_nothing' or 'best_effort', got {other:?}"
            )))
        }
    }
    Ok(options)
}

/// Find and replace text in a PDF while preserving fonts, positioning and the
/// surrounding page content.
///
/// Trial-tier edits stamp a small "PDFluent trial" notice on each modified
/// page; licensed tiers edit without it. Searching never modifies anything.
#[pyclass(name = "TextEditor", module = "pdfluent")]
pub struct PyTextEditor {
    doc: pdfluent::PdfDocument,
}

#[pymethods]
impl PyTextEditor {
    /// Open a PDF from a file path.
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let doc = pdfluent::PdfDocument::open(path).map_err(runtime_err)?;
        Ok(Self { doc })
    }

    /// Open a PDF from raw bytes.
    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let doc = pdfluent::PdfDocument::from_bytes(data).map_err(runtime_err)?;
        Ok(Self { doc })
    }

    /// Number of pages.
    fn page_count(&self) -> usize {
        self.doc.page_count()
    }

    /// Find occurrences of `text`.
    ///
    /// Returns a list of dicts. Each carries an opaque `id` that stays valid
    /// until the next applied edit on this document, and an `editable` flag —
    /// matches inside Form XObjects, style-mixed spans or `/ActualText`
    /// regions are reported rather than silently omitted.
    #[pyo3(signature = (
        text,
        *,
        case_insensitive = false,
        pages = None,
        region = None,
        region_relation = None,
        limit = None,
        regex = false,
    ))]
    fn find_text(
        &mut self,
        py: Python<'_>,
        text: &str,
        case_insensitive: bool,
        pages: Option<(u32, u32)>,
        region: Option<(u32, [f64; 4])>,
        region_relation: Option<&str>,
        limit: Option<usize>,
        regex: bool,
    ) -> PyResult<PyObject> {
        let query = build_query(
            text,
            case_insensitive,
            pages,
            region,
            region_relation,
            limit,
            regex,
        )?;
        let matches = self.doc.find_text(query).map_err(runtime_err)?;
        to_py(py, &matches)
    }

    /// Find and replace in one call; returns the replacement report as a dict.
    ///
    /// Every occurrence found is accounted for in the report — applied, or
    /// failed with a reason. Nothing is skipped silently.
    #[pyo3(signature = (
        text,
        replacement,
        *,
        case_insensitive = false,
        pages = None,
        region = None,
        region_relation = None,
        limit = None,
        regex = false,
        font_fallback = None,
        fallback_font_name = None,
        unicode_font = None,
        fit = None,
        signature_policy = None,
        commit_policy = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn replace_text(
        &mut self,
        py: Python<'_>,
        text: &str,
        replacement: &str,
        case_insensitive: bool,
        pages: Option<(u32, u32)>,
        region: Option<(u32, [f64; 4])>,
        region_relation: Option<&str>,
        limit: Option<usize>,
        regex: bool,
        font_fallback: Option<&str>,
        fallback_font_name: Option<&str>,
        unicode_font: Option<&[u8]>,
        fit: Option<&str>,
        signature_policy: Option<&str>,
        commit_policy: Option<&str>,
    ) -> PyResult<PyObject> {
        let query = build_query(
            text,
            case_insensitive,
            pages,
            region,
            region_relation,
            limit,
            regex,
        )?;
        let options = build_options(
            font_fallback,
            fallback_font_name,
            unicode_font,
            fit,
            signature_policy,
            commit_policy,
        )?;
        let report = self
            .doc
            .replace_text(query, replacement, options)
            .map_err(runtime_err)?;
        to_py(py, &report)
    }

    /// Apply replacements to matches located earlier with `find_text`.
    ///
    /// `edits` is a sequence of `(match_id, replacement_text)` pairs. They
    /// commit as one transaction; by default any invalid edit aborts the whole
    /// batch, leaving the document untouched.
    #[pyo3(signature = (
        edits,
        *,
        font_fallback = None,
        fallback_font_name = None,
        unicode_font = None,
        fit = None,
        signature_policy = None,
        commit_policy = None,
    ))]
    fn replace_matches(
        &mut self,
        py: Python<'_>,
        edits: Vec<(String, String)>,
        font_fallback: Option<&str>,
        fallback_font_name: Option<&str>,
        unicode_font: Option<&[u8]>,
        fit: Option<&str>,
        signature_policy: Option<&str>,
        commit_policy: Option<&str>,
    ) -> PyResult<PyObject> {
        let options = build_options(
            font_fallback,
            fallback_font_name,
            unicode_font,
            fit,
            signature_policy,
            commit_policy,
        )?;
        let edits: Vec<(MatchId, String)> = edits
            .into_iter()
            .map(|(id, text)| (MatchId::from_token(id), text))
            .collect();
        let report = self
            .doc
            .replace_text_matches(&edits, options)
            .map_err(runtime_err)?;
        to_py(py, &report)
    }

    /// Serialize the (possibly edited) document to bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self.doc.to_bytes().map_err(runtime_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Write the (possibly edited) document to `path`.
    fn save(&self, path: &str) -> PyResult<()> {
        self.doc.save(path).map_err(runtime_err)
    }

    fn __repr__(&self) -> String {
        format!("TextEditor(pages={})", self.doc.page_count())
    }
}
