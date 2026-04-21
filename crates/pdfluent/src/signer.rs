//! Digital signature types.

use std::path::Path;

use crate::error::Result;

/// PAdES (PDF Advanced Electronic Signatures) profile level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PadesProfile {
    /// PAdES B-B — basic, no timestamp.
    BB,
    /// PAdES B-T — with timestamp.
    BT,
    /// PAdES B-LT — long-term validation with DSS. Default.
    #[default]
    BLT,
    /// PAdES B-LTA — long-term with archive timestamp.
    BLTA,
}

/// Options for signing a document.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SignOptions {
    pub(crate) reason: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) contact_info: Option<String>,
    pub(crate) field_name: Option<String>,
    pub(crate) visible_rect: Option<(u32, [f64; 4])>,
    pub(crate) profile: PadesProfile,
}

impl SignOptions {
    /// New options with PAdES B-LT default.
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
    pub fn visible_rect(mut self, page: u32, rect: [f64; 4]) -> Self {
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
#[derive(Debug)]
pub struct Pkcs12Signer {
    _inner: (),
}

impl Pkcs12Signer {
    /// Load a PKCS#12 identity from a file on disk.
    pub fn from_pfx_file<P: AsRef<Path>>(_path: P, _password: &str) -> Result<Self> {
        unimplemented!("Epic 2 #1244 wires this against pdf_sign::signer::Pkcs12Signer");
    }

    /// Load a PKCS#12 identity from bytes.
    pub fn from_pfx_bytes(_bytes: &[u8], _password: &str) -> Result<Self> {
        unimplemented!("Epic 2 #1244 wires this against pdf_sign::signer::Pkcs12Signer");
    }
}

impl PdfSigner for Pkcs12Signer {
    fn sign(&self, _data: &[u8]) -> Result<Vec<u8>> {
        unimplemented!("Epic 2 #1244");
    }
    fn certificate_chain(&self) -> &[Vec<u8>] {
        unimplemented!("Epic 2 #1244");
    }
}

/// A digital signature found in a document.
#[derive(Debug, Clone)]
pub struct Signature {
    /// Form field name.
    pub field_name: String,
    /// Human-readable signer name (from CN).
    pub signer_name: String,
    /// Signing timestamp (ISO 8601), if present.
    pub timestamp: Option<String>,
    /// Validation status.
    pub status: SignatureStatus,
}

/// Signature validation status.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SignatureStatus {
    /// Cryptographically valid, chain trusted.
    Valid,
    /// Cryptographically invalid.
    Invalid {
        /// Reason.
        reason: String,
    },
    /// Unknown or not yet validated.
    Unknown {
        /// Reason.
        reason: String,
    },
}

/// Aggregate report of signature validation over a document.
#[derive(Debug, Clone, Default)]
pub struct SignatureValidationReport {
    signatures: Vec<Signature>,
}

impl SignatureValidationReport {
    /// All signatures in the document.
    pub fn signatures(&self) -> &[Signature] {
        &self.signatures
    }

    /// True if every signature is [`SignatureStatus::Valid`].
    pub fn all_valid(&self) -> bool {
        !self.signatures.is_empty()
            && self
                .signatures
                .iter()
                .all(|s| matches!(s.status, SignatureStatus::Valid))
    }

    /// List of signatures that are not [`SignatureStatus::Valid`].
    pub fn failures(&self) -> Vec<&Signature> {
        self.signatures
            .iter()
            .filter(|s| !matches!(s.status, SignatureStatus::Valid))
            .collect()
    }
}
