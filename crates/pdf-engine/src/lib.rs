#![deny(missing_docs)]
//! Unified PDF rendering engine.
//!
//! `pdf-engine` is the main public-facing API for reading and rendering PDF
//! documents. It wraps the lower-level `pdf-syntax` / `pdf-interpret` /
//! `pdf-render` stack and exposes a single [`PdfDocument`] handle for all
//! common operations: page rendering, text extraction, thumbnails, metadata,
//! bookmarks, and full-text search.
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//! use pdf_engine::{PdfDocument, RenderOptions};
//!
//! // Load from bytes (accepts Arc<Vec<u8>>, Vec<u8>, or any Into<PdfData>).
//! let data = Arc::new(std::fs::read("invoice.pdf").unwrap());
//! let doc = PdfDocument::open(data).unwrap();
//!
//! println!("{} pages — {:?}", doc.page_count(), doc.info().title);
//!
//! // Render page 0 at 150 DPI → raw RGBA pixel data.
//! let opts = RenderOptions { dpi: 150.0, ..Default::default() };
//! let rendered = doc.render_page(0, &opts).unwrap();
//! println!("{}×{} px", rendered.width, rendered.height);
//!
//! // Plain-text extraction.
//! let text = doc.extract_text(0).unwrap();
//! println!("{text}");
//!
//! // Structured text with per-span positions.
//! for block in doc.extract_text_blocks(0).unwrap() {
//!     for span in &block.spans {
//!         println!("  [{:.0}, {:.0}] {}", span.x, span.y, span.text);
//!     }
//! }
//!
//! // Full-text search — returns 0-based page indices.
//! let hits = doc.search_text("total");
//! println!("'total' found on {} page(s)", hits.len());
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |---|---|
//! | [`PdfDocument`] | Main document handle |
//! | [`RenderOptions`] | DPI, background colour, optional forced width/height |
//! | [`RenderedPage`] | RGBA pixel data (row-major, 4 bytes per pixel) |
//! | [`PageGeometry`] | MediaBox, CropBox, TrimBox, BleedBox, rotation |
//! | [`PageBox`] | A rectangle in PDF user-space points |
//! | [`DocumentInfo`] | Title, author, subject, creator, producer |
//! | [`TextBlock`] / [`TextSpan`] | Structured text with position and font size |
//! | [`BookmarkItem`] | Outline node — title, target page, nested children |
//! | [`ThumbnailOptions`] | Max-dimension constraint for thumbnail rendering |

pub mod document;
pub mod error;
pub mod geometry;
pub mod ocr;
pub mod render;
pub mod text;
pub mod thumbnail;

pub use document::{BookmarkItem, DocumentInfo, PdfDocument};
pub use error::{EngineError, Result};
pub use geometry::{PageBox, PageGeometry, PageRotation};
pub use ocr::{OcrBackend, OcrError, OcrResult, OcrWord};
pub use render::{RenderOptions, RenderedPage};
pub use text::{TextBlock, TextSpan};
pub use thumbnail::ThumbnailOptions;

#[cfg(feature = "ocr")]
pub use ocr::OcrsBackend;
#[cfg(all(feature = "ocr", not(target_arch = "wasm32")))]
pub use ocr::ocr_page_default;
