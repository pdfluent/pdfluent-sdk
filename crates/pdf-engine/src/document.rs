//! Unified document facade — multi-page rendering, text extraction,
//! metadata, bookmarks, and thumbnails.

use crate::error::{EngineError, Result};
use crate::geometry::{self, PageGeometry};
use crate::render::{self, ColorMode, RenderConfig, RenderOptions, RenderedPage};
use crate::text::{TextBlock, TextExtractionDevice};
use crate::thumbnail::ThumbnailOptions;

use pdf_forms::parse::parse_acroform;
use pdf_forms::tree::FieldValue;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{interpret_page, Context, InterpreterSettings};
use pdf_render::pdf_syntax::object::dict::keys::{FIRST, NEXT, OUTLINES, TITLE};
use pdf_render::pdf_syntax::object::Dict;
use pdf_render::pdf_syntax::page::Page;
use pdf_render::pdf_syntax::Pdf;
#[cfg(feature = "parallel")]
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

    /// Render a single page using the high-level render config.
    pub fn render_page_with_config(
        &self,
        index: usize,
        config: &RenderConfig,
    ) -> Result<RenderedPage> {
        let page = self.get_page(index)?;
        Ok(render::render_page_with_config(
            page,
            config,
            &self.settings,
        ))
    }

    /// Render a single page to a CMYK buffer.
    pub fn render_page_cmyk(&self, index: usize, dpi: u32) -> Result<RenderedPage> {
        self.render_page_with_config(
            index,
            &RenderConfig {
                color_mode: ColorMode::PreserveCmyk,
                dpi,
            },
        )
    }

    /// Render all pages, in parallel when the `parallel` feature is enabled.
    pub fn render_all(&self, options: &RenderOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_page(&pages[i], options, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
            .map(|i| render::render_page(&pages[i], options, &self.settings))
            .collect()
    }

    /// Render all pages using the high-level render config.
    pub fn render_all_with_config(&self, config: &RenderConfig) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_page_with_config(&pages[i], config, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
            .map(|i| render::render_page_with_config(&pages[i], config, &self.settings))
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

    /// Generate thumbnails for all pages, in parallel when the `parallel` feature is enabled.
    pub fn thumbnails_all(&self, options: &ThumbnailOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_thumbnail(&pages[i], options.max_dimension, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
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

    /// Extract text values from AcroForm fields (text and choice fields only).
    ///
    /// Returns a single string concatenating all non-empty field values separated
    /// by newlines. Useful when the document stores its readable content in form
    /// field values rather than (or in addition to) page content streams.
    pub fn extract_acroform_text(&self) -> String {
        let Some(tree) = parse_acroform(&self.pdf) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for id in tree.all_ids() {
            let node = tree.get(id);
            if node.children.is_empty() {
                // Terminal (widget) — collect text-like values.
                let value_str = match &node.value {
                    Some(FieldValue::Text(s)) if !s.is_empty() => Some(s.clone()),
                    Some(FieldValue::StringArray(arr)) => {
                        let joined = arr
                            .iter()
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        if joined.is_empty() {
                            None
                        } else {
                            Some(joined)
                        }
                    }
                    _ => None,
                };
                if let Some(s) = value_str {
                    parts.push(s);
                }
            }
        }
        parts.join("\n")
    }

    /// Extract all text from the document: page content streams plus AcroForm
    /// field values.  Mirrors pdftotext behaviour.
    pub fn extract_all_text(&self) -> String {
        let mut text = String::new();
        for i in 0..self.page_count() {
            if let Ok(page_text) = self.extract_text(i) {
                text.push_str(&page_text);
            }
        }
        let acroform = self.extract_acroform_text();
        if !acroform.is_empty() {
            text.push('\n');
            text.push_str(&acroform);
        }
        text
    }

    /// Simple text search: returns page indices containing the query string.
    pub fn search_text(&self, query: &str) -> Vec<usize> {
        let pages = self.pdf.pages();
        let query_lower = query.to_lowercase();
        let page_contains = |i: usize| -> Option<usize> {
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
            if device.into_text().to_lowercase().contains(&query_lower) {
                Some(i)
            } else {
                None
            }
        };
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .filter_map(page_contains)
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len()).filter_map(page_contains).collect()
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

    /// Run OCR on a page and return the recognized text and word positions.
    ///
    /// The page is rendered at `dpi` (default 150) before recognition.
    /// Pass any [`OcrBackend`] implementation; use [`OcrsBackend::try_default`]
    /// to load the pure-Rust `ocrs` engine from the standard model paths.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[cfg(feature = "ocr")] {
    /// use pdf_engine::{PdfDocument, OcrsBackend, RenderOptions};
    ///
    /// let doc = PdfDocument::open(std::fs::read("scan.pdf").unwrap()).unwrap();
    /// let backend = OcrsBackend::try_default().unwrap();
    /// let result = doc.ocr_page(0, &backend, 150.0_f64).unwrap();
    /// println!("{}", result.text);
    /// # }
    /// ```
    pub fn ocr_page(
        &self,
        index: usize,
        backend: &dyn crate::ocr::OcrBackend,
        dpi: f64,
    ) -> crate::error::Result<crate::ocr::OcrResult> {
        let opts = crate::render::RenderOptions {
            dpi,
            ..Default::default()
        };
        let rendered = self.render_page(index, &opts)?;

        // Convert RGBA → RGB (ocrs expects RGB input).
        let mut rgb = Vec::with_capacity((rendered.width * rendered.height * 3) as usize);
        for chunk in rendered.pixels.chunks(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }

        backend
            .recognize(&rgb, rendered.width, rendered.height)
            .map_err(|e| crate::error::EngineError::RenderError(e.to_string()))
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
    use crate::render::{ColorMode, PixelFormat, RenderConfig, RenderOptions};
    use lopdf::{Document as LoDocument, Object};
    use std::path::PathBuf;

    fn corpus_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus")
            .join(name)
    }

    fn normalize_text(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn strip_type0_tounicode(data: &[u8]) -> (Vec<u8>, usize) {
        fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<Vec<u8>> {
            match dict.get(key).ok()? {
                Object::Name(name) => Some(name.clone()),
                _ => None,
            }
        }

        fn descendant_is_cidfont_type2(doc: &LoDocument, type0: &lopdf::Dictionary) -> bool {
            let Some(Object::Array(descendants)) = type0.get(b"DescendantFonts").ok() else {
                return false;
            };
            let Some(Object::Reference(desc_id)) = descendants.first() else {
                return false;
            };
            let Ok(Object::Dictionary(descendant)) = doc.get_object(*desc_id) else {
                return false;
            };
            matches!(
                descendant.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"CIDFontType2"
            )
        }

        let mut doc = LoDocument::load_mem(data).expect("load stripped-to-unicode fixture");
        let ids: Vec<_> = doc.objects.keys().copied().collect();
        let mut removed = 0usize;

        for id in ids {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            if !matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"Type0"
            ) {
                continue;
            }
            if !matches!(
                get_name(dict, b"Encoding").as_deref(),
                Some(b"Identity-H") | Some(b"Identity-V")
            ) {
                continue;
            }
            if !descendant_is_cidfont_type2(&doc, dict) {
                continue;
            }

            if let Some(Object::Dictionary(type0)) = doc.objects.get_mut(&id) {
                if type0.has(b"ToUnicode") {
                    type0.remove(b"ToUnicode");
                    removed += 1;
                }
            }
        }

        let mut out = Vec::new();
        doc.save_to(&mut out)
            .expect("save stripped-to-unicode fixture");
        (out, removed)
    }

    fn solid_fill_pdf_bytes(color_operator: &str) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = format!("{color_operator}\n0 0 72 72 re f\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(72),
                    Object::Integer(72),
                ]),
                "Contents" => Object::Reference(content_id),
            }),
        );

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save solid fill fixture");
        bytes
    }

    fn mixed_rgb_cmyk_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = b"1 0 0 rg\n0 0 36 72 re f\n1 0 0 0 k\n36 0 36 72 re f\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 72.into(), 72.into()]),
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("save mixed rgb/cmyk fixture");
        bytes
    }

    fn transparent_cmyk_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let gs_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(0.5),
        }));
        let content = b"/GS1 gs\n1 0 0 0 k\n0 0 72 72 re f\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 72.into(), 72.into()]),
                "Resources" => dictionary! {
                    "ExtGState" => dictionary! {
                        "GS1" => Object::Reference(gs_id),
                    },
                },
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("save transparent cmyk fixture");
        bytes
    }

    fn cmyk_image_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let image_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => Object::Integer(2),
                "Height" => Object::Integer(1),
                "BitsPerComponent" => Object::Integer(8),
                "ColorSpace" => "DeviceCMYK",
            },
            vec![255, 0, 0, 0, 0, 255, 0, 0],
        ));
        let content = b"q\n2 0 0 1 0 0 cm\n/Im1 Do\nQ\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 2.into(), 1.into()]),
                "Resources" => dictionary! {
                    "XObject" => dictionary! {
                        "Im1" => Object::Reference(image_id),
                    },
                },
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save cmyk image fixture");
        bytes
    }

    fn pixel_at(rendered: &RenderedPage, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * rendered.width + x) * 4) as usize;
        [
            rendered.pixels[idx],
            rendered.pixels[idx + 1],
            rendered.pixels[idx + 2],
            rendered.pixels[idx + 3],
        ]
    }

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

    #[test]
    fn extract_text_type0_without_tounicode_uses_font_program_fallback() {
        let original = std::fs::read(corpus_path("sf181.pdf")).expect("read sf181 fixture");
        let expected = PdfDocument::open(original.clone())
            .expect("open original sf181")
            .extract_text(0)
            .expect("extract original sf181 text");
        assert!(
            expected.contains("Guide to Personnel Data Standards"),
            "unexpected baseline extraction: {expected}"
        );

        let (stripped, removed) = strip_type0_tounicode(&original);
        assert!(
            removed > 0,
            "expected to strip at least one Type0 ToUnicode"
        );

        let actual = PdfDocument::open(stripped)
            .expect("open stripped sf181")
            .extract_text(0)
            .expect("extract stripped sf181 text");

        let actual_norm = normalize_text(&actual);
        let expected_norm = normalize_text(&expected);

        assert!(
            actual_norm.contains("Guide to Personnel Data Standards"),
            "missing main heading after stripping ToUnicode: {actual_norm}"
        );
        assert!(
            actual_norm.contains("Privacy Act Statement"),
            "missing body text after stripping ToUnicode: {actual_norm}"
        );
        assert!(
            actual_norm.len() + 32 >= expected_norm.len(),
            "too much text lost after stripping ToUnicode: expected {} chars, got {}",
            expected_norm.len(),
            actual_norm.len()
        );
    }

    #[test]
    fn render_page_with_config_srgb_matches_legacy_render_page() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 rg")).expect("open rgb fixture");
        let legacy = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 72.0,
                    ..Default::default()
                },
            )
            .expect("legacy render succeeds");
        let configured = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::Srgb,
                    dpi: 72,
                },
            )
            .expect("configured render succeeds");

        assert_eq!(legacy.width, configured.width);
        assert_eq!(legacy.height, configured.height);
        assert_eq!(legacy.pixel_format, PixelFormat::Rgba8);
        assert_eq!(configured.pixel_format, PixelFormat::Rgba8);
        assert_eq!(legacy.pixels, configured.pixels);
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_returns_cmyk_buffer() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 0 k")).expect("open cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Cmyk8);
        assert_eq!(
            rendered.pixels.len(),
            rendered.width as usize * rendered.height as usize * 4
        );
        assert_eq!(
            pixel_at(&rendered, rendered.width / 2, rendered.height / 2),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn render_page_with_config_simulate_cmyk_does_not_panic_on_cmyk_pdf() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 0 k")).expect("open cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::SimulateCmyk,
                    dpi: 72,
                },
            )
            .expect("simulate cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Rgba8);
        assert!(!rendered.pixels.is_empty());
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_mixed_page_preserves_only_cmyk_region() {
        let doc = PdfDocument::open(mixed_rgb_cmyk_pdf_bytes()).expect("open mixed fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("mixed render succeeds");

        assert_eq!(
            pixel_at(&rendered, 54, 36),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
        assert_ne!(
            pixel_at(&rendered, 18, 36),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_transparent_page_does_not_crash() {
        let doc =
            PdfDocument::open(transparent_cmyk_pdf_bytes()).expect("open transparent cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("transparent cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Cmyk8);
        assert_eq!(
            rendered.pixels.len(),
            rendered.width as usize * rendered.height as usize * 4
        );
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_keeps_device_cmyk_image_bytes() {
        let doc = PdfDocument::open(cmyk_image_pdf_bytes()).expect("open cmyk image fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("cmyk image render succeeds");

        assert_eq!(rendered.width, 2);
        assert_eq!(rendered.height, 1);
        assert_eq!(pixel_at(&rendered, 0, 0), [255, 0, 0, 0]);
        assert_eq!(pixel_at(&rendered, 1, 0), [0, 255, 0, 0]);
    }
}
