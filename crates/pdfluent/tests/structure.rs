//! TRUE-100 follow-up: outlines, annotations, attachments.
//!
//! Validates the newly-exposed facade read APIs (and outline write) plus
//! the v1 write-unsupported boundaries for annotations/attachments
//! (no facade write method exists; reads never panic, never silently
//! succeed on a missing target).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use pdfluent::prelude::*;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn open(name: &str) -> PdfDocument {
    let bytes = std::fs::read(mini(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
    PdfDocument::from_bytes(&bytes).unwrap_or_else(|e| panic!("open {name}: {e}"))
}

fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => panic!("{what} PANICKED"),
    }
}

// ---------------------------------------------------------------------------
// Outlines (read + write)
// ---------------------------------------------------------------------------

#[test]
fn outlines_read_never_panics_on_fixtures() {
    for name in [
        "simple.pdf",
        "multi-page.pdf",
        "acroform.pdf",
        "pdfa-2b.pdf",
    ] {
        let doc = open(name);
        let outlines = no_panic(&format!("outlines({name})"), || doc.outlines());
        // Documents may or may not have an outline; either way it must be Ok.
        assert!(
            outlines.is_ok(),
            "{name}: outlines() must be Ok, got {outlines:?}"
        );
    }
}

#[test]
fn outlines_write_then_read_roundtrips() {
    let mut doc = open("multi-page.pdf");
    let n = doc.page_count();
    assert!(n >= 3, "fixture should have >=3 pages");

    let written = vec![
        Outline {
            title: "Chapter 1".to_string(),
            page: Some(0),
            children: vec![Outline::new("Section 1.1", 1)],
        },
        Outline::new("Chapter 2", 2),
    ];
    doc.set_outlines(&written).expect("set_outlines");

    // Read back from the same in-memory doc.
    let read = doc.outlines().expect("outlines after write");
    assert_eq!(read.len(), 2, "two top-level outlines");
    assert_eq!(read[0].title, "Chapter 1");
    assert_eq!(read[0].page, Some(0));
    assert_eq!(read[0].children.len(), 1, "one child under Chapter 1");
    assert_eq!(read[0].children[0].title, "Section 1.1");
    assert_eq!(read[0].children[0].page, Some(1));
    assert_eq!(read[1].title, "Chapter 2");
    assert_eq!(read[1].page, Some(2));

    // Persist + reopen: outline survives a save/reload.
    let bytes = doc.to_bytes().expect("save");
    let reopened = PdfDocument::from_bytes(&bytes).expect("reopen");
    let read2 = reopened.outlines().expect("outlines after reload");
    assert_eq!(read2.len(), 2, "outline preserved across save/reload");
    assert_eq!(read2[0].title, "Chapter 1");
}

#[test]
fn outlines_clear_removes_all() {
    let mut doc = open("simple.pdf");
    doc.set_outlines(&[Outline::new("Only", 0)])
        .expect("set one");
    assert_eq!(doc.outlines().expect("read").len(), 1);
    doc.set_outlines(&[]).expect("clear");
    assert_eq!(doc.outlines().expect("read empty").len(), 0, "cleared");
}

// ---------------------------------------------------------------------------
// Annotations (read/list)
// ---------------------------------------------------------------------------

#[test]
fn annotations_read_is_ok_and_never_panics() {
    for name in ["simple.pdf", "acroform.pdf", "multi-page.pdf"] {
        let doc = open(name);
        let r = no_panic(&format!("annotations({name})"), || doc.annotations(0));
        assert!(r.is_ok(), "{name}: annotations(0) must be Ok");
        // Each annotation has a non-empty subtype string.
        for a in r.unwrap() {
            assert!(!a.subtype.is_empty(), "annotation subtype must be set");
        }
    }
}

#[test]
fn annotations_out_of_range_page_returns_empty_not_panic() {
    let doc = open("simple.pdf");
    let r = no_panic("annotations(9999)", || doc.annotations(9999));
    assert_eq!(
        r.expect("ok"),
        Vec::new(),
        "out-of-range page -> empty list"
    );
}

// ---------------------------------------------------------------------------
// Attachments (read/list + extract)
// ---------------------------------------------------------------------------

#[test]
fn attachments_list_never_panics() {
    for name in ["simple.pdf", "zugferd.pdf", "pdfa-2b.pdf"] {
        let doc = open(name);
        let r = no_panic(&format!("attachments({name})"), || doc.attachments());
        assert!(r.is_ok(), "{name}: attachments() must be Ok");
    }
}

#[test]
fn zugferd_attachment_extraction_if_present() {
    // ZUGFeRD invoices embed a factur-x / ZUGFeRD XML attachment. If this
    // fixture exposes attachments, extraction must return real bytes; if
    // it exposes none, the list is simply empty (no panic, no error).
    let doc = open("zugferd.pdf");
    let list = doc.attachments().expect("attachments()");
    for att in &list {
        let bytes = doc
            .attachment_bytes(&att.name)
            .expect("attachment_bytes ok")
            .unwrap_or_else(|| panic!("listed attachment '{}' must extract", att.name));
        assert!(!bytes.is_empty(), "extracted attachment must be non-empty");
        assert_eq!(
            bytes.len(),
            att.size,
            "reported size must match extracted byte length"
        );
    }
}

#[test]
fn attachment_bytes_missing_name_returns_none_not_error_not_panic() {
    let doc = open("simple.pdf");
    let r = no_panic("attachment_bytes(missing)", || {
        doc.attachment_bytes("definitely-not-an-attachment.xml")
    });
    assert_eq!(
        r.expect("ok"),
        None,
        "missing attachment -> None (no silent success, no error)"
    );
}
