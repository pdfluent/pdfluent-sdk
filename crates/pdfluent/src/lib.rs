//! # PDFluent — Pure-Rust PDF SDK
//!
//! `pdfluent` is the public API for the PDFluent SDK. It provides a single,
//! coherent entry point for PDF manipulation: loading, editing, rendering,
//! merging, signing, encryption, redaction, PDF/A compliance, and more.
//!
//! ## Quick start
//!
//! ```no_run
//! use pdfluent::prelude::*;
//!
//! fn main() -> Result<()> {
//!     let mut doc = PdfDocument::open("invoice.pdf")?;
//!     doc.metadata_mut()
//!         .set_title("Processed Invoice")
//!         .commit()?;
//!     {
//!         let mut form = doc.form_mut();
//!         form.set_text("invoice_number", "INV-2026-0042")?
//!             .set_checkbox("paid", true)?;
//!     }
//!     doc.compress(CompressOptions::strict())?;
//!     doc.save("invoice-processed.pdf")?;
//!     Ok(())
//! }
//! ```
//!
//! ## Licensing
//!
//! There is no licence key, no tier and no activation call. PDFluent is
//! published under the AGPLv3 with a commercial licence as the alternative,
//! and a commercial licensee's rights come from a signed order form rather
//! than from a check in the binary. Nothing here reads a key, refuses a
//! capability or marks output. See `docs/licensing.md`.
//!
//! ## Design foundation
//!
//! The public API is frozen per RFC 0001 (see `docs/rfc/0001-sdk-core-api.md`).
//! Breaking changes require a new RFC.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Async I/O wrappers via Tokio's blocking pool.
/// Layout-aware text replacement engine (find & replace by match id).
pub use pdf_manip::text_edit;
/// Embedding a caller-supplied Unicode font, so a replacement can contain
/// scripts the document itself never had. See
/// [`text_edit::FontFallback::EmbedUnicode`].
#[cfg(feature = "font-subset")]
pub use pdf_manip::unicode_font;

#[cfg(feature = "async-tokio")]
pub mod async_io;
pub mod compliance;

/// PDF/A profiles and validation reports — an alias for [`compliance`].
///
/// `pdfa` is the name our own documentation uses and the better one for what
/// this holds; `compliance` stays because it is what the module has been called
/// since 1.0 and removing it would break every caller. See
/// `docs/API_CONTRACT.md`.
///
/// The verb lives on the document: [`document::PdfDocument::validate_pdfa`] and
/// [`document::PdfDocument::convert_to_pdfa`]. This module holds the nouns.
pub mod pdfa {
    pub use crate::compliance::*;
}

/// Optical character recognition.
///
/// The trait and the backends live in `pdf-engine`, which the facade already
/// depends on; this is the name they were missing here.
///
/// **The facade wires up no backend.** That is deliberate (decision of
/// 19-08-2026): a plain `pdfluent` dependency must not drag in system libraries
/// or download models. Bring your own [`ocr::OcrBackend`], or enable one of
/// `pdf-engine`'s features and use the backend it exposes.
pub mod ocr {
    pub use pdf_engine::ocr::{OcrBackend, OcrError, OcrResult, OcrWord};
}

/// Annotation types.
///
/// [`document::PdfDocument::annotations`] already returned these; they had no
/// name in the facade, so an example could not spell out what it got back.
pub mod annotation {
    pub use crate::structure::AnnotationInfo;
}

pub mod decoration;
pub mod diagnostics;
pub mod document;
pub mod encrypt;
pub mod error;
pub mod form;
pub mod merger;
pub mod metadata;
mod page_labels;
pub mod parity;
pub mod prelude;
pub mod redact;
pub mod signer;
pub mod structure;
#[cfg(feature = "pdfa")]
pub mod tagged;
pub mod watermark;
pub mod xfa;

// ---------------------------------------------------------------------------
// Top-level re-exports (public API surface)
// ---------------------------------------------------------------------------

pub use crate::compliance::{PdfAProfile, PdfAValidationReport, Violation};
pub use crate::decoration::PageDecoration;
pub use crate::diagnostics::{Diagnostic, DiagnosticCategory, LeniencyReport, Severity};
pub use crate::document::{
    OpenOptions, Page, Pages, PdfDocument, PdfVersion, SaveOptions, TextBlock,
};
pub use crate::encrypt::{EncryptOptions, EncryptionAlgorithm, Permissions};
pub use crate::error::{Error, ResourceLimitKind, Result};
pub use crate::form::{FieldType, FormField, PdfFormMut};
pub use crate::merger::{BookmarkMergeStrategy, MergeOptions, PdfMerger};
pub use crate::metadata::{Metadata, MetadataMut};
pub use crate::parity::{
    CompressOptions, CompressReport, FontSubsetReport, ImageFormat, ImageInsert, ImageInsertReport,
    InsertImageFormat, ToImagesOptions, ToImagesReport,
};
pub use crate::redact::RedactOptions;
pub use crate::signer::{
    PadesProfile, PdfSigner, Pkcs12Signer, SignOptions, SignatureInfo, SignatureStatus,
    SignatureValidation, SignatureValidationReport,
};
pub use crate::watermark::{Layer, Position, Rotation, WatermarkOptions};
pub use crate::xfa::{
    XfaField, XfaFieldOption, XfaFieldType, XfaFieldValue, XfaFormModel, XfaRect, XfaSetOutcome,
    XfaWidget,
};
/// Re-export of [`pdf_engine::ProcessingLimits`] for use with
/// [`OpenOptions::with_processing_limits`].
pub use pdf_engine::ProcessingLimits;

/// The SDK version at compile time. Bindings check this at runtime to ensure
/// compatibility with the loaded `pdfluent` dynamic library.
pub const fn api_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
