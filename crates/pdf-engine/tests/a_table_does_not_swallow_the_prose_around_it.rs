// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The caption above a table and the sentence below it are not cells (#600).
//!
//! WHY THIS EXISTS. T5 measured that 9.3 per cent of the words inside our
//! PubTables table blocks are not in the annotated table at all -- ordinary
//! prose, "the", "to", "were" -- because the block swallowed the caption over
//! the table or the sentence under it. A table that eats a sentence is worse
//! than a table we never found: the sentence is gone from the text as well, so
//! one defect costs two axes.
//!
//! THE 9.3 IS ABOUT FOUR TIMES THE REAL RATE, which T5 established on the same
//! evidence when I asked which side the damage fell on. Of the 1,255 invented
//! words, 58.2 per cent sit in the MIDDLE rows and are mostly real table
//! content PubTables does not annotate -- a second table on the page, or a
//! part outside the annotated one (`36.56 +/- 2.12`, a crystal-structure page
//! with several tables). That is the instrument counting unlabelled truth as
//! invention, not a defect of ours. The caption above and the sentence below
//! are 12.0 and 13.1 per cent, about a quarter together and about EQUAL -- so
//! growing the region on one side only buys nothing, which was my first idea
//! when I read the 9.3.
//!
//! The shape is still worth holding open; the number it came from is not worth
//! quoting.
//!
//! WHAT THIS IS NOT. It is not an argument against the region model growing
//! its region. `pairs::GROWTH` reaches two line heights past the cells the
//! heuristics claimed, and that measured BETTER than a tight region -- TEDS
//! 0.5975 against 0.5446 -- because a table's units row and its continuation
//! line live there. Growth decides what the model is ASKED about; this decides
//! what may end up in a cell. A region may reach over the caption and still
//! owe an answer that leaves it out.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::tables::{refine_gated, Weakness};
use pdf_engine::learned::{document_facts, page_blocks, Switches, SWITCHES};
use pdf_engine::{Block, BlockKind, PdfDocument};

const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn at(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        x,
        y,
    }
}

fn stream_of(pieces: &[Piece]) -> Vec<u8> {
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(piece.text.clone())],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    Content { operations }.encode().expect("content")
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        stream_of(pieces),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => vec![Object::Reference(page)],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// A caption, a three-column table of figures, and a sentence under it.
fn page() -> Vec<Piece> {
    let mut pieces = vec![at(
        "Table 2: Reported figures by region for the year under review.",
        MARGIN,
        TOP,
    )];
    let rows = [
        ("Region", "Amount", "Share"),
        ("North", "120", "41%"),
        ("South", "95", "32%"),
        ("East", "78", "27%"),
    ];
    for (n, (a, b, c)) in rows.iter().enumerate() {
        let y = TOP - (n as f64 + 1.5) * PITCH;
        pieces.push(at(a, MARGIN, y));
        pieces.push(at(b, 240.0, y));
        pieces.push(at(c, 360.0, y));
    }
    pieces.push(at(
        "These results were consistent with the figures reported to the board.",
        MARGIN,
        TOP - 7.0 * PITCH,
    ));
    pieces
}

fn cell_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .collect::<Vec<&str>>()
        .join(" ")
}

#[test]
fn the_caption_above_and_the_sentence_below_stay_out_of_the_cells() {
    let doc = PdfDocument::open(pdf_of(&page())).expect("open");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((612.0, 792.0));
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let blocks = page_blocks(&all, 0);

    let both_on = Switches {
        table_region: true,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    };
    // The hybrid gate opened, because this is about what the pass does with a
    // table once it is allowed to, not about when it is allowed to. That is
    // `the_hybrid_gate_on_a_weak_grid`.
    let weak = |_r: &[usize], _p: &[&Block]| {
        Some(Weakness {
            thin_rows: 2,
            fill: 0.31,
            retried_from: 1,
            full_rows: 0,
            columns: 5,
        })
    };
    let (out, origin) = refine_gated(blocks.clone(), size, &facts, both_on, &weak);
    let cells = cell_text(&out);

    // NOT VACUOUS: the models must actually be doing something here, or this
    // page is testing the heuristics and would stay green however the models
    // behaved.
    //
    // ASKED OF THE ORIGIN MAP AND NOT OF THE OUTPUT, because those are
    // different questions. This compared `kind|text` before and after, which
    // says the pass CHANGED something -- and on this page the only thing it
    // changed was a header the rebuild invented from the row number. With
    // that defect fixed the model rebuilds the grid the heuristics built,
    // agrees with it exactly, and the old guard read that agreement as the
    // models never having run (codex, #600).
    //
    // `origin` says which input block each output came from, and a rebuilt
    // run maps all of its cells back to one index. So a repeat in it is the
    // pass acting, whatever it decided.
    let acted = origin.windows(2).any(|w| w[0] == w[1]);
    assert!(
        acted,
        "the table pass rebuilt nothing on this page, so it tests the \
         heuristics and not the models: {origin:?}"
    );

    assert!(
        cells.contains("North") && cells.contains("120"),
        "the table itself was not found, so this tests nothing: {cells:?}"
    );

    // THE WORDS T5 COUNTED. Prose words that belong to the caption and the
    // sentence, and which are 9.3 per cent of what our table blocks hold on
    // PubTables. Each is checked on its own so the failure names which side
    // leaked rather than saying the string differs.
    for word in ["Table", "consistent", "board", "review"] {
        assert!(
            !cells
                .split_whitespace()
                .any(|w| w.trim_matches(|c: char| !c.is_alphanumeric()) == word),
            "{word:?} is prose from around the table and ended up in a cell: {cells:?}"
        );
    }
}
