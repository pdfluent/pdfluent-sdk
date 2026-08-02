#![warn(missing_docs)]
//! PDF manipulation: pages, encryption, watermarks, text replacement, and more.
//!
//! All operations work on [`lopdf::Document`] objects, which can be loaded
//! from a file (`Document::load`) or obtained from another `pdf-manip` call.
//! Most functions return [`Result<_, ManipError>`].
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, ManipError>`.
//!
//! **Note on page numbering:** all page indices throughout this crate are
//! **1-based** to match PDF conventions.
//!
//! # Quick Start
//!
//! ```no_run
//! use lopdf::Document;
//! use pdf_manip::pages;
//! use pdf_manip::encrypt::{EncryptionAlgorithm, encrypt_and_save, EncryptConfig, Permissions};
//!
//! // Merge two PDFs.
//! let mut merged = pages::merge(&["a.pdf", "b.pdf"]).unwrap();
//! merged.save("merged.pdf").unwrap();
//!
//! // Extract pages 1–3 from a document.
//! let doc = Document::load("report.pdf").unwrap();
//! let mut subset = pages::extract_pages(&doc, &[1, 2, 3]).unwrap();
//! subset.save("pages_1_to_3.pdf").unwrap();
//!
//! // Encrypt with AES-256.
//! let mut doc = Document::load("sensitive.pdf").unwrap();
//! let cfg = EncryptConfig {
//!     owner_password: b"owner_pw".to_vec(),
//!     user_password: b"user_pw".to_vec(),
//!     algorithm: EncryptionAlgorithm::Aes256,
//!     permissions: Permissions::allow_all(),
//! };
//! let out = std::fs::File::create("sensitive_enc.pdf").unwrap();
//! encrypt_and_save(&mut doc, &cfg, out).unwrap();
//! ```
//!
//! # Modules
//!
//! Functions are accessed through their sub-modules rather than re-exports:
//!
//! | Module | Operations |
//! |---|---|
//! | [`pages`] | `merge`, `split`, `extract_pages`, `delete_pages`, `insert_pages`, `rotate_pages` |
//! | [`encrypt`] | `encrypt_document`, `remove_encryption`, AES-256 / RC4 |
//! | [`watermark`] | Text and image watermarks |
//! | [`optimize`] | Stream compression, object deduplication |
//! | [`bookmarks`] | Read and write document outline / bookmarks |
//! | [`text_replace`] | Search-and-replace in content streams |
//! | [`header_footer`] | Add headers and footers to pages |

pub mod bookmarks;
pub mod cff_append;
pub mod content_editor;
#[cfg(feature = "image-insert")]
pub mod downsample;
pub(crate) mod encoding_utils;
pub mod encrypt;
pub mod error;
pub(crate) mod flate_decode;
#[cfg(feature = "font-subset")]
pub mod font_subset;
pub mod header_footer;
#[cfg(feature = "image-insert")]
pub mod image_insert;
pub mod optimize;
pub mod pages;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_cleanup;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_colorspace;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_fixups;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_fonts;
#[cfg(feature = "pdfa-convert")]
pub(crate) mod pdfa_repair;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_structure;
#[cfg(feature = "pdfa-convert")]
pub mod pdfa_xmp;
pub mod pdfua;
pub mod text_replace;
pub mod text_run;
pub mod text_style;
pub mod watermark;

pub use content_editor::{ContentEditor, GraphicsSnapshot, GraphicsStateTracker};
pub use error::{ManipError, Result};
pub use text_run::{FontMap, TextRun};
