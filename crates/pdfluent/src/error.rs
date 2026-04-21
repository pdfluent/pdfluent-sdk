//! Error types for `pdfluent`.
//!
//! One [`Error`] enum covers all public operations. The enum is
//! `#[non_exhaustive]` to permit additional variants in minor releases
//! without breaking match exhaustiveness in downstream code.
//!
//! Each variant carries:
//! - a stable [`code`](Error::code) string of the form `E-<CATEGORY>-<SPECIFIC>`,
//! - a deep-linked [`docs_url`](Error::docs_url) to
//!   `https://pdfluent.com/errors/<code>`,
//! - a human-readable message via [`Display`].
//!
//! See RFC 0001 §5 for the full contract.

use std::path::PathBuf;

use crate::capability::Capability;
use crate::compliance::{PdfAProfile, Violation};
use crate::tier::Tier;

/// Unified result type for the `pdfluent` crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type for all `pdfluent` operations.
///
/// Variants are grouped by category; the discriminant [`Error::code`] is
/// stable across `1.x` releases.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    // ---------- I/O ----------
    /// Underlying I/O operation failed.
    Io {
        /// The original `std::io::Error`.
        source: std::io::Error,
        /// Path under operation, if applicable.
        path: Option<PathBuf>,
    },

    /// File not found at the given path.
    FileNotFound {
        /// Path that was searched.
        path: PathBuf,
    },

    // ---------- Parsing ----------
    /// PDF is structurally invalid.
    InvalidPdf {
        /// Byte offset where parsing failed, if known.
        byte_offset: Option<u64>,
        /// Human-readable reason.
        reason: String,
    },

    /// PDF version is newer than the supported maximum.
    UnsupportedPdfVersion {
        /// Version in the document header.
        found: String,
        /// Highest supported by this build.
        supported_up_to: String,
    },

    // ---------- Compliance ----------
    /// PDF/A validation failed against the requested profile.
    PdfaValidationFailed {
        /// Profile under validation.
        profile: PdfAProfile,
        /// All detected violations.
        violations: Vec<Violation>,
    },

    // ---------- Security ----------
    /// Decryption failed — wrong password or unsupported algorithm.
    DecryptionFailed {
        /// Specific failure cause.
        reason: DecryptionFailureReason,
    },

    /// A digital signature in the document is invalid.
    InvalidSignature {
        /// Form field name holding the signature.
        field: String,
        /// Reason the signature is invalid.
        reason: String,
    },

    // ---------- Licensing ----------
    /// Required capability is not available in the current tier.
    FeatureNotInTier {
        /// Capability that was requested.
        capability: Capability,
        /// The tier the user currently holds.
        current_tier: Tier,
        /// The minimum tier required for this capability.
        required_tier: Tier,
        /// Deep link to the error docs page.
        docs_url: &'static str,
        /// Deep link to the pricing page for upgrade.
        upgrade_url: &'static str,
    },

    /// Capability is gated behind a Cargo feature that is not enabled at
    /// compile time.
    CapabilityNotCompiled {
        /// Capability that was requested.
        capability: Capability,
        /// Cargo feature flag to enable.
        feature_flag: &'static str,
        /// Docs URL.
        docs_url: &'static str,
    },

    /// License key is malformed or expired.
    InvalidLicense {
        /// Human-readable reason.
        reason: String,
        /// Docs URL.
        docs_url: &'static str,
    },

    // ---------- Environment ----------
    /// Operation is not supported in WebAssembly builds.
    UnsupportedOnWasm {
        /// Name of the operation that was attempted.
        operation: &'static str,
        /// Docs URL.
        docs_url: &'static str,
    },

    /// A native dependency is required but not installed or discoverable.
    MissingDependency {
        /// Name of the missing dependency.
        dep: &'static str,
        /// Installation hint.
        install_hint: &'static str,
        /// Docs URL.
        docs_url: &'static str,
    },

    // ---------- Budget ----------
    /// Memory budget set via [`crate::OpenOptions::strict_memory_limit`] was
    /// exceeded.
    MemoryBudgetExceeded {
        /// Bytes that would have been allocated.
        requested: usize,
        /// Configured limit.
        limit: usize,
    },

    // ---------- Internal ----------
    /// Internal safety-net. Should never fire under normal operation — if
    /// observed, please report a bug.
    Internal {
        /// Diagnostic message.
        message: String,
        /// Crate version at build time.
        crate_version: &'static str,
    },
}

/// Specific cause of a decryption failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecryptionFailureReason {
    /// Wrong password.
    WrongPassword,
    /// Encryption algorithm not supported.
    UnsupportedAlgorithm,
    /// Encryption dictionary is malformed.
    MalformedDictionary,
}

impl Error {
    /// Stable error code of the form `E-<CATEGORY>-<SPECIFIC>`.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "E-IO-GENERIC",
            Error::FileNotFound { .. } => "E-IO-FILE-NOT-FOUND",
            Error::InvalidPdf { .. } => "E-PARSE-INVALID-PDF",
            Error::UnsupportedPdfVersion { .. } => "E-PARSE-UNSUPPORTED-VERSION",
            Error::PdfaValidationFailed { .. } => "E-COMPLIANCE-PDFA-INVALID",
            Error::DecryptionFailed { .. } => "E-SECURITY-DECRYPTION-FAILED",
            Error::InvalidSignature { .. } => "E-SECURITY-INVALID-SIGNATURE",
            Error::FeatureNotInTier { .. } => "E-LICENSE-FEATURE-NOT-IN-TIER",
            Error::CapabilityNotCompiled { .. } => "E-LICENSE-CAPABILITY-NOT-COMPILED",
            Error::InvalidLicense { .. } => "E-LICENSE-INVALID",
            Error::UnsupportedOnWasm { .. } => "E-ENV-UNSUPPORTED-ON-WASM",
            Error::MissingDependency { .. } => "E-ENV-MISSING-DEPENDENCY",
            Error::MemoryBudgetExceeded { .. } => "E-BUDGET-MEMORY-EXCEEDED",
            Error::Internal { .. } => "E-INTERNAL",
        }
    }

    /// Deep link to the documentation page for this error code.
    pub fn docs_url(&self) -> String {
        format!("https://pdfluent.com/errors/{}", self.code())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io { source, path } => match path {
                Some(p) => write!(f, "I/O error on {}: {source}", p.display()),
                None => write!(f, "I/O error: {source}"),
            },
            Error::FileNotFound { path } => write!(f, "File not found: {}", path.display()),
            Error::InvalidPdf { byte_offset, reason } => match byte_offset {
                Some(o) => write!(f, "Invalid PDF at byte {o}: {reason}"),
                None => write!(f, "Invalid PDF: {reason}"),
            },
            Error::UnsupportedPdfVersion { found, supported_up_to } => write!(
                f,
                "Unsupported PDF version {found} (this build supports up to {supported_up_to})"
            ),
            Error::PdfaValidationFailed { profile, violations } => write!(
                f,
                "PDF/A validation failed for profile {profile:?} with {} violation(s)",
                violations.len()
            ),
            Error::DecryptionFailed { reason } => write!(f, "Decryption failed: {reason:?}"),
            Error::InvalidSignature { field, reason } => {
                write!(f, "Signature '{field}' is invalid: {reason}")
            }
            Error::FeatureNotInTier {
                capability,
                current_tier,
                required_tier,
                docs_url,
                upgrade_url,
            } => write!(
                f,
                "Capability {capability:?} requires tier {required_tier:?}; current tier is {current_tier:?}.\n  Upgrade: {upgrade_url}\n  Docs: {docs_url}"
            ),
            Error::CapabilityNotCompiled {
                capability,
                feature_flag,
                docs_url,
            } => write!(
                f,
                "Capability {capability:?} requires the `{feature_flag}` Cargo feature, which is not enabled in this build.\n  Docs: {docs_url}"
            ),
            Error::InvalidLicense { reason, docs_url } => {
                write!(f, "Invalid license: {reason}\n  Docs: {docs_url}")
            }
            Error::UnsupportedOnWasm { operation, docs_url } => write!(
                f,
                "Operation `{operation}` is not supported on wasm32 targets.\n  Docs: {docs_url}"
            ),
            Error::MissingDependency {
                dep,
                install_hint,
                docs_url,
            } => write!(
                f,
                "Missing dependency: {dep}.\n  Install: {install_hint}\n  Docs: {docs_url}"
            ),
            Error::MemoryBudgetExceeded { requested, limit } => write!(
                f,
                "Memory budget exceeded: requested {requested} bytes, limit is {limit}"
            ),
            Error::Internal { message, crate_version } => write!(
                f,
                "Internal error (please report): {message} [pdfluent {crate_version}]"
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
