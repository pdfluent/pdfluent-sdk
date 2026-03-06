//! Opaque handle types and C-compatible enums.

/// Opaque document handle. Wraps a `pdf_engine::PdfDocument`.
pub struct PdfDocument(pub(crate) pdf_engine::PdfDocument);

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
    /// An unknown error occurred.
    ErrorUnknown = 99,
}
