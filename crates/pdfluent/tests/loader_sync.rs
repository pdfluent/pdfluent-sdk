//! Dual-loader sync invariant (RFC 0002, Option C).
//!
//! PDFluent loads every document through two models: the read-only `pd-syntax`
//! engine (`page_count` / render / extract) and the mutable `lopdf` model
//! (writes + serialization). After a mutation the facade serializes lopdf and
//! re-parses through both loaders (`refresh_from_lopdf`). These tests assert
//! the two models stay in sync across a mutate -> serialize -> reload cycle:
//! page count is preserved and a write round-trips from the lopdf side back
//! through the pd-syntax read side. They fail loudly if a future change
//! desyncs the loaders.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

const SAMPLE: &str = "tests/fixtures/sample.pdf";
const FORM: &str = "tests/fixtures/form.pdf";

#[test]
fn page_count_survives_lopdf_serialize_then_reload() {
    let doc = PdfDocument::open(SAMPLE).expect("open sample");
    let pages = doc.page_count();
    assert_eq!(pages, 1, "fixture precondition: sample.pdf has 1 page");

    // lopdf write path -> serialize -> pd-syntax + lopdf reload.
    let bytes = doc.to_bytes().expect("serialize via lopdf");
    let reloaded = PdfDocument::from_bytes(&bytes).expect("reload via both loaders");
    assert_eq!(
        reloaded.page_count(),
        pages,
        "pd-syntax read view must agree with the lopdf-serialized bytes"
    );
}

#[test]
fn mutation_roundtrips_through_both_loaders() {
    let mut doc = PdfDocument::open(SAMPLE).expect("open sample");
    let pages = doc.page_count();

    doc.metadata_mut()
        .set_title("Loader Sync Probe")
        .commit()
        .expect("commit metadata to lopdf");

    let bytes = doc.to_bytes().expect("serialize");
    let reloaded = PdfDocument::from_bytes(&bytes).expect("reload");

    // Sync invariant: structure preserved AND the lopdf-side mutation is
    // visible after the pd-syntax reparse path.
    assert_eq!(reloaded.page_count(), pages, "page count must be stable");
    assert_eq!(
        reloaded.metadata().title.as_deref(),
        Some("Loader Sync Probe"),
        "mutation must round-trip lopdf -> bytes -> reload"
    );
}

#[test]
fn acroform_doc_stays_in_sync_across_reload() {
    let doc = PdfDocument::open(FORM).expect("open form");
    let pages = doc.page_count();
    let bytes = doc.to_bytes().expect("serialize form");
    let reloaded = PdfDocument::from_bytes(&bytes).expect("reload form");
    assert_eq!(
        reloaded.page_count(),
        pages,
        "AcroForm page count stable across the dual-loader reload"
    );
}
