// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![warn(missing_docs)]
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
//! | [`BatchConfig`] / [`BatchResult`] | Worker-pool processing for many PDFs |
//! | [`PdfDocument`] | Main document handle |
//! | [`RenderConfig`] / [`RenderOptions`] | DPI, color mode, background colour, optional forced width/height |
//! | [`RenderedPage`] | RGBA or CMYK pixel data (row-major, 4 bytes per pixel) |
//! | [`PageGeometry`] | MediaBox, CropBox, TrimBox, BleedBox, rotation |
//! | [`PageBox`] | A rectangle in PDF user-space points |
//! | [`DocumentInfo`] | Title, author, subject, creator, producer |
//! | [`TextBlock`] / [`TextSpan`] | Structured text with position and font size |
//! | [`Block`] / [`BlockKind`] / [`ExtractOptions`] | Paragraphs, headings and list items, and which pages of them come back |
//! | [`Word`] | One word with its box, page, block and line |
//! | [`to_markdown`] / [`to_json`] | Those blocks as markdown for a reader, or as JSON for a chunker |
//! | [`BookmarkItem`] | Outline node — title, target page, nested children |
//! | [`ThumbnailOptions`] | Max-dimension constraint for thumbnail rendering |

pub mod api;
/// Public engine error type and stable error-code mapping. See
/// [`api_error::PdfEngineError`].
pub mod api_error;
/// The balanced tier's page assembly: detector boxes plus our own text (#641).
/// Off by default, beside [`layout`].
#[cfg(feature = "ort-layout")]
pub mod balanced;
pub mod batch;
/// Visual-to-logical reordering of a line of extracted text (UAX #9 L2).
/// Internal: extraction reorders, nothing outside it needs to.
mod bidi;
/// Paragraphs and the [`blocks::Block`] model the structured serializers
/// consume (#567).
pub mod blocks;
pub mod color;
pub mod docling;
pub mod document;
pub mod error;
/// Running headers, footers and page numbers, from what a document repeats on
/// three or more pages (#571). Internal: it runs on the way out of
/// `extract_blocks` and the kind it decides is on the block. The public API
/// over the model is #573.
mod furniture;
pub mod geometry;
/// Heading levels, from the document's own size and weight statistics (#568).
/// Internal: it runs on the way out of `extract_blocks`, and the level it
/// decides is on the block. The public API over the model is #573.
mod headings;
/// The block model as JSON: every block with its kind, page, box, text, spans
/// and reading-order index, for a caller that chunks the document itself
/// (#572).
pub mod json;
/// Layout detection on the shared ONNX Runtime layer (#641). Off by default.
#[cfg(feature = "ort-layout")]
pub mod layout;
#[doc(hidden)]
pub mod learned;
/// Typographic ligatures fold to the letters they stand for. Internal: the
/// option that turns it off rides on the extraction device.
mod ligatures;
pub mod limits;
/// List items, from bullet glyphs, numbering and the column's indent steps
/// (#569). Internal: it runs inside the paragraph merge, and the level and the
/// number it decides are on the block. The public API over the model is #573.
mod lists;
/// The block model as markdown: headings, lists, tables and a marker where the
/// page changed (#572).
pub mod markdown;
/// Figures in the block model: listing, naming, pixels, and references (#658).
pub mod markdown_images;
/// The model tiers' shared surface: specs, failures and install paths (#641).
/// Always compiled; only the session half needs the `ort` feature.
pub mod model;
pub mod ocr;
/// Scan detection: how much OCR a page needs, before any recognizer runs.
pub mod ocr_need;
/// OCR before extraction: a searchable copy of the document to read instead.
/// Internal: it runs inside `extract_blocks` when `ExtractOptions::ocr` asks,
/// and the one public method on it is for bindings with their own engine.
mod ocr_searchable;
/// The reading order of the whole document: the columns of a page, the pages
/// in turn, and the paragraph a page break cut in two (#571). Internal, for
/// the reason the other passes are.
mod reading_order;
pub mod render;
/// Many documents as markdown in one process, for the structured benchmark's
/// fair clock (#623).
#[doc(hidden)]
pub mod structured_batch;
/// Marker-led rows the table pass left at the edge of an accepted table
/// (#636). Internal: it runs after the learned pass, over its blocks, and
/// only adopts paragraphs the accepted tables' own columns confirm.
mod table_row_reclaim;
/// Which stage produced each table, for the env-gated diagnostic trace
/// (#636). Internal: it measures finished cells and veto verdicts and prints
/// numbers-only lines on stderr. Rule A2 (drawing_dense) lives here too: it
/// withdraws a table whose box holds three or more drawn clusters.
pub(crate) mod table_trace;
/// Tables: the column grid a run of rows stands in, and the cells that fill it
/// (#570). Internal: it runs inside the paragraph merge, and the row, column
/// and spans it decides are on the block. The public API over the model is
/// #573.
mod tables;
/// Kinds and reading order from a tagged PDF's own structure tree (#636).
/// Internal: it runs last over the finished blocks when
/// `ExtractOptions::use_tags` is set, and only where the tree passes the
/// validity check; anything else returns the geometric answer untouched.
mod tagged_evidence;
pub mod text;
pub mod text_info;
pub mod thumbnail;
/// Optional vision layer: raster plus ONNX runner (#636). Off by default.
#[cfg(feature = "vision")]
pub mod vision;
pub mod words;
pub mod words_join;
#[cfg(feature = "xfa")]
pub mod xfa;

pub use batch::{
    process_batch, process_batch_with_page_progress, BatchConfig, BatchResult, ErrorStrategy,
    PdfBatch,
};
pub use blocks::{
    Block, BlockKind, ClipMargins, ExtractOptions, OcrMode, PageSeparators, Quality,
    TableRenderMode, TableStrategy, TatrOptions,
};
/// The P3 test seam behind [`TatrOptions`]. Hidden: tests name it, callers
/// never do.
#[doc(hidden)]
pub use blocks::{TatrTestCell, TatrTestHook};
pub use color::preserve_device_cmyk;
pub use docling::{to_docling_json, DoclingPage};
pub use document::{join_page_texts, BookmarkItem, DocumentInfo, PdfDocument};
pub use error::{EngineError, OptionsError, Result};
pub use geometry::{PageBox, PageGeometry, PageRotation};
pub use json::to_json;
pub use limits::{LimitError, ProcessingLimits};
pub use markdown::{
    to_markdown, to_markdown_with_code, to_markdown_with_options, to_markdown_with_switches,
    MarkdownOptions,
};
pub use ocr::{OcrBackend, OcrError, OcrResult, OcrWord};
pub use ocr_need::{page_needs_ocr, OcrNeed};
pub use render::{
    ColorMode, PixelFormat, RasterQuality, RenderConfig, RenderOptions, RenderedPage,
};
pub use text::{
    BoundsSource, FontMetrics, GeometryMode, TextBlock, TextExtractionDevice, TextSpan, WidthSource,
};
pub use text_info::TextSpanInfo;
pub use thumbnail::ThumbnailOptions;
pub use words::{to_json_with_words, to_words_json_lines, Word};

#[cfg(not(target_arch = "wasm32"))]
pub use ocr::best_available_backend;
#[cfg(all(feature = "ocr", not(target_arch = "wasm32")))]
pub use ocr::ocr_page_default;
#[cfg(feature = "ocr-aws")]
pub use ocr::AwsTextractBackend;
#[cfg(feature = "ocr-azure")]
pub use ocr::AzureDocIntelBackend;
#[cfg(feature = "ocr-google")]
pub use ocr::GoogleVisionBackend;
#[cfg(feature = "ocr-mistral")]
pub use ocr::MistralOcrBackend;
#[cfg(feature = "ocr")]
pub use ocr::OcrsBackend;
#[cfg(feature = "ocr-onnx")]
pub use ocr::PaddleOnnxBackend;
