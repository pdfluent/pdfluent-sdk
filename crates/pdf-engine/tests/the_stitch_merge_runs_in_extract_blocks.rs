//! The stitch merge joins the halves with the stacked split off (#574).
//!
//! These test the MERGE, with `table_split_stacked` pinned off in both
//! tests' switches: the statement's lower half opens with its own year
//! head, which the stacked split (shipped on) and its merge guard separate
//! by design, so on the shipped path this page reads as two tables. The
//! merge's one-table shape is pinned here, through `refine_page`.
//!
//! So: an operator-built financial statement whose two halves share three
//! figure columns, with a label-only line ("Capital lease") between them, as
//! on the proxy page the identity check caught. The judge chooses one set per
//! half; with the merge on and the stacked split off the page reads as
//! exactly one table, holding rows from both halves and the label line as a
//! row.
//!
//! The page is shaped so the judge is offered two halves and nothing spanning.
//! The repeated quarter heads split the region detector (`heads_again`: two
//! years' statements, two tables), and the white above the label line breaks
//! the alignment proposer (past 2.5 row heights) while staying inside the
//! stitch walk's reach (1.6 pitches): at a pitch of 18 over a size of 9 both
//! hold with room. Red with `table_stitch_merge` off (the second test pins
//! it: two tables, the label line prose), and red with the `stitch_sets` call
//! in `judge_regions_traced` skipped, checked once by commenting it out.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{to_markdown, Block, BlockKind, PdfDocument};

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

/// How many tables the serializer writes, read the way every consumer reads a
/// run of cells: a new table wherever `(row, column)` stops climbing.
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

fn statement() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Statements of Income", STUB, 716.0)];
    let mut y = 716.0 - PITCH;
    let halves: [[(&str, [&str; 3]); 3]; 2] = [
        [
            ("Net sales", ["9,898", "2,240", "2,598"]),
            ("Cost of sales", ["1,111", "2,222", "3,333"]),
            ("Gross profit", ["4,444", "5,555", "6,666"]),
        ],
        [
            ("Operating income", ["7,777", "8,888", "9,999"]),
            ("Interest expense", ["1,010", "2,020", "3,030"]),
            ("Income taxes", ["4,040", "5,050", "6,060"]),
        ],
    ];
    for (half, rows) in halves.into_iter().enumerate() {
        // The quarter heads repeat over the second half, which is what splits
        // the region detector in two (`heads_again`); the years alone would
        // not (no letter in them).
        for (x, head) in FIGS.iter().zip(["Q1", "Q2", "Q3"]) {
            pieces.push(at(head, *x, y));
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

/// The merge ships on, and with the stacked split pinned off the two halves
/// read as one table with the label line between them as a row of it.
#[test]
fn the_stitch_merge_joins_the_halves_with_the_stacked_split_off() {
    let doc = PdfDocument::open(pdf_of(&statement())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let blocks = refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_split_stacked: false,
            ..SWITCHES
        },
    );
    let markdown = to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        1,
        "one statement, one table:\n{markdown}"
    );
    // Rows from both halves, and the label line between them, are cells of it.
    for needle in [
        "Net sales",
        "Cost of sales",
        "Gross profit",
        "Capital lease",
        "Operating income",
        "Interest expense",
        "Income taxes",
    ] {
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} is not a cell of the table:\n{markdown}"
        );
    }
    // The title stands one pitch above the table at the stub, which the merge
    // does not reach for: it stays prose.
    let title = blocks
        .iter()
        .find(|b| b.text.contains("Consolidated Statements"))
        .expect("the title is read");
    assert!(matches!(title.kind, BlockKind::Paragraph), "{markdown}");
}

/// The operator test for `table_stitch_merge`: red with the switch off. The
/// judge chooses one set per half either way; without the merge the halves
/// stay two tables and the label line between them stays prose. The stacked
/// split is pinned off: with it on the merge guard refuses the lower half
/// and both arms read two tables.
#[test]
fn the_merge_switch_moves_the_page() {
    let doc = PdfDocument::open(pdf_of(&statement())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let page = page_blocks(&all, 0);
    let run = |merge: bool| {
        refine_page(
            page.clone(),
            (792.0, 792.0),
            &facts,
            Switches {
                table_stitch_merge: merge,
                table_split_stacked: false,
                ..SWITCHES
            },
        )
    };
    let (off, on) = (run(false), run(true));
    let (off_md, on_md) = (to_markdown(&off), to_markdown(&on));
    assert_eq!(tables_in(&on), 1, "with the merge on:\n{on_md}");
    assert_eq!(tables_in(&off), 2, "with the merge off:\n{off_md}");
    let lease = |blocks: &[Block]| {
        blocks
            .iter()
            .find(|b| b.text == "Capital lease")
            .expect("the label line is read")
            .kind
    };
    assert!(
        matches!(lease(&on), BlockKind::TableCell { .. }),
        "the label line is not a cell with the merge on:\n{on_md}"
    );
    assert!(
        !matches!(lease(&off), BlockKind::TableCell { .. }),
        "the label line joined with the merge off:\n{off_md}"
    );
}
