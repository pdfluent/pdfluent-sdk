//! Signed-license contract — full lifecycle E2E in one process.
//!
//! Single integration-test binary so the order of scenarios is
//! deterministic and the `OnceLock<Tier>` semantics of
//! `pdfluent::set_license_payload` are exercised exactly as they would
//! be in a real customer process.
//!
//! Hard rules:
//! - No private keys on disk — keypair generated in-memory once,
//!   used to sign all fixtures.
//! - No silent fallback — every error path is asserted to leave the
//!   process tier unchanged.
//! - Typed `.code` (= `E-LICENSE-…`) asserted on every error scenario.
//!
//! Closes matrix flows F1 (valid signed payload), F2 (tampered),
//! F3 (expired), F8 (already-set, different tier), F9 (invalid /
//! malformed JSON), F13 (no silent fallback after failed activation).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::{
    license_info, set_license_key, set_license_payload, set_license_public_key, Error, Tier,
};
use xfa_license::claims::LicensePayload;
use xfa_license::token::{generate_keypair, sign_license};
use xfa_license::Tier as XfaTier;

/// Future expiry (year 3000) — safely outside any realistic test
/// runtime.
const FUTURE_EXPIRY: u64 = 32_503_680_000;
/// Past expiry (year 2020).
const PAST_EXPIRY: u64 = 1_577_836_800;

fn payload(tier: XfaTier, expires_at: u64) -> LicensePayload {
    LicensePayload {
        licensee: "Test Licensee".into(),
        email: "ci@pdfluent.test".into(),
        company: "PDFluent CI".into(),
        tier,
        seats: 1,
        issued_at: 1_700_000_000,
        expires_at,
        features: Some(vec!["xfa".into(), "pdfa-validation".into()]),
    }
}

fn assert_code(err: &Error, expected: &str) {
    assert_eq!(err.code(), expected, "wrong typed code on error: {err:?}",);
}

#[test]
fn signed_license_lifecycle_e2e() {
    // ─── Setup: generate a fresh test keypair in-memory ────────────────
    let (private_key, public_key) = generate_keypair();
    // Inject the public key — first call. Idempotent re-call should
    // succeed; conflicting re-call would error.
    set_license_public_key(&public_key).expect("first set_license_public_key");
    set_license_public_key(&public_key).expect("idempotent set_license_public_key");

    // Default status BEFORE any activation must be Trial.
    let info0 = license_info();
    assert_eq!(info0.tier, Tier::Trial);
    assert!(info0.expires_at.is_none());
    assert!(info0.features.is_empty());

    // ─── Scenario F2 — tampered signature ──────────────────────────────
    {
        let p = payload(XfaTier::Basic, FUTURE_EXPIRY);
        let signed = sign_license(&private_key, &p).expect("sign valid");
        // Flip one ASCII byte inside the base64 signature. Both the
        // needle and replacement are ASCII so reconstructing the
        // String from bytes is safe.
        let needle = "\"signature\": \"";
        let pos = signed.find(needle).unwrap() + needle.len();
        let mut buf = signed.into_bytes();
        buf[pos] = if buf[pos] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(buf).expect("ASCII-only mutation");
        let err = set_license_payload(&tampered).expect_err("tampered must fail");
        assert_code(&err, "E-LICENSE-INVALID-SIGNATURE");
        // No silent fallback — tier stays Trial.
        assert_eq!(license_info().tier, Tier::Trial);
    }

    // ─── Scenario F3 — expired payload ─────────────────────────────────
    {
        let p = payload(XfaTier::Basic, PAST_EXPIRY);
        let signed = sign_license(&private_key, &p).expect("sign expired");
        let err = set_license_payload(&signed).expect_err("expired must fail");
        match &err {
            Error::LicenseExpired { expires_at } => {
                assert_eq!(*expires_at, PAST_EXPIRY);
            }
            other => panic!("expected LicenseExpired, got {other:?}"),
        }
        assert_code(&err, "E-LICENSE-EXPIRED");
        // No silent fallback.
        assert_eq!(license_info().tier, Tier::Trial);
    }

    // ─── Scenario F9 — malformed JSON ──────────────────────────────────
    {
        let err = set_license_payload("{not json").expect_err("malformed must fail");
        assert_code(&err, "E-LICENSE-INVALID");
        assert_eq!(license_info().tier, Tier::Trial);
    }

    // ─── Scenario F1 — valid signed payload ────────────────────────────
    // Sign a Basic-tier payload. The umbrella maps Basic → Developer per
    // SIGNED_LICENSE_PAYLOAD_ARCHITECTURE.md Q5.
    let valid =
        sign_license(&private_key, &payload(XfaTier::Basic, FUTURE_EXPIRY)).expect("sign valid");
    set_license_payload(&valid).expect("valid payload activates");

    let info = license_info();
    assert_eq!(
        info.tier,
        Tier::Developer,
        "xfa-license Basic → pdfluent Developer mapping"
    );
    assert!(
        info.expires_at
            .as_deref()
            .map(|s| s.starts_with("3000-"))
            .unwrap_or(false),
        "expires_at must be ISO 8601 in year 3000, got {:?}",
        info.expires_at
    );
    assert_eq!(info.licensee.as_deref(), Some("Test Licensee"));
    assert_eq!(info.company.as_deref(), Some("PDFluent CI"));
    assert_eq!(info.features, vec!["xfa", "pdfa-validation"]);
    // output_is_marked must be FALSE on a paid tier.
    assert!(
        !info.output_is_marked,
        "Developer tier must NOT mark output, got {info:?}"
    );

    // ─── Scenario F8 — idempotent re-activation (same tier) ────────────
    set_license_payload(&valid).expect("idempotent re-activate same tier");
    assert_eq!(license_info().tier, Tier::Developer);

    // ─── Scenario F5 — already-set, DIFFERENT tier ─────────────────────
    {
        let enterprise = sign_license(&private_key, &payload(XfaTier::Enterprise, FUTURE_EXPIRY))
            .expect("sign enterprise");
        let err = set_license_payload(&enterprise).expect_err("conflicting tier must fail");
        assert_code(&err, "E-LICENSE-INVALID");
        // Tier stays at Developer.
        assert_eq!(license_info().tier, Tier::Developer);
    }

    // ─── Scenario F4 — set_license_key auto-detects JSON payloads ──────
    // Re-activating the same Developer-tier payload via set_license_key
    // (the legacy entry point) must succeed because we auto-route JSON
    // → set_license_payload internally.
    set_license_key(&valid).expect("auto-detect JSON via set_license_key");
    assert_eq!(license_info().tier, Tier::Developer);
}

#[test]
fn license_info_default_when_no_payload() {
    // Distinct binary case is in signed_license_missing_key.rs.
    // This test in the same binary as the lifecycle test runs AFTER
    // the lifecycle (or before — irrelevant) and only verifies that
    // license_info() never panics + always returns a structurally-
    // valid LicenseInfo regardless of process state.
    let info = license_info();
    // Tier is always a valid variant.
    let _ = matches!(
        info.tier,
        Tier::Trial | Tier::Developer | Tier::Team | Tier::Business | Tier::Enterprise
    );
    // features field is always present (Vec is never null).
    let _ = info.features.len();
    // output_is_marked must be true iff Trial.
    assert_eq!(info.output_is_marked, info.tier == Tier::Trial);
}
