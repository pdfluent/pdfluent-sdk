//! A white corridor splits a decoder stripe the bridge model left joined (#574).
//!
//! `bands` puts a word into the current stripe when its x-extent overlaps any
//! word already in it, so one word the bridge model does not flag -- a long
//! row label, a note line inside the region -- joins two columns and every row
//! below them before any column model is asked. After `across` builds the
//! stripes, each stripe of at least `CORRIDOR_MIN_LINES` lines is checked for
//! a white corridor: an x-interval free of ink in at least `CORRIDOR_SHARE` of
//! its lines and at least `CORRIDOR_EM` wide. Where one exists the words
//! crossing it span the columns, as model-flagged bridges do, and the two
//! sides become two stripes, so the column model scores the gap.
//!
//! The first two tests are the positives: a long body label crossing a gutter
//! (3 columns out, the label spanning), and the same shape over ten body rows
//! with a full commit row. Both assert the switch-off reading too -- the
//! columns merged -- so the red is locked in, not just the green. The third
//! positive pins the width: right-aligned figures of different widths whose
//! common corridor is 1.2 em while every line's own gap is wider, splitting
//! at 1.0 em and joined at 1.5 em. The last two are the guards: a three-line
//! annex whose gap is free in 2 of 3 lines, and a six-line ragged annex whose
//! gaps never align, both staying one column. Their red is a constants flip,
//! not the switch: share 0.5 (plus min-lines 1) for the first, share 0.3 for
//! the second, both with V2, split the annex in two. A row-0 or mid-table
//! span would be the spec's header test, but the bridge model flags every
//! span over a numeric pair or past a stripe's edge, so the positives span
//! from the edge near the foot instead.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::{Switches, SWITCHES};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })));
    let pages_id = doc.new_object_id();
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
            vec![Object::string_literal(piece.text.as_str())],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode"),
    ));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
            }),
        }),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => 1,
        }),
    );
    let catalogue = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalogue));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn blocks_of(pieces: &[Piece], corridors: bool) -> Vec<Block> {
    let switches = Switches {
        table_corridor_split: corridors,
        ..SWITCHES
    };
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks_with_switches(&ExtractOptions::default(), switches)
        .expect("extract blocks")
}

/// (row, column, column span, text) of every table cell, in block order.
fn cells_of(blocks: &[Block]) -> Vec<(u16, u16, u16, String)> {
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

fn body_rows() -> Vec<(String, String, String)> {
    let stubs = [
        "Item A", "Item B", "Item C", "Item D", "Item E", "Item F", "Item G", "Item H", "Item I",
        "Item J",
    ];
    let words = [
        "Accepted form",
        "Rejected form",
        "Approved form",
        "Declined form",
        "Resolved form",
        "Reopened case",
        "Verified case",
        "Filed form",
        "Closed form",
        "Opened case",
        "Signed form",
        "Sealed case",
    ];
    stubs
        .iter()
        .zip(words.iter())
        .enumerate()
        .map(|(r, (stub, word))| (stub.to_string(), format!("1{r}.3"), word.to_string()))
        .collect()
}

/// Six body rows, a long label crossing the stub-figure gutter, a commit row.
fn layout_label_crossing() -> Vec<Piece> {
    let rows = body_rows();
    let mut pieces = Vec::new();
    for (r, (stub, fig, word)) in rows.iter().take(6).enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: stub.clone(),
            x: 40.0,
            y,
        });
        pieces.push(Piece {
            text: fig.clone(),
            x: 110.0,
            y,
        });
        pieces.push(Piece {
            text: word.clone(),
            x: 185.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications".to_string(),
        x: 40.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: rows[6].2.clone(),
        x: 185.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Item H".to_string(),
        x: 40.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces.push(Piece {
        text: rows[7].2.clone(),
        x: 185.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces
}

/// Ten body rows, the crossing label, and a full commit row: the judge takes
/// a clean ten-row sub-table over a trailing span with a short commit, so the
/// commit carries its figure.
fn layout_label_over_ten_rows() -> Vec<Piece> {
    let rows = body_rows();
    let mut pieces = Vec::new();
    for (r, (stub, fig, word)) in rows.iter().take(10).enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: stub.clone(),
            x: 40.0,
            y,
        });
        pieces.push(Piece {
            text: fig.clone(),
            x: 110.0,
            y,
        });
        pieces.push(Piece {
            text: word.clone(),
            x: 185.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications".to_string(),
        x: 40.0,
        y: TOP - 10.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Signed form".to_string(),
        x: 185.0,
        y: TOP - 10.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Item K".to_string(),
        x: 40.0,
        y: TOP - 11.0 * PITCH,
    });
    pieces.push(Piece {
        text: "111.3".to_string(),
        x: 110.0,
        y: TOP - 11.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Sealed case".to_string(),
        x: 185.0,
        y: TOP - 11.0 * PITCH,
    });
    pieces
}

/// The crossing label's gap is a corridor: 3 columns, the label spanning.
/// With the switch off the stub and figure columns come out merged.
#[test]
fn a_long_body_label_spanning_yields_three_columns() {
    let pieces = layout_label_crossing();
    let cells = cells_of(&blocks_of(&pieces, true));
    assert_eq!(
        cells.iter().map(|c| c.1).max(),
        Some(2),
        "the corridor did not split the joined stripe: {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|c| c.3 == "Telecommunications" && c.1 == 0 && c.2 == 2),
        "the crossing label did not span two columns: {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|c| c.0 == 0 && c.3 == "Item A" && c.1 == 0)
            && cells.iter().any(|c| c.0 == 0 && c.3 == "10.3" && c.1 == 1)
            && cells
                .iter()
                .any(|c| c.0 == 0 && c.3 == "Accepted form" && c.1 == 2),
        "the first body row did not split in three: {cells:?}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert_eq!(
        off.iter().map(|c| c.1).max(),
        Some(1),
        "without the rule the columns should stay merged: {off:?}"
    );
    assert!(
        off.iter().any(|c| c.3 == "Item A 10.3" && c.1 == 0),
        "without the rule the stub and figure should merge: {off:?}"
    );
}

/// The same shape over ten body rows: the numeric columns stay separate and
/// the label spans them. With the switch off they merge.
#[test]
fn a_spanning_label_over_ten_body_rows_keeps_three_columns() {
    let pieces = layout_label_over_ten_rows();
    let cells = cells_of(&blocks_of(&pieces, true));
    assert_eq!(
        cells.iter().map(|c| c.1).max(),
        Some(2),
        "the corridor did not split the joined stripe: {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|c| c.3 == "Telecommunications" && c.0 == 10 && c.1 == 0 && c.2 == 2),
        "the crossing label did not span two columns: {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|c| c.0 == 11 && c.3 == "Item K" && c.1 == 0)
            && cells
                .iter()
                .any(|c| c.0 == 11 && c.3 == "111.3" && c.1 == 1)
            && cells
                .iter()
                .any(|c| c.0 == 11 && c.3 == "Sealed case" && c.1 == 2),
        "the commit row did not split in three: {cells:?}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert_eq!(
        off.iter().map(|c| c.1).max(),
        Some(1),
        "without the rule the columns should stay merged: {off:?}"
    );
}

/// Eight body rows of figures, a crossing label, a commit row. Column A is
/// left-aligned with figures of different widths, so its right edge is
/// ragged (53.90 to 65.02 on the clean lines); columns B and C are
/// right-aligned to 108.0 and 260.0 with figures of different widths, so
/// their left edges are ragged (77.42 to 94.10 for B). Every line's own A-B
/// gap is at least 15.18 (1.52 em at the 10 pt word height), but the strip
/// all but one line leave alone is (65.02, 77.42): 12.40 wide, 1.24 em -- a
/// corridor at 1.0 em and none at 1.5 em (KSS_2009 p63). Row 7's wide A
/// figure reaches to 78.92, chaining the A and B stripes through the wide B
/// figures above it; it sits in a full body row over one column below, so
/// the bridge model leaves it in and the corridor is what splits.
fn layout_figures_with_a_narrow_common_corridor() -> Vec<Piece> {
    // (A figure, B figure at its right-aligned x, C figure at its own).
    let rows = [
        ("12.3", "12.3", 88.54, "123.4", 234.98),
        ("1.3", "1234.5", 77.42, "12.3", 240.54),
        ("123.4", "1.3", 94.10, "1.3", 246.10),
        ("12.3", "1234.5", 77.42, "1234.5", 229.42),
        ("1.3", "12.3", 88.54, "12.3", 240.54),
        ("123.4", "123.4", 82.98, "1.3", 246.10),
        ("12.3", "1.3", 94.10, "123.4", 234.98),
        ("1234567", "1.3", 94.10, "12.34", 234.98),
    ];
    let mut pieces = Vec::new();
    for (r, (a, b, bx, c, cx)) in rows.iter().enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: a.to_string(),
            x: 40.0,
            y,
        });
        pieces.push(Piece {
            text: b.to_string(),
            x: *bx,
            y,
        });
        pieces.push(Piece {
            text: c.to_string(),
            x: *cx,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications".to_string(),
        x: 40.0,
        y: TOP - 8.0 * PITCH,
    });
    pieces.push(Piece {
        text: "12.3".to_string(),
        x: 240.54,
        y: TOP - 8.0 * PITCH,
    });
    pieces.push(Piece {
        text: "12.3".to_string(),
        x: 40.0,
        y: TOP - 9.0 * PITCH,
    });
    pieces.push(Piece {
        text: "12.3".to_string(),
        x: 88.54,
        y: TOP - 9.0 * PITCH,
    });
    pieces.push(Piece {
        text: "123.4".to_string(),
        x: 234.98,
        y: TOP - 9.0 * PITCH,
    });
    pieces
}

/// The 1.2 em common corridor splits the joined figure columns in three at
/// 1.0 em; at 1.5 em the corridor is too narrow and they stay joined. With
/// the switch off they stay joined too.
#[test]
fn a_narrow_common_corridor_splits_right_aligned_figures() {
    let pieces = layout_figures_with_a_narrow_common_corridor();
    let cells = cells_of(&blocks_of(&pieces, true));
    assert_eq!(
        cells.iter().map(|c| c.1).max(),
        Some(2),
        "the 1.2 em corridor did not split the joined figures: {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|c| c.3 == "Telecommunications" && c.0 == 8 && c.1 == 0 && c.2 == 2),
        "the crossing label did not span two columns: {cells:?}"
    );
    assert!(
        cells.iter().any(|c| c.0 == 0 && c.3 == "12.3" && c.1 == 0)
            && cells.iter().any(|c| c.0 == 0 && c.3 == "12.3" && c.1 == 1)
            && cells.iter().any(|c| c.0 == 0 && c.3 == "123.4" && c.1 == 2),
        "the first body row did not split in three: {cells:?}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert_eq!(
        off.iter().map(|c| c.1).max(),
        Some(1),
        "without the rule the figure columns should stay merged: {off:?}"
    );
}

/// A three-line annex with an aligned gap: free in 2 of 3 lines, which is
/// under the share, so the annex stays one column. Flip the share to 0.5
/// with min-lines 1 and V2 and it splits in two.
#[test]
fn a_two_line_aligned_gap_does_not_split() {
    let mut pieces = layout_label_crossing();
    for r in [4, 5] {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: "alpha".to_string(),
            x: 315.5,
            y,
        });
        pieces.push(Piece {
            text: "text".to_string(),
            x: 370.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Altruist".to_string(),
        x: 339.5,
        y: TOP - 6.0 * PITCH,
    });
    let cells = cells_of(&blocks_of(&pieces, true));
    let annex: Vec<(u16, u16, u16, String)> = cells
        .iter()
        .filter(|c| c.3 == "alpha text" || c.3 == "Altruist")
        .cloned()
        .collect();
    assert_eq!(annex.len(), 3, "the annex left the table: {cells:?}");
    assert!(
        annex.iter().all(|c| c.1 == 3 && c.2 == 1),
        "the annex split without a corridor: {cells:?}"
    );
    assert_eq!(
        cells.iter().map(|c| c.1).max(),
        Some(3),
        "the body lost a column: {cells:?}"
    );
}

/// A six-line ragged annex: no strip is free in 5 of 6 lines, so it stays
/// one column. Flip the share to 0.3 with V2 and it splits in two.
#[test]
fn a_ragged_six_line_annex_does_not_split() {
    let mut pieces = layout_label_crossing();
    for r in [1, 3, 5] {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: "alpha".to_string(),
            x: 315.5,
            y,
        });
        pieces.push(Piece {
            text: "bodytext".to_string(),
            x: 354.0,
            y,
        });
    }
    for r in [2, 4] {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: "alphabet".to_string(),
            x: 311.6,
            y,
        });
        pieces.push(Piece {
            text: "bodytext".to_string(),
            x: 368.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "International".to_string(),
        x: 335.0,
        y: TOP - 6.0 * PITCH,
    });
    let cells = cells_of(&blocks_of(&pieces, true));
    let annex: Vec<(u16, u16, u16, String)> = cells.iter().filter(|c| c.1 == 3).cloned().collect();
    assert_eq!(annex.len(), 6, "the annex left the table: {cells:?}");
    assert!(
        annex.iter().all(|c| c.2 == 1),
        "the annex split without a corridor: {cells:?}"
    );
    assert!(
        annex.iter().any(|c| c.3 == "International"),
        "the spanner left the annex column: {cells:?}"
    );
    assert_eq!(
        cells.iter().map(|c| c.1).max(),
        Some(3),
        "the body lost a column: {cells:?}"
    );
}
