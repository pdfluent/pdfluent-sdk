//! WASM surface parity tests (Epic 4 #1233).
//!
//! These tests run on the **native** target but compile-check the
//! wasm-stub behaviour: both native and wasm paths of methods in the
//! "⚠️ native + wasm stub" row of `WASM_SUPPORT.md` must expose an
//! identical signature + return a typed error when the underlying
//! implementation is absent.
//!
//! The companion wasm build (`cargo check --target
//! wasm32-unknown-unknown -p pdfluent`) covers the actual
//! cross-compile; this file catches signature drift on native.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

fn enterprise_doc(path: &str) -> PdfDocument {
    PdfDocument::open_with(
        path,
        pdfluent::OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open sample")
}

// ---------------------------------------------------------------------------
// Signature compile-time parity
// ---------------------------------------------------------------------------
//
// A native-only method like `to_docx` has `#[cfg(not(target_arch =
// "wasm32"))]` on the real impl and `#[cfg(target_arch = "wasm32")]`
// on the wasm stub. Both must have the same external signature so
// callers don't need target-gated code. These `fn`-pointer
// assignments fail to compile if any signature drifts.

#[test]
fn to_docx_has_stable_signature() {
    let _: fn(&PdfDocument, &std::path::Path) -> Result<()> = |doc, p| doc.to_docx(p);
}

#[test]
fn to_images_has_stable_signature() {
    let _: fn(&PdfDocument, &std::path::Path, ToImagesOptions) -> Result<ToImagesReport> =
        |doc, p, o| doc.to_images(p, o);
}

// ---------------------------------------------------------------------------
// WASM_SUPPORT.md §2.5 truth-check — native path invariants
// ---------------------------------------------------------------------------
//
// These tests run on native and verify the native arm works. The
// wasm arm is verified by `cargo check --target
// wasm32-unknown-unknown` (a separate CI step) and by the matrix
// doc itself.

#[test]
fn to_docx_native_succeeds_on_sample() {
    let doc = enterprise_doc("tests/fixtures/sample.pdf");
    let out = std::env::temp_dir().join("pdfluent-wasm-surface-to_docx.docx");
    let _ = std::fs::remove_file(&out);
    doc.to_docx(&out).expect("to_docx native");
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn to_images_native_succeeds_on_sample() {
    let doc = enterprise_doc("tests/fixtures/sample.pdf");
    let dir = std::env::temp_dir().join("pdfluent-wasm-surface-to_images");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pattern = dir.join("page_{page}.png");
    let report = doc
        .to_images(&pattern, ToImagesOptions::new().with_dpi(72))
        .expect("to_images native");
    assert_eq!(report.paths.len(), doc.page_count());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Deferred items in WASM_SUPPORT.md §2.5 — must return
// Error::MissingDependency on BOTH targets today. The native arm
// is directly testable; the wasm arm compiles identically so the
// contract is preserved by signature-parity above.
// ---------------------------------------------------------------------------

#[test]
fn linearize_reports_unsupported_rather_than_a_missing_package() {
    let mut doc = enterprise_doc("tests/fixtures/sample.pdf");
    let err = doc.linearize().expect_err("deferred");
    assert_eq!(err.code(), "E-UNSUPPORTED");
}

#[test]
fn embed_font_rejects_data_that_is_not_a_font_on_both_targets() {
    let mut doc = enterprise_doc("tests/fixtures/sample.pdf");
    doc.embed_font(b"not-a-real-font", "Unknown")
        .expect_err("bytes that are not a font must not embed");
}

#[test]
fn add_decoration_watermarks_on_both_targets() {
    let mut doc = enterprise_doc("tests/fixtures/sample.pdf");

    // This asserted a failure until 23-08-2026. The watermark runtime it was
    // waiting for was already in pdf-manip and already reachable from the C
    // ABI; only the facade refused. Both targets must now do the work, because
    // "wasm can't" is a claim that needs to be true when it is made.
    doc.add_decoration(PageDecoration::watermark(
        "DRAFT",
        WatermarkOptions::centered(),
    ))
    .expect("add_decoration");

    assert!(
        doc.extract_text().expect("extract_text").contains("DRAFT"),
        "the watermark is not in the document"
    );
}

// ---------------------------------------------------------------------------
// Error surface — UnsupportedOnWasm variant must remain stable.
// ---------------------------------------------------------------------------
//
// The WASM stubs construct this variant; its `code()` is part of
// the frozen error contract (STABILITY.md §8).

#[test]
fn unsupported_on_wasm_code_is_stable() {
    let err = Error::UnsupportedOnWasm {
        operation: "example",
    };
    assert_eq!(err.code(), "E-ENV-UNSUPPORTED-ON-WASM");
    let msg = format!("{err}");
    assert!(msg.contains("example"));
    assert!(msg.to_lowercase().contains("wasm"));
}
