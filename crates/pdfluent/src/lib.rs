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
//!     doc.add_watermark(
//!         "PAID",
//!         WatermarkOptions::centered().rotated(45.0).opacity(0.3),
//!     )?;
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
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod capability;
pub mod compliance;
pub mod document;
pub mod encrypt;
pub mod error;
pub mod form;
pub mod license;
pub mod merger;
pub mod metadata;
pub mod prelude;
pub mod redact;
pub mod signer;
pub mod tier;
pub mod watermark;

// ---------------------------------------------------------------------------
// Top-level re-exports (public API surface)
// ---------------------------------------------------------------------------

pub use crate::capability::{Capability, CapabilitySet};
pub use crate::compliance::{PdfAProfile, PdfAValidationReport, Violation};
pub use crate::document::{
    OpenOptions, Page, Pages, PagesMut, PdfDocument, PdfVersion, SaveOptions, TextBlock,
};
pub use crate::encrypt::{EncryptOptions, EncryptionAlgorithm, Permissions};
pub use crate::error::{Error, Result};
pub use crate::form::{FieldType, FormField, PdfFormMut};
pub use crate::license::{license_info, set_license_key, LicenseInfo};
pub use crate::merger::{BookmarkMergeStrategy, MergeOptions, PdfMerger};
pub use crate::metadata::{Metadata, MetadataMut};
pub use crate::redact::RedactOptions;
pub use crate::signer::{
    PadesProfile, PdfSigner, Pkcs12Signer, SignOptions, SignatureInfo, SignatureStatus,
    SignatureValidation, SignatureValidationReport,
};
pub use crate::tier::Tier;
pub use crate::watermark::{Layer, Position, Rotation, WatermarkOptions};

/// The SDK version at compile time. Bindings check this at runtime to ensure
/// compatibility with the loaded `pdfluent` dynamic library.
pub const fn api_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
