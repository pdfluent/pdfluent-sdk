//! TATR as the first structure stage of the balanced tier's R1 (track B, P4).
//!
//! Per kept table region, TATR runs before the engine grid: where it fires
//! its cells are the region's table, and where it declines, fires under
//! coverage, or the region fails, R1 continues exactly as without it. R7
//! orphan engine tables never reach TATR: no detector region, no offer. The
//! fake seam (`TatrTestHook`) proves the wiring without a model or a dylib,
//! on any build with `ort-layout`; the real structurer needs `ort-tables`
//! as well and is measured, not unit-tested, here.
//!
//! Take the R1 step out and the fake's cells never land; take the fallback
//! out and a declining structurer stops reading as today's R1; route the
//! orphans through TATR and the firing fake rewrites them.

#![cfg(feature = "ort-layout")]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::balanced::{balanced_page_markdown, BalancedOptions};
use pdf_engine::layout::LayoutBox;
use pdf_engine::{ExtractOptions, PdfDocument, TatrOptions, TatrTestCell, TatrTestHook};

/// A thin line: a filled rectangle 0.5 pt across, as producers draw rulings.
fn rule(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
    let (w, h) = if (y1 - y0).abs() < 1e-9 {
        (x1 - x0, 0.5)
    } else {
        (0.5, y1 - y0)
    };
    vec![
        Operation::new("re", vec![x0.into(), y0.into(), w.into(), h.into()]),
        Operation::new("f", vec![]),
    ]
}

fn run(ops: &mut Vec<Operation>, body: &str, x: f64, y: f64, size: i64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new(
        "Tj",
        vec![Object::String(
            body.as_bytes().to_vec(),
            StringFormat::Literal,
        )],
    ));
    ops.push(Operation::new("ET", vec![]));
}

fn pdf_of(ops: Vec<Operation>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
        }),
    }));
    let pages = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
        p.set("Parent", Object::Reference(pages));
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// A ruled table, isolated on its page: a box round the lot, a line between
/// every row and column, and a head spanning all three because its inner lines
/// stop under it.
fn ruled_pdf() -> Vec<u8> {
    let xs = [72.0, 172.0, 372.0, 540.0];
    let ys = [700.0, 680.0, 640.0, 600.0, 560.0];
    let mut ops = Vec::new();
    for &y in &ys {
        ops.extend(rule(xs[0], y, xs[3], y));
    }
    ops.extend(rule(xs[0], ys[4], xs[0], ys[0]));
    ops.extend(rule(xs[3], ys[4], xs[3], ys[0]));
    ops.extend(rule(xs[1], ys[4], xs[1], ys[1]));
    ops.extend(rule(xs[2], ys[4], xs[2], ys[1]));
    run(&mut ops, "Feed materials of plant origin", 76.0, 686.0, 9);
    for (k, (a, b, c, d)) in [
        (
            "1.1.1",
            "Barley",
            "Grains of Hordeum vulgare L.",
            "It may be",
        ),
        (
            "1.1.2",
            "Barley, puffed",
            "Product obtained from milled",
            "or broken barley",
        ),
        (
            "1.1.3",
            "Barley, toasted",
            "Product of barley roasting",
            "partially burnt",
        ),
    ]
    .iter()
    .enumerate()
    {
        let top = ys[k + 1] - 14.0;
        run(&mut ops, a, 76.0, top, 9);
        run(&mut ops, b, 76.0, top - 12.0, 9);
        run(&mut ops, c, 176.0, top, 9);
        run(&mut ops, d, 376.0, top, 9);
    }
    pdf_of(ops)
}

/// One detector box over `[x0, y0, x1, y1]` in PDF points (y up): a kept
/// table region at score 0.9, addressed against the 96 dpi render.
fn table_box(x0: f64, y0: f64, x1: f64, y1: f64) -> LayoutBox {
    let (render_w, render_h) = (816u32, 1056u32);
    let px = |x: f64| (x / 72.0 * 96.0) as f32;
    let py = |y: f64| ((792.0 - y) / 72.0 * 96.0) as f32;
    LayoutBox {
        label: "table".to_string(),
        score: 0.9,
        bbox_px: [px(x0), py(y1), px(x1), py(y0)],
        render_w,
        render_h,
    }
}

/// A 2x2 over the region's first words: a shape the base grid never emits
/// for this page, so every cell of it in the output is the fake's.
fn two_by_two() -> TatrTestHook {
    let cell = |row: usize, col: usize, header: bool, words: Vec<usize>| {
        TatrTestCell::new(vec![row], vec![col], header, words)
    };
    TatrTestHook::Cells(vec![
        cell(0, 0, true, (0..8).collect()),
        cell(0, 1, true, (8..16).collect()),
        cell(1, 0, false, (16..24).collect()),
        cell(1, 1, false, (24..32).collect()),
    ])
}

/// The balanced assembly of one page: `None` for today's R1, `Some` for the
/// stage behind the seam.
fn balanced_of(pdf: &[u8], boxes: &[LayoutBox], hook: Option<TatrTestHook>) -> String {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    let mut opts = BalancedOptions::default();
    if let Some(hook) = hook {
        let mut tatr = TatrOptions::default();
        tatr.test_hook = Some(hook);
        opts = opts.with_tatr(tatr);
    }
    balanced_page_markdown(&doc, 0, boxes, &opts).expect("balanced runs")
}

/// The markdown's words, tags gone, sorted: the multiset the page reads.
fn sorted_markdown_words(markdown: &str) -> Vec<String> {
    let mut text = String::with_capacity(markdown.len());
    let mut in_tag = false;
    for c in markdown.chars() {
        match c {
            '<' => in_tag = true,
            // A tag ends a word: adjacent cells (`</td><td>`) hold no space
            // of their own.
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    // Pipe-table syntax (`|`, `---`) is structure, not words.
    let mut words: Vec<String> = text
        .split_whitespace()
        .filter(|w| w.chars().any(|c| c != '|' && c != '-'))
        .map(str::to_string)
        .collect();
    words.sort();
    words
}

/// The engine's own word multiset for the page: what both arms must keep.
fn engine_words(pdf: &[u8]) -> Vec<String> {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let mut words: Vec<String> = blocks
        .iter()
        .flat_map(|b| b.text.split_whitespace().map(str::to_string))
        .collect();
    words.sort();
    words
}

/// A declining structurer in a kept region is today's R1, byte for byte.
#[test]
fn a_declining_structurer_in_a_kept_region_is_todays_r1() {
    let pdf = ruled_pdf();
    let boxes = vec![table_box(60.0, 550.0, 550.0, 710.0)];
    let base = balanced_of(&pdf, &boxes, None);
    assert!(
        base.contains("<table"),
        "the page holds no table to keep: {base:?}"
    );
    let declined = balanced_of(&pdf, &boxes, Some(TatrTestHook::Declined));
    assert_eq!(declined, base, "a decline drifted from today's R1");
}

/// A failed region in a kept region is today's R1, byte for byte.
#[test]
fn a_failed_region_in_a_kept_region_is_todays_r1() {
    let pdf = ruled_pdf();
    let boxes = vec![table_box(60.0, 550.0, 550.0, 710.0)];
    let base = balanced_of(&pdf, &boxes, None);
    let failed = balanced_of(&pdf, &boxes, Some(TatrTestHook::Failed));
    assert_eq!(failed, base, "a failure drifted from today's R1");
}

/// R7 orphans never reach TATR: no detector region offers them, so a firing
/// fake reads exactly as without the stage -- while the orphan engine table
/// still renders.
#[test]
fn orphan_engine_tables_never_reach_tatr() {
    let pdf = ruled_pdf();
    let base = balanced_of(&pdf, &[], None);
    assert!(
        base.contains("<table"),
        "the orphan engine table did not render: {base:?}"
    );
    let fired = balanced_of(&pdf, &[], Some(two_by_two()));
    assert_eq!(
        fired, base,
        "a firing fake rewrote an orphan it must never be offered"
    );
}

/// A firing fake structures the kept region: its 2x2 lands where the base
/// reads the ruled grid, and the engine's word multiset survives the fire.
#[test]
fn a_firing_fake_structures_the_kept_region() {
    let pdf = ruled_pdf();
    let boxes = vec![table_box(60.0, 550.0, 550.0, 710.0)];
    let base = balanced_of(&pdf, &boxes, None);
    let base_tables = base.matches("<table").count();
    assert_eq!(
        base_tables, 1,
        "the base holds no single grid to replace: {base:?}"
    );
    let fired = balanced_of(&pdf, &boxes, Some(two_by_two()));
    // Pipes, as the region entry renders them: the stage shares
    // `to_markdown` with the region-built grid, not the engine HTML.
    assert!(
        fired.contains("| --- | --- |"),
        "the fake's 2x2 never landed: {fired:?}"
    );
    assert_ne!(
        fired, base,
        "the fake's 2x2 reads as the base grid: {fired:?}"
    );
    assert_eq!(
        sorted_markdown_words(&fired),
        engine_words(&pdf),
        "the fire lost or duplicated a word"
    );
    assert_eq!(
        sorted_markdown_words(&base),
        engine_words(&pdf),
        "the base itself does not hold the engine's words"
    );
}

/// Asking for TATR in the balanced tier on a build without it is refused,
/// never run without the stage.
#[cfg(not(feature = "ort-tables"))]
#[test]
fn asking_for_tatr_in_balanced_without_the_feature_refuses() {
    let pdf = ruled_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let boxes = vec![table_box(60.0, 550.0, 550.0, 710.0)];
    let opts = BalancedOptions::default().with_tatr(TatrOptions::default());
    let err = balanced_page_markdown(&doc, 0, &boxes, &opts).expect_err("must refuse");
    assert_eq!(
        err.options(),
        Some(&pdf_engine::OptionsError::TatrNeedsOrtTables),
        "the refusal names the missing build: {err:?}"
    );
}
