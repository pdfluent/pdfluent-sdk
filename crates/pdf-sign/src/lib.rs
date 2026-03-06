//! PDF digital signature validation and signing.
//!
//! Supports PAdES baseline signatures, CMS, and timestamping
//! per ISO 32000-2 §12.8.

/// The status of a signature validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Signature is cryptographically valid and the certificate chain is trusted.
    Valid,
    /// Signature is invalid (reason provided).
    Invalid(String),
    /// Validity could not be determined (reason provided).
    Unknown(String),
}

/// A signature validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Validation outcome.
    pub status: ValidationStatus,
    /// Common name of the signer, if available.
    pub signer: Option<String>,
    /// Signing timestamp (RFC 3339), if available.
    pub timestamp: Option<String>,
}

/// A reference to a signature field in the document.
#[derive(Debug, Clone)]
pub struct SignatureField {
    /// Fully qualified field name.
    pub name: String,
    /// Byte range of the signed data.
    pub byte_range: Option<[usize; 4]>,
}

/// Trait for external signing implementations.
///
/// Implementors provide the actual cryptographic signing operation
/// (e.g., PKCS#7/CMS, PAdES).
pub trait Signer {
    /// Signs the given data and returns the DER-encoded CMS signature.
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}

/// Core trait for PDF signature validation and signing.
pub trait SignatureValidator {
    /// Validates an existing signature field.
    fn validate_signature(&self, sig: &SignatureField) -> ValidationResult;

    /// Signs the document using the provided signer implementation.
    fn sign_document(&mut self, signer: &dyn Signer) -> Result<(), Box<dyn std::error::Error>>;
}
