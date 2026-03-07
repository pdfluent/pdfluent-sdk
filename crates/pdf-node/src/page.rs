//! PdfPage class exposed to Node.js.
//!
//! Provides per-page operations: render, text extraction, geometry.

use crate::annotation::{self, AnnotationInfo};
use crate::document::{PageGeometry, RenderOpts, RenderResult, TextBlockInfo, TextSpanInfo};
use crate::error::to_napi_error;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdf_engine::{PdfDocument as RustDocument, RenderOptions, ThumbnailOptions};
use std::sync::Arc;

/// A handle to a single page within a PDF document.
#[napi]
pub struct PdfPage {
    doc: Arc<RustDocument>,
    index: u32,
}

impl PdfPage {
    pub fn new(doc: Arc<RustDocument>, index: u32) -> Self {
        Self { doc, index }
    }
}

fn convert_render_opts(opts: &Option<RenderOpts>) -> RenderOptions {
    crate::document::convert_render_opts_inner(opts)
}

#[napi]
impl PdfPage {
    /// Page index (0-based).
    #[napi(getter)]
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Page width in PDF points.
    #[napi(getter)]
    pub fn width(&self) -> Result<f64> {
        let g = self
            .doc
            .page_geometry(self.index as usize)
            .map_err(to_napi_error)?;
        let (w, _) = g.effective_dimensions();
        Ok(w)
    }

    /// Page height in PDF points.
    #[napi(getter)]
    pub fn height(&self) -> Result<f64> {
        let g = self
            .doc
            .page_geometry(self.index as usize)
            .map_err(to_napi_error)?;
        let (_, h) = g.effective_dimensions();
        Ok(h)
    }

    /// Page geometry (dimensions + rotation).
    #[napi]
    pub fn geometry(&self) -> Result<PageGeometry> {
        let g = self
            .doc
            .page_geometry(self.index as usize)
            .map_err(to_napi_error)?;
        let (w, h) = g.effective_dimensions();
        Ok(PageGeometry {
            width: w,
            height: h,
            rotation: g.rotation.degrees(),
        })
    }

    /// Render this page to RGBA pixels (synchronous).
    #[napi]
    pub fn render(&self, options: Option<RenderOpts>) -> Result<RenderResult> {
        let ro = convert_render_opts(&options);
        let rp = self
            .doc
            .render_page(self.index as usize, &ro)
            .map_err(to_napi_error)?;
        Ok(crate::document::rendered_to_result_inner(rp))
    }

    /// Render this page to RGBA pixels (async — worker thread).
    #[napi]
    pub async fn render_async(&self, options: Option<RenderOpts>) -> Result<RenderResult> {
        let doc = self.doc.clone();
        let index = self.index;
        let ro = convert_render_opts(&options);
        let rp = tokio::task::spawn_blocking(move || doc.render_page(index as usize, &ro))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)?;
        Ok(crate::document::rendered_to_result_inner(rp))
    }

    /// Generate a thumbnail (async).
    #[napi]
    pub async fn thumbnail(&self, max_dimension: Option<u32>) -> Result<RenderResult> {
        let doc = self.doc.clone();
        let index = self.index;
        let opts = ThumbnailOptions {
            max_dimension: max_dimension.unwrap_or(256),
        };
        let rp = tokio::task::spawn_blocking(move || doc.thumbnail(index as usize, &opts))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)?;
        Ok(crate::document::rendered_to_result_inner(rp))
    }

    /// Extract text from this page.
    #[napi]
    pub fn text(&self) -> Result<String> {
        self.doc
            .extract_text(self.index as usize)
            .map_err(to_napi_error)
    }

    /// Extract text from this page (async).
    #[napi]
    pub async fn text_async(&self) -> Result<String> {
        let doc = self.doc.clone();
        let index = self.index;
        tokio::task::spawn_blocking(move || doc.extract_text(index as usize))
            .await
            .map_err(|e| napi::Error::from_reason(format!("join error: {e}")))?
            .map_err(to_napi_error)
    }

    /// Extract structured text blocks from this page.
    #[napi]
    pub fn text_blocks(&self) -> Result<Vec<TextBlockInfo>> {
        let blocks = self
            .doc
            .extract_text_blocks(self.index as usize)
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

    /// Get annotations on this page.
    #[napi]
    pub fn annotations(&self) -> Vec<AnnotationInfo> {
        annotation::page_annotations(self.doc.pdf(), self.index as usize)
    }
}
