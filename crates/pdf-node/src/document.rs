//! PdfDocument class exposed to Node.js.

use crate::error::to_napi_error;
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
        Ok(PdfDocument {
            inner: Arc::new(doc),
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
        Ok(PdfDocument {
            inner: Arc::new(doc),
        })
    }

    /// Open a password-protected PDF.
    #[napi(factory)]
    pub fn open_with_password(data: Buffer, password: String) -> Result<PdfDocument> {
        let bytes: Vec<u8> = data.to_vec();
        let doc = RustDocument::open_with_password(bytes, &password).map_err(to_napi_error)?;
        Ok(PdfDocument {
            inner: Arc::new(doc),
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
}
