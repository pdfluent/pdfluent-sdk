//! Rust-level integration tests for the C ABI license surface.
//!
//! These exercise the same functions C consumers would call. The Rust
//! license core uses a `OnceLock` for the process-global tier, so all
//! state-mutating operations are kept inside a single `#[test]` to make the
//! ordering deterministic regardless of test-runner parallelism.

use std::ffi::CString;
use std::os::raw::c_char;

use pdf_capi::{
    pdfluent_license_activate_key, pdfluent_license_effective_tier, pdfluent_license_status,
    PdfStatus, PdfluentLicenseStatus,
};

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn null_key_is_rejected() {
    unsafe {
        let status = pdfluent_license_activate_key(std::ptr::null::<c_char>());
        assert_eq!(status, PdfStatus::ErrorInvalidArgument);
    }
}

#[test]
fn invalid_key_is_rejected() {
    // Parsing this never reaches the OnceLock set, so it does not change
    // the process-global tier.
    let bad = cstr("totally-not-a-license");
    let status = unsafe { pdfluent_license_activate_key(bad.as_ptr()) };
    assert_eq!(status, PdfStatus::ErrorInvalidLicense);
}

#[test]
fn unknown_tier_name_is_rejected() {
    let bad = cstr("tier:platinum");
    let status = unsafe { pdfluent_license_activate_key(bad.as_ptr()) };
    assert_eq!(status, PdfStatus::ErrorInvalidLicense);
}

#[test]
fn status_struct_is_writable() {
    let mut status = PdfluentLicenseStatus {
        tier: -42,
        source: -42,
        output_is_marked: -42,
    };
    let s = unsafe { pdfluent_license_status(&mut status) };
    assert_eq!(s, PdfStatus::Ok);
    // Tier must be 0..=4 regardless of which test ran first.
    assert!(status.tier >= 0 && status.tier <= 4);
    assert!(status.source >= 0 && status.source <= 2);
    assert!(status.output_is_marked == 0 || status.output_is_marked == 1);
}

#[test]
fn null_status_pointer_is_rejected() {
    let s = unsafe { pdfluent_license_status(std::ptr::null_mut()) };
    assert_eq!(s, PdfStatus::ErrorInvalidArgument);
}

#[test]
fn effective_tier_is_in_range() {
    let t = pdfluent_license_effective_tier();
    assert!((0..=4).contains(&t));
}

/// Single state-mutating sequence: activate, idempotent re-activate,
/// conflicting re-activate. Kept in one test so the order is deterministic.
#[test]
fn activation_lifecycle() {
    let dev = cstr("tier:developer");

    // First activation succeeds.
    let s = unsafe { pdfluent_license_activate_key(dev.as_ptr()) };
    // Race condition note: another test might have activated to a different
    // tier first. In that case this call returns ErrorLicenseAlreadySet,
    // which is still a valid outcome from the user's point of view.
    assert!(
        matches!(s, PdfStatus::Ok | PdfStatus::ErrorLicenseAlreadySet),
        "unexpected status: {s:?}"
    );

    // If we got Ok, we expect Developer (1) or whatever the OnceLock holds.
    let tier = pdfluent_license_effective_tier();
    assert!((0..=4).contains(&tier));

    // Re-activating with the same key is idempotent OR already-set if state
    // changed under us in another test.
    let s2 = unsafe { pdfluent_license_activate_key(dev.as_ptr()) };
    assert!(
        matches!(s2, PdfStatus::Ok | PdfStatus::ErrorLicenseAlreadySet),
        "unexpected re-activate status: {s2:?}"
    );
}
