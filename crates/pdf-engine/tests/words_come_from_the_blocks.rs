//! Words come from the blocks, with the box each one was drawn in.
//!
//! `extract_words` is the `extract_words=True` of pymupdf4llm: every word of
//! the document with its box, page, block and line. It is built from the
//! blocks `extract_blocks` returns -- same options, same window -- splitting
//! each span on whitespace and taking the union of that word's `char_bounds`.
//!
//! Every case here is a page built from operators, so that what is tested is
//! the whole path -- interpreter, spans, blocks, words. Take the word pass
//! out and the first test goes red with no words where it asks for six;
//! box a word from anything but its glyphs and the containment and overlap
//! cases go red with it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{Block, ExtractOptions, PdfDocument, Word};

const SIZE: f64 = 10.0;
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;

fn helvetica(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })))
}

fn helvetica_bold(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica-Bold".to_vec()),
    })))
}

fn show(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), SIZE.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

fn content_of(lines: &[&str], top: f64) -> Vec<u8> {
    let mut operations = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        show(&mut operations, line, MARGIN, top - i as f64 * LEADING);
    }
    Content { operations }.encode().expect("encode content")
}

/// A PDF of one page per element, each page holding its lines.
fn document_of(pages: &[&[&str]]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = helvetica(&mut doc);
    let mut kids = Vec::new();
    let mut page_ids = Vec::new();
    for lines in pages {
        let content = content_of(lines, TOP);
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
        page_ids.push(page_id);
    }
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => Object::Integer(pages.len() as i64),
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

/// One page whose only word the stream writes as two `Tj` with a font switch
/// in between: "Hel" in roman, "lo" in bold, the pen running on untouched.
fn split_word_document() -> Vec<u8> {
    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), SIZE.into()]),
        Operation::new("Td", vec![MARGIN.into(), TOP.into()]),
        Operation::new("Tj", vec![Object::string_literal("Hel")]),
        Operation::new("Tf", vec![Object::Name(b"F2".to_vec()), SIZE.into()]),
        Operation::new("Tj", vec![Object::string_literal("lo")]),
        Operation::new("ET", vec![]),
    ];
    let content = Content { operations }.encode().expect("encode content");

    let mut doc = Document::with_version("1.7");
    let roman = helvetica(&mut doc);
    let bold = helvetica_bold(&mut doc);
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => roman, "F2" => bold }),
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

const FIRST: &str = "The quick brown fox";
const SECOND: &str = "jumps over the lazy dog";

fn two_lines() -> Vec<u8> {
    document_of(&[&[FIRST, SECOND]])
}

fn blocks_and_words(bytes: Vec<u8>) -> (Vec<Block>, Vec<Word>) {
    let document = PdfDocument::open(bytes).expect("open pdf");
    let options = ExtractOptions::default();
    let blocks = document.extract_blocks(&options).expect("extract blocks");
    let words = document.extract_words(&options).expect("extract words");
    (blocks, words)
}

fn texts(words: &[Word]) -> Vec<&str> {
    words.iter().map(|w| w.text.as_str()).collect()
}

#[test]
fn two_lines_come_back_as_words_in_order() {
    let (_, words) = blocks_and_words(two_lines());
    assert_eq!(
        texts(&words),
        vec!["The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog"],
        "words are not in reading order"
    );
    for word in &words {
        assert_eq!(word.page, 0);
        assert!(
            (word.font_size - SIZE).abs() < 1e-6,
            "font size: {}",
            word.font_size
        );
        assert!(
            !word.bold && !word.italic,
            "Helvetica is neither bold nor italic"
        );
    }
    // The first line's words share a line, the second line's share the next.
    let first_line: Vec<usize> = words[..4].iter().map(|w| w.line).collect();
    let second_line: Vec<usize> = words[4..].iter().map(|w| w.line).collect();
    assert!(first_line.iter().all(|l| *l == first_line[0]));
    assert!(second_line.iter().all(|l| *l == second_line[0]));
    assert_ne!(
        first_line[0], second_line[0],
        "the two lines share a line index"
    );
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
fn the_words_of_one_line_do_not_overlap() {
    let (_, words) = blocks_and_words(two_lines());
    let mut lines: std::collections::BTreeMap<(usize, usize), Vec<&Word>> =
        std::collections::BTreeMap::new();
    for word in &words {
        lines.entry((word.block, word.line)).or_default().push(word);
    }
    assert!(lines.len() >= 2, "the two lines did not stay two lines");
    for ((block, line), mut line_words) in lines {
        line_words.sort_by(|a, b| a.bbox[0].partial_cmp(&b.bbox[0]).unwrap());
        for pair in line_words.windows(2) {
            assert!(
                pair[0].bbox[2] <= pair[1].bbox[0] + 1e-6,
                "block {block} line {line}: {:?} {:?} overlaps {:?} {:?}",
                pair[0].text,
                pair[0].bbox,
                pair[1].text,
                pair[1].bbox
            );
        }
    }
}

#[test]
fn block_and_line_indices_agree_with_extract_blocks() {
    let (blocks, words) = blocks_and_words(two_lines());
    assert!(!blocks.is_empty() && !words.is_empty());
    for word in &words {
        assert!(
            word.block < blocks.len(),
            "block {} is past the {} blocks",
            word.block,
            blocks.len()
        );
        let block = &blocks[word.block];
        assert_eq!(
            word.page, block.page,
            "word {:?} names the wrong page",
            word.text
        );
        assert!(
            block
                .text
                .split_whitespace()
                .any(|token| token == word.text),
            "word {:?} is not in the text of block {}: {:?}",
            word.text,
            word.block,
            block.text
        );
    }
    // Every block that holds words is named by them, in order.
    let mut seen = Vec::new();
    for word in &words {
        if seen.last() != Some(&word.block) {
            seen.push(word.block);
        }
    }
    let mut ordered = seen.clone();
    ordered.sort_unstable();
    assert_eq!(seen, ordered, "words do not run block by block: {seen:?}");
}

#[test]
fn a_page_list_returns_only_those_pages_words() {
    let bytes = document_of(&[&["alpha beta gamma"], &["delta epsilon"]]);
    let document = PdfDocument::open(bytes).expect("open pdf");
    let mut options = ExtractOptions::default();
    options.pages = Some(vec![1]);
    let words = document.extract_words(&options).expect("extract words");
    assert_eq!(texts(&words), vec!["delta", "epsilon"]);
    assert!(words.iter().all(|w| w.page == 1));
    assert!(
        words.iter().all(|w| w.block == 0),
        "the window's block numbering: {words:?}"
    );

    let all = document
        .extract_words(&ExtractOptions::default())
        .expect("extract all words");
    assert_eq!(
        texts(&all),
        vec!["alpha", "beta", "gamma", "delta", "epsilon"]
    );
}

#[test]
fn a_word_split_across_two_tj_with_a_font_switch_is_one_word() {
    let (blocks, words) = blocks_and_words(split_word_document());
    assert_eq!(texts(&words), vec!["Hello"]);
    // The join did this: the page drew two spans, and take the join out and
    // this comes back as "Hel" and "lo".
    let span_texts: Vec<&str> = blocks
        .iter()
        .flat_map(|block| block.spans.iter().map(|span| span.text.as_str()))
        .collect();
    assert_eq!(span_texts, vec!["Hel", "lo"]);
    assert_eq!(words[0].block, 0);
    assert!(
        words[0].bbox[2] > words[0].bbox[0] && words[0].bbox[3] > words[0].bbox[1],
        "the joined box has no extent: {:?}",
        words[0].bbox
    );
}
