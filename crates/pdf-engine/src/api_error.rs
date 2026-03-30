//! Top-level error types for the PDFluent SDK.

use std::path::PathBuf;

/// The central Error enum for all PDFluent operations.
/// Designed to provide high context and actionable help for developers.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("File not found: {path}\n\nHelp: Check that the file exists at the specified path.\nDocs: https://docs.pdfluent.dev/errors/file-not-found")]
    FileNotFound { path: PathBuf },

    #[error("This PDF requires a password\n\nHelp: Use pdfluent::read_with(path, |opts| opts.password(\"...\"))\nDocs: https://docs.pdfluent.dev/errors/password-required")]
    PasswordRequired { path: PathBuf },

    #[error("Corrupt PDF: {reason}\n\nHelp: Try opts.repair(true) to attempt automatic repair.\nDocs: https://docs.pdfluent.dev/errors/corrupt-pdf")]
    CorruptPdf { path: Option<PathBuf>, reason: String },

    #[error("Permission Denied: {reason}\n\nHelp: The PDF's permissions do not allow this operation.\nDocs: https://docs.pdfluent.dev/errors/permission-denied")]
    PermissionDenied { reason: String },

    #[error("Invalid Signature: {reason}\n\nHelp: Check the certificate chain and document integrity.\nDocs: https://docs.pdfluent.dev/errors/invalid-signature")]
    InvalidSignature { reason: String },

    #[error("Page Out of Bounds: Requested page {requested}, but document has only {total}\n\nHelp: Use 1-based page indices up to doc.page_count().\nDocs: https://docs.pdfluent.dev/errors/page-out-of-bounds")]
    PageOutOfBounds { requested: usize, total: usize },

    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Unsupported Feature: {feature}\n\nHelp: This feature is not yet supported by the PDFluent SDK.\nDocs: https://docs.pdfluent.dev/errors/unsupported-feature")]
    UnsupportedFeature { feature: String },
}
