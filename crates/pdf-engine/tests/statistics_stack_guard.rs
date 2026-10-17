// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Statistics stacks stay fused under B's spread guard (#574).
//!
//! A mean over its `(SD)` and an estimate over its `(95% CI)` read as one
//! value with its spread on the line below, not as two rows. B's veto would
//! split them -- every second-line figure a spread -- so the guard leaves
//! such rows fused while an accounting row with an amount below still splits:
//! a plain figure or a nil dash beside the negatives (see DLR p61 in
//! `dlr_stacked_figure_rows.rs`), or separators and three-digit integers
//! with no spread shape at all.
//!
//! The operator-built pages test the whole path: interpreter, spans, cut,
//! judge, decoder, tidy, serializer. Each together-test runs with B on and
//! fails with the guard removed (two rows where one belongs); each
//! apart-test fails with m258's parenthesised-only predicate (one fused row
//! where two belong).
//!
//! An accounting row whose stub holds stacked labels splits regardless (the
//! stub rule, same switch), even over spreads.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{Switches, SWITCHES};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const SIZE: f64 = 10.0;
const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;
const STUB_X: f64 = 40.0;
const FIG_X: f64 = 300.0;
const LAST_X: f64 = 420.0;

fn pdf_of(pieces: &[(String, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut ops = Vec::new();
    for (text, x, y) in pieces {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), SIZE.into()],
        ));
        ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        ops.push(Operation::new("ET", vec![]));
    }
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }
            .encode()
            .expect("encode content"),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font),
            }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

fn switches_b() -> Switches {
    Switches {
        table_runs_on_figures: true,
        ..SWITCHES
    }
}

fn blocks_with_b(pieces: &[(String, f64, f64)]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks_with_switches(&ExtractOptions::default(), switches_b())
        .expect("extract blocks")
}

fn cell_texts(blocks: &[Block]) -> Vec<String> {
    let mut texts: Vec<String> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.clone())
        .collect();
    texts.sort();
    texts
}

/// PMC2685810 p4 in miniature: `297.3` over `(30.8)`, the total only in
/// the upper row. The stub column reads as prose on this narrow page, so the
/// proof is the figure cell: one `297.3 (30.8)` cell with B on, two cells
/// with the guard removed.
#[test]
fn a_mean_over_sd_stack_stays_fused_through_extract_blocks() {
    let pieces = vec![
        ("Clinic".to_string(), STUB_X, TOP),
        ("Mean".to_string(), FIG_X, TOP),
        ("Total".to_string(), LAST_X, TOP),
        ("Clinics' outpatient".to_string(), STUB_X, TOP - PITCH),
        ("297.3".to_string(), FIG_X, TOP - PITCH),
        ("15840".to_string(), LAST_X, TOP - PITCH),
        ("books (i)".to_string(), STUB_X, TOP - 2.0 * PITCH),
        ("(30.8)".to_string(), FIG_X, TOP - 2.0 * PITCH),
        ("Total".to_string(), STUB_X, TOP - 3.0 * PITCH),
        ("500.0".to_string(), FIG_X, TOP - 3.0 * PITCH),
        ("30000".to_string(), LAST_X, TOP - 3.0 * PITCH),
    ];
    let blocks = blocks_with_b(&pieces);
    let texts = cell_texts(&blocks);
    assert!(
        texts.iter().any(|t| t == "297.3 (30.8)"),
        "the mean joins its spread: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "(30.8)"),
        "no split stack survives: {texts:?}"
    );
}

/// PMC2853555 p6 in miniature: `3.53` over `(1.22,6.12)`, the stub a single
/// line and the last figure only in the upper row. One fused row with B on.
#[test]
fn an_estimate_over_ci_stack_stays_fused_through_extract_blocks() {
    let pieces = vec![
        ("Effect".to_string(), STUB_X, TOP),
        ("Estimate".to_string(), FIG_X, TOP),
        ("Total".to_string(), LAST_X, TOP),
        ("Effect estimate".to_string(), STUB_X, TOP - PITCH),
        ("3.53".to_string(), FIG_X, TOP - PITCH),
        ("106.9".to_string(), LAST_X, TOP - PITCH),
        ("(1.22,6.12)".to_string(), FIG_X, TOP - 2.0 * PITCH),
        ("Total".to_string(), STUB_X, TOP - 3.0 * PITCH),
        ("5.00".to_string(), FIG_X, TOP - 3.0 * PITCH),
        ("200.0".to_string(), LAST_X, TOP - 3.0 * PITCH),
    ];
    let blocks = blocks_with_b(&pieces);
    let texts = cell_texts(&blocks);
    assert!(
        texts.iter().any(|t| t == "3.53 (1.22,6.12)"),
        "the estimate joins its interval: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "(1.22,6.12)"),
        "no split stack survives: {texts:?}"
    );
}

/// A spread under a plain value through the whole path: `12.4` over `(3.1)`
/// with a stub-empty lower row. The guard holds every spread, so the pair
/// joins with B on; two cells with the guard removed.
#[test]
fn a_spread_under_a_plain_value_stays_fused_through_extract_blocks() {
    let pieces = vec![
        ("Effect".to_string(), STUB_X, TOP),
        ("Estimate".to_string(), FIG_X, TOP),
        ("Total".to_string(), LAST_X, TOP),
        ("Value".to_string(), STUB_X, TOP - PITCH),
        ("12.4".to_string(), FIG_X, TOP - PITCH),
        ("106.9".to_string(), LAST_X, TOP - PITCH),
        ("(3.1)".to_string(), FIG_X, TOP - 2.0 * PITCH),
        ("Total".to_string(), STUB_X, TOP - 3.0 * PITCH),
        ("5.00".to_string(), FIG_X, TOP - 3.0 * PITCH),
        ("200.0".to_string(), LAST_X, TOP - 3.0 * PITCH),
    ];
    let blocks = blocks_with_b(&pieces);
    let texts = cell_texts(&blocks);
    assert!(
        texts.iter().any(|t| t == "12.4 (3.1)"),
        "the value joins its spread: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "(3.1)"),
        "no split stack survives: {texts:?}"
    );
}

/// An amount under an amount through the whole path: `86,090` over `(4,305)`
/// with a stub-empty lower row, T1's m249b pin. Separators refuse the spread,
/// so the pair stays apart with B on; one fused cell with m258's
/// parenthesised-only predicate.
#[test]
fn an_amount_under_an_amount_stays_apart_through_extract_blocks() {
    let pieces = vec![
        ("Item".to_string(), STUB_X, TOP),
        ("Amount".to_string(), FIG_X, TOP),
        ("Total".to_string(), LAST_X, TOP),
        ("Revenue".to_string(), STUB_X, TOP - PITCH),
        ("86,090".to_string(), FIG_X, TOP - PITCH),
        ("106,900".to_string(), LAST_X, TOP - PITCH),
        ("(4,305)".to_string(), FIG_X, TOP - 2.0 * PITCH),
        ("Cost".to_string(), STUB_X, TOP - 3.0 * PITCH),
        ("12,000".to_string(), FIG_X, TOP - 3.0 * PITCH),
        ("20,000".to_string(), LAST_X, TOP - 3.0 * PITCH),
    ];
    let blocks = blocks_with_b(&pieces);
    let texts = cell_texts(&blocks);
    assert!(
        !texts.iter().any(|t| t == "86,090 (4,305)"),
        "no fused amount survives: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "86,090"),
        "the upper amount stands alone: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "(4,305)"),
        "the lower amount stands alone: {texts:?}"
    );
}

/// AME 2009 p17's all-negative row through the whole path: `Total segment
/// operating income` over `Corporate administrative and other expenses`,
/// `399,457 19.0` over `(33,407) (1.6)`. The amounts are no spreads --
/// separators above -- so B splits, and the stacked stub labels would split
/// the row regardless. Two rows with B on; one fused row with B off.
///
/// A real page, like DLR p61 in `dlr_stacked_figure_rows.rs`: the decoder
/// fuses no operator-built pair, so only a real page drives the split.
#[test]
fn an_all_negative_accounting_row_splits_on_stacked_labels_through_extract_blocks() {
    let pdf = include_bytes!("fixtures/fintabnet/stacked-labels-ame-p17.pdf");
    let blocks = PdfDocument::open(pdf.to_vec())
        .expect("open pdf")
        .extract_blocks_with_switches(&ExtractOptions::default(), switches_b())
        .expect("extract blocks");
    let texts = cell_texts(&blocks);
    let plain = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        texts
            .iter()
            .any(|t| plain(t) == "Total segment operating income"),
        "the upper stub stands alone: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| plain(t) == "Corporate administrative and other expenses"),
        "the lower stub stands alone: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| plain(t) == "(33,407) (1.6)"),
        "the lower figures stand alone: {texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("operating income Corporate")),
        "no fused row survives: {texts:?}"
    );
}
