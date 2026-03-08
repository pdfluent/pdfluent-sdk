//! PDF content extraction: images, text with positions, and full-text search.

pub mod error;
pub mod images;
pub mod search;
pub mod text;

pub use error::{ExtractError, Result};
pub use images::{extract_all_images, extract_page_images, ExtractedImage, ImageFilter};
pub use search::{
    count_occurrences, count_text_only, pages_containing, search_text, SearchOptions, SearchResult,
};
pub use text::{
    extract_page_text, extract_positioned_chars, extract_text, PositionedChar, TextBlock,
};
