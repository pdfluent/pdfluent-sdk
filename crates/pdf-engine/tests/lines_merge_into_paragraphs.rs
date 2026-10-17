//! Lines become paragraphs, and stop at a column, a rule and a change of type
//! (#567).
//!
//! Extraction handed back one block per LINE. A line is not a unit anybody
//! chunks, cites or embeds: the breaks in it are the measure the typesetter
//! chose and nothing else, and a retrieval system that indexes them indexes
//! half-sentences. So the lines are merged into paragraphs.
//!
//! Every case here is a page built from operators, so that what is tested is
//! the whole path -- interpreter, spans, cut, merge -- and not a table of
//! numbers handed to the merge directly. Take the merge out and the first test
//! goes red with three blocks where it asks for one; weaken it until it crosses
//! a gutter, a ruling line or a footnote rule, and one of the others does.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;

/// One line of text, at a size, at a place.
struct Line {
    text: &'static str,
    x: f64,
    y: f64,
    size: f64,
}

fn helvetica(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })))
}

fn show(operations: &mut Vec<Operation>, line: &Line) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), line.size.into()],
    ));
    operations.push(Operation::new("Td", vec![line.x.into(), line.y.into()]));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(line.text)],
    ));
    operations.push(Operation::new("ET", vec![]));
}

/// A hairline the width of a table, at a height.
fn rule(operations: &mut Vec<Operation>, left: f64, right: f64, y: f64) {
    operations.push(Operation::new(
        "re",
        vec![left.into(), y.into(), (right - left).into(), 0.18.into()],
    ));
    operations.push(Operation::new("f", vec![]));
}

/// A one-page PDF of whatever the caller drew.
fn page_of(operations: Vec<Operation>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = helvetica(&mut doc);
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
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

fn lines_page(lines: &[Line]) -> Vec<u8> {
    let mut operations = Vec::new();
    for line in lines {
        show(&mut operations, line);
    }
    page_of(operations)
}

fn blocks_of(bytes: Vec<u8>) -> Vec<pdf_engine::Block> {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

fn body(text: &'static str, y: f64) -> Line {
    Line {
        text,
        x: MARGIN,
        y,
        size: BODY_SIZE,
    }
}

/// A paragraph of prose, one line per element, set solid from `TOP`.
const PARAGRAPH: [&str; 3] = [
    "Compiler design is the study of how a program written in one",
    "language becomes a program written in another, and of what may",
    "safely be assumed about the two along the way.",
];

#[test]
fn the_lines_of_a_paragraph_come_back_as_one_block() {
    let lines: Vec<Line> = PARAGRAPH
        .iter()
        .enumerate()
        .map(|(i, text)| body(text, TOP - i as f64 * LEADING))
        .collect();
    let blocks = blocks_of(lines_page(&lines));

    assert_eq!(
        blocks.len(),
        1,
        "three lines of one paragraph came back as {} blocks: {:?}",
        blocks.len(),
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].text, PARAGRAPH.join(" "));
    assert_eq!(blocks[0].kind, BlockKind::Paragraph);
    assert_eq!(blocks[0].page, 0);
    assert_eq!(blocks[0].index, 0);
    assert_eq!(
        blocks[0].spans.iter().map(|s| s.text.len()).sum::<usize>(),
        PARAGRAPH.iter().map(|l| l.len()).sum::<usize>(),
        "the block dropped or invented spans"
    );
    // The box holds the type it was made of, and it is a box: y0 below y1.
    assert!(blocks[0].bbox[0] >= MARGIN - 1.0 && blocks[0].bbox[0] <= MARGIN + 1.0);
    assert!(blocks[0].bbox[1] < blocks[0].bbox[3]);
    assert!(blocks[0].bbox[3] >= TOP);
}

#[test]
fn a_blank_line_between_two_paragraphs_keeps_them_apart() {
    let mut lines: Vec<Line> = PARAGRAPH
        .iter()
        .enumerate()
        .map(|(i, text)| body(text, TOP - i as f64 * LEADING))
        .collect();
    let second_top = TOP - 3.0 * LEADING - LEADING;
    lines.push(body(
        "The second paragraph opens after a line of",
        second_top,
    ));
    lines.push(body(
        "space, and runs to a second line of its own.",
        second_top - LEADING,
    ));

    let blocks = blocks_of(lines_page(&lines));
    assert_eq!(
        blocks.len(),
        2,
        "{:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].text, PARAGRAPH.join(" "));
    assert!(blocks[1].text.starts_with("The second paragraph"));
    assert_eq!(blocks[0].index, 0);
    assert_eq!(blocks[1].index, 1);
}

#[test]
fn a_paragraph_never_crosses_the_gutter_between_two_columns() {
    // A full-width title over two columns, each column a paragraph of its own.
    const GUTTER_RIGHT: f64 = 330.0;
    let mut lines = vec![Line {
        text: "On the reading of pages set in more than one column",
        x: MARGIN,
        y: TOP + 2.0 * LEADING,
        size: 14.0,
    }];
    for (i, text) in [
        "The left column is read to its",
        "own foot before the right one",
        "is begun at the head of the page.",
    ]
    .iter()
    .enumerate()
    {
        lines.push(Line {
            text,
            x: MARGIN,
            y: TOP - i as f64 * LEADING,
            size: BODY_SIZE,
        });
    }
    for (i, text) in [
        "The right column holds the rest",
        "of the sentence the left one",
        "began, and is read after it.",
    ]
    .iter()
    .enumerate()
    {
        lines.push(Line {
            text,
            x: GUTTER_RIGHT,
            y: TOP - i as f64 * LEADING,
            size: BODY_SIZE,
        });
    }

    let blocks = blocks_of(lines_page(&lines));
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(blocks.len(), 3, "{texts:?}");
    assert!(texts[0].starts_with("On the reading"), "{texts:?}");
    assert_eq!(
        texts[1],
        "The left column is read to its own foot before the right one is begun at the head of the page.",
    );
    assert_eq!(
        texts[2],
        "The right column holds the rest of the sentence the left one began, and is read after it.",
    );
    for block in &blocks {
        assert!(
            !(block.text.contains("left column") && block.text.contains("right column")),
            "a block crossed the gutter: {:?}",
            block.text
        );
    }
}

#[test]
fn the_rows_of_a_ruled_table_are_not_a_paragraph() {
    const PITCH: f64 = 18.0;
    const ROWS: [(&str, &str); 4] = [
        ("id-6-1-2-t01", "Document does not start with a percent"),
        ("id-6-1-3-t01", "The trailer dictionary has no ID entry"),
        ("id-6-1-3-t02", "The trailer dictionary has an Encrypt"),
        ("id-6-1-7-t01", "The stream token has no single newline"),
    ];

    let mut operations = Vec::new();
    for (row, (name, message)) in ROWS.iter().enumerate() {
        let y = TOP - row as f64 * PITCH;
        rule(&mut operations, 55.0, 540.0, y + 11.0);
        show(
            &mut operations,
            &Line {
                text: name,
                x: 60.0,
                y,
                size: BODY_SIZE,
            },
        );
        show(
            &mut operations,
            &Line {
                text: message,
                x: 200.0,
                y,
                size: BODY_SIZE,
            },
        );
    }

    // The rows are a table, so each cell is its own block and the two columns
    // never join (#570). What this case still holds is #545's own claim: the
    // four rows are four rows, and no two of them are one block.
    let blocks = blocks_of(page_of(operations));
    for (name, message) in ROWS {
        for text in [name, message] {
            let holding: Vec<&String> = blocks
                .iter()
                .filter(|b| b.text.contains(text))
                .map(|b| &b.text)
                .collect();
            assert_eq!(
                holding.len(),
                1,
                "{text} is in {} blocks: {:?}",
                holding.len(),
                blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
            );
        }
        let together = blocks
            .iter()
            .any(|b| b.text.contains(name) && b.text.contains(message));
        assert!(!together, "{name} and its message are one cell");
    }
    assert!(
        blocks.len() >= 2 * ROWS.len(),
        "the ruled rows were merged into {} block(s): {:?}",
        blocks.len(),
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
}

#[test]
fn justified_lines_with_wide_word_gaps_are_still_one_paragraph() {
    // Justified setting on a narrow measure stretches the word spaces of a bad
    // line well past an em. Nothing lines up under them, so they are word
    // spaces and not a gutter, and the paragraph survives.
    let mut operations = Vec::new();
    let wide: [(&str, f64); 3] = [
        ("Compilers", MARGIN),
        ("translate", 190.0),
        ("programs", 330.0),
    ];
    for (word, x) in wide {
        show(
            &mut operations,
            &Line {
                text: word,
                x,
                y: TOP,
                size: BODY_SIZE,
            },
        );
    }
    show(
        &mut operations,
        &body("from one language into another, and", TOP - LEADING),
    );
    show(
        &mut operations,
        &body("say nothing about why anyone would.", TOP - 2.0 * LEADING),
    );

    let blocks = blocks_of(page_of(operations));
    assert_eq!(
        blocks.len(),
        1,
        "a justified line was read as a table: {:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert!(blocks[0].text.starts_with("Compilers translate programs"));
    assert!(blocks[0].text.ends_with("why anyone would."));
}

#[test]
fn a_footnote_under_its_rule_is_not_the_end_of_the_body() {
    let mut operations = Vec::new();
    show(
        &mut operations,
        &body("The body of the page ends on this", 220.0),
    );
    show(
        &mut operations,
        &body("line, above the footnote rule.", 208.0),
    );
    rule(&mut operations, MARGIN, 250.0, 196.0);
    show(
        &mut operations,
        &Line {
            text: "1 The note itself is set two points smaller.",
            x: MARGIN,
            y: 186.0,
            size: 8.0,
        },
    );

    let blocks = blocks_of(page_of(operations));
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(blocks.len(), 2, "{texts:?}");
    assert_eq!(
        texts[0],
        "The body of the page ends on this line, above the footnote rule."
    );
    assert!(texts[1].starts_with("1 The note itself"));
}

#[test]
fn a_page_with_no_text_produces_no_blocks() {
    let mut operations = Vec::new();
    rule(&mut operations, MARGIN, 400.0, 400.0);
    assert!(blocks_of(page_of(operations)).is_empty());
}
