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
//! ## Design foundation
//!
//! The public API is frozen per RFC 0001 (see `docs/rfc/0001-sdk-core-api.md`
//! in the repository). Breaking changes require a new RFC.
//!
//! - Sync default; async opt-in via the `async-tokio` feature under
//!   [`pdfluent::r#async`](r#async).
//! - Capability-gated: each licensed capability is enforced at runtime with a
//!   documented [`Error::FeatureNotInTier`] variant when unavailable.
//! - One error type: [`Error`]. No `anyhow`, no `Box<dyn Error>` in the public
//!   signature.
//!
//! ## Modules
//!
//! - [`document`] — [`PdfDocument`] and lifecycle types.
//! - [`merger`]   — [`PdfMerger`] factory builder for combining documents.
//! - [`signer`]   — [`PdfSigner`] trait and [`Pkcs12Signer`] for PAdES signing.
//! - [`form`]     — form field reading and mutation.
//! - [`metadata`] — document metadata (Info dict + XMP).
//! - [`encrypt`]  — AES-256 encryption and permissions.
//! - [`watermark`] — text/image watermarks.
//! - [`redact`]    — content redaction.
//! - [`compliance`] — PDF/A validation and conversion.
//! - [`capability`] — [`Capability`] enum.
//! - [`tier`] — [`Tier`] enum and runtime license.
//! - [`error`] — [`Error`] enum.
//! - [`prelude`] — convenient re-export of the top types.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod capability;
pub mod compliance;
pub mod document;
pub mod encrypt;
pub mod error;
pub mod form;
pub mod merger;
pub mod metadata;
pub mod prelude;
pub mod redact;
pub mod signer;
pub mod tier;
pub mod watermark;

#[cfg(feature = "async-tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "async-tokio")))]
pub mod r#async;

// ---------------------------------------------------------------------------
// Top-level re-exports (public API surface)
// ---------------------------------------------------------------------------

pub use crate::capability::{Capability, CapabilitySet};
pub use crate::compliance::{PdfAProfile, PdfAValidationReport, Violation};
pub use crate::document::{OpenOptions, PdfDocument, PdfDocumentBuilder, Page, Pages, PagesMut, SaveOptions};
pub use crate::encrypt::{
    EncryptOptions, EncryptionAlgorithm, Permissions, PermissionsBuilder,
};
pub use crate::error::{Error, Result};
pub use crate::form::{FieldType, FormField, PdfFormMut};
pub use crate::merger::{BookmarkMergeStrategy, MergeOptions, PdfMerger};
pub use crate::metadata::{Metadata, MetadataMut};
pub use crate::redact::RedactOptions;
pub use crate::signer::{
    PadesProfile, PdfSigner, Pkcs12Signer, SignOptions, Signature, SignatureValidationReport,
};
pub use crate::tier::Tier;
pub use crate::watermark::{Alignment, Layer, Position, Rotation, WatermarkOptions};

/// The SDK version at compile time. Bindings check this at runtime to ensure
/// compatibility with the loaded `pdfluent` dynamic library.
pub const fn api_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
