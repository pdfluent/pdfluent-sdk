//! A sentence period next to leader dots keeps its place and its space (#636).
//!
//! The by-line cell order moves every dot-only word after its line, which
//! treats a sentence-final period as leader fill: in corpus/f1040.pdf the
//! 35a cell reads `refunded to youIf Form 8888 ...`, the period moved to the
//! line end and the word space gone with it. A period that ends a word or a
//! sentence -- a dot glyph abutting the word before it, or followed by a
//! capitalised word after a normal word gap -- is no leader dot and keeps its
//! place; leader dots are runs of dots separated by leader spacing.
//!
//! Through `PdfDocument::extract_blocks`: the first test on an
//! operator-built ruled table shaped like the 35a row -- one word per show
//! op, the period followed by a capitalised word after a word gap, leader
//! dots at leader spacing, and the `35a` marker -- and the second on the
//! real form, whose abutting period the operator build cannot reproduce (a
//! same-face period with no gap glues to its word in our own extraction,
//! while the form's survives as the head of its `. If Form ...` span).
//! Without the sentence-period rule both cells trail the period to the line
//! end, so both tests go red; the trailing leader run still trails its
//! label, which the third test pins.
//!
//! The third test pins the existing leader behaviour, not the fix: it reads
//! the same with and without the sentence-period rule, and goes red only
//! with the leader reorder itself removed (`Total 12. .` falls back to the
//! geometric `Total . . 12`).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

const BODY: f64 = 8.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 680.0;
const PITCH: f64 = 18.0;

/// One row as its own show op at an explicit origin.
struct Piece {
    text: Vec<u8>,
    x: f64,
    y: f64,
}

fn piece(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.as_bytes().to_vec(),
        x,
        y,
    }
}

fn form_like_pdf() -> Vec<u8> {
    // One word per show op. The sentence period opens a letter span, so its
    // word box is exact; a same-face period with no gap at all would glue to
    // its word in our own extraction, while the form's abutting period
    // survives as the head of its `. If Form ...` span -- the corpus test
    // below pins that abutting shape, this row the spaced one. Either way
    // the period is followed by a capitalised word after a normal word gap,
    // so it is no leader dot. The marker stands ahead of the fill, so the
    // leader dots fold into the marker's span the way they do on the form,
    // and keep the sentence span's boxes exact.
    let mut pieces = vec![piece("Item", MARGIN, TOP)];
    // The 35a shape: `Owed to you . If late, pay 35a . .`.
    let y = TOP - PITCH;
    let mut x = MARGIN;
    for word in ["Owed", "to", "you"] {
        pieces.push(piece(word, x, y));
        x += word.chars().count() as f64 * 0.6 * BODY + 2.5;
    }
    // A narrow gap ahead of the period keeps it its own word; the abutting
    // shape lives in the corpus test below.
    x += 1.5 - 2.5;
    pieces.push(piece(". If late, pay", x, y));
    x += ". If late, pay".chars().count() as f64 * 0.6 * BODY + 8.0;
    pieces.push(piece("35a", x, y));
    x += "35a".chars().count() as f64 * 0.6 * BODY;
    for _ in 0..2 {
        x += 10.0;
        pieces.push(piece(".", x, y));
    }
    let marker_x = x;
    pieces.push(piece("Line", marker_x, TOP));
    // The leader control: `Total . . 12` geometrically, with the dots
    // folding into the label's span; the reorder still trails them past the
    // marker.
    let y = TOP - 2.0 * PITCH;
    let mut x = MARGIN;
    pieces.push(piece("Total", x, y));
    x += "Total".chars().count() as f64 * 0.6 * BODY;
    for _ in 0..2 {
        x += 10.0;
        pieces.push(piece(".", x, y));
    }
    x += 0.6 * BODY + 8.0;
    pieces.push(piece("12", x, y));
    x += "12".chars().count() as f64 * 0.6 * BODY;
    // Plain rows, so the grid holds its rows past the dotted ones. Their
    // markers line up under the dotted rows' markers.
    let plain_marker = marker_x.max(x);
    for (index, label) in ["Other", "Extra", "Spare", "Bonus", "Final"]
        .iter()
        .enumerate()
    {
        let y = TOP - (3.0 + index as f64) * PITCH;
        pieces.push(piece(label, MARGIN, y));
        pieces.push(piece(&format!("{}", 13 + index), plain_marker, y));
    }

    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Courier");
    let mut operations = Vec::new();
    for index in 0..9 {
        let y = TOP - index as f64 * PITCH + 11.0;
        operations.push(Operation::new(
            "re",
            vec![
                (MARGIN - 6.0).into(),
                y.into(),
                (520.0 - MARGIN + 6.0).into(),
                0.4.into(),
            ],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for piece in &pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), BODY.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(piece.text.clone(), StringFormat::Literal)],
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
                "F1" => regular.clone(),
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
    doc.save_to(&mut buf).expect("save the form-like page");
    buf
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn cell_texts() -> Vec<String> {
    let blocks = PdfDocument::open(form_like_pdf())
        .expect("open the form-like page")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options");
    blocks
        .into_iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell { .. } => Some(b.text),
            _ => None,
        })
        .collect()
}

/// A period followed by a capitalised word after a word gap stays where the
/// page put it: `you . If`, never trailed to the line end.
#[test]
fn a_sentence_period_before_a_leader_run_keeps_its_place() {
    let cells = cell_texts();
    let row = cells
        .iter()
        .find(|text| text.contains("Owed"))
        .expect("the 35a-like row reads");
    assert!(
        row.contains("you . If"),
        "the period stays ahead of `If`: {row:?} ({cells:?})"
    );
    assert!(
        !row.contains("youIf"),
        "the word space never drops: {row:?} ({cells:?})"
    );
}

/// The form's abutting period stays with its word: `you. If`, never `youIf`.
/// The 35a cell at its measured bbox, through the public path on the real
/// form.
#[test]
fn the_f1040_35a_cell_keeps_its_sentence_period() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/f1040.pdf"
    ))
    .expect("read the f1040 fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open the f1040 fixture")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options");
    let cell = blocks
        .iter()
        .find(|b| {
            (b.bbox[0] - 115.2).abs() < 2.0
                && (b.bbox[1] - 290.3).abs() < 2.0
                && (b.bbox[3] - 298.3).abs() < 2.0
        })
        .expect("the 35a cell reads");
    assert!(
        cell.text.contains("you. If"),
        "the period stays with `you`: {:?}",
        cell.text
    );
    assert!(
        !cell.text.contains("youIf"),
        "the word space never drops: {:?}",
        cell.text
    );
}

/// The trailing leader run still trails its label: `Total 12. .` (the
/// first dot joins the marker, the way `35a.` does on the form).
#[test]
fn a_trailing_leader_run_still_trails_its_label() {
    let cells = cell_texts();
    let row = cells
        .iter()
        .find(|text| text.contains("Total"))
        .expect("the leader row reads");
    assert!(
        row.contains("Total 12. ."),
        "the leader dots trail the label and its marker: {row:?} ({cells:?})"
    );
}
