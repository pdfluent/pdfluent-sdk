//! A table is a column grid the whole run agrees on, and its cells are what
//! fills it (#570).
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, row detection, grid, serializer. Take the
//! column consensus out of `crate::tables` and the first four go red the same
//! way -- the rows come back as paragraphs with their cells run together, which
//! is the reading #566 measured at 0.000 against Docling's 0.553. Take the span
//! detection out and the two that need HTML come back as a rectangle that drops
//! the merges the document drew.
//!
//! The failure cases the ticket names each have a case of their own:
//! borderless tables, a table on two pages, a nested table, numbers set flush
//! right, a header that is bold and nothing else, a form whose widgets look
//! like a table, and prose that must never become one.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
/// One row to the next. Well over the type size, which is what a table is set
/// on and what keeps the rows apart.
const PITCH: f64 = 18.0;

/// The face a piece of text is set in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Regular,
    Bold,
}

/// One run of glyphs at a point.
struct Piece {
    text: String,
    face: Face,
    x: f64,
    y: f64,
}

/// A horizontal rule the page draws: a filled rectangle a third of a point
/// high, which is how a document rules a table.
struct Rule {
    left: f64,
    right: f64,
    y: f64,
}

fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        face: Face::Regular,
        x,
        y,
    }
}

fn bold(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        face: Face::Bold,
        x,
        y,
    }
}

/// One row of a table: the cells at the column positions given.
fn row(cells: &[(&str, f64)], y: f64, face: Face) -> Vec<Piece> {
    cells
        .iter()
        .map(|(body, x)| match face {
            Face::Bold => bold(body, *x, y),
            Face::Regular => text(body, *x, y),
        })
        .collect()
}

/// A rule the width of the table, drawn over the row at `row`: between that
/// row's glyphs and the ones above it, which is where a document rules a table
/// and where the row detection looks for one.
fn rule_over(row: usize) -> Rule {
    Rule {
        left: MARGIN - 6.0,
        right: 460.0,
        y: TOP - row as f64 * PITCH + 11.0,
    }
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

/// The content stream of one page: its rules first, its text over them.
fn stream_of(pieces: &[Piece], rules: &[Rule]) -> Vec<u8> {
    let mut operations = Vec::new();
    for drawn in rules {
        operations.push(Operation::new(
            "re",
            vec![
                drawn.left.into(),
                drawn.y.into(),
                (drawn.right - drawn.left).into(),
                0.4.into(),
            ],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(
                    match piece.face {
                        Face::Regular => b"F1",
                        Face::Bold => b"F2",
                    }
                    .to_vec(),
                ),
                BODY_SIZE.into(),
            ],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    Content { operations }.encode().expect("encode content")
}

/// A PDF of one page per entry, each with its own pieces and rules.
fn pdf_of_pages(pages: &[(&[Piece], &[Rule])]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let heavy = font(&mut doc, b"Helvetica-Bold");

    let mut page_ids = Vec::new();
    for (pieces, rules) in pages {
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            stream_of(pieces, rules),
        )));
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => regular.clone(),
                    "F2" => heavy.clone(),
                }),
            }),
        })));
    }

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>()),
        "Count" => Object::Integer(page_ids.len() as i64),
    }));
    for page_id in &page_ids {
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(*page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
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

/// The same page, with AcroForm text fields over it: widgets with a value and
/// no appearance stream, which is the shape a filled form has.
fn pdf_of_pages_with_widgets(pieces: &[Piece], rules: &[Rule], fields: &[(&str, f64)]) -> Vec<u8> {
    let mut doc = Document::load_mem(&pdf_of_pages(&[(pieces, rules)])).expect("reload pdf");
    let page_id = doc.page_iter().next().expect("one page");

    let mut widget_ids = Vec::new();
    for (i, (name, x)) in fields.iter().enumerate() {
        let y = TOP - i as f64 * PITCH;

        widget_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
            "Rect" => Object::Array(vec![(*x).into(), y.into(), (x + 100.0).into(), (y + 12.0).into()]),
            "P" => Object::Reference(page_id),
        })));
    }
    let annots: Vec<Object> = widget_ids.iter().copied().map(Object::Reference).collect();
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Annots", Object::Array(annots.clone()));
    }
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .expect("catalog");
    if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(catalog_id) {
        catalog.set(
            "AcroForm",
            Object::Dictionary(dictionary! { "Fields" => Object::Array(annots) }),
        );
    }

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic form");
    buf
}

fn blocks_of(pieces: &[Piece], rules: &[Rule]) -> Vec<Block> {
    PdfDocument::open(pdf_of_pages(&[(pieces, rules)]))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// Every cell of the blocks, as `(row, column, row_span, column_span, header,
/// text)`: the whole of what the grid claims, in one comparable shape.
fn cells(blocks: &[Block]) -> Vec<(u16, u16, u16, u16, bool, &str)> {
    blocks
        .iter()
        .filter_map(|block| match block.kind {
            BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                header,
            } => Some((
                row,
                column,
                row_span,
                column_span,
                header,
                block.text.as_str(),
            )),
            _ => None,
        })
        .collect()
}

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

/// Three columns and three rows, nothing ruled: the commonest table there is,
/// and the one a width heuristic cannot see at all.
fn borderless() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[("Region", MARGIN), ("Amount", 240.0), ("Share", 360.0)],
        TOP,
        Face::Bold,
    ));
    pieces.extend(row(
        &[("North", MARGIN), ("120", 240.0), ("41%", 360.0)],
        TOP - PITCH,
        Face::Regular,
    ));
    pieces.extend(row(
        &[("South", MARGIN), ("95", 240.0), ("32%", 360.0)],
        TOP - 2.0 * PITCH,
        Face::Regular,
    ));
    pieces
}

#[test]
fn a_borderless_table_is_a_grid_of_cells() {
    let blocks = blocks_of(&borderless(), &[]);
    assert_eq!(
        cells(&blocks),
        vec![
            (0, 0, 1, 1, true, "Region"),
            (0, 1, 1, 1, true, "Amount"),
            (0, 2, 1, 1, true, "Share"),
            (1, 0, 1, 1, false, "North"),
            (1, 1, 1, 1, false, "120"),
            (1, 2, 1, 1, false, "41%"),
            (2, 0, 1, 1, false, "South"),
            (2, 1, 1, 1, false, "95"),
            (2, 2, 1, 1, false, "32%"),
        ],
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn a_rectangle_without_spans_is_a_markdown_table() {
    assert_eq!(
        to_markdown(&blocks_of(&borderless(), &[])),
        "| Region | Amount | Share |\n\
         | --- | --- | --- |\n\
         | North | 120 | 41% |\n\
         | South | 95 | 32% |\n"
    );
}

/// The column a cell is in is the TABLE's column, not its own row's. A row
/// whose middle cell is empty still puts its last cell in the third column,
/// which is the whole reason the grid is computed once for the run.
#[test]
fn an_empty_cell_does_not_shift_the_column_after_it() {
    let mut pieces = borderless();
    pieces.extend(row(
        &[("East", MARGIN), ("27%", 360.0)],
        TOP - 3.0 * PITCH,
        Face::Regular,
    ));
    let blocks = blocks_of(&pieces, &[]);
    let last: Vec<_> = cells(&blocks)
        .into_iter()
        .filter(|cell| cell.0 == 3)
        .collect();
    assert_eq!(
        last,
        vec![(3, 0, 1, 1, false, "East"), (3, 2, 1, 1, false, "27%")],
        "{:?}",
        cells(&blocks)
    );
}

/// An HTML row counts its columns from the cells before it, so a row with a
/// hole in it has to say so. Otherwise `27%` -- the third column's figure --
/// comes out under the second column's heading in every reader (codex,
/// round 1).
#[test]
fn a_hole_in_a_row_keeps_its_place_in_the_html() {
    let mut pieces = borderless();
    pieces.extend(row(
        &[("East", MARGIN), ("27%", 360.0)],
        TOP - 3.0 * PITCH,
        Face::Regular,
    ));
    let markdown = to_markdown(&blocks_of(&pieces, &[]));
    assert!(
        markdown.contains("<tr><td>East</td><td></td><td>27%</td></tr>"),
        "the hole was dropped and the figure moved a column left: {markdown}"
    );
}

/// A cell whose words cross a boundary the rest of the table keeps is a cell
/// that SPANS it -- and a table with a span in it cannot be written as a
/// markdown rectangle, so it comes out as HTML.
#[test]
fn a_heading_over_two_columns_is_a_column_span() {
    let mut pieces = row(
        &[("Region", MARGIN), ("Reported figures", 240.0)],
        TOP,
        Face::Bold,
    );
    for (i, (region, amount, share)) in [
        ("North", "120", "41%"),
        ("South", "95", "32%"),
        ("East", "78", "27%"),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(region, MARGIN), (amount, 240.0), (share, 360.0)],
            TOP - (i as f64 + 1.0) * PITCH,
            Face::Regular,
        ));
    }

    let blocks = blocks_of(&pieces, &[]);
    let spanning: Vec<_> = cells(&blocks)
        .into_iter()
        .filter(|cell| cell.3 > 1)
        .collect();
    assert_eq!(
        spanning,
        vec![(0, 1, 1, 2, true, "Reported figures")],
        "{:?}",
        cells(&blocks)
    );

    let markdown = to_markdown(&blocks);
    assert!(
        markdown.starts_with("<table>") && markdown.contains(r#"<th colspan="2">"#),
        "a spanning table has to be written as HTML: {markdown}"
    );
    assert!(
        markdown.contains("<td>North</td><td>120</td><td>41%</td>"),
        "{markdown}"
    );
}

/// A rule between two rows runs the width of the table and stops over the
/// columns whose cells the document merged. That break is the only evidence a
/// row span has, and it is read from nowhere else.
///
/// THE BROKEN RULE IS TWO PIECES, which is how a document draws one: the
/// separator stops at the merged cell and starts again after it, and
/// `record_rule` stores each subpath on its own. A reading that asks one piece
/// to cross half the table finds no rule at all on exactly the tables this
/// case is about (codex, round 1).
#[test]
fn a_rule_that_breaks_over_a_column_is_a_row_span() {
    let mut pieces = row(
        &[("Party", MARGIN), ("Clause", 240.0), ("Effect", 360.0)],
        TOP,
        Face::Bold,
    );
    pieces.extend(row(
        &[("Buyer", MARGIN), ("4.1", 240.0), ("payment", 360.0)],
        TOP - PITCH,
        Face::Regular,
    ));
    // The clause cell is merged down: this row draws nothing in it.
    pieces.extend(row(
        &[("Seller", MARGIN), ("delivery", 360.0)],
        TOP - 2.0 * PITCH,
        Face::Regular,
    ));
    pieces.extend(row(
        &[("Agent", MARGIN), ("5.1", 240.0), ("notice", 360.0)],
        TOP - 3.0 * PITCH,
        Face::Regular,
    ));

    let broken = TOP - 2.0 * PITCH + 11.0;
    let rules = vec![
        rule_over(1),
        Rule {
            left: MARGIN - 6.0,
            right: 171.0,
            y: broken,
        },
        Rule {
            left: 317.0,
            right: 460.0,
            y: broken,
        },
        rule_over(3),
    ];

    let blocks = blocks_of(&pieces, &rules);
    let merged: Vec<_> = cells(&blocks)
        .into_iter()
        .filter(|cell| cell.2 > 1)
        .collect();
    assert_eq!(
        merged,
        vec![(1, 1, 2, 1, false, "4.1")],
        "{:?}",
        cells(&blocks)
    );
    assert!(
        to_markdown(&blocks).contains(r#"<td rowspan="2">4.1</td>"#),
        "{}",
        to_markdown(&blocks)
    );
}

/// The header is what the document said it was. A first row set bold over a
/// body that is not is one of the two ways it says so; the other is a rule
/// under the first row and nowhere else.
#[test]
fn a_first_row_ruled_off_from_the_body_is_the_header() {
    let mut pieces = row(
        &[("Region", MARGIN), ("Amount", 240.0), ("Share", 360.0)],
        TOP,
        Face::Regular,
    );
    for (i, (region, amount, share)) in [("North", "120", "41%"), ("South", "95", "32%")]
        .iter()
        .enumerate()
    {
        pieces.extend(row(
            &[(region, MARGIN), (amount, 240.0), (share, 360.0)],
            TOP - (i as f64 + 1.0) * PITCH,
            Face::Regular,
        ));
    }

    let unruled = blocks_of(&pieces, &[]);
    assert!(
        cells(&unruled).iter().all(|cell| !cell.4),
        "nothing said header: {:?}",
        cells(&unruled)
    );

    let ruled = blocks_of(&pieces, &[rule_over(1)]);
    assert!(
        cells(&ruled)
            .iter()
            .all(|cell| cell.4 == (cell.0 == 0) && cell.0 < 3),
        "the rule under the first row is what makes it the header: {:?}",
        cells(&ruled)
    );
}

/// Figures set flush right sit at three different left edges and one right
/// edge. The strip of white to the left of them still runs through the table,
/// so they are one column.
#[test]
fn a_column_set_flush_right_is_still_one_column() {
    let right = |body: &str, y: f64| {
        // Helvetica's digits are half an em wide, so this puts every figure's
        // last glyph on the same x.
        text(
            body,
            340.0 - BODY_SIZE * 0.5 * body.chars().count() as f64,
            y,
        )
    };
    let mut pieces = row(&[("Region", MARGIN), ("Amount", 300.0)], TOP, Face::Bold);
    for (i, (region, amount)) in [("North", "7"), ("South", "1,204"), ("East", "96")]
        .iter()
        .enumerate()
    {
        let y = TOP - (i as f64 + 1.0) * PITCH;
        pieces.push(text(region, MARGIN, y));
        pieces.push(right(amount, y));
    }

    let blocks = blocks_of(&pieces, &[]);
    assert!(
        cells(&blocks).iter().all(|cell| cell.1 < 2 && cell.3 == 1),
        "the figures made a column each: {:?}",
        cells(&blocks)
    );
    assert_eq!(
        cells(&blocks)
            .iter()
            .filter(|cell| cell.1 == 1)
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec!["Amount", "7", "1,204", "96"]
    );
}

/// Prose is never a table. Justified setting stretches a word space past an em
/// and a wide gap lands where another line has one now and then; neither makes
/// a strip of white that runs through the whole of it.
#[test]
fn running_text_is_never_a_table() {
    let lines = [
        "The committee considered the proposals put before it",
        "in the order that they had been received, and resolved",
        "that each of them be referred to the officers for a",
        "report before the next ordinary meeting of the board.",
    ];
    let pieces: Vec<Piece> = lines
        .iter()
        .enumerate()
        .map(|(i, body)| text(body, MARGIN, TOP - i as f64 * 12.0))
        .collect();
    let blocks = blocks_of(&pieces, &[]);
    assert!(
        cells(&blocks).is_empty(),
        "prose came back as a table: {:?}",
        shape(&blocks)
    );
}

/// A table inside a cell is the cell's text. Nothing recurses: the inner rows
/// are lines of the outer cell, and they are joined the way the lines of a
/// paragraph are, because a grid inside a grid is a shape neither markdown nor
/// the block model has.
#[test]
fn a_table_inside_a_cell_is_that_cell_s_text() {
    /// The table's own row pitch, wide enough for a cell to wrap inside it.
    const CELL_PITCH: f64 = 28.0;

    let mut pieces = row(
        &[("Item", MARGIN), ("Detail", 240.0), ("Term", 380.0)],
        TOP,
        Face::Bold,
    );
    for (i, (item, detail, term)) in [
        ("Schedule", "first 12", "annual"),
        ("Notice", "thirty days", "written"),
        ("Renewal", "one year", "automatic"),
        ("Audit", "on request", "annual"),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(item, MARGIN), (detail, 240.0), (term, 380.0)],
            TOP - (i as f64 + 1.0) * CELL_PITCH,
            Face::Regular,
        ));
    }
    // The inner table's second row, set inside the first body row's middle
    // cell -- nearer the cell it belongs to than the row under it, which is
    // what a wrapped cell looks like and what tells the two apart.
    pieces.push(text("second 34", 240.0, TOP - CELL_PITCH - 11.0));

    let blocks = blocks_of(&pieces, &[]);
    let inner: Vec<_> = cells(&blocks)
        .into_iter()
        .filter(|cell| cell.5.contains("first"))
        .collect();
    assert_eq!(
        inner,
        vec![(1, 1, 1, 1, false, "first 12 second 34")],
        "{:?}",
        cells(&blocks)
    );
    assert_eq!(
        cells(&blocks)
            .into_iter()
            .filter(|cell| cell.0 == 2)
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec!["Notice", "thirty days", "written"],
        "the row after the wrapped cell moved: {:?}",
        cells(&blocks)
    );
}

/// A table under a paragraph keeps its rows, wrapped cell and all.
///
/// The row pitch used to be the median of every baseline step in the REGION,
/// and a page like this one has more prose lines than table rows: the median
/// is then the prose leading, which on a table set on a wider pitch is smaller
/// than the wrapped line the row has to admit (codex, round 1). The pitch is
/// now taken over the tabular lines only.
///
/// THIS CASE DOES NOT GO RED ON THAT CHANGE ALONE, and it is written down
/// rather than claimed otherwise: on these numbers `crate::text` folds the
/// wrapped cell into its row before a pitch is asked for at all (#434), so
/// both readings agree here. What it does hold is the shape -- a table under
/// prose, read as two rows of three -- which is what the region's pitch broke
/// on the page the finding describes.
#[test]
fn a_table_under_a_paragraph_keeps_its_rows() {
    const PROSE_LEADING: f64 = 12.0;
    const WIDE_PITCH: f64 = 32.0;

    let mut pieces: Vec<Piece> = [
        "The committee considered the proposals put before it in",
        "the order that they had been received, and resolved that",
        "each of them be referred to the officers for a report",
        "before the next ordinary meeting of the board.",
    ]
    .iter()
    .enumerate()
    .map(|(i, body)| text(body, MARGIN, TOP + 40.0 - i as f64 * PROSE_LEADING))
    .collect();

    let first = TOP - 16.0;
    for (i, (item, detail, term)) in [
        ("Item", "Detail", "Term"),
        ("Notice", "thirty days", "written"),
        ("Renewal", "one year", "automatic"),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(item, MARGIN), (detail, 240.0), (term, 380.0)],
            first - i as f64 * WIDE_PITCH,
            Face::Regular,
        ));
    }
    // The middle cell of the second row wraps, one prose leading below it --
    // well inside the table's own pitch and well outside the region's median.
    pieces.push(text(
        "and one week",
        240.0,
        first - WIDE_PITCH - PROSE_LEADING,
    ));

    // THREE ROWS, since #574. The first line of the table, `Item Detail
    // Term`, is set exactly like the two under it, and it used to be trimmed
    // off as the tail of the prose above: the table and the paragraph shared
    // one region of the cut. The table's region is now found before the cut,
    // so the paragraph is not in it and nothing is trimmed.
    let blocks = blocks_of(&pieces, &[]);
    assert_eq!(
        cells(&blocks),
        vec![
            (0, 0, 1, 1, false, "Item"),
            (0, 1, 1, 1, false, "Detail"),
            (0, 2, 1, 1, false, "Term"),
            (1, 0, 1, 1, false, "Notice"),
            (1, 1, 1, 1, false, "thirty days and one week"),
            (1, 2, 1, 1, false, "written"),
            (2, 0, 1, 1, false, "Renewal"),
            (2, 1, 1, 1, false, "one year"),
            (2, 2, 1, 1, false, "automatic"),
        ],
        "the wrapped cell took its row apart: {:?}",
        shape(&blocks)
    );
}

/// A table that runs over a page break is a table on each page. The grid is a
/// statement about a column of a page, and the second page's rows stand in
/// their own.
#[test]
fn a_table_on_two_pages_is_a_grid_on_each() {
    let first = borderless();
    let mut second = Vec::new();
    for (i, (region, amount, share)) in [("East", "78", "27%"), ("West", "63", "22%")]
        .iter()
        .enumerate()
    {
        second.extend(row(
            &[(region, MARGIN), (amount, 240.0), (share, 360.0)],
            TOP - i as f64 * PITCH,
            Face::Regular,
        ));
    }

    let blocks = PdfDocument::open(pdf_of_pages(&[
        (first.as_slice(), [].as_slice()),
        (second.as_slice(), [].as_slice()),
    ]))
    .expect("open pdf")
    .extract_blocks(&ExtractOptions::default())
    .expect("extract blocks");

    let pages: Vec<usize> = blocks
        .iter()
        .filter(|block| matches!(block.kind, BlockKind::TableCell { .. }))
        .map(|block| block.page)
        .collect();
    assert_eq!(
        pages,
        vec![0; 9].into_iter().chain(vec![1; 6]).collect::<Vec<_>>()
    );
    assert_eq!(
        cells(&blocks)
            .into_iter()
            .filter(|cell| cell.5 == "West")
            .collect::<Vec<_>>(),
        vec![(1, 0, 1, 1, false, "West")]
    );
}

/// The blocks are numbered in one run over the document, and a table does not
/// break that: its cells take the indices between the block before it and the
/// block after.
#[test]
fn the_cells_are_numbered_with_everything_else() {
    let mut pieces = vec![bold("Regional figures", MARGIN, TOP + 2.0 * PITCH)];
    pieces.extend(borderless());
    let blocks = blocks_of(&pieces, &[]);
    assert_eq!(
        blocks.iter().map(|block| block.index).collect::<Vec<_>>(),
        (0..blocks.len()).collect::<Vec<_>>(),
        "{:?}",
        shape(&blocks)
    );
    assert!(
        !matches!(blocks[0].kind, BlockKind::TableCell { .. }),
        "the title above the table became a cell: {:?}",
        shape(&blocks)
    );
}

/// A form that looks like a table is a table of its LABELS, and the widgets
/// stay widgets. A field draws no glyph into the content stream, so its value
/// is not a span and cannot be a cell; the labels beside it are laid out as a
/// table and come back as one, which is the right answer for them.
#[test]
fn a_form_s_widgets_are_not_cells() {
    let pieces = borderless();

    // Three empty fields, one beside each label row: boxes a reader sees and
    // the text layer does not.
    let blocks = PdfDocument::open(pdf_of_pages_with_widgets(
        &pieces,
        &[],
        &[("given", 460.0), ("family", 460.0), ("born", 460.0)],
    ))
    .expect("open pdf")
    .extract_blocks(&ExtractOptions::default())
    .expect("extract blocks");

    assert_eq!(
        cells(&blocks)
            .into_iter()
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec!["Region", "Amount", "Share", "North", "120", "41%", "South", "95", "32%",],
        "{:?}",
        shape(&blocks)
    );
    assert!(
        cells(&blocks).iter().all(|cell| cell.1 < 3),
        "the empty fields became a fourth column: {:?}",
        cells(&blocks)
    );
}

// ---------------------------------------------------------------------------
// The shapes #576 found in the worst five per cent of the DocLayNet and
// PubTables-1M rounds: a table broken at every section head, a table of
// descriptions read down its columns, and the rows of one read as fragments.
// ---------------------------------------------------------------------------

/// A table with a head set across it between each bank of rows -- `Geographic
/// locations`, `Cancer subtype` -- which is the commonest furniture inside a
/// table and the shape of `PMC4845003-p0005`, where one table came out as
/// fifteen.
fn banked() -> Vec<Piece> {
    let columns = [MARGIN, 240.0, 340.0, 430.0];
    let mut pieces = row(
        &[
            ("Subgroup", columns[0]),
            ("Site", columns[1]),
            ("Cases", columns[2]),
            ("Studies", columns[3]),
        ],
        TOP,
        Face::Bold,
    );
    let mut at = 1.0;
    for (head, bank) in [
        (
            "Geographic locations of the studies",
            [
                ("Europe", "Esophageal", "645", "2"),
                ("America", "Esophageal", "4558", "5"),
            ],
        ),
        (
            "Cancer subtype of the studies",
            [
                ("ESCC", "Esophageal", "827", "4"),
                ("EAC", "Esophageal", "435", "2"),
            ],
        ),
    ] {
        pieces.push(text(head, MARGIN, TOP - at * PITCH));
        at += 1.0;
        for cells in bank {
            pieces.extend(row(
                &[
                    (cells.0, columns[0]),
                    (cells.1, columns[1]),
                    (cells.2, columns[2]),
                    (cells.3, columns[3]),
                ],
                TOP - at * PITCH,
                Face::Regular,
            ));
            at += 1.0;
        }
        // A row of data with only its first column filled, which is an
        // ordinary thing for a table to hold and is not a head set across it.
        pieces.push(text("Other", columns[0], TOP - at * PITCH));
        at += 1.0;
        pieces.extend(row(
            &[
                ("Total", columns[0]),
                ("All", columns[1]),
                ("5205", columns[2]),
                ("7", columns[3]),
            ],
            TOP - at * PITCH,
            Face::Regular,
        ));
        at += 1.0;
    }
    pieces
}

#[test]
fn a_section_head_inside_a_table_is_a_row_of_it() {
    let blocks = blocks_of(&banked(), &[]);
    let rows: Vec<(u16, u16, u16, &str)> = cells(&blocks)
        .into_iter()
        .map(|cell| (cell.0, cell.1, cell.3, cell.5))
        .collect();
    assert_eq!(
        rows.iter().map(|cell| cell.0).max(),
        Some(10),
        "the table broke at a section head: {:?}",
        shape(&blocks)
    );
    assert!(
        rows.contains(&(1, 0, 4, "Geographic locations of the studies"))
            && rows.contains(&(6, 0, 4, "Cancer subtype of the studies")),
        "a head set across the table is not a cell of its own row, as wide as \
         the table: {:?}",
        rows
    );
    assert!(
        rows.contains(&(4, 0, 1, "Other")) && rows.contains(&(9, 0, 1, "Other")),
        "a sparse row of data was read as a head set across the table: {:?}",
        rows
    );
    assert!(
        rows.contains(&(2, 0, 1, "Europe")) && rows.contains(&(10, 3, 1, "7")),
        "the banks either side of a head are not one grid: {:?}",
        rows
    );
}

/// The same page with the second bank set apart under a caption of its own,
/// which is what a SECOND table looks like. The rule that lets a section head
/// through must not let this through, or one table swallows the next.
#[test]
fn a_caption_between_two_tables_does_not_join_them() {
    let columns = [MARGIN, 240.0, 340.0, 430.0];
    let bank = |at: f64, rows: [(&str, &str, &str, &str); 2]| {
        let mut pieces = Vec::new();
        for (i, cells) in rows.iter().enumerate() {
            pieces.extend(row(
                &[
                    (cells.0, columns[0]),
                    (cells.1, columns[1]),
                    (cells.2, columns[2]),
                    (cells.3, columns[3]),
                ],
                TOP - (at + i as f64) * PITCH,
                Face::Regular,
            ));
        }
        pieces
    };
    let mut pieces = bank(
        0.0,
        [
            ("Europe", "Esophageal", "645", "2"),
            ("America", "Esophageal", "4558", "5"),
        ],
    );
    // Two clear lines of white, a caption, and two more: the page setting the
    // next table apart, which is what a caption is for.
    pieces.push(text(
        "Table 2 The same by subtype",
        MARGIN,
        TOP - 4.0 * PITCH,
    ));
    pieces.extend(bank(
        6.0,
        [
            ("ESCC", "Esophageal", "827", "4"),
            ("EAC", "Esophageal", "435", "2"),
        ],
    ));

    let blocks = blocks_of(&pieces, &[]);
    assert!(
        shape(&blocks)
            .iter()
            .any(|(kind, text)| !matches!(kind, BlockKind::TableCell { .. })
                && text.contains("Table 2")),
        "a caption set apart was swallowed into the table above it: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        cells(&blocks)
            .iter()
            .filter(|cell| cell.0 == 0)
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec![
            "Europe",
            "Esophageal",
            "645",
            "2",
            "ESCC",
            "Esophageal",
            "827",
            "4"
        ],
        "the two tables are not two: {:?}",
        cells(&blocks)
    );
}

/// The leading a description wraps at, and the step between two rows of it.
/// Their ratio is what tells one from the other.
const CELL_LEADING: f64 = 12.0;

/// A page of glyph samples laid out in a staggered grid: every line sits one
/// leading under the last, and every line opens cells of its own at x nothing
/// above it uses. Those are lines, not the wrapped continuations of the row
/// above, and reading them as one row turns a font test page into a
/// twenty-column table (PDFBOX-5784 of the extraction gate set).
#[test]
fn a_line_that_opens_its_own_cells_is_not_a_wrapped_one() {
    let mut pieces = Vec::new();
    for step in 0..9 {
        let y = TOP - step as f64 * CELL_LEADING;
        // Every third line starts a whole pattern-width further right, and
        // the lines between are one leading apart -- two kinds of step, which
        // is what a wrapped cell also shows.
        let shift = (step % 3) as f64 * 60.0;
        let mut on_the_line: Vec<(&str, f64)> = Vec::new();
        for (i, glyph) in ["W", "I", "a", "q", "g"].iter().enumerate() {
            on_the_line.push((glyph, MARGIN + shift + i as f64 * 20.0));
        }
        pieces.extend(row(&on_the_line, y, Face::Regular));
    }
    // One wider step, so the run has two clusters of step in it.
    for (i, glyph) in ["W", "I", "a", "q", "g"].iter().enumerate() {
        pieces.push(text(
            glyph,
            MARGIN + i as f64 * 20.0,
            TOP - 9.0 * CELL_LEADING - 20.0,
        ));
    }

    let blocks = blocks_of(&pieces, &[]);
    assert!(
        cells(&blocks).is_empty(),
        "staggered lines of glyphs were folded into rows of a table: {:?}",
        shape(&blocks)
    );
}
const DESCRIPTION_PITCH: f64 = 20.0;

/// A table of descriptions: a narrow column of names against a wide column of
/// sentences, each row several lines deep. `PMC6019521-p0002` is the page, and
/// with nothing ruled it is the commonest table in a scientific paper.
fn descriptions() -> Vec<Piece> {
    let columns = [MARGIN, 200.0, 420.0];
    let mut pieces = row(
        &[
            ("Element", columns[0]),
            ("Description", columns[1]),
            ("Systems", columns[2]),
        ],
        TOP,
        Face::Bold,
    );
    let mut y = TOP - CELL_LEADING - 6.0;
    for bank in [
        [
            (
                "Community",
                "Providers connecting patients to community",
                "Health-related social",
            ),
            (
                "resources",
                "programs through partnerships that expand",
                "networks that support",
            ),
            (
                "",
                "health system services beyond primary care",
                "health and social care.",
            ),
            ("", "and promote self-help strategies for all.", ""),
        ],
        [
            (
                "Health system",
                "Designing health systems to support the",
                "EHRs, PHRs, mHealth",
            ),
            (
                "",
                "organizations and providers in all of their",
                "and Telehealth, online",
            ),
            ("", "interactions with patients every day.", "resources."),
            ("", "", ""),
        ],
    ] {
        for line in bank {
            if line.0.is_empty() && line.1.is_empty() && line.2.is_empty() {
                continue;
            }
            let mut on_the_line: Vec<(&str, f64)> = Vec::new();
            if !line.0.is_empty() {
                on_the_line.push((line.0, columns[0]));
            }
            on_the_line.push((line.1, columns[1]));
            if !line.2.is_empty() {
                on_the_line.push((line.2, columns[2]));
            }
            pieces.extend(row(&on_the_line, y, Face::Regular));
            y -= CELL_LEADING;
        }
        y -= DESCRIPTION_PITCH - CELL_LEADING;
    }
    pieces
}

#[test]
fn a_table_of_descriptions_is_a_grid_and_not_two_columns() {
    let blocks = blocks_of(&descriptions(), &[]);
    let rows: Vec<(u16, u16, &str)> = cells(&blocks)
        .into_iter()
        .map(|cell| (cell.0, cell.1, cell.5))
        .collect();
    assert_eq!(
        rows.len(),
        9,
        "a table of descriptions came back as something else: {:?}",
        shape(&blocks)
    );
    assert!(
        rows.contains(&(1, 0, "Community resources")),
        "the name column was not read as cells: {:?}",
        rows
    );
    assert!(
        rows.contains(&(
            1,
            1,
            "Providers connecting patients to community programs through partnerships \
             that expand health system services beyond primary care and promote \
             self-help strategies for all."
        )),
        "a cell of four lines did not come out whole: {:?}",
        rows
    );
    assert!(
        rows.contains(&(
            2,
            2,
            "EHRs, PHRs, mHealth and Telehealth, online resources."
        )),
        "the second row's last cell is wrong: {:?}",
        rows
    );
}

/// A compact table set at the same spacing as the prose around it, in a region
/// that has wider gaps elsewhere. The leading rail is that spacing, and every
/// row of the table sits at it with the same cell edges as the row above --
/// so the rail alone would fold the whole table into one row and `find` would
/// lose it for having fewer than two (codex, #576).
#[test]
fn a_compact_table_is_not_one_wrapped_row() {
    const TIGHT: f64 = 12.0;

    let mut pieces: Vec<Piece> = Vec::new();
    let mut y = TOP + 60.0;
    for body in [
        "The committee considered the proposals put before it in",
        "the order that they had been received, and resolved that",
    ] {
        pieces.push(text(body, MARGIN, y));
        y -= TIGHT;
    }
    // Two gaps wider than the spacing, which is what gives the region a rail
    // at all: the paragraph break above the table and the one below it. Half
    // a line again, which is over the jump and under the gap that would make
    // the table a region of its own.
    y -= TIGHT * 0.5;
    for cells in [
        ("Item", "Detail", "Term"),
        ("Notice", "thirty days", "written"),
        ("Renewal", "one year", "automatic"),
        ("Audit", "on request", "annual"),
    ] {
        pieces.extend(row(
            &[(cells.0, MARGIN), (cells.1, 240.0), (cells.2, 380.0)],
            y,
            Face::Regular,
        ));
        y -= TIGHT;
    }
    y -= TIGHT * 0.5;
    for body in [
        "each of them be referred to the officers for a report",
        "before the next ordinary meeting of the board.",
    ] {
        pieces.push(text(body, MARGIN, y));
        y -= TIGHT;
    }

    let blocks = blocks_of(&pieces, &[]);
    assert_eq!(
        cells(&blocks)
            .iter()
            .filter(|cell| cell.1 == 0)
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec!["Item", "Notice", "Renewal", "Audit"],
        "the rows of a compact table were folded into one: {:?}",
        shape(&blocks)
    );
}

/// The same table with one sparse row in it -- a value in the first column
/// and nothing beside it, which is an ordinary row of data. A short line is
/// the evidence that a cell wrapped, so this one has to be told from one,
/// or a single sparse row keeps the whole table folded and `find` drops it
/// (codex, #576).
#[test]
fn a_sparse_row_does_not_fold_a_compact_table() {
    const TIGHT: f64 = 12.0;

    let mut pieces: Vec<Piece> = Vec::new();
    let mut y = TOP + 60.0;
    for body in [
        "The committee considered the proposals put before it in",
        "the order that they had been received, and resolved that",
    ] {
        pieces.push(text(body, MARGIN, y));
        y -= TIGHT;
    }
    y -= TIGHT * 0.5;
    for cells in [
        ("Item", "Detail", "Term"),
        ("Notice", "thirty days", "written"),
        ("Renewal", "", ""),
        ("Audit", "on request", "annual"),
    ] {
        let mut on_the_line: Vec<(&str, f64)> = vec![(cells.0, MARGIN)];
        if !cells.1.is_empty() {
            on_the_line.push((cells.1, 240.0));
            on_the_line.push((cells.2, 380.0));
        }
        pieces.extend(row(&on_the_line, y, Face::Regular));
        y -= TIGHT;
    }
    y -= TIGHT * 0.5;
    for body in [
        "each of them be referred to the officers for a report",
        "before the next ordinary meeting of the board.",
    ] {
        pieces.push(text(body, MARGIN, y));
        y -= TIGHT;
    }

    let blocks = blocks_of(&pieces, &[]);
    assert_eq!(
        cells(&blocks)
            .iter()
            .filter(|cell| cell.1 == 0)
            .map(|cell| cell.5)
            .collect::<Vec<_>>(),
        vec!["Item", "Notice", "Renewal", "Audit"],
        "one sparse row folded the table into a single row: {:?}",
        shape(&blocks)
    );
}
/// A head set in words is wider than the figures under it and overhangs the
/// white they leave between them. The column belongs to the cell that covers
/// most of it, and the head of column two is not a cell across columns one and
/// two for a seven-point overhang (#577).
///
/// Before that rule, the overhanging head claimed the column its neighbour
/// stood in, the row could not be read against the grid, and `find` gave the
/// whole run back to the paragraph merge -- then found the table again without
/// its head, which is the header row missing from every reading of it.
fn a_head_that_overhangs() -> Vec<Piece> {
    // The head of column two is set over that column and reaches past the
    // middle of the white the figures leave before column three, which is where
    // the boundary between the two columns sits.
    let mut pieces = row(
        &[
            ("Region", MARGIN),
            ("Amount reported", 150.0),
            ("Share", 265.0),
        ],
        TOP,
        Face::Bold,
    );
    for (i, line) in [
        ["12", "34", "56"],
        ["23", "45", "67"],
        ["34", "56", "78"],
        ["45", "67", "89"],
        ["56", "78", "90"],
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(line[0], MARGIN), (line[1], 160.0), (line[2], 265.0)],
            TOP - (i + 1) as f64 * PITCH,
            Face::Regular,
        ));
    }
    pieces
}

#[test]
fn a_head_wider_than_its_column_is_still_one_cell_of_it() {
    let blocks = blocks_of(&a_head_that_overhangs(), &[]);
    let grid: Vec<(u16, u16, u16, &str)> = cells(&blocks)
        .into_iter()
        .map(|cell| (cell.0, cell.1, cell.3, cell.5))
        .collect();
    assert!(
        grid.contains(&(0, 0, 1, "Region")) && grid.contains(&(0, 1, 1, "Amount reported")),
        "the head of column two overhangs the white beside it and was read as \
         a cell across two columns, which cost the header row its place in the \
         table: {:?}",
        shape(&blocks)
    );
    assert!(
        grid.contains(&(1, 0, 1, "12")) && grid.contains(&(1, 2, 1, "56")),
        "the first row of figures is not the row under that head: {grid:?}"
    );
}

/// A table of figures in two banks of columns, with the strip of white between
/// the banks narrower than the gaps the banks have inside them (#577).
///
/// This is the shape the XY-cut halves: the strip between the banks runs the
/// whole height of the table, both sides of it are dense columns of numbers, and
/// the head of each bank is set wide enough to close the strips inside it -- so
/// the widest full-height white on the page is the one between the banks and the
/// cut lands there. Cut, every row of the table is halved and the figures of row
/// four stand under the figures of row one.
fn two_banks() -> Vec<Piece> {
    // The banks: two columns each, with the head of each bank set over both of
    // its columns and closing the white inside it.
    let head = [
        ("Farms", 80.0),
        ("Reported", 140.0),
        ("Farms", 260.0),
        ("Reported", 320.0),
    ];
    let figures = [
        ["12,345", "67,890", "23,456", "78,901"],
        ["23,456", "78,901", "34,567", "89,012"],
        ["34,567", "89,012", "45,678", "90,123"],
        ["45,678", "90,123", "56,789", "12,345"],
        ["56,789", "12,345", "67,890", "23,456"],
    ];
    let columns = [80.0, 175.0, 260.0, 350.0];
    let mut pieces = row(
        &head.iter().map(|(body, x)| (*body, *x)).collect::<Vec<_>>(),
        TOP,
        Face::Bold,
    );
    for (i, line) in figures.iter().enumerate() {
        pieces.extend(row(
            &line
                .iter()
                .zip(columns)
                .map(|(body, x)| (*body, x))
                .collect::<Vec<_>>(),
            TOP - (i + 1) as f64 * PITCH,
            Face::Regular,
        ));
    }
    pieces
}

#[test]
fn two_banks_of_figures_are_one_table_and_not_two() {
    let blocks = blocks_of(&two_banks(), &[]);
    let grid: Vec<(u16, u16, &str)> = cells(&blocks)
        .into_iter()
        .map(|cell| (cell.0, cell.1, cell.5))
        .collect();
    assert_eq!(
        grid.iter().map(|cell| cell.1).max(),
        Some(3),
        "the table was cut down the strip between its banks and came back as \
         two tables of two columns: {:?}",
        shape(&blocks)
    );
    assert!(
        grid.contains(&(1, 0, "12,345")) && grid.contains(&(1, 3, "78,901")),
        "the first row of figures does not read across the strip between the \
         banks: {grid:?}"
    );
    assert!(
        grid.contains(&(0, 2, "Farms")) && grid.contains(&(0, 3, "Reported")),
        "the head of the second bank is not the head of columns three and \
         four: {grid:?}"
    );
}

/// A table whose first column is one cell a row -- a label, a name, a date --
/// against a bank of figures, with the widest full-height white on the page
/// between the two (#577).
///
/// The label column breaks nowhere, because one cell cannot have a gap in it, so
/// the test that asks BOTH sides of a cut to break sees one bank and lets the
/// cut through; `a_column_of_labels` does not refuse it either, because it asks
/// the thin side to have ink on far fewer bands than the wide one and here both
/// sides have ink on every band. What decides is the measurement a grid is read
/// by: the label column's typical band leaves white beside it, so it is a column
/// OF the table rather than a column of prose the page is set in.
fn labels_beside_a_bank() -> Vec<Piece> {
    // The head of the bank is set across both of its columns, which is what
    // closes the white inside the bank and leaves the white before it the widest
    // on the page.
    let mut pieces = vec![
        bold("Characteristic", MARGIN, TOP),
        bold("Reported measurements", 320.0, TOP),
    ];
    for (i, line) in [
        ("Mean age", "12,345", "23,456"),
        ("Sex (male)", "23,456", "34,567"),
        ("Weight (kg)", "34,567", "45,678"),
        ("Height (cm)", "45,678", "56,789"),
        ("Smokers", "56,789", "67,890"),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - (i + 1) as f64 * PITCH;
        pieces.extend(row(
            &[(line.0, MARGIN), (line.1, 280.0), (line.2, 470.0)],
            y,
            Face::Regular,
        ));
    }
    pieces
}

#[test]
fn a_column_of_labels_beside_a_bank_of_figures_is_one_table() {
    let blocks = blocks_of(&labels_beside_a_bank(), &[]);
    let grid: Vec<(u16, u16, &str)> = cells(&blocks)
        .into_iter()
        .map(|cell| (cell.0, cell.1, cell.5))
        .collect();
    let labels: Vec<&str> = grid
        .iter()
        .filter(|cell| cell.1 == 0)
        .map(|cell| cell.2)
        .collect();
    assert_eq!(
        labels,
        vec![
            "Characteristic",
            "Mean age",
            "Sex (male)",
            "Weight (kg)",
            "Height (cm)",
            "Smokers"
        ],
        "the page was cut between the labels and the figures, so the labels are \
         a column of prose and nothing says which figure belongs to which: {:?}",
        shape(&blocks)
    );
    assert!(
        grid.iter()
            .any(|cell| cell.0 == 1 && cell.1 > 0 && cell.2 == "12,345"),
        "the first row's figures are not cells of the row its label opens: \
         {grid:?}"
    );
}
