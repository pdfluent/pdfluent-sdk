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
//! Provide a license key via any of (highest precedence first):
//!
//! 1. [`OpenOptions::with_license_key`](crate::document::OpenOptions::with_license_key)
//!    — per-document override.
//! 2. [`license::set_license_key`] — process-global.
//! 3. `PDFLUENT_LICENSE_KEY` environment variable.
//!
//! Without a license the SDK runs in [`Tier::Trial`] mode: all capabilities
//! accessible, output marked via `/Producer` metadata.
//!
//! ## Design foundation
//!
//! The public API is frozen per RFC 0001 (see `docs/rfc/0001-sdk-core-api.md`).
//! Breaking changes require a new RFC.

#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Async I/O wrappers via Tokio's blocking pool.
#[cfg(feature = "async-tokio")]
pub mod async_io;
pub mod capability;
pub mod compliance;
pub mod decoration;
pub mod diagnostics;
pub mod document;
pub mod encrypt;
pub mod error;
pub mod form;
pub mod license;
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
pub mod tier;
pub mod watermark;

// ---------------------------------------------------------------------------
// Top-level re-exports (public API surface)
// ---------------------------------------------------------------------------

pub use crate::capability::{Capability, CapabilitySet};
pub use crate::compliance::{PdfAProfile, PdfAValidationReport, Violation};
pub use crate::decoration::PageDecoration;
pub use crate::diagnostics::{Diagnostic, DiagnosticCategory, LeniencyReport, Severity};
pub use crate::document::{
    OpenOptions, Page, Pages, PdfDocument, PdfVersion, SaveOptions, TextBlock,
};
pub use crate::encrypt::{EncryptOptions, EncryptionAlgorithm, Permissions};
pub use crate::error::{Error, ResourceLimitKind, Result};
pub use crate::form::{FieldType, FormField, PdfFormMut};
pub use crate::license::{
    license_info, set_license_key, set_license_payload, set_license_public_key, LicenseInfo,
};
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
pub use crate::tier::Tier;
pub use crate::watermark::{Layer, Position, Rotation, WatermarkOptions};
/// Re-export of [`pdf_engine::ProcessingLimits`] for use with
/// [`OpenOptions::with_processing_limits`].
pub use pdf_engine::ProcessingLimits;

/// The SDK version at compile time. Bindings check this at runtime to ensure
/// compatibility with the loaded `pdfluent` dynamic library.
pub const fn api_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
