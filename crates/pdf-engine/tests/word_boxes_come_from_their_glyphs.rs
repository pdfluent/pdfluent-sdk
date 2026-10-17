//! Word boxes come from the glyphs that drew them.
//!
//! Every word `extract_words` hands back carries the union of its own glyphs'
//! boxes, the word's index within its line, and the rotation its baseline was
//! drawn at. The boxes are built from the per-glyph geometry the text pass
//! kept -- never by slicing a span's box across its words -- so a word that
//! one glyph drew for two letters keeps that glyph's whole box, and a word
//! drawn turned keeps the envelope of its turned glyphs.
//!
//! Every case here is a page built from operators, so that what is tested is
//! the whole path: interpreter, spans, blocks, words. What each test refuses:
//!
//! - `words_come_in_reading_order_with_line_and_word_indices`: the order and
//!   the numbering. Without the word index there is nothing to number by.
//! - `a_single_line_numbers_its_words_from_zero`: the exact block, line and
//!   word numbers on one line, where no merge can move them.
//! - `each_word_box_lies_inside_its_span_box`: the containment. A box sliced
//!   from the span by character count still passes this; the tests below are
//!   the ones it fails.
//! - `joining_a_blocks_words_reproduces_its_text`: text and boxes agree word
//!   for word on every fixture in this file, whatever joins the spans needed.
//! - `a_ligature_word_keeps_its_glyph_box_whole`: one glyph declared U+FB03,
//!   folded to `ffi`, still covering the word it stands in.
//! - `one_glyph_box_covering_two_letters_stays_whole`: an `/ActualText` glyph
//!   decoding to two letters. The old character-to-glyph walk hands the
//!   second letter the next glyph's box, so `fi` comes back one advance too
//!   wide and this fails.
//! - `a_ninety_degree_run_reads_as_words_with_envelopes_and_direction`: the
//!   merged rotated shape. Each word keeps the axis-aligned envelope of its
//!   turned glyphs and carries 90 degrees.
//! - `tj_offsets_split_and_join_words`: a -300 TJ unit space parts two words
//!   and a -50 one does not, with the exact boxes of both.
//! - the `json_*` tests: `to_json` carries no word lines, `to_json_with_words`
//!   carries them, and every other byte is the same.
//! - `words_serialise_as_one_object_per_line`: the word-list output shape.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{Block, ExtractOptions, PdfDocument, Word};

const SIZE: f64 = 12.0;
const ADVANCE: f64 = 0.6 * SIZE;
const TOP: f64 = 700.0;

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

fn courier_bold(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier-Bold".to_vec()),
    })))
}

/// One run of text drawn at an angle in a single show operation.
fn run_ops(text: &str, degrees: f64, origin: (f64, f64)) -> Vec<Operation> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
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

/// One run drawn glyph by glyph, each with its own text matrix at an exact
/// origin. Word gaps are positioning, not space glyphs: `gaps` holds the
/// advances each space in the text stands for, one entry per space.
fn glyph_per_show_ops(
    text: &str,
    degrees: f64,
    origin: (f64, f64),
    gaps: &[f64],
) -> Vec<Operation> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let mut ops = Vec::new();
    let mut pos = 0.0;
    let mut gap = 0;
    for ch in text.chars() {
        if ch == ' ' {
            pos += gaps.get(gap).copied().unwrap_or(1.0);
            gap += 1;
            continue;
        }
        let x = origin.0 + cos * ADVANCE * pos;
        let y = origin.1 + sin * ADVANCE * pos;
        pos += 1.0;
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), SIZE.into()],
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

fn finish(doc: &mut Document, pages_ops: Vec<Vec<Operation>>) -> Vec<u8> {
    let font = courier(doc);
    finish_with_fonts(doc, pages_ops, &[("F1", font)])
}

fn finish_with_fonts(
    doc: &mut Document,
    pages_ops: Vec<Vec<Operation>>,
    fonts: &[(&str, Object)],
) -> Vec<u8> {
    let mut kids = Vec::new();
    for operations in pages_ops {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let mut resources = dictionary! {};
        let mut font_dict = dictionary! {};
        for (name, font) in fonts {
            font_dict.set(*name, font.clone());
        }
        resources.set("Font", Object::Dictionary(font_dict));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
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

const FIRST: &str = "The quick brown fox";
const SECOND: &str = "jumps over the lazy dog";

fn two_lines() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(FIRST, 0.0, (72.0, TOP));
    ops.extend(run_ops(SECOND, 0.0, (72.0, TOP - SIZE)));
    finish(&mut doc, vec![ops])
}

/// One page whose only word the stream writes as two `Tj` with a font switch
/// in between: "Hel" in roman, "lo" in bold, the pen running on untouched.
fn split_word_document() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let roman = courier(&mut doc);
    let bold = courier_bold(&mut doc);
    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
        Operation::new("Td", vec![72.0.into(), TOP.into()]),
        Operation::new("Tj", vec![Object::string_literal("Hel")]),
        Operation::new("Tf", vec![Object::Name(b"F2".to_vec()), SIZE.into()]),
        Operation::new("Tj", vec![Object::string_literal("lo")]),
        Operation::new("ET", vec![]),
    ];
    finish_with_fonts(&mut doc, vec![operations], &[("F1", roman), ("F2", bold)])
}

/// A one-page PDF that draws one glyph per character of `drawn`, left to
/// right, through a CID font whose `ToUnicode` map names that character.
/// The advances are explicit in `/W`, so every box below is arithmetic.
fn page_drawing(drawn: &str) -> Vec<u8> {
    let glyphs: Vec<char> = drawn.chars().collect();
    let advance = 12.0_f64;
    let font_size = 20.0_f64;

    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 20.into()]),
    ];
    let mut widths: Vec<Object> = Vec::new();
    let mut to_unicode = String::new();
    let mut x = 72.0_f64;
    for (index, ch) in glyphs.iter().enumerate() {
        let cid = (index + 1) as u16;
        ops.push(Operation::new(
            "Tm",
            vec![1.into(), 0.into(), 0.into(), 1.into(), x.into(), 700.into()],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                cid.to_be_bytes().to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )],
        ));
        widths.push(Object::Integer(i64::from(cid)));
        widths.push(Object::Array(vec![Object::Integer(
            (advance / font_size * 1000.0) as i64,
        )]));
        to_unicode.push_str(&format!(
            "<{:04X}> <{}>\n",
            cid,
            ch.encode_utf16(&mut [0u16; 2])
                .iter()
                .map(|unit| format!("{unit:04X}"))
                .collect::<String>()
        ));
        x += advance;
    }
    ops.push(Operation::new("ET", vec![]));
    let content = Content { operations: ops }
        .encode()
        .expect("encode content");

    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapName /Custom def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {} beginbfchar\n{}endbfchar\nendcmap\n\
         CMapName currentdict /CMap defineresource pop\nend\nend",
        glyphs.len(),
        to_unicode
    );

    let mut doc = Document::with_version("1.7");
    let to_unicode_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"NotoSerif".to_vec()),
        "Flags" => Object::Integer(4),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-568), Object::Integer(-307),
            Object::Integer(2000), Object::Integer(1007),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(891),
        "Descent" => Object::Integer(-216),
        "CapHeight" => Object::Integer(693),
        "StemV" => Object::Integer(82),
    }));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(b"NotoSerif".to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "DW" => Object::Integer(1000),
        "W" => Object::Array(widths),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(b"NotoSerif".to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
        "ToUnicode" => Object::Reference(to_unicode_id),
    }));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
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

/// One glyph standing for two letters: the show of `A` inside an `/ActualText
/// (fi)` sequence, then a drawn space and `x`. Three glyphs, four characters.
fn actual_text_document() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
        Operation::new("Td", vec![72.0.into(), TOP.into()]),
        Operation::new(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Dictionary(dictionary! {
                    "ActualText" => Object::string_literal("fi"),
                }),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal("A")]),
        Operation::new("EMC", vec![]),
        Operation::new("Tj", vec![Object::string_literal(" ")]),
        Operation::new("Tj", vec![Object::string_literal("x")]),
        Operation::new("ET", vec![]),
    ];
    finish(&mut doc, vec![operations])
}

/// Two lines set with `TJ` offsets: a -300 unit space that parts two words
/// and a -50 unit kern that does not.
fn tj_document() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let tj = |parts: Vec<Object>| Operation::new("TJ", vec![Object::Array(parts)]);
    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
        Operation::new("Td", vec![72.0.into(), TOP.into()]),
        tj(vec![
            Object::string_literal("AA"),
            Object::Integer(-300),
            Object::string_literal("BB"),
        ]),
        Operation::new("ET", vec![]),
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
        Operation::new("Td", vec![72.0.into(), (TOP - 20.0).into()]),
        tj(vec![
            Object::string_literal("CC"),
            Object::Integer(-50),
            Object::string_literal("DD"),
        ]),
        Operation::new("ET", vec![]),
    ];
    finish(&mut doc, vec![operations])
}

/// The upright body a turned run stands on. Longer than the run, which is
/// what makes upright the direction of the page.
const UPRIGHT_LINE: &str =
    "The body of the page is written upright and holds more letters than anything turned";
const TURNED_LINE: &str = "Weighted average interest rate";

/// A 90-degree run, one glyph per show, whose middle word gap is wider than
/// the cut parts a line at -- so the run merges to one span and reads whole.
fn turned_document() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let mut ops = run_ops(UPRIGHT_LINE, 0.0, (72.0, TOP));
    ops.extend(glyph_per_show_ops(
        TURNED_LINE,
        90.0,
        (500.0, 300.0),
        &[1.0, 2.0, 1.0],
    ));
    finish(&mut doc, vec![ops])
}

fn blocks_and_words(bytes: Vec<u8>) -> (Vec<Block>, Vec<Word>) {
    blocks_and_words_with(&bytes, ExtractOptions::default())
}

fn blocks_and_words_with(bytes: &[u8], options: ExtractOptions) -> (Vec<Block>, Vec<Word>) {
    let document = PdfDocument::open(bytes.to_vec()).expect("open pdf");
    let blocks = document.extract_blocks(&options).expect("extract blocks");
    let words = document.extract_words(&options).expect("extract words");
    (blocks, words)
}

fn texts(words: &[Word]) -> Vec<&str> {
    words.iter().map(|w| w.text.as_str()).collect()
}

fn assert_box_near(actual: [f64; 4], expected: [f64; 4]) {
    // A ten-thousandth of a point: positions accumulate through the f32
    // metric path and decimal round-trips, so the last ulps are noise, but
    // every geometric claim below stands points away from its neighbours.
    for (a, e) in actual.iter().zip(expected.iter()) {
        assert!((a - e).abs() < 1e-4, "box {actual:?} is not {expected:?}");
    }
}

#[test]
fn words_come_in_reading_order_with_line_and_word_indices() {
    let (_, words) = blocks_and_words(two_lines());
    assert_eq!(
        texts(&words),
        vec!["The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog"],
        "words are not in reading order"
    );
    // Two line groups of four and five, whatever blocks the paragraph merge
    // reads them as; the word index restarts at each line.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for word in &words {
        let key = (word.block, word.line);
        if groups.last() != Some(&key) {
            groups.push(key);
        }
    }
    assert_eq!(groups.len(), 2, "the two lines did not stay two lines");
    let first: Vec<&Word> = words
        .iter()
        .filter(|w| (w.block, w.line) == groups[0])
        .collect();
    let second: Vec<&Word> = words
        .iter()
        .filter(|w| (w.block, w.line) == groups[1])
        .collect();
    assert_eq!(first.len(), 4);
    assert_eq!(second.len(), 5);
    assert_eq!(
        first.iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "the word index does not count within its line"
    );
    assert_eq!(
        second.iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
    for word in &words {
        assert_eq!(word.page, 0);
        assert_eq!(word.dir, 0.0, "upright words carry no rotation");
        assert!(
            (word.font_size - SIZE).abs() < 1e-6,
            "font size: {}",
            word.font_size
        );
    }
}

#[test]
fn a_single_line_numbers_its_words_from_zero() {
    let mut doc = Document::with_version("1.7");
    let bytes = finish(&mut doc, vec![run_ops(FIRST, 0.0, (72.0, TOP))]);
    let (blocks, words) = blocks_and_words(bytes);
    assert_eq!(blocks.len(), 1);
    assert_eq!(texts(&words), vec!["The", "quick", "brown", "fox"]);
    for (i, word) in words.iter().enumerate() {
        assert_eq!((word.block, word.line, word.word), (0, 0, i));
    }
}

#[test]
fn each_word_box_lies_inside_its_span_box_and_has_extent() {
    let (blocks, words) = blocks_and_words(two_lines());
    assert!(!words.is_empty(), "no words came back at all");
    for word in &words {
        assert!(
            word.bbox[2] > word.bbox[0] && word.bbox[3] > word.bbox[1],
            "the box of {:?} has no extent: {:?}",
            word.text,
            word.bbox
        );
        let block = &blocks[word.block];
        let holding = block.spans.iter().find(|span| {
            span.text.split_whitespace().any(|token| token == word.text)
                && word.bbox[0] >= span.x - 1e-6
                && word.bbox[1] >= span.y - 1e-6
                && word.bbox[2] <= span.x + span.width + 1e-6
                && word.bbox[3] <= span.y + span.height + 1e-6
        });
        assert!(
            holding.is_some(),
            "the box of {:?} lies outside every span holding it: {:?}",
            word.text,
            word.bbox
        );
    }
}

#[test]
fn joining_a_blocks_words_reproduces_its_text() {
    let options = ExtractOptions::default();
    for (name, bytes) in [
        ("two lines", two_lines()),
        ("split word", split_word_document()),
        ("ligature", page_drawing("o\u{FB03}ce hours")),
        ("actual text", actual_text_document()),
        ("tj offsets", tj_document()),
    ] {
        let (blocks, words) = blocks_and_words_with(&bytes, options.clone());
        for (index, block) in blocks.iter().enumerate() {
            let joined = words
                .iter()
                .filter(|w| w.block == index)
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let text = block.text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(
                joined, text,
                "{name}: block {index} reads {text:?} but its words read {joined:?}"
            );
        }
    }
}

#[test]
fn a_word_split_across_two_tj_with_a_font_switch_is_one_word() {
    let (blocks, words) = blocks_and_words(split_word_document());
    assert_eq!(texts(&words), vec!["Hello"]);
    assert_eq!(words[0].word, 0);
    assert_eq!(words[0].line, 0);
    assert_eq!(words[0].block, 0);
    let span_texts: Vec<&str> = blocks
        .iter()
        .flat_map(|block| block.spans.iter().map(|span| span.text.as_str()))
        .collect();
    assert_eq!(span_texts, vec!["Hel", "lo"]);
    // Courier at 12 points advances 7.2 a glyph: the joined box runs the
    // whole word, from the first glyph's origin to the last glyph's end.
    assert_box_near(words[0].bbox, [72.0, TOP, 72.0 + 5.0 * ADVANCE, TOP + SIZE]);
}

#[test]
fn a_ligature_word_keeps_its_glyph_box_whole() {
    let (blocks, words) = blocks_and_words(page_drawing("o\u{FB03}ce hours"));
    assert_eq!(texts(&words), vec!["office", "hours"]);
    // The fixture advances 12 points a glyph at size 20: `office` covers
    // four of them and `hours` five, starting after the drawn space.
    assert_box_near(words[0].bbox, [72.0, 700.0, 120.0, 720.0]);
    assert_box_near(words[1].bbox, [132.0, 700.0, 192.0, 720.0]);
    assert_eq!(words[0].word, 0);
    assert_eq!(words[1].word, 1);
    let span = blocks
        .iter()
        .flat_map(|block| block.spans.iter())
        .find(|span| span.text.contains("office"))
        .expect("a span holding the folded word");
    assert_eq!(
        span.char_bounds.len(),
        span.text.chars().count(),
        "the folded span carries one box per character"
    );
}

#[test]
fn one_glyph_box_covering_two_letters_stays_whole() {
    // The replacement reads only with `actual_text`: the default leaves the
    // drawn glyph, exactly as without the option (#663).
    let mut options = ExtractOptions::default();
    options.actual_text = true;
    let (blocks, words) = blocks_and_words_with(&actual_text_document(), options);
    assert_eq!(texts(&words), vec!["fi", "x"]);
    // Three glyphs drew four characters: the `fi` word keeps the first
    // glyph's box only, not the space glyph's box beside it.
    assert_box_near(words[0].bbox, [72.0, TOP, 72.0 + ADVANCE, TOP + SIZE]);
    assert_box_near(
        words[1].bbox,
        [72.0 + 2.0 * ADVANCE, TOP, 72.0 + 3.0 * ADVANCE, TOP + SIZE],
    );
    let span = &blocks[0].spans[0];
    assert_eq!(span.text, "fi x");
    assert_eq!(
        span.char_bounds.len(),
        3,
        "three glyphs drew this span: {:?}",
        span.char_bounds
    );
    assert_eq!(span.text.chars().count(), 4);
}

#[test]
fn a_ninety_degree_run_reads_as_words_with_envelopes_and_direction() {
    let mut options = ExtractOptions::default();
    options.learned = false;
    let (blocks, words) = blocks_and_words_with(&turned_document(), options);
    let block_texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(block_texts, vec![UPRIGHT_LINE, TURNED_LINE]);
    let turned: Vec<&Word> = words.iter().filter(|w| w.block == 1).collect();
    assert_eq!(
        turned.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
        vec!["Weighted", "average", "interest", "rate"]
    );
    // Each word keeps the axis-aligned envelope of its turned glyphs: twelve
    // points of height stand twelve points left of the baseline origin, and
    // each glyph advances 7.2 up the page from its explicit origin.
    let envelopes = [
        [488.0, 300.0, 500.0, 357.6],
        [488.0, 364.8, 500.0, 415.2],
        [488.0, 429.6, 500.0, 487.2],
        [488.0, 494.4, 500.0, 523.2],
    ];
    for (word, expected) in turned.iter().zip(envelopes) {
        assert_box_near(word.bbox, expected);
        assert!(
            (word.dir - 90.0).abs() < 1e-9,
            "the word carries its rotation: {}",
            word.dir
        );
        assert_eq!(word.line, 0);
        assert_eq!(word.page, 0);
    }
    assert_eq!(
        turned.iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(
        turned
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        TURNED_LINE,
        "the turned words join back to the turned line"
    );
}

#[test]
fn tj_offsets_split_and_join_words() {
    let (_, words) = blocks_and_words(tj_document());
    assert_eq!(texts(&words), vec!["AA", "BB", "CCDD"]);
    // The -300 unit shift moves the pen 3.6 points right: past a word space.
    assert_box_near(words[0].bbox, [72.0, TOP, 72.0 + 2.0 * ADVANCE, TOP + SIZE]);
    assert_box_near(
        words[1].bbox,
        [
            72.0 + 2.0 * ADVANCE + 3.6,
            TOP,
            72.0 + 4.0 * ADVANCE + 3.6,
            TOP + SIZE,
        ],
    );
    assert_eq!(words[0].word, 0);
    assert_eq!(words[1].word, 1);
    assert_eq!(
        (words[0].block, words[0].line),
        (words[1].block, words[1].line),
        "one span, one line"
    );
    // The -50 unit kern moves it 0.6: the four letters stay one word.
    assert_box_near(
        words[2].bbox,
        [
            72.0,
            TOP - 20.0,
            72.0 + 4.0 * ADVANCE + 0.6,
            TOP - 20.0 + SIZE,
        ],
    );
    assert_eq!(words[2].word, 0);
    assert_ne!(
        (words[2].block, words[2].line),
        (words[0].block, words[0].line),
        "the kerned line is its own line"
    );
}

#[test]
fn json_without_the_option_carries_no_word_lines() {
    let (blocks, _) = blocks_and_words(two_lines());
    let json = pdf_engine::to_json(&blocks);
    assert!(
        !json.contains("\"lines\""),
        "the default JSON carries word lines: {json}"
    );
    assert!(
        !json.contains("\"word\""),
        "the default JSON carries word entries: {json}"
    );
}

#[test]
fn json_with_words_carries_each_lines_words_and_nothing_else_changes() {
    let (blocks, _) = blocks_and_words(two_lines());
    let plain: serde_json::Value =
        serde_json::from_str(&pdf_engine::to_json(&blocks)).expect("plain JSON parses");
    let with_words = pdf_engine::to_json_with_words(&blocks);
    let mut worded: serde_json::Value =
        serde_json::from_str(&with_words).expect("words JSON parses");
    let blocks_json = worded["blocks"].as_array().expect("blocks").clone();
    assert!(
        blocks_json.iter().all(|b| b.get("lines").is_some()),
        "every block carries its lines: {with_words}"
    );
    let lines: Vec<serde_json::Value> = blocks_json
        .iter()
        .flat_map(|b| b["lines"].as_array().expect("lines").iter().cloned())
        .collect();
    assert_eq!(lines.len(), 2, "one entry per line: {with_words}");
    let first: Vec<&str> = lines[0]["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|w| w["text"].as_str().expect("text"))
        .collect();
    let second: Vec<&str> = lines[1]["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|w| w["text"].as_str().expect("text"))
        .collect();
    assert_eq!(first, vec!["The", "quick", "brown", "fox"]);
    assert_eq!(second, vec!["jumps", "over", "the", "lazy", "dog"]);
    assert_eq!(lines[0]["words"][3]["word"], 3);
    assert_eq!(lines[1]["words"][0]["word"], 0);
    assert_eq!(lines[0]["words"][0]["dir"], 0.0);
    let bbox = lines[0]["words"][0]["bbox"]
        .as_array()
        .expect("a word carries its box");
    assert_eq!(bbox.len(), 4);
    assert!(
        (bbox[0].as_f64().unwrap() - 72.0).abs() < 1e-6,
        "the JSON word box is the glyph box: {bbox:?}"
    );
    assert!(
        lines[0].get("spans").is_some(),
        "a line names the spans it groups: {with_words}"
    );
    // Every other byte is the contract the plain form already pins: strip
    // the added key and the two documents are the same.
    for block in worded["blocks"].as_array_mut().expect("blocks") {
        block.as_object_mut().expect("a block").remove("lines");
    }
    assert_eq!(worded, plain);
}

#[test]
fn words_serialise_as_one_object_per_line() {
    let (_, words) = blocks_and_words(split_word_document());
    let lines = pdf_engine::to_words_json_lines(&words);
    assert!(lines.ends_with('\n'), "each line ends at a newline");
    let rows: Vec<&str> = lines.lines().collect();
    assert_eq!(rows.len(), 1);
    let row: serde_json::Value = serde_json::from_str(rows[0]).expect("a line parses");
    assert_eq!(row["page"], 0);
    assert_eq!(row["block"], 0);
    assert_eq!(row["line"], 0);
    assert_eq!(row["word"], 0);
    assert_eq!(row["text"], "Hello");
    assert_eq!(row["dir"], 0.0);
    let bbox = row["bbox"].as_array().expect("a box");
    assert_eq!(bbox.len(), 4);
    assert!((bbox[0].as_f64().unwrap() - 72.0).abs() < 1e-6);
}

#[test]
fn the_word_boxes_option_defaults_off_and_sets() {
    assert!(!ExtractOptions::default().word_boxes);
    let options = ExtractOptions::default().with_word_boxes(true);
    assert!(options.word_boxes);
    fn is_eq<T: Eq>() {}
    is_eq::<ExtractOptions>();
}
