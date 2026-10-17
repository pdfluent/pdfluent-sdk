//! The heading model decides, and says so where it differs from the rule (#599).
//!
//! The rules in `headings` have their own operator tests and run there with the
//! learned pass off: they assert what the rule does. This file asserts what the
//! ENGINE does with the switch on, which is the model's answer over the rule's
//! -- including the two pages where the two disagree, because a disagreement
//! nobody wrote down is one somebody rediscovers as a bug.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::{document_facts, refine_page, Switches};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;

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

fn pdf_of(pages: &[Vec<Line>]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })));
    let bold = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica-Bold".to_vec()),
    })));
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for lines in pages {
        let mut operations = Vec::new();
        for line in lines {
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
        let content = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations }.encode().expect("encode"),
        ));
        kids.push(Object::Reference(doc.add_object(Object::Dictionary(
            dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "Contents" => Object::Reference(content),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
                "Resources" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! {
                        "F1" => regular.clone(),
                        "F2" => bold.clone(),
                    }),
                }),
            },
        ))));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => count,
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

fn blocks_of(pages: &[Vec<Line>], learned: bool) -> Vec<Block> {
    let options = if learned {
        ExtractOptions::default()
    } else {
        ExtractOptions::default().without_the_learned_pass()
    };
    PdfDocument::open(pdf_of(pages))
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks")
}

fn headings(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .map(|b| b.text.as_str())
        .collect()
}

/// A page of sections set in the body's own type: the model reads all three,
/// where the rule reads two (#599, #584).
///
/// The rule cannot promote the FIRST block of a page -- there is no white above
/// it to measure, which is the gap #584 documented and left open. The model has
/// no such blind spot, and this is the case it was built for: a document that
/// says "heading" with the page rather than with the type.
///
/// Take the switch out of `learned::SWITCHES` and the first heading comes back
/// as a paragraph.
#[test]
fn the_model_reads_the_heading_the_rule_cannot_reach() {
    let pages = [sections_in_the_body_type()];

    assert_eq!(
        headings(&blocks_of(&pages, false)),
        vec!["Significant Accounting Policies"],
        "the rule reads the second heading and cannot reach the first"
    );
    assert_eq!(
        headings(&blocks_of(&pages, true)),
        vec![
            "Financial Reporting Framework",
            "Significant Accounting Policies"
        ],
        "the model reads both, which is what this switch is for"
    );
}

/// Two sections whose headings are set in the body's own type, the first at
/// the top of the page.
fn sections_in_the_body_type() -> Vec<Line> {
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
    page
}

/// And where the model is wrong, it is wrong here too (#599).
///
/// A sentence broken over a wide gap -- `the works be awarded` between two
/// halves of one sentence -- is not a heading, and the rule refuses it for the
/// reason #584 measured: what stands above it has not finished. The model
/// promotes it. On the validation split that family is 42 per cent of what it
/// promotes, a third of which are table rows, and the axis still wins because
/// what the model REMOVES outweighs it: the heading axis goes 0.5853 to 0.7454.
///
/// This test asserts the engine as it is, so that the day the model stops
/// making this mistake somebody has to come here and say so.
///
/// THAT DAY CAME with the line model (#574, #632): it reads the lines around
/// this one, sees the sentence run on above and below it, and leaves it body
/// text. The block model that promoted it is gone from the tree.
#[test]
fn the_model_promotes_a_broken_sentence_and_the_rule_does_not() {
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
    let pages = [page];

    assert!(
        headings(&blocks_of(&pages, false)).is_empty(),
        "the rule refuses a line whose sentence above has not finished"
    );
    assert!(
        headings(&blocks_of(&pages, true)).is_empty(),
        "the shipped line model leaves a sentence broken over a gap as body \
         text; the block model promoted it"
    );
}

/// Only the heading line model, or nothing.
///
/// The list and table switches are named and turned off; any switch added
/// later keeps its shipped value rather than breaking this literal, because
/// what these tests compare is one heading switch against another.
fn only(heading_lines: bool) -> Switches {
    let mut switches = pdf_engine::learned::SWITCHES;
    switches.list_item = false;
    switches.list_opens = false;
    switches.heading_lines = heading_lines;
    switches.table_region = false;
    switches.table_structure = false;
    switches.table_judge = false;
    switches.captions = false;
    switches.line_gnn = false;
    switches
}

/// The line model reads the heading the rule cannot reach, on its own, and
/// with its switch off the page comes back as the heuristics left it (#574).
///
/// The operator test for `heading_lines`: RED with the switch off. The page is
/// the one the block model was built for -- sections set in the body's own
/// type, the first at the top of the page where the rule has no white above it
/// to measure.
#[test]
fn the_line_model_reads_the_heading_the_rule_cannot_reach() {
    let pages = [sections_in_the_body_type()];
    let document = PdfDocument::open(pdf_of(&pages)).expect("open pdf");
    let blocks = document
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks");
    let facts = document_facts(&blocks);
    let size = (612.0, 792.0);
    let off = refine_page(blocks.clone(), size, &facts, only(false));
    let on = refine_page(blocks, size, &facts, only(true));
    assert_eq!(
        headings(&off),
        vec!["Significant Accounting Policies"],
        "with every heading model off the rule reads the second heading only"
    );
    assert_eq!(
        headings(&on),
        vec![
            "Financial Reporting Framework",
            "Significant Accounting Policies"
        ],
        "the line model reads both"
    );
    let first = on
        .iter()
        .find(|b| b.text == "Financial Reporting Framework")
        .expect("the first heading");
    assert!(
        first.confidence.is_some_and(|p| p >= 0.45),
        "a heading the line model made carries its probability: {:?}",
        first.confidence
    );
}

/// The pass does its work with this switch ALONE, and the guard that makes it
/// free when everything is off does not swallow it (#599).
///
/// `decide_page` returns early when no switch is on, and that condition names
/// every switch there is: miss one and its lane silently does nothing while
/// every other test stays green. The boundary lane proves the same from its
/// side (`learned_heading_boundary_on_a_real_page.rs`), so a fifth switch has
/// two tests to answer to rather than a condition somebody has to re-read.
#[test]
fn the_heading_switch_works_on_its_own() {
    let mut page = Vec::new();
    let mut y = TOP;
    for (head, lines) in [(
        "Significant Accounting Policies",
        [
            "The following is a summary of the material accounting policies which",
            "the consolidated entity adopted in the preparation of this report.",
        ],
    )] {
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
    }
    let document = PdfDocument::open(pdf_of(&[page])).expect("open pdf");
    let blocks = document
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks");
    let facts = document_facts(&blocks);
    let size = (612.0, 792.0);

    let nothing_on = refine_page(
        blocks.clone(),
        size,
        &facts,
        Switches {
            list_item: false,
            list_opens: false,
            heading_lines: false,
            captions: false,
            line_gnn: false,
            line_image: false,
            line_gnn_v2: false,
            demote_per_cell: false,
            table_region: false,
            table_structure: false,
            table_judge: false,
            table_lattice: false,
            table_stitch_header: false,
            table_stitch_rows: false,
            table_stitch_merge: false,
            table_is_v2: false,
            table_drop_empty_columns: false,
            table_cell_text_by_line: false,
            table_v2_release_rejoin: false,
            table_v2_release_column_order: false,
            table_split_side_by_side: false,
            table_split_stacked: false,
            table_runs_on_figures: false,
            markdown_header_block_figures: false,
            table_corridor_split: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
        },
    );
    let only_heading = refine_page(
        blocks,
        size,
        &facts,
        Switches {
            list_item: false,
            list_opens: false,
            heading_lines: true,
            captions: false,
            line_gnn: false,
            line_image: false,
            line_gnn_v2: false,
            demote_per_cell: false,
            table_region: false,
            table_structure: false,
            table_judge: false,
            table_lattice: false,
            table_stitch_header: false,
            table_stitch_rows: false,
            table_stitch_merge: false,
            table_is_v2: false,
            table_drop_empty_columns: false,
            table_cell_text_by_line: false,
            table_v2_release_rejoin: false,
            table_v2_release_column_order: false,
            table_split_side_by_side: false,
            table_split_stacked: false,
            table_runs_on_figures: false,
            markdown_header_block_figures: false,
            table_corridor_split: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
        },
    );

    let headings = |blocks: &[Block]| {
        blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .count()
    };
    assert_eq!(
        headings(&nothing_on),
        0,
        "with every switch off the pass must change nothing"
    );
    assert!(
        headings(&only_heading) > 0,
        "with this switch alone the pass must still decide: if the early \
         return has forgotten a flag, this lane does nothing and no other \
         test says so"
    );
}

/// A kind the model produced carries the probability that produced it (#599).
///
/// `Block::confidence` is documented as the probability of the decision behind
/// THIS block's kind, and a caller routing uncertain decisions for review has
/// nothing else to read. The list lane has set it since #1977; the heading lane
/// changed the kind and left the heuristics' `None` in place, so every promoted
/// heading looked like one nobody had decided (codex, #1983).
#[test]
fn a_block_the_model_promotes_carries_the_probability_that_promoted_it() {
    let mut page = Vec::new();
    let mut y = TOP;
    page.push(Line {
        text: "Significant Accounting Policies",
        x: MARGIN,
        y,
        size: BODY_SIZE,
        bold: false,
    });
    y -= LEADING * 1.9;
    for text in [
        "The following is a summary of the material accounting policies which",
        "the consolidated entity adopted in the preparation of this report.",
    ] {
        page.push(body(text, y));
        y -= LEADING;
    }
    let document = PdfDocument::open(pdf_of(&[page])).expect("open pdf");
    let blocks = document
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks");
    let facts = document_facts(&blocks);
    let refined = refine_page(
        blocks,
        (612.0, 792.0),
        &facts,
        Switches {
            list_item: false,
            list_opens: false,
            heading_lines: true,
            captions: false,
            line_gnn: false,
            line_image: false,
            line_gnn_v2: false,
            demote_per_cell: false,
            table_region: false,
            table_structure: false,
            table_judge: false,
            table_lattice: false,
            table_stitch_header: false,
            table_stitch_rows: false,
            table_stitch_merge: false,
            table_is_v2: false,
            table_drop_empty_columns: false,
            table_cell_text_by_line: false,
            table_v2_release_rejoin: false,
            table_v2_release_column_order: false,
            table_split_side_by_side: false,
            table_split_stacked: false,
            table_runs_on_figures: false,
            markdown_header_block_figures: false,
            table_corridor_split: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
        },
    );

    let promoted: Vec<&Block> = refined
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .collect();
    assert!(
        !promoted.is_empty(),
        "nothing was promoted, so this test says nothing about confidence"
    );
    for block in &promoted {
        let p = block.confidence.unwrap_or_else(|| {
            panic!(
                "a heading the model made carries no probability: {:?}",
                block.text
            )
        });
        assert!(
            (0.0..=1.0).contains(&p),
            "a probability outside [0, 1]: {p}"
        );
    }

    // AND A BLOCK IT LEFT ALONE KEEPS ITS `None`. A number on a kind the model
    // did not decide would read as a number about that kind.
    for block in refined
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
    {
        assert_eq!(
            block.confidence, None,
            "a paragraph the model did not touch carries a probability: {:?}",
            block.text
        );
    }
}

/// The line graph network reads both headings too, on its own (#638).
///
/// The operator test for `line_gnn`: with every other heading switch off, the
/// network (its probability averaged with the heading line model's, as it
/// ships) reads the heading the rule cannot reach, and with it off the page is
/// the heuristics' again.
#[test]
fn the_line_graph_network_reads_the_heading_the_rule_cannot_reach() {
    let pages = [sections_in_the_body_type()];
    let document = PdfDocument::open(pdf_of(&pages)).expect("open pdf");
    let blocks = document
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks");
    let facts = document_facts(&blocks);
    let size = (612.0, 792.0);
    let mut on = only(false);
    on.line_gnn = true;
    let with = refine_page(blocks.clone(), size, &facts, on);
    let without = refine_page(blocks, size, &facts, only(false));
    assert_eq!(
        headings(&without),
        vec!["Significant Accounting Policies"],
        "with every heading model off the rule reads the second heading only"
    );
    assert_eq!(
        headings(&with),
        vec![
            "Financial Reporting Framework",
            "Significant Accounting Policies"
        ],
        "the graph network reads both"
    );
}

/// A title the heuristics make stays one when only one of the two line models
/// is sure of it (#574, round 4).
///
/// The shipped heading decision is the mean of the graph network and
/// `heading_lines`. On ParseBench it fell to 0.51-0.58 on titles
/// `heading_lines` read at 0.66-0.94 -- `Explanation :` on an exam sheet,
/// `AUSTRALIA` over a distribution list, a council's names -- and took them
/// away, which cost the formatting axis 0.446 -> 0.429. So a block the
/// heuristics already call a heading keeps the kind at a lower mean than a
/// promotion needs: the pair has to agree to take a title away.
///
/// The page is this sheet's shape: a bold question, its options, the answer,
/// and a bold `Explanation :` over the dashed lines that explain it. On it the
/// graph network reads the label at 0.94 and `heading_lines` at 0.20: a mean
/// of 0.57, a heading at the keeping bar and not at the promoting one.
#[test]
fn a_bold_label_over_its_explanation_stays_a_heading() {
    const SIZE: f64 = 11.0;
    const STEP: f64 = SIZE * 1.4;
    let mut page = Vec::new();
    let mut y = TOP;
    let mut put = |text: &'static str, x: f64, bold: bool| {
        page.push(Line {
            text,
            x,
            y,
            size: SIZE,
            bold,
        });
        y -= STEP;
    };
    put(
        "Ques : The bank raised the small loan ceiling for",
        MARGIN,
        true,
    );
    put("cooperative banks to what amount?", MARGIN, true);
    for option in [
        "(a) Rs 1 crore",
        "(b) Rs 2 crore",
        "(c) Rs 3 crore",
        "(d) Rs 4 crore",
    ] {
        put(option, MARGIN + 18.0, false);
    }
    put("Answer : Option C", MARGIN, false);
    put("Explanation :", MARGIN, true);
    for text in [
        "- The bank has increased the small loan ceiling for",
        "- cooperative banks to Rs 3 crore this year.",
        "- Previously the limit was two tenths of a per cent",
        "- of their capital, with a cap of Rs 1 crore.",
    ] {
        put(text, MARGIN + 18.0, false);
    }
    let without = blocks_of(&[page_clone(&page)], false);
    assert!(
        headings(&without).contains(&"Explanation :"),
        "the heuristics make the label a heading: {:?}",
        headings(&without)
    );
    let with = blocks_of(&[page], true);
    assert!(
        headings(&with).contains(&"Explanation :"),
        "the learned pass took the label's heading away: {:?}",
        headings(&with)
    );
}

fn page_clone(page: &[Line]) -> Vec<Line> {
    page.iter()
        .map(|l| Line {
            text: l.text,
            x: l.x,
            y: l.y,
            size: l.size,
            bold: l.bold,
        })
        .collect()
}
