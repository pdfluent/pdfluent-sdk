//! PdfDocument class exposed to Node.js.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::annotation::{self, AnnotationInfo};
use crate::error::to_napi_error;
use crate::form::{FormEngine, FormFieldInfo};
use crate::page::PdfPage;
use lopdf::Document as LopdfDocument;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdf_engine::{PdfDocument as RustDocument, RenderOptions, RenderedPage, ThumbnailOptions};
use pdf_forms::{apply_choice_multi, apply_field_value, WriteOutcome, WriteValue, WritebackError};
use std::sync::{Arc, Mutex};

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
) -> std::result::Result<WriteOutcome, WritebackError> {
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

/// A PDF document handle.
///
/// Open a document with `PdfDocument.open(buffer)` or
/// `PdfDocument.openAsync(buffer)`.
#[napi]
pub struct PdfDocument {
    inner: Arc<RustDocument>,
    form_engine: Option<Arc<FormEngine>>,
    /// Mutable lopdf document for write operations (annotations, redact, encrypt, save).
    /// `None` if the PDF could not be loaded by lopdf (rare edge case).
    doc: Option<Arc<Mutex<LopdfDocument>>>,
    /// Why `doc` is `None`, when it is. Kept so write operations can say what
    /// actually went wrong instead of only that the document is unwritable —
    /// a corrupted checkout used to surface here as a bare "not writable".
    doc_load_error: Option<String>,
}

impl PdfDocument {
    /// Apply a closure to the mutable lopdf document, returning an error if
    /// the document is not writable.
    fn with_doc_mut<T, F>(&self, f: F) -> napi::Result<T>
    where
        F: FnOnce(&mut LopdfDocument) -> napi::Result<T>,
    {
        let arc = self.doc.as_ref().ok_or_else(|| {
            napi::Error::from_reason(match &self.doc_load_error {
                Some(why) => format!("document is not writable: {why}"),
                None => "document is not writable".to_string(),
            })
        })?;
        let mut doc = arc.lock().unwrap();
        f(&mut doc)
    }

    /// Rebuild the rendering engine from the current lopdf state.
    ///
    /// Call this after any mutation so that `render_page` reflects the change.
    /// Outstanding `PdfPage` handles obtained before this call are stale.
    fn rebuild_inner(&mut self) -> napi::Result<()> {
        if let Some(arc) = &self.doc {
            let mut buf = Vec::with_capacity(64 * 1024);
            arc.lock()
                .unwrap()
                .clone()
                .save_to(&mut buf)
                .map_err(|e| napi::Error::from_reason(format!("engine refresh failed: {e}")))?;
            self.inner = Arc::new(RustDocument::open(buf).map_err(to_napi_error)?);
        }
        Ok(())
    }
}

/// Document metadata.
#[napi(object)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

/// A bookmark / outline item.
#[napi(object)]
pub struct BookmarkItem {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<BookmarkItem>,
}

/// Render options passed from JavaScript.
#[napi(object)]
#[derive(Default)]
pub struct RenderOpts {
    /// DPI (default: 72.0).
    pub dpi: Option<f64>,
    /// Background RGBA [r, g, b, a] each 0.0–1.0.
    pub background: Option<Vec<f64>>,
    /// Force output width in pixels.
    pub width: Option<u32>,
    /// Force output height in pixels.
    pub height: Option<u32>,
}

/// Rendered page result returned to JavaScript.
#[napi(object)]
pub struct RenderResult {
    /// RGBA pixel data.
    pub data: Buffer,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Page geometry information.
#[napi(object)]
pub struct PageGeometry {
    /// Width in PDF points.
    pub width: f64,
    /// Height in PDF points.
    pub height: f64,
    /// Rotation in degrees.
    pub rotation: u32,
}

/// A text span at a specific position.
///
/// Field *values* are derived from the canonical [`pdf_engine::TextSpanInfo`]
/// (the SDK single source of truth) via the `From` impl below, so the Node
/// binding cannot drift from the Tauri/serde wire form in its value logic.
/// (JS key casing follows napi's own camelCase convention.)
#[napi(object)]
pub struct TextSpanInfo {
    /// The extracted text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Span width in user space.
    pub width: f64,
    /// Span height in user space (font-size / ascent extent).
    pub height: f64,
    /// Font size in points.
    pub font_size: f64,
    /// PostScript font name with subset prefix stripped, if known.
    pub font_name: Option<String>,
    /// Inferred bold style.
    pub is_bold: bool,
    /// Inferred italic style.
    pub is_italic: bool,
    /// Fill colour as sRGB `[r, g, b]` in 0.0–1.0, if resolved.
    pub color: Option<Vec<f64>>,
    /// Width provenance: "Metric" or "Estimate".
    pub width_source: String,
    /// Per-glyph bounds `[x0, y0, x1, y1]` (y up), if available.
    pub char_bounds: Option<Vec<Vec<f64>>>,
    /// Full affine transform `[a, b, c, d, e, f]` of the first glyph, if any.
    pub transform: Option<Vec<f64>>,
    /// Numeric font weight (~100–900) from embedded font data, if available.
    pub font_weight: Option<u32>,
    /// Serif flag from embedded font data, if available.
    pub is_serif: Option<bool>,
    /// Monospace flag from embedded font data, if available.
    pub is_monospace: Option<bool>,
    /// Coarse PDF text render mode: 0 fill, 1 stroke, 3 invisible.
    pub render_mode: Option<u32>,
    /// Vertical font metrics (/1000 em) from the embedded font, if available.
    pub font_metrics: Option<FontMetricsInfo>,
}

/// Vertical font metrics returned to JavaScript (values in /1000 em).
#[napi(object)]
pub struct FontMetricsInfo {
    /// Ascent above the baseline.
    pub ascent: f64,
    /// Descent below the baseline (negative).
    pub descent: f64,
    /// Cap height, if present in the font.
    pub cap_height: Option<f64>,
    /// x-height, if present in the font.
    pub x_height: Option<f64>,
}

impl From<pdf_engine::TextSpan> for TextSpanInfo {
    /// Re-shape the canonical [`pdf_engine::TextSpanInfo`] into the napi object.
    /// All value logic lives in the canonical DTO; this only adapts container
    /// types to napi-friendly ones.
    fn from(span: pdf_engine::TextSpan) -> Self {
        let c = pdf_engine::TextSpanInfo::from(span);
        TextSpanInfo {
            text: c.text,
            x: c.x,
            y: c.y,
            width: c.width,
            height: c.height,
            font_size: c.font_size,
            font_name: c.font_name,
            is_bold: c.is_bold,
            is_italic: c.is_italic,
            color: c.color.map(|[r, g, b]| vec![r as f64, g as f64, b as f64]),
            width_source: c.width_source.as_str().to_string(),
            char_bounds: if c.char_bounds.is_empty() {
                None
            } else {
                Some(c.char_bounds.iter().map(|b| b.to_vec()).collect())
            },
            transform: c.transform.map(|t| t.to_vec()),
            font_weight: c.font_weight.map(|w| w as u32),
            is_serif: c.is_serif,
            is_monospace: c.is_monospace,
            render_mode: c.render_mode.map(|m| m as u32),
            font_metrics: c.font_metrics.map(|m| FontMetricsInfo {
                ascent: m.ascent,
                descent: m.descent,
                cap_height: m.cap_height,
                x_height: m.x_height,
            }),
        }
    }
}

#[cfg(test)]
mod text_span_info_tests {
    use super::TextSpanInfo;

    #[test]
    fn napi_span_derives_from_canonical_dto() {
        let span = pdf_engine::TextSpan {
            text: "Hi".to_string(),
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 9.0,
            font_size: 4.0,
            font_name: Some("Arial".to_string()),
            is_bold: true,
            is_italic: false,
            color: Some([255, 128, 0, 255]),
            width_source: pdf_engine::WidthSource::Metric,
            char_bounds: vec![[1.0, 2.0, 3.0, 6.0]],
            geometry_mode: pdf_engine::GeometryMode::Basic,
            bounds_source: pdf_engine::BoundsSource::default(),
            tight_char_bounds: vec![],
            glyph_advances: vec![],
            glyph_bounds_sources: vec![],
            transform: Some([0.5, 0.0, 0.0, 0.5, 1.0, 2.0]),
            font_weight: Some(400),
            is_serif: Some(false),
            is_monospace: Some(true),
            render_mode: Some(3),
            font_metrics: Some(pdf_engine::FontMetrics {
                ascent: 750.0,
                descent: -250.0,
                cap_height: Some(700.0),
                x_height: Some(500.0),
            }),
        };
        let canonical = pdf_engine::TextSpanInfo::from(span.clone());
        let napi = TextSpanInfo::from(span);

        assert_eq!(napi.text, canonical.text);
        assert_eq!(napi.x, canonical.x);
        assert_eq!(napi.y, canonical.y);
        assert_eq!(napi.width, canonical.width);
        // height mirrors font_size in the canonical contract.
        assert_eq!(napi.height, canonical.height);
        assert_eq!(napi.font_size, canonical.font_size);
        assert_eq!(napi.font_name, canonical.font_name);
        assert_eq!(napi.is_bold, canonical.is_bold);
        assert_eq!(napi.is_italic, canonical.is_italic);
        assert_eq!(napi.width_source, canonical.width_source.as_str());
        assert_eq!(
            napi.color,
            canonical
                .color
                .map(|[r, g, b]| vec![r as f64, g as f64, b as f64])
        );
        assert_eq!(
            napi.char_bounds,
            Some(
                canonical
                    .char_bounds
                    .iter()
                    .map(|b| b.to_vec())
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(napi.transform, canonical.transform.map(|t| t.to_vec()));
        assert_eq!(napi.font_weight, canonical.font_weight.map(|w| w as u32));
        assert_eq!(napi.is_serif, canonical.is_serif);
        assert_eq!(napi.is_monospace, canonical.is_monospace);
        assert_eq!(napi.render_mode, canonical.render_mode.map(|m| m as u32));
        assert_eq!(
            napi.font_metrics.is_some(),
            canonical.font_metrics.is_some()
        );
        if let (Some(n), Some(cm)) = (&napi.font_metrics, &canonical.font_metrics) {
            assert_eq!(n.ascent, cm.ascent);
            assert_eq!(n.descent, cm.descent);
            assert_eq!(n.cap_height, cm.cap_height);
            assert_eq!(n.x_height, cm.x_height);
        }
    }
}

/// A block of text (grouped by vertical proximity).
#[napi(object)]
pub struct TextBlockInfo {
    /// Concatenated text of the block.
    pub text: String,
    /// Individual spans within this block.
    pub spans: Vec<TextSpanInfo>,
}

/// Result of a text redaction operation.
#[napi(object)]
pub struct RedactionResult {
    /// Number of text matches found.
    pub matches_found: u32,
    /// Number of page areas redacted.
    pub areas_redacted: u32,
    /// Number of pages affected.
    pub pages_affected: u32,
}

/// Signature validation result.
#[napi(object)]
pub struct SignatureResult {
    /// Validation status: "valid", "invalid", or "unknown".
    pub status: String,
    /// Reason for invalid/unknown status.
    pub reason: Option<String>,
    /// Fully qualified field name.
    pub field_name: String,
    /// Signer common name, if available.
    pub signer: Option<String>,
    /// Signing timestamp, if available.
    pub timestamp: Option<String>,
}

/// PDF/A compliance issue.
#[napi(object)]
pub struct ComplianceIssueInfo {
    /// Rule identifier.
    pub rule: String,
    /// Severity: "error", "warning", or "info".
    pub severity: String,
    /// Human-readable description.
    pub message: String,
}

/// PDF/A compliance report.
#[napi(object)]
pub struct ComplianceReportInfo {
    /// Whether the document is compliant.
    pub compliant: bool,
    /// Number of errors.
    pub error_count: u32,
    /// Number of warnings.
    pub warning_count: u32,
    /// All issues found.
    pub issues: Vec<ComplianceIssueInfo>,
}

pub(crate) fn convert_render_opts_inner(opts: &Option<RenderOpts>) -> RenderOptions {
    convert_render_opts(opts)
}

fn convert_render_opts(opts: &Option<RenderOpts>) -> RenderOptions {
    let mut ro = RenderOptions::default();
    if let Some(o) = opts {
        if let Some(dpi) = o.dpi {
            ro.dpi = dpi;
        }
        if let Some(ref bg) = o.background {
            if bg.len() >= 4 {
                ro.background = [bg[0] as f32, bg[1] as f32, bg[2] as f32, bg[3] as f32];
            }
        }
        if let Some(w) = o.width {
            ro.width = Some(w as u16);
        }
        if let Some(h) = o.height {
            ro.height = Some(h as u16);
        }
    }
    ro
}

pub(crate) fn rendered_to_result_inner(rp: RenderedPage) -> RenderResult {
    rendered_to_result(rp)
}

fn rendered_to_result(rp: RenderedPage) -> RenderResult {
    RenderResult {
        data: Buffer::from(rp.pixels),
        width: rp.width,
        height: rp.height,
    }
}

fn convert_bookmarks(items: Vec<pdf_engine::BookmarkItem>) -> Vec<BookmarkItem> {
    items
        .into_iter()
        .map(|b| BookmarkItem {
            title: b.title,
            page: b.page.map(|p| p as u32),
            children: convert_bookmarks(b.children),
        })
        .collect()
}

#[napi]
impl PdfDocument {
    /// Open a PDF from a Buffer (synchronous).
    #[napi(factory)]
    pub fn open(data: Buffer) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = RustDocument::open(bytes.clone()).map_err(to_napi_error)?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match LopdfDocument::load_mem(&bytes) {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Open a PDF from a Buffer (async — runs on worker thread).
    #[napi(factory)]
    pub async fn open_async(data: Buffer) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = tokio::task::spawn_blocking(move || {
            let pdf = RustDocument::open(bytes.clone())?;
            let lopdf = LopdfDocument::load_mem(&bytes).map_err(|e| e.to_string());
            Ok::<_, pdf_engine::EngineError>((pdf, lopdf))
        })
        .await
        .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
        .map_err(to_napi_error)?;
        let (pdf_doc, lopdf_result) = doc;
        let form_engine = FormEngine::from_pdf(pdf_doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match lopdf_result {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e)),
        };
        Ok(PdfDocument {
            inner: Arc::new(pdf_doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Open a password-protected PDF.
    #[napi(factory)]
    pub fn open_with_password(data: Buffer, password: String) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc =
            RustDocument::open_with_password(bytes.clone(), &password).map_err(to_napi_error)?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        let (lopdf_doc, doc_load_error) = match LopdfDocument::load_mem_with_options(
            &bytes,
            lopdf::LoadOptions::with_password(&password),
        ) {
            Ok(d) => (Some(Arc::new(Mutex::new(d))), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
            doc: lopdf_doc,
            doc_load_error,
        })
    }

    /// Number of pages in the document.
    #[napi(getter)]
    pub fn page_count(&self) -> u32 {
        self.inner.page_count() as u32
    }

    /// Get document metadata.
    #[napi]
    pub fn info(&self) -> DocumentInfo {
        let i = self.inner.info();
        DocumentInfo {
            title: i.title,
            author: i.author,
            subject: i.subject,
            keywords: i.keywords,
            creator: i.creator,
            producer: i.producer,
        }
    }

    /// Get a page handle (0-based index).
    #[napi]
    pub fn page(&self, index: u32) -> Result<PdfPage> {
        let count = self.inner.page_count();
        if (index as usize) >= count {
            return Err(napi::Error::from_reason(format!(
                "page {index} out of range (document has {count} pages)"
            )));
        }
        Ok(PdfPage::new(self.inner.clone(), index))
    }

    /// Render a single page to RGBA pixels (synchronous).
    #[napi]
    pub fn render_page(&self, index: u32, options: Option<RenderOpts>) -> Result<RenderResult> {
        let ro = convert_render_opts(&options);
        let rp = self
            .inner
            .render_page(index as usize, &ro)
            .map_err(to_napi_error)?;
        Ok(rendered_to_result(rp))
    }

    /// Render a single page to RGBA pixels (async — worker thread).
    #[napi]
    pub async fn render_page_async(
        &self,
        index: u32,
        options: Option<RenderOpts>,
    ) -> Result<RenderResult> {
        let inner = self.inner.clone();
        let ro = convert_render_opts(&options);
        let rp = tokio::task::spawn_blocking(move || inner.render_page(index as usize, &ro))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)?;
        Ok(rendered_to_result(rp))
    }

    /// Generate a thumbnail for a page (async).
    #[napi]
    pub async fn thumbnail(&self, index: u32, max_dimension: Option<u32>) -> Result<RenderResult> {
        let inner = self.inner.clone();
        let opts = ThumbnailOptions {
            max_dimension: max_dimension.unwrap_or(256),
        };
        let rp = tokio::task::spawn_blocking(move || inner.thumbnail(index as usize, &opts))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)?;
        Ok(rendered_to_result(rp))
    }

    /// Extract text from a page (synchronous).
    #[napi]
    pub fn extract_text(&self, index: u32) -> Result<String> {
        self.inner
            .extract_text(index as usize)
            .map_err(to_napi_error)
    }

    /// Extract text from a page (async — worker thread).
    #[napi]
    pub async fn extract_text_async(&self, index: u32) -> Result<String> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.extract_text(index as usize))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)
    }

    /// Search for text across all pages. Returns 0-based page indices.
    #[napi]
    pub fn search_text(&self, query: String) -> Vec<u32> {
        self.inner
            .search_text(&query)
            .into_iter()
            .map(|i| i as u32)
            .collect()
    }

    /// Get document bookmarks / outline.
    #[napi]
    pub fn bookmarks(&self) -> Vec<BookmarkItem> {
        convert_bookmarks(self.inner.bookmarks())
    }

    /// Get page geometry (dimensions, rotation).
    #[napi]
    pub fn page_geometry(&self, index: u32) -> Result<PageGeometry> {
        let g = self
            .inner
            .page_geometry(index as usize)
            .map_err(to_napi_error)?;
        let (w, h) = g.effective_dimensions();
        Ok(PageGeometry {
            width: w,
            height: h,
            rotation: g.rotation.degrees(),
        })
    }

    /// Render all pages in parallel (async).
    #[napi]
    pub async fn render_all(&self, options: Option<RenderOpts>) -> Result<Vec<RenderResult>> {
        let inner = self.inner.clone();
        let ro = convert_render_opts(&options);
        let results = tokio::task::spawn_blocking(move || inner.render_all(&ro))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?;
        Ok(results.into_iter().map(rendered_to_result).collect())
    }

    /// Extract structured text blocks from a page.
    #[napi]
    pub fn extract_text_blocks(&self, index: u32) -> Result<Vec<TextBlockInfo>> {
        let blocks = self
            .inner
            .extract_text_blocks(index as usize)
            .map_err(to_napi_error)?;
        Ok(blocks
            .into_iter()
            .map(|b| TextBlockInfo {
                text: b.text(),
                spans: b.spans.into_iter().map(TextSpanInfo::from).collect(),
            })
            .collect())
    }

    /// Get all form fields in the document.
    #[napi]
    pub fn get_form_fields(&self) -> Vec<FormFieldInfo> {
        match &self.form_engine {
            Some(fe) => fe.fields(),
            None => Vec::new(),
        }
    }

    /// Get all form fields in the document.
    ///
    /// @deprecated Use {@link getFormFields} instead — the canonical
    /// cross-language name (snake_case `get_form_fields` in Rust/Python).
    /// Kept for backward compatibility; will be removed in 1.0.0.
    #[napi]
    pub fn form_fields(&self) -> Vec<FormFieldInfo> {
        self.get_form_fields()
    }

    /// Get the value of a form field by its fully qualified name.
    #[napi]
    pub fn get_field_value(&self, name: String) -> Option<String> {
        self.form_engine.as_ref()?.get_value(&name)
    }

    /// Set the value of a form field by its fully qualified name.
    ///
    /// Routes through [`pdf_forms::apply_field_value`] — the single SDK
    /// writeback chain — so `/V` encoding (ASCII literal else UTF-16BE+BOM),
    /// per-widget `/AS` sync, and `/AP` regeneration stay consistent, and
    /// read-only fields are rejected. The change is persisted to the
    /// document — a subsequent `save()` will write the updated value.
    #[napi]
    pub fn set_form_field(&mut self, name: String, value: String) -> Result<()> {
        self.with_doc_mut(|doc| {
            apply_string_value(doc, &name, &value)
                .map(|_| ())
                .map_err(|e| napi::Error::from_reason(format!("setFormField '{name}': {e}")))
        })?;
        // Keep the form engine's in-memory view in sync so getFieldValue()
        // reflects the write without reloading the document.
        if let Some(fe) = &self.form_engine {
            fe.set_value(&name, &value)?;
        }
        self.rebuild_inner()
    }

    /// Set the value of a form field by its fully qualified name.
    ///
    /// @deprecated Use {@link setFormField} instead — the canonical
    /// cross-language name (snake_case `set_form_field` in Rust/Python).
    /// Kept for backward compatibility; will be removed in 1.0.0.
    #[napi]
    pub fn set_field_value(&mut self, name: String, value: String) -> Result<()> {
        self.set_form_field(name, value)
    }

    /// Set multiple selected values on a multi-select list box.
    ///
    /// Routes through [`pdf_forms::apply_choice_multi`]: writes `/V` as an
    /// array of text strings and rebuilds `/I` (the sorted selected-index
    /// cache) to match what Adobe Acrobat produces. Pass an empty array to
    /// clear the selection. The field must be a multi-select list box
    /// (`/Ff` MultiSelect flag); for non-editable list boxes every value must
    /// be one of the field's `/Opt` options.
    #[napi]
    pub fn set_multi_select(&mut self, name: String, values: Vec<String>) -> Result<()> {
        self.with_doc_mut(|doc| {
            apply_choice_multi(doc, &name, &values)
                .map(|_| ())
                .map_err(|e| napi::Error::from_reason(format!("setMultiSelect '{name}': {e}")))
        })?;
        self.rebuild_inner()
    }

    /// Get annotations on a specific page (0-based index).
    #[napi]
    pub fn annotations(&self, page_index: u32) -> Result<Vec<AnnotationInfo>> {
        annotation::page_annotations(self.inner.pdf(), page_index as usize)
    }

    /// Validate all digital signatures in the document.
    #[napi]
    pub fn validate_signatures(&self) -> Vec<SignatureResult> {
        let results = pdf_sign::validate_signatures(self.inner.pdf());
        results
            .into_iter()
            .map(|r| {
                let (status, reason) = match r.status {
                    pdf_sign::ValidationStatus::Valid => ("valid".into(), None),
                    pdf_sign::ValidationStatus::Invalid(msg) => ("invalid".into(), Some(msg)),
                    pdf_sign::ValidationStatus::Unknown(msg) => ("unknown".into(), Some(msg)),
                };
                SignatureResult {
                    status,
                    reason,
                    field_name: r.field_name,
                    signer: r.signer,
                    timestamp: r.timestamp,
                }
            })
            .collect()
    }

    /// Save the document to a file path.
    ///
    /// Writes the current (possibly modified) document to disk. Any changes
    /// from `setFormField`, `addAnnotation`, or `redactText` are included.
    #[napi]
    pub fn save(&self, path: String) -> Result<()> {
        if let Some(arc) = &self.doc {
            let mut doc = arc.lock().unwrap();
            doc.save(&path)
                .map(|_| ())
                .map_err(|e| napi::Error::from_reason(format!("cannot save to '{path}': {e}")))
        } else {
            // Fallback: write original bytes unchanged.
            let bytes: &[u8] = self.inner.pdf().data().as_ref();
            std::fs::write(&path, bytes)
                .map_err(|e| napi::Error::from_reason(format!("cannot write '{path}': {e}")))
        }
    }

    /// Add an annotation to a page (0-based index).
    ///
    /// `annot_type` must be one of: `"highlight"`, `"freetext"`, `"note"`,
    /// `"underline"`, `"strikeout"`, `"squiggly"`.
    ///
    /// `rect` is `[x0, y0, x1, y1]` in PDF user-space coordinates.
    /// For `"freetext"`, `content` becomes the visible text.
    #[napi]
    pub fn add_annotation(
        &mut self,
        page: u32,
        annot_type: String,
        rect: Vec<f64>,
        content: Option<String>,
    ) -> Result<()> {
        if rect.len() < 4 {
            return Err(napi::Error::from_reason(
                "rect must have 4 elements [x0,y0,x1,y1]",
            ));
        }
        let ar = pdf_annot::builder::AnnotRect::new(rect[0], rect[1], rect[2], rect[3]);
        self.with_doc_mut(|doc| {
            let builder = match annot_type.to_lowercase().as_str() {
                "highlight" => pdf_annot::builder::AnnotationBuilder::highlight(ar),
                "underline" => pdf_annot::builder::AnnotationBuilder::underline(ar),
                "strikeout" => pdf_annot::builder::AnnotationBuilder::strikeout(ar),
                "squiggly" => pdf_annot::builder::AnnotationBuilder::squiggly(ar),
                "freetext" => {
                    let text = content.as_deref().unwrap_or("");
                    pdf_annot::builder::AnnotationBuilder::free_text(ar, text, 12.0)
                }
                "note" => {
                    pdf_annot::builder::AnnotationBuilder::sticky_note(
                        ar,
                        pdf_annot::builder::TextIcon::Note,
                    )
                }
                other => {
                    return Err(napi::Error::from_reason(format!(
                        "unknown annotation type '{other}'; expected: highlight, freetext, note, underline, strikeout, squiggly"
                    )))
                }
            };
            let builder = if let Some(c) = &content {
                builder.contents(c.clone())
            } else {
                builder
            };
            // build() adds the annotation object to the document
            let annot_id = builder
                .build(doc)
                .map_err(|e| napi::Error::from_reason(format!("build annotation: {e}")))?;
            // page is 0-based in our API; lopdf uses 1-based
            pdf_annot::builder::add_annotation_to_page(doc, page + 1, annot_id)
                .map_err(|e| napi::Error::from_reason(format!("add annotation to page: {e}")))
        })?;
        self.rebuild_inner()
    }

    /// Redact all occurrences of `search_term` on a page (0-based index).
    ///
    /// Pass `page = u32::MAX` (or omit via a wrapper) to redact across all pages.
    /// The document is modified in-place; call `save()` to persist.
    ///
    /// Returns a summary of what was redacted.
    #[napi]
    pub fn redact_text(
        &mut self,
        search_term: String,
        page: Option<u32>,
    ) -> Result<RedactionResult> {
        let result = self.with_doc_mut(|doc| {
            let mut opts = pdf_redact::RedactSearchOptions::exact(&search_term);
            if let Some(p) = page {
                opts = opts.pages(vec![p + 1]); // convert to 1-based
            }
            let report = pdf_redact::search_and_redact(doc, &search_term, &opts)
                .map_err(|e| napi::Error::from_reason(format!("redact failed: {e}")))?;
            Ok(RedactionResult {
                matches_found: report.matches_found as u32,
                areas_redacted: report.areas_redacted as u32,
                pages_affected: report.pages_affected as u32,
            })
        })?;
        self.rebuild_inner()?;
        Ok(result)
    }

    /// Encrypt the document and write it to `output_path`.
    ///
    /// Uses AES-256 with `password` as both the user and owner password.
    /// The in-memory document is not modified — encryption is applied to a
    /// temporary clone and only the written file is encrypted.
    #[napi]
    pub fn encrypt(&self, output_path: String, password: String) -> Result<()> {
        // Clone before encrypting so the shared document stays in plaintext state.
        let mut doc_clone = {
            let arc = self
                .doc
                .as_ref()
                .ok_or_else(|| napi::Error::from_reason("document is not writable"))?;
            arc.lock().unwrap().clone()
        };
        let config = pdf_manip::encrypt::EncryptConfig {
            user_password: password.as_bytes().to_vec(),
            owner_password: password.as_bytes().to_vec(),
            ..Default::default() // AES-256, all permissions
        };
        let file = std::fs::File::create(&output_path)
            .map_err(|e| napi::Error::from_reason(format!("cannot create '{output_path}': {e}")))?;
        let mut writer = std::io::BufWriter::new(file);
        pdf_manip::encrypt::encrypt_and_save(&mut doc_clone, &config, &mut writer)
            .map_err(|e| napi::Error::from_reason(format!("encrypt failed: {e}")))
    }

    /// Remove encryption and write the decrypted document to `output_path`.
    ///
    /// Only useful if the document was opened with `openWithPassword`.
    /// After this call the saved file has no password protection.
    #[napi]
    pub fn decrypt(&self, output_path: String) -> Result<()> {
        self.with_doc_mut(|doc| {
            pdf_manip::encrypt::remove_encryption(doc);
            doc.save(&output_path)
                .map(|_| ())
                .map_err(|e| napi::Error::from_reason(format!("cannot save '{output_path}': {e}")))
        })
    }

    /// Validate the document against a PDF/A conformance level.
    ///
    /// Level is specified as a string: "1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u".
    #[napi]
    pub fn validate_pdfa(&self, level: String) -> Result<ComplianceReportInfo> {
        let pdfa_level = parse_pdfa_level(&level)?;
        let report = pdf_compliance::validate_pdfa(self.inner.pdf(), pdfa_level);
        Ok(compliance_to_info(report))
    }

    /// Convert the document to a Word `.docx` package.
    ///
    /// Requires a Business licence or higher. Without one the call throws,
    /// and the message points at the free 30-day evaluation key rather than
    /// the price list.
    #[napi]
    pub fn to_docx(&self) -> Result<Buffer> {
        self.office_export(pdfluent::Capability::DocxExport, |b| {
            pdf_docx::convert_pdf_bytes_to_docx(b).map_err(|e| e.to_string())
        })
    }

    /// Convert the document to an Excel `.xlsx` workbook. See `toDocx`.
    #[napi]
    pub fn to_xlsx(&self) -> Result<Buffer> {
        self.office_export(pdfluent::Capability::XlsxExport, |b| {
            pdf_xlsx::convert_pdf_bytes_to_xlsx(b).map_err(|e| e.to_string())
        })
    }

    /// Convert the document to a PowerPoint `.pptx` deck, one slide per page.
    /// See `toDocx`.
    #[napi]
    pub fn to_pptx(&self) -> Result<Buffer> {
        self.office_export(pdfluent::Capability::PptxExport, |b| {
            pdf_pptx::convert_pdf_bytes_to_pptx(b).map_err(|e| e.to_string())
        })
    }
}

impl PdfDocument {
    /// Shared body for the three Office exports.
    ///
    /// The capability check comes from `pdfluent::require_capability` rather
    /// than a tier comparison here, so the rule lives in one place and the
    /// refusal carries the message the facade writes.
    fn office_export(
        &self,
        cap: pdfluent::Capability,
        convert: fn(&[u8]) -> std::result::Result<Vec<u8>, String>,
    ) -> Result<Buffer> {
        // The facade's message carries the tier, the free-key URL and the
        // error-code link. Re-wording it here would mean maintaining the same
        // sentence in five bindings.
        pdfluent::require_capability(cap).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let raw = self.inner.pdf().data().as_ref().to_vec();
        convert(&raw)
            .map(Buffer::from)
            .map_err(|e| napi::Error::from_reason(e))
    }
}

pub(crate) fn parse_pdfa_level(s: &str) -> Result<pdf_compliance::PdfALevel> {
    match s.to_lowercase().as_str() {
        "1a" => Ok(pdf_compliance::PdfALevel::A1a),
        "1b" => Ok(pdf_compliance::PdfALevel::A1b),
        "2a" => Ok(pdf_compliance::PdfALevel::A2a),
        "2b" => Ok(pdf_compliance::PdfALevel::A2b),
        "2u" => Ok(pdf_compliance::PdfALevel::A2u),
        "3a" => Ok(pdf_compliance::PdfALevel::A3a),
        "3b" => Ok(pdf_compliance::PdfALevel::A3b),
        "3u" => Ok(pdf_compliance::PdfALevel::A3u),
        _ => Err(napi::Error::from_reason(format!(
            "unknown PDF/A level '{s}'; expected one of: 1a, 1b, 2a, 2b, 2u, 3a, 3b, 3u"
        ))),
    }
}

pub(crate) fn compliance_to_info(report: pdf_compliance::ComplianceReport) -> ComplianceReportInfo {
    ComplianceReportInfo {
        compliant: report.is_compliant(),
        error_count: report.error_count() as u32,
        warning_count: report.warning_count() as u32,
        issues: report
            .issues
            .into_iter()
            .map(|i| ComplianceIssueInfo {
                rule: i.rule,
                severity: match i.severity {
                    pdf_compliance::Severity::Error => "error".into(),
                    pdf_compliance::Severity::Warning => "warning".into(),
                    pdf_compliance::Severity::Info => "info".into(),
                },
                message: i.message,
            })
            .collect(),
    }
}

#[cfg(test)]
mod writeback_dispatch_tests {
    //! Pin the new writeback behavior of `setFormField`'s dispatch helper:
    //! correct /V encoding (ASCII literal else UTF-16BE+BOM), /V-as-Name +
    //! /AS sync for buttons, and read-only rejection — replacing the old
    //! raw `string_literal(value.as_bytes())` write (mojibake, no /AS).

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
