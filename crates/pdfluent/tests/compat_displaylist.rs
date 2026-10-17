// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Page.get_displaylist` through the facade (#643).
//!
//! Freezing answers the page box with and without marks, as the recording
//! pins; replaying the frozen list fails loudly (replay stays red).

use pdfluent::compat::Page;
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> PdfDocument {
    PdfDocument::from_bytes(SAMPLE_PDF).expect("sample opens")
}

#[test]
fn freeze_answers_the_page_box_either_way() {
    let doc = open_sample();
    for annots in [false, true] {
        let list = Page::new(&doc, 0)
            .get_displaylist(annots)
            .expect("freeze works");
        let rect = list.rect();
        assert_eq!(
            (rect.x0, rect.y0, rect.x1, rect.y1),
            (0.0, 0.0, 612.0, 792.0)
        );
    }
}

#[test]
fn replay_fails_loudly() {
    use pdfluent::compat::Matrix;
    let doc = open_sample();
    let list = Page::new(&doc, 0)
        .get_displaylist(true)
        .expect("freeze works");
    match list.run(Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    }) {
        Err(pdfluent::Error::Unsupported(reason)) => assert_eq!(
            reason,
            "not supported in this version: DisplayList.run; see KNOWN_DIFFERENCES"
        ),
        Err(other) => panic!("wrong error class: {other:?}"),
        Ok(()) => panic!("silent success, expected Unsupported"),
    }
}
