//! Behaviour tests for `pdfluent validate`.
//!
//! The legacy parse check, PDF/A verdicts with machine-readable violations,
//! and the PDF/UA facade gap.

// CLI covers: validate --pdfa

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{run, stdout_json, tempdir};

#[test]
fn validate_without_flags_is_a_parse_check() {
    let sample = crate::require_sample!();
    let out = run(&["validate", &sample.to_string_lossy()]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("parses, 1 page(s)"));
}

#[test]
fn validate_pdfa_reports_non_compliance_with_violations() {
    // The shared sample is an ordinary PDF 1.7 file: it cannot be PDF/A.
    let sample = crate::require_sample!();
    let out = run(&[
        "validate",
        &sample.to_string_lossy(),
        "--pdfa",
        "--profile",
        "a2b",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("NOT_COMPLIANT"), "{err}");

    // JSON carries the violations for automation.
    let out = run(&["--json", "validate", &sample.to_string_lossy(), "--pdfa"]);
    assert_eq!(out.status.code(), Some(2));
    let value = stdout_json(&out);
    assert_eq!(value["error"]["code"], "NOT_COMPLIANT");
    assert_eq!(value["data"]["compliant"], false);
    assert_eq!(value["data"]["profile"], "a2b");
    assert!(!value["data"]["violations"].as_array().unwrap().is_empty());
}

#[test]
fn validate_fails_on_corrupt_input() {
    let dir = tempdir("validate-corrupt");
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, common::corrupt_bytes()).unwrap();
    let out = run(&["validate", &bad.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["validate", &bad.to_string_lossy(), "--pdfa"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn validate_reports_pdfua_as_a_facade_gap() {
    let sample = crate::require_sample!();
    let out = run(&["validate", &sample.to_string_lossy(), "--pdfua"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("PDFUA_NOT_IN_FACADE"), "{err}");
    assert!(err.contains("facade gap"), "{err}");
}
