//! A table column empty in every row is not emitted (#574).
//!
//! End to end, on a synthesized ruled statement: the page draws a box with an
//! inner ruling that parts a stripe no word stands in -- the shape the drawn
//! grids of real statements have beside their `$` gutter -- so the lattice
//! numbers a column nothing starts in. With `table_drop_empty_columns` on,
//! the emitted table has four columns and every figure stays in its column;
//! with it off, today's five-column shape with the stripe standing empty.
//!
//! The on arm goes through `refine_with_rulings` with the page's own rules
//! and the rulings it drew, and through `PdfDocument::extract_blocks` too,
//! which ships the drop on, so the emitted shape is pinned where a caller
//! reaches it. The off arm sets the switch off explicitly and pins today's
//! five-column shape. Red with the drop taken out (the on arms read five
//! columns), and the staying test is red against a drop that takes a column
//! with a value in it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{refine_with_rulings, Switches, SWITCHES};
use pdf_engine::text::Ruling;
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

/// Vertical ruling edges: the stub, a stripe no word stands in, three figure
/// columns.
const XS: [f64; 6] = [72.0, 190.0, 210.0, 290.0, 370.0, 450.0];
/// Horizontal ruling edges: a header row and four body rows.
const YS: [f64; 6] = [700.0, 684.0, 668.0, 652.0, 636.0, 620.0];
const STUB: f64 = 76.0;
const FIGS: [f64; 3] = [214.0, 294.0, 374.0];

/// A thin line: a filled rectangle 0.5 pt across, as producers draw rulings.
fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
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

fn text(body: &str, x: f64, y: f64) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 9.into()]),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                body.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
    ]
}

fn ruler(vertical: bool, at: f64, from: f64, to: f64) -> Ruling {
    if vertical {
        Ruling {
            x0: at,
            y0: from,
            x1: at,
            y1: to,
        }
    } else {
        Ruling {
            x0: from,
            y0: at,
            x1: to,
            y1: at,
        }
    }
}

/// The rulings the page draws: what the text device records for the `re`
/// rectangles below, handed to the pass the way the engine hands them.
fn rulings(header_spans: bool) -> Vec<Ruling> {
    let mut out = Vec::new();
    for &y in &YS {
        out.push(ruler(false, y, XS[0], XS[5]));
    }
    out.push(ruler(true, XS[0], YS[5], YS[0]));
    out.push(ruler(true, XS[5], YS[5], YS[0]));
    // Inner verticals: under the header where it spans, full height else.
    let top = if header_spans { YS[1] } else { YS[0] };
    for &x in &XS[1..5] {
        out.push(ruler(true, x, YS[5], top));
    }
    out
}

fn body_rows() -> Vec<(String, Vec<Vec<String>>)> {
    vec![
        (
            "Net sales".to_string(),
            vec![
                vec!["$".to_string(), "9,898".to_string()],
                vec!["2,240".to_string()],
                vec!["2,598".to_string()],
            ],
        ),
        (
            "Cost of sales".to_string(),
            vec![
                vec!["1,111".to_string()],
                vec!["2,222".to_string()],
                vec!["3,333".to_string()],
            ],
        ),
        (
            "Gross profit".to_string(),
            vec![
                vec!["4,444".to_string()],
                vec!["5,555".to_string()],
                vec!["6,666".to_string()],
            ],
        ),
        (
            "Total".to_string(),
            vec![
                vec!["$".to_string(), "7,777".to_string()],
                vec!["8,888".to_string()],
                vec!["9,999".to_string()],
            ],
        ),
    ]
}

/// A ruled statement: `header` across the header row (one label per column,
/// or a single spanning label where `header_spans`), `$` on the first and
/// total rows only, three figure columns, and the wordless stripe between
/// the stub and the figures. Where `stripe_value` stands, that row holds a
/// value in the stripe instead.
fn pdf_of(header: &[&str], header_spans: bool, stripe_value: Option<(usize, &str)>) -> Vec<u8> {
    let mut ops = Vec::new();
    for &y in &YS {
        ops.extend(line(XS[0], y, XS[5], y));
    }
    ops.extend(line(XS[0], YS[5], XS[0], YS[0]));
    ops.extend(line(XS[5], YS[5], XS[5], YS[0]));
    let top = if header_spans { YS[1] } else { YS[0] };
    for &x in &XS[1..5] {
        ops.extend(line(x, YS[5], x, top));
    }
    let head_y = YS[0] - 6.0;
    if header_spans {
        ops.extend(text(header[0], STUB, head_y));
    } else {
        ops.extend(text(header[0], STUB, head_y));
        for (x, label) in FIGS.iter().zip(header[1..].iter()) {
            ops.extend(text(label, *x, head_y));
        }
    }
    for (r, (label, figs)) in body_rows().iter().enumerate() {
        let y = YS[r + 1] - 6.0;
        ops.extend(text(label, STUB, y));
        for (x, words) in FIGS.iter().zip(figs.iter()) {
            let mut dx = 0.0;
            for word in words {
                ops.extend(text(word, x + dx, y));
                dx += 14.0;
            }
        }
        if let Some((row, value)) = stripe_value {
            if row == r {
                ops.extend(text(value, XS[1] + 2.0, y));
            }
        }
    }
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

fn statement() -> Vec<u8> {
    pdf_of(&["In millions", "2024", "2023", "2022"], false, None)
}

/// The page through the pass with the switch as given: the engine's own
/// rules, the rulings the page drew.
fn refined(pdf: &[u8], header_spans: bool, drop: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let rulings = vec![rulings(header_spans)];
    refine_with_rulings(
        blocks,
        &[(612.0, 792.0)],
        &rules,
        &rulings,
        Switches {
            table_drop_empty_columns: drop,
            ..SWITCHES
        },
    )
}

/// `(row, column, column_span, text)` of every cell, in block order.
fn grid_of(blocks: &[Block]) -> Vec<(u16, u16, u16, String)> {
    blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell {
                row,
                column,
                column_span,
                ..
            } => Some((row, column, column_span, b.text.clone())),
            _ => None,
        })
        .collect()
}

fn figures() -> Vec<(u16, u16, &'static str)> {
    vec![
        (1, 1, "$ 9,898"),
        (1, 2, "2,240"),
        (1, 3, "2,598"),
        (2, 1, "1,111"),
        (2, 2, "2,222"),
        (2, 3, "3,333"),
        (3, 1, "4,444"),
        (3, 2, "5,555"),
        (3, 3, "6,666"),
        (4, 1, "$ 7,777"),
        (4, 2, "8,888"),
        (4, 3, "9,999"),
    ]
}

#[test]
fn an_empty_currency_column_is_not_emitted() {
    // Through the pass with the switch on, set explicitly, and where a
    // caller reaches it, which ships it on: four columns, every figure in
    // its column.
    for (tag, blocks) in [
        ("refine", refined(&statement(), false, true)),
        (
            "extract_blocks",
            PdfDocument::open(statement())
                .expect("open")
                .extract_blocks(&ExtractOptions::default())
                .expect("blocks"),
        ),
    ] {
        let grid = grid_of(&blocks);
        let markdown = to_markdown(&blocks);
        let columns = grid.iter().map(|(_, c, cs, _)| c + cs).max().unwrap_or(0);
        assert_eq!(columns, 4, "{tag} reads four columns:\n{markdown}");
        for (row, column, text) in figures() {
            assert!(
                grid.contains(&(row, column, 1, text.to_string())),
                "{tag} ({row}, {column}) is not {text:?}:\n{markdown}"
            );
        }
        // The header row kept its labels over the figures.
        for (column, label) in [(0, "In millions"), (1, "2024"), (2, "2023"), (3, "2022")] {
            assert!(
                grid.contains(&(0, column, 1, label.to_string())),
                "{tag} lost {label:?} over column {column}:\n{markdown}"
            );
        }
    }
}

#[test]
fn switch_off_todays_shape_pins_the_empty_column() {
    // Through the pass with the switch off, set explicitly: five columns,
    // the stripe standing empty in every row.
    let blocks = refined(&statement(), false, false);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let columns = grid.iter().map(|(_, c, cs, _)| c + cs).max().unwrap_or(0);
    assert_eq!(columns, 5, "the off shape reads five columns:\n{markdown}");
    for row in 0..5 {
        assert!(
            grid.iter().all(|(r, c, _, _)| *r != row || *c != 1),
            "off row {row} stands in the stripe:\n{markdown}"
        );
    }
}

#[test]
fn a_column_with_one_value_stays() {
    let pdf = pdf_of(
        &["In millions", "2024", "2023", "2022"],
        false,
        Some((1, "nil")),
    );
    let on = refined(&pdf, false, true);
    let grid = grid_of(&on);
    let markdown = to_markdown(&on);
    let columns = grid.iter().map(|(_, c, cs, _)| c + cs).max().unwrap_or(0);
    assert_eq!(columns, 5, "the column with the value goes:\n{markdown}");
    assert!(
        grid.contains(&(2, 1, 1, "nil".to_string())),
        "the value is not where it was put:\n{markdown}"
    );
}

#[test]
fn a_spanning_header_over_the_removed_column_keeps_its_text() {
    let pdf = pdf_of(&["Fiscal years ended December 31"], true, None);
    let on = refined(&pdf, true, true);
    let grid = grid_of(&on);
    let markdown = to_markdown(&on);
    assert!(
        grid.contains(&(0, 0, 4, "Fiscal years ended December 31".to_string())),
        "the header lost its text or its span:\n{markdown}"
    );
    for (row, column, text) in figures() {
        assert!(
            grid.contains(&(row, column, 1, text.to_string())),
            "({row}, {column}) is not {text:?}:\n{markdown}"
        );
    }
}
