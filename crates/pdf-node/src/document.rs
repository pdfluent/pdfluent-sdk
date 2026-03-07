//! PdfDocument class exposed to Node.js.

use crate::annotation::{self, AnnotationInfo};
use crate::error::to_napi_error;
use crate::form::{FormEngine, FormFieldInfo};
use crate::page::PdfPage;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdf_engine::{PdfDocument as RustDocument, RenderOptions, RenderedPage, ThumbnailOptions};
use std::sync::Arc;

/// A PDF document handle.
///
/// Open a document with `PdfDocument.open(buffer)` or
/// `PdfDocument.openAsync(buffer)`.
#[napi]
pub struct PdfDocument {
    inner: Arc<RustDocument>,
    form_engine: Option<Arc<FormEngine>>,
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
#[napi(object)]
pub struct TextSpanInfo {
    /// The extracted text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Approximate font size.
    pub font_size: f64,
}

/// A block of text (grouped by vertical proximity).
#[napi(object)]
pub struct TextBlockInfo {
    /// Concatenated text of the block.
    pub text: String,
    /// Individual spans within this block.
    pub spans: Vec<TextSpanInfo>,
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
        let doc = RustDocument::open(bytes).map_err(to_napi_error)?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
        })
    }

    /// Open a PDF from a Buffer (async — runs on worker thread).
    #[napi(factory)]
    pub async fn open_async(data: Buffer) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = tokio::task::spawn_blocking(move || RustDocument::open(bytes))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
        })
    }

    /// Open a password-protected PDF.
    #[napi(factory)]
    pub fn open_with_password(data: Buffer, password: String) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = RustDocument::open_with_password(bytes, &password).map_err(to_napi_error)?;
        let form_engine = FormEngine::from_pdf(doc.pdf()).map(Arc::new);
        Ok(PdfDocument {
            inner: Arc::new(doc),
            form_engine,
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
                spans: b
                    .spans
                    .into_iter()
                    .map(|s| TextSpanInfo {
                        text: s.text,
                        x: s.x,
                        y: s.y,
                        font_size: s.font_size,
                    })
                    .collect(),
            })
            .collect())
    }

    /// Get all form fields in the document.
    #[napi]
    pub fn form_fields(&self) -> Vec<FormFieldInfo> {
        match &self.form_engine {
            Some(fe) => fe.fields(),
            None => Vec::new(),
        }
    }

    /// Get the value of a form field by its fully qualified name.
    #[napi]
    pub fn get_field_value(&self, name: String) -> Option<String> {
        self.form_engine.as_ref()?.get_value(&name)
    }

    /// Set the value of a form field by its fully qualified name.
    #[napi]
    pub fn set_field_value(&self, name: String, value: String) -> Result<()> {
        let fe = self
            .form_engine
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("document has no form fields"))?;
        fe.set_value(&name, &value)
    }

    /// Get annotations on a specific page (0-based index).
    #[napi]
    pub fn annotations(&self, page_index: u32) -> Vec<AnnotationInfo> {
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

    /// Validate the document against a PDF/A conformance level.
    ///
    /// Level is specified as a string: "1a", "1b", "2a", "2b", "2u", "3a", "3b", "3u".
    #[napi]
    pub fn validate_pdfa(&self, level: String) -> Result<ComplianceReportInfo> {
        let pdfa_level = parse_pdfa_level(&level)?;
        let report = pdf_compliance::validate_pdfa(self.inner.pdf(), pdfa_level);
        Ok(compliance_to_info(report))
    }
}

fn parse_pdfa_level(s: &str) -> Result<pdf_compliance::PdfALevel> {
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

fn compliance_to_info(report: pdf_compliance::ComplianceReport) -> ComplianceReportInfo {
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
