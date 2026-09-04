//! WASM license activation tests.
//!
//! Run on wasm32:
//!   wasm-pack test --node           (browserless)
//!   wasm-pack test --headless --chrome
//!
//! On native (default `cargo test`), the wasm-bindgen `JsValue` machinery
//! panics when constructing string errors, so the tests below are gated to
//! `target_arch = "wasm32"`. Native-side coverage for the same Rust logic is
//! provided by the Rust core's own tests in the `pdfluent` crate.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use xfa_wasm::license::{activate_license_key, license_status, LicenseStatus};

#[wasm_bindgen_test]
fn status_shape() {
    let s: LicenseStatus = license_status();
    assert!(matches!(
        s.tier().as_str(),
        "Trial" | "Developer" | "Team" | "Business" | "Enterprise" | "Unknown"
    ));
    assert!(matches!(
        s.source().as_str(),
        "Default" | "EnvVar" | "Explicit"
    ));
}

#[wasm_bindgen_test]
fn invalid_key_throws() {
    assert!(activate_license_key("totally-not-a-license").is_err());
}

#[wasm_bindgen_test]
fn unknown_tier_throws() {
    assert!(activate_license_key("tier:platinum").is_err());
}

#[wasm_bindgen_test]
fn activation_lifecycle() {
    let _ = activate_license_key("tier:developer");
    let s = license_status();
    assert!(!s.tier().is_empty());
    assert!(!s.source().is_empty());
}
