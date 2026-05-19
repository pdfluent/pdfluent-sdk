//! C ABI signed-license-payload contract test.
//!
//! Exercises the `pdfluent_license_activate_payload` +
//! `pdfluent_license_set_public_key` externs end-to-end via Rust
//! callers. Equivalent to what a JNI / P/Invoke / Python-ctypes
//! consumer would see at the boundary.
//!
//! Lives in its own integration-test binary so the `OnceLock<[u8;32]>`
//! holding the public key starts unset; the `signing` dev-dep of
//! `pdfluent` is not active here, so xfa-license is consumed via its
//! main API only.
//!
//! Pins, in particular:
//! - `PdfStatus::ErrorLicenseExpired (=19)` for an expired payload.
//! - `PdfStatus::ErrorLicenseInvalidSignature (=20)` for a tampered
//!   payload.
//! - `pdfluent_license_set_public_key(null, 32)` and bad lengths
//!   produce `PdfStatus::ErrorInvalidArgument`.

use std::ffi::CString;
use std::os::raw::c_char;

use pdf_capi::{
    pdfluent_license_activate_key, pdfluent_license_activate_payload,
    pdfluent_license_set_public_key, pdfluent_license_status, PdfStatus, PdfluentLicenseStatus,
};

#[test]
fn pdfstatus_signed_payload_codes_are_pinned() {
    // ABI contract: these numeric values are referenced from every
    // native consumer (Java / .NET / Python ctypes) and MUST stay stable.
    assert_eq!(PdfStatus::ErrorLicenseExpired as i32, 19);
    assert_eq!(PdfStatus::ErrorLicenseInvalidSignature as i32, 20);
}

#[test]
fn set_public_key_rejects_null_and_wrong_length() {
    // Null pointer.
    let rc = unsafe { pdfluent_license_set_public_key(std::ptr::null(), 32) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);

    // Wrong length (16 bytes instead of 32).
    let half = [0u8; 16];
    let rc = unsafe { pdfluent_license_set_public_key(half.as_ptr(), 16) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
}

#[test]
fn activate_payload_rejects_null() {
    let rc = unsafe { pdfluent_license_activate_payload(std::ptr::null::<c_char>()) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
}

#[test]
fn activate_payload_without_public_key_returns_invalid_license() {
    // No prior pdfluent_license_set_public_key call in this test binary,
    // so the umbrella's guard fires.
    let stub = CString::new(r#"{"payload":{},"signature":""}"#).unwrap();
    let rc = unsafe { pdfluent_license_activate_payload(stub.as_ptr()) };
    // The umbrella returns Error::InvalidLicense with reason
    // "no public key configured" → C ABI maps to ErrorInvalidLicense.
    assert_eq!(rc, PdfStatus::ErrorInvalidLicense);
}

#[test]
fn status_after_failed_activation_is_unchanged() {
    // Read status before.
    let mut s = PdfluentLicenseStatus {
        tier: -1,
        source: -1,
        output_is_marked: -1,
    };
    let rc = unsafe { pdfluent_license_status(&mut s) };
    assert_eq!(rc, PdfStatus::Ok);
    let tier_before = s.tier;
    let source_before = s.source;

    // Try invalid activation.
    let bad = CString::new("not-a-valid-key").unwrap();
    let _ = unsafe { pdfluent_license_activate_key(bad.as_ptr()) };

    // Status must be unchanged.
    let rc = unsafe { pdfluent_license_status(&mut s) };
    assert_eq!(rc, PdfStatus::Ok);
    assert_eq!(s.tier, tier_before, "tier must NOT change after failed activation");
    assert_eq!(s.source, source_before, "source must NOT change after failed activation");
}
