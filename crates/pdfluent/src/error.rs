//! Error types for `pdfluent`.
//!
//! One [`Error`] enum covers all public operations. The enum is
//! `#[non_exhaustive]` to permit additional variants in minor releases
//! without breaking match exhaustiveness.
//!
//! Each variant carries:
//! - a stable [`code`](Error::code) string of the form `E-<CATEGORY>-<SPECIFIC>`,
//! - a deep-linked [`docs_url`](Error::docs_url) to
//!   `https://pdfluent.com/errors/<code>`,
//! - a human-readable message via the [`std::fmt::Display`] implementation.
//!
//! See RFC 0001 §5 for the full contract.

use std::path::PathBuf;

use crate::capability::Capability;
use crate::compliance::{PdfAProfile, Violation};
use crate::tier::Tier;

/// Unified result type for the `pdfluent` crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type for all `pdfluent` operations.
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
    /// A digital signature is invalid.
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
        /// The minimum tier required.
        required_tier: Tier,
    },
    /// Capability is gated behind a Cargo feature that is not compiled in.
    CapabilityNotCompiled {
        /// Capability that was requested.
        capability: Capability,
        /// Cargo feature flag to enable.
        feature_flag: &'static str,
    },
    /// License key is malformed or expired.
    InvalidLicense {
        /// Human-readable reason.
        reason: String,
    },

    // ---------- Environment ----------
    /// Operation is not supported in WebAssembly builds.
    UnsupportedOnWasm {
        /// Name of the attempted operation.
        operation: &'static str,
    },
    /// A native dependency is required but not installed or discoverable.
    MissingDependency {
        /// Name of the missing dependency.
        dep: &'static str,
        /// Installation hint.
        install_hint: &'static str,
    },

    // ---------- Budget ----------
    /// Memory budget set via [`crate::OpenOptions::strict_memory_limit`] exceeded.
    MemoryBudgetExceeded {
        /// Bytes that would have been allocated.
        requested: usize,
        /// Configured limit.
        limit: usize,
    },

    // ---------- Internal ----------
    /// Internal safety-net. Should never fire under normal operation.
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

impl std::fmt::Display for DecryptionFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongPassword => f.write_str("wrong password"),
            Self::UnsupportedAlgorithm => f.write_str("unsupported encryption algorithm"),
            Self::MalformedDictionary => f.write_str("malformed encryption dictionary"),
        }
    }
}

impl Error {
    /// Stable error code (`E-<CATEGORY>-<SPECIFIC>`), frozen per snapshot test.
    pub const fn code(&self) -> &'static str {
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

    /// Static deep-link to the documentation page for this error code.
    pub const fn docs_url(&self) -> &'static str {
        match self {
            Error::Io { .. } => "https://pdfluent.com/errors/E-IO-GENERIC",
            Error::FileNotFound { .. } => "https://pdfluent.com/errors/E-IO-FILE-NOT-FOUND",
            Error::InvalidPdf { .. } => "https://pdfluent.com/errors/E-PARSE-INVALID-PDF",
            Error::UnsupportedPdfVersion { .. } => {
                "https://pdfluent.com/errors/E-PARSE-UNSUPPORTED-VERSION"
            }
            Error::PdfaValidationFailed { .. } => {
                "https://pdfluent.com/errors/E-COMPLIANCE-PDFA-INVALID"
            }
            Error::DecryptionFailed { .. } => {
                "https://pdfluent.com/errors/E-SECURITY-DECRYPTION-FAILED"
            }
            Error::InvalidSignature { .. } => {
                "https://pdfluent.com/errors/E-SECURITY-INVALID-SIGNATURE"
            }
            Error::FeatureNotInTier { .. } => {
                "https://pdfluent.com/errors/E-LICENSE-FEATURE-NOT-IN-TIER"
            }
            Error::CapabilityNotCompiled { .. } => {
                "https://pdfluent.com/errors/E-LICENSE-CAPABILITY-NOT-COMPILED"
            }
            Error::InvalidLicense { .. } => "https://pdfluent.com/errors/E-LICENSE-INVALID",
            Error::UnsupportedOnWasm { .. } => {
                "https://pdfluent.com/errors/E-ENV-UNSUPPORTED-ON-WASM"
            }
            Error::MissingDependency { .. } => {
                "https://pdfluent.com/errors/E-ENV-MISSING-DEPENDENCY"
            }
            Error::MemoryBudgetExceeded { .. } => {
                "https://pdfluent.com/errors/E-BUDGET-MEMORY-EXCEEDED"
            }
            Error::Internal { .. } => "https://pdfluent.com/errors/E-INTERNAL",
        }
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
            Error::DecryptionFailed { reason } => write!(f, "Decryption failed: {reason}"),
            Error::InvalidSignature { field, reason } => {
                write!(f, "Signature '{field}' is invalid: {reason}")
            }
            Error::FeatureNotInTier {
                capability,
                current_tier,
                required_tier,
            } => write!(
                f,
                "Capability {capability:?} requires tier {required_tier:?}; current tier is {current_tier:?}.\n  Upgrade: https://pdfluent.com/pricing\n  Docs: {}",
                self.docs_url()
            ),
            Error::CapabilityNotCompiled {
                capability,
                feature_flag,
            } => write!(
                f,
                "Capability {capability:?} requires the `{feature_flag}` Cargo feature, which is not enabled in this build.\n  Docs: {}",
                self.docs_url()
            ),
            Error::InvalidLicense { reason } => {
                write!(f, "Invalid license: {reason}\n  Docs: {}", self.docs_url())
            }
            Error::UnsupportedOnWasm { operation } => write!(
                f,
                "Operation `{operation}` is not supported on wasm32 targets.\n  Docs: {}",
                self.docs_url()
            ),
            Error::MissingDependency {
                dep,
                install_hint,
            } => write!(
                f,
                "Missing dependency: {dep}.\n  Install: {install_hint}\n  Docs: {}",
                self.docs_url()
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

// ---------------------------------------------------------------------------
// Internal helpers (pub(crate) — not part of the public API)
// ---------------------------------------------------------------------------

/// Build an [`Error::Internal`] with the given message and the current
/// crate version. Used for runtime invariant checks that should never fire
/// under normal operation (e.g. out-of-range page index after bounds
/// validation).
pub(crate) fn internal_error(message: impl Into<String>) -> Error {
    Error::Internal {
        message: message.into(),
        crate_version: env!("CARGO_PKG_VERSION"),
    }
}

// ---------------------------------------------------------------------------
// From<internal error> conversions
// ---------------------------------------------------------------------------
//
// These impls replace the earlier ad-hoc `map_*_error` helpers in
// `document.rs` and siblings. With `From` impls in place, call-sites can
// use the `?` operator directly instead of `.map_err(map_engine_error)?`.
//
// The impls are at the Rust item-level public (an `impl From<X> for Y`
// block has no visibility modifier), but since the internal error types
// (`pdf_engine::EngineError`, `lopdf::Error`, `pdf_manip::ManipError`,
// `pdf_sign::SignError`, `pdf_redact::RedactError`) are not re-exported
// from the `pdfluent` public surface, end users of the `pdfluent` crate
// never encounter them: the leak is theoretical only.
//
// Every conversion preserves a short textual reason but **never** wraps
// the internal error as `source()` — that would expose the internal type
// through `std::error::Error::source`. Peek at the `source()` impl above
// to confirm: only `Error::Io { source, .. }` chains, and its source is
// `std::io::Error` which is public std.

impl From<pdf_engine::EngineError> for Error {
    fn from(e: pdf_engine::EngineError) -> Self {
        use pdf_engine::EngineError as E;
        match e {
            E::Encrypted(_reason) => Error::DecryptionFailed {
                reason: DecryptionFailureReason::WrongPassword,
            },
            E::InvalidPdf(reason) => Error::InvalidPdf {
                byte_offset: None,
                reason,
            },
            other => Error::InvalidPdf {
                byte_offset: None,
                reason: format!("{other:?}"),
            },
        }
    }
}

impl From<lopdf::Error> for Error {
    fn from(e: lopdf::Error) -> Self {
        Error::InvalidPdf {
            byte_offset: None,
            reason: e.to_string(),
        }
    }
}

impl From<pdf_manip::ManipError> for Error {
    fn from(e: pdf_manip::ManipError) -> Self {
        use pdf_manip::ManipError as M;
        match e {
            M::DecryptionFailed => Error::DecryptionFailed {
                reason: DecryptionFailureReason::WrongPassword,
            },
            other => Error::InvalidPdf {
                byte_offset: None,
                reason: other.to_string(),
            },
        }
    }
}

impl From<pdf_sign::SignError> for Error {
    fn from(e: pdf_sign::SignError) -> Self {
        use pdf_sign::SignError as S;
        match e {
            S::Pkcs12Load(reason)
            | S::UnsupportedKeyType(reason)
            | S::CmsBuild(reason)
            | S::SigningFailed(reason) => Error::InvalidSignature {
                field: "<signing>".into(),
                reason,
            },
            S::NoPrivateKey => Error::InvalidSignature {
                field: "<signing>".into(),
                reason: "PKCS#12 identity contained no private key".into(),
            },
            S::NoCertificate => Error::InvalidSignature {
                field: "<signing>".into(),
                reason: "PKCS#12 identity contained no certificate".into(),
            },
        }
    }
}

impl From<pdf_redact::RedactError> for Error {
    fn from(e: pdf_redact::RedactError) -> Self {
        Error::InvalidPdf {
            byte_offset: None,
            reason: e.to_string(),
        }
    }
}
