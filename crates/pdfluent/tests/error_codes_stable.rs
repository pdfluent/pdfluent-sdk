//! Integration tests for Epic 3 #1229 / #1230 / #1231 — error system.
//!
//! - Stable-code snapshot: every public `Error` variant produces a
//!   canonical `code()` string that is frozen for the life of 1.x.
//! - Docs-url mapping: every variant's `docs_url()` deep-links into the
//!   expected `/errors/<code>` subtree.
//! - From-impl contracts: each interne crate's error type maps to the
//!   agreed public variant without leaking the interne type via
//!   `Error::source()`.

use pdfluent::Error;

// ---------------------------------------------------------------------------
// Stable error codes — snapshot
// ---------------------------------------------------------------------------

/// Every `Error` variant we currently publish, each with:
///   (sample-variant, expected code, docs_url tail)
fn variant_samples() -> Vec<(Error, &'static str)> {
    vec![
        (
            Error::Io {
                source: std::io::Error::other("x"),
                path: None,
            },
            "E-IO-GENERIC",
        ),
        (
            Error::FileNotFound {
                path: std::path::PathBuf::from("/x"),
            },
            "E-IO-FILE-NOT-FOUND",
        ),
        (
            Error::InvalidPdf {
                byte_offset: None,
                reason: "test".into(),
            },
            "E-PARSE-INVALID-PDF",
        ),
        (
            Error::UnsupportedPdfVersion {
                found: "2.0".into(),
                supported_up_to: "1.7".into(),
            },
            "E-PARSE-UNSUPPORTED-VERSION",
        ),
        (
            Error::PdfaValidationFailed {
                profile: pdfluent::PdfAProfile::A2b,
                violations: Vec::new(),
            },
            "E-COMPLIANCE-PDFA-INVALID",
        ),
        (
            Error::DecryptionFailed {
                reason: pdfluent::error::DecryptionFailureReason::WrongPassword,
            },
            "E-SECURITY-DECRYPTION-FAILED",
        ),
        (
            Error::InvalidSignature {
                field: "Signature1".into(),
                reason: "test".into(),
            },
            "E-SECURITY-INVALID-SIGNATURE",
        ),
        (
            Error::FeatureNotInTier {
                capability: pdfluent::Capability::DigitalSignatureSign,
                current_tier: pdfluent::Tier::Developer,
                required_tier: pdfluent::Tier::Team,
            },
            "E-LICENSE-FEATURE-NOT-IN-TIER",
        ),
        (
            Error::CapabilityNotCompiled {
                capability: pdfluent::Capability::Html2Pdf,
                feature_flag: "html-to-pdf",
            },
            "E-LICENSE-CAPABILITY-NOT-COMPILED",
        ),
        (
            Error::InvalidLicense {
                reason: "test".into(),
            },
            "E-LICENSE-INVALID",
        ),
        (
            Error::UnsupportedOnWasm { operation: "x" },
            "E-ENV-UNSUPPORTED-ON-WASM",
        ),
        (
            Error::MissingDependency {
                dep: "x",
                install_hint: "x",
            },
            "E-ENV-MISSING-DEPENDENCY",
        ),
        (
            Error::MemoryBudgetExceeded {
                requested: 0,
                limit: 0,
            },
            "E-BUDGET-MEMORY-EXCEEDED",
        ),
        (
            Error::ResourceLimitExceeded {
                kind: pdfluent::ResourceLimitKind::FileTooLarge,
                observed: 0,
                limit: 0,
            },
            "E-BUDGET-RESOURCE-LIMIT",
        ),
        (
            Error::Internal {
                message: "x".into(),
                crate_version: "test",
            },
            "E-INTERNAL",
        ),
    ]
}

#[test]
fn every_variant_exposes_its_canonical_code() {
    for (err, expected_code) in variant_samples() {
        assert_eq!(
            err.code(),
            expected_code,
            "variant {err:?} must produce code {expected_code}",
        );
    }
}

#[test]
fn error_codes_are_unique() {
    let samples = variant_samples();
    let mut seen = std::collections::HashSet::new();
    for (err, code) in &samples {
        assert!(seen.insert(code), "duplicate code {code} (variant {err:?})",);
    }
}

#[test]
fn error_codes_follow_e_prefix_scheme() {
    for (err, code) in variant_samples() {
        assert!(
            code.starts_with("E-"),
            "code {code} for {err:?} must start with E-",
        );
        assert!(
            code.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'),
            "code {code} must be UPPER-KEBAB-CASE",
        );
    }
}

// ---------------------------------------------------------------------------
// docs_url mapping
// ---------------------------------------------------------------------------

#[test]
fn docs_url_resolves_to_expected_subtree() {
    for (err, code) in variant_samples() {
        let url = err.docs_url();
        assert_eq!(
            url,
            format!("https://pdfluent.com/errors/{code}"),
            "docs_url for {err:?} must deep-link to /errors/{code}",
        );
    }
}

#[test]
fn docs_url_returns_static_str() {
    // `Error::docs_url()` is a `const fn` returning `&'static str`. This
    // test exists to pin that contract: the compiler rejects assigning a
    // non-'static &str to a `&'static str` binding.
    let err = Error::InvalidPdf {
        byte_offset: None,
        reason: "x".into(),
    };
    let _url: &'static str = err.docs_url();
}

// ---------------------------------------------------------------------------
// From<interne> impl contract
// ---------------------------------------------------------------------------
//
// Each interne crate's error type must map to a well-defined public
// `Error` variant. These tests pin the mapping so future refactors of
// the underlying crates can't silently drift the public error surface.

#[test]
fn from_pdf_engine_encrypted_maps_to_decryption_failed() {
    let e = pdf_engine::EngineError::Encrypted("reason".into());
    let mapped: Error = e.into();
    assert!(
        matches!(mapped, Error::DecryptionFailed { .. }),
        "pdf_engine::EngineError::Encrypted must map to Error::DecryptionFailed, got {mapped:?}",
    );
}

#[test]
fn from_pdf_engine_invalid_maps_to_invalid_pdf() {
    let e = pdf_engine::EngineError::InvalidPdf("parse error".into());
    let mapped: Error = e.into();
    assert!(matches!(mapped, Error::InvalidPdf { .. }));
}

#[test]
fn from_lopdf_error_maps_to_invalid_pdf() {
    // lopdf::Error::DictKey is one of the simplest variants to construct.
    let e: lopdf::Error = lopdf::Error::DictKey("test-key".into());
    let mapped: Error = e.into();
    assert!(matches!(mapped, Error::InvalidPdf { .. }));
}

#[test]
fn from_pdf_manip_decryption_failed_maps_to_decryption_failed() {
    let e = pdf_manip::ManipError::DecryptionFailed;
    let mapped: Error = e.into();
    assert!(
        matches!(mapped, Error::DecryptionFailed { .. }),
        "ManipError::DecryptionFailed must map to Error::DecryptionFailed, got {mapped:?}",
    );
}

#[test]
fn from_pdf_manip_other_maps_to_invalid_pdf() {
    let e = pdf_manip::ManipError::Encryption("x".into());
    let mapped: Error = e.into();
    assert!(matches!(mapped, Error::InvalidPdf { .. }));
}

#[test]
fn from_pdf_sign_all_variants_map_to_invalid_signature() {
    use pdf_sign::SignError;
    for e in [
        SignError::Pkcs12Load("x".into()),
        SignError::UnsupportedKeyType("x".into()),
        SignError::CmsBuild("x".into()),
        SignError::SigningFailed("x".into()),
        SignError::NoPrivateKey,
        SignError::NoCertificate,
    ] {
        let mapped: Error = e.into();
        assert!(
            matches!(mapped, Error::InvalidSignature { .. }),
            "pdf_sign::SignError variant must map to InvalidSignature",
        );
    }
}

#[test]
fn from_pdf_redact_maps_to_invalid_pdf() {
    let e = pdf_redact::RedactError::NoAreas;
    let mapped: Error = e.into();
    assert!(matches!(mapped, Error::InvalidPdf { .. }));
}

// ---------------------------------------------------------------------------
// No leak of interne types via Error::source()
// ---------------------------------------------------------------------------

#[test]
fn error_source_does_not_leak_internal_types() {
    // Walk every public variant. The contract is: every variant whose
    // From-impl has just converted from an interne crate's error type
    // must NOT re-expose that interne type via `source()`. Only `Error::Io`
    // chains a source — and that source is `std::io::Error`, a public
    // std type, which is allowed.
    //
    // We use `downcast_ref::<std::io::Error>()` to confirm the Io chain is
    // exactly what we document. Variants constructed via From from
    // pdf_engine/lopdf/pdf_manip/pdf_sign/pdf_redact must have
    // `source() == None` so their interne type never travels through
    // `std::error::Error::source`.

    // Io variants chain to std::io::Error (allowed).
    let io_err = Error::Io {
        source: std::io::Error::other("test"),
        path: None,
    };
    let source = std::error::Error::source(&io_err).expect("Io has a source");
    assert!(
        source.downcast_ref::<std::io::Error>().is_some(),
        "Io variant's source must be std::io::Error (which is allowed)",
    );

    // Every From<interne> conversion must produce a variant with
    // source() == None.
    let engine_err: Error = pdf_engine::EngineError::InvalidPdf("x".into()).into();
    assert!(std::error::Error::source(&engine_err).is_none());

    let lopdf_err: Error = lopdf::Error::DictKey("x".into()).into();
    assert!(std::error::Error::source(&lopdf_err).is_none());

    let manip_err: Error = pdf_manip::ManipError::DecryptionFailed.into();
    assert!(std::error::Error::source(&manip_err).is_none());

    let sign_err: Error = pdf_sign::SignError::NoPrivateKey.into();
    assert!(std::error::Error::source(&sign_err).is_none());

    let redact_err: Error = pdf_redact::RedactError::NoAreas.into();
    assert!(std::error::Error::source(&redact_err).is_none());
}
