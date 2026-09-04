//! Integration tests for Epic 2 #1243 — Merge & combine methods wiring.
//!
//! Exercises `PdfDocument::rotate_page`, `split_pages`, `extract_pages`, and
//! `PdfMerger::build()`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

const SAMPLE: &str = "tests/fixtures/sample.pdf";

// ---------------------------------------------------------------------------
// PdfMerger::build
// ---------------------------------------------------------------------------

#[test]
fn merge_two_documents_concatenates_pages() {
    let a = PdfDocument::open(SAMPLE).expect("open a");
    let b = PdfDocument::open(SAMPLE).expect("open b");

    let merged = PdfMerger::new().add(a).add(b).build().expect("merge");
    assert_eq!(
        merged.page_count(),
        2,
        "two 1-page docs merge to a 2-page doc",
    );
}

#[test]
fn merge_three_documents_concatenates_pages() {
    let merged = PdfMerger::new()
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .build()
        .expect("merge");
    assert_eq!(merged.page_count(), 3);
}

#[test]
fn merge_saves_and_reopens_with_correct_page_count() {
    let tmp = std::env::temp_dir().join("pdfluent-test-merge-roundtrip.pdf");
    let _ = std::fs::remove_file(&tmp);

    let merged = PdfMerger::new()
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .build()
        .expect("merge");
    merged.save(&tmp).expect("save");

    let reloaded = PdfDocument::open(&tmp).expect("reopen");
    assert_eq!(reloaded.page_count(), 2);

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn merge_empty_returns_error() {
    let err = PdfMerger::new().build().unwrap_err();
    match err {
        pdfluent::Error::Internal { message, .. } => {
            assert!(message.contains("no inputs"));
        }
        other => panic!("expected Error::Internal, got {other:?}"),
    }
}

#[test]
fn merge_with_bookmark_strategy_accepts_all_variants() {
    // 1.0 truth-gap: all strategies are accepted but only Concat is fully
    // honoured by the underlying pdf-manip merger. Regardless of the
    // strategy we should not error — if we ever tighten this to "only
    // Concat supported", the contract change ships via an RFC amendment
    // and this test flips to assert the new behaviour.
    for strat in [
        pdfluent::BookmarkMergeStrategy::Concat,
        pdfluent::BookmarkMergeStrategy::FlattenAll,
        pdfluent::BookmarkMergeStrategy::Discard,
    ] {
        let merged = PdfMerger::new()
            .add(PdfDocument::open(SAMPLE).unwrap())
            .add(PdfDocument::open(SAMPLE).unwrap())
            .with_bookmarks(strat)
            .build()
            .expect("all strategies accepted in 1.0");
        assert_eq!(merged.page_count(), 2);
    }
}

// ---------------------------------------------------------------------------
// split_pages
// ---------------------------------------------------------------------------

#[test]
fn split_pages_on_multi_page_doc() {
    // Build a 3-page source via merger, then split back.
    let source = PdfMerger::new()
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .build()
        .expect("merge");
    assert_eq!(source.page_count(), 3);

    let parts = source.split_pages().expect("split");
    assert_eq!(parts.len(), 3);
    for part in &parts {
        assert_eq!(part.page_count(), 1);
    }
}

#[test]
fn split_pages_on_single_page_returns_one_doc() {
    let doc = PdfDocument::open(SAMPLE).expect("open");
    let parts = doc.split_pages().expect("split");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].page_count(), 1);
}

// ---------------------------------------------------------------------------
// extract_pages
// ---------------------------------------------------------------------------

#[test]
fn extract_pages_inclusive_range() {
    // Build a 5-page source.
    let mut merger = PdfMerger::new();
    for _ in 0..5 {
        merger = merger.add(PdfDocument::open(SAMPLE).unwrap());
    }
    let source = merger.build().expect("merge");
    assert_eq!(source.page_count(), 5);

    let chunk = source.extract_pages(2..=4).expect("extract");
    assert_eq!(chunk.page_count(), 3, "pages 2, 3, 4 inclusive");
}

#[test]
fn extract_pages_from_open_range() {
    let source = PdfMerger::new()
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .add(PdfDocument::open(SAMPLE).unwrap())
        .build()
        .expect("merge");

    let tail = source.extract_pages(2..).expect("extract from 2");
    assert_eq!(tail.page_count(), 2, "pages 2 and 3");
}

#[test]
fn extract_pages_empty_range_errors() {
    let doc = PdfDocument::open(SAMPLE).expect("open");
    // 999..1000 is out of bounds for a 1-page doc.
    let err = doc.extract_pages(999..=1000).unwrap_err();
    match err {
        pdfluent::Error::Internal { message, .. } => {
            assert!(message.contains("out of bounds"));
        }
        other => panic!("expected Error::Internal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// rotate_page
// ---------------------------------------------------------------------------

#[test]
fn rotate_page_90_succeeds() {
    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    doc.rotate_page(1, pdfluent::Rotation::Clockwise90)
        .expect("rotate 90");
}

#[test]
fn rotate_page_180_and_270_succeed() {
    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    doc.rotate_page(1, pdfluent::Rotation::Clockwise180)
        .expect("rotate 180");
    doc.rotate_page(1, pdfluent::Rotation::Clockwise270)
        .expect("rotate 270");
}

#[test]
fn rotate_page_out_of_range_errors() {
    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    let err = doc
        .rotate_page(99, pdfluent::Rotation::Clockwise90)
        .unwrap_err();
    match err {
        pdfluent::Error::Internal { message, .. } => {
            assert!(message.contains("out of range"));
        }
        other => panic!("expected Error::Internal, got {other:?}"),
    }

    let err0 = doc
        .rotate_page(0, pdfluent::Rotation::Clockwise90)
        .unwrap_err();
    assert!(matches!(err0, pdfluent::Error::Internal { .. }));
}

#[test]
fn rotate_then_save_roundtrip() {
    let tmp = std::env::temp_dir().join("pdfluent-test-rotate-roundtrip.pdf");
    let _ = std::fs::remove_file(&tmp);

    let mut doc = PdfDocument::open(SAMPLE).expect("open");
    doc.rotate_page(1, pdfluent::Rotation::Clockwise90)
        .expect("rotate");
    doc.save(&tmp).expect("save");

    let reloaded = PdfDocument::open(&tmp).expect("reopen");
    assert_eq!(
        reloaded.page_count(),
        1,
        "rotation doesn't change page count"
    );

    let _ = std::fs::remove_file(&tmp);
}
