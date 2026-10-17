//! A table cell whose text wraps is one cell, not two halves with the rest of
//! the row between them (#434, class O-C of #389).
//!
//! A cell that does not fit its column is drawn as two lines, and the second
//! line is a text run like any other: it sits below the row, at its own cell's
//! left edge. Read line by line, the row therefore comes out as every cell's
//! first line and only then the wrapped cell's second one -- so the sentence
//! the cell holds is cut in half by the text of the cells beside it. Both
//! mutool and pdftotext emit the cell whole and then move on, which is the
//! order this fixture asserts.
//!
//! `8.2.5.26-t05-pass-d.pdf` of the PDF/UA-2 test suite is the document the
//! class is named after: its second row reads `15-003 In a table not organized
//! with Headers attributes and IDs, a UA1:7.5-2 Object Machine` and then, a
//! line later, `TH cell does not contain a Scope attribute.`
//!
//! The table below is that document's shape with its own text: three columns,
//! two rows, and the middle cell of the second row wrapping to two lines. It
//! uses Courier, whose every glyph advances 600/1000 of the font size, so the
//! cell edges below are exact rather than nearly right.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const FONT_SIZE: f64 = 10.0;
const COURIER_ADVANCE: f64 = 0.6;

/// The left edge of each column, and the two baselines of the second row.
const COLUMN_X: [f64; 3] = [60.0, 200.0, 400.0];
const HEADER_Y: f64 = 700.0;
const ROW_Y: f64 = 680.0;
const WRAP_Y: f64 = 668.0;

const HEADER: [&str; 3] = ["Index", "Failure condition", "Section"];
const ROW: [&str; 3] = ["15-003", "In a table not organized", "UA1:7.5-2"];
const WRAPPED_LINE: &str = "a cell without a Scope";

/// One line of one cell: text set at a point, in Courier.
struct Cell {
    text: &'static str,
    x: f64,
    y: f64,
}

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

/// A one-page PDF that draws every cell given, each as its own text object so
/// that the order in the content stream says nothing about the reading order.
fn table_page(cells: &[Cell]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = courier(&mut doc);

    let mut operations = Vec::new();
    for cell in cells {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), FONT_SIZE.into()],
        ));
        operations.push(Operation::new("Td", vec![cell.x.into(), cell.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(cell.text)],
        ));
        operations.push(Operation::new("ET", vec![]));
    }

    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            0.into(), 0.into(), 612.into(), 792.into(),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
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

/// The header row, the second row, and the second row's wrapped middle cell.
fn table_with_a_wrapped_middle_cell() -> Vec<Cell> {
    let mut cells = Vec::new();
    for (column, text) in HEADER.iter().enumerate() {
        cells.push(Cell {
            text,
            x: COLUMN_X[column],
            y: HEADER_Y,
        });
    }
    for (column, text) in ROW.iter().enumerate() {
        cells.push(Cell {
            text,
            x: COLUMN_X[column],
            y: ROW_Y,
        });
    }
    cells.push(Cell {
        text: WRAPPED_LINE,
        x: COLUMN_X[1],
        y: WRAP_Y,
    });
    cells
}

fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
}

/// Where a piece of text starts in the extracted output. Panics rather than
/// returning an option: a missing piece is a different failure from a
/// misplaced one, and it should say so.
fn position_of(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is missing from the extracted text: {text:?}"))
}

#[test]
fn a_wrapped_cell_reads_as_one_cell() {
    let text = text_of(table_page(&table_with_a_wrapped_middle_cell()));

    let first_line = position_of(&text, ROW[1]);
    let second_line = position_of(&text, WRAPPED_LINE);
    let next_cell = position_of(&text, ROW[2]);

    assert!(
        first_line < second_line,
        "the cell's two lines are out of order: {text:?}"
    );
    assert!(
        second_line < next_cell,
        "the cell's second line is behind the rest of the row: {text:?}"
    );
    // Adjacent, not merely in order: nothing of the row may sit between the two
    // lines of one cell.
    let between = &text[first_line + ROW[1].len()..second_line];
    assert!(
        between.trim().is_empty(),
        "the cell's two lines are split by {between:?}: {text:?}"
    );
}

#[test]
fn the_row_before_the_wrapped_one_is_untouched() {
    let text = text_of(table_page(&table_with_a_wrapped_middle_cell()));

    let header_positions: Vec<usize> = HEADER.iter().map(|cell| position_of(&text, cell)).collect();
    assert!(
        header_positions.windows(2).all(|pair| pair[0] < pair[1]),
        "the header row is no longer read left to right: {text:?}"
    );
    assert!(
        header_positions[2] < position_of(&text, ROW[0]),
        "the header row is no longer read before the row below it: {text:?}"
    );
}

#[test]
fn a_table_whose_cells_all_fit_stays_row_major() {
    // The same table without the wrap. Every row is one line, and a row read
    // cell by cell would read the whole first column before the second one --
    // which is what the guard against reordering a plain table is for.
    let mut cells = table_with_a_wrapped_middle_cell();
    cells.pop();
    let text = text_of(table_page(&cells));

    assert!(
        position_of(&text, HEADER[1]) < position_of(&text, ROW[0]),
        "a table whose cells all fit is no longer read row by row: {text:?}"
    );
}

#[test]
fn the_fixture_draws_cells_that_fit_their_column() {
    // A cell that overran its column would be a different document from the one
    // this file claims to test: the reordering is about a cell that wraps
    // because it does not fit, not about text that crosses a column edge.
    for (column, text) in ROW.iter().enumerate().chain([(1usize, &WRAPPED_LINE)]) {
        let right = COLUMN_X[column] + text.chars().count() as f64 * FONT_SIZE * COURIER_ADVANCE;
        let edge = COLUMN_X.get(column + 1).copied().unwrap_or(f64::INFINITY);
        assert!(
            right < edge,
            "{text:?} reaches {right} pt, past the column edge at {edge} pt"
        );
    }
}

/// Three runs on a line and a short line under them is also what a paragraph
/// looks like when two words of it are set in another font. Nothing there is a
/// grid -- the line above them is a paragraph of its own -- and reordering it
/// would move the rest of the first line behind the second one, the class O-B
/// mistake of #389 made on a single paragraph.
const PROSE_INTRO: &str = "An introduction that runs the whole width";
const PROSE_LEAD: &str = "Short lead";
const PROSE_MIDDLE: &str = "a middle";
const PROSE_TAIL: &str = "and a tail";
const PROSE_SECOND_LINE: &str = "runs on";

fn a_paragraph_whose_first_line_is_set_in_three_runs() -> Vec<Cell> {
    vec![
        Cell {
            text: PROSE_INTRO,
            x: COLUMN_X[0],
            y: HEADER_Y + 20.0,
        },
        Cell {
            text: PROSE_LEAD,
            x: COLUMN_X[0],
            y: HEADER_Y,
        },
        Cell {
            text: PROSE_MIDDLE,
            x: COLUMN_X[1],
            y: HEADER_Y,
        },
        Cell {
            text: PROSE_TAIL,
            x: COLUMN_X[2],
            y: HEADER_Y,
        },
        Cell {
            text: PROSE_SECOND_LINE,
            x: COLUMN_X[0],
            y: HEADER_Y - 12.0,
        },
    ]
}

#[test]
fn prose_that_looks_like_a_row_is_not_reordered() {
    // The introduction crosses the edges the line under it draws, so no line of
    // this page corroborates a grid -- which is the only thing that separates
    // this page from the table above.
    let intro_right =
        COLUMN_X[0] + PROSE_INTRO.chars().count() as f64 * FONT_SIZE * COURIER_ADVANCE;
    assert!(
        intro_right > COLUMN_X[1],
        "the introduction no longer crosses a cell edge: it ends at {intro_right} pt"
    );

    let text = text_of(table_page(
        &a_paragraph_whose_first_line_is_set_in_three_runs(),
    ));

    assert!(
        position_of(&text, PROSE_MIDDLE) < position_of(&text, PROSE_SECOND_LINE),
        "the first line's middle run was moved behind the line under it: {text:?}"
    );
    assert!(
        position_of(&text, PROSE_TAIL) < position_of(&text, PROSE_SECOND_LINE),
        "the first line's tail was moved behind the line under it: {text:?}"
    );
}
