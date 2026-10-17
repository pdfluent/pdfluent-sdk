//! Headings are the type a document sets above its own body (#568).
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, paragraph merge, classifier. Take the
//! classifier out and every one of these goes red with `Paragraph` where it
//! asks for a `Heading`; put a fixed point size back in and the first two
//! disagree with each other, because one sets its body in ten point and the
//! other sets its headings in ten point.
//!
//! The four documents the ticket names as the ones that break a naive
//! classifier each have a case: one size with bold headings, headings in
//! capitals with neither size nor weight, numbered headings, and a document
//! whose title is the only large text on it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;

/// One line of text: what it says, where it is, how big, and whether the face
/// it is set in is the bold one.
struct Line {
    text: &'static str,
    x: f64,
    y: f64,
    size: f64,
    bold: bool,
}

fn body(text: &'static str, y: f64) -> Line {
    Line {
        text,
        x: MARGIN,
        y,
        size: BODY_SIZE,
        bold: false,
    }
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
    })))
}

fn show(operations: &mut Vec<Operation>, line: &Line) {
    let face: &[u8] = if line.bold { b"F2" } else { b"F1" };
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(face.to_vec()), line.size.into()],
    ));
    operations.push(Operation::new("Td", vec![line.x.into(), line.y.into()]));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(line.text)],
    ));
    operations.push(Operation::new("ET", vec![]));
}

/// A PDF of one page per list of lines: regular text in Helvetica, bold text in
/// Helvetica-Bold, which is where `TextSpan::is_bold` gets its answer from.
fn pdf_of(pages: &[Vec<Line>]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let bold = font(&mut doc, b"Helvetica-Bold");

    let mut page_ids = Vec::new();
    for lines in pages {
        let mut operations = Vec::new();
        for line in lines {
            show(&mut operations, line);
        }
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => regular.clone(),
                    "F2" => bold.clone(),
                }),
            }),
        })));
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    let count = kids.len() as i64;
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count),
    }));
    for page_id in page_ids {
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
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

/// THE HEURISTIC AND NOT THE MODEL. Every page here is built from operators to
/// test what `headings::classify` does with it, and the learned pass of #599
/// is a stage after it with a switch of its own: run together, a red test here
/// would not say which of the two had changed. The model's own behaviour on
/// these pages is asserted in `learned_heading_switch.rs`, including the two
/// places where the two disagree.
fn blocks_of(pages: &[Vec<Line>]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pages))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

/// The block whose text starts with `prefix`, or a failure naming every block
/// there was -- a classifier that moved a boundary should say so, not panic on
/// an index.
fn block_starting<'a>(blocks: &'a [Block], prefix: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.text.starts_with(prefix))
        .unwrap_or_else(|| panic!("no block opens {prefix:?}: {:?}", shape(blocks)))
}

/// Two paragraphs of body text under a heading, set solid from `y`.
fn prose(y: f64) -> Vec<Line> {
    [
        "Compiler design is the study of how a program written in one",
        "language becomes a program written in another, and of what may",
        "safely be assumed about the two along the way. The subject is",
        "older than the machines that made it necessary, and the proofs",
        "it rests on are older still than that.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| body(text, y - i as f64 * LEADING))
    .collect()
}

#[test]
fn the_cluster_above_the_body_is_a_heading_and_the_body_is_not() {
    let mut lines = vec![
        Line {
            text: "The Structure of a Compiler",
            x: MARGIN,
            y: TOP,
            size: 20.0,
            bold: true,
        },
        Line {
            text: "Analysis and synthesis",
            x: MARGIN,
            y: TOP - 40.0,
            size: 13.0,
            bold: true,
        },
    ];
    lines.extend(prose(TOP - 62.0));
    lines.push(Line {
        text: "Lexical analysis",
        x: MARGIN,
        y: TOP - 150.0,
        size: 13.0,
        bold: true,
    });
    lines.extend(prose(TOP - 172.0));

    let blocks = blocks_of(&[lines]);
    assert_eq!(
        block_starting(&blocks, "The Structure").kind,
        BlockKind::Heading { level: 1 },
        "the largest type in the document is not level 1: {:?}",
        shape(&blocks)
    );
    for subhead in ["Analysis and synthesis", "Lexical analysis"] {
        assert_eq!(
            block_starting(&blocks, subhead).kind,
            BlockKind::Heading { level: 2 },
            "{subhead:?} is not level 2: {:?}",
            shape(&blocks)
        );
    }
    for block in &blocks {
        if block.text.starts_with("Compiler design") {
            assert_eq!(
                block.kind,
                BlockKind::Paragraph,
                "the body of the document was read as a heading"
            );
        }
    }
}

#[test]
fn a_document_set_in_one_size_has_headings_by_weight_alone() {
    // Every line is ten point. The only thing the document says about its
    // headings is the face they are set in -- which is what a classifier with a
    // point size in it cannot see.
    let mut lines = vec![Line {
        text: "Scope of this agreement",
        x: MARGIN,
        y: TOP,
        size: BODY_SIZE,
        bold: true,
    }];
    lines.extend(prose(TOP - 2.0 * LEADING));
    lines.push(Line {
        text: "Definitions",
        x: MARGIN,
        y: TOP - 8.0 * LEADING,
        size: BODY_SIZE,
        bold: true,
    });
    lines.extend(prose(TOP - 10.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    for heading in ["Scope of this agreement", "Definitions"] {
        assert_eq!(
            block_starting(&blocks, heading).kind,
            BlockKind::Heading { level: 1 },
            "{heading:?} is bold in a document that is not: {:?}",
            shape(&blocks)
        );
    }
    assert_eq!(
        block_starting(&blocks, "Compiler design").kind,
        BlockKind::Paragraph,
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn capitals_are_the_only_signal_a_single_face_document_has() {
    // One size, one weight, and the headings shout. A form does this, and a
    // classifier that reads only size and weight finds nothing on it.
    let mut lines = vec![body("SECTION A: THE APPLICANT", TOP)];
    lines.extend(prose(TOP - 2.0 * LEADING));
    lines.push(body("SECTION B: THE PROPERTY", TOP - 8.0 * LEADING));
    lines.extend(prose(TOP - 10.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    for heading in ["SECTION A", "SECTION B"] {
        assert_eq!(
            block_starting(&blocks, heading).kind,
            BlockKind::Heading { level: 1 },
            "{heading:?} is the only thing set apart on the page: {:?}",
            shape(&blocks)
        );
    }
    assert_eq!(
        block_starting(&blocks, "Compiler design").kind,
        BlockKind::Paragraph,
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn a_numbered_heading_keeps_its_number_and_its_level() {
    let mut lines = vec![Line {
        text: "2 Scope",
        x: MARGIN,
        y: TOP,
        size: 14.0,
        bold: true,
    }];
    lines.extend(prose(TOP - 2.0 * LEADING));
    lines.push(Line {
        text: "2.1 Materials",
        x: MARGIN,
        y: TOP - 8.0 * LEADING,
        size: 11.5,
        bold: true,
    });
    lines.extend(prose(TOP - 10.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    let outer = block_starting(&blocks, "2 Scope");
    let inner = block_starting(&blocks, "2.1 Materials");
    assert_eq!(
        outer.kind,
        BlockKind::Heading { level: 1 },
        "{:?}",
        shape(&blocks)
    );
    assert_eq!(
        inner.kind,
        BlockKind::Heading { level: 2 },
        "{:?}",
        shape(&blocks)
    );
    assert_eq!(outer.text, "2 Scope", "the number is part of the heading");
    assert_eq!(inner.text, "2.1 Materials");
}

#[test]
fn a_title_that_is_the_only_large_text_is_the_only_heading() {
    let mut lines = vec![Line {
        text: "On the reading of documents",
        x: MARGIN,
        y: TOP,
        size: 18.0,
        bold: false,
    }];
    lines.extend(prose(TOP - 40.0));
    let second = prose(TOP - 130.0);
    lines.extend(second);

    let blocks = blocks_of(&[lines]);
    assert_eq!(
        block_starting(&blocks, "On the reading").kind,
        BlockKind::Heading { level: 1 },
        "{:?}",
        shape(&blocks)
    );
    assert_eq!(
        blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .count(),
        1,
        "a document with one heading grew a second: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_pull_quote_set_large_is_not_a_heading() {
    // Five lines of prose in fourteen point, over a body set in ten. The type
    // stands above the body and the block is not a heading, because a heading
    // is a line and this is a page of text.
    let mut lines: Vec<Line> = [
        "The subject is older than the machines that made",
        "it necessary, and the proofs it rests on are older",
        "still than that, which is the reason a compiler",
        "writer reads mathematics before reading anyone's",
        "manual of an instruction set.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| Line {
        text,
        x: MARGIN,
        y: TOP - i as f64 * 17.0,
        size: 14.0,
        bold: false,
    })
    .collect();
    lines.extend(prose(TOP - 120.0));
    lines.extend(prose(TOP - 220.0));

    let blocks = blocks_of(&[lines]);
    assert_eq!(
        block_starting(&blocks, "The subject is older").kind,
        BlockKind::Paragraph,
        "a five-line block became a heading: {:?}",
        shape(&blocks)
    );
}

#[test]
fn position_separates_two_clusters_the_type_cannot() {
    // Both headings are set in the same size and the same weight. The outer one
    // starts at the margin; the inner one is indented into the text column. The
    // type says they are the same thing and the page says which is which.
    let mut lines = vec![Line {
        text: "Chapter three",
        x: MARGIN,
        y: TOP,
        size: 12.0,
        bold: true,
    }];
    lines.extend(prose(TOP - 2.0 * LEADING));
    lines.push(Line {
        text: "A note on notation",
        x: MARGIN + 40.0,
        y: TOP - 8.0 * LEADING,
        size: 12.0,
        bold: true,
    });
    lines.extend(prose(TOP - 10.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    assert_eq!(
        block_starting(&blocks, "Chapter three").kind,
        BlockKind::Heading { level: 1 },
        "{:?}",
        shape(&blocks)
    );
    assert_eq!(
        block_starting(&blocks, "A note on notation").kind,
        BlockKind::Heading { level: 2 },
        "the indented heading of the pair is not the inner one: {:?}",
        shape(&blocks)
    );
}

#[test]
fn the_statistics_are_the_whole_documents_and_not_one_pages() {
    // Page two is set entirely in the size page one uses for its title. Read on
    // its own it has no body and no heading; read as part of the document it is
    // body text under the heading page one opened with.
    let first = {
        let mut lines = vec![Line {
            text: "A report in two pages",
            x: MARGIN,
            y: TOP,
            size: 16.0,
            bold: true,
        }];
        lines.extend(prose(TOP - 40.0));
        lines
    };
    let second = prose(TOP);

    let blocks = blocks_of(&[first, second]);
    assert_eq!(
        block_starting(&blocks, "A report in two pages").kind,
        BlockKind::Heading { level: 1 },
        "{:?}",
        shape(&blocks)
    );
    assert_eq!(
        blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .count(),
        1,
        "page two grew a heading of its own: {:?}",
        shape(&blocks)
    );
}

#[test]
fn the_markdown_of_a_document_carries_its_heading_levels() {
    let mut lines = vec![
        Line {
            text: "The Structure of a Compiler",
            x: MARGIN,
            y: TOP,
            size: 20.0,
            bold: true,
        },
        Line {
            text: "Analysis and synthesis",
            x: MARGIN,
            y: TOP - 40.0,
            size: 13.0,
            bold: true,
        },
    ];
    lines.extend(prose(TOP - 62.0));
    lines.push(Line {
        text: "A note on notation",
        x: MARGIN,
        y: TOP - 150.0,
        size: 11.0,
        bold: true,
    });
    lines.extend(prose(TOP - 172.0));

    let blocks = blocks_of(&[lines]);
    let markdown = to_markdown(&blocks);
    assert!(
        markdown.contains("# The Structure of a Compiler\n"),
        "no level-1 heading in the markdown: {markdown}"
    );
    assert!(
        markdown.contains("## Analysis and synthesis\n"),
        "no level-2 heading in the markdown: {markdown}"
    );
    assert!(
        markdown.contains("### A note on notation\n"),
        "no level-3 heading in the markdown: {markdown}"
    );
    assert!(
        markdown.contains("\nCompiler design is the study"),
        "the body did not survive as a paragraph: {markdown}"
    );
    assert_eq!(
        markdown,
        to_markdown(&blocks),
        "the serializer is not deterministic"
    );
}

#[test]
fn a_bold_heading_set_solid_with_its_paragraph_is_still_its_own_block() {
    // No extra white, no change of size, no indent: the document says "heading"
    // with its face and with nothing else. The paragraph merge has to refuse the
    // pair on weight, or there is no block here for the classifier to read.
    let mut lines = vec![Line {
        text: "Scope of this agreement",
        x: MARGIN,
        y: TOP,
        size: BODY_SIZE,
        bold: true,
    }];
    lines.extend(prose(TOP - LEADING));
    lines.push(Line {
        text: "Definitions",
        x: MARGIN,
        y: TOP - 7.0 * LEADING,
        size: BODY_SIZE,
        bold: true,
    });
    lines.extend(prose(TOP - 8.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    for heading in ["Scope of this agreement", "Definitions"] {
        let block = block_starting(&blocks, heading);
        assert_eq!(
            block.text,
            heading,
            "the heading kept the paragraph under it: {:?}",
            shape(&blocks)
        );
        assert_eq!(
            block.kind,
            BlockKind::Heading { level: 1 },
            "{heading:?}: {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn one_heading_style_in_two_columns_is_one_level() {
    // The same subhead over each column of a two-column page. Measured against
    // a single left edge for the whole page, the right-hand one reads as
    // indented by the width of a column and becomes a level of its own.
    const RIGHT: f64 = 330.0;
    let column = |x: f64, top: f64| -> Vec<Line> {
        [
            "Compiler design is the study of",
            "how a program written in one",
            "language becomes a program",
            "written in another, and of what",
            "may safely be assumed of both.",
        ]
        .iter()
        .enumerate()
        .map(|(i, text)| Line {
            text,
            x,
            y: top - i as f64 * LEADING,
            size: BODY_SIZE,
            bold: false,
        })
        .collect()
    };

    let mut lines = vec![
        Line {
            text: "Analysis",
            x: MARGIN,
            y: TOP,
            size: 12.0,
            bold: true,
        },
        Line {
            text: "Synthesis",
            x: RIGHT,
            y: TOP,
            size: 12.0,
            bold: true,
        },
    ];
    lines.extend(column(MARGIN, TOP - 2.0 * LEADING));
    lines.extend(column(RIGHT, TOP - 2.0 * LEADING));

    let blocks = blocks_of(&[lines]);
    for heading in ["Analysis", "Synthesis"] {
        assert_eq!(
            block_starting(&blocks, heading).kind,
            BlockKind::Heading { level: 1 },
            "{heading:?} is set exactly like the other one: {:?}",
            shape(&blocks)
        );
    }
}

/// A title that wraps is one heading, and the page says so in the type (#581).
///
/// The shape is the one the benchmark's tail is full of: a first line that
/// runs to the margin and a second set further in, so the paragraph merge's
/// column run breaks between them and two half-headings come out. Take
/// `put_split_headings_together` out of `headings::classify` and this goes red
/// with `NOTES TO AND FORMING PART OF THE FINANCIAL STATEMENTS` and `FOR THE
/// YEAR ENDED 30 JUNE` as two headings.
#[test]
fn a_heading_that_wraps_is_one_heading() {
    let mut page = vec![
        Line {
            text: "NOTES TO AND FORMING PART OF THE FINANCIAL STATEMENTS",
            x: MARGIN + 30.0,
            y: TOP,
            size: BODY_SIZE + 2.0,
            bold: true,
        },
        Line {
            text: "FOR THE YEAR ENDED 30 JUNE",
            x: MARGIN + 300.0,
            y: TOP - LEADING * 1.3,
            size: BODY_SIZE + 2.0,
            bold: true,
        },
    ];
    let mut y = TOP - LEADING * 3.3;
    for text in [
        "The directors of the company during the financial year were as follows,",
        "and each of them held office for the whole of the year under review,",
        "except where the notes to these statements say otherwise in a note.",
        "The company met four times in the year and the meetings are recorded.",
        "No director had an interest in a contract with the company in the year.",
        "The auditors have indicated their willingness to continue in office.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    let headings: Vec<&Block> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .collect();
    assert_eq!(
        headings.len(),
        1,
        "a title that wraps came back as {} headings: {:?}",
        headings.len(),
        shape(&blocks)
    );
    assert_eq!(
        headings[0].text,
        "NOTES TO AND FORMING PART OF THE FINANCIAL STATEMENTS FOR THE YEAR ENDED 30 JUNE",
        "the two lines are not one heading's text"
    );
}

/// And two headings of DIFFERENT type under each other stay two, which is the
/// clause that pays for the one above: joining adjacent headings without it
/// took the benchmark's heading axis from 0.465 to 0.408, where the rule with
/// it takes it to 0.508.
#[test]
fn two_headings_of_different_type_are_two_headings() {
    let mut page = vec![
        Line {
            text: "ANNUAL REPORT",
            x: MARGIN,
            y: TOP,
            size: BODY_SIZE + 6.0,
            bold: true,
        },
        Line {
            text: "Directors and officers",
            x: MARGIN,
            y: TOP - LEADING * 1.6,
            size: BODY_SIZE + 2.0,
            bold: true,
        },
    ];
    let mut y = TOP - LEADING * 3.6;
    for text in [
        "The directors of the company during the financial year were as follows,",
        "and each of them held office for the whole of the year under review,",
        "except where the notes to these statements say otherwise in a note.",
        "The company met four times in the year and the meetings are recorded.",
        "No director had an interest in a contract with the company in the year.",
        "The auditors have indicated their willingness to continue in office.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    let headings: Vec<&Block> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .collect();
    assert_eq!(
        headings.len(),
        2,
        "a title over a subtitle came back as one heading: {:?}",
        shape(&blocks)
    );
}

/// A document that sets its headings in the body's own type says it with the
/// page instead (#583): a line of its own, short of the measure, a finished
/// block above it, and the section it names starting flush under it.
///
/// The first heading of the page is NOT promoted and that is the rule rather
/// than a gap in it: the top of a page has no white above it to measure, which
/// is what `space_above` says with `None`.
///
/// Take `set_apart_by_the_page` out of `headings::classify` and the two that
/// are promoted come back as paragraphs.
#[test]
fn the_page_sets_a_heading_apart_where_the_type_says_nothing() {
    let mut page = Vec::new();
    let mut y = TOP;
    for (head, lines) in [
        (
            "Financial Reporting Framework",
            [
                "The financial report is a general purpose financial report which has",
                "been prepared in accordance with Accounting Standards and the law.",
            ],
        ),
        (
            "Significant Accounting Policies",
            [
                "The following is a summary of the material accounting policies which",
                "the consolidated entity adopted in the preparation of this report.",
            ],
        ),
        (
            "Principles of Consolidation",
            [
                "A controlled entity is any entity over which the parent is able to",
                "govern the financial and operating policies so as to obtain benefits.",
            ],
        ),
    ] {
        page.push(Line {
            text: head,
            x: MARGIN,
            y,
            size: BODY_SIZE,
            bold: false,
        });
        y -= LEADING * 1.9;
        for text in lines {
            page.push(body(text, y));
            y -= LEADING;
        }
        y -= LEADING * 2.8;
    }

    let blocks = blocks_of(&[page]);
    let headings: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(
        headings,
        vec![
            "Significant Accounting Policies",
            "Principles of Consolidation"
        ],
        "the page set these apart and the classifier did not: {:?}",
        shape(&blocks)
    );
}

/// And the clause that pays for it: a short line with no SECTION under it is
/// not a heading (#583).
///
/// The page here gives the rule its other three clauses -- the line is one
/// line, short of the measure, unpunctuated, and there is more white under it
/// than the page puts anywhere else -- and takes the fourth away: what follows
/// is indented like the next paragraph of a running argument rather than set
/// flush like the start of a section. A promoter without this clause was
/// measured on the round before and recovered 22 headings while inventing 58,
/// which is the whole reason this one asks four questions.
#[test]
fn a_short_line_with_no_section_under_it_is_not_a_heading() {
    // A page set with its paragraphs' first lines in and its blocks solid, so
    // that the white this page puts between two blocks is a line and the white
    // above the short line is far more than that.
    let first_line_in = |text: &'static str, y: f64| Line {
        text,
        x: MARGIN + 18.0,
        y,
        size: BODY_SIZE,
        bold: false,
    };
    let inset = |text: &'static str, y: f64| Line {
        text,
        x: MARGIN + 36.0,
        y,
        size: BODY_SIZE,
        bold: false,
    };
    let mut page = Vec::new();
    let mut y = TOP;
    for (first, second) in [
        (
            "The committee considered the proposals put before it in the order",
            "they had been received, and referred each of them to the officers,",
        ),
        (
            "The officers reported that the works had been completed within the",
            "estimate, and that the retention would be released in due course,",
        ),
        (
            "The chairman thanked the officers for their report and asked that",
            "the contractor be written to about the defects in the meantime,",
        ),
    ] {
        page.push(first_line_in(first, y));
        y -= LEADING;
        page.push(body(second, y));
        y -= LEADING;
    }
    // A short line of its own, unpunctuated, with far more white above it than
    // this page puts between its blocks: three of the four clauses hold.
    y -= LEADING * 2.5;
    page.push(body("and the works at the quay,", y));
    y -= LEADING * 2.5;
    // What follows is set IN from it -- a quotation, a note, a table's stub --
    // and none of those is the section a heading owns.
    for text in [
        "The surveyor was instructed to report on the quay wall before the",
        "meeting, the contractor having made good the defects in the meantime.",
    ] {
        page.push(inset(text, y));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    assert!(
        !blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Heading { .. })),
        "a short line with nothing of its own under it was read as a heading: {:?}",
        shape(&blocks)
    );
}

/// Capitals may speak for a block set slightly UNDER the body's size (#583):
/// a preprint's `I. INTRODUCTION` measures nine per cent under a page whose
/// running text is set one size up, and the band on its own refused it.
#[test]
fn capitals_speak_for_type_that_is_not_larger_than_the_body() {
    let mut page = vec![Line {
        text: "I. INTRODUCTION",
        x: MARGIN + 60.0,
        y: TOP,
        size: BODY_SIZE * 0.92,
        bold: false,
    }];
    let mut y = TOP - LEADING * 2.0;
    for i in 0..8 {
        page.push(body(
            match i % 2 {
                0 => "The nonvanishing neutrino masses have been confirmed by various",
                _ => "oscillation phenomena and indicate physics beyond the Standard Model",
            },
            y,
        ));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Heading { .. }) && b.text == "I. INTRODUCTION"),
        "capitals under the body's size said nothing: {:?}",
        shape(&blocks)
    );
}

/// A document that sets its sections at the paragraph gap still sets them
/// apart (#584).
///
/// The white above these headings is the white the page puts between two of
/// its paragraphs, to the point -- which is what the annual reports in the
/// benchmark do, and while the rule asked for MORE white than that it read
/// none of them. What it asks instead is that the block above be finished:
/// `Significant Accounting Policies` follows a sentence that has ended, and a
/// paragraph's own last line never does.
///
/// Take the promoter out of `headings::classify` and both come back as
/// paragraphs.
#[test]
fn a_section_set_at_the_paragraph_gap_is_still_a_heading() {
    let mut page = Vec::new();
    let mut y = TOP;
    // The page opens with a paragraph, so that the first heading has something
    // above it, and every gap on the page is the same 1.6 lines.
    for text in [
        "The consolidated entity has prepared this report for the year ended",
        "thirty June and the directors present it with the auditor's report.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    y -= LEADING * 0.6;
    for (head, lines) in [
        (
            "Financial Reporting Framework",
            [
                "The financial report is a general purpose financial report which has",
                "been prepared in accordance with Accounting Standards and the law.",
            ],
        ),
        (
            "Significant Accounting Policies",
            [
                "The following is a summary of the material accounting policies which",
                "the consolidated entity adopted in the preparation of this report.",
            ],
        ),
    ] {
        page.push(Line {
            text: head,
            x: MARGIN,
            y,
            size: BODY_SIZE,
            bold: false,
        });
        y -= LEADING * 1.6;
        for text in lines {
            page.push(body(text, y));
            y -= LEADING;
        }
        y -= LEADING * 0.6;
    }

    let blocks = blocks_of(&[page]);
    let headings: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(
        headings,
        vec![
            "Financial Reporting Framework",
            "Significant Accounting Policies"
        ],
        "the page sets its sections at its own paragraph gap and they were read \
         as body: {:?}",
        shape(&blocks)
    );
}

/// And the clause that pays for it: a line that continues the sentence above
/// it is not a heading, however much white it is given (#584).
///
/// The page here hands the promoter everything else -- one line, short of the
/// measure, no full stop, a section starting flush under it, and three lines
/// of white above it, far more than the page puts between two paragraphs. What
/// it takes away is the sentence: the block above breaks off mid-clause, which
/// is what a paragraph interrupted by anything at all looks like, and what
/// follows such a break is the rest of the sentence rather than a new section.
///
/// Take the `ends_sentence` clause out of `set_apart_by_the_page` and this
/// goes red: the white alone promotes the line.
#[test]
fn a_line_that_finishes_the_sentence_above_it_is_not_a_heading() {
    let mut page = Vec::new();
    let mut y = TOP;
    for text in [
        "The committee reviewed the tender documents at its meeting in March",
        "and resolved, subject to the conditions set out in the schedule, that",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    y -= LEADING * 3.0;
    page.push(body("the works be awarded", y));
    y -= LEADING * 3.0;
    for text in [
        "to the lowest conforming tenderer on the terms recorded in the minutes",
        "of that meeting, and that the chief executive sign the contract.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    assert_eq!(
        block_starting(&blocks, "the works be awarded").kind,
        BlockKind::Paragraph,
        "a sentence broken over a gap was read as a heading: {:?}",
        shape(&blocks)
    );
}

/// What stands under a heading is its SECTION, and a section can open with a
/// list (#584).
///
/// The clause that pays for the promoter is ownership -- the text under the
/// line begins at the line's own left edge -- and a page that opens its section
/// with numbered items owns them exactly as it owns a paragraph. Reading only a
/// paragraph as a section left these behind: of the 44 headings the document
/// reading of the benchmark reads as body, admitting a list or a table under
/// the line reaches 3 more, and over the whole annotation it reads 71 headings
/// against 66 with the same 9 other blocks.
///
/// Let only a `Paragraph` stand under the line again and this goes red.
#[test]
fn a_section_may_open_with_a_list() {
    let mut page = Vec::new();
    let mut y = TOP;
    for text in [
        "The board has adopted the policies set out below and reviews them",
        "each year at the meeting at which the annual report is approved.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    y -= LEADING * 0.6;
    page.push(body("Directors Interests", y));
    y -= LEADING * 1.6;
    for text in [
        "1. Each director holds the shares recorded in the register.",
        "2. No director has an interest in a contract with the company.",
        "3. No option over an unissued share was granted in the year.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    assert_eq!(
        block_starting(&blocks_of(&[page]), "Directors Interests").kind,
        BlockKind::Heading { level: 1 },
        "a section that opens with a list has no heading"
    );
}

/// And a heading is short of the MEASURE, not short of a title (#584).
///
/// Six tenths of the column is a line and a half of type, and the set names
/// its sections at length: `Alternate Non Executive Director - Appointed 29
/// September` is a heading of an annual report and three quarters of the
/// measure. Widening the clause to eight tenths reads 2 more of the annotation's
/// headings and not one more of its other blocks, because what the clause is
/// there to refuse is a FULL line of a paragraph.
///
/// Put six tenths back and this goes red.
#[test]
fn a_heading_may_run_to_three_quarters_of_the_measure() {
    let mut page = Vec::new();
    let mut y = TOP;
    for text in [
        "The directors present their report together with the financial report",
        "of the consolidated entity for the year ended thirty June.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    y -= LEADING * 0.6;
    page.push(body("Non Executive Director - Appointed 12 August", y));
    y -= LEADING * 1.6;
    for text in [
        "He is a chartered accountant and has served on the audit committee",
        "of the company since his appointment to the board in that year.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    assert_eq!(
        block_starting(&blocks_of(&[page]), "Non Executive Director").kind,
        BlockKind::Heading { level: 1 },
        "a heading that names a section at length was read as body"
    );
}

/// Two wrapped headings on one page are both put together (#584).
///
/// Joining removes a block, and the measurements the classifier reads were
/// taken of the blocks as they arrived -- so after the first join every later
/// block sits one row to the left of its own metrics, and the second pair is
/// compared against the style of the block BEFORE it. A page with one wrapped
/// heading never shows it; a page with two does, and the benchmark's annual
/// reports put a wrapped title over every note.
///
/// Read `metrics[i + 1]` again instead of the row the block came from and the
/// second title stays in halves.
#[test]
fn two_wrapped_headings_on_one_page_are_both_put_together() {
    // The shape of #581's own fixture, twice and in two sizes: a first line
    // that runs to the margin and a second set far in, so that each title
    // arrives here as two blocks rather than one.
    let wrapped = |first: &'static str, second: &'static str, size: f64, y: f64| {
        [
            Line {
                text: first,
                x: MARGIN + 30.0,
                y,
                size,
                bold: true,
            },
            Line {
                text: second,
                x: MARGIN + 300.0,
                y: y - LEADING * 1.3,
                size,
                bold: true,
            },
        ]
    };
    let mut page = Vec::new();
    page.extend(wrapped(
        "NOTES TO AND FORMING PART OF THE FINANCIAL STATEMENTS",
        "FOR THE YEAR ENDED 30 JUNE",
        BODY_SIZE + 4.0,
        TOP,
    ));
    let mut y = TOP - LEADING * 3.3;
    for text in [
        "The directors of the company during the financial year were as follows,",
        "and each of them held office for the whole of the year under review,",
        "except where the notes to these statements say otherwise in a note.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    page.extend(wrapped(
        "SIGNIFICANT ACCOUNTING POLICIES OF THE GROUP",
        "AND OF ITS CONTROLLED ENTITIES",
        BODY_SIZE + 2.0,
        y - LEADING * 1.5,
    ));
    y -= LEADING * 4.3;
    for text in [
        "The company met four times in the year and the meetings are recorded.",
        "No director had an interest in a contract with the company in the year.",
        "The auditors have indicated their willingness to continue in office.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }

    let blocks = blocks_of(&[page]);
    let headings: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(
        headings,
        vec![
            "NOTES TO AND FORMING PART OF THE FINANCIAL STATEMENTS FOR THE YEAR ENDED 30 JUNE",
            "SIGNIFICANT ACCOUNTING POLICIES OF THE GROUP AND OF ITS CONTROLLED ENTITIES"
        ],
        "the second wrapped title was read against the wrong block's type: {:?}",
        shape(&blocks)
    );
}
