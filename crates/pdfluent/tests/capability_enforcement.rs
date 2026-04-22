//! Integration tests for Epic 3 #1226 / #1227 / #1228 — capability + tier
//! enforcement.
//!
//! Verifies that:
//! - Without a license key, the SDK runs in Trial mode (all technical caps
//!   available; output would be marked in save paths).
//! - `set_license_key` parses the 1.0 simple evaluation format.
//! - Capabilities gated to higher tiers return `Error::FeatureNotInTier`
//!   with the correct fields.
//!
//! These tests share process-global state (`OnceLock`) so they don't call
//! `set_license_key` directly — that would make test ordering matter. Instead
//! they exercise the pure parse + capability-set machinery.

// Note: we use fully-qualified paths throughout rather than `use
// pdfluent::prelude::*;` because these tests exercise enum/tier/capability
// types directly without touching PdfDocument, and the prelude glob
// triggers `unused_imports` under -D warnings.

#[test]
fn license_info_defaults_to_trial_without_key() {
    let info = pdfluent::license_info();
    // Regardless of whether another test called set_license_key earlier,
    // we check that license_info returns a coherent LicenseInfo for
    // the current tier.
    assert!(info.capabilities.contains(pdfluent::Capability::PdfParse));
    // Trial marks output; other tiers don't. We don't assert the exact
    // tier here (test ordering) — just that the marked flag matches.
    if info.tier == pdfluent::Tier::Trial {
        assert!(info.output_is_marked);
    } else {
        assert!(!info.output_is_marked);
    }
}

#[test]
fn tier_trial_grants_minimum_surface_only() {
    use pdfluent::Capability as C;
    let caps = pdfluent::Tier::Trial.capabilities();

    // Trial grants minimum evaluation surface
    for cap in [C::PdfParse, C::PdfWrite, C::PageOps, C::TextExtract, C::AcroFormRead, C::PdfaValidate] {
        assert!(caps.contains(cap), "Trial must grant {cap:?}");
    }

    // Trial does NOT grant advanced capabilities
    for cap in [
        C::RenderRaster,
        C::DigitalSignatureSign,
        C::Redaction,
        C::OcrTesseract,
        C::Html2Pdf,
        C::AirGapped,
        C::OemRedistribution,
    ] {
        assert!(!caps.contains(cap), "Trial must not grant {cap:?}");
    }
}

#[test]
fn tier_developer_excludes_signing_but_includes_core() {
    use pdfluent::Capability as C;
    let caps = pdfluent::Tier::Developer.capabilities();
    assert!(caps.contains(C::PdfParse));
    assert!(caps.contains(C::PdfWrite));
    assert!(caps.contains(C::TextExtract));
    assert!(caps.contains(C::EncryptionRead));
    // EncryptionWrite and signing are Team+
    assert!(!caps.contains(C::EncryptionWrite));
    assert!(!caps.contains(C::DigitalSignatureSign));
    // OCR is Business+
    assert!(!caps.contains(C::OcrTesseract));
}

#[test]
fn tier_team_includes_signing_and_pdfa() {
    use pdfluent::Capability as C;
    let caps = pdfluent::Tier::Team.capabilities();
    assert!(caps.contains(C::DigitalSignatureSign));
    assert!(caps.contains(C::DigitalSignatureVerify));
    assert!(caps.contains(C::PdfaValidate));
    assert!(caps.contains(C::Redaction));
    // OCR is Business+
    assert!(!caps.contains(C::OcrTesseract));
}

#[test]
fn tier_enterprise_includes_everything() {
    use pdfluent::Capability as C;
    let caps = pdfluent::Tier::Enterprise.capabilities();
    for cap in [
        C::PdfParse,
        C::DigitalSignatureSign,
        C::OcrTesseract,
        C::Html2Pdf,
        C::AirGapped,
        C::OemRedistribution,
    ] {
        assert!(caps.contains(cap), "Enterprise must grant {cap:?}");
    }
}

#[test]
fn license_key_invalid_format_returns_invalid_license() {
    // Use a format that cannot succeed regardless of whether
    // set_license_key was called earlier.
    let err = pdfluent::set_license_key("not-a-valid-format").unwrap_err();
    assert!(
        matches!(err, pdfluent::Error::InvalidLicense { .. }),
        "expected InvalidLicense, got {err:?}",
    );
}

#[test]
fn license_key_unknown_tier_returns_invalid_license() {
    let err = pdfluent::set_license_key("tier:platinum").unwrap_err();
    assert!(matches!(err, pdfluent::Error::InvalidLicense { .. }));
}

#[test]
fn error_feature_not_in_tier_has_structured_fields() {
    // Construct the error directly (without runtime enforcement since
    // Trial unlocks everything) and verify the Display impl exposes the
    // structured upgrade hint promised by RFC §5.4.
    let err = pdfluent::Error::FeatureNotInTier {
        capability: pdfluent::Capability::AirGapped,
        current_tier: pdfluent::Tier::Developer,
        required_tier: pdfluent::Tier::Enterprise,
    };
    let rendered = format!("{err}");
    assert!(
        rendered.contains("AirGapped"),
        "capability in Display: {rendered}",
    );
    assert!(
        rendered.contains("Developer"),
        "current_tier in Display: {rendered}",
    );
    assert!(
        rendered.contains("Enterprise"),
        "required_tier in Display: {rendered}",
    );
    assert!(
        rendered.contains("pricing") || rendered.contains("Upgrade"),
        "upgrade hint in Display: {rendered}",
    );
    // Error code must be stable.
    assert_eq!(err.code(), "E-LICENSE-FEATURE-NOT-IN-TIER");
}
