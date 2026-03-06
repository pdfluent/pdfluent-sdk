//! Unified PDF rendering engine.
//!
//! Provides page rendering, text extraction, thumbnails, metadata, and bookmarks
//! on top of the hayro rendering stack (pdf-syntax / pdf-interpret / pdf-render).

pub mod document;
pub mod error;
pub mod geometry;
pub mod render;
pub mod text;
pub mod thumbnail;

pub use document::{BookmarkItem, DocumentInfo, PdfDocument};
pub use error::{EngineError, Result};
pub use geometry::{PageBox, PageGeometry, PageRotation};
pub use render::{RenderOptions, RenderedPage};
pub use text::{TextBlock, TextSpan};
pub use thumbnail::ThumbnailOptions;
