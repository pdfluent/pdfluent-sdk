//! Signed-license contract test — missing public key.
//!
//! Runs in its own integration-test binary so that the `OnceLock`
//! holding the public verification key is guaranteed unset. Tests in
//! `signed_license_lifecycle.rs` (a separate binary) own the
//! "key-is-set" half of the contract.
//!
//! Operator hard rules honoured:
//! - No production private keys in the repo.
//! - No test private keys committed to disk — fixtures are generated
//!   in-memory inside each test.
//! - The public-key state is NOT modified by this test (the file is
//!   reserved for the `set_license_public_key`-not-called case).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::{set_license_payload, Error};

#[test]
fn set_license_payload_without_public_key_returns_invalid_license() {
    // The xfa-license::token::sign_license + generate_keypair APIs are
    // gated behind the `signing` feature, which pdfluent activates via
    // dev-dependencies. We do not even need to call them here: even
    // with a syntactically-valid JSON shape, the umbrella rejects the
    // call because no public key has been injected.
    //
    // The payload below would be invalid anyway (no signature), but
    // the umbrella stops BEFORE attempting verification — this proves
    // the "no public key configured" guard fires first.
    let stub = r#"{"payload":{},"signature":""}"#;
    let err = set_license_payload(stub).expect_err("must fail when no public key set");
    match err {
        Error::InvalidLicense { reason } => {
            assert!(
                reason.contains("no public key configured"),
                "expected 'no public key configured' guard message, got {reason:?}"
            );
            assert_eq!(
                Error::InvalidLicense {
                    reason: String::new()
                }
                .code(),
                "E-LICENSE-INVALID"
            );
        }
        other => panic!("expected Error::InvalidLicense, got {other:?}"),
    }
}
