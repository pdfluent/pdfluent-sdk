//! Digital signature types.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::Path;

use crate::error::Result;

/// PAdES (PDF Advanced Electronic Signatures) profile level.
///
/// Mapping to ETSI EN 319 142-1:
///
/// - [`BasicSignature`](Self::BasicSignature) — PAdES B-B (basic, no timestamp). **Default.**
/// - [`Timestamped`](Self::Timestamped) — PAdES B-T. Needs a timestamp over
///   the signature, so it is signed through
///   [`PdfDocument::sign_timestamped`](crate::PdfDocument::sign_timestamped).
/// - [`LongTerm`](Self::LongTerm) — PAdES B-LT. The same, plus a DSS built
///   from the signer's own chain and the revocation evidence given to
///   [`SignOptions::revocation_data`].
/// - [`LongTermArchive`](Self::LongTermArchive) — PAdES B-LTA. **Not
///   implemented.** Asking for it is an error, see the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PadesProfile {
    /// PAdES B-B — basic signature, no timestamp. **Default.**
    ///
    /// The default is what the signer actually produces. It used to be
    /// LongTerm, which mapped to the same code path and produced B-B anyway —
    /// so callers asking for long-term validity got a basic signature and no
    /// warning. A signature that claims more than it carries is worse than one
    /// that claims less.
    #[default]
    BasicSignature,
    /// PAdES B-T — signature with a trusted timestamp.
    ///
    /// The timestamp has to cover the signature, which does not exist until
    /// signing is done, so this profile is reached through
    /// [`PdfDocument::sign_timestamped`](crate::PdfDocument::sign_timestamped):
    /// it signs, hands the signature to your authority, and embeds what comes
    /// back. Without a token, signing fails rather than falling back to B-B.
    Timestamped,
    /// PAdES B-LT — timestamp plus long-term validation data (DSS).
    ///
    /// Signed the same way as [`Timestamped`](Self::Timestamped), and the DSS
    /// carries the signer's certificate chain plus the revocation evidence
    /// given to [`SignOptions::revocation_data`]. Both halves are the point of
    /// the profile: a verifier offline in five years needs the chain *and* the
    /// proof it was not revoked, so this profile is refused without the second.
    LongTerm,
    /// PAdES B-LTA — B-LT plus an archive timestamp for renewal.
    ///
    /// **Not implemented.** B-LTA needs a document timestamp signature
    /// (`/Type /DocTimeStamp` with SubFilter `ETSI.RFC3161`) written over the
    /// whole file, and nothing here writes one. Asking for it returns an error;
    /// it is listed so the error can name what is missing rather than the
    /// variant quietly not existing.
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
    pub(crate) timestamp_token: Option<Vec<u8>>,
    pub(crate) ocsp_responses: Vec<Vec<u8>>,
    pub(crate) crls: Vec<Vec<u8>>,
}

impl SignOptions {
    /// New options with [`PadesProfile::BasicSignature`] as default.
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

    /// Revocation evidence for the DSS, DER-encoded.
    ///
    /// PAdES B-LT is the profile that lets a verifier check the signature years
    /// later without going back on the network, and that takes more than the
    /// certificate chain: it takes the evidence that those certificates were
    /// not revoked at signing time. Fetching it is the caller's, because the
    /// engine makes no network call while signing.
    ///
    /// Include what covers the signer's chain, and the timestamp authority's
    /// chain as well if you have it -- a verifier offline in five years checks
    /// both. [`PadesProfile::LongTerm`] is refused without any of it, rather
    /// than writing a DSS that names the profile and cannot support it.
    pub fn revocation_data(mut self, ocsp_responses: Vec<Vec<u8>>, crls: Vec<Vec<u8>>) -> Self {
        self.ocsp_responses = ocsp_responses;
        self.crls = crls;
        self
    }

    /// Select the PAdES profile.
    ///
    /// [`PadesProfile::Timestamped`] and [`PadesProfile::LongTerm`] have to be
    /// signed through
    /// [`PdfDocument::sign_timestamped`](crate::PdfDocument::sign_timestamped),
    /// which fetches the token over the signature it just made.
    pub fn profile(mut self, p: PadesProfile) -> Self {
        self.profile = p;
        self
    }

    /// Supply a DER-encoded RFC 3161 timestamp token you already hold.
    ///
    /// **Only a token over this document's signature will be accepted**, and a
    /// token obtained before signing cannot be one: the signature does not
    /// exist yet, so its digest cannot be in the token's `messageImprint`.
    /// [`PdfDocument::sign`](crate::PdfDocument::sign) refuses a token that
    /// stamps other bytes rather than writing a B-T claim that no verifier will
    /// accept (#515).
    ///
    /// So this method is for the case where the token genuinely is about this
    /// signature -- one carried across a process boundary, say. To fetch a
    /// token in the ordinary way, use
    /// [`PdfDocument::sign_timestamped`](crate::PdfDocument::sign_timestamped),
    /// which sequences the two correctly. The engine deliberately has no HTTP
    /// client of its own (that is what keeps document content off the network),
    /// so talking to the authority stays the caller's job either way.
    ///
    /// Ignored for [`PadesProfile::BasicSignature`], which carries no
    /// timestamp by definition.
    pub fn timestamp_token(mut self, der: impl Into<Vec<u8>>) -> Self {
        self.timestamp_token = Some(der.into());
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
