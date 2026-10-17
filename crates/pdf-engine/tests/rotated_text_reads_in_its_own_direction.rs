//! Rotated text reads in its own direction, never backwards (#574).
//!
//! A minority run drawn turned on an upright page -- a table column head, a
//! cover band -- arrives as one span per glyph (one glyph per show operation),
//! so no run forms in the device. The page-space layout then orders the glyphs
//! by x/y: for a 90-degree run that is top to bottom, which is the run
//! backwards, and the table grid takes the stack for one-glyph cells.
//! `T R O P E R L A U N N A` where the page says `ANNUAL REPORT`.
//!
//! All five run through `extract_blocks` with the learned pass off (#599: a
//! test of the heuristic), on operator-built PDFs. The first three are red
//! without the fix: the 90- and 270-degree lines come out split and reversed,
//! and the column heads come out as one-glyph cells. The fourth pins the
//! no-op: a slightly skewed line reads exactly as before. The fifth pins the
//! turned page: #546 owns those, and the merge never runs in its frame.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{Block, ExtractOptions, PdfDocument};

const FONT_SIZE: f64 = 12.0;
const LINE: &str = "Weighted average interest rate";

/// The upright body the page is written in. Longer than the turned run, which
/// is what makes upright the direction of the page: the turned run is the
/// minority, and the minority is what gets assembled along its own baseline.
const UPRIGHT_LINE: &str =
    "The body of the page is written upright and holds more letters than anything turned";

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

fn finish(doc: &mut Document, pages_ops: Vec<Vec<Operation>>) -> Vec<u8> {
    let font = courier(doc);
    let mut kids = Vec::new();
    for operations in pages_ops {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => font.clone() }),
            }),
        }));
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len();
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count as i64),
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

/// One run of text drawn at an angle, every glyph its own show operation with
/// its own text matrix, the way the producers of the defect's pages write a
/// turned run. Word gaps are positioning, not space glyphs: no show operation
/// draws the space. Courier advances 600/1000 of the font size per glyph, so
/// the origins step one advance along the baseline per character, plus the
/// space's own advances from `gaps` (one entry per space in the text).
///
/// The gaps vary on purpose, as positioned word spaces do: the middle gap of
/// the test line is wider than the paragraph-break scale the cut reads the
/// other two against, so the unassembled run is cut there -- the two-word
/// shape the defect's pages show -- while the fix still joins it.
fn glyph_per_show_ops(
    text: &str,
    degrees: f64,
    origin: (f64, f64),
    gaps: &[f64],
) -> Vec<Operation> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let advance = 0.6 * FONT_SIZE;
    let mut ops = Vec::new();
    let mut pos = 0.0;
    let mut gap = 0;
    for ch in text.chars() {
        if ch == ' ' {
            pos += gaps.get(gap).copied().unwrap_or(1.0);
            gap += 1;
            continue;
        }
        let x = origin.0 + cos * advance * pos;
        let y = origin.1 + sin * advance * pos;
        pos += 1.0;
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), FONT_SIZE.into()],
        ));
        ops.push(Operation::new(
            "Tm",
            vec![
                cos.into(),
                sin.into(),
                (-sin).into(),
                cos.into(),
                x.into(),
                y.into(),
            ],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::string_literal(ch.to_string())],
        ));
        ops.push(Operation::new("ET", vec![]));
    }
    ops
}

/// One run of text drawn at an angle in a single show operation.
fn run_ops(text: &str, degrees: f64, origin: (f64, f64)) -> Vec<Operation> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), FONT_SIZE.into()]),
        Operation::new(
            "Tm",
            vec![
                cos.into(),
                sin.into(),
                (-sin).into(),
                cos.into(),
                origin.0.into(),
                origin.1.into(),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal(text)]),
        Operation::new("ET", vec![]),
    ]
}

/// Like [`finish`], but the page carries a `/Rotate`: its content reaches
/// the device turned with it, so content drawn at 0 degrees reads 90 and
/// content drawn at 270 reads upright.
fn finish_rotated(doc: &mut Document, pages_ops: Vec<Vec<Operation>>, rotate: i64) -> Vec<u8> {
    let font = courier(doc);
    let mut kids = Vec::new();
    for operations in pages_ops {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Rotate" => Object::Integer(rotate),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => font.clone() }),
            }),
        }));
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len();
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(count as i64),
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

fn blocks_of(buf: Vec<u8>) -> Vec<Block> {
    let mut options = ExtractOptions::default();
    options.learned = false;
    PdfDocument::open(buf)
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks")
}

fn texts_of(buf: Vec<u8>) -> Vec<String> {
    blocks_of(buf).iter().map(|b| b.text.clone()).collect()
}

/// One page per quarter turn: the upright body and the line drawn at that
/// turn, every glyph its own show. Each page holds the identical string.
#[test]
fn the_same_line_reads_the_same_at_every_quarter_turn() {
    for (degrees, origin) in [
        (0.0_f64, (72.0, 600.0)),
        (90.0_f64, (500.0, 300.0)),
        (180.0_f64, (540.0, 400.0)),
        (270.0_f64, (100.0, 560.0)),
    ] {
        let mut doc = Document::with_version("1.7");
        let mut ops = run_ops(UPRIGHT_LINE, 0.0, (72.0, 700.0));
        ops.extend(glyph_per_show_ops(LINE, degrees, origin, &[1.0, 2.0, 1.0]));
        let texts = texts_of(finish(&mut doc, vec![ops]));
        assert!(
            texts.iter().any(|t| t == LINE),
            "at {degrees} degrees the line was split or reversed: {texts:?}"
        );
    }
}

/// The defect's own shape: a 90-degree line where every glyph is its own `Tj`.
/// The string comes out forwards, whole, in one block beside the upright one.
#[test]
fn a_rotated_run_drawn_one_glyph_per_show_reads_forwards() {
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(UPRIGHT_LINE, 0.0, (72.0, 700.0));
    ops.extend(glyph_per_show_ops(
        LINE,
        90.0,
        (500.0, 300.0),
        &[1.0, 2.0, 1.0],
    ));
    let texts = texts_of(finish(&mut doc, vec![ops]));
    assert_eq!(
        texts,
        vec![UPRIGHT_LINE.to_string(), LINE.to_string()],
        "the glyph-per-show run was split or reversed: {texts:?}"
    );
}

/// A small upright table with 90-degree column heads, the p62/p63 shape. Each
/// head is one cell holding the whole word, read forwards; no cell holds a
/// single letter of a head.
#[test]
fn rotated_column_heads_over_an_upright_table_are_not_cells_of_letters() {
    let mut doc = Document::with_version("1.7");
    let mut ops = Vec::new();
    for (row, cells) in [
        ["2023", "2024", "2025"],
        ["100", "200", "300"],
        ["110", "210", "310"],
        ["120", "220", "320"],
    ]
    .iter()
    .enumerate()
    {
        for (column, cell) in cells.iter().enumerate() {
            ops.extend(run_ops(
                cell,
                0.0,
                (100.0 + column as f64 * 120.0, 500.0 - row as f64 * 20.0),
            ));
        }
    }
    for (column, head) in ["SALES", "COSTS", "TOTAL"].iter().enumerate() {
        ops.extend(glyph_per_show_ops(
            head,
            90.0,
            (100.0 + column as f64 * 120.0, 540.0),
            &[],
        ));
    }
    let blocks = blocks_of(finish(&mut doc, vec![ops]));
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    for head in ["SALES", "COSTS", "TOTAL"] {
        assert!(
            texts.contains(&head),
            "the head {head} was split or reversed: {texts:?}"
        );
    }
    let letters: Vec<&&str> = texts
        .iter()
        .filter(|t| t.chars().count() == 1 && t.chars().all(|c| c.is_alphabetic()))
        .collect();
    assert!(
        letters.is_empty(),
        "one-glyph cells survived the fix: {texts:?}"
    );
}

/// A 3-degree skew is below the 10-degree guard: the line reads whole and
/// forwards exactly as before the fix, one block holding the whole string.
#[test]
fn a_slightly_skewed_line_is_unchanged() {
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(UPRIGHT_LINE, 0.0, (72.0, 700.0));
    ops.extend(run_ops(
        "A slightly skewed line stays whole",
        3.0,
        (72.0, 600.0),
    ));
    let texts = texts_of(finish(&mut doc, vec![ops]));
    assert_eq!(
        texts,
        vec![
            UPRIGHT_LINE.to_string(),
            "A slightly skewed line stays whole".to_string()
        ],
        "a skewed line changed shape under the fix: {texts:?}"
    );
}

/// A 60-degree region is a free angle, not a quarter turn: the minority-run
/// assembly never touches it, so the page comes out exactly as before the
/// fix -- here the word gaps part the run and the pieces come back in page
/// order (#574: a 60-degree region on one proxy page was merged, spans
/// 197 -> 31, and the page lost text it used to hold).
#[test]
fn a_sixty_degree_region_is_left_as_it_was() {
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(UPRIGHT_LINE, 0.0, (72.0, 700.0));
    ops.extend(glyph_per_show_ops(
        LINE,
        60.0,
        (300.0, 300.0),
        &[1.0, 2.0, 1.0],
    ));
    let texts = texts_of(finish(&mut doc, vec![ops]));
    assert_eq!(
        texts,
        vec![
            UPRIGHT_LINE.to_string(),
            "rate".to_string(),
            "interest".to_string(),
            "average".to_string(),
            "Weighted".to_string(),
        ],
        "a free-angle region was re-grouped: {texts:?}"
    );
}

/// A turned page is #546's, not this merge's: in the frame the page is read
/// in, its own runs read upright and the minority reads turned. Merging that
/// minority would re-decide a landscape table around it, so the page comes
/// back as #546 leaves it -- here the minority's word gap parts it in two.
#[test]
fn a_turned_pages_minority_is_never_merged() {
    const FIRST: &str = "The first line of the body of this page";
    const SECOND: &str = "and the second line carries on beneath it";
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(FIRST, 0.0, (72.0, 700.0));
    ops.extend(run_ops(SECOND, 0.0, (72.0, 680.0)));
    // Drawn at 270 degrees of content, so it reaches the device upright: the
    // minority of a page the device reads turned. One glyph per show, with a
    // word gap on the paragraph-break scale between the halves.
    ops.extend(glyph_per_show_ops("AB CD", 270.0, (100.0, 600.0), &[2.0]));
    let texts = texts_of(finish_rotated(&mut doc, vec![ops], 90));
    assert!(
        texts.iter().any(|t| t == FIRST),
        "the turned page stopped reading: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == SECOND),
        "the turned page stopped reading: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "AB") && texts.iter().any(|t| t == "CD"),
        "the minority was merged in the turned frame: {texts:?}"
    );
}
