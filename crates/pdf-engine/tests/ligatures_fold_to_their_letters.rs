//! A typographic ligature comes out as the letters it stands for (#437, class
//! R-G of #389).
//!
//! Every document here draws one glyph and declares a ligature code point for
//! it through `ToUnicode` -- which is what a serif face does for `office`, and
//! what made our extracted text unsearchable: U+FB03 is not `f` `f` `i`, so a
//! reader, an index or a `grep` looking for the word never found it.
//!
//! What each test refuses:
//!
//! - `the_seven_latin_ligatures_come_out_as_letters` -- the line of
//!   copy_paste_ligatures.pdf. Removing the fold puts the seven presentation
//!   forms back and this fails on all of them.
//! - `a_ligature_inside_a_word_is_folded_where_it_sits` -- the fold happens in
//!   place, so the word stays one word and stays searchable.
//! - `armenian_ligatures_come_out_as_letters` -- the second range of the
//!   block; it fails on its own if only the Latin range is folded.
//! - `keep_ligatures_hands_back_the_code_points` -- the option. It fails if
//!   the fold is not optional, and equally if the option stops working, so it
//!   is the half of the claim the other three do not cover.
//! - `structured_output_folds_the_code_points` -- the fold lives on the spans
//!   every TEXT output assembles from, so blocks, their lines and their spans
//!   all hand back letters. Removing the span fold puts U+FB03 back in all of
//!   them.
//! - `structured_markdown_and_json_fold_a_ligature` -- the whole structured
//!   path on a real extraction: markdown and JSON contain `fi` and no U+FB01.
//! - `folded_spans_keep_one_box_per_character` -- the letters of a decomposed
//!   ligature split its glyph's box evenly, so character positions stay
//!   aligned with the folded text.
//! - `keep_ligatures_keeps_the_structured_code_points` -- the option covers
//!   the structured outputs too: with it on, blocks and markdown carry the
//!   file's own characters.
//! - `a_page_without_a_ligature_is_unchanged` -- the fold touches nothing
//!   else; NFKC over the same string would have moved three of these
//!   characters.
//! - `a_search_finds_the_page_a_ligature_was_copied_from` -- the needle is
//!   folded exactly when the haystack is, so the one query that most needs
//!   the fold is not the one query that can never match.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A one-page PDF that draws one glyph per character of `drawn`, left to
/// right, through a CID font whose `ToUnicode` map names that character.
///
/// This is how a ligature reaches an extractor: the content stream holds a
/// single code, and the map says the code means U+FB03. Nothing in the file
/// says `ffi`.
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
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
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
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
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

fn text_of(drawn: &str) -> String {
    PdfDocument::open(page_drawing(drawn))
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string()
}

/// The seven Latin ligatures, in the line copy_paste_ligatures.pdf draws them
/// in. `ﬅ` folds to `st` and not to NFKC's `ſt`: the long s is a presentation
/// form with the same problem the ligature has.
#[test]
fn the_seven_latin_ligatures_come_out_as_letters() {
    let drawn = "abcdef\u{FB00}\u{FB01}\u{FB02}\u{FB03}\u{FB04}\u{FB05}\u{FB06}ghijklmno";
    assert_eq!(text_of(drawn), "abcdeffffiflffifflststghijklmno");
}

/// The word the ticket is about. A search for `office` has to find it, which
/// is the whole claim in one assertion.
#[test]
fn a_ligature_inside_a_word_is_folded_where_it_sits() {
    let text = text_of("o\u{FB03}ce hours");
    assert_eq!(text, "office hours");
    assert!(
        text.contains("office"),
        "the word is not searchable: {text:?}"
    );
}

/// The other range of the block. Folding only U+FB00-FB06 leaves these five.
#[test]
fn armenian_ligatures_come_out_as_letters() {
    assert_eq!(
        text_of("\u{FB13}\u{FB14}\u{FB15}\u{FB16}\u{FB17}"),
        "\u{574}\u{576}\u{574}\u{565}\u{574}\u{56B}\u{57E}\u{576}\u{574}\u{56D}"
    );
}

/// The option: a caller who wants the file's own characters back gets them.
#[test]
fn keep_ligatures_hands_back_the_code_points() {
    let drawn = "o\u{FB03}ce";
    let mut doc = PdfDocument::open(page_drawing(drawn)).expect("open pdf");
    doc.set_keep_ligatures(true);
    assert_eq!(
        doc.extract_text(0).expect("extract text").trim(),
        drawn,
        "keep_ligatures did not hand back the ligature code point"
    );

    doc.set_keep_ligatures(false);
    assert_eq!(
        doc.extract_text(0).expect("extract text").trim(),
        "office",
        "the option did not turn back off"
    );
}

/// Every TEXT output assembles from spans, so every one of them folds: the
/// block's line, each span under it, and the plain text beside them. The
/// bindings read `TextBlock::text` verbatim for their per-block text, which is
/// what the line assertion below covers.
#[test]
fn structured_output_folds_the_code_points() {
    let doc = PdfDocument::open(page_drawing("o\u{FB03}ce")).expect("open pdf");
    let blocks = doc.extract_text_blocks(0).expect("extract blocks");
    let lines: String = blocks
        .iter()
        .map(|block| block.text())
        .collect::<Vec<_>>()
        .join("\n");
    let spans: String = blocks
        .iter()
        .flat_map(|block| block.spans.iter())
        .map(|span| span.text.as_str())
        .collect();
    assert!(
        lines.contains("office"),
        "the block lines kept the ligature code point: {lines:?}"
    );
    assert!(
        !lines.contains('\u{FB03}'),
        "the block lines kept the ligature code point: {lines:?}"
    );
    assert!(
        spans.contains("office"),
        "the spans kept the ligature code point: {spans:?}"
    );
    assert!(
        !spans.contains('\u{FB03}'),
        "the spans kept the ligature code point: {spans:?}"
    );
    assert_eq!(
        doc.extract_text(0).expect("extract text").trim(),
        "office",
        "the plain text of the same page should still be folded"
    );
}

/// The whole structured path on a real extraction. No fixture in the tree
/// draws a ligature glyph, so the document is the synthetic one above, whose
/// `ToUnicode` maps a glyph to U+FB01 -- which is how a ligature reaches an
/// extractor.
#[test]
fn structured_markdown_and_json_fold_a_ligature() {
    let doc = PdfDocument::open(page_drawing("The \u{FB01}lm runs long")).expect("open pdf");
    let options = ExtractOptions::default();
    let markdown = doc.extract_markdown(&options).expect("extract markdown");
    assert!(
        markdown.contains("fi"),
        "the markdown kept the ligature code point: {markdown:?}"
    );
    assert!(
        !markdown.contains('\u{FB01}'),
        "the markdown kept the ligature code point: {markdown:?}"
    );
    let blocks = doc.extract_blocks(&options).expect("extract blocks");
    let json = pdf_engine::to_json(&blocks);
    assert!(
        json.contains("fi"),
        "the JSON kept the ligature code point: {json:?}"
    );
    assert!(
        !json.contains('\u{FB01}'),
        "the JSON kept the ligature code point: {json:?}"
    );
    let plain = doc.extract_text(0).expect("extract text");
    assert!(
        plain.contains("fi"),
        "the plain text kept the ligature code point: {plain:?}"
    );
    assert!(
        !plain.contains('\u{FB01}'),
        "the plain text kept the ligature code point: {plain:?}"
    );
}

/// A decomposed ligature keeps its glyph's box for all of its letters: the
/// span comes out with one box per character, and the letters' boxes tile the
/// glyph's box evenly.
#[test]
fn folded_spans_keep_one_box_per_character() {
    let doc = PdfDocument::open(page_drawing("o\u{FB03}ce")).expect("open pdf");
    let blocks = doc.extract_text_blocks(0).expect("extract blocks");
    let span = blocks
        .iter()
        .flat_map(|block| block.spans.iter())
        .find(|span| span.text.contains("ffi"))
        .expect("a span holding the folded ligature");
    let chars = span.text.chars().count();
    assert_eq!(
        span.char_bounds.len(),
        chars,
        "boxes no longer align with the folded text: {:?}",
        span.char_bounds
    );
    // The three letters of the ligature sit side by side where the one glyph
    // was: each box starts where the last ended, evenly spaced.
    let ffi: Vec<[f64; 4]> = {
        let at = span.text.find("ffi").expect("ffi in span text");
        let first = span.text[..at].chars().count();
        span.char_bounds[first..first + 3].to_vec()
    };
    for pair in ffi.windows(2) {
        assert!(
            (pair[0][2] - pair[1][0]).abs() < 1e-6,
            "the letters do not tile the glyph box: {ffi:?}"
        );
        assert!(
            (pair[0][3] - pair[1][3]).abs() < 1e-9 && (pair[0][1] - pair[1][1]).abs() < 1e-9,
            "the letters do not share the glyph box height: {ffi:?}"
        );
    }
    let widths: Vec<f64> = ffi.iter().map(|b| b[2] - b[0]).collect();
    assert!(
        (widths[0] - widths[1]).abs() < 1e-6 && (widths[1] - widths[2]).abs() < 1e-6,
        "the split is not even: {ffi:?}"
    );
}

/// The option covers the structured outputs too: with it on, blocks and
/// markdown carry the file's own characters.
#[test]
fn keep_ligatures_keeps_the_structured_code_points() {
    let mut doc = PdfDocument::open(page_drawing("o\u{FB03}ce")).expect("open pdf");
    doc.set_keep_ligatures(true);
    let options = ExtractOptions::default();
    let spans: String = doc
        .extract_text_blocks(0)
        .expect("extract blocks")
        .iter()
        .flat_map(|block| block.spans.iter())
        .map(|span| span.text.as_str())
        .collect();
    assert!(
        spans.contains('\u{FB03}'),
        "the option did not reach the spans: {spans:?}"
    );
    let markdown = doc.extract_markdown(&options).expect("extract markdown");
    assert!(
        markdown.contains('\u{FB03}'),
        "the option did not reach the markdown: {markdown:?}"
    );
}

/// Everything the fold must not touch. NFKC rewrites the superscript, the
/// fraction and the full-width letter; the decision on #437 is that only
/// ligatures fold, because those three change what the document says.
#[test]
fn a_page_without_a_ligature_is_unchanged() {
    let drawn = "2\u{B3} \u{BD} \u{FF21}";
    assert_eq!(text_of(drawn), drawn);
}

/// A search is the reason the fold exists, and its query can still carry the
/// file's own characters: a word pasted out of a content stream, or out of an
/// extraction made with the keep option on, carries `ﬃ` into `search_text`.
/// The needle folds with the haystack.
#[test]
fn a_search_finds_the_page_a_ligature_was_copied_from() {
    let mut doc = PdfDocument::open(page_drawing("o\u{FB03}ce hours")).expect("open pdf");

    assert_eq!(
        doc.search_text("o\u{FB03}ce"),
        vec![0],
        "a query carrying the ligature found nothing on the page it came from"
    );
    assert_eq!(doc.search_text("office"), vec![0]);

    // With the option on, both sides carry the code points and both still meet.
    doc.set_keep_ligatures(true);
    assert_eq!(doc.search_text("o\u{FB03}ce"), vec![0]);
}
