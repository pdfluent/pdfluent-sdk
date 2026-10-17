//! Running heads leave the flow, and a paragraph the page break cut in two is
//! read as one (#571).
//!
//! Every page here is built from operators, so what is measured is the whole
//! path: interpreter, spans, cut, paragraph merge, the two passes of this
//! ticket, serializer. Take `crate::furniture` out and every case that asserts
//! a `Header` or a `Footer` goes red, and so does the markdown of the first
//! one: the line the document repeats comes back between two paragraphs.
//! Take the joining out of `crate::reading_order` and the four page-break
//! cases go red -- the halves of the sentence come back as two paragraphs.
//!
//! The failure cases the ticket names each have a case of their own: a first
//! page without the running head, a footnote that must stay at the foot of its
//! page and must not be joined onto the next one, and a landscape page in a
//! portrait document.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
/// The top of the text frame, well below the running head's band.
const TOP: f64 = 680.0;
/// One line of the body to the next.
const LEADING: f64 = 12.0;
/// Where a running header sits: a band of its own, a line and a half of white
/// between it and the first line of the body.
const HEAD_Y: f64 = 720.0;
/// Where a running footer sits, with the same white under the last line.
const FOOT_Y: f64 = 80.0;

/// One run of glyphs at a point, in a size of its own.
struct Piece {
    text: String,
    x: f64,
    y: f64,
    size: f64,
}

/// A page: its pieces and the box it is drawn in.
struct Page {
    pieces: Vec<Piece>,
    width: f64,
    height: f64,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        size: BODY_SIZE,
    }
}

fn small(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        size: 7.0,
    }
}

/// A column of body text at the standard leading, from `TOP` down.
fn body(lines: &[&str]) -> Vec<Piece> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| at(line, MARGIN, TOP - i as f64 * LEADING))
        .collect()
}

fn portrait(pieces: Vec<Piece>) -> Page {
    Page {
        pieces,
        width: 612.0,
        height: 792.0,
    }
}

/// The same frame turned on its side: the running head has to be recognised
/// against this page's own measure and not against the portrait pages'.
fn landscape(pieces: Vec<Piece>) -> Page {
    Page {
        pieces,
        width: 792.0,
        height: 612.0,
    }
}

fn stream_of(pieces: &[Piece]) -> Vec<u8> {
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), piece.size.into()],
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
    Content { operations }.encode().expect("encode content")
}

fn pdf_of(pages: &[Page]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));

    let mut page_ids = Vec::new();
    for page in pages {
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            stream_of(&page.pieces),
        )));
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), page.width.into(), page.height.into(),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => regular.clone() }),
            }),
        })));
    }

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>()),
        "Count" => Object::Integer(page_ids.len() as i64),
    }));
    for page_id in &page_ids {
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(*page_id) {
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

/// THE FURNITURE RULE AND NOT THE MODEL. These pages are built to test what
/// `furniture::mark` does, and the learned pass of #599 decides afterwards --
/// it calls `Annual Report 2026` on a two-page report a heading, where this
/// rule leaves it in the body because two pages cannot show repetition. Run
/// together, a red test here would not say which of the two had changed.
fn blocks_of(pages: &[Page]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pages))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

fn kind_of<'a>(blocks: &'a [Block], starting: &str) -> &'a BlockKind {
    &blocks
        .iter()
        .find(|block| block.text.starts_with(starting))
        .unwrap_or_else(|| panic!("no block starting {starting:?} in {:?}", texts(blocks)))
        .kind
}

fn texts(blocks: &[Block]) -> Vec<&str> {
    blocks.iter().map(|block| block.text.as_str()).collect()
}

/// Four pages of a report, each with the same line at the top and a page
/// number at the foot.
fn report(head: &str, pages: usize) -> Vec<Page> {
    (0..pages)
        .map(|page| {
            let mut pieces = vec![at(head, MARGIN, HEAD_Y)];
            pieces.extend(body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]));
            pieces.push(at(&format!("Page {} of {pages}", page + 1), 290.0, FOOT_Y));
            portrait(pieces)
        })
        .collect()
}

#[test]
fn three_pages_that_repeat_a_line_at_the_top_make_it_a_header() {
    let blocks = blocks_of(&report("Annual Report 2026", 4));
    let heads: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.text.starts_with("Annual Report"))
        .collect();
    assert_eq!(heads.len(), 4, "{:?}", texts(&blocks));
    for head in heads {
        assert_eq!(head.kind, BlockKind::Header, "{:?}", head.text);
    }
}

#[test]
fn a_page_number_that_counts_up_is_still_one_footer() {
    let blocks = blocks_of(&report("Annual Report 2026", 4));
    let feet: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.text.starts_with("Page "))
        .collect();
    assert_eq!(feet.len(), 4, "{:?}", texts(&blocks));
    for foot in feet {
        assert_eq!(foot.kind, BlockKind::Footer, "{:?}", foot.text);
    }
}

#[test]
fn two_pages_are_not_a_running_head() {
    // The same line at the top of two pages is a pair, and a pair is a
    // coincidence: two schedules of a contract open alike. Three is the
    // shortest run that is the typesetter speaking.
    let blocks = blocks_of(&report("Annual Report 2026", 2));
    assert_eq!(
        kind_of(&blocks, "Annual Report"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn the_page_without_the_running_head_keeps_its_own_first_line() {
    // The commonest shape of all: a title page whose top line is its own, and
    // the running head from the second page on.
    let mut pages = report("Annual Report 2026", 4);
    pages[0].pieces[0] = at("What This Report Is For", MARGIN, HEAD_Y);
    let blocks = blocks_of(&pages);
    assert_eq!(
        kind_of(&blocks, "What This Report"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
    assert_eq!(kind_of(&blocks, "Annual Report"), &BlockKind::Header);
}

#[test]
fn a_line_too_near_the_body_is_not_furniture() {
    // The same words at the top of four pages, a step above the body that is
    // too big for the merge and too small for a band of white: a standing
    // clause over every page of a form is the top of its page and not its
    // furniture. The footer three lines under the last body line IS.
    let pages: Vec<Page> = (0..4)
        .map(|page| {
            let mut pieces = vec![at(
                "Subject to the terms of schedule two.",
                MARGIN,
                TOP + 17.0,
            )];
            pieces.extend(body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]));
            pieces.push(at("Page 1 of 4", 290.0, FOOT_Y));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    assert_eq!(
        kind_of(&blocks, "Subject to the terms"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
    assert_eq!(kind_of(&blocks, "Page 1 of 4"), &BlockKind::Footer);
}

#[test]
fn a_repeated_line_the_merge_took_into_the_body_is_body() {
    // The same clause at the top of four pages, set at the body's own
    // leading. The merge takes it into the paragraph it opens (#567), and
    // what a page then repeats is a whole paragraph rather than a line.
    let pages: Vec<Page> = (0..4)
        .map(|page| {
            let mut pieces = body(&[
                "Subject to the terms set out in schedule two below.",
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
                "And a third, which is where the page's own matter begins.",
            ]);
            pieces.push(at("Page 1 of 4", 290.0, FOOT_Y));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    for block in &blocks {
        assert_ne!(block.kind, BlockKind::Header, "{:?}", block.text);
    }
}

#[test]
fn a_landscape_page_is_judged_against_its_own_measure() {
    // A wide table turned sideways in the middle of a portrait report. Its
    // running head is CENTRED, as a running head is, so it stands some 150
    // points further across the page than the head of every portrait page --
    // and at the same place on its own measure. A position compared in points
    // finds two groups of which neither reaches three pages; a position
    // compared as a fraction of the page's own text finds one group of four.
    const HEAD: &str = "Annual Report 2026";
    let pages: Vec<Page> = (0..4)
        .map(|page| {
            let wide = page == 2;
            let measure = if wide { 628.0 } else { 318.0 };
            let line = if wide {
                "The section opens here and runs the whole width of a page that was turned to hold it."
            } else {
                "The section opens here and says something."
            };
            let mut pieces = vec![at(
                HEAD,
                MARGIN + (measure - HEAD.len() as f64 * BODY_SIZE * 0.5) / 2.0,
                HEAD_Y,
            )];
            pieces.extend(body(&[line, "It runs on for a second line of its own body type."]));
            pieces.push(at(
                &format!("Page {} of 4", page + 1),
                MARGIN + measure / 2.0,
                FOOT_Y,
            ));
            if wide {
                landscape(pieces)
            } else {
                portrait(pieces)
            }
        })
        .collect();

    let blocks = blocks_of(&pages);
    let heads: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.text.starts_with("Annual Report"))
        .collect();
    assert_eq!(heads.len(), 4, "{:?}", texts(&blocks));
    for head in heads {
        assert_eq!(head.kind, BlockKind::Header, "{:?}", head.text);
    }

    // And the heads really are far apart in points, or the case above would
    // pass whatever the rule compared.
    let x: Vec<f64> = blocks
        .iter()
        .filter(|block| block.text.starts_with("Annual Report"))
        .map(|block| block.bbox[0])
        .collect();
    let spread = x.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - x.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(spread > 100.0, "{x:?}");
}

#[test]
fn a_footnote_that_differs_on_every_page_is_not_a_footer() {
    let pages: Vec<Page> = (0..4)
        .map(|page| {
            let mut pieces = body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]);
            // Words of its own, not a number in a fixed sentence: what a
            // footnote says is about the page it is on.
            pieces.push(small(
                [
                    "1 On the meaning of tenancy in this clause.",
                    "2 The register was closed in March.",
                    "3 See the correspondence with the auditor.",
                    "4 Quoted from the second reading.",
                ][page],
                MARGIN,
                FOOT_Y,
            ));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    for block in &blocks {
        assert_ne!(block.kind, BlockKind::Footer, "{:?}", block.text);
    }
}

#[test]
fn a_roman_numeral_at_the_foot_is_a_page_number() {
    // The front matter of a report, numbered ii, iii, iv. Nothing the two
    // pages share is a character, so a comparison of the text alone finds
    // nothing; a lone roman numeral is read as the number it is.
    let pages: Vec<Page> = ["ii", "iii", "iv", "v"]
        .iter()
        .enumerate()
        .map(|(page, number)| {
            let mut pieces = body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]);
            pieces.push(at(number, 300.0, FOOT_Y));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    let feet: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.kind == BlockKind::Footer)
        .collect();
    assert_eq!(feet.len(), 4, "{:?}", texts(&blocks));
}

#[test]
fn the_running_head_leaves_the_flow_and_stays_in_the_blocks() {
    let pages = report("Annual Report 2026", 4);
    let blocks = blocks_of(&pages);
    let markdown = to_markdown(&blocks);
    assert!(
        !markdown.contains("Annual Report") && !markdown.contains("Page 1 of"),
        "{markdown}"
    );
    assert!(markdown.contains("The 0th section opens"), "{markdown}");

    // Still in the model, and after the body: a caller reading the blocks in
    // order reads the document and then the lines it repeats.
    let first_furniture = blocks
        .iter()
        .position(|block| matches!(block.kind, BlockKind::Header | BlockKind::Footer))
        .expect("furniture");
    assert!(blocks[first_furniture..]
        .iter()
        .all(|block| matches!(block.kind, BlockKind::Header | BlockKind::Footer)));
    assert_eq!(blocks.len() - first_furniture, 8, "{:?}", texts(&blocks));

    // And the index is the order of the vector, with no gap where the
    // furniture came out of the body.
    for (n, block) in blocks.iter().enumerate() {
        assert_eq!(block.index, n, "{:?}", block.text);
    }
}

/// A column of text at `x`, its paragraphs separated by a line of white.
fn column(paragraphs: &[&[&str]], x: f64) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = TOP;
    for paragraph in paragraphs {
        for line in *paragraph {
            pieces.push(at(line, x, y));
            y -= LEADING;
        }
        y -= LEADING;
    }
    pieces
}

#[test]
fn a_two_column_page_is_read_one_column_at_a_time() {
    // The left column runs deeper than the right, so the block LOWEST on the
    // page is the left column's and the block LAST in the reading order is
    // the right column's. Only the second of those two readings joins the
    // sentence the page break cut in half, which is what makes this case
    // measure the column order rather than assert it.
    let mut pieces = column(
        &[
            &[
                "The first section of the left column opens here and",
                "runs to a full stop.",
            ],
            &[
                "The second says something else of its own and also",
                "runs to a full stop.",
            ],
            &[
                "The third is deeper down the page than anything in",
                "the right column is.",
            ],
        ],
        MARGIN,
    );
    pieces.extend(column(
        &[
            &[
                "The right column opens with a paragraph of its own",
                "and comes to a stop.",
            ],
            &[
                "It closes with a sentence that is cut in half by the",
                "end of the page and the",
            ],
        ],
        330.0,
    ));

    let pages = vec![
        portrait(pieces),
        portrait(body(&[
            "beginning of the next, where it runs on to its own full",
            "stop at last.",
        ])),
    ];
    let blocks = blocks_of(&pages);

    let order: Vec<&str> = blocks
        .iter()
        .filter(|block| block.page == 0)
        .map(|block| block.text.as_str())
        .collect();
    assert_eq!(order.len(), 5, "{order:?}");
    assert!(order[0].starts_with("The first section"), "{order:?}");
    assert!(order[1].starts_with("The second says"), "{order:?}");
    assert!(order[2].starts_with("The third is deeper"), "{order:?}");
    assert!(order[3].starts_with("The right column opens"), "{order:?}");
    assert!(order[4].starts_with("It closes with"), "{order:?}");

    let markdown = to_markdown(&blocks);
    assert!(
        markdown.contains("the end of the page and the beginning of the next"),
        "{markdown}"
    );
}

/// Two pages whose body is cut in half by the page break.
fn broken(first: &[&str], second: &[&str]) -> Vec<Page> {
    vec![portrait(body(first)), portrait(body(second))]
}

#[test]
fn a_paragraph_the_page_break_cut_in_two_is_written_as_one() {
    let blocks = blocks_of(&broken(
        &[
            "The committee met on the fourth of June and resolved that the",
            "proposal put before it by the secretary of the standing",
        ],
        &[
            "committee should be referred back for a second reading before",
            "the end of the financial year.",
        ],
    ));
    let markdown = to_markdown(&blocks);
    assert!(
        markdown.contains("standing committee should be referred"),
        "{markdown}"
    );
    assert_eq!(markdown.matches("\n\n").count(), 0, "{markdown}");
    assert!(blocks[1].continues_previous, "{:?}", texts(&blocks));
}

#[test]
fn a_full_stop_before_the_page_break_ends_the_paragraph() {
    let blocks = blocks_of(&broken(
        &[
            "The committee met on the fourth of June and resolved that the",
            "proposal be referred back for a second reading.",
        ],
        &[
            "the secretary wrote to every member of the standing committee",
            "before the end of the financial year.",
        ],
    ));
    assert!(!blocks[1].continues_previous, "{:?}", texts(&blocks));
    // The paragraph ends at the break, so the page marker of #572 stands in
    // the gap -- which is exactly the gap the joined case must not have.
    let markdown = to_markdown(&blocks);
    assert!(
        markdown.contains("second reading.\n\n<!-- page 1 -->\n\nthe secretary wrote"),
        "{markdown}"
    );
}

#[test]
fn a_capital_after_the_page_break_starts_one() {
    let blocks = blocks_of(&broken(
        &[
            "The committee met on the fourth of June and resolved that the",
            "proposal be referred back for a second reading before the",
        ],
        &[
            "Secretary of State wrote to every member of the standing",
            "committee before the end of the financial year.",
        ],
    ));
    assert!(!blocks[1].continues_previous, "{:?}", texts(&blocks));
}

#[test]
fn a_footnote_is_not_joined_to_the_next_page() {
    // The last thing on the page is a note at its foot, set smaller, and it
    // breaks off mid-sentence exactly as a body paragraph would. What keeps
    // it apart from the next page is the type it is set in.
    let mut first = body(&[
        "The committee met on the fourth of June and resolved.",
        "The matter was closed.",
    ]);
    first.push(small(
        "1 The note runs on past the foot of the page and stops at the",
        MARGIN,
        FOOT_Y,
    ));
    let pages = vec![
        portrait(first),
        portrait(body(&[
            "committee should be referred back for a second reading before",
            "the end of the financial year.",
        ])),
    ];
    let blocks = blocks_of(&pages);
    assert!(
        blocks.iter().all(|block| !block.continues_previous),
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_word_broken_across_the_page_break_is_put_back_together() {
    let blocks = blocks_of(&broken(
        &[
            "The committee met on the fourth of June and resolved that the",
            "proposal put before it raised a question that was consti-",
        ],
        &[
            "tutional rather than procedural and had to be referred back",
            "before the end of the financial year.",
        ],
    ));
    let markdown = to_markdown(&blocks);
    assert!(markdown.contains("constitutional rather"), "{markdown}");
}

#[test]
fn the_running_head_between_the_halves_does_not_stop_the_join() {
    // The composition of the two halves of this ticket: without the furniture
    // rule the running head of the second page stands between the two halves
    // of the sentence, and nothing can join them.
    let pages: Vec<Page> = [
        vec![
            "The committee met on the fourth of June and resolved that the",
            "proposal put before it by the secretary of the standing",
        ],
        vec![
            "committee should be referred back for a second reading before",
            "the end of the financial year.",
        ],
        vec![
            "A third page follows so that the head is repeated often enough.",
            "It says something else of its own.",
        ],
        vec![
            "A fourth page follows it, with the same head over it again.",
            "It says something else again.",
        ],
    ]
    .iter()
    .enumerate()
    .map(|(page, lines)| {
        let mut pieces = vec![at("Annual Report 2026", MARGIN, HEAD_Y)];
        pieces.extend(body(lines));
        pieces.push(at(&format!("Page {} of 4", page + 1), 290.0, FOOT_Y));
        portrait(pieces)
    })
    .collect();

    let blocks = blocks_of(&pages);
    let markdown = to_markdown(&blocks);
    assert!(
        markdown.contains("standing committee should be referred"),
        "{markdown}"
    );
}

#[test]
fn a_running_head_at_the_outer_margin_is_found_on_both_sides() {
    // A book sets its running head at the OUTER margin: left on every verso
    // and right on every recto. One text, two places, and a rule that kept
    // only the bigger of the two would leave half the pages' heads in the
    // body (codex, round 1).
    let pages: Vec<Page> = (0..6)
        .map(|page| {
            let x = if page % 2 == 0 { MARGIN } else { 380.0 };
            let mut pieces = vec![at("The Wealth of Nations", x, HEAD_Y)];
            pieces.extend(body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    let heads: Vec<&Block> = blocks
        .iter()
        .filter(|block| block.text.starts_with("The Wealth"))
        .collect();
    assert_eq!(heads.len(), 6, "{:?}", texts(&blocks));
    for head in heads {
        assert_eq!(head.kind, BlockKind::Header, "{:?}", head.text);
    }
}

#[test]
fn a_blank_page_between_the_halves_is_a_break() {
    // The sentence breaks off at the foot of the first page and the third
    // opens in lower case, but the page between them is blank -- and a blank
    // page in a book is a break, not a hyphenless continuation (codex,
    // round 1).
    let pages = vec![
        portrait(body(&[
            "The committee met on the fourth of June and resolved that the",
            "proposal put before it by the secretary of the standing",
        ])),
        portrait(Vec::new()),
        portrait(body(&[
            "committee should be referred back for a second reading before",
            "the end of the financial year.",
        ])),
    ];
    let blocks = blocks_of(&pages);
    assert!(
        blocks.iter().all(|block| !block.continues_previous),
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_compound_broken_at_the_page_break_keeps_its_hyphen() {
    // `consti-` before `tutional` is a word the typesetter broke; `x-` before
    // `ray` is a compound the page happened to break at the hyphen it already
    // had. The cross-page join answers that the way the join inside a page
    // does, and it is the same test (codex, round 1).
    let blocks = blocks_of(&broken(
        &["The committee met on the fourth of June and looked at an x-"],
        &[
            "ray of the joint before it referred the matter back for a",
            "second reading.",
        ],
    ));
    let markdown = to_markdown(&blocks);
    assert!(markdown.contains("x-"), "{markdown}");
    assert!(!markdown.contains("xray"), "{markdown}");
}

#[test]
fn three_words_spelt_in_roman_letters_are_not_page_numbers() {
    // `Civil`, `Mild` and `Livid` are spelt out of nothing but the seven
    // letters a roman numeral uses, and a membership test read all three as
    // the same `#` -- so three one-word headings at the same place on three
    // pages went out of the body and out of the markdown (codex, round 2).
    let pages: Vec<Page> = ["Civil", "Mild", "Livid", "Vivid"]
        .iter()
        .enumerate()
        .map(|(page, word)| {
            let mut pieces = vec![at(word, MARGIN, HEAD_Y)];
            pieces.extend(body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]));
            portrait(pieces)
        })
        .collect();
    let blocks = blocks_of(&pages);
    for block in &blocks {
        assert_ne!(block.kind, BlockKind::Header, "{:?}", block.text);
    }
    assert!(to_markdown(&blocks).contains("Civil"));
}

// ---------------------------------------------------------------------------
// One page's own evidence (#579).
// ---------------------------------------------------------------------------

/// A page whose folio stands in the outer corner beside a caption set across
/// the middle of the same horizontal strip.
///
/// The caption's box reaches lower than the folio's top edge, so a gap
/// measured against every other block on the page comes out NEGATIVE and the
/// folio is never a candidate at all -- the shape that left 66 of the 128
/// running heads the rule missed on the annotated sample of #579 in the body.
/// Here the caption is 200 points away across the page, and the folio has the
/// whole margin around it.
fn report_with_a_caption_beside_the_folio(pages: usize) -> Vec<Page> {
    (0..pages)
        .map(|page| {
            let mut pieces = body(&[
                &format!("The {page}th section opens here and says something."),
                "It runs on for a second line of its own body type.",
            ]);
            // Across the middle of the page, two lines of it, whose lower
            // line hangs to just below the TOP of the folio's own line -- the
            // shape an annual report's plate caption has beside its folio.
            pieces.push(small(
                "The barge at its moorings in Dampier,",
                260.0,
                FOOT_Y + 20.0,
            ));
            pieces.push(small(
                "photographed in March of that year.",
                260.0,
                FOOT_Y + 8.0,
            ));
            // The folio itself: out at the margin, alternating side by side.
            let x = if page % 2 == 0 { MARGIN } else { 520.0 };
            pieces.push(at(&format!("{}", page + 4), x, FOOT_Y));
            portrait(pieces)
        })
        .collect()
}

#[test]
fn a_folio_beside_a_caption_is_not_measured_against_the_caption() {
    // Six pages, because the folio alternates from margin to margin and each
    // side has to reach three before the repetition rule believes in it.
    let blocks = blocks_of(&report_with_a_caption_beside_the_folio(6));
    let folios: Vec<&Block> = blocks
        .iter()
        .filter(|block| matches!(block.text.as_str(), "4" | "5" | "6" | "7" | "8" | "9"))
        .collect();
    assert_eq!(folios.len(), 6, "{:?}", texts(&blocks));
    for folio in folios {
        assert_eq!(folio.kind, BlockKind::Footer, "{:?}", folio.text);
    }
    // And the caption beside them is body: the fix is about what the white is
    // measured against, not about taking the bottom band out wholesale.
    assert_eq!(
        kind_of(&blocks, "The barge at its moorings"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_page_number_at_the_foot_of_a_single_page_is_a_footer() {
    // No second page, so the repetition rule has nothing to say. A figure at
    // the foot of the page, standing clear of the body's measure with white
    // above it, is a page number on the evidence of the page it is on -- which
    // is the whole of what a corpus of loose pages can offer (#579).
    let mut pieces = body(&[
        "The section opens here and says something.",
        "It runs on for a second line of its own body type.",
    ]);
    pieces.push(at("Page 4 of 12", 290.0, FOOT_Y));
    let blocks = blocks_of(&[portrait(pieces)]);
    assert_eq!(
        kind_of(&blocks, "Page 4 of 12"),
        &BlockKind::Footer,
        "{:?}",
        texts(&blocks)
    );
    assert!(
        !to_markdown(&blocks).contains("Page 4 of 12"),
        "{}",
        to_markdown(&blocks)
    );
}

#[test]
fn a_footnote_at_the_foot_of_a_single_page_is_not_a_footer() {
    // The mutation that pays for the case above: a footnote opens with its
    // reference figure and sits in the same band. What separates the two on
    // one page is where the line starts -- a footnote is set in the body's own
    // measure and a folio stands clear of it. Take that clause out and this
    // footnote leaves the document.
    let mut pieces = body(&[
        "The section opens here and says something.",
        "It runs on for a second line of its own body type.",
    ]);
    pieces.push(small(
        "1 On the meaning of tenancy in this clause.",
        MARGIN,
        FOOT_Y,
    ));
    let blocks = blocks_of(&[portrait(pieces)]);
    assert_eq!(
        kind_of(&blocks, "1 On the meaning"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_line_of_prose_at_the_foot_of_a_single_page_is_not_a_footer() {
    // The other mutation: the per-page reading asks for a FIGURE, so a line
    // the page sets apart at the foot with no number in it stays body. Without
    // that clause every last paragraph of a page set off by white would go.
    let mut pieces = body(&[
        "The section opens here and says something.",
        "It runs on for a second line of its own body type.",
    ]);
    pieces.push(at("Continued on the following page", 290.0, FOOT_Y));
    let blocks = blocks_of(&[portrait(pieces)]);
    assert_eq!(
        kind_of(&blocks, "Continued on"),
        &BlockKind::Paragraph,
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_heading_over_its_own_white_at_the_top_of_a_single_page_is_body() {
    // Why the per-page reading is the FOOT and not both ends. The same signal
    // at the top of the page marks 26 blocks the ground truth calls body over
    // the 200 annotated pages of run structured-doclaynet-20260921T000905Z,
    // and takes the heading axis from 0.465 to 0.404: a section heading stands
    // over its own white exactly as a running head does.
    // CENTRED, so that the clause about the body's measure is not what saves
    // it: what saves it is that the per-page reading is the foot alone.
    let mut pieces = vec![at("24. REMUNERATION OF DIRECTORS 2000", 200.0, HEAD_Y)];
    pieces.extend(body(&[
        "The section opens here and says something.",
        "It runs on for a second line of its own body type.",
    ]));
    let blocks = blocks_of(&[portrait(pieces)]);
    assert_ne!(
        kind_of(&blocks, "24. REMUNERATION"),
        &BlockKind::Header,
        "{:?}",
        texts(&blocks)
    );
}

/// A running head set over TWO lines is one band (#584).
///
/// The shape the structured benchmark is full of: the company on one line and
/// what the statements are on the next, repeated at the top of every page of
/// a report. Both lines arrive as blocks of their own, and until the white was
/// measured after the repetition instead of before it the second line counted
/// as the page's body -- so the rail below the band was the leading between
/// the head's own two lines, nothing on the page was furniture, and both
/// halves came out as headings on all twelve pages of `NASDAQ_SHEN_2003.pdf`.
#[test]
fn a_running_head_of_two_lines_is_one_band() {
    let pages: Vec<Page> = (0..4)
        .map(|page| {
            // The second line set in from the first, which is how the page
            // this comes from centres it -- and which is what makes the two
            // lines two blocks instead of one.
            let mut pieces = vec![
                at(
                    "SHENANDOAH TELECOMMUNICATIONS COMPANY AND SUBSIDIARIES",
                    MARGIN,
                    TOP + 60.0,
                ),
                at(
                    "NOTES TO CONSOLIDATED FINANCIAL STATEMENTS",
                    MARGIN + 150.0,
                    TOP + 48.0,
                ),
            ];
            pieces.extend(body(&[
                &format!("The {page}th note opens here and says something of its own."),
                "It runs on for a second line of the body's own type and leading.",
                "And a third, so that the page has a body to stand apart from.",
            ]));
            portrait(pieces)
        })
        .collect();

    let blocks = blocks_of(&pages);
    let second: Vec<&BlockKind> = blocks
        .iter()
        .filter(|b| b.text.starts_with("NOTES TO CONSOLIDATED"))
        .map(|b| &b.kind)
        .collect();
    assert!(
        !second.is_empty() && second.iter().all(|kind| **kind == BlockKind::Header),
        "the second line of the running head is not furniture on every page it          stands on: {:?} in {:?}",
        second,
        texts(&blocks)
    );
    let first: Vec<&BlockKind> = blocks
        .iter()
        .filter(|b| {
            b.text
                .starts_with("SHENANDOAH TELECOMMUNICATIONS COMPANY AND SUBSIDIARIES")
        })
        .filter(|b| !b.text.contains("NOTES TO"))
        .map(|b| &b.kind)
        .collect();
    assert!(
        !first.is_empty() && first.iter().all(|kind| **kind == BlockKind::Header),
        "the first line of the running head is not furniture: {:?} in {:?}",
        first,
        texts(&blocks)
    );
}
