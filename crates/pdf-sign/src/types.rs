//! Core types for signature validation.

/// The status of a signature validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Signature is cryptographically valid.
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
    /// Fully qualified field name.
    pub field_name: String,
    /// Common name of the signer, if available.
    pub signer: Option<String>,
    /// Signing timestamp as a string, if available.
    pub timestamp: Option<String>,
    /// The SubFilter used.
    pub sub_filter: Option<SubFilter>,
}

/// Signature SubFilter values (ISO 32000-2 Table 257).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubFilter {
    /// adbe.pkcs7.detached — PKCS#7 detached signature.
    AdbePkcs7Detached,
    /// adbe.pkcs7.sha1 — PKCS#7 with SHA-1 digest.
    AdbePkcs7Sha1,
    /// adbe.x509.rsa_sha1 — X.509 with RSA-SHA1.
    AdbeX509RsaSha1,
    /// ETSI.CAdES.detached — PAdES baseline.
    EtsiCadesDetached,
    /// ETSI.RFC3161 — RFC 3161 timestamp token.
    EtsiRfc3161,
}

impl SubFilter {
    /// Parse from a PDF name value.
    pub fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"adbe.pkcs7.detached" => Some(Self::AdbePkcs7Detached),
            b"adbe.pkcs7.sha1" => Some(Self::AdbePkcs7Sha1),
            b"adbe.x509.rsa_sha1" => Some(Self::AdbeX509RsaSha1),
            b"ETSI.CAdES.detached" => Some(Self::EtsiCadesDetached),
            b"ETSI.RFC3161" => Some(Self::EtsiRfc3161),
            _ => None,
        }
    }
}

/// Result of byte-range digest verification.
#[derive(Debug)]
pub enum DigestVerification {
    /// Digest matches.
    Ok,
    /// Digest does not match.
    Mismatch,
    /// SubFilter is unsupported.
    Unsupported,
    /// An error occurred during verification.
    Error(String),
}

/// DocMDP permission level (ISO 32000-2 Table 260).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocMdpPermission {
    /// No changes allowed.
    NoChanges = 1,
    /// Form filling and signing allowed.
    FormFillAndSign = 2,
    /// Form filling, signing, and annotation allowed.
    FormFillSignAnnotate = 3,
}

impl DocMdpPermission {
    /// Parse from the /P value in a TransformParams dictionary.
    pub fn from_value(v: u32) -> Self {
        match v {
            1 => Self::NoChanges,
            3 => Self::FormFillSignAnnotate,
            _ => Self::FormFillAndSign, // Default per spec.
        }
    }
}

/// Lock action for FieldMDP (ISO 32000-2 §12.8.4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockAction {
    /// Lock all fields.
    All,
    /// Lock only the listed fields.
    Include(Vec<String>),
    /// Lock all fields except the listed ones.
    Exclude(Vec<String>),
}

/// Signature appearance style.
#[derive(Debug, Clone)]
pub enum SignatureAppearanceStyle {
    /// Standard text-based appearance showing signer name, date, reason.
    Standard,
    /// Description only — just the signer description text.
    Description(String),
}
