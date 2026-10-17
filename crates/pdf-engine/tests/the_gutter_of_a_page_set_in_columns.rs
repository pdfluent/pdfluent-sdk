// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The gutter of a page set in columns, and what stands in it (#582).
//!
//! Every page here is built from operators, so what is measured is the whole
//! path: interpreter, spans, the XY-cut, the paragraph merge, serializer. The
//! cases are the shapes the crossing pages of run
//! `structured-doclaynet-20260922T081244Z` turned out to have -- a block of
//! ours carrying text from two truth blocks that stand side by side -- and the
//! shapes that must NOT be cut, which is where the rules here are paid for.
//!
//! WHAT THIS FILE REFUTED FIRST. The narrow gutter was the obvious suspect and
//! it is innocent: ten, twelve and sixteen points of white at ten-point type
//! all come back as two clean columns, so `min_gap` is not what those pages
//! ran into.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{ExtractOptions, PdfDocument};

struct Piece {
    text: String,
    x: f64,
    y: f64,
    size: f64,
    /// Letter spacing set with `Tc`, in points. Zero for ordinary setting.
    tracking: f64,
    /// Letter spacing set as a `TJ` adjustment between every pair instead,
    /// which is the other way a page can say the same thing.
    tj_tracking: bool,
    /// Draw the text as ONE `TJ` array, a glyph at a time, with this much
    /// adjustment between the letters of a word and a wider one at the word
    /// spaces -- which is how a real page draws a line, and the only way the
    /// glyphs land in one span where the rule under test can see them.
    tj_prose: bool,
    /// Set in Courier rather than Helvetica. A monospaced face gives every
    /// glyph the same advance, which is what lets a fixture place glyphs by
    /// hand without opening gaps of its own around the narrow letters.
    mono: bool,
}

fn p(text: &str, x: f64, y: f64, size: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        size,
        tracking: 0.0,
        tj_tracking: false,
        tj_prose: false,
        mono: false,
    }
}

fn pdf_of(pieces: &[Piece], w: f64, h: f64) -> Vec<u8> {
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if piece.mono {
                    b"F2".to_vec()
                } else {
                    b"F1".to_vec()
                }),
                piece.size.into(),
            ],
        ));
        operations.push(Operation::new("Tc", vec![piece.tracking.into()]));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        if piece.tj_prose {
            let mut run: Vec<Object> = Vec::new();
            for letter in piece.text.chars() {
                if letter == ' ' {
                    run.push(Object::Real(-280.0));
                    continue;
                }
                run.push(Object::String(
                    letter.to_string().into_bytes(),
                    StringFormat::Literal,
                ));
                // No adjustment between the letters of a word: they sit at
                // their own advances, so the gap there is exactly nothing --
                // which is the evidence the rule needs and the thing a filter
                // on positive gaps throws away.
            }
            operations.push(Operation::new("TJ", vec![Object::Array(run)]));
        } else if piece.tj_tracking {
            let mut run: Vec<Object> = Vec::new();
            for letter in piece.text.chars() {
                run.push(Object::String(
                    letter.to_string().into_bytes(),
                    StringFormat::Literal,
                ));
                run.push(Object::Real(-300.0));
            }
            operations.push(Operation::new("TJ", vec![Object::Array(run)]));
        } else {
            operations.push(Operation::new(
                "Tj",
                vec![Object::String(
                    piece.text.clone().into_bytes(),
                    StringFormat::Literal,
                )],
            ));
        }
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations }.encode().expect("encode");
    let mut doc = Document::with_version("1.7");
    let font = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Times-Roman".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));
    let mono = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font, "F2" => mono }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(pg)) = doc.get_object_mut(page_id) {
        pg.set("Parent", Object::Reference(pages_id));
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save");
    buf
}

/// Two columns of prose, `gutter` points of white between them.
fn two_columns(gutter: f64, size: f64, head: bool, line_numbers: bool) -> Vec<Piece> {
    columns_with_numbers(gutter, size, head, line_numbers, false)
}

/// The same page, and where `glued` the line number is drawn straight before
/// the right column's line on its row -- as an OCR text layer draws it, one
/// line object from the gutter to the right margin -- so that the figure and
/// the line land in ONE span.
fn columns_with_numbers(
    gutter: f64,
    size: f64,
    head: bool,
    line_numbers: bool,
    glued: bool,
) -> Vec<Piece> {
    let left_x = 77.0;
    let col_w = (531.0 - 77.0 - gutter) / 2.0;
    let right_x = left_x + col_w + gutter;
    let leading = size * 1.2;
    let top = 700.0;
    let mut pieces = Vec::new();
    if head {
        // Centred over the gutter, which is where a patent sets it.
        pieces.push(p("US 9,840,656 B2", 259.0, 740.0, size));
    }
    for i in 0..24 {
        let y = top - i as f64 * leading;
        pieces.push(p(
            &format!("left column line {i} of prose running the measure here"),
            left_x,
            y,
            size,
        ));
        // In the gutter itself and CENTRED in it, as a US patent prints them:
        // a couple of points of white either side, which is all the room a
        // twelve-point gutter has.
        let number = (line_numbers && i % 5 == 0 && i > 0).then(|| {
            let figure = format!("{i}");
            let number_size = size * 0.7;
            let width = figure.chars().count() as f64 * number_size * 0.5;
            let centre = left_x + col_w + gutter * 0.5;
            p(&figure, centre - width * 0.5, y, number_size)
        });
        let right = p(
            &format!("right column line {i} of prose running the measure too"),
            right_x,
            y,
            size,
        );
        match number {
            Some(number) if glued => {
                pieces.push(number);
                pieces.push(right);
            }
            Some(number) => {
                pieces.push(right);
                pieces.push(number);
            }
            None => pieces.push(right),
        }
    }
    pieces
}

/// A band of two column headings, well above the body: the shape of
/// `NYSE_MSA_2011` p6, where the two headings came back as one line.
fn headings_over_two_columns(gutter: f64, size: f64) -> Vec<Piece> {
    let left_x = 77.0;
    let col_w = (531.0 - 77.0 - gutter) / 2.0;
    let right_x = left_x + col_w + gutter;
    let mut pieces = vec![
        p(
            "The year after The Year of General Monitors",
            left_x,
            700.0,
            size,
        ),
        p("Well positioned for a great 2012", right_x, 700.0, size),
    ];
    let leading = size * 1.2;
    for i in 0..20 {
        let y = 660.0 - i as f64 * leading;
        pieces.push(p(
            &format!("left column line {i} of prose running the measure here"),
            left_x,
            y,
            size,
        ));
        pieces.push(p(
            &format!("right column line {i} of prose running the measure too"),
            right_x,
            y,
            size,
        ));
    }
    pieces
}

fn blocks_of(pieces: &[Piece]) -> Vec<pdf_engine::Block> {
    PdfDocument::open(pdf_of(pieces, 612.0, 792.0))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

fn texts(blocks: &[pdf_engine::Block]) -> Vec<&str> {
    blocks.iter().map(|b| b.text.as_str()).collect()
}

/// How many blocks carry text from BOTH columns.
fn crossing(blocks: &[pdf_engine::Block]) -> usize {
    blocks
        .iter()
        .filter(|b| b.text.contains("left column") && b.text.contains("right column"))
        .count()
}

#[test]
fn a_narrow_gutter_is_not_what_those_pages_ran_into() {
    // The refutation, kept as a case so that nobody spends another afternoon
    // on it: ten points of white at ten-point type is the tightest of the
    // three and it reads as two columns.
    for gutter in [10.0, 12.0, 16.0] {
        let blocks = blocks_of(&two_columns(gutter, 10.0, false, false));
        assert_eq!(crossing(&blocks), 0, "{gutter}pt: {:?}", texts(&blocks));
    }
}

#[test]
fn a_running_head_across_the_gutter_does_not_cost_the_columns() {
    // The other suspect, and also innocent: the head is a band of its own and
    // the horizontal cut takes it off before the columns are looked for.
    let blocks = blocks_of(&two_columns(12.0, 10.0, true, false));
    assert_eq!(crossing(&blocks), 0, "{:?}", texts(&blocks));
    assert!(
        blocks.iter().any(|b| b.text.starts_with("US 9,840,656")),
        "{:?}",
        texts(&blocks)
    );
}

/// THE COUNTED REMAINDER, pinned so that the next round can see it move
/// (#582). The two headings of a two-column page are a region of two spans --
/// a heading is one span -- so the cut's span count refuses them and they come
/// back as ONE block, while the body under them cuts correctly. This asserts
/// what the tree does today rather than what it should do, because cutting
/// them was built, measured and taken out again: over the 500-document
/// paragraph set the rule took the agreement from 86.137 to 85.751 and put two
/// more documents under 95 %, and with both runs required to be comparable in
/// length it still split `tracemonkey.pdf` -- the two-column paper this rig is
/// built around -- fourteen more times for 1.99 points. A region of one band
/// cannot see the columns under it, and that is what the evidence would have
/// to come from.
#[test]
fn two_column_headings_over_their_own_columns_are_still_one_block() {
    let blocks = blocks_of(&headings_over_two_columns(12.0, 10.0));
    assert!(
        blocks.iter().any(|b| b.text
            == "The year after The Year of General Monitors Well positioned for a great 2012"),
        "the headings were cut apart; if that is deliberate, the paragraph gate \
         set is what has to say it is worth it: {:?}",
        texts(&blocks)
    );
    // And the body below them is two columns, which is the half that works.
    assert_eq!(crossing(&blocks), 0, "{:?}", texts(&blocks));
}

#[test]
fn a_row_of_three_cells_is_not_two_columns() {
    // What the case above is paid for. A page set in columns has ONE gap wider
    // than every other in the band; a row of cells has several of a kind, and
    // a rule that cut at the widest would take every header row apart.
    let blocks = blocks_of(&[
        p("Name", 77.0, 700.0, 10.0),
        p("Value", 250.0, 700.0, 10.0),
        p("Date", 430.0, 700.0, 10.0),
    ]);
    assert_eq!(blocks.len(), 1, "{:?}", texts(&blocks));
    assert_eq!(blocks[0].text, "Name Value Date");
}

#[test]
fn line_numbers_printed_in_the_gutter_do_not_cost_the_columns() {
    // The shape the three patent pages of the sample have, and the whole of
    // what they lost. The scan asks for an x free of every span; a column of
    // figures standing in the gutter leaves none, at any depth.
    let blocks = blocks_of(&two_columns(12.0, 10.0, true, true));
    assert_eq!(crossing(&blocks), 0, "{:?}", texts(&blocks));
    // And they come out as their own block rather than inside a column.
    assert!(
        blocks.iter().any(
            |b| b.text.split_whitespace().all(|w| w.parse::<u32>().is_ok())
                && !b.text.trim().is_empty()
        ),
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_line_number_drawn_into_the_line_beside_it_does_not_cost_the_columns() {
    // The US patents of DocLayNet's validation half (45 of their 65 pages) and
    // the one document that carries the whole of the rig's text gap,
    // 9840656_AdobeOCR: the OCR layer draws each right-column line as one
    // object that opens in the gutter with its line number, so `10 example,
    // aluminum and steel` is ONE span reaching from the gutter to the margin.
    // The margin scan looks past a span that is a line number, and this one is
    // a line number and a line -- so no x in the gutter was free and the page
    // was read across both columns (#1973).
    let blocks = blocks_of(&columns_with_numbers(12.0, 10.0, true, true, true));
    assert_eq!(crossing(&blocks), 0, "{:?}", texts(&blocks));
    // And the right column's lines keep their words: the figure is peeled
    // off, not the line's first word.
    assert!(
        blocks
            .iter()
            .any(|b| b.text.contains("right column line 10 of prose")),
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_bank_of_figures_is_read_row_by_row() {
    // What the rule above is paid for, and the shape #577 had just taken out
    // (T2, #580): every cell of a table of figures stands in white on both
    // sides, so the clearance clause alone would let the gap scan look past
    // them, find the boundary INSIDE the table, and cut it into two tables.
    //
    // AND THIS PAGE NEVER REACHES THE MARGIN SCAN, which the review turned up
    // when the missing assertion was added: with the clearance and the
    // one-column clauses both mutated open, this fixture still reads correctly,
    // because #577's own guards refuse the cut before `a_margin_column` is
    // called at all. The test was named for a clause it does not exercise. The
    // clause is pinned directly instead, on a bank fixture handed to
    // `a_margin_column` itself, by
    // `a_margin_column_is_one_thin_column_and_not_a_table` in the module.
    // What THIS test holds is the end-to-end invariant those guards exist for,
    // which is worth holding on its own.
    let rows = [
        ["12,345", "67,890", "23,456", "78,901"],
        ["23,456", "78,901", "34,567", "89,012"],
        ["34,567", "89,012", "45,678", "90,123"],
        ["45,678", "90,123", "56,789", "12,345"],
        ["56,789", "12,345", "67,890", "23,456"],
    ];
    let mut pieces = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        for (cell, x) in row.iter().zip([80.0, 175.0, 260.0, 350.0]) {
            pieces.push(p(cell, x, 700.0 - i as f64 * 14.0, 10.0));
        }
    }
    let blocks = blocks_of(&pieces);
    let read = texts(&blocks);

    // THE INVARIANT, ASSERTED AND NOT PRINTED (raised in review; this test
    // logged its answer and passed whatever the answer was). Two things have
    // to hold and each fails a different way:
    //
    // every cell is still there -- a cell taken out as a margin column would
    // be missing, and the count says so before the order does;
    let expected: Vec<&str> = rows.iter().flat_map(|row| row.iter().copied()).collect();
    assert_eq!(
        read.len(),
        expected.len(),
        "the bank lost or gained a cell: {read:?}"
    );
    // and the cells are read ROW BY ROW. This is the clause that catches the
    // failure the test is named for: a gap scan that looked past the cells
    // would find a boundary inside the table and cut it into column groups,
    // and the same twenty cells would then come back column by column --
    // `12,345 23,456 34,567 ...` rather than `12,345 67,890 23,456 ...`.
    // Comparing the sequence rather than the set is what tells those apart,
    // because a bank of figures repeats its values.
    assert_eq!(
        read, expected,
        "the bank was read out of its rows: {read:?}"
    );
}

#[test]
fn a_bibliography_keeps_its_columns() {
    // The hanging figures of a bibliography are narrow, short and at one x on
    // every band -- a thin column by every test the margin rule applies, and
    // standing at the LEFT EDGE of a column rather than in the gutter. The
    // first reading of that rule refused the cut whenever it found a column
    // outside the gap, which took the columns off this page: `tracemonkey.pdf`
    // page 14 went from 100.0 to 74.5 on the reading-order axis while both
    // foreign readers stayed at 100.
    let left_x = 77.0;
    let gutter = 12.0;
    let col_w = (531.0 - 77.0 - gutter) / 2.0;
    let right_x = left_x + col_w + gutter;
    let mut pieces = Vec::new();
    for i in 0..20 {
        let y = 700.0 - i as f64 * 12.0;
        // The marker, then the entry, hanging from a common indent.
        pieces.push(p(&format!("[{}]", i + 1), left_x, y, 9.0));
        pieces.push(p(
            "left column line of a reference entry that runs on",
            left_x + 22.0,
            y,
            9.0,
        ));
        pieces.push(p(&format!("[{}]", i + 21), right_x, y, 9.0));
        pieces.push(p(
            "right column line of a reference entry running on",
            right_x + 22.0,
            y,
            9.0,
        ));
    }
    let blocks = blocks_of(&pieces);
    assert_eq!(
        blocks
            .iter()
            .filter(|b| b.text.contains("left column") && b.text.contains("right column"))
            .count(),
        0,
        "{:?}",
        texts(&blocks)
    );
}

// ---------------------------------------------------------------------------
// The typesetter's tracking is not a word space (#582, #583).
// ---------------------------------------------------------------------------

/// A page whose display line is set in letter-spaced capitals, with the
/// tracking expressed the two ways a PDF can express it, over a body of
/// ordinary prose that gives the page its median character width.
fn letter_spaced_heading(with_tj: bool) -> Vec<Piece> {
    let mut pieces = vec![Piece {
        tj_prose: false,
        mono: false,
        text: "DIRECTORS' REPORT".to_string(),
        x: 77.0,
        y: 720.0,
        size: 12.0,
        tracking: if with_tj { 0.0 } else { 3.6 },
        tj_tracking: with_tj,
    }];
    for i in 0..6 {
        pieces.push(p(
            "The directors present their report on the company and its controlled entities",
            77.0,
            690.0 - i as f64 * 12.0,
            10.0,
        ));
    }
    pieces
}

#[test]
fn letter_spaced_capitals_are_one_word() {
    // The consensus fires on every pair of a tracked line -- the TJ adjustment
    // is a full one and the geometric gap is far past a third of a character
    // -- so `DIRECTORS' REPORT` came back as `D I R E C TO R S ' R E P O R T`.
    // That is 47 of the 200 pages of run structured-doclaynet-20260922T081244Z
    // writing text the ground truth does not have, and 65 of 154 missed
    // headings on the heading axis (#583).
    for with_tj in [false, true] {
        let blocks = blocks_of(&letter_spaced_heading(with_tj));
        assert!(
            blocks.iter().any(|b| b.text == "DIRECTORS' REPORT"),
            "tracking expressed with {}: {:?}",
            if with_tj { "TJ" } else { "Tc" },
            texts(&blocks)
        );
    }
}

/// Prose whose words are drawn ONE PIECE AT A TIME, which is what a real page
/// does and what the case below could not see.
fn prose_drawn_word_by_word(y: f64, size: f64) -> Vec<Piece> {
    // ONE TEXT OBJECT, a glyph at a time, which is how a real page draws a
    // line and the only way the glyphs land in ONE span -- a fixture that
    // gives every glyph its own `BT`/`ET` starts a new span each time, and the
    // rule under test is never asked at all. Courier, so that every advance is
    // the same and a narrow letter cannot open a gap of its own.
    vec![Piece {
        text: "presence of a hyperactive behavior with the Strengths and \
               Difficulties Questionnaire in a healthy child population"
            .to_string(),
        x: 77.0,
        y,
        size,
        tracking: 0.0,
        tj_tracking: false,
        tj_prose: true,
        mono: true,
    }]
}

#[test]
fn a_page_that_draws_every_word_apart_keeps_its_word_spaces() {
    // THE CASE THE ONE BELOW COULD NOT SEE, and it cost a round. When a page
    // draws its whole line with one `Tj`, the word spaces are 0x20 characters
    // in the string and no gap is ever judged -- so the rule under test was
    // never asked at all. A real page draws a piece at a time, and then a
    // sentence is a series of roughly equal gaps that are all over the
    // word-space threshold, which is exactly what tracking looks like:
    // `PMC1184062-p0004` came back with `in a healthy` as `in ahealthy` and
    // `• media use` as `•media use` (#582).
    //
    // WHAT IT TAKES TO ASK THE QUESTION AT ALL, because three fixtures could
    // not. A line drawn with one `Tj` keeps its word spaces as 0x20 in the
    // string and no gap is judged. A glyph-per-`BT` fixture starts a new span
    // each time, and the run the rule reads is cleared before it is four
    // pairs long. A proportional face placed by hand opens gaps around the
    // narrow letters and `hyperactive` comes back as `hyperact i ve`. So:
    // Courier, one text object, one `TJ` array, and NO adjustment between the
    // letters of a word -- which makes the gaps inside the words exactly
    // nothing, and those are the gaps the fix stopped throwing away.
    let mut pieces = prose_drawn_word_by_word(720.0, 10.0);
    for i in 0..6 {
        pieces.extend(prose_drawn_word_by_word(690.0 - i as f64 * 12.0, 10.0));
    }
    let blocks = blocks_of(&pieces);
    // Joined, because where the line comes apart into blocks is the paragraph
    // merge's business and not this rule's.
    let text = texts(&blocks).join(" ");
    assert!(
        text.contains("in a healthy") && text.contains("presence of a hyperactive"),
        "a word space was read as the typesetter's tracking: {text:?}"
    );
}

#[test]
fn prose_keeps_its_word_spaces() {
    // What the rule above is paid for. Four consecutive gaps that agree within
    // a quarter while every one of them is over the word-space threshold is
    // what tracking looks like; a sentence never offers it, and this is the
    // case that says so.
    let blocks = blocks_of(&[p(
        "The directors present their report on the company and its controlled entities",
        77.0,
        720.0,
        10.0,
    )]);
    assert_eq!(
        blocks[0].text,
        "The directors present their report on the company and its controlled entities",
        "{:?}",
        texts(&blocks)
    );
}

#[test]
fn a_tracked_word_between_ordinary_words_keeps_both() {
    // The case that can turn the rule over (T3, #583): a table head whose
    // middle word is set in tracked small capitals while the words either side
    // are set normally. A rule that took the run's AVERAGE gap would read the
    // whole head as tracked and join it into one word; this one asks about the
    // last four pairs, and the word space on each side of the tracked word is
    // wider than the tracking it stands beside.
    let mut pieces = vec![
        p("Revenue by", 77.0, 720.0, 10.0),
        Piece {
            text: "REGION".to_string(),
            x: 140.0,
            y: 720.0,
            size: 10.0,
            tracking: 3.0,
            tj_tracking: false,
            tj_prose: false,
            mono: false,
        },
        p("in thousands", 215.0, 720.0, 10.0),
    ];
    for i in 0..6 {
        pieces.push(p(
            "The directors present their report on the company and its controlled entities",
            77.0,
            690.0 - i as f64 * 12.0,
            10.0,
        ));
    }
    let blocks = blocks_of(&pieces);
    let head = blocks
        .iter()
        .find(|b| b.text.contains("Revenue"))
        .map(|b| b.text.as_str())
        .unwrap_or("");
    assert!(
        head.contains("REGION"),
        "the tracked word came apart: {head:?}"
    );
    assert!(
        head.contains("Revenue by") && head.contains("in thousands"),
        "the words beside it lost their spaces: {head:?}"
    );
}

#[test]
fn a_new_line_is_not_judged_by_the_last_line_s_tracking() {
    // THE RUN BELONGS TO THE SPAN IT WAS MEASURED IN (#582, found in review).
    // The gaps that say "this is tracking" were recorded on the heading; a
    // fresh span that keeps them judges its FIRST gap against the previous
    // line's rhythm, and a line opening with a one-letter word then loses its
    // space -- `A report` as `Areport`. The two lines are set at the same size
    // on purpose, because that is what puts the word gap of the second inside
    // the tolerance of the first's tracking; at different sizes the defect
    // hides and the test says nothing.
    let mut pieces = vec![
        Piece {
            text: "DIRECTORS REPORT".to_string(),
            x: 77.0,
            y: 720.0,
            size: 12.0,
            tracking: 0.0,
            tj_tracking: true,
            tj_prose: false,
            mono: true,
        },
        Piece {
            text: "A report of the year and of the company".to_string(),
            x: 77.0,
            y: 700.0,
            size: 12.0,
            tracking: 0.0,
            tj_tracking: false,
            tj_prose: true,
            mono: true,
        },
    ];
    for i in 0..6 {
        pieces.push(Piece {
            text: "The directors present their report on the company".to_string(),
            x: 77.0,
            y: 676.0 - i as f64 * 14.0,
            size: 12.0,
            tracking: 0.0,
            tj_tracking: false,
            tj_prose: true,
            mono: true,
        });
    }
    let text = texts(&blocks_of(&pieces)).join(" ");
    assert!(
        text.contains("A report of the year"),
        "the new line was read under the heading's tracking: {text:?}"
    );
    assert!(
        text.contains("DIRECTORSREPORT") || text.contains("DIRECTORS REPORT"),
        "the heading itself stopped being read as tracked: {text:?}"
    );
}

#[test]
fn a_line_of_single_letter_words_reads_as_tracking_and_that_is_the_choice() {
    // A CHOSEN READING, NOT AN OVERSIGHT (#582, raised in review). A page that
    // draws `A B C D E F` as separate glyphs with equal gaps and no space
    // GLYPHS is, byte for byte, the same page as a display line set in
    // letter-spaced capitals. The evidence that separates them is not on the
    // page: both are a run of single characters at one constant advance, and
    // no measurement of this page can tell which the author meant.
    //
    // So the rule takes the reading that is far more common and that the
    // corpus measured -- letter-spacing, 47 of the 200 pages of run
    // `structured-doclaynet-20260922T081244Z` -- and pays for it on a line of
    // isolated single-letter words, which is rare. This test exists so the
    // price is written down: if it ever turns red, somebody has found the
    // discriminator, and the note above is what they have to argue with.
    //
    // It is NOT a licence to lose spaces generally: a real word space between
    // real words survives, which is what the two tests above hold.
    let mut pieces = vec![Piece {
        text: "A B C D E F".to_string(),
        x: 77.0,
        y: 720.0,
        size: 12.0,
        tracking: 0.0,
        tj_tracking: false,
        tj_prose: true,
        mono: true,
    }];
    for i in 0..6 {
        pieces.push(Piece {
            text: "The directors present their report on the company".to_string(),
            x: 77.0,
            y: 690.0 - i as f64 * 14.0,
            size: 12.0,
            tracking: 0.0,
            tj_tracking: false,
            tj_prose: true,
            mono: true,
        });
    }
    let text = texts(&blocks_of(&pieces)).join(" ");
    assert!(
        text.contains("ABCDEF"),
        "the reading changed; read the note above before changing this test: {text:?}"
    );
}

#[test]
fn a_five_letter_tracked_word_is_read_once_its_span_is_finished() {
    // THE BOUNDARY #582 PINNED HERE, AND WHERE IT MOVED (#1973). The gap
    // being judged is recorded AFTER it is judged, so a word of five glyphs
    // offers only three earlier gaps at its last pair and never reaches
    // `TRACKING_MIN_PAIRS`: `TITLE` came back as `T I T L E`. #582 left that
    // as a trade to be measured -- reaching five-letter headings widens the
    // single-letter-word ambiguity -- and #1973 measured it: asking #582's
    // question again of the FINISHED span (`retrack_a_finished_span`), on the
    // 6,446 DocLayNet validation pages, changes nine pages, every one of them a
    // tracked word read right (`P E N N` -> `PENN`, `O U T S TA N D IN G` ->
    // `OUTSTANDING`) and none of them a line of single-letter words. So a
    // five-letter tracked word now reads as one, and a six-letter one still
    // does as it did.
    let read = |word: &str| {
        let mut pieces = vec![Piece {
            text: word.to_string(),
            x: 77.0,
            y: 720.0,
            size: 12.0,
            tracking: 0.0,
            tj_tracking: true,
            tj_prose: false,
            mono: true,
        }];
        for i in 0..6 {
            pieces.push(Piece {
                text: "The directors present their report on the company".to_string(),
                x: 77.0,
                y: 690.0 - i as f64 * 14.0,
                size: 12.0,
                tracking: 0.0,
                tj_tracking: false,
                tj_prose: true,
                mono: true,
            });
        }
        texts(&blocks_of(&pieces)).join(" ")
    };
    assert!(
        read("TITLE").contains("TITLE"),
        "a five-letter tracked word is split again; the note on TRACKING_MIN_PAIRS \
         and `retrack_a_finished_span` are what to read first: {:?}",
        read("TITLE")
    );
    assert!(
        read("REGION").contains("REGION"),
        "six glyphs no longer reach the rule: {:?}",
        read("REGION")
    );
}

/// A piece in Courier, whose fixed advance places every glyph where the
/// fixture says.
fn mono(text: &str, x: f64, y: f64, size: f64) -> Piece {
    Piece {
        mono: true,
        ..p(text, x, y, size)
    }
}

/// A line of `n` characters, `label` first, padded with prose.
fn padded(label: &str, n: usize) -> String {
    let mut line = format!("{label} keeps running on across the whole measure here");
    line.truncate(n);
    line
}

/// Two columns of 8 pt Courier whose text is 21 pt apart, ragged at the
/// right, both columns' items marked with a `t` hung before their text and the
/// right column's hung 6 pt from the left column's measure, the space after
/// each marker drawn as a run of its own -- NYSE_MSA on the rig (#574).
fn columns_with_hung_markers() -> Vec<Piece> {
    let size = 8.0;
    let advance = 0.6 * size;
    let (left, right) = (60.0, 331.0);
    let measure = 52;
    let hang = |pieces: &mut Vec<Piece>, x: f64, y: f64| {
        pieces.push(mono("t", x, y, size));
        // Two spaces in the text face, as the page sets them: narrower than
        // an em, a word space and nothing more.
        pieces.push(p("  ", x + advance, y, size));
    };
    let mut pieces = Vec::new();
    for row in 0..14 {
        let y = 700.0 - 11.0 * f64::from(row);
        // Ragged: every third line of each column stops short.
        let n = if row % 3 == 2 { measure - 9 } else { measure };
        if row % 5 == 0 {
            hang(&mut pieces, left - 10.0, y);
        }
        pieces.push(mono(
            &padded(&format!("left column {row}"), n),
            left,
            y,
            size,
        ));
        if row % 4 == 1 {
            hang(&mut pieces, left + measure as f64 * advance + 6.4, y);
        }
        let m = if row % 3 == 0 { measure - 12 } else { measure };
        pieces.push(mono(
            &padded(&format!("right column {row}"), m),
            right,
            y,
            size,
        ));
    }
    pieces
}

#[test]
fn markers_hung_into_a_narrow_gutter_do_not_close_it() {
    // Counted as ink, the hung markers and their space runs leave 6 pt of
    // white where the columns' text leaves 21, and every line of the page was
    // read across both columns.
    let blocks = blocks_of(&columns_with_hung_markers());
    assert_eq!(crossing(&blocks), 0, "{:?}", texts(&blocks));
}
