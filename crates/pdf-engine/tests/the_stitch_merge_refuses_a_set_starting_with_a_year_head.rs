//! The stitch merge refuses a set starting with its own year head (#574).
//!
//! Two stacked statements sharing three figure columns with a label line
//! between them, as in `the_stitch_merge_runs_in_extract_blocks`, except the
//! quarter heads carry a year (`Q1 2010`, repeated over both halves like the
//! bare heads). The judge still chooses one set per half; with
//! `table_split_stacked` off the merge joins them into one table, with it on
//! the merge refuses into the lower half -- its topmost line carries a year
//! right of the stub and reads as no figure row -- and the page reads as two
//! tables with the label line prose between them.
//!
//! Red with the year-head guard taken out of `try_merge`: the on-shape reads
//! one table, checked once by disabling it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const SIZE: f64 = 9.0;
const PITCH: f64 = 18.0;
const STUB: f64 = 72.0;
const FIGS: [f64; 3] = [360.0, 430.0, 500.0];
/// Tops from the top half's last row to the label-only line between the
/// halves: past the alignment proposer's 2.5 row heights of white above it, so
/// no proposal spans the page, and inside the stitch walk's 1.6 pitches, so
/// the merge still reaches across. Either edge breaks the test by design.
const GAP_ABOVE_LEASE: f64 = 35.0;

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
    }
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), SIZE.into()],
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
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 792.into(), 792.into()]),
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

/// How many tables the blocks hold, read the way every consumer reads a run
/// of cells: a new table wherever `(row, column)` stops climbing.
fn tables_in(blocks: &[Block]) -> usize {
    let mut tables = 0;
    let mut last: Option<(u16, u16)> = None;
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                if last.is_none_or(|seen| (row, column) <= seen) {
                    tables += 1;
                }
                last = Some((row, column));
            }
            _ => last = None,
        }
    }
    tables
}

fn statements() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Statements of Income", STUB, 716.0)];
    let mut y = 716.0 - PITCH;
    let halves = [
        (
            "2010",
            [
                ("Net sales", ["9,898", "2,240", "2,598"]),
                ("Cost of sales", ["1,111", "2,222", "3,333"]),
                ("Gross profit", ["4,444", "5,555", "6,666"]),
            ],
        ),
        (
            "2009",
            [
                ("Operating income", ["7,777", "8,888", "9,999"]),
                ("Interest expense", ["1,010", "2,020", "3,030"]),
                ("Income taxes", ["4,040", "5,050", "6,060"]),
            ],
        ),
    ];
    for (half, (_year, rows)) in halves.into_iter().enumerate() {
        // The quarter heads repeat over the second half, which is what splits
        // the region detector in two (`heads_again`); each carries a year,
        // which is what the merge refuses on.
        for (x, head) in FIGS.iter().zip(["Q1", "Q2", "Q3"]) {
            pieces.push(at(&format!("{head} 2010"), *x, y));
        }
        y -= PITCH;
        pieces.push(at("Fiscal Year", STUB, y));
        for (x, year) in FIGS.iter().zip(["2003", "2002", "2001"]) {
            pieces.push(at(year, *x, y));
        }
        y -= PITCH;
        for (label, figs) in rows {
            pieces.push(at(label, STUB, y));
            for (x, fig) in FIGS.iter().zip(figs) {
                pieces.push(at(fig, *x, y));
            }
            y -= PITCH;
        }
        if half == 0 {
            y -= GAP_ABOVE_LEASE - PITCH;
            pieces.push(at("Capital lease", STUB, y));
            y -= PITCH;
        }
    }
    pieces
}

#[test]
fn the_stitch_merge_refuses_a_set_starting_with_a_year_head() {
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let page = page_blocks(&all, 0);
    let run = |stacked: bool| {
        refine_page(
            page.clone(),
            (792.0, 792.0),
            &facts,
            Switches {
                table_split_stacked: stacked,
                ..SWITCHES
            },
        )
    };
    let (off, on) = (run(false), run(true));
    let (off_md, on_md) = (to_markdown(&off), to_markdown(&on));
    assert_eq!(tables_in(&off), 1, "with the stacked split off:\n{off_md}");
    assert_eq!(tables_in(&on), 2, "with the stacked split on:\n{on_md}");
    // The label line between the halves is prose with the merge refused.
    let lease = on
        .iter()
        .find(|b| b.text == "Capital lease")
        .expect("the label line is read");
    assert!(
        !matches!(lease.kind, BlockKind::TableCell { .. }),
        "the label line joined with the merge refused:\n{on_md}"
    );
    // The stacked split ships on, so the shipped path refuses: two tables.
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 2, "on the shipped path:\n{shipped_md}");
}
