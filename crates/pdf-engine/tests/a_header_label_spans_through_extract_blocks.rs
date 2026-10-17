// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A header label of several words spans its columns on the production path (#574).
//!
//! m69/m69b's `lone_header_span` has green unit tests and never fires in
//! production: `cells_placed` reaches it only when the header row holds one
//! slot, but `place()` puts each word of a multi-word label in the column
//! stripe under it, so a four-word label lands in three slots and the guard
//! skips the row. `spaced_header_joined` later fuses the three into `colspan=3`
//! over eight figure columns. The unit tests hand-build a one-slot `Placement`,
//! a shape `place()` never produces for a label of several words.
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, judge, decoder, serializer. The centred
//! label goes red without the runs grouping in `cells_placed` (it comes back
//! `column_span: 3`); the off-centre label and the two labels stay narrow with
//! and without it, which is what keeps the spanning rule honest.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::separations::HEADER_SPAN_TAKES_STUB;
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;

/// Stub plus eight figure columns, wide enough for the decoder to cut nine
/// stripes and narrow enough at the stub for the heuristic builder to stand
/// down, so the judged region forces the decoder path.
const COLUMNS: [f64; 9] = [40.0, 120.0, 175.0, 230.0, 285.0, 340.0, 395.0, 450.0, 505.0];
const STUBS: [&str; 4] = ["Strain A", "Strain B", "Strain C", "Strain D"];
const FIGS: [&str; 8] = [
    "12.3", "45.6", "78.9", "10.2", "33.4", "55.6", "77.8", "99.0",
];

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn pdf_of(pieces: &[(String, bool, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let heavy = font(&mut doc, b"Helvetica-Bold");
    let mut ops = Vec::new();
    for (text, bold, x, y) in pieces {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if *bold { b"F2" } else { b"F1" }.to_vec()),
                BODY_SIZE.into(),
            ],
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
                "F1" => regular.clone(),
                "F2" => heavy.clone(),
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

fn body_rows(pieces: &mut Vec<(String, bool, f64, f64)>) {
    for (r, stub) in STUBS.iter().enumerate() {
        let y = TOP - (r as f64 + 1.0) * PITCH;
        pieces.push((stub.to_string(), false, COLUMNS[0], y));
        for (c, fig) in FIGS.iter().enumerate() {
            pieces.push((format!("{fig}{r}"), false, COLUMNS[c + 1], y));
        }
    }
}

fn blocks_of(pieces: &[(String, bool, f64, f64)]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// Every cell of row 0 as `(column, column_span, text)`, left to right.
fn header_cells(blocks: &[Block]) -> Vec<(u16, u16, String)> {
    let mut out: Vec<(u16, u16, String)> = blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell {
                row: 0,
                column,
                column_span,
                ..
            } => Some((column, column_span, b.text.clone())),
            _ => None,
        })
        .collect();
    out.sort();
    out
}

fn body_cell_count(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { row: 1.., .. }))
        .count()
}

/// A centred four-word label over a stub and eight figure columns spans the
/// figures it heads, not just the three stripes its words stand over.
#[test]
fn a_centred_multiword_header_spans_the_figures_it_heads() {
    let mut pieces = vec![("Amino acid identity (%)".to_string(), true, 250.0, TOP)];
    body_rows(&mut pieces);
    let blocks = blocks_of(&pieces);
    let first: u16 = if HEADER_SPAN_TAKES_STUB { 0 } else { 1 };
    let span: u16 = 9 - first;
    assert_eq!(
        header_cells(&blocks),
        vec![(first, span, "Amino acid identity (%)".to_string())],
        "the centred label spans the table, not the three stripes under it: {:?}",
        header_cells(&blocks),
    );
    assert_eq!(
        body_cell_count(&blocks),
        36,
        "four body rows of a stub and eight figures"
    );
}

/// The same label set over columns 3-4 only, clearly left of centre, stays
/// narrow: a head over two columns of eight is not a table heading (#576).
#[test]
fn an_off_centre_header_over_two_columns_stays_narrow() {
    let mut pieces = vec![("Amino acid identity (%)".to_string(), true, 150.0, TOP)];
    body_rows(&mut pieces);
    let blocks = blocks_of(&pieces);
    let header = header_cells(&blocks);
    assert_eq!(header.len(), 1, "one header cell: {header:?}");
    let (column, span, _) = &header[0];
    assert!(
        *span < 8,
        "an off-centre head must not span the table: {header:?}"
    );
    assert_ne!(*column, 0, "it does not take the stub: {header:?}");
    assert_eq!(
        body_cell_count(&blocks),
        36,
        "four body rows of a stub and eight figures"
    );
}

/// Rows of the table as `(row, column, column_span, header, text)`, row-major.
fn rows_of(blocks: &[Block]) -> Vec<(u16, u16, u16, bool, String)> {
    let mut out: Vec<(u16, u16, u16, bool, String)> = blocks
        .iter()
        .filter_map(|b| match &b.kind {
            BlockKind::TableCell {
                row,
                column,
                column_span,
                header,
                ..
            } => Some((*row, *column, *column_span, *header, b.text.clone())),
            _ => None,
        })
        .collect();
    out.sort();
    out
}

/// A header band does not swallow the year row under it (#574).
///
/// ODFL_2018 p5 t1 in miniature: `Year Ended December 31,` centred over a
/// stub and three figure columns, the year row `(In thousands) | 2018 | 2017
/// | 2016`, three body rows whose `$` signs stand apart so the decoder cuts
/// `$` sub-columns the year row leaves empty. The stubs fall outside the
/// judged table region and decode as prose -- the TWTR_2015 p91 shape, where
/// the year row's stub is empty too -- and the year row's empties are the `$`
/// sub-columns. Without the guard the full-width span arms
/// `join_rows_that_run_on`: every written year cell sits under the one cell,
/// the row has empties, the label ends in `,`, and the whole year line joins
/// the span.
#[test]
fn a_header_band_does_not_swallow_the_year_row_under_it() {
    const LABEL: &str = "Year Ended December 31,";
    const STUBS: [&str; 3] = ["Tractors", "Trailers", "Total rocks"];
    const FIGS: [[&str; 3]; 3] = [
        ["185,209", "123,152", "114,166"],
        ["98,835", "37,424", "94,040"],
        ["284,044", "160,576", "208,206"],
    ];
    let mut pieces = vec![(LABEL.to_string(), true, 265.0, TOP)];
    let year_y = TOP - PITCH;
    pieces.push(("(In thousands)".to_string(), true, COLUMNS[0], year_y));
    for (c, year) in ["2018", "2017", "2016"].iter().enumerate() {
        pieces.push((year.to_string(), true, 225.0 + c as f64 * 100.0, year_y));
    }
    for (r, stub) in STUBS.iter().enumerate() {
        let y = TOP - (r as f64 + 2.0) * PITCH;
        pieces.push((stub.to_string(), false, COLUMNS[0], y));
        for (c, fig) in FIGS[r].iter().enumerate() {
            let x = 225.0 + c as f64 * 100.0;
            pieces.push(("$".to_string(), false, x - 25.0, y));
            pieces.push((fig.to_string(), false, x, y));
        }
    }
    let blocks = blocks_of(&pieces);
    let rows = rows_of(&blocks);
    let row0: Vec<_> = rows.iter().filter(|(r, ..)| *r == 0).collect();
    assert_eq!(row0.len(), 1, "the label is one spanning cell: {rows:?}");
    assert_eq!(row0[0].4, LABEL, "the span holds only the label: {rows:?}");
    assert!(
        row0[0].2 >= 2,
        "the label spans the columns it heads: {rows:?}"
    );
    let years: Vec<String> = rows
        .iter()
        .filter(|(r, ..)| *r == 1)
        .map(|(_, _, _, _, text)| text.clone())
        .collect();
    for year in ["2018", "2017", "2016"] {
        assert!(
            years.iter().any(|t| t == year),
            "the year row keeps {year} as its own cell: {rows:?}"
        );
    }
    let last: u16 = rows.iter().map(|(r, ..)| *r).max().unwrap_or(0);
    assert_eq!(last, 4, "label, year row, three body rows: {rows:?}");
}

/// A currency column under a header span still joins its figures (#574).
///
/// CMG_2010 p56 t0 in miniature: `For the years ended December 31` centred
/// over a stub and three figure groups, each a `$` glyph set apart from its
/// figure, a year row, three body rows. The writer's currency join folds each
/// `$` column into its figures, and the full-width header span covers the
/// same columns -- a span that contains both the currency column and the next
/// one must not block the join, it only shrinks with it. Asserted on the
/// markdown, where the join lives: a header over the three figure columns,
/// `$ 114,750`-style body cells, no bare `$` column. Red before the fix as a
/// cs7 header over bare `$` columns.
#[test]
fn a_currency_column_under_a_header_span_still_joins_its_figures() {
    const LABEL: &str = "For the years ended December 31";
    const STUBS: [&str; 3] = ["Net sales", "Cost of sales", "Gross profit"];
    const FIGS: [[&str; 3]; 3] = [
        ["114,750", "101,029", "90,547"],
        ["86,133", "74,920", "66,610"],
        ["28,617", "26,109", "23,937"],
    ];
    let mut pieces = vec![(LABEL.to_string(), true, 265.0, TOP)];
    let year_y = TOP - PITCH;
    pieces.push(("(In thousands)".to_string(), true, COLUMNS[0], year_y));
    for (c, year) in ["2010", "2009", "2008"].iter().enumerate() {
        pieces.push((year.to_string(), true, 225.0 + c as f64 * 100.0, year_y));
    }
    for (r, stub) in STUBS.iter().enumerate() {
        let y = TOP - (r as f64 + 2.0) * PITCH;
        pieces.push((stub.to_string(), false, COLUMNS[0], y));
        for (c, fig) in FIGS[r].iter().enumerate() {
            let x = 225.0 + c as f64 * 100.0;
            pieces.push(("$".to_string(), false, x - 25.0, y));
            pieces.push((fig.to_string(), false, x, y));
        }
    }
    let blocks = blocks_of(&pieces);
    let markdown = to_markdown(&blocks);
    let span: usize = if HEADER_SPAN_TAKES_STUB { 4 } else { 3 };
    assert!(
        markdown.contains(&format!("<th colspan=\"{span}\">{LABEL}</th>")),
        "the header spans the stub and the three joined figure columns:\n{markdown}"
    );
    for fig in ["114,750", "101,029", "90,547"] {
        assert!(
            markdown.contains(&format!("$ {fig}")),
            "the currency column joins its figure:\n{markdown}"
        );
    }
    assert!(
        !markdown.contains(">$<"),
        "no bare currency column survives:\n{markdown}"
    );
}

/// A body label wrapped onto a second line still joins the body row above:
/// the header-band guard stops at the head (#574).
#[test]
fn a_wrapped_body_label_still_joins_the_row_above() {
    let pieces = vec![
        ("Measure".to_string(), true, COLUMNS[0], TOP),
        ("Value".to_string(), true, 300.0, TOP),
        (
            "Deferred tax and".to_string(),
            false,
            COLUMNS[0],
            TOP - PITCH,
        ),
        ("1,398".to_string(), false, 300.0, TOP - PITCH),
        (
            "other liabilities".to_string(),
            false,
            COLUMNS[0],
            TOP - 2.0 * PITCH,
        ),
        ("Equity".to_string(), false, COLUMNS[0], TOP - 3.0 * PITCH),
        ("2,100".to_string(), false, 300.0, TOP - 3.0 * PITCH),
    ];
    let blocks = blocks_of(&pieces);
    let rows = rows_of(&blocks);
    assert!(
        rows.iter()
            .any(|(_, column, _, _, text)| *column == 0
                && text == "Deferred tax and other liabilities"),
        "the wrapped label joins its row: {rows:?}"
    );
    let last: u16 = rows.iter().map(|(r, ..)| *r).max().unwrap_or(0);
    assert_eq!(last, 2, "header, joined row, one body row: {rows:?}");
}

/// Two labels on one header row are two cells, not one table-wide span.
#[test]
fn two_header_labels_are_two_cells_and_no_span() {
    let mut pieces = vec![
        ("Consolidated".to_string(), true, 172.0, TOP),
        ("The Company".to_string(), true, 382.0, TOP),
    ];
    body_rows(&mut pieces);
    let blocks = blocks_of(&pieces);
    let header = header_cells(&blocks);
    assert_eq!(header.len(), 2, "two labels, two cells: {header:?}");
    assert!(
        header.iter().all(|(_, span, _)| *span < 8),
        "neither label spans the table: {header:?}"
    );
    assert_eq!(
        body_cell_count(&blocks),
        36,
        "four body rows of a stub and eight figures"
    );
}
