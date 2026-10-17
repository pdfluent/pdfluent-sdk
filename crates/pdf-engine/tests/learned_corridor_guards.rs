//! The corridor guards hold the splits a ragged edge would make (#574).
//!
//! The shipped corridor splits any stripe a white strip runs through, and on
//! the TUNING pages that over-fires: wrapped prose with a ragged right edge
//! gains a column, tall two-column tables split to three and four, and six
//! tables lose their all-`th` header row as collateral. G1 holds a split
//! unless both sides hold ink on most body lines, G2 unless an edge aligns,
//! G3 drops splits that cost header rows, and G4 vetoes narrow tables unless
//! G1 and G2 both pass stricter. The TUNING grid ships G1 at 0.7 alone.
//!
//! The first test is the shape from the TUNING worsenings: stub, figure and
//! word joined by a spanning label, but the figure column holds ink on four
//! of six body rows, a ragged edge where the pages wrap text. The shipped
//! corridor splits it in three; G1 vetoes the stub-fig gutter and holds it
//! at two. The second is the counterweight: the same shape with a full
//! figure column of aligned right edges still splits. The third pins the
//! header collateral: the split shreds the first row across its extra
//! column, the veto keeps it whole in the emitted markdown. Each test
//! asserts the switch-off reading too, so the red is locked in, not just
//! the green.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::{Switches, SWITCHES};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

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

/// Six body rows over stub, figure and word, a crossing label, a commit
/// row -- the corridor test's joined shape -- but the figure column holds
/// ink on four of six body rows, a ragged edge where TUNING pages wrap
/// text. The stub-fig corridor G1 vetoes (east ink on three of seven body
/// lines), holding the table at two columns where the shipped split cuts
/// three.
fn layout_wrapped_prose() -> Vec<Piece> {
    let stubs = ["Item A", "Item B", "Item C", "Item D", "Item E", "Item F"];
    let figs = ["10.3", "51.7", "12.3", "4.56"];
    let words = [
        "Accepted form",
        "Rejected form",
        "Approved form",
        "Declined form",
        "Resolved form",
        "Reopened case",
    ];
    let mut pieces = Vec::new();
    for (r, stub) in stubs.iter().enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: stub.to_string(),
            x: 40.0,
            y,
        });
        if r == 0 || r == 1 || r == 4 || r == 5 {
            pieces.push(Piece {
                text: figs[if r < 2 { r } else { r - 2 }].to_string(),
                x: 110.0,
                y,
            });
        }
        pieces.push(Piece {
            text: words[r].to_string(),
            x: 220.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications grid".to_string(),
        x: 40.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Verified case".to_string(),
        x: 220.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Item H".to_string(),
        x: 40.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Filed form".to_string(),
        x: 220.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces
}

#[test]
fn the_guards_hold_a_wrapped_stripe_the_shipped_split_cuts() {
    let pieces = layout_wrapped_prose();
    let cells = cells_of(&blocks_of(&pieces, true));
    assert!(
        !cells.is_empty(),
        "the wrapped stripe left no table: {cells:?}"
    );
    assert!(
        cells.iter().map(|c| c.1).max() == Some(1),
        "the guards did not hold the ragged edge at two columns: {cells:?}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert!(
        off.iter().map(|c| c.1).max() == Some(1),
        "the switch-off reading is not the two-column reference: {off:?}"
    );
}

/// A figure table with aligned right edges over stub, figure and word,
/// the stub-fig gutter crossed by a label near the foot: both sides hold
/// ink on every body line, so G1 lets the corridor split. (Two bands never
/// reach the decoder's rebuild -- the builder's clean two columns stand --
/// so the true gutter is pinned with three, where the shipped split cuts.)
fn layout_figure_table() -> Vec<Piece> {
    let stubs = ["Item A", "Item B", "Item C", "Item D", "Item E", "Item F"];
    let figs = ["10.3", "51.7", "12.3", "45.6", "1.2", "34.5"];
    let words = [
        "Accepted form",
        "Rejected form",
        "Approved form",
        "Declined form",
        "Resolved form",
        "Reopened case",
    ];
    let mut pieces = Vec::new();
    for (r, stub) in stubs.iter().enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: stub.to_string(),
            x: 40.0,
            y,
        });
        // Right edges aligned at 150: the gutter's east side is a figure column.
        let width = 6.0 * figs[r].len() as f64;
        pieces.push(Piece {
            text: figs[r].to_string(),
            x: 150.0 - width,
            y,
        });
        pieces.push(Piece {
            text: words[r].to_string(),
            x: 220.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications grid".to_string(),
        x: 40.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Verified case".to_string(),
        x: 220.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Item H".to_string(),
        x: 40.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Filed form".to_string(),
        x: 220.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces
}

#[test]
fn an_aligned_figure_table_still_splits() {
    let pieces = layout_figure_table();
    let cells = cells_of(&blocks_of(&pieces, true));
    assert!(
        cells.iter().map(|c| c.1).max() == Some(2),
        "the guards vetoed a true figure gutter: {cells:?}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert!(
        off.iter().map(|c| c.1).max() == Some(1),
        "the switch-off reading is not the merged reference: {off:?}"
    );
}

/// The wrapped stripe's first row reads `Item A 10.3 | Accepted form` under
/// the veto; the shipped split shreds it across its extra column. (An
/// all-`th` row cannot pin the header collateral: the markdown head rule
/// reads cell text, which the split does not change; the decode's own
/// header flags need figures the veto's ragged cases lack, and a spanning
/// label reads as a caption the peel takes in every arm. The grid ships G1
/// alone; its header proof is this edge row kept whole in the emitted
/// markdown, plus the eight TUNING header_rows restorations.)
fn layout_header_over_wrapped() -> Vec<Piece> {
    layout_wrapped_prose()
}

#[test]
fn the_guards_keep_the_header_row_the_split_shreds() {
    let pieces = layout_header_over_wrapped();
    let cells = cells_of(&blocks_of(&pieces, true));
    let head: Vec<_> = cells.iter().filter(|c| c.0 == 0).collect();
    assert!(
        head.iter().any(|c| c.1 == 0 && c.3 == "Item A 10.3"),
        "the guards did not keep the first row whole: {cells:?}"
    );
    let markdown = to_markdown(&blocks_of(&pieces, true));
    assert!(
        markdown.contains("Item A 10.3</td>")
            || markdown.contains("Item A 10.3</th>")
            || markdown.contains("| Item A 10.3 |"),
        "the first row is not whole in the markdown: {markdown}"
    );
    let off = cells_of(&blocks_of(&pieces, false));
    assert!(
        off.iter()
            .any(|c| c.0 == 0 && c.1 == 0 && c.3 == "Item A 10.3"),
        "the switch-off reading lost the first row without a corridor: {off:?}"
    );
}
