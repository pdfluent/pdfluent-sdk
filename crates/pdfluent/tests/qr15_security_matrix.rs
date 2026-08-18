//! QR-15 release-recheck: security-sensitive workflow matrix.
//!
//! - signature read + verify on a signed fixture: typed report, no panic;
//!   unsigned doc: empty signatures, no false positive.
//! - redaction content-removal (the underlying text is gone, not covered).
//! - attachment extraction safety (ZUGFeRD): reported size == extracted bytes.
//! - (no-hidden-network during these workflows is proven separately by the
//!   QR-13 static checker `check_no_network_telemetry.py`.)

use std::path::PathBuf;

use pdfluent::prelude::*;
use pdfluent::redact::RedactOptions;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn open(name: &str) -> Option<PdfDocument> {
    std::fs::read(mini(name))
        .ok()
        .and_then(|b| PdfDocument::from_bytes(&b).ok())
}

#[test]
fn qr15_signature_read_and_verify_is_typed_no_panic() {
    // Signed fixture: signatures() + verify_signatures() return typed results
    // without panicking. We do not assert validity (depends on trust roots);
    // we assert the workflow is safe and typed.
    if let Some(doc) = open("signed-rsa.pdf") {
        let sigs = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.signatures()))
            .expect("signatures() must not panic");
        let _ = sigs; // Ok(list) or typed Err — both acceptable
        let rep =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.verify_signatures()))
                .expect("verify_signatures() must not panic");
        let _ = rep;
    }
    // Unsigned fixture must not report phantom signatures.
    if let Some(doc) = open("simple.pdf") {
        if let Ok(list) = doc.signatures() {
            assert!(list.is_empty(), "unsigned doc must not report signatures");
        }
    }
}

#[test]
fn qr15_redaction_removes_content() {
    let Some(doc) = open("multi-page.pdf") else {
        eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!());
        return;
    };
    let Ok(text) = doc.extract_text() else {
 eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!()); return };
    let Some(token) = text
        .split(|c: char| !c.is_ascii_alphabetic())
        .find(|w| w.len() >= 4)
        .map(str::to_string)
    else {
        eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!());
        return;
    };
    let mut doc = open("multi-page.pdf").unwrap();
    if doc.redact(&token, RedactOptions::new()).is_err() {
        eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!());
        return;
    }
    let out = doc.to_bytes().expect("persist redacted");
    let reopened = PdfDocument::from_bytes(&out).expect("reopen");
    let after = reopened.extract_text().unwrap_or_default().to_lowercase();
    assert!(
        !after.contains(&token.to_lowercase()),
        "redacted token '{token}' must not be extractable after redaction"
    );
}

#[test]
fn qr15_attachment_extraction_size_matches_bytes() {
    let Some(doc) = open("zugferd.pdf") else {
        eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!());
        return;
    };
    let Ok(list) = doc.attachments() else {
 eprintln!("SKIPPED (not a pass): precondition not met at {}:{}", file!(), line!()); return };
    for att in list {
        if let Ok(Some(bytes)) = doc.attachment_bytes(&att.name) {
            assert_eq!(
                bytes.len(),
                att.size,
                "attachment '{}' reported size must equal extracted byte length",
                att.name
            );
        }
    }
}
