//! A table's region is found on the whole page before the XY-cut runs, so the
//! cut never goes through a table (#574).
//!
//! The page here is the shape the DocLayNet tables axis loses most to: a
//! financial statement whose line items are set in groups, each under a label
//! in the stub column, with more white between the groups than between the
//! rows of one. The horizontal cut fires in that white, every group became a
//! region of its own, and `tables::find` -- called once per region -- returned
//! one table per group, with the group labels and the lines of a group too
//! short to be a table on their own left behind as paragraphs. The #596 guard
//! on the horizontal cut holds the first page here; the second is the shape it
//! cannot see, a VERTICAL cut down the white a column of dashes leaves. Take
//! `xy_cut_round_tables` out of `paragraph_blocks` and
//! `a_wide_grid_of_dashes_keeps_its_labels` goes red.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const SIZE: f64 = 9.0;
const PITCH: f64 = 12.0;
const STUB: f64 = 72.0;
const VALUES: [f64; 4] = [330.0, 400.0, 470.0, 530.0];

struct Piece {
    text: String,
    x: f64,
    y: f64,
    /// Set in the fixed-pitch face.
    mono: bool,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        mono: false,
    }
}

/// A piece set in the fixed-pitch face, so lines of one length end flush.
fn mono(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        mono: true,
        ..at(text, x, y)
    }
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    pdf_with_rules(pieces, &[])
}

/// The page with horizontal rules drawn as thin filled rectangles, each
/// `(left, right, y)`.
fn pdf_with_rules(pieces: &[Piece], rules: &[(f64, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    // A fixed-pitch face for the lines a test needs to end flush.
    let mono = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut operations = Vec::new();
    for (left, right, y) in rules {
        operations.push(Operation::new(
            "re",
            vec![
                (*left).into(),
                (*y).into(),
                (right - left).into(),
                0.5.into(),
            ],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if piece.mono {
                    b"F2".to_vec()
                } else {
                    b"F1".to_vec()
                }),
                SIZE.into(),
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
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 792.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font),
                "F2" => Object::Reference(mono),
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

/// A group of a statement: its label, and three line items with their figures.
type Group = (&'static str, [(&'static str, [&'static str; 4]); 3]);

/// A statement of three groups, each under a label, with two and a half
/// pitches of white between the groups, a head over the value columns, and a
/// paragraph under it all.
fn statement() -> Vec<Piece> {
    let mut pieces = vec![at("Notes to the financial statements", STUB, 740.0)];
    let mut y = 700.0;
    for (column, head) in VALUES.iter().zip(["2000", "1999", "2000", "1999"]) {
        pieces.push(at(head, *column, y));
    }
    y -= PITCH;
    let groups: [Group; 3] = [
        (
            "Leasehold buildings and improvements",
            [
                (
                    "Independent valuation 1998",
                    ["8,500,000", "8,500,000", "-", "-"],
                ),
                (
                    "Accumulated depreciation",
                    ["(302,619)", "(100,319)", "-", "-"],
                ),
                ("Carrying amount", ["8,197,381", "8,399,681", "-", "-"]),
            ],
        ),
        (
            "Vessels",
            [
                ("At cost", ["7,748,302", "4,939,456", "-", "-"]),
                (
                    "Accumulated depreciation",
                    ["(625,559)", "(416,918)", "-", "-"],
                ),
                ("Carrying amount", ["7,122,743", "4,522,538", "-", "-"]),
            ],
        ),
        (
            "Plant and equipment",
            [
                ("At cost", ["2,621,336", "1,813,810", "-", "-"]),
                (
                    "Accumulated depreciation",
                    ["(816,726)", "(350,918)", "-", "-"],
                ),
                ("Carrying amount", ["1,804,610", "1,462,892", "-", "-"]),
            ],
        ),
    ];
    for (label, rows) in groups {
        y -= PITCH * 1.5;
        pieces.push(at(label, STUB, y));
        for (stub, values) in rows {
            y -= PITCH;
            pieces.push(at(&leader(stub), STUB + 9.0, y));
            for (column, value) in VALUES.iter().zip(values) {
                pieces.push(at(value, *column, y));
            }
        }
    }
    y -= PITCH * 4.0;
    pieces.push(at(
        "The revaluation of leasehold buildings was performed on the basis of market value.",
        STUB,
        y,
    ));
    pieces
}

/// A line item with its dot leader run out towards the first value column, as
/// an annual report sets it: `Accumulated depreciation . . . . . .`.
fn leader(stub: &str) -> String {
    let dots = 48usize.saturating_sub(stub.len());
    format!("{stub} {}", ". ".repeat(dots))
}

fn blocks_of(bytes: Vec<u8>) -> Vec<Block> {
    let doc = PdfDocument::open(bytes).expect("open synthetic pdf");
    doc.extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// The page through the learned pass with the final-table switch off: what
/// the pass emits where v2 never refuses. The table here is built by the
/// learned pass itself (the judge and the grid decoder), so switching the
/// whole pass off would assert on paragraphs; switching v2 off keeps the
/// verdict this test holds -- found before the cut -- and none other.
fn blocks_with_v2_off(bytes: Vec<u8>) -> Vec<Block> {
    use pdf_engine::learned::{document_facts, page_blocks, refine_page, SWITCHES};
    let doc = PdfDocument::open(bytes).expect("open synthetic pdf");
    let before = doc
        .extract_blocks_before_learning()
        .expect("extract blocks");
    let facts = document_facts(&before);
    let mut off = SWITCHES;
    off.table_is_v2 = false;
    refine_page(page_blocks(&before, 0), (792.0, 792.0), &facts, off)
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

#[test]
fn a_statement_set_in_groups_is_one_table() {
    let blocks = blocks_of(pdf_of(&statement()));
    let markdown = to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        1,
        "one statement, one table:\n{markdown}"
    );
    // The labels of the later groups are rows of it, not paragraphs beside it.
    for label in ["Vessels", "Plant and equipment"] {
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.trim() == label),
            "the label {label:?} is a cell of the table:\n{markdown}"
        );
    }
}

#[test]
fn the_paragraph_under_the_table_stays_a_paragraph() {
    let blocks = blocks_of(pdf_of(&statement()));
    let prose = blocks
        .iter()
        .find(|b| b.text.starts_with("The revaluation"))
        .expect("the paragraph is read");
    assert!(
        matches!(prose.kind, BlockKind::Paragraph),
        "prose under a table is not a row of it: {:?}",
        prose.kind
    );
    // And the reading order puts it after the table.
    let last_cell = blocks
        .iter()
        .rposition(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .expect("a table is found");
    assert!(prose.index > blocks[last_cell].index);
}

#[test]
fn prose_set_in_two_columns_is_no_table() {
    // Two columns of prose break at the same place on every line, which is a
    // strip of white down the page -- and one strip, with long runs either
    // side of it, is a page layout and not a table.
    let mut pieces = Vec::new();
    for line in 0..12 {
        let y = 700.0 - f64::from(line) * PITCH;
        pieces.push(at(
            "the quick brown fox jumps over the lazy dog again",
            STUB,
            y,
        ));
        pieces.push(at(
            "and the second column carries on its own story",
            320.0,
            y,
        ));
    }
    let blocks = blocks_of(pdf_of(&pieces));
    assert_eq!(tables_in(&blocks), 0, "{}", to_markdown(&blocks));
}

/// A wide grid most of whose columns hold a dash: an interest-rate note with a
/// label column, seven value columns, two sections under their own heads and a
/// total under each. The columns of dashes leave a strip of white down the
/// page wide enough for a vertical cut, and the cut took the labels off their
/// values: the labels came out as a list of paragraphs and the figures as a
/// table of their own (DocLayNet `ASX_MRM_2000`, page 60).
/// A section of the note: its head, and its line items with their figures.
type Section = (&'static str, &'static [(&'static str, [&'static str; 7])]);

fn a_grid_of_dashes() -> Vec<Piece> {
    const COLUMNS: [f64; 7] = [230.0, 300.0, 380.0, 450.0, 520.0, 600.0, 680.0];
    let mut pieces = vec![at("Interest rate risk", STUB, 740.0)];
    let mut y = 712.0;
    for (column, head) in COLUMNS.iter().zip([
        "Floating", "1 year", "1 to 5", "Over 5", "Non-int.", "Total", "Rate",
    ]) {
        pieces.push(at(head, *column, y));
    }
    let sections: [Section; 2] = [
        (
            "(i) Financial assets",
            &[
                (
                    "Cash",
                    ["5,647,299", "-", "-", "-", "-", "5,647,299", "4.5%"],
                ),
                (
                    "Trade debtors",
                    ["-", "-", "-", "-", "4,077,295", "4,077,295", "N/a"],
                ),
                (
                    "Other debtors",
                    ["-", "-", "-", "-", "231,746", "231,746", "N/a"],
                ),
                (
                    "Loans - related parties",
                    ["-", "11,184", "-", "-", "-", "11,184", "6.70%"],
                ),
                (
                    "Loans - other persons",
                    ["-", "5,590", "-", "-", "-", "5,590", "7.55%"],
                ),
            ],
        ),
        (
            "(ii) Financial liabilities",
            &[
                (
                    "Trade creditors",
                    ["-", "-", "-", "-", "1,079,327", "1,079,327", "N/a"],
                ),
                (
                    "Other creditors",
                    ["-", "-", "-", "-", "389,522", "389,522", "N/a"],
                ),
                (
                    "Hire purchase",
                    [
                        "-",
                        "1,250,000",
                        "2,400,120",
                        "-",
                        "-",
                        "3,650,120",
                        "8.10%",
                    ],
                ),
                (
                    "Bank loans",
                    ["-", "-", "4,000,000", "-", "-", "4,000,000", "7.25%"],
                ),
            ],
        ),
    ];
    for (head, rows) in sections {
        y -= 28.0;
        pieces.push(at(head, STUB, y));
        for (label, values) in rows.iter() {
            y -= 14.0;
            pieces.push(at(label, STUB + 18.0, y));
            for (column, value) in COLUMNS.iter().zip(values) {
                pieces.push(at(value, *column, y));
            }
        }
        y -= 21.0;
        pieces.push(at("Total", STUB, y));
        for column in COLUMNS.iter().skip(5) {
            pieces.push(at("9,973,114", *column, y));
        }
    }
    pieces
}

#[test]
fn a_wide_grid_of_dashes_keeps_its_labels() {
    let blocks = blocks_of(pdf_of(&a_grid_of_dashes()));
    let markdown = to_markdown(&blocks);
    for (label, figure) in [
        ("Trade debtors", "4,077,295"),
        ("Loans - other persons", "5,590"),
        ("Trade creditors", "1,079,327"),
        ("Bank loans", "4,000,000"),
    ] {
        let label_at = cell_at(&blocks, label);
        assert!(
            label_at.is_some() && label_at == cell_at(&blocks, figure),
            "{label:?} and {figure:?} are one row of one table:\n{markdown}"
        );
    }
}

/// The table and row a cell holding exactly `text` sits in.
fn cell_at(blocks: &[Block], text: &str) -> Option<(usize, u16)> {
    let mut table = 0;
    let mut last: Option<(u16, u16)> = None;
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                if last.is_some_and(|seen| (row, column) <= seen) || last.is_none() {
                    table += 1;
                }
                last = Some((row, column));
                if block.text.trim() == text {
                    return Some((table, row));
                }
            }
            _ => last = None,
        }
    }
    None
}

/// A statement in two halves with three and a half lines of white between them:
/// more than the learned pass's run allows inside one table, less than the
/// region allows.
fn two_halves() -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = 700.0;
    for half in 0..2 {
        for line in 0..5 {
            let figures = [
                format!("{},{:03}", 10 + line, 100 * half + 7 * line),
                format!("{},{:03}", 20 + line, 90 * half + 11 * line),
                format!("({})", 300 + 13 * line),
            ];
            pieces.push(at(&format!("Line item {half}.{line}"), STUB, y));
            for (column, figure) in VALUES.iter().zip(&figures) {
                pieces.push(at(figure, *column, y));
            }
            y -= PITCH;
        }
        y -= PITCH * 2.5;
    }
    pieces
}

#[test]
fn the_learned_pass_does_not_rebuild_part_of_a_table() {
    use pdf_engine::learned::tables::{refine_gated, Weakness};
    use pdf_engine::learned::{document_facts, page_blocks, Switches, SWITCHES};

    let doc = PdfDocument::open(pdf_of(&two_halves())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    assert_eq!(
        tables_in(&all),
        1,
        "the builder reads the two halves as one table:\n{}",
        to_markdown(&all)
    );
    let facts = document_facts(&all);
    let page = page_blocks(&all, 0);
    // The region model off, so the runs are the builder's own cells, broken
    // where the white between the halves is more than three lines; and every
    // run called weak, so the structure model would rebuild whatever it is
    // handed.
    let switches = Switches {
        table_region: false,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    };
    let weak = |_run: &[usize], _page: &[&Block]| {
        Some(Weakness {
            thin_rows: 2,
            fill: 0.31,
            retried_from: 1,
            full_rows: 0,
            columns: 4,
        })
    };
    let (out, _) = refine_gated(page.clone(), (792.0, 792.0), &facts, switches, &weak);
    assert_eq!(
        tables_in(&out),
        1,
        "a run that holds half a table leaves the table alone:\n{}",
        to_markdown(&out)
    );
    let texts = |blocks: &[Block]| blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>();
    assert_eq!(texts(&out), texts(&page));
}

/// A manual's menu: a short key and what it does, two columns under a head,
/// between two paragraphs (DocLayNet `IBM-i-s5445349`, 118 pages of the tuning
/// half). The cut divided the keys from their functions, so the page read
/// `Option 11 12 13 14` and then the functions, and no table was found.
fn a_menu() -> Vec<Piece> {
    let mut pieces = vec![at(
        "You can start the following four functions from this menu:",
        STUB,
        700.0,
    )];
    let rows = [
        ("Option", "Function"),
        ("11", "Work with PFD definitions"),
        ("12", "Work with PFD definition files"),
        ("13", "Print database file member"),
        ("14", "Print AFP Utilities tutorial"),
    ];
    for (n, (key, function)) in rows.iter().enumerate() {
        let y = 676.0 - n as f64 * PITCH;
        pieces.push(at(key, STUB, y));
        pieces.push(at(function, STUB + 50.0, y));
    }
    pieces.push(at(
        "The following describe the options available on the above menu.",
        STUB,
        600.0,
    ));
    pieces
}

#[test]
fn a_menu_of_keys_is_a_table_of_two_columns() {
    let blocks = blocks_of(pdf_of(&a_menu()));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    for (key, function) in [
        ("11", "Work with PFD definitions"),
        ("14", "Print AFP Utilities tutorial"),
    ] {
        let at = cell_at(&blocks, key);
        assert!(
            at.is_some() && at == cell_at(&blocks, function),
            "{key:?} and {function:?} are one row:\n{markdown}"
        );
    }
}

/// A list of tables in a manual: each entry runs out to its page number along
/// a dot leader, every dot drawn on its own (DocLayNet `IBM-i-s5445349`, the
/// front matter). Nothing but the leader stands between the label and the
/// figure, so no gutter was ever seen and the page came back as a list.
fn a_list_of_tables() -> Vec<Piece> {
    let entries = [
        ("56. Function Keys for Edit Operation", "237"),
        ("57. Function Keys to Specify End Position", "238"),
        ("58. Formatting examples", "262"),
        ("59. POSTNET types", "270"),
        ("60. Select Page Segment display fields", "277"),
    ];
    let mut pieces = Vec::new();
    for (n, (label, page)) in entries.iter().enumerate() {
        let y = 700.0 - n as f64 * PITCH;
        pieces.push(at(label, STUB, y));
        let mut x = STUB + 8.0 + 5.0 * label.len() as f64;
        while x < 470.0 {
            pieces.push(at(".", x, y));
            x += 12.0;
        }
        pieces.push(at(page, 490.0, y));
    }
    pieces
}

#[test]
fn entries_on_a_dot_leader_are_a_table() {
    let blocks = blocks_of(pdf_of(&a_list_of_tables()));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    let row_of = |needle: &str| {
        cell_at(
            &blocks,
            blocks
                .iter()
                .find(|b| b.text.starts_with(needle))
                .map(|b| b.text.trim())
                .unwrap_or(needle),
        )
    };
    assert!(
        row_of("58. Formatting examples").is_some()
            && row_of("58. Formatting examples") == cell_at(&blocks, "262"),
        "the entry and its page are one row:\n{markdown}"
    );
    // The dots stay in the text: the reader drew them.
    assert!(markdown.contains("examples . . ."), "{markdown}");
}

/// A paper's table, ruled above and below and nowhere else: a column of names
/// and a column of descriptions too long to be cells by their shape alone, set
/// between two paragraphs (the booktabs style; LiteParse's detector, #632).
fn a_booktabs_table() -> Vec<Piece> {
    let mut pieces = vec![at(
        "We compare three methods on the held-out split, summarised below.",
        STUB,
        720.0,
    )];
    let rows = [
        ("Method of finding it", "What it reads from the page"),
        (
            "Rulings in the drawing",
            "the vector paths the document drew between its cells",
        ),
        (
            "Alignment of the text",
            "the edges of text that line up from one row to the next",
        ),
        (
            "A judge over proposals",
            "a learned score over every region the other two propose",
        ),
    ];
    for (n, (name, what)) in rows.iter().enumerate() {
        let y = 690.0 - n as f64 * PITCH * 1.4;
        pieces.push(at(name, STUB, y));
        pieces.push(at(what, STUB + 150.0, y));
    }
    pieces.push(at(
        "The judge is trained on the train split and never sees a rig document.",
        STUB,
        600.0,
    ));
    pieces
}

#[test]
fn a_table_ruled_above_and_below_is_found() {
    let bytes = pdf_with_rules(
        &a_booktabs_table(),
        &[(STUB - 4.0, 540.0, 702.0), (STUB - 4.0, 540.0, 628.0)],
    );
    let blocks = blocks_of(bytes);
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "Alignment of the text").is_some()
            && cell_at(&blocks, "Alignment of the text")
                == cell_at(
                    &blocks,
                    "the edges of text that line up from one row to the next"
                ),
        "{markdown}"
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Paragraph) && b.text.starts_with("The judge")),
        "the paragraph under the foot rule is not a row: {markdown}"
    );
}

/// A register of affiliates: a head in two lines, the rows grouped under the
/// country they sit in, and a total row whose label runs across two columns
/// (DocLayNet `OTC_NSANY_2004`). The region is right; what went wrong was
/// after it, when the builder read the region as any column and let each
/// country label end one table and start the next, with the head and a
/// one-row group left over as paragraphs.
fn a_register() -> Vec<Piece> {
    const COLUMNS: [f64; 5] = [72.0, 220.0, 300.0, 460.0, 530.0];
    let mut pieces = vec![
        at("Capital", COLUMNS[3], 712.0),
        at("Nissan", COLUMNS[4], 712.0),
    ];
    for (column, head) in COLUMNS.iter().zip([
        "Company",
        "Location",
        "Principal business",
        "(millions)",
        "share (%)",
    ]) {
        pieces.push(at(head, *column, 700.0));
    }
    let groups: [(&str, &[[&str; 5]]); 2] = [
        (
            "Japan",
            &[
                [
                    "Nissan Diesel Motor",
                    "Ageo",
                    "Automobiles and parts",
                    "65,835",
                    "24.38",
                ],
                [
                    "Kinugawa Rubber",
                    "Chiba",
                    "Automotive parts",
                    "5,655",
                    "20.25",
                ],
            ],
        ),
        (
            "France",
            &[[
                "Renault",
                "Billancourt",
                "Automobiles and parts",
                "1,086",
                "15.00",
            ]],
        ),
    ];
    let mut y = 700.0;
    for (country, rows) in groups {
        y -= PITCH * 1.5;
        pieces.push(at(country, COLUMNS[0], y));
        for row in rows.iter() {
            y -= PITCH;
            for (column, cell) in COLUMNS.iter().zip(row) {
                pieces.push(at(cell, *column, y));
            }
        }
    }
    pieces
}

#[test]
fn a_region_that_is_one_grid_is_one_table() {
    let blocks = blocks_of(pdf_of(&a_register()));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    for (a, b) in [
        ("Nissan Diesel Motor", "24.38"),
        ("Renault", "15.00"),
        ("Company", "Location"),
    ] {
        assert!(
            cell_at(&blocks, a).is_some() && cell_at(&blocks, a) == cell_at(&blocks, b),
            "{a:?} and {b:?} are one row of the one table:\n{markdown}"
        );
    }
}

/// The same entries with the leader drawn inside each string, label, dots and
/// figure in one `Tj` (DocLayNet `IBM-i-s5445349`'s overlay settings, and most
/// contents pages): 62 of the tables no proposer reached on the tuning half.
fn a_list_drawn_in_one_string() -> Vec<Piece> {
    [
        ("Characters per line", "10"),
        ("Lines per inch", "6"),
        ("Overlay width", "5.15"),
        ("Overlay height", "6.10"),
    ]
    .iter()
    .enumerate()
    .map(|(n, (label, figure))| {
        let dots = ". ".repeat(30usize.saturating_sub(label.len()));
        at(
            &format!("{label} {dots}{figure}"),
            STUB,
            700.0 - n as f64 * PITCH,
        )
    })
    .collect()
}

#[test]
fn a_leader_inside_one_string_still_divides_label_from_figure() {
    let blocks = blocks_of(pdf_of(&a_list_drawn_in_one_string()));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "5.15").is_some()
            && blocks.iter().any(|b| b.text.starts_with("Overlay width")
                && cell_at(&blocks, b.text.trim()) == cell_at(&blocks, "5.15")),
        "{markdown}"
    );
}

/// The same ruled table with descriptions set in narrow letters that stop well
/// inside the rules (DocLayNet `IBM-i-s5445349`'s field tables). The width a
/// string is ESTIMATED at runs far past its glyphs here, and measured by that
/// every description ran out past the rule and the table was refused.
#[test]
fn a_description_inside_its_rules_is_inside_them() {
    let mut pieces = Vec::new();
    for (n, name) in [
        "Field name in full",
        "Library it is filed in",
        "Title of the listing",
        "Limit of its fill",
    ]
    .iter()
    .enumerate()
    {
        let y = 690.0 - n as f64 * PITCH * 1.4;
        pieces.push(at(name, STUB, y));
        pieces.push(at(
            "illicit little lilies fill it till it tilts; it is still ill-lit if it",
            STUB + 150.0,
            y,
        ));
    }
    let bytes = pdf_with_rules(
        &pieces,
        &[(STUB - 4.0, 430.0, 702.0), (STUB - 4.0, 430.0, 628.0)],
    );
    let blocks = blocks_of(bytes);
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "Library it is filed in").is_some(),
        "{markdown}"
    );
}

/// The same ruled table with each rule drawn cell by cell -- one segment per
/// column, end to end -- as a word processor's grid draws it (DocLayNet's
/// tender annexes). No one segment spans the table, and each was too short to
/// open or close it.
#[test]
fn a_rule_drawn_cell_by_cell_is_one_rule() {
    let segments = |y: f64| {
        [
            (STUB - 4.0, STUB + 146.0, y),
            (STUB + 146.5, STUB + 400.0, y),
            (STUB + 400.5, 540.0, y),
        ]
    };
    let mut rules = segments(702.0).to_vec();
    rules.extend(segments(628.0));
    let blocks = blocks_of(pdf_with_rules(&a_booktabs_table(), &rules));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "Alignment of the text").is_some()
            && cell_at(&blocks, "Alignment of the text")
                == cell_at(
                    &blocks,
                    "the edges of text that line up from one row to the next"
                ),
        "{markdown}"
    );
}

/// The ruled table again, with a manual's change bars in the left margin beside
/// some of its rows (DocLayNet `IBM-i-s5445349`). A bar is ink outside the
/// rules, and it made the table's rows look like text that runs past them.
#[test]
fn a_change_bar_in_the_margin_leaves_a_ruled_table_ruled() {
    let mut pieces = Vec::new();
    for (n, name) in [
        "Field name in full",
        "Library it is filed in",
        "Title of the listing",
        "Limit of its fill",
    ]
    .iter()
    .enumerate()
    {
        let y = 690.0 - n as f64 * PITCH * 1.4;
        pieces.push(at(name, STUB, y));
        pieces.push(at(
            "illicit little lilies fill it till it tilts; it is still ill-lit if it",
            STUB + 150.0,
            y,
        ));
        if n % 2 == 1 {
            pieces.push(at("|", STUB - 40.0, y));
        }
    }
    let bytes = pdf_with_rules(
        &pieces,
        &[(STUB - 4.0, 430.0, 702.0), (STUB - 4.0, 430.0, 628.0)],
    );
    let blocks = blocks_of(bytes);
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "Title of the listing").is_some(),
        "{markdown}"
    );
}

/// A segment table in an annual report: a key column whose longest key stops an
/// em short of the descriptions, each description wrapping over several lines
/// (DocLayNet `NYSE_HRL_2004`). Two ems of white is what separates cells of a
/// grid, and a key and its description stood closer than that: every row read
/// as one run of ink and the table was never found.
fn a_key_with_its_description() -> Vec<Piece> {
    const DESCRIPTION: f64 = STUB + 80.0;
    let mut pieces = vec![
        at("Segment", STUB, 700.0),
        at("Business conducted", DESCRIPTION, 700.0),
    ];
    let mut y = 700.0 - PITCH * 1.5;
    for (key, lines) in [
        (
            "Grocery Products",
            [
                "This segment consists of the",
                "processing and sale of shelf-",
                "stable food products.",
            ],
        ),
        (
            "Refrigerated",
            [
                "This segment includes the meat",
                "products and foodservice",
                "business units.",
            ],
        ),
        (
            "Specialty Foods",
            [
                "This segment includes the",
                "brands acquired in the year",
                "and their operations.",
            ],
        ),
    ] {
        pieces.push(at(key, STUB, y));
        for (n, line) in lines.iter().enumerate() {
            pieces.push(at(line, DESCRIPTION, y - n as f64 * PITCH));
        }
        y -= PITCH * 4.0;
    }
    pieces
}

/// Read with the final-table switch OFF, on purpose: end to end the judge
/// refuses this wordy page (m134 ships v2 on), but what this test holds is
/// the finder's verdict -- the table found before the cut -- and that is
/// unchanged.
#[test]
fn a_key_an_em_from_its_description_is_a_table_of_two_columns() {
    let blocks = blocks_with_v2_off(pdf_of(&a_key_with_its_description()));
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 1, "{markdown}");
    assert!(
        cell_at(&blocks, "Refrigerated").is_some()
            && cell_at(&blocks, "Refrigerated")
                == cell_at(&blocks, "This segment includes the meat"),
        "{markdown}"
    );
}

/// A KNOWN LOSS of is-table v2 (#638), pinned end to end: the key + wordy
/// description shape (DocLayNet `NYSE_HRL_2004`), through
/// `PdfDocument::extract_blocks` with the shipped defaults, holds no table
/// today -- v2 refuses the page the finder still reads (the test above, with
/// v2 off). To be recovered by train-side positives of that shape in the
/// next v2 round; when this turns red, turn it into a positive test and
/// remove this note.
#[test]
fn a_key_with_its_description_is_refused_end_to_end_a_known_loss() {
    let blocks = blocks_of(pdf_of(&a_key_with_its_description()));
    assert_eq!(
        tables_in(&blocks),
        0,
        "is-table v2 now keeps this table: the known loss is recovered -- turn this test into a positive one (the table is found end to end) and remove the known-loss note"
    );
}

/// A paper set in two columns: a table in the right one, a justified paragraph
/// in the left beside it (PubTables-1M `PMC1065025`). Every row of the table
/// shares its band with a line of the paragraph, and each such line read as
/// the row's first cell: the paragraph came out as the table's stub column.
#[test]
fn the_paragraph_beside_a_table_is_not_its_first_column() {
    let prose = [
        "The girl was transported to a university hospital. Admission",
        "parameters are presented in Table 1. After rewarming to 32 C",
        "and successful resuscitation, 180 min after admission to the",
        "hospital, haemodynamic stability was achieved with adrenaline",
        "infusion and the child was admitted to the intensive care unit",
        "where the pupils were slightly dilated with reaction to light",
    ];
    let rows = [
        ("Parameter", "Girl", "Boy"),
        ("Pupils dilated", "Yes", "Yes"),
        ("Corneal reflex", "No", "No"),
        ("Temperature", "28", "27"),
        ("pH on admission", "6.63", "7.00"),
        ("CPR time (min)", "180", "120"),
    ];
    let mut pieces = Vec::new();
    for (n, (line, (a, b, c))) in prose.iter().zip(rows).enumerate() {
        let y = 700.0 - n as f64 * PITCH;
        // Justified: every line of the paragraph ends at the same edge.
        pieces.push(mono(&line[..52], STUB, y));
        pieces.push(at(a, 405.0, y));
        pieces.push(at(b, 500.0, y));
        pieces.push(at(c, 550.0, y));
    }
    let blocks = blocks_of(pdf_of(&pieces));
    let markdown = to_markdown(&blocks);
    assert!(
        !blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains("hospital")),
        "the paragraph is not a column of the table:\n{markdown}"
    );
    assert!(cell_at(&blocks, "Corneal reflex").is_some(), "{markdown}");
}
