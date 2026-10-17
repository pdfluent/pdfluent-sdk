// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `module.get_highlight_selection` through the facade (#643).
//!
//! One quad per spanned line span, clamped to the given points' x edges;
//! the bounds below are the recorded `l5-module-get-highlight-selection`
//! selection boxes.

use pdfluent::compat::{get_highlight_selection, Page, Point, Quad, Rect};
use pdfluent::prelude::*;

const STRUCTURED_PDF: &[u8] = include_bytes!("../../../fixtures/structured.pdf");

fn open_structured() -> PdfDocument {
    PdfDocument::from_bytes(STRUCTURED_PDF).expect("structured opens")
}

fn bounds(quad: &Quad) -> (f64, f64, f64, f64) {
    let xs = [quad.ul.x, quad.ur.x, quad.ll.x, quad.lr.x];
    let ys = [quad.ul.y, quad.ur.y, quad.ll.y, quad.lr.y];
    (
        xs.iter().cloned().fold(f64::INFINITY, f64::min),
        ys.iter().cloned().fold(f64::INFINITY, f64::min),
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    )
}

fn assert_bounds(quads: &[Quad], want: &[[f64; 4]]) {
    assert_eq!(quads.len(), want.len(), "quad count");
    for (quad, w) in quads.iter().zip(want.iter()) {
        let got = bounds(quad);
        // Millipoint pins: the dict path the door used to take rounds
        // through its window, so raw capture unions sit ~1e-6 off the
        // recorded f32-flavoured values; the harness allows half a point.
        for (g, e) in [got.0, got.1, got.2, got.3].iter().zip(w.iter()) {
            assert!((g - e).abs() < 1e-3, "bound {got:?} vs want {w:?}");
        }
    }
}

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn span_clamps_to_the_stop_edge() {
    let doc = open_structured();
    let page = Page::new(&doc, 0);
    let quads = get_highlight_selection(
        &page,
        Some(point(72.0, 100.0)),
        Some(point(200.0, 140.0)),
        None,
    )
    .expect("highlight works");
    assert_bounds(
        &quads,
        &[
            [
                72.0,
                94.87999725341797,
                162.70401000976562,
                116.91200256347656,
            ],
            [72.0, 125.25, 75.5, 138.99000549316406],
            [86.0, 125.25, 241.0799560546875, 138.99000549316406],
            [72.0, 137.25, 75.5, 150.99000549316406],
            [86.0, 137.25, 200.0, 150.99000549316406],
        ],
    );
}

#[test]
fn whole_and_blank_and_tiny_clip() {
    let doc = open_structured();
    let page = Page::new(&doc, 0);
    let whole = get_highlight_selection(&page, None, None, None).expect("whole works");
    assert_eq!(whole.len(), 7);
    let tiny = get_highlight_selection(
        &page,
        None,
        None,
        Some(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
        }),
    )
    .expect("clip works");
    assert!(tiny.is_empty());

    let fresh = PdfDocument::create();
    let blank = Page::new(&fresh, 0);
    let none = get_highlight_selection(&blank, None, None, None).expect("blank works");
    assert!(none.is_empty());
}
