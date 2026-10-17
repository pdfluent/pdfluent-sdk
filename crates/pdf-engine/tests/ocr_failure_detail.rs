//! OCR failures name the engine they tried (lane L6).
//!
//! The failure detail reverted to a bare message: forced recognition on a
//! build without an engine fails saying only that no engine exists, never
//! which engine was attempted. The rebuilt engine carries the detail -- at
//! least the engine it tried -- so a caller knows what to install.
//!
//! Only the shape is pinned: the message names the attempted engine.
//! Unspecified; our behaviour: without an engine the front door stays
//! silent (G184) where we raise, so the least the raise can do is say what
//! it looked for. Run on the default build, without the OCR feature.
//!
//! The gate below is load-bearing, not decorative: in a workspace-wide
//! build Cargo unifies features, so a crate that enables `ocr-cli`
//! (pdf-node does) compiles an engine into this test's build, forced
//! recognition succeeds, and the failure below would fail. With the gate
//! the test only exists where its premise holds.

// Without an engine compiled in, forced recognition names the engine it
// looked for. Under `ocr-cli` an engine exists by construction, so the
// premise -- and the test -- does not apply.
#![cfg(not(feature = "ocr-cli"))]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, OcrMode, PdfDocument};

/// One line on one page.
fn one_line() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 10.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new("Tj", vec![Object::string_literal("The line on the page.")]),
        Operation::new("ET", vec![]),
    ];
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_forced_run_without_an_engine_names_the_engine_it_tried() {
    let mut options = ExtractOptions::default();
    options.ocr = OcrMode::Force;
    let error = PdfDocument::open(one_line())
        .expect("open")
        .extract_blocks(&options)
        .expect_err("forced OCR without an engine fails");
    let message = error
        .ocr_message()
        .expect("an OCR failure reads through ocr_message");
    assert!(
        message.to_lowercase().contains("tesseract"),
        "the failure names no engine: {message:?}"
    );
}
