//! OCR on the markdown path, through the system Tesseract.
//!
//! With `ocr=Auto` a scanned page reads its words and a born-digital page
//! reads exactly as without OCR; with `ocr=Force` every page in the window is
//! recognized and a born-digital page's own text is replaced, never doubled.
//! Without a `tesseract` binary `Auto` reads on silently and `Force` raises
//! naming it.
//!
//! Requires the `ocr-cli` feature: `cargo test -p pdf-engine --features
//! ocr-cli`. Tests that need the binary skip with a reason where none is
//! installed; the missing-binary behaviour is pinned by pointing
//! `PDFLUENT_TESSERACT` at nothing, which needs no install. The tests share
//! one lock because that override is process-wide: a test that needs the
//! real binary must not run while another has hidden it.
#![cfg(feature = "ocr-cli")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::Mutex;

use pdf_engine::{ExtractOptions, OcrMode, PdfDocument};
use test_skip::skip_test;

/// The process-wide Tesseract environment belongs to one test at a time.
static TESSERACT_ENV: Mutex<()> = Mutex::new(());

/// The environment lock, tolerating a poisoned mutex: a sibling that fails
/// out of `have_tesseract` never touches the environment, so there is no
/// state to distrust and the failure must stay its own.
fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    TESSERACT_ENV
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The repository's scanned page: an image of the words "Test page".
fn scan_fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scanned/scan_01_sample.pdf"
    ))
    .expect("scan fixture is readable")
}

/// A born-digital document: two pages of prose and a table.
fn typeset_fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/structured.pdf"
    ))
    .expect("typeset fixture is readable")
}

/// The scan's born-digital source: one "Test page" line in a stream whose
/// dictionary the working parser misreads (`//Length`) while the lenient
/// reader tolerates. The OCR copy must keep reading it.
fn quirky_source_fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scanned/scan_01_sample.source.pdf"
    ))
    .expect("quirky source fixture is readable")
}

fn options_with_ocr(mode: OcrMode) -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.ocr = mode;
    options
}

/// Whether the binary answers: no install, no recognition. Where
/// `PDFLUENT_REQUIRE_TESSERACT` is set a missing binary fails the test, so a
/// broken install cannot hide behind skips.
///
/// A boolean rather than a skip in disguise: `skip_test!` returns from the
/// function it sits in, so a helper that "skips" merely returns to a test
/// that runs on regardless.
fn have_tesseract() -> bool {
    if pdf_ocr::TesseractCliEngine::is_available() {
        return true;
    }
    if std::env::var_os("PDFLUENT_REQUIRE_TESSERACT").is_some() {
        panic!(
            "PDFLUENT_REQUIRE_TESSERACT is set but no tesseract binary \
             answers --version"
        );
    }
    false
}

/// A recognizer that always reads one word, for the tests that pin the copy
/// rather than the recognition. Needs no binary and touches no environment.
struct WordEngine;

impl pdf_ocr::OcrEngine for WordEngine {
    fn recognize(
        &self,
        _image_data: &[u8],
        width: u32,
        height: u32,
        _dpi: u32,
    ) -> Result<pdf_ocr::OcrPageResult, String> {
        Ok(pdf_ocr::OcrPageResult {
            words: vec![pdf_ocr::OcrWord {
                text: "hello".to_string(),
                bbox_px: [10, 10, 110, 60],
                confidence: 0.95,
            }],
            confidence: 0.95,
            image_width: width,
            image_height: height,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["eng".to_string()]
    }
}

#[test]
fn auto_reads_a_scanned_page_and_never_does_not() {
    let _env = lock_env();
    if !have_tesseract() {
        skip_test!("no tesseract binary installed");
    }
    let document = PdfDocument::open(scan_fixture()).unwrap();

    let plain = document
        .extract_markdown(&options_with_ocr(OcrMode::Never))
        .unwrap();
    assert_eq!(
        plain.trim(),
        "![](figure-0001-01.png)",
        "without OCR the scan is one figure marker"
    );

    let read = document
        .extract_markdown(&options_with_ocr(OcrMode::Auto))
        .unwrap();
    assert!(
        read.contains("Test") && read.contains("page"),
        "Auto reads the scanned words: {read:?}"
    );
}

#[test]
fn auto_leaves_a_born_digital_page_alone() {
    let _env = lock_env();
    if !have_tesseract() {
        skip_test!("no tesseract binary installed");
    }
    let document = PdfDocument::open(typeset_fixture()).unwrap();

    let plain = document
        .extract_markdown(&options_with_ocr(OcrMode::Never))
        .unwrap();
    let auto = document
        .extract_markdown(&options_with_ocr(OcrMode::Auto))
        .unwrap();
    assert_eq!(
        auto, plain,
        "Auto over born-digital pages reads the document itself"
    );
}

#[test]
fn force_on_a_born_digital_page_does_not_duplicate_its_text() {
    let _env = lock_env();
    if !have_tesseract() {
        skip_test!("no tesseract binary installed");
    }
    let document = PdfDocument::open(typeset_fixture()).unwrap();

    let plain = document
        .extract_markdown(&options_with_ocr(OcrMode::Never))
        .unwrap();
    let forced = document
        .extract_markdown(&options_with_ocr(OcrMode::Force))
        .unwrap();
    assert!(
        forced.contains("Safety"),
        "Force reads the page it stripped: {forced:?}"
    );
    for word in plain.split_whitespace() {
        if word.len() < 4 {
            continue;
        }
        let before = plain.matches(word).count();
        let after = forced.matches(word).count();
        assert!(
            after <= before,
            "{word:?} reads {before} times natively and {after} times forced"
        );
    }
}

#[test]
fn without_a_binary_auto_reads_on_and_force_raises() {
    let _env = lock_env();
    let previous = std::env::var_os(pdf_ocr::tesseract_cli::TESSERACT_ENV_OVERRIDE);
    std::env::set_var(
        pdf_ocr::tesseract_cli::TESSERACT_ENV_OVERRIDE,
        "/nonexistent-dir-for-ocr-tests/tesseract",
    );

    let document = PdfDocument::open(scan_fixture()).unwrap();
    let plain = document
        .extract_markdown(&options_with_ocr(OcrMode::Never))
        .unwrap();
    let auto = document
        .extract_markdown(&options_with_ocr(OcrMode::Auto))
        .unwrap();
    assert_eq!(auto, plain, "Auto without a binary is today's output");
    let err = document
        .extract_markdown(&options_with_ocr(OcrMode::Force))
        .unwrap_err();
    assert!(
        err.to_string().contains("tesseract"),
        "Force names the missing binary: {err}"
    );

    match previous {
        Some(value) => std::env::set_var(pdf_ocr::tesseract_cli::TESSERACT_ENV_OVERRIDE, value),
        None => std::env::remove_var(pdf_ocr::tesseract_cli::TESSERACT_ENV_OVERRIDE),
    }
}

#[test]
fn the_copy_keeps_native_text_a_rewrite_would_lose() {
    // The quirky source reads "Test page" through the lenient reader while
    // the working parser sees an empty stream. The copy is an incremental
    // update over the original bytes, so both the native line and the layer
    // read; a rewrite would serialize the misread stream back empty and the
    // native line would be gone.
    let document = PdfDocument::open(quirky_source_fixture()).unwrap();
    let copy = document
        .searchable_copy(&options_with_ocr(OcrMode::Auto), &WordEngine)
        .unwrap();
    let markdown = copy
        .extract_markdown(&options_with_ocr(OcrMode::Never))
        .unwrap();
    assert!(
        markdown.contains("Test") && markdown.contains("page"),
        "the native line survives the copy: {markdown:?}"
    );
    assert!(
        markdown.contains("hello"),
        "the recognized word lands on the copy: {markdown:?}"
    );
}
