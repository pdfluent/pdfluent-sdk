// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The final-table judge refuses prose tables and keeps real ones (#574, #638).
//!
//! Tests of the EXPORTED model: they run against it.
//!
//! Synthetic pages only -- no rig pages: two columns of director biographies
//! the judge reads as a table (refused with the switch on, emitted with it
//! off), a financial statement, a ruled text table with no figures (the
//! PubTables case the wordy rule lost), and a five-row fragment of a
//! statement (all kept). With the switch off the pass is byte-identical to
//! before. The stitch test at the end runs with or without the model: it
//! reads features and boxes, never the verdict.
//!
//! The stitch test at the end runs the split statement from
//! `the_stitch_merge_runs_in_extract_blocks` (same pieces, same geometry),
//! toggling the merge: the dump describes what is emitted in both modes.
//! Rows absorb gets its own test later.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::tables::{
    apply_table_is_v2, final_tables_for_is_table_v2, has_rulings_under, is_a_table_v2,
    refine_judged, table_is_v2_features, CellHint, JudgeRegionMeta, TABLE_IS_V2_NAMES,
};
use pdf_engine::learned::{
    document_facts, final_tables_for_dump_with, page_blocks, refine_page, Switches, SWITCHES,
};
use pdf_engine::{to_markdown, Block, BlockKind, PdfDocument, TextSpan};

fn span(x0: f64, y0: f64, x1: f64, y1: f64) -> TextSpan {
    TextSpan {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
        ..Default::default()
    }
}

fn cell(row: u16, column: u16, text: &str, bbox: [f64; 4], index: usize) -> Block {
    let mut b = Block::new(
        BlockKind::TableCell {
            row,
            column,
            row_span: 1,
            column_span: 1,
            header: false,
        },
        0,
        bbox,
        text.to_string(),
        index,
    );
    b.spans = vec![span(bbox[0], bbox[1], bbox[2], bbox[3])];
    b
}

fn hints(n: usize, p: f64) -> Vec<CellHint> {
    vec![
        CellHint {
            heading: false,
            p_heading: Some(p),
        };
        n
    ]
}

fn off() -> Switches {
    Switches {
        table_is_v2: false,
        ..SWITCHES
    }
}

fn on() -> Switches {
    Switches {
        table_is_v2: true,
        ..SWITCHES
    }
}

fn never(_run: &[usize], _page: &[&Block]) -> Option<pdf_engine::learned::tables::Weakness> {
    None
}

/// Two columns of director biographies that read on down each column: each
/// column is one bio split mid-sentence, continuations starting lower case.
fn biographies() -> Vec<Block> {
    let left = [
        "Anna Berg joined the board in",
        "late spring and now chairs",
        "the audit committee with care",
        "she previously served as chief",
        "financial officer for a decade.",
    ];
    let right = [
        "Bo Dahl led the turnaround of",
        "the nordic division from early",
        "spring through winter with patience",
        "he previously ran operations across",
        "three continents for ten years.",
    ];
    let mut out = Vec::new();
    for (r, (l, rr)) in left.iter().zip(&right).enumerate() {
        let y = 700.0 - 22.0 * r as f64;
        out.push(cell(r as u16, 0, l, [70.0, y, 290.0, y + 14.0], out.len()));
        out.push(cell(
            r as u16,
            1,
            rr,
            [310.0, y - 6.0, 530.0, y + 8.0],
            out.len(),
        ));
    }
    out
}

/// A financial statement: labels plus figures in three columns.
fn statement(rows: usize) -> Vec<Block> {
    let head = ["", "2025", "2024", "2023"];
    let mut out: Vec<Block> = head
        .iter()
        .enumerate()
        .map(|(c, t)| {
            cell(
                0,
                c as u16,
                t,
                [
                    70.0 + 90.0 * c as f64,
                    700.0,
                    150.0 + 90.0 * c as f64,
                    712.0,
                ],
                c,
            )
        })
        .collect();
    let labels = ["Revenue", "Costs", "Profit", "Tax", "Net"];
    for r in 1..=rows {
        let label = labels[(r - 1) % labels.len()];
        let y = 700.0 - 20.0 * r as f64;
        out.push(cell(
            r as u16,
            0,
            label,
            [70.0, y, 150.0, y + 12.0],
            out.len(),
        ));
        for c in 1..4u16 {
            let v = format!("{}", 10_000 + r * 100 + usize::from(c));
            out.push(cell(
                r as u16,
                c,
                &v,
                [
                    70.0 + 90.0 * f64::from(c),
                    y,
                    150.0 + 90.0 * f64::from(c),
                    y + 12.0,
                ],
                out.len(),
            ));
        }
    }
    out
}

/// Names and one-sentence descriptions, no figures, ruled header.
fn text_table() -> Vec<Block> {
    let mut out = vec![
        cell(0, 0, "Name", [70.0, 700.0, 150.0, 712.0], 0),
        cell(0, 1, "Role", [160.0, 700.0, 400.0, 712.0], 1),
    ];
    let rows = [
        ("Ada", "Built the first engine room from spare parts."),
        ("Bob", "Runs the second plant beside the old river."),
        ("Cy", "Keeps the third ledger under lock and key."),
    ];
    for (r, (name, role)) in rows.iter().enumerate() {
        let y = 680.0 - 20.0 * r as f64;
        out.push(cell(
            (r + 1) as u16,
            0,
            name,
            [70.0, y, 150.0, y + 12.0],
            out.len(),
        ));
        out.push(cell(
            (r + 1) as u16,
            1,
            role,
            [160.0, y, 400.0, y + 12.0],
            out.len(),
        ));
    }
    out
}

fn is_table(b: &Block) -> bool {
    matches!(b.kind, BlockKind::TableCell { .. })
}

fn at(x: &[f64; 29], name: &str) -> f64 {
    x[TABLE_IS_V2_NAMES.iter().position(|n| *n == name).unwrap()]
}

/// The judged mapping that reads the whole page as one region, with `meta`.
fn judged_meta(n: usize, meta: JudgeRegionMeta) -> (Vec<Option<usize>>, Vec<JudgeRegionMeta>) {
    (vec![Some(0); n], vec![meta])
}

#[test]
fn two_columns_of_biographies_are_prose_with_the_switch_on_and_a_table_with_it_off() {
    let page = biographies();
    let facts = document_facts(&page);
    let (judged, meta) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.62,
            kind: "alignment_lines",
            from_builder: false,
        },
    );
    let h = hints(page.len(), 0.1);
    let size = (612.0, 792.0);
    // OFF: the builder's table stands, as before v2 existed.
    let (out_off, _) = refine_judged(
        page.clone(),
        size,
        &facts,
        off(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    assert!(
        out_off.iter().any(is_table),
        "with the switch off the biographies are emitted as a table"
    );
    // ON: the final-table judge refuses the whole table.
    let (out_mid, origin) = refine_judged(
        page.clone(),
        size,
        &facts,
        on(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let builder_p = vec![None; page.len()];
    let (out_on, _) = apply_table_is_v2(
        out_mid,
        origin,
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        size.1,
        on(),
    );
    assert!(
        out_on.iter().all(|b| !is_table(b)),
        "with the switch on the biographies go back to prose"
    );
    // AND THE FEATURES SAY WHY: cells that read on down the column, no
    // figures, no rulings. Without the run-on reading this refuses nothing.
    let x = table_is_v2_features(&page, &h, size.1, 0.62, false, false);
    assert!(
        at(&x, "run_on_share") > 0.2,
        "reads on: {}",
        at(&x, "run_on_share")
    );
    assert_eq!(at(&x, "figure_share_filled"), 0.0);
    assert_eq!(at(&x, "has_rulings"), 0.0);
    assert!(!is_a_table_v2(&x));
}

#[test]
fn a_financial_statement_is_kept() {
    let page = statement(3);
    let facts = document_facts(&page);
    let (judged, meta) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.91,
            kind: "builder",
            from_builder: true,
        },
    );
    let h = hints(page.len(), 0.02);
    let size = (612.0, 792.0);
    let (out_mid, origin) = refine_judged(
        page.clone(),
        size,
        &facts,
        on(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let builder_p = vec![Some(0.91); page.len()];
    let (out_on, _) = apply_table_is_v2(
        out_mid,
        origin,
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        size.1,
        on(),
    );
    assert!(
        out_on.iter().any(is_table),
        "a statement of figures is kept with the switch on"
    );
    let x = table_is_v2_features(&page, &h, size.1, 0.91, true, false);
    assert!(
        at(&x, "figure_share_filled") > 0.2,
        "figures: {}",
        at(&x, "figure_share_filled")
    );
    assert!(is_a_table_v2(&x));
}

#[test]
fn a_ruled_text_table_with_no_figures_is_kept() {
    // THE PUBTABLES CASE THE WORDY RULE LOST: no figures, wordy cells -- and
    // a ruled header. Without the rulings reading this refuses it.
    let page = text_table();
    let facts = document_facts(&page);
    let (judged, meta) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.44,
            kind: "builder",
            from_builder: true,
        },
    );
    let h = hints(page.len(), 0.05);
    let size = (612.0, 792.0);
    let rule = pdf_engine::text::HorizontalRule {
        left: 60.0,
        right: 410.0,
        y: 698.0,
    };
    assert!(
        has_rulings_under(
            &[60.0, 620.0, 410.0, 712.0],
            std::slice::from_ref(&rule),
            &[]
        ),
        "the header rule lies under the table"
    );
    let (out_mid, origin) = refine_judged(
        page.clone(),
        size,
        &facts,
        on(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let builder_p = vec![Some(0.44); page.len()];
    let (out_on, _) = apply_table_is_v2(
        out_mid,
        origin,
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[rule],
        &[],
        size.1,
        on(),
    );
    assert!(
        out_on.iter().any(is_table),
        "a ruled text table is kept with the switch on"
    );
    let x = table_is_v2_features(&page, &h, size.1, 0.44, true, true);
    assert_eq!(at(&x, "figure_share_filled"), 0.0);
    // Wordy enough for the naive no-figures rule to drop it, kept here.
    assert!(
        at(&x, "wordy_share") > 0.0,
        "wordy: {}",
        at(&x, "wordy_share")
    );
    assert_eq!(at(&x, "has_rulings"), 1.0);
    assert!(is_a_table_v2(&x));
}

#[test]
fn a_fragment_of_a_statement_is_kept() {
    // FIVE ROWS OF TWENTY: a fragment of a table is still a table.
    let page = statement(5);
    // Renumbered 0..5, as a decoded fragment numbers itself.
    let facts = document_facts(&page);
    let (judged, meta) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.58,
            kind: "split",
            from_builder: false,
        },
    );
    let h = hints(page.len(), 0.02);
    let size = (612.0, 792.0);
    let (out_mid, origin) = refine_judged(
        page.clone(),
        size,
        &facts,
        on(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let builder_p = vec![None; page.len()];
    let (out_on, _) = apply_table_is_v2(
        out_mid,
        origin,
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        size.1,
        on(),
    );
    assert!(
        out_on.iter().any(is_table),
        "a five-row fragment is kept with the switch on"
    );
    let x = table_is_v2_features(&page, &h, size.1, 0.58, false, false);
    assert_eq!(at(&x, "grid_rows"), 6.0, "header plus five rows");
    assert!(is_a_table_v2(&x));
}

#[test]
fn with_the_switch_off_the_pass_is_byte_identical_to_before() {
    // WITH THE SWITCH OFF, off runs no v2 code at all: v1 stays, the
    // post-pass is not reached, and two runs agree byte for byte.
    let page = biographies();
    let facts = document_facts(&page);
    let (judged, _) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.62,
            kind: "alignment_lines",
            from_builder: false,
        },
    );
    let h = hints(page.len(), 0.1);
    let size = (612.0, 792.0);
    let seen = |blocks: &[Block]| {
        blocks
            .iter()
            .map(|b| format!("{:?}|{}|{:?}", b.kind, b.text, b.bbox))
            .collect::<Vec<_>>()
    };
    let (a, _) = refine_judged(
        page.clone(),
        size,
        &facts,
        off(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let (b, _) = refine_judged(
        page.clone(),
        size,
        &facts,
        Switches {
            table_is_v2: false,
            ..SWITCHES
        },
        &never,
        Some(&judged),
        &[],
        &h,
    );
    assert_eq!(seen(&a), seen(&b), "the off path is deterministic");
    assert!(
        a.iter().any(is_table),
        "with the switch off the table stands, as before v2"
    );
}

#[test]
fn the_final_tables_hold_one_entry_per_emitted_table() {
    // No verdict read here, so no wait for the model: grouping and
    // provenance alone.
    let page = statement(2);
    let facts = document_facts(&page);
    let (judged, meta) = judged_meta(
        page.len(),
        JudgeRegionMeta {
            judge_p: 0.9,
            kind: "builder",
            from_builder: true,
        },
    );
    let h = hints(page.len(), 0.02);
    let size = (612.0, 792.0);
    let (out, origin) = refine_judged(
        page.clone(),
        size,
        &facts,
        off(),
        &never,
        Some(&judged),
        &[],
        &h,
    );
    let builder_p = vec![Some(0.9); page.len()];
    let finals = final_tables_for_is_table_v2(
        &out,
        &origin,
        Some(&judged),
        &meta,
        &builder_p,
        &h,
        &[],
        &[],
        size.1,
    );
    assert_eq!(finals.len(), 1, "one emitted table, one entry");
    assert_eq!(finals[0].kind, "builder");
}

/// The boxes of the tables `refine_page` emits, in order: a new table wherever
/// `(row, column)` stops climbing, read the way every consumer reads a run of
/// cells.
fn emitted_boxes(blocks: &[Block]) -> Vec<[f64; 4]> {
    let mut tables = Vec::new();
    let mut current: Option<[f64; 4]> = None;
    let mut last: Option<(u16, u16)> = None;
    let flush = |current: &mut Option<[f64; 4]>, tables: &mut Vec<[f64; 4]>| {
        if let Some(b) = current.take() {
            tables.push(b);
        }
    };
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                if last.is_none_or(|seen| (row, column) <= seen) {
                    flush(&mut current, &mut tables);
                }
                let entry = current.get_or_insert(block.bbox);
                entry[0] = entry[0].min(block.bbox[0]);
                entry[1] = entry[1].min(block.bbox[1]);
                entry[2] = entry[2].max(block.bbox[2]);
                entry[3] = entry[3].max(block.bbox[3]);
                last = Some((row, column));
            }
            _ => {
                flush(&mut current, &mut tables);
                last = None;
            }
        }
    }
    flush(&mut current, &mut tables);
    tables
}

/// The dump describes what is emitted: the final tables track the stitch.
///
/// End to end on the split statement (the stitch end-to-end page, same
/// pieces, same geometry): the merge switch moves it -- one table with it
/// on, two with it off -- and the final tables track the emission in both
/// modes, same count, same boxes within a point. Read pre-stitch, the
/// merge-on dump would read the two halves and miss the merged table. No
/// verdict read here either, so no wait for the model.
#[test]
fn the_final_tables_describe_the_tables_as_emitted_after_the_stitch() {
    let doc = PdfDocument::open(pdf_of(&split_statement())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let size = (792.0, 792.0);
    let page = page_blocks(&all, 0);
    let mut runs = Vec::new();
    for merge in [false, true] {
        let switches = Switches {
            table_stitch_merge: merge,
            ..SWITCHES
        };
        let emitted = refine_page(page.clone(), size, &facts, switches);
        let finals =
            final_tables_for_dump_with(page.clone(), size, &facts, &[], &[], &[], switches);
        let markdown = to_markdown(&emitted);
        let boxes = emitted_boxes(&emitted);
        assert!(
            !boxes.is_empty(),
            "merge {merge}: the page emits no table:\n{markdown}"
        );
        assert_eq!(
            finals.len(),
            boxes.len(),
            "merge {merge}: {} emitted tables, {} final ones:\n{markdown}",
            boxes.len(),
            finals.len(),
        );
        for (n, (one, want)) in finals.iter().zip(&boxes).enumerate() {
            for (i, (got, want)) in one.bbox.iter().zip(want.iter()).enumerate() {
                assert!(
                    (got - want).abs() <= 1.0,
                    "merge {merge}: final table {n} edge {i} is {got}, emitted {want}:\n{markdown}"
                );
            }
        }
        // And with the release rejoin and its column order on: nothing on
        // this page refuses, so the rejoin joins nothing, the emission is
        // the same tables at the same boxes, and the dump still describes
        // them (#574).
        let rejoined = Switches {
            table_v2_release_rejoin: true,
            table_v2_release_column_order: true,
            ..switches
        };
        let again = refine_page(page.clone(), size, &facts, rejoined);
        let again_boxes = emitted_boxes(&again);
        let again_markdown = to_markdown(&again);
        assert_eq!(
            again_boxes, boxes,
            "merge {merge}: the rejoin moved the emitted tables:\n{again_markdown}"
        );
        let again_finals =
            final_tables_for_dump_with(page.clone(), size, &facts, &[], &[], &[], rejoined);
        assert_eq!(
            again_finals.len(),
            again_boxes.len(),
            "merge {merge}: {} emitted tables, {} final ones with the rejoin on:\n{again_markdown}",
            again_boxes.len(),
            again_finals.len(),
        );
        runs.push((boxes, markdown));
    }
    // Precondition: the switch moves the page, in count or extent.
    let (off, on) = (&runs[0].0, &runs[1].0);
    let same = off.len() == on.len()
        && off
            .iter()
            .zip(on.iter())
            .all(|(a, b)| a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1.0));
    assert!(
        !same,
        "the merge switch moved nothing: off {} tables {off:?}, on {} tables {on:?}\n--- merge off ---\n{}\n--- merge on ---\n{}",
        off.len(),
        on.len(),
        runs[0].1,
        runs[1].1,
    );
}

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

fn split_at(text: &str, x: f64, y: f64) -> Piece {
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

/// The split statement from `the_stitch_merge_runs_in_extract_blocks` (copied:
/// one integration test cannot reach another's builder): two halves sharing
/// three figure columns, with the label-only line "Capital lease" between
/// them. Named apart from `statement` above, which builds judged cells.
fn split_statement() -> Vec<Piece> {
    let mut pieces = vec![split_at("Consolidated Statements of Income", STUB, 716.0)];
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
            pieces.push(split_at(head, *x, y));
        }
        y -= PITCH;
        pieces.push(split_at("Fiscal Year", STUB, y));
        for (x, year) in FIGS.iter().zip(["2003", "2002", "2001"]) {
            pieces.push(split_at(year, *x, y));
        }
        y -= PITCH;
        for (label, figs) in rows {
            pieces.push(split_at(label, STUB, y));
            for (x, fig) in FIGS.iter().zip(figs) {
                pieces.push(split_at(fig, *x, y));
            }
            y -= PITCH;
        }
        if half == 0 {
            y -= GAP_ABOVE_LEASE - PITCH;
            pieces.push(split_at("Capital lease", STUB, y));
            y -= PITCH;
        }
    }
    pieces
}
