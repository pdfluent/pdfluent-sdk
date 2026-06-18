#![warn(missing_docs)]
//! PDF content extraction: text with positions, images, and full-text search.
//!
//! Works directly on [`lopdf::Document`] objects, which can be loaded from a
//! file with `Document::load`. Three extraction targets are available:
//!
//! - **Text** — plain strings or [`TextBlock`] records with page, font, bbox
//! - **Positioned characters** — per-character [`PositionedChar`] with bounding boxes
//! - **Images** — [`ExtractedImage`] with raw pixel data and format metadata
//! - **Search** — substring search with [`SearchResult`] entries (page + bboxes)
//!
//! # Quick Start
//!
//! ```no_run
//! use lopdf::Document;
//! use pdfluent_extract::{extract_text, extract_page_text, search_text, SearchOptions};
//!
//! let doc = Document::load("document.pdf").unwrap();
//!
//! // All text blocks from every page.
//! for block in extract_text(&doc) {
//!     println!("[page {}] {} (font: {}, size: {:.1}pt)",
//!         block.page, block.text, block.font_name, block.font_size);
//! }
//!
//! // Plain text from one page (1-based page number).
//! let text = extract_page_text(&doc, 1).unwrap();
//!
//! // Full-text search with bounding boxes.
//! let opts = SearchOptions { case_insensitive: true, ..Default::default() };
//! for result in search_text(&doc, "invoice", &opts) {
//!     println!("Page {}: {:?} ({} bboxes)",
//!         result.page, result.text, result.bounding_boxes.len());
//! }
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |---|---|
//! | [`TextBlock`] | Text run with page number, bounding box, font name/size |
//! | [`PositionedChar`] | Single character with per-character bounding box |
//! | [`ExtractedImage`] | Raw image data extracted from a page |
//! | [`SearchResult`] | Match with page, text, character bounding boxes, offset |
//! | [`SearchOptions`] | Case sensitivity, page filter, max results, bbox toggle |

pub mod error;
pub mod images;
pub mod search;
pub mod text;

pub use error::{ExtractError, Result};
pub use images::{
    encode_image_for_document, extract_all_images, extract_images_from_page_id,
    extract_page_images, EncodedImage, ExtractedImage, ImageFilter,
};
pub use search::{
    count_occurrences, count_text_only, pages_containing, search_text, SearchOptions, SearchResult,
};
pub use text::{
    extract_blocks_from_page_id, extract_page_blocks, extract_page_text, extract_positioned_chars,
    extract_text, PositionedChar, TextBlock, WidthSource,
};
