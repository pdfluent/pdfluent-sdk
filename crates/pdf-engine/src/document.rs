//! Unified document facade — multi-page rendering, text extraction,
//! metadata, bookmarks, and thumbnails.

use crate::error::{EngineError, Result};
use crate::geometry::{self, PageGeometry};
use crate::render::{self, RenderOptions, RenderedPage};
use crate::text::{TextBlock, TextExtractionDevice};
use crate::thumbnail::ThumbnailOptions;

use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{interpret_page, Context, InterpreterSettings};
use pdf_render::pdf_syntax::object::dict::keys::{FIRST, NEXT, OUTLINES, TITLE};
use pdf_render::pdf_syntax::object::Dict;
use pdf_render::pdf_syntax::page::Page;
use pdf_render::pdf_syntax::Pdf;
use rayon::prelude::*;

use kurbo::Rect;

/// Document metadata extracted from the info dictionary.
#[derive(Debug, Clone, Default)]
pub struct DocumentInfo {
    /// Document title.
    pub title: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Keywords.
    pub keywords: Option<String>,
    /// Creator application.
    pub creator: Option<String>,
    /// Producer application.
    pub producer: Option<String>,
}

/// A bookmark / outline item.
#[derive(Debug, Clone)]
pub struct BookmarkItem {
    /// Bookmark title.
    pub title: String,
    /// Target page index (0-based), if resolvable.
    pub page: Option<usize>,
    /// Nested child bookmarks.
    pub children: Vec<BookmarkItem>,
}

/// High-level PDF document handle.
pub struct PdfDocument {
    pdf: Pdf,
    settings: InterpreterSettings,
}

impl PdfDocument {
    /// Open a PDF from bytes.
    pub fn open(data: impl Into<pdf_render::pdf_syntax::PdfData>) -> Result<Self> {
        let pdf = Pdf::new(data).map_err(|e| EngineError::InvalidPdf(format!("{e:?}")))?;
        Ok(Self {
            pdf,
            settings: InterpreterSettings::default(),
        })
    }

    /// Open a password-protected PDF.
    pub fn open_with_password(
        data: impl Into<pdf_render::pdf_syntax::PdfData>,
        password: &str,
    ) -> Result<Self> {
        let pdf = Pdf::new_with_password(data, password)
            .map_err(|e| EngineError::InvalidPdf(format!("{e:?}")))?;
        Ok(Self {
            pdf,
            settings: InterpreterSettings::default(),
        })
    }

    /// Access the underlying parsed PDF.
    pub fn pdf(&self) -> &Pdf {
        &self.pdf
    }

    /// Set interpreter settings (font resolver, cmap resolver, etc.).
    pub fn set_settings(&mut self, settings: InterpreterSettings) {
        self.settings = settings;
    }

    /// Number of pages.
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Get the geometry of a page.
    pub fn page_geometry(&self, index: usize) -> Result<PageGeometry> {
        let page = self.get_page(index)?;
        Ok(geometry::extract_geometry(page))
    }

    /// Render a single page.
    pub fn render_page(&self, index: usize, options: &RenderOptions) -> Result<RenderedPage> {
        let page = self.get_page(index)?;
        Ok(render::render_page(page, options, &self.settings))
    }

    /// Render all pages in parallel using rayon.
    pub fn render_all(&self, options: &RenderOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_page(&pages[i], options, &self.settings))
            .collect()
    }

    /// Generate a thumbnail for a single page.
    pub fn thumbnail(&self, index: usize, options: &ThumbnailOptions) -> Result<RenderedPage> {
        let page = self.get_page(index)?;
        Ok(render::render_thumbnail(
            page,
            options.max_dimension,
            &self.settings,
        ))
    }

    /// Generate thumbnails for all pages in parallel.
    pub fn thumbnails_all(&self, options: &ThumbnailOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_thumbnail(&pages[i], options.max_dimension, &self.settings))
            .collect()
    }

    /// Extract text from a page as a single string.
    pub fn extract_text(&self, index: usize) -> Result<String> {
        let page = self.get_page(index)?;
        let mut device = TextExtractionDevice::new();
        let mut ctx = self.create_context(page);
        interpret_page(page, &mut ctx, &mut device);
        Ok(device.into_text())
    }

    /// Extract structured text blocks from a page.
    pub fn extract_text_blocks(&self, index: usize) -> Result<Vec<TextBlock>> {
        let page = self.get_page(index)?;
        let mut device = TextExtractionDevice::new();
        let mut ctx = self.create_context(page);
        interpret_page(page, &mut ctx, &mut device);
        Ok(device.into_blocks())
    }

    /// Simple text search: returns page indices containing the query string.
    pub fn search_text(&self, query: &str) -> Vec<usize> {
        let pages = self.pdf.pages();
        let query_lower = query.to_lowercase();

        (0..pages.len())
            .into_par_iter()
            .filter_map(|i| {
                let page = &pages[i];
                let mut device = TextExtractionDevice::new();
                let mut ctx = Context::new(
                    page.initial_transform(false),
                    Rect::new(
                        0.0,
                        0.0,
                        page.render_dimensions().0 as f64,
                        page.render_dimensions().1 as f64,
                    ),
                    page.xref(),
                    self.settings.clone(),
                );
                interpret_page(page, &mut ctx, &mut device);
                let text = device.into_text().to_lowercase();
                if text.contains(&query_lower) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Extract document metadata.
    pub fn info(&self) -> DocumentInfo {
        let meta = self.pdf.metadata();
        DocumentInfo {
            title: meta.title.as_ref().map(|b| bytes_to_string(b)),
            author: meta.author.as_ref().map(|b| bytes_to_string(b)),
            subject: meta.subject.as_ref().map(|b| bytes_to_string(b)),
            keywords: meta.keywords.as_ref().map(|b| bytes_to_string(b)),
            creator: meta.creator.as_ref().map(|b| bytes_to_string(b)),
            producer: meta.producer.as_ref().map(|b| bytes_to_string(b)),
        }
    }

    /// Extract document outline / bookmarks.
    pub fn bookmarks(&self) -> Vec<BookmarkItem> {
        let xref = self.pdf.xref();
        let root_id = xref.root_id();
        let catalog: Dict<'_> = match xref.get(root_id) {
            Some(d) => d,
            None => return Vec::new(),
        };

        let outlines: Dict<'_> = match catalog.get(OUTLINES) {
            Some(d) => d,
            None => return Vec::new(),
        };

        let first: Dict<'_> = match outlines.get(FIRST) {
            Some(d) => d,
            None => return Vec::new(),
        };

        parse_outline_items(&first)
    }

    fn get_page(&self, index: usize) -> Result<&Page<'_>> {
        let pages = self.pdf.pages();
        if index >= pages.len() {
            return Err(EngineError::PageOutOfRange {
                index,
                count: pages.len(),
            });
        }
        Ok(&pages[index])
    }

    fn create_context<'a>(&self, page: &Page<'a>) -> Context<'a> {
        let (w, h) = page.render_dimensions();
        Context::new(
            page.initial_transform(false),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            page.xref(),
            self.settings.clone(),
        )
    }
}

/// Walk the outline linked list (FIRST → NEXT chain).
fn parse_outline_items(item_dict: &Dict<'_>) -> Vec<BookmarkItem> {
    let mut items = Vec::new();
    let mut current: Option<Dict<'_>> = Some(item_dict.clone());

    while let Some(dict) = current {
        let title = dict
            .get::<pdf_render::pdf_syntax::object::String>(TITLE)
            .map(|s| bytes_to_string(s.as_bytes()))
            .unwrap_or_default();

        let children = match dict.get::<Dict<'_>>(FIRST) {
            Some(child_dict) => parse_outline_items(&child_dict),
            None => Vec::new(),
        };

        items.push(BookmarkItem {
            title,
            page: None, // Destination resolution requires named-dest lookup — left for follow-up
            children,
        });

        current = dict.get::<Dict<'_>>(NEXT);
    }

    items
}

/// Convert PDF string bytes to a Rust String (UTF-8 with Latin-1 fallback).
fn bytes_to_string(bytes: &[u8]) -> String {
    // Check for UTF-16 BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let chars: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_be_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        return String::from_utf16_lossy(&chars);
    }

    // Try UTF-8, fall back to Latin-1.
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_to_string_utf8() {
        assert_eq!(bytes_to_string(b"hello"), "hello");
    }

    #[test]
    fn bytes_to_string_latin1() {
        let bytes = &[0xC4, 0xD6, 0xDC]; // ÄÖÜ in Latin-1
        let s = bytes_to_string(bytes);
        assert_eq!(s, "ÄÖÜ");
    }

    #[test]
    fn bytes_to_string_utf16() {
        let bytes = &[0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69]; // UTF-16 "Hi"
        assert_eq!(bytes_to_string(bytes), "Hi");
    }

    #[test]
    fn document_info_default() {
        let info = DocumentInfo::default();
        assert!(info.title.is_none());
        assert!(info.author.is_none());
    }

    #[test]
    fn bookmark_item_children() {
        let item = BookmarkItem {
            title: "Root".into(),
            page: None,
            children: vec![BookmarkItem {
                title: "Child".into(),
                page: Some(0),
                children: Vec::new(),
            }],
        };
        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].title, "Child");
    }
}
