// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A refused table goes back to prose as paragraphs, not one block per cell
//! (#574).
//!
//! When `table_is_v2` refuses a final table, its cells are released back to
//! prose. With `table_v2_release_rejoin` on, a cell joins the block above it
//! within its column where m157's narrow-column pair conditions hold, and a
//! block built from two cells or more is a paragraph; with
//! `table_v2_release_column_order` on as well, a rejoined refusal of two
//! columns or more is emitted column by column rather than row-major.
//!
//! The operator page is two run-on prose columns the pair detector tables
//! and the final-table judge refuses, with a wider gap between the third and
//! fourth rows so row-major collapse and column order differ. The arms go
//! through `refine_page` with the switches set explicitly and read the
//! markdown, the shipped default (rejoin on) through `extract_blocks`; the
//! NYSE page pins the real-type end state now that its marker column fuses
//! and the builder tables nothing there. Take the rejoin out and the on
//! arms go red with one block per line.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::tables::{apply_table_is_v2, CellHint, JudgeRegionMeta};
use pdf_engine::learned::{
    document_facts, page_blocks, refine_page, table_signals_with, Switches, SWITCHES,
};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument, TextSpan};

/// The left column: run-on fragments of sixteen characters or fewer, so the
/// pair detector tables the page, wordy enough for the judge to refuse it.
const COL0: [&str; 5] = [
    "Anna Berg joined",
    "the board in late",
    "spring and now",
    "chairs the audit",
    "group each year",
];

/// The right column: longer run-on fragments.
const COL1: [&str; 5] = [
    "she previously served as chief",
    "financial officer for a decade",
    "across three continents with",
    "patience through every season",
    "and retired last spring.",
];

struct Piece {
    text: &'static str,
    x: f64,
    y: f64,
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
            vec![Object::Name(b"F1".to_vec()), 10.0.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.text.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
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

/// Two run-on prose columns the builder tables and the final-table judge
/// refuses, with a wider gap between the third and fourth rows: both columns
/// break there, so row-major collapse reads `[left-above, right-above,
/// left-below, right-below]` and column order reads the left column whole
/// first.
fn directory() -> Vec<u8> {
    let mut pieces = Vec::new();
    for r in 0..5 {
        let y = 700.0 - 18.0 * r as f64 - if r >= 3 { 18.0 } else { 0.0 };
        pieces.push(Piece {
            text: COL0[r],
            x: 72.0,
            y,
        });
        pieces.push(Piece {
            text: COL1[r],
            x: 220.0,
            y,
        });
    }
    pdf_of(&pieces)
}

fn rejoin(switches: Switches) -> Vec<Block> {
    let doc = PdfDocument::open(directory()).expect("open synthetic pdf");
    let before = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&before);
    let page = page_blocks(&before, 0);
    // The builder tables the page: without the table there is no refusal and
    // this test says nothing.
    assert!(
        before
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. })),
        "the builder tabled nothing"
    );
    refine_page(page, (612.0, 792.0), &facts, switches)
}

fn off() -> Switches {
    Switches {
        table_v2_release_rejoin: false,
        table_v2_release_column_order: false,
        ..SWITCHES
    }
}

fn on() -> Switches {
    Switches {
        table_v2_release_rejoin: true,
        table_v2_release_column_order: false,
        ..SWITCHES
    }
}

fn ordered() -> Switches {
    Switches {
        table_v2_release_rejoin: true,
        table_v2_release_column_order: true,
        ..SWITCHES
    }
}

const LEFT_ABOVE: &str = "Anna Berg joined the board in late spring and now";
const LEFT_BELOW: &str = "chairs the audit group each year";
const RIGHT_ABOVE: &str =
    "she previously served as chief financial officer for a decade across three continents with";
const RIGHT_BELOW: &str = "patience through every season and retired last spring.";

#[test]
fn with_the_rejoin_off_a_refused_table_is_one_block_per_line() {
    // The off state, said explicitly: ten cells, ten blocks, row-major,
    // each cell its own text. Reading it from the shipped switches broke
    // the day the rejoin shipped on.
    let blocks = rejoin(off());
    assert_eq!(blocks.len(), 10, "one block per cell");
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    let mut want = Vec::new();
    for r in 0..5 {
        want.push(COL0[r]);
        want.push(COL1[r]);
    }
    assert_eq!(texts, want, "row-major, each cell its own text");
    assert!(
        blocks
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the table stands, so nothing was refused"
    );
}

#[test]
fn the_shipped_default_rejoins_a_refused_table_into_four_paragraphs() {
    // The on shape, under the shipped switches: the rejoin ships on, so
    // the product path joins the ten cells into four paragraphs in
    // row-major order. Take the rejoin out and this goes red with one
    // block per line.
    let doc = PdfDocument::open(directory()).expect("open synthetic pdf");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [LEFT_ABOVE, RIGHT_ABOVE, LEFT_BELOW, RIGHT_BELOW],
        "one paragraph per column part, where its first cell stood"
    );
    for block in &blocks {
        assert_eq!(
            block.kind,
            BlockKind::Paragraph,
            "a joined block is a paragraph: {:?}",
            block.text
        );
    }
    assert!(
        blocks
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the table stands, so nothing was refused"
    );
}

#[test]
fn a_refused_two_column_table_comes_back_as_paragraphs() {
    let blocks = rejoin(on());
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [LEFT_ABOVE, RIGHT_ABOVE, LEFT_BELOW, RIGHT_BELOW],
        "one paragraph per column part, where its first cell stood"
    );
    for block in &blocks {
        assert_eq!(
            block.kind,
            BlockKind::Paragraph,
            "a joined block is a paragraph: {:?}",
            block.text
        );
        assert_eq!(block.confidence, None);
        assert_eq!(block.grid, None);
    }
    let markdown = to_markdown(&blocks);
    for part in [LEFT_ABOVE, RIGHT_ABOVE, LEFT_BELOW, RIGHT_BELOW] {
        assert!(
            markdown.contains(part),
            "the markdown lost {part:?}:\n{markdown}"
        );
    }
}

#[test]
fn with_the_order_switch_a_rejoined_refusal_reads_column_by_column() {
    let blocks = rejoin(ordered());
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [LEFT_ABOVE, LEFT_BELOW, RIGHT_ABOVE, RIGHT_BELOW],
        "the left column top to bottom, then the right one"
    );
}

#[test]
fn the_order_switch_alone_changes_nothing() {
    // Without the rejoin there is no joined block, so the order condition
    // never holds and the release is still one block per cell.
    let blocks = rejoin(Switches {
        table_v2_release_rejoin: false,
        table_v2_release_column_order: true,
        ..SWITCHES
    });
    assert_eq!(blocks.len(), 10, "one block per cell");
}

/// A refused cell with its type: one span, one face, fourteen-point steps,
/// so a column of them joins.
fn typed_cell(
    row: u16,
    column: u16,
    text: &str,
    bold: bool,
    index: usize,
    column_span: u16,
    row_span: u16,
) -> Block {
    let y = 700.0 - 14.0 * f64::from(row);
    let x = 70.0 + 150.0 * f64::from(column);
    let mut block = Block::new(
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header: false,
        },
        0,
        [x, y, x + 140.0, y + 12.0],
        text.to_string(),
        index,
    );
    block.spans = vec![TextSpan {
        x,
        y: y + 2.0,
        width: 100.0,
        height: 10.0,
        font_size: 10.0,
        font_name: Some(if bold { "Helvetica-Bold" } else { "Helvetica" }.to_string()),
        is_bold: bold,
        text: text.to_string(),
        ..Default::default()
    }];
    block
}

fn plain_cell(row: u16, column: u16, text: &str, index: usize) -> Block {
    typed_cell(row, column, text, false, index, 1, 1)
}

fn directory_cells() -> Vec<Block> {
    let mut page = Vec::new();
    for r in 0..5u16 {
        for (c, text) in [COL0[usize::from(r)], COL1[usize::from(r)]]
            .iter()
            .enumerate()
        {
            let index = page.len();
            page.push(plain_cell(r, c as u16, text, index));
        }
    }
    page
}

fn hints(n: usize, heading: Option<usize>) -> Vec<CellHint> {
    (0..n)
        .map(|i| CellHint {
            heading: heading == Some(i),
            p_heading: Some(if heading == Some(i) { 0.9 } else { 0.1 }),
        })
        .collect()
}

fn judged_meta(n: usize) -> (Vec<Option<usize>>, Vec<JudgeRegionMeta>) {
    (
        vec![Some(0); n],
        vec![JudgeRegionMeta {
            judge_p: 0.62,
            kind: "alignment_lines",
            from_builder: false,
        }],
    )
}

#[test]
fn a_bold_heading_cell_stays_its_own_block_and_stays_a_heading() {
    // A bold line over regular ones: another weight, so no join either way,
    // and its hint says heading, so it keeps its heading while the regulars
    // join into paragraphs around it.
    let mut page = directory_cells();
    page[0] = typed_cell(0, 0, "NOTE", true, 0, 1, 1);
    let (judged, meta) = judged_meta(page.len());
    let h = hints(page.len(), Some(0));
    let builder_p = vec![None; page.len()];
    let (out, _) = apply_table_is_v2(
        page,
        (0..10).collect(),
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        792.0,
        ordered(),
    );
    assert!(
        out.iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the table stands, so nothing was refused"
    );
    assert_eq!(
        out.len(),
        3,
        "the note, the rest of its column, the other column"
    );
    assert_eq!(out[0].kind, BlockKind::Heading { level: 1 });
    assert_eq!(out[0].text, "NOTE");
    assert_eq!(out[0].confidence, Some(0.9));
    assert_eq!(out[1].kind, BlockKind::Paragraph);
    assert_eq!(
        out[1].text,
        "the board in late spring and now chairs the audit group each year"
    );
    assert_eq!(out[2].kind, BlockKind::Paragraph);
    assert_eq!(
        out[2].text,
        "she previously served as chief financial officer for a decade \
        across three continents with patience through every season \
        and retired last spring."
    );
}

#[test]
fn a_spanning_cell_never_joins_and_is_never_joined_onto() {
    // A cell across two columns and a cell across two rows: each breaks its
    // column into segments and stays its own block, while the rest joins
    // around them.
    let mut page = directory_cells();
    page[4] = typed_cell(2, 0, "spring and now", false, 4, 2, 1);
    page[7] = typed_cell(3, 1, "patience through every season", false, 7, 1, 2);
    let (judged, meta) = judged_meta(page.len());
    let h = hints(page.len(), None);
    let builder_p = vec![None; page.len()];
    let (out, _) = apply_table_is_v2(
        page,
        (0..10).collect(),
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        792.0,
        ordered(),
    );
    assert!(
        out.iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the table stands, so nothing was refused"
    );
    let texts: Vec<&str> = out.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Anna Berg joined the board in late",
            "spring and now",
            "chairs the audit group each year",
            "she previously served as chief financial officer for a decade across three continents with",
            "patience through every season",
            "and retired last spring.",
        ]
    );
}

#[test]
fn a_kept_table_is_byte_identical_with_the_switch_on() {
    let head = ["", "2025", "2024", "2023"];
    let mut page: Vec<Block> = Vec::new();
    for (c, t) in head.iter().enumerate() {
        let x = 70.0 + 90.0 * c as f64;
        let index = page.len();
        let mut block = Block::new(
            BlockKind::TableCell {
                row: 0,
                column: c as u16,
                row_span: 1,
                column_span: 1,
                header: false,
            },
            0,
            [x, 700.0, x + 80.0, 712.0],
            t.to_string(),
            index,
        );
        block.spans = vec![TextSpan {
            x,
            y: 702.0,
            width: 60.0,
            height: 10.0,
            font_size: 10.0,
            font_name: Some("Helvetica".to_string()),
            text: t.to_string(),
            ..Default::default()
        }];
        page.push(block);
    }
    for r in 1..=3u16 {
        let y = 700.0 - 20.0 * f64::from(r);
        for c in 0..4u16 {
            let text = if c == 0 {
                ["Revenue", "Costs", "Profit"][usize::from(r) - 1].to_string()
            } else {
                format!("{}", 10_000 + usize::from(r) * 100 + usize::from(c))
            };
            let x = 70.0 + 90.0 * f64::from(c);
            let index = page.len();
            let mut block = Block::new(
                BlockKind::TableCell {
                    row: r,
                    column: c,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                0,
                [x, y, x + 80.0, y + 12.0],
                text.clone(),
                index,
            );
            block.spans = vec![TextSpan {
                x,
                y: y + 2.0,
                width: 60.0,
                height: 10.0,
                font_size: 10.0,
                font_name: Some("Helvetica".to_string()),
                text,
                ..Default::default()
            }];
            page.push(block);
        }
    }
    let seen = |blocks: &[Block]| {
        blocks
            .iter()
            .map(|b| {
                format!(
                    "{:?}|{}|{:?}|{:?}|{:?}|{}|{:?}",
                    b.kind, b.text, b.bbox, b.confidence, b.grid, b.index, b.spans
                )
            })
            .collect::<Vec<_>>()
    };
    let (judged, meta) = (
        vec![Some(0); page.len()],
        vec![JudgeRegionMeta {
            judge_p: 0.91,
            kind: "builder",
            from_builder: true,
        }],
    );
    let h = hints(page.len(), None);
    let builder_p = vec![Some(0.91); page.len()];
    let (without, _) = apply_table_is_v2(
        page.clone(),
        (0..page.len()).collect(),
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        792.0,
        off(),
    );
    let (with, _) = apply_table_is_v2(
        page,
        (0..without.len()).collect(),
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        792.0,
        ordered(),
    );
    assert!(
        without
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. })),
        "the statement refused, so this test says nothing"
    );
    assert_eq!(seen(&with), seen(&without), "a kept table moved");
}

/// On real type: the same two-column governance page, now that its marker
/// column fuses back onto its lines. The builder tabled the debris row-major
/// and the judge refused it; with the debris gone the builder tables nothing,
/// the rejoin idles, the bullets are items, and the flowing sentences are
/// whole paragraphs. The rejoin switch itself stays pinned by the synthetic
/// arms above; this pins the real-type end state.
#[test]
fn a_real_two_column_page_with_its_debris_gone_tables_nothing() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let (before, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let sizes: Vec<(f64, f64)> = (0..doc.page_count())
        .map(|p| {
            doc.page_geometry(p)
                .map(|g| g.effective_dimensions())
                .unwrap_or((0.0, 0.0))
        })
        .collect();
    let rulings: Vec<Vec<pdf_engine::text::Ruling>> = (0..doc.page_count())
        .map(|p| doc.page_rulings(p).expect("rulings"))
        .collect();
    let images = doc.extract_image_boxes().expect("images");
    let refined = |switches: Switches| {
        pdf_engine::learned::refine_with_images(
            before.clone(),
            &sizes,
            &rules,
            &rulings,
            &images,
            switches,
        )
    };
    let without = refined(off());
    let with = refined(on());
    // The builder tables nothing: no cells, no judged table, and the rejoin
    // joins nothing because nothing was refused.
    assert!(
        before
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the builder tabled the page after all"
    );
    let signals = |switches: Switches| {
        table_signals_with(&before, &sizes, &rules, &rulings, &images, switches)
            .into_iter()
            .flatten()
            .flat_map(|page| page.tables)
            .collect::<Vec<_>>()
    };
    assert!(
        signals(off()).is_empty() && signals(on()).is_empty(),
        "the judge saw a table on the page"
    );
    assert_eq!(
        with.len(),
        without.len(),
        "the rejoin joined, so something was refused"
    );
    // The bullets are items, markers off, each its own block.
    for item in [
        "assisting the board and management in monitoring the effectiveness of the company’s risk management process; and",
        "assisting the board and management in maintaining effective controls by evaluating those controls on an ongoing basis to determine their efficiency and effectiveness and to  recommend improvements.",
        "the information management environment;",
        "the reliability and integrity of financial and operating information;",
        "the safeguarding of assets; and",
        "the effective and efficient use of the company’s resources.",
    ] {
        assert!(
            with.iter().any(
                |b| matches!(b.kind, BlockKind::ListItem { .. }) && b.text == item
            ),
            "the bulleted line joined, vanished, or kept its marker: {item:?}"
        );
    }
    // One bullet on its own is still a coincidence, not a list: the
    // heuristics' own verdict, before the learned pass reads it.
    assert!(
        before.iter().any(|b| b.kind == BlockKind::Paragraph
            && b.text
                == "● a documented organisational structure and reasonable division of responsibility;"),
        "the singleton bullet became an item or vanished"
    );
    // The flowing column is whole paragraphs, not cells.
    assert!(
        with.iter().any(|b| b.kind == BlockKind::Paragraph
            && b.text
                == "The directors are ultimately responsible for the group’s system of internal control, which is designed to provide reasonable assurance against material misstatement and loss. The group maintains a system of internal financial control that is designed to provide assurances regarding the maintenance of proper accounting records and the reliability of financial information used within the business for publication."),
        "the flowing column did not come back as whole paragraphs"
    );
    assert!(
        with.iter().all(|b| b.text.trim() != "●"),
        "a lone bullet survived as a block of its own"
    );
}
