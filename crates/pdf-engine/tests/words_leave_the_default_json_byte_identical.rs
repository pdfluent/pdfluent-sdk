//! The default JSON does not move for words, and the word list has its shape.
//!
//! `to_json` is pinned byte for byte by the serializer goldens; these hold the
//! other side of the same promise. The pinned files carry no word lines, one
//! real document's JSON is identical to a golden taken before words existed,
//! and `to_words_json_lines` writes one typed object per word -- the last of
//! which fails until W1b writes it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{to_json, ExtractOptions, PdfDocument};
use std::collections::HashSet;

fn golden_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn blocks_of(json: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(json)
        .expect("JSON parses")
        .get("blocks")
        .expect("blocks")
        .as_array()
        .expect("blocks array")
        .clone()
}

#[test]
fn the_json_goldens_carry_no_word_lines() {
    for name in ["report", "handbook", "tariff", "merged", "notice"] {
        let path = golden_dir().join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).expect("golden reads");
        let blocks = blocks_of(&text);
        assert!(!blocks.is_empty(), "{name} has no blocks");
        for block in &blocks {
            assert!(
                block.get("lines").is_none(),
                "{name}: the pinned JSON carries word lines"
            );
        }
    }
}

#[test]
fn to_json_of_a_real_document_is_byte_identical() {
    let pdf =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/structured.pdf");
    let bytes = std::fs::read(&pdf).expect("fixture reads");
    let document = PdfDocument::open(bytes).expect("open");
    let blocks = document
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(!blocks.is_empty(), "the fixture reads no blocks");
    let produced = to_json(&blocks);
    for block in &blocks_of(&produced) {
        assert!(
            block.get("lines").is_none(),
            "the default JSON carries word lines"
        );
    }
    let path = golden_dir().join("words_identity_structured.json");
    if std::env::var_os("PDFLUENT_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &produced).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("no golden at {}: {why}", path.display()));
    assert_eq!(
        produced, expected,
        "to_json has moved. Read the diff; regenerate with PDFLUENT_WRITE_GOLDEN=1 only once you \
         have decided the new output is right."
    );
}

/// One line of Courier at an exact origin: two words, one line, one block.
fn two_word_pdf() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let operations = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                72.into(),
                700.into(),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal("Hello world")]),
        Operation::new("ET", vec![]),
    ];
    let content = Content { operations }.encode().expect("encode content");

    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
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

#[test]
fn words_json_lines_shape_is_one_typed_object_per_word() {
    let document = PdfDocument::open(two_word_pdf()).expect("open");
    let words = document
        .extract_words(&ExtractOptions::default())
        .expect("words");
    assert!(!words.is_empty(), "no words came back at all");
    let lines = pdf_engine::to_words_json_lines(&words);
    assert!(lines.ends_with('\n'), "each line ends at a newline");
    let rows: Vec<&str> = lines.lines().collect();
    assert_eq!(rows.len(), words.len(), "one line per word");
    let keys: HashSet<&str> = ["page", "block", "line", "word", "text", "bbox", "dir"]
        .into_iter()
        .collect();
    for (row, word) in rows.iter().zip(words.iter()) {
        let value: serde_json::Value = serde_json::from_str(row).expect("a line parses");
        let object = value.as_object().expect("an object");
        assert_eq!(
            object.keys().map(String::as_str).collect::<HashSet<_>>(),
            keys,
            "exactly the seven word fields: {row}"
        );
        assert_eq!(value["text"], word.text);
        assert!(value["page"].is_number(), "{row}");
        assert!(value["block"].is_number(), "{row}");
        assert!(value["line"].is_number(), "{row}");
        assert!(value["word"].is_number(), "{row}");
        assert!(value["dir"].is_number(), "{row}");
        let bbox = value["bbox"].as_array().expect("a box");
        assert_eq!(bbox.len(), 4, "{row}");
        assert!(bbox.iter().all(|n| n.is_number()), "{row}");
    }
}
