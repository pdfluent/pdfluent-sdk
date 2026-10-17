//! One doubt score per page, from decisions the learned pass already takes.
//!
//! The auto-router hands a page to the teacher only when the engine is in
//! doubt, and `learned::page_confidence` is that doubt: the largest of the
//! heading, link and class-entropy parts, with the heading probability per
//! line to localise it. Every number comes from the same `decide_page`
//! decisions `refine` acts on, with the switches as shipped -- no new model.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::{
    cell_hints, document_vectors, page_confidence, page_confidence_with_switches, Switches,
    SWITCHES,
};
use pdf_engine::{Block, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const HEADING_SIZE: f64 = 16.0;
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
const SIZE: [(f64, f64); 1] = [(612.0, 792.0)];

struct Line {
    text: &'static str,
    size: f64,
    bold: bool,
}

const SECTIONS: [(&str, [&str; 6]); 4] = [
    (
        "Financial Reporting Framework",
        [
            "The financial report is a general purpose report which has been",
            "prepared in accordance with Accounting Standards and the law.",
            "It covers the consolidated entity for the year ended 30 June.",
            "Comparative figures have been restated where the standards require.",
            "The directors are satisfied the entity remains a going concern.",
            "Rounding in these statements follows the legislative instrument.",
        ],
    ),
    (
        "Significant Accounting Policies",
        [
            "The following is a summary of the material accounting policies.",
            "The consolidated entity adopted them in preparing this report.",
            "Revenue is recognised when control of the goods has passed.",
            "Trade receivables are carried at amortised cost less provisions.",
            "Plant and equipment is depreciated over its useful life.",
            "Impairment is tested whenever events suggest the amount may not hold.",
        ],
    ),
    (
        "Risk Management",
        [
            "The board oversees the framework that identifies and prices risk.",
            "Credit risk is spread across a large number of counterparties.",
            "Liquidity is held well above the minimum the covenants require.",
            "Interest rate exposure is hedged within the approved bands.",
            "Foreign balances are translated at the rates ruling at year end.",
            "No material concentration of risk arose during the period.",
        ],
    ),
    (
        "Subsequent Events",
        [
            "No matter has arisen since year end that requires adjustment.",
            "A dividend was declared after the reporting date, as disclosed.",
            "The facility was refinanced on substantially the same terms.",
            "There were no changes in the composition of the group.",
            "Litigation outstanding at year end remains unresolved.",
            "The directors know of nothing further that is material.",
        ],
    ),
];

/// One page of the sections, headings set as given, body at body size.
fn page_of(clear: bool) -> Vec<Line> {
    let mut page = Vec::new();
    for (head, body) in SECTIONS {
        page.push(Line {
            text: head,
            size: if clear { HEADING_SIZE } else { BODY_SIZE },
            bold: clear,
        });
        for text in body {
            page.push(Line {
                text,
                size: BODY_SIZE,
                bold: false,
            });
        }
    }
    page
}

fn pdf_of(pages: &[Vec<Line>], clear: bool) -> Vec<u8> {
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
        let mut y = TOP;
        for line in lines {
            // WITH WHITESPACE, the heading stands apart: a gap above and
            // below it that body lines never have. Without, every line steps
            // one leading down, headings and body alike.
            if clear && line.size > BODY_SIZE {
                y -= LEADING;
            }
            let face: &[u8] = if line.bold { b"F2" } else { b"F1" };
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new(
                "Tf",
                vec![Object::Name(face.to_vec()), line.size.into()],
            ));
            operations.push(Operation::new("Td", vec![MARGIN.into(), y.into()]));
            operations.push(Operation::new(
                "Tj",
                vec![Object::string_literal(line.text)],
            ));
            operations.push(Operation::new("ET", vec![]));
            y -= LEADING;
            if clear && line.size > BODY_SIZE {
                y -= LEADING;
            }
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

/// The page's blocks as the learned pass reads them: before it has run.
fn blocks_of(clear: bool) -> Vec<Block> {
    PdfDocument::open(pdf_of(&[page_of(clear)], clear))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

/// A page with clear headings doubts almost no line, and the same page with
/// body-size headings and no whitespace doubts more of them.
#[test]
fn clear_headings_doubt_less_than_body_size_ones() {
    let clear = page_confidence(&blocks_of(true), &SIZE, &[]);
    let plain = page_confidence(&blocks_of(false), &SIZE, &[]);
    assert_eq!(clear.len(), 1, "one page in, one doubt out");
    assert_eq!(plain.len(), 1, "one page in, one doubt out");
    assert!(
        clear[0].heading_doubt < 0.1,
        "a page of large bold headings with whitespace doubts almost no line: {}",
        clear[0].heading_doubt
    );
    assert!(
        plain[0].heading_doubt > clear[0].heading_doubt,
        "body-size headings with no whitespace doubt more lines: {} against {}",
        plain[0].heading_doubt,
        clear[0].heading_doubt
    );
}

/// The per-line heading probability is the one the pass decides on: each
/// block's mean over its lines is the `CellHint` the table pass reads.
#[test]
fn p_heading_per_line_is_what_refine_decides_on() {
    use pdf_engine::learned::features::page_features;
    use pdf_engine::learned::lines::lines_of;

    for clear in [true, false] {
        let blocks = blocks_of(clear);
        let refs: Vec<&Block> = blocks.iter().collect();
        let words = page_features(&refs, SIZE[0].0, SIZE[0].1).words;
        let hints = cell_hints(&blocks, &SIZE, &[]);
        let conf = &page_confidence(&blocks, &SIZE, &[])[0];
        let mut at = 0;
        for (pos, hint) in hints.iter().enumerate() {
            let n = lines_of(pos, &words).len();
            let ps: Vec<f64> = conf.lines[at..at + n]
                .iter()
                .filter_map(|l| l.p_heading)
                .collect();
            at += n;
            match hint.p_heading {
                Some(mean) => {
                    assert_eq!(
                        ps.len(),
                        n,
                        "a decided block's every line carries the probability"
                    );
                    let got = ps.iter().sum::<f64>() / ps.len() as f64;
                    assert!(
                        (got - mean).abs() < 1e-12,
                        "block {pos} means {got}, the pass decided {mean}"
                    );
                }
                None => assert!(
                    ps.is_empty(),
                    "a block the pass never read carries no probability"
                ),
            }
        }
        assert_eq!(at, conf.lines.len(), "every line belongs to a block");
    }
}

/// Doubt is a share in 0..=1, and the largest of its parts.
#[test]
fn doubt_is_a_share_above_its_parts() {
    for clear in [true, false] {
        let conf = &page_confidence(&blocks_of(clear), &SIZE, &[])[0];
        for (name, v) in [
            ("doubt", conf.doubt),
            ("heading_doubt", conf.heading_doubt),
            ("link_doubt", conf.link_doubt),
            ("class_entropy", conf.class_entropy),
        ] {
            assert!(
                (0.0..=1.0).contains(&v),
                "{name} is a share in 0..=1 on the {} page: {v}",
                if clear { "clear" } else { "plain" }
            );
        }
        assert!(
            conf.doubt >= conf.heading_doubt
                && conf.doubt >= conf.link_doubt
                && conf.doubt >= conf.class_entropy,
            "doubt {} is the largest part on the {} page: {:?}",
            conf.doubt,
            if clear { "clear" } else { "plain" },
            (conf.heading_doubt, conf.link_doubt, conf.class_entropy)
        );
    }
}

/// With the graph network off, the link and class parts are NaN -- nothing
/// measured them -- and doubt is the heading part.
#[test]
fn without_the_network_only_the_heading_part_is_measured() {
    let switches = pdf_engine::learned::Switches {
        line_gnn: false,
        ..SWITCHES
    };
    let conf = &page_confidence_with_switches(&blocks_of(true), &SIZE, &[], switches)[0];
    assert!(
        conf.link_doubt.is_nan(),
        "no network, no link doubt: {}",
        conf.link_doubt
    );
    assert!(
        conf.class_entropy.is_nan(),
        "no network, no class entropy: {}",
        conf.class_entropy
    );
    assert_eq!(
        conf.doubt, conf.heading_doubt,
        "doubt is the heading part alone"
    );
    assert!(!conf.lines.is_empty(), "the page's lines are still listed");
    assert!(
        conf.lines.iter().all(|l| l.class_probs.is_empty()),
        "no network, no class distributions"
    );
    assert!(
        conf.lines.iter().all(|l| l.p_link_next.is_nan()),
        "no network, no link probabilities"
    );
}

/// A page without lines doubts nothing and lists nothing.
#[test]
fn a_page_without_lines_has_no_doubt() {
    let conf = &page_confidence(&[], &SIZE, &[])[0];
    assert!(conf.lines.is_empty(), "no lines in, none out");
    assert_eq!(conf.doubt, 0.0, "no lines, no doubt");
}

/// With the retrained network on, the heading doubt is measured around 0.5:
/// recomputing it from the per-line probabilities with the 0.5 bar gives the
/// reported share, and the old 0.6 bar gives another -- on a real page with a
/// line that splits the two windows.
#[test]
fn heading_doubt_is_measured_around_the_retrained_bar() {
    let switches = Switches {
        line_gnn: true,
        line_gnn_v2: true,
        ..SWITCHES
    };
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
    );
    let doc = PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open");
    let sizes: Vec<(f64, f64)> = (0..doc.page_count())
        .map(|p| {
            doc.page_geometry(p)
                .map(|g| g.effective_dimensions())
                .unwrap_or((0.0, 0.0))
        })
        .collect();
    let blocks = doc
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("blocks");
    let conf = &page_confidence_with_switches(&blocks, &sizes, &[], switches)[0];
    let vectors = document_vectors(&blocks, &sizes);
    let mut at = 0usize;
    let mut near_half = 0usize;
    let mut near_old = 0usize;
    let mut decided = 0usize;
    for (block, v) in blocks.iter().zip(&vectors) {
        assert_eq!(block.page, 0, "the fixture is one page");
        let old_bar = if v.kind == "heading" { 0.5 } else { 0.6 };
        for _ in 0..v.lines.len() {
            if let Some(p) = conf.lines[at].p_heading {
                decided += 1;
                if (p - 0.5).abs() <= 0.15 {
                    near_half += 1;
                }
                if (p - old_bar).abs() <= 0.15 {
                    near_old += 1;
                }
            }
            at += 1;
        }
    }
    assert_eq!(at, conf.lines.len(), "every line belongs to a block");
    assert!(decided > 0, "the page decides no line");
    let share = |near: usize| near as f64 / decided as f64;
    assert!(
        (share(near_half) - conf.heading_doubt).abs() < 1e-12,
        "heading doubt {} is not the share around 0.5 ({})",
        conf.heading_doubt,
        share(near_half)
    );
    assert!(
        (share(near_old) - conf.heading_doubt).abs() > 1e-9,
        "no line splits the doubt around 0.5 from the doubt around 0.6: {}",
        conf.heading_doubt
    );
}
