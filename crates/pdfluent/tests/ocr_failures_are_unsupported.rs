//! OCR asked for and not runnable is `Unsupported`, not a broken file.
//!
//! The engine carries OCR failures as `Io` with a typed cause (the enum is
//! exhaustive in 1.0.0), and the facade reads them back through
//! `ocr_message`. Forcing OCR with no `tesseract` binary must therefore
//! refuse as unsupported naming the cause, whatever the engine build carries:
//! with `ocr-cli` the typed construction failure, without it the engine-side
//! one. Pointing `PDFLUENT_TESSERACT` at nothing pins the missing binary
//! without needing the install.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::{Error, ExtractOptions, OcrMode, PdfDocument};

const SCAN: &str = "../../fixtures/scanned/scan_01_sample.pdf";
const OVERRIDE: &str = "PDFLUENT_TESSERACT";
const NO_BINARY: &str = "/nonexistent-dir-for-ocr-tests/tesseract";

#[test]
fn forcing_ocr_without_a_binary_is_unsupported() {
    let previous = std::env::var_os(OVERRIDE);
    std::env::set_var(OVERRIDE, NO_BINARY);

    let mut options = ExtractOptions::default();
    options.ocr = OcrMode::Force;
    let err = PdfDocument::open(SCAN)
        .expect("scan fixture opens")
        .extract_markdown(&options)
        .unwrap_err();

    match previous {
        Some(value) => std::env::set_var(OVERRIDE, value),
        None => std::env::remove_var(OVERRIDE),
    }

    match err {
        Error::Unsupported(message) => assert!(
            message.starts_with("OCR error: "),
            "the refusal names OCR: {message:?}"
        ),
        other => panic!("expected Unsupported, got: {other:?}"),
    }
}
