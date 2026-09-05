//! Opaque handle types and C-compatible enums.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Opaque document handle. Wraps a `pdf_engine::PdfDocument`.
pub struct PdfDocument(pub(crate) pdf_engine::PdfDocument);

/// Opaque compliance report handle. Wraps a `pdf_compliance::ComplianceReport`.
pub struct PdfComplianceReport(pub(crate) pdf_compliance::ComplianceReport);

/// Status codes returned by all C API functions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfStatus {
    /// Operation succeeded.
    Ok = 0,
    /// A null pointer or otherwise invalid argument was passed.
    ErrorInvalidArgument = 1,
    /// The file could not be found or read.
    ErrorFileNotFound = 2,
    /// The password is incorrect.
    ErrorInvalidPassword = 3,
    /// The PDF data is corrupt or unparseable.
    ErrorCorruptPdf = 4,
    /// The page index is out of range.
    ErrorPageRange = 5,
    /// A rendering error occurred.
    ErrorRender = 6,
    /// PDF/A conversion failed.
    ErrorConvert = 7,
    /// Redaction failed.
    ErrorRedact = 8,
    /// Document signing failed.
    ErrorSign = 9,
    /// An annotation operation failed.
    ErrorAnnotation = 10,
    /// A document merge operation failed.
    ErrorMerge = 11,
    /// An image or content extraction failed.
    ErrorExtract = 12,
    /// A document split operation failed.
    ErrorSplit = 13,
    /// A watermark operation failed.
    ErrorWatermark = 14,
    /// A compression/optimization operation failed.
    ErrorCompress = 15,
    /// A license key is malformed or names an unknown tier.
    ErrorInvalidLicense = 16,
    /// A license is already set to a different tier in this process.
    /// Restart the process to switch tiers.
    ErrorLicenseAlreadySet = 17,
    /// A license file could not be read from disk.
    ErrorLicenseFile = 18,
    /// A signed license payload has expired (its `expires_at` is in the past).
    ErrorLicenseExpired = 19,
    /// A signed license payload's Ed25519 signature does not verify against
    /// the configured public key (tampered or wrong-key payload).
    ErrorLicenseInvalidSignature = 20,
    /// An unknown error occurred.
    /// A text-edit operation failed (stale match id, unsupported container,
    /// encoding failure, signed document refused, …). The detail is available
    /// via `pdf_last_error_message`.
    ErrorTextEdit = 21,
    /// The licence is valid but its tier does not include the requested
    /// capability — Office export on a tier below Business, for instance.
    ///
    /// Distinct from `ErrorInvalidLicense` (16) on purpose: "your key is bad"
    /// and "your plan does not cover this" send a caller to different places,
    /// and collapsing them into one code costs a support conversation every
    /// time. Added 23-08-2026 with the Office exports; appended rather than
    /// inserted so every existing number keeps its meaning.
    ErrorCapabilityNotLicensed = 22,
    ErrorUnknown = 99,
}

/// PDF/A conformance level for the C API.
///
/// Maps to `pdf_compliance::PdfALevel`. Values are stable across versions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfALevel {
    /// PDF/A-1B — basic conformance (most common archival target).
    A1b = 0,
    /// PDF/A-1A — accessible conformance (requires tagged PDF).
    A1a = 1,
    /// PDF/A-2B — ISO 19005-2 basic.
    A2b = 2,
    /// PDF/A-2U — ISO 19005-2 with Unicode mapping.
    A2u = 3,
    /// PDF/A-2A — ISO 19005-2 accessible.
    A2a = 4,
    /// PDF/A-3B — ISO 19005-3 basic (allows embedded files).
    A3b = 5,
    /// PDF/A-3U — ISO 19005-3 with Unicode mapping.
    A3u = 6,
    /// PDF/A-3A — ISO 19005-3 accessible.
    A3a = 7,
    /// PDF/A-4 — ISO 19005-4 base level.
    A4 = 8,
    /// PDF/A-4F — allows file attachments.
    A4f = 9,
    /// PDF/A-4E — allows engineering content (3D, rich media).
    A4e = 10,
}

impl PdfALevel {
    /// Convert to the internal `pdf_compliance::PdfALevel` representation.
    pub(crate) fn to_compliance_level(self) -> pdf_compliance::PdfALevel {
        match self {
            Self::A1b => pdf_compliance::PdfALevel::A1b,
            Self::A1a => pdf_compliance::PdfALevel::A1a,
            Self::A2b => pdf_compliance::PdfALevel::A2b,
            Self::A2u => pdf_compliance::PdfALevel::A2u,
            Self::A2a => pdf_compliance::PdfALevel::A2a,
            Self::A3b => pdf_compliance::PdfALevel::A3b,
            Self::A3u => pdf_compliance::PdfALevel::A3u,
            Self::A3a => pdf_compliance::PdfALevel::A3a,
            Self::A4 => pdf_compliance::PdfALevel::A4,
            Self::A4f => pdf_compliance::PdfALevel::A4f,
            Self::A4e => pdf_compliance::PdfALevel::A4e,
        }
    }

    /// The conformance the conversion pipeline should write.
    ///
    /// The pipeline writes B-level XMP for every part. The A and U levels add
    /// requirements (a tagged structure tree, complete Unicode mapping) that
    /// conversion cannot manufacture from an untagged source, so claiming them
    /// in the metadata would produce a file that fails validation for the level
    /// it advertises. Part 4 has no separate B level and maps to A3b, the
    /// closest profile the pipeline can actually produce.
    pub(crate) fn to_convert_conformance(self) -> pdf_manip::pdfa_xmp::PdfAConformance {
        use pdf_manip::pdfa_xmp::PdfAConformance;
        match self {
            Self::A1b | Self::A1a => PdfAConformance::A1b,
            Self::A2b | Self::A2u | Self::A2a => PdfAConformance::A2b,
            Self::A3b | Self::A3u | Self::A3a => PdfAConformance::A3b,
            Self::A4 | Self::A4f | Self::A4e => PdfAConformance::A3b,
        }
    }
}
