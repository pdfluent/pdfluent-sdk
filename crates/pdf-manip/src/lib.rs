//! PDF manipulation library.
//!
//! Provides page manipulation (merge, split, insert, delete, rearrange,
//! rotate, crop), encryption/decryption with password protection,
//! watermarking (text + image), compression/optimization, and
//! bookmark/outline management.
//!
//! # Modules
//!
//! - [`pages`] — Page manipulation (M1)
//! - [`encrypt`] — Encryption, decryption, passwords (M2)
//! - [`watermark`] — Text and image watermarks (M3)
//! - [`optimize`] — Compression and optimization (M4)
//! - [`bookmarks`] — Bookmarks / outlines (M5)

pub mod bookmarks;
pub mod content_editor;
#[cfg(feature = "image-insert")]
pub mod downsample;
pub mod encrypt;
pub mod error;
#[cfg(feature = "font-subset")]
pub mod font_subset;
pub mod header_footer;
#[cfg(feature = "image-insert")]
pub mod image_insert;
pub mod optimize;
pub mod pages;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_xmp;
pub mod text_replace;
pub mod text_run;
pub mod watermark;

pub use content_editor::{ContentEditor, GraphicsSnapshot, GraphicsStateTracker};
pub use error::{ManipError, Result};
pub use text_run::{FontMap, TextRun};
