//! PDF page rendering via PDFium.
//!
//! Wraps pdfium-render to provide page rendering to images.
//! Requires the PDFium shared library to be available at runtime.

use crate::error::{PdfError, Result};
use pdfium_render::prelude::*;
use std::path::Path;

/// A wrapper around PDFium for PDF operations.
pub struct PdfRenderer {
    pdfium: Pdfium,
}

impl PdfRenderer {
    /// Create a new renderer by loading PDFium from the system.
    pub fn new() -> Result<Self> {
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|e| PdfError::LibraryNotFound(format!("{e}")))?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    /// Create a new renderer from a specific library path.
    pub fn from_library(path: &Path) -> Result<Self> {
        let bindings =
            Pdfium::bind_to_library(path).map_err(|e| PdfError::LibraryNotFound(format!("{e}")))?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    /// Load a PDF document from bytes.
    pub fn load_document<'a>(&'a self, bytes: &'a [u8]) -> Result<PdfDocument<'a>> {
        self.pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| PdfError::LoadFailed(format!("{e}")))
    }

    /// Load a PDF document from a file path.
    pub fn load_file(&self, path: &Path) -> Result<PdfDocument<'_>> {
        self.pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| PdfError::LoadFailed(format!("{e}")))
    }

    /// Render a single page to an image.
    pub fn render_page(
        &self,
        document: &PdfDocument<'_>,
        page_index: u16,
        dpi: f32,
    ) -> Result<image::DynamicImage> {
        let page = document
            .pages()
            .get(page_index)
            .map_err(|e| PdfError::RenderError(format!("page {page_index}: {e}")))?;

        let config = PdfRenderConfig::new()
            .set_target_width((page.width().value * dpi / 72.0) as Pixels)
            .set_maximum_height((page.height().value * dpi / 72.0) as Pixels);

        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| PdfError::RenderError(format!("{e}")))?;

        Ok(bitmap.as_image())
    }

    /// Render all pages of a document to images.
    pub fn render_all_pages(
        &self,
        document: &PdfDocument<'_>,
        dpi: f32,
    ) -> Result<Vec<image::DynamicImage>> {
        let page_count = document.pages().len();
        let mut images = Vec::with_capacity(page_count as usize);

        for i in 0..page_count {
            let img = self.render_page(document, i, dpi)?;
            images.push(img);
        }

        Ok(images)
    }

    /// Get page count for a document.
    pub fn page_count(&self, document: &PdfDocument<'_>) -> u16 {
        document.pages().len()
    }
}

// Note: Integration tests for the renderer require the PDFium library
// to be installed. These tests are gated behind the `pdfium` feature
// or run manually with the library present.
