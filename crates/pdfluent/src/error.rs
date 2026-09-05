//! Error types for `pdfluent`.
//!
//! One [`Error`] enum covers all public operations. The enum is
//! `#[non_exhaustive]` to permit additional variants in minor releases
//! without breaking match exhaustiveness.
//!
//! Each variant carries:
//! - a stable [`code`](Error::code) string of the form `E-<CATEGORY>-<SPECIFIC>`,
//! - a deep-linked [`docs_url`](Error::docs_url) to
//!   `https://pdfluent.com/errors#<code>`,
//! - a human-readable message via the [`std::fmt::Display`] implementation.
//!
//! See RFC 0001 §5 for the full contract.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
    /// License key is well-formed and signed, but its `expires_at` is in
    /// the past.
    ///
    /// Surfaced by the signed-payload pathway only — mock `tier:X` keys
    /// have no expiry and never produce this variant.
    LicenseExpired {
        /// Unix timestamp from the payload's `expires_at` field.
        expires_at: u64,
    },
    /// License key is structurally a signed JSON payload but the Ed25519
    /// signature does not verify against the configured public key.
    ///
    /// Indicates either a tampered payload or a payload signed by a
    /// different private key. Treat as a hard failure — never fall
    /// through to Trial.
    LicenseInvalidSignature,
    /// A license-enforced rate or usage limit was exceeded at runtime.
    ///
    /// Returned by `LicenseGuard::record_*` calls during operation; not
    /// an activation-time error. Carries the metered resource, used
    /// value, and configured cap.
    LicenseRateLimited {
        /// Metered resource name, e.g. `"api_calls"` or `"pages"`.
        resource: String,
        /// How many units have been consumed in the current window.
        used: u64,
        /// The license's hard cap for this resource in the current window.
        limit: u64,
    },

    // ---------- Environment ----------
    /// Operation is not supported in WebAssembly builds.
    UnsupportedOnWasm {
        /// Name of the attempted operation.
        operation: &'static str,
    },
    /// A text-edit transaction failed (see [`crate::text_edit`] for the
    /// typed per-edit errors this message summarizes).
    TextEditFailed {
        /// Human-readable failure description from the engine.
        reason: String,
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
    /// A configured [`ProcessingLimits`](pdf_engine::ProcessingLimits)
    /// resource cap was exceeded while loading or processing the
    /// document.
    ///
    /// Returned when the caller has set a limits object via
    /// [`crate::OpenOptions::with_processing_limits`] and the input
    /// breaches one of those caps. The `kind` field discriminates which
    /// cap fired so callers can tell a "file too large" rejection from
    /// e.g. an "image too large" rejection without parsing the message.
    ResourceLimitExceeded {
        /// Which resource cap fired.
        kind: ResourceLimitKind,
        /// Observed value (size in bytes / pixel count / depth, by kind).
        observed: u64,
        /// Configured limit (same units as `observed`).
        limit: u64,
    },

    // ---------- Unsupported ----------
    /// The requested operation or parameter is unsupported.
    Unsupported(String),

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

/// Discriminator for a [`Error::ResourceLimitExceeded`] error.
///
/// Each variant maps onto one of the caps declared by
/// [`pdf_engine::ProcessingLimits`]. The variants are deliberately
/// stable across 1.x — a caller can branch on `kind` without parsing
/// human-readable messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceLimitKind {
    /// PDF file size exceeded
    /// [`ProcessingLimits::max_file_bytes`](pdf_engine::ProcessingLimits::max_file_bytes).
    FileTooLarge,
    /// A decompressed stream exceeded
    /// [`ProcessingLimits::max_stream_bytes`](pdf_engine::ProcessingLimits::max_stream_bytes).
    StreamTooLarge,
    /// An image XObject exceeded
    /// [`ProcessingLimits::max_image_pixels`](pdf_engine::ProcessingLimits::max_image_pixels).
    ImageTooLarge,
    /// Indirect-reference depth exceeded
    /// [`ProcessingLimits::max_object_depth`](pdf_engine::ProcessingLimits::max_object_depth).
    ObjectDepthExceeded,
    /// Content-stream operator count exceeded
    /// [`ProcessingLimits::max_operator_count`](pdf_engine::ProcessingLimits::max_operator_count).
    TooManyOperators,
    /// XFA template nesting exceeded
    /// [`ProcessingLimits::max_xfa_nesting_depth`](pdf_engine::ProcessingLimits::max_xfa_nesting_depth).
    XfaNestingTooDeep,
    /// FormCalc recursion exceeded
    /// [`ProcessingLimits::max_formcalc_depth`](pdf_engine::ProcessingLimits::max_formcalc_depth).
    FormCalcRecursionTooDeep,
}

impl std::fmt::Display for ResourceLimitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileTooLarge => f.write_str("file too large"),
            Self::StreamTooLarge => f.write_str("decompressed stream too large"),
            Self::ImageTooLarge => f.write_str("image too large (pixel count)"),
            Self::ObjectDepthExceeded => f.write_str("object reference depth exceeded"),
            Self::TooManyOperators => f.write_str("content stream operator count exceeded"),
            Self::XfaNestingTooDeep => f.write_str("XFA template nesting too deep"),
            Self::FormCalcRecursionTooDeep => f.write_str("FormCalc recursion too deep"),
        }
    }
}

impl From<pdf_engine::LimitError> for Error {
    fn from(e: pdf_engine::LimitError) -> Self {
        use pdf_engine::LimitError as LE;
        let (kind, observed, limit) = match e {
            LE::FileTooLarge {
                actual_bytes,
                limit_bytes,
            } => (ResourceLimitKind::FileTooLarge, actual_bytes, limit_bytes),
            LE::StreamTooLarge {
                actual_bytes,
                limit_bytes,
            } => (ResourceLimitKind::StreamTooLarge, actual_bytes, limit_bytes),
            LE::ImageTooLarge {
                pixels,
                limit_pixels,
                ..
            } => (ResourceLimitKind::ImageTooLarge, pixels, limit_pixels),
            LE::ObjectDepthExceeded { depth, limit } => (
                ResourceLimitKind::ObjectDepthExceeded,
                depth as u64,
                limit as u64,
            ),
            LE::TooManyOperators { count, limit } => {
                (ResourceLimitKind::TooManyOperators, count, limit)
            }
            LE::XfaNestingTooDeep { depth, limit } => (
                ResourceLimitKind::XfaNestingTooDeep,
                depth as u64,
                limit as u64,
            ),
            LE::FormCalcRecursionTooDeep { depth, limit } => (
                ResourceLimitKind::FormCalcRecursionTooDeep,
                depth as u64,
                limit as u64,
            ),
        };
        Error::ResourceLimitExceeded {
            kind,
            observed,
            limit,
        }
    }
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
    ///
    /// # Append-only policy
    ///
    /// Codes are **frozen** once assigned. You may:
    /// - Add a new variant with a new code.
    ///
    /// You must **never**:
    /// - Remove a code.
    /// - Rename an existing code.
    /// - Reassign a code to a different variant.
    ///
    /// Violating this policy breaks any consumer that stores or compares codes
    /// (logs, analytics, downstream SDKs, client-side switch statements).
    /// See `scripts/release/error_catalogue_sync.sh` for the CI gate.
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
            Error::LicenseExpired { .. } => "E-LICENSE-EXPIRED",
            Error::LicenseInvalidSignature => "E-LICENSE-INVALID-SIGNATURE",
            Error::LicenseRateLimited { .. } => "E-LICENSE-RATE-LIMITED",
            Error::TextEditFailed { .. } => "E-EDIT-TEXT-FAILED",
            Error::UnsupportedOnWasm { .. } => "E-ENV-UNSUPPORTED-ON-WASM",
            Error::MissingDependency { .. } => "E-ENV-MISSING-DEPENDENCY",
            Error::MemoryBudgetExceeded { .. } => "E-BUDGET-MEMORY-EXCEEDED",
            Error::ResourceLimitExceeded { .. } => "E-BUDGET-RESOURCE-LIMIT",
            Error::Unsupported(_) => "E-UNSUPPORTED",
            Error::Internal { .. } => "E-INTERNAL",
        }
    }

    /// Static deep-link to the documentation page for this error code.
    pub const fn docs_url(&self) -> &'static str {
        match self {
            Error::Io { .. } => "https://pdfluent.com/errors#E-IO-GENERIC",
            Error::FileNotFound { .. } => "https://pdfluent.com/errors#E-IO-FILE-NOT-FOUND",
            Error::InvalidPdf { .. } => "https://pdfluent.com/errors#E-PARSE-INVALID-PDF",
            Error::UnsupportedPdfVersion { .. } => {
                "https://pdfluent.com/errors#E-PARSE-UNSUPPORTED-VERSION"
            }
            Error::PdfaValidationFailed { .. } => {
                "https://pdfluent.com/errors#E-COMPLIANCE-PDFA-INVALID"
            }
            Error::DecryptionFailed { .. } => {
                "https://pdfluent.com/errors#E-SECURITY-DECRYPTION-FAILED"
            }
            Error::InvalidSignature { .. } => {
                "https://pdfluent.com/errors#E-SECURITY-INVALID-SIGNATURE"
            }
            Error::FeatureNotInTier { .. } => {
                "https://pdfluent.com/errors#E-LICENSE-FEATURE-NOT-IN-TIER"
            }
            Error::CapabilityNotCompiled { .. } => {
                "https://pdfluent.com/errors#E-LICENSE-CAPABILITY-NOT-COMPILED"
            }
            Error::InvalidLicense { .. } => "https://pdfluent.com/errors#E-LICENSE-INVALID",
            Error::LicenseExpired { .. } => "https://pdfluent.com/errors#E-LICENSE-EXPIRED",
            Error::LicenseInvalidSignature => {
                "https://pdfluent.com/errors#E-LICENSE-INVALID-SIGNATURE"
            }
            Error::LicenseRateLimited { .. } => {
                "https://pdfluent.com/errors#E-LICENSE-RATE-LIMITED"
            }
            Error::TextEditFailed { .. } => "https://pdfluent.com/errors#E-EDIT-TEXT-FAILED",
            Error::UnsupportedOnWasm { .. } => {
                "https://pdfluent.com/errors#E-ENV-UNSUPPORTED-ON-WASM"
            }
            Error::MissingDependency { .. } => {
                "https://pdfluent.com/errors#E-ENV-MISSING-DEPENDENCY"
            }
            Error::MemoryBudgetExceeded { .. } => {
                "https://pdfluent.com/errors#E-BUDGET-MEMORY-EXCEEDED"
            }
            Error::ResourceLimitExceeded { .. } => {
                "https://pdfluent.com/errors#E-BUDGET-RESOURCE-LIMIT"
            }
            Error::Unsupported(_) => "https://pdfluent.com/errors#E-UNSUPPORTED",
            Error::Internal { .. } => "https://pdfluent.com/errors#E-INTERNAL",
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
            } => {
                // On Trial the useful next step is a free evaluation key, not a
                // price list. Sending an evaluating developer to /pricing when
                // they can have the feature working in a minute for nothing is
                // the most expensive sentence in the funnel. Above Trial the
                // key already exists, so the price list is the right pointer.
                let next_step = if *current_tier == Tier::Trial {
                    "  Free 30-day evaluation key, no card: https://pdfluent.com/sdk\n  Pricing: https://pdfluent.com/pricing"
                } else {
                    "  Upgrade: https://pdfluent.com/pricing"
                };
                write!(
                    f,
                    "Capability {capability:?} requires tier {required_tier:?}; current tier is {current_tier:?}.\n{next_step}\n  Docs: {}",
                    self.docs_url()
                )
            }
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
            Error::LicenseExpired { expires_at } => write!(
                f,
                "License expired at unix timestamp {expires_at}.\n  Renew: https://pdfluent.com/pricing\n  Docs: {}",
                self.docs_url()
            ),
            Error::LicenseInvalidSignature => write!(
                f,
                "License signature does not verify against the configured public key — tampered or wrong-key payload.\n  Docs: {}",
                self.docs_url()
            ),
            Error::LicenseRateLimited {
                resource,
                used,
                limit,
            } => write!(
                f,
                "Rate limit exceeded: {used}/{limit} {resource} in the current window.\n  Upgrade or wait for window reset.\n  Docs: {}",
                self.docs_url()
            ),
            Error::TextEditFailed { reason } => {
                write!(f, "Text edit failed: {reason}")
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
            Error::ResourceLimitExceeded {
                kind,
                observed,
                limit,
            } => write!(
                f,
                "Resource limit exceeded: {kind} (observed {observed}, limit {limit}).\n  Docs: {}",
                self.docs_url()
            ),
            Error::Unsupported(reason) => write!(f, "Unsupported operation: {reason}"),
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
            // #1467: LimitExceeded surfaces as ResourceLimitExceeded via the
            // existing From<LimitError> impl — the full chain is now closed.
            E::LimitExceeded(le) => Error::from(le),
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    /// A tier refusal must always name a way out.
    ///
    /// This is the promise the bindings inherit: the message says what is not
    /// available *and* where to get a key. Two bindings had quietly broken it
    /// by paraphrasing this text into their own sentence -- Python dropped the
    /// link entirely, Node never carried one -- so the caller learned only
    /// that they were blocked. Assert it here, at the source, because that is
    /// the one place all five bindings read from.
    #[test]
    fn a_tier_refusal_always_names_a_route_to_a_key() {
        for (current, required, expect) in [
            (Tier::Trial, Tier::Business, "https://pdfluent.com/sdk"),
            (
                Tier::Developer,
                Tier::Business,
                "https://pdfluent.com/pricing",
            ),
            (Tier::Team, Tier::Enterprise, "https://pdfluent.com/pricing"),
        ] {
            let text = Error::FeatureNotInTier {
                capability: Capability::DocxExport,
                current_tier: current,
                required_tier: required,
            }
            .to_string();
            assert!(
                text.contains(expect),
                "{current:?} -> {required:?} does not point at {expect}: {text}"
            );
        }
    }

    /// On Trial the route must be the free key, not the price list. Someone
    /// evaluating the SDK has not decided to buy yet; sending them to pricing
    /// asks for a decision they cannot make and hides the thing that would let
    /// them try it.
    #[test]
    fn trial_is_offered_a_free_key_rather_than_a_price_list() {
        let text = Error::FeatureNotInTier {
            capability: Capability::DocxExport,
            current_tier: Tier::Trial,
            required_tier: Tier::Business,
        }
        .to_string();
        assert!(text.contains("evaluation"), "no free-key offer: {text}");
    }

    use super::*;
    use std::collections::HashSet;

    /// Every Error variant must produce a unique stable code string.
    ///
    /// This test is the freeze-gate for RFC 0001 §5: if two variants share a
    /// code the catalogue is broken by definition.
    #[test]
    fn error_codes_are_unique() {
        use std::path::PathBuf;

        // One representative instance per variant.
        let variants: Vec<Error> = vec![
            Error::Io {
                source: std::io::Error::other("test"),
                path: None,
            },
            Error::FileNotFound {
                path: PathBuf::from("/tmp/test.pdf"),
            },
            Error::InvalidPdf {
                byte_offset: None,
                reason: "test".into(),
            },
            Error::UnsupportedPdfVersion {
                found: "2.1".into(),
                supported_up_to: "2.0".into(),
            },
            Error::PdfaValidationFailed {
                profile: crate::compliance::PdfAProfile::A1b,
                violations: vec![],
            },
            Error::DecryptionFailed {
                reason: DecryptionFailureReason::WrongPassword,
            },
            Error::InvalidSignature {
                field: "sig1".into(),
                reason: "bad cert".into(),
            },
            Error::FeatureNotInTier {
                capability: crate::capability::Capability::XfaFlatten,
                current_tier: crate::tier::Tier::Trial,
                required_tier: crate::tier::Tier::Developer,
            },
            Error::CapabilityNotCompiled {
                capability: crate::capability::Capability::XfaFlatten,
                feature_flag: "xfa",
            },
            Error::InvalidLicense {
                reason: "expired".into(),
            },
            Error::LicenseExpired {
                expires_at: 1_700_000_000,
            },
            Error::LicenseInvalidSignature,
            Error::LicenseRateLimited {
                resource: "api_calls".into(),
                used: 1100,
                limit: 1000,
            },
            Error::UnsupportedOnWasm { operation: "sign" },
            Error::MissingDependency {
                dep: "pdfium",
                install_hint: "see README",
            },
            Error::MemoryBudgetExceeded {
                requested: 1024,
                limit: 512,
            },
            Error::ResourceLimitExceeded {
                kind: ResourceLimitKind::FileTooLarge,
                observed: 2000,
                limit: 1000,
            },
            Error::Unsupported("test".into()),
            Error::Internal {
                message: "test".into(),
                crate_version: "0.0.0",
            },
        ];

        let mut seen: HashSet<&'static str> = HashSet::new();
        for v in &variants {
            let code = v.code();
            assert!(seen.insert(code), "Duplicate error code detected: {code}");
        }

        // Confirm every variant is covered (count guard).
        assert_eq!(
            variants.len(),
            19,
            "Update this test when new Error variants are added"
        );
    }

    /// docs_url must be consistent with code() — both must use the same slug.
    #[test]
    fn docs_url_matches_code() {
        use std::path::PathBuf;

        let sample = Error::FileNotFound {
            path: PathBuf::from("/tmp/x.pdf"),
        };
        let code = sample.code();
        let url = sample.docs_url();
        assert!(
            url.ends_with(code),
            "docs_url {url:?} must end with code {code:?}"
        );
    }
}
