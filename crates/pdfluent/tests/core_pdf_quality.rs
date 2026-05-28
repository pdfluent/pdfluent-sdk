//! CORE_PDF_SDK_ENTERPRISE_QUALITY_100 — targeted non-XFA core tests.
//!
//! These cover gaps not already exercised by the existing suites
//! (`security.rs`, `processing_limits.rs`, `merge.rs`, `lifecycle.rs`,
//! `metadata.rs`): object-stream / xref-stream parse + write roundtrip,
//! and hostile-but-safe malformed inputs that must fail *typed* and
//! *without panic* across the public `PdfDocument` API.
//!
//! No XFA. No network. Read-only use of one committed object-stream
//! fixture (`corpus/fw7.pdf`); all other inputs are synthesised in-test.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use pdfluent::prelude::*;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name)
}

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

/// Asserts the closure does not panic; returns its value.
fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => panic!("{what} PANICKED on hostile/edge input (must fail typed, not panic)"),
    }
}

// ---------------------------------------------------------------------------
// Object-stream / xref-stream support (ObjStm + /Type /XRef)
// ---------------------------------------------------------------------------

#[test]
fn objstream_xrefstream_pdf_loads_and_roundtrips_page_count() {
    // fw7.pdf is a committed fixture whose PDF structure uses object
    // streams (/ObjStm) and a cross-reference stream (/Type /XRef).
    let path = corpus("fw7.pdf");
    let bytes = std::fs::read(&path).expect("read fw7.pdf objstream fixture");

    // Confirm the fixture really is object-stream / xref-stream shaped.
    assert!(
        bytes.windows(7).any(|w| w == b"/ObjStm") || bytes.windows(5).any(|w| w == b"/XRef"),
        "fixture is expected to use ObjStm/XRef streams"
    );

    let doc = no_panic("from_bytes(objstream)", || PdfDocument::from_bytes(&bytes))
        .expect("object-stream PDF must load");
    let n = doc.page_count();
    assert!(n > 0, "object-stream PDF must report a positive page count");

    // Save → reload → page count preserved (write path handles objstream input).
    let out = doc.to_bytes().expect("serialise object-stream-sourced doc");
    let reopened = PdfDocument::from_bytes(&out).expect("reopen serialised doc");
    assert_eq!(
        reopened.page_count(),
        n,
        "page count must be preserved across save/reload of an object-stream PDF"
    );
}

// ---------------------------------------------------------------------------
// Classic xref roundtrip (baseline correctness)
// ---------------------------------------------------------------------------

#[test]
fn classic_xref_roundtrip_preserves_page_count() {
    for name in ["simple.pdf", "multi-page.pdf", "acroform.pdf"] {
        let bytes = std::fs::read(mini(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
        let doc = PdfDocument::from_bytes(&bytes).unwrap_or_else(|e| panic!("open {name}: {e}"));
        let n = doc.page_count();
        assert!(n > 0, "{name}: positive page count");
        let out = doc
            .to_bytes()
            .unwrap_or_else(|e| panic!("save {name}: {e}"));
        let reopened =
            PdfDocument::from_bytes(&out).unwrap_or_else(|e| panic!("reopen {name}: {e}"));
        assert_eq!(reopened.page_count(), n, "{name}: page count preserved");
    }
}

// ---------------------------------------------------------------------------
// Hostile-but-safe malformed inputs — must be typed Err, never panic.
// ---------------------------------------------------------------------------

#[test]
fn empty_bytes_return_typed_error_not_panic() {
    let r = no_panic("from_bytes(empty)", || PdfDocument::from_bytes(b""));
    assert!(r.is_err(), "empty input must be a typed error");
    // It must be a parse/structural error, not a license/internal panic.
    let e = r.unwrap_err();
    assert!(
        matches!(e, Error::InvalidPdf { .. } | Error::Io { .. }),
        "empty-input error must be a parse/IO error, got: {e:?}"
    );
}

#[test]
fn garbage_bytes_return_typed_error_not_panic() {
    let garbage = b"this is definitely not a PDF file, just some bytes \x00\x01\x02\xff";
    let r = no_panic("from_bytes(garbage)", || PdfDocument::from_bytes(garbage));
    assert!(r.is_err(), "garbage input must be a typed error");
}

#[test]
fn truncated_pdf_fails_typed_not_panic() {
    let full = std::fs::read(mini("multi-page.pdf")).expect("read multi-page.pdf");
    // Truncate at several boundaries: header-only, mid-body, pre-xref.
    for frac in [0.02_f64, 0.4, 0.85] {
        let cut = (full.len() as f64 * frac) as usize;
        let truncated = &full[..cut];
        let r = no_panic(&format!("from_bytes(truncated@{frac})"), || {
            PdfDocument::from_bytes(truncated)
        });
        // Either a typed error, or (with xref recovery) a successful open —
        // both are acceptable; the contract is "no panic, no hang". If it
        // opens, page_count must not panic either.
        if let Ok(doc) = r {
            let _ = no_panic("page_count(truncated-recovered)", || doc.page_count());
        }
    }
}

#[test]
fn header_without_body_fails_typed_not_panic() {
    let r = no_panic("from_bytes(header-only)", || {
        PdfDocument::from_bytes(b"%PDF-1.7\n")
    });
    assert!(r.is_err(), "header-only input must be a typed error");
}

#[test]
fn bogus_xref_offset_does_not_panic() {
    // A PDF whose startxref points far past EOF. Must fail typed or recover,
    // never panic.
    let body = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
                 2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
                 startxref\n999999999\n%%EOF\n";
    let r = no_panic("from_bytes(bogus-xref)", || PdfDocument::from_bytes(body));
    if let Ok(doc) = r {
        let _ = no_panic("page_count(bogus-xref)", || doc.page_count());
    }
}

// ---------------------------------------------------------------------------
// Decrypt / wrong-password typed behaviour on a real encrypted fixture
// (complements security.rs which builds encrypted docs in-memory).
// ---------------------------------------------------------------------------

#[test]
fn encrypted_fixture_open_behaviour_is_typed() {
    let path = mini("encrypted.pdf");
    let bytes = std::fs::read(&path).expect("read encrypted.pdf");
    // Opening an encrypted PDF without a password must be a typed outcome
    // (either WrongPassword/DecryptionFailed/InvalidPdf), never a panic.
    let r = no_panic("from_bytes(encrypted,no-pw)", || {
        PdfDocument::from_bytes(&bytes)
    });
    match r {
        Ok(_) => { /* fixture has empty user password — acceptable */ }
        Err(e) => {
            assert!(
                matches!(e, Error::DecryptionFailed { .. } | Error::InvalidPdf { .. }),
                "unexpected error variant for encrypted open: {e:?}"
            );
        }
    }
}
