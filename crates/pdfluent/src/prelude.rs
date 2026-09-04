//! Convenient re-exports for common `pdfluent` usage.
//!
//! `use pdfluent::prelude::*;` brings the top-level types into scope.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub use crate::capability::Capability;
pub use crate::compliance::{PdfAProfile, PdfAValidationReport};
pub use crate::decoration::PageDecoration;
pub use crate::diagnostics::{Diagnostic, DiagnosticCategory, LeniencyReport, Severity};
pub use crate::document::{OpenOptions, PdfDocument, SaveOptions, TextBlock};
pub use crate::encrypt::{EncryptOptions, EncryptionAlgorithm, Permissions};
pub use crate::error::{Error, Result};
pub use crate::form::{FieldType, FormField};
pub use crate::license::{license_info, set_license_key, LicenseInfo};
pub use crate::merger::{BookmarkMergeStrategy, PdfMerger};
pub use crate::metadata::{Metadata, MetadataMut};
pub use crate::parity::{
    CompressOptions, CompressReport, FontSubsetReport, ImageFormat, ImageInsert, ImageInsertReport,
    InsertImageFormat, ToImagesOptions, ToImagesReport,
};
pub use crate::redact::RedactOptions;
pub use crate::signer::{PadesProfile, PdfSigner, Pkcs12Signer, SignOptions};
pub use crate::structure::{AnnotationInfo, Attachment, Outline};
#[cfg(feature = "pdfa")]
pub use crate::tagged::{DocumentStructure, StructureNode};
pub use crate::tier::Tier;
pub use crate::watermark::{Layer, Position, Rotation, WatermarkOptions};
pub use crate::xfa::{XfaField, XfaFieldType, XfaFieldValue, XfaFormModel, XfaSetOutcome};
