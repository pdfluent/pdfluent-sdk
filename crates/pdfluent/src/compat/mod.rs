//! A second, compatible spelling of the document API, built on the facade.
//!
//! This module holds an alternative API surface over [`PdfDocument`] that
//! mirrors the call shapes of a widely used PDF library, so existing code
//! written against that library ports over with mechanical edits. It adds no
//! behaviour of its own: every method here delegates to the facade, and every
//! error it returns is an existing [`crate::Error`] variant — no new error
//! variants are introduced.
//!
//! Conventions every lane follows:
//!
//! - Coordinates are top-left origin, y down, in points, matching the rest of
//!   the facade. Lanes convert at the boundary where the mirrored spelling
//!   assumes otherwise.
//! - This module is feature-free: it must not pull optional dependencies
//!   (no `ort`, no model tiers). Lanes needing a model go through the
//!   facade's existing feature-gated paths.
//!
//! # Lane ownership
//!
//! Five lanes fill this module in parallel, and each touches only its own
//! files so no two lanes ever edit the same code:
//!
//! | Lane | Rust file here | Python native file | Python names file |
//! |---|---|---|
//! | P0 value types | `geometry.rs` | `compat/geometry.rs` | `_compat_geometry.py` |
//! | L1 text | `text.rs` | `compat/text.rs` | `_compat_text.py` |
//! | L2 document ops | `document.rs` | `compat/document.rs` | `_compat_document.py` |
//! | L3 images | `pixmap.rs` | `compat/pixmap.rs` | `_compat_pixmap.py` |
//! | L4 editing | `edit.rs` | `compat/edit.rs` | `_compat_edit.py` |
//! | L5 module entry points | `functions.rs` | — | — |
//!
//! Each lane adds its methods as further `impl Page` / `impl PageMut` blocks
//! (or `impl PdfDocument` blocks, or extension traits) inside its own file —
//! multiple inherent impl blocks across files of one crate are fine. The L5
//! free functions live in `functions.rs`. This file holds only the shared
//! views below; lane code never lands here. The Node binding mirrors the
//! layout under `pdf-node/src/compat/`.
//!
//! # Borrowing
//!
//! [`Page`] borrows the document shared (`&PdfDocument`) for the read lanes;
//! [`PageMut`] borrows it exclusively (`&mut PdfDocument`) for the editing
//! lane. That is the facade's existing rule — [`PdfDocument`] is `Send + Sync`
//! and not `Clone`, mutation goes through `&mut self` or a scoped `_mut`
//! accessor ([`MetadataMut`](crate::MetadataMut), [`PdfFormMut`](crate::PdfFormMut))
//! — so no new interior mutability is introduced here. (The interior
//! mutability [`PdfDocument`] already has, the diagnostics collector, stays an
//! implementation detail of the facade.)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::document::PdfDocument;

pub mod constants;
pub mod document;
pub mod edit;
pub mod functions;
pub mod geometry;
pub mod open;
pub mod pixmap;
pub mod text;

pub use document::*;
pub use edit::*;
pub use functions::*;
pub use geometry::*;
pub use pixmap::*;
pub use text::*;

/// Borrowed handle to a single page for the compatible API.
///
/// A read-side view: the text (L1), document (L2) and image (L3) lanes add
/// their methods as further `impl Page` blocks in their own files. The index
/// is 0-based, matching [`PdfDocument::page`]'s numbering once adjusted (that
/// method takes a 1-based page number; see [`new`](Self::new)).
#[derive(Clone, Copy)]
pub struct Page<'a> {
    doc: &'a PdfDocument,
    index: usize,
}

impl<'a> Page<'a> {
    /// Borrow page `index` (0-based) of `doc`.
    ///
    /// The index is stored unchecked; an out-of-range index surfaces as an
    /// error from the lane method that uses it, never as a panic here.
    pub fn new(doc: &'a PdfDocument, index: usize) -> Self {
        Self { doc, index }
    }

    /// The 0-based page index this view was created with.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The document this page belongs to.
    pub fn document(&self) -> &'a PdfDocument {
        self.doc
    }
}

/// Exclusively borrowed handle to a single page for the compatible API.
///
/// The editing lane (L4) adds its methods as further `impl PageMut` blocks in
/// `edit.rs`. Exclusive rather than shared because every mutation on
/// [`PdfDocument`] takes `&mut self`; holding `&mut` here keeps the compatible
/// spelling under the same rule instead of adding interior mutability.
pub struct PageMut<'a> {
    doc: &'a mut PdfDocument,
    index: usize,
}

impl<'a> PageMut<'a> {
    /// Exclusively borrow page `index` (0-based) of `doc`.
    ///
    /// Like [`Page::new`], the index is stored unchecked and validated by the
    /// lane methods that use it.
    pub fn new(doc: &'a mut PdfDocument, index: usize) -> Self {
        Self { doc, index }
    }

    /// The 0-based page index this view was created with.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The document this page belongs to, shared.
    pub fn document(&self) -> &PdfDocument {
        self.doc
    }

    /// The document this page belongs to, for mutation.
    pub fn document_mut(&mut self) -> &mut PdfDocument {
        self.doc
    }
}

/// A dynamically shaped value for structures no recording pins down yet.
///
/// Lanes replace `Value` fields and returns with typed structs as recordings
/// land; until then this keeps the signatures compilable and explicit about
/// what is still unshaped.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// Absent value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Float value.
    Float(f64),
    /// Text value.
    Str(String),
    /// Byte value.
    Bytes(Vec<u8>),
    /// Ordered values.
    List(Vec<Value>),
    /// Named values.
    Dict(std::collections::HashMap<String, Value>),
}

/// Compat failure classes: one kind per recorded exception shape.
///
/// Every lane method that can fail returns [`crate::Error`]; the kind records
/// which recorded exception class the failure answers to, so the bindings
/// raise the matching error. The mapping from recorded classes:
///
/// | Recorded classes | Kind |
/// |---|---|
/// | `TypeError`, `AttributeError` | [`WrongType`](Self::WrongType) |
/// | `ValueError`, `AssertionError`, `ZeroDivisionError` | [`BadValue`](Self::BadValue) |
/// | `KeyError` | [`UnknownKey`](Self::UnknownKey) |
/// | `IndexError` | [`IndexOutOfRange`](Self::IndexOutOfRange) |
/// | `FileNotFoundError` | [`MissingFile`](Self::MissingFile) |
/// | `FileDataError`, `FzErrorFormat` | [`UnreadableFile`](Self::UnreadableFile) |
/// | `EmptyFileError` | [`EmptyFile`](Self::EmptyFile) |
/// | `FzErrorSystem`, `FzErrorArgument`, `RuntimeError`, `Exception` | [`EngineFault`](Self::EngineFault) |
/// | `ModuleNotFoundError` | [`MissingComponent`](Self::MissingComponent) |
/// | `FitzDeprecation` | [`Deprecation`](Self::Deprecation) |
///
/// No new [`crate::Error`] variant is introduced: file failures map onto the
/// existing file variants, and everything else rides inside [`crate::Error::Io`]
/// with [`std::io::ErrorKind::InvalidInput`], following the `OptionsError`
/// pattern, readable through [`crate::Error::compat_kind`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Wrong argument type or shape.
    WrongType,
    /// Right type, unacceptable value.
    BadValue,
    /// Unknown key.
    UnknownKey,
    /// Index outside the valid range.
    IndexOutOfRange,
    /// File not found.
    MissingFile,
    /// File bytes do not parse.
    UnreadableFile,
    /// File has no bytes.
    EmptyFile,
    /// Engine failure.
    EngineFault,
    /// Optional component not installed.
    MissingComponent,
    /// Retired spelling was used.
    Deprecation,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongType => f.write_str("wrong argument type"),
            Self::BadValue => f.write_str("bad value"),
            Self::UnknownKey => f.write_str("unknown key"),
            Self::IndexOutOfRange => f.write_str("index out of range"),
            Self::MissingFile => f.write_str("file not found"),
            Self::UnreadableFile => f.write_str("unreadable file"),
            Self::EmptyFile => f.write_str("empty file"),
            Self::EngineFault => f.write_str("engine fault"),
            Self::MissingComponent => f.write_str("missing component"),
            Self::Deprecation => f.write_str("retired spelling"),
        }
    }
}

impl std::error::Error for ErrorKind {}

impl ErrorKind {
    /// Build the facade error for this kind with a detail message.
    pub fn to_error(&self, detail: impl Into<String>) -> crate::Error {
        let detail = detail.into();
        match self {
            Self::MissingFile => crate::Error::FileNotFound {
                path: detail.into(),
            },
            Self::UnreadableFile | Self::EmptyFile => crate::Error::InvalidPdf {
                byte_offset: None,
                reason: detail,
            },
            kind => crate::Error::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    CompatCause {
                        kind: *kind,
                        detail,
                    },
                ),
                path: None,
            },
        }
    }
}

/// Typed cause carried inside the I/O error [`ErrorKind::to_error`] builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatCause {
    /// Failure class.
    pub kind: ErrorKind,
    /// Detail message.
    pub detail: String,
}

impl std::fmt::Display for CompatCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for CompatCause {}

/// Build the shared "not supported in this version" error for a compat member.
///
/// Every silent no-op on the compatible surface fails with this instead of
/// returning success: one [`crate::Error::Unsupported`] kind (no new variants
/// in 1.x) whose reason names the member and the differences file. Bindings
/// surface the reason verbatim.
pub fn not_supported(class: &str, member: &str) -> crate::Error {
    crate::Error::Unsupported(format!(
        "not supported in this version: {class}.{member}; see KNOWN_DIFFERENCES"
    ))
}
