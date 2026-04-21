//! Digital signature types.

use std::path::Path;

use crate::error::Result;

/// PAdES (PDF Advanced Electronic Signatures) profile level.
///
/// Mapping to ETSI EN 319 142-1:
///
/// - [`BasicSignature`](Self::BasicSignature) — PAdES B-B (basic, no timestamp).
/// - [`Timestamped`](Self::Timestamped) — PAdES B-T (with timestamp).
/// - [`LongTerm`](Self::LongTerm) — PAdES B-LT (with DSS, long-term validation). **Default.**
/// - [`LongTermArchive`](Self::LongTermArchive) — PAdES B-LTA (with archive timestamp).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PadesProfile {
    /// PAdES B-B — basic signature, no timestamp.
    BasicSignature,
    /// PAdES B-T — signature with trusted timestamp.
    Timestamped,
    /// PAdES B-LT — signature with long-term validation data (DSS). **Default.**
    #[default]
    LongTerm,
    /// PAdES B-LTA — signature with archive timestamp for renewal.
    LongTermArchive,
}

/// Options for signing a document.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SignOptions {
    pub(crate) reason: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) contact_info: Option<String>,
    pub(crate) field_name: Option<String>,
    pub(crate) visible_rect: Option<(usize, [f64; 4])>,
    pub(crate) profile: PadesProfile,
}

impl SignOptions {
    /// New options with [`PadesProfile::LongTerm`] as default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Human-readable reason for signing.
    pub fn reason(mut self, v: impl Into<String>) -> Self {
        self.reason = Some(v.into());
        self
    }

    /// Location of signing.
    pub fn location(mut self, v: impl Into<String>) -> Self {
        self.location = Some(v.into());
        self
    }

    /// Signer contact information.
    pub fn contact_info(mut self, v: impl Into<String>) -> Self {
        self.contact_info = Some(v.into());
        self
    }

    /// Signature form field name. Defaults to `Signature1`.
    pub fn field_name(mut self, v: impl Into<String>) -> Self {
        self.field_name = Some(v.into());
        self
    }

    /// Place a visible signature appearance on a specific page.
    pub fn visible_rect(mut self, page: usize, rect: [f64; 4]) -> Self {
        self.visible_rect = Some((page, rect));
        self
    }

    /// Select the PAdES profile.
    pub fn profile(mut self, p: PadesProfile) -> Self {
        self.profile = p;
        self
    }
}

/// Pluggable digital signer.
pub trait PdfSigner: Send + Sync {
    /// Sign a data blob and return the DER-encoded CMS SignedData.
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// DER-encoded certificate chain, leaf first.
    fn certificate_chain(&self) -> &[Vec<u8>];
}

/// A signer backed by a PKCS#12 (`.p12` / `.pfx`) identity.
pub struct Pkcs12Signer {
    inner: pdf_sign::Pkcs12Signer,
}

impl std::fmt::Debug for Pkcs12Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkcs12Signer").finish_non_exhaustive()
    }
}

impl Pkcs12Signer {
    /// Load a PKCS#12 identity from a file on disk.
    pub fn from_pfx_file<P: AsRef<Path>>(path: P, password: &str) -> Result<Self> {
        let path_ref = path.as_ref();
        let bytes = std::fs::read(path_ref).map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => crate::Error::FileNotFound {
                path: path_ref.to_path_buf(),
            },
            _ => crate::Error::Io {
                source,
                path: Some(path_ref.to_path_buf()),
            },
        })?;
        Self::from_pfx_bytes(&bytes, password)
    }

    /// Load a PKCS#12 identity from bytes.
    pub fn from_pfx_bytes(bytes: &[u8], password: &str) -> Result<Self> {
        let inner = pdf_sign::Pkcs12Signer::from_pkcs12(bytes, password).map_err(|e| {
            crate::Error::InvalidSignature {
                field: "<signer-load>".into(),
                reason: format!("{e:?}"),
            }
        })?;
        Ok(Self { inner })
    }
}

impl PdfSigner for Pkcs12Signer {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        use pdf_sign::PdfSigner as InnerSigner;
        self.inner
            .sign(data)
            .map_err(|e| crate::Error::InvalidSignature {
                field: "<signing>".into(),
                reason: format!("{e:?}"),
            })
    }
    fn certificate_chain(&self) -> &[Vec<u8>] {
        use pdf_sign::PdfSigner as InnerSigner;
        self.inner.certificate_chain_der()
    }
}

// ---------------------------------------------------------------------------
// Read-only vs validated types
// ---------------------------------------------------------------------------

/// Lightweight metadata about a signature present in the document.
///
/// `SignatureInfo` carries **no validation result**. Use
/// [`crate::PdfDocument::verify_signatures`] for cryptographic validation.
#[derive(Debug, Clone)]
pub struct SignatureInfo {
    /// Form field name holding the signature.
    pub field_name: String,
    /// Human-readable signer name (from certificate CN).
    pub signer_name: String,
    /// Signing timestamp (ISO 8601) if present.
    pub timestamp: Option<String>,
    /// PAdES profile declared by the signature, if determinable from `/SubFilter`.
    pub profile: Option<PadesProfile>,
}

/// A signature with its validation result.
#[derive(Debug, Clone)]
pub struct SignatureValidation {
    /// Metadata about the signature.
    pub info: SignatureInfo,
    /// Validation status.
    pub status: SignatureStatus,
}

/// Signature validation status.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SignatureStatus {
    /// Cryptographically valid and certificate chain trusted.
    Valid,
    /// Cryptographically invalid.
    Invalid {
        /// Reason.
        reason: String,
    },
    /// Validation could not be completed (e.g. chain unknown).
    Unknown {
        /// Reason.
        reason: String,
    },
}

/// Aggregate report produced by
/// [`crate::PdfDocument::verify_signatures`].
#[derive(Debug, Clone, Default)]
pub struct SignatureValidationReport {
    validations: Vec<SignatureValidation>,
}

impl SignatureValidationReport {
    /// Crate-private constructor — used by `PdfDocument::verify_signatures`
    /// to wrap the underlying `pdf_sign::ValidationResult` list after
    /// conversion to our public types.
    pub(crate) fn from_validations(validations: Vec<SignatureValidation>) -> Self {
        Self { validations }
    }

    /// All validations in the report.
    pub fn validations(&self) -> &[SignatureValidation] {
        &self.validations
    }

    /// `true` if the document contains at least one signature.
    ///
    /// Use this to distinguish "unsigned document" from "signed document
    /// with all signatures valid" — both produce [`all_valid`](Self::all_valid) `== true`.
    pub fn is_signed(&self) -> bool {
        !self.validations.is_empty()
    }

    /// `true` when every signature in the document has
    /// [`SignatureStatus::Valid`].
    ///
    /// **Vacuous semantics:** returns `true` for a document without any
    /// signatures ("no signature fails validation"). Combine with
    /// [`is_signed`](Self::is_signed) if presence is required:
    ///
    /// ```
    /// # use pdfluent::SignatureValidationReport;
    /// # fn check(report: &SignatureValidationReport) -> bool {
    /// report.is_signed() && report.all_valid()
    /// # }
    /// ```
    pub fn all_valid(&self) -> bool {
        self.validations
            .iter()
            .all(|v| matches!(v.status, SignatureStatus::Valid))
    }

    /// Signatures that are not [`SignatureStatus::Valid`].
    pub fn failures(&self) -> Vec<&SignatureValidation> {
        self.validations
            .iter()
            .filter(|v| !matches!(v.status, SignatureStatus::Valid))
            .collect()
    }
}
