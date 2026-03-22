//! Opaque handle types and C-compatible enums.

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
    /// An unknown error occurred.
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

    /// Whether this level targets PDF/A part 1 (stricter subset of PDF 1.4).
    pub(crate) fn is_part1(self) -> bool {
        matches!(self, Self::A1b | Self::A1a)
    }
}
