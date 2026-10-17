//! Words through the facade, with their boxes and numbers.
//!
//! `PdfDocument::extract_words` hands back every word of the requested pages
//! with the box it was drawn in, and `Page::words` the words of one page.
//! Both read the same engine pass as `extract_blocks`, so the numbering they
//! carry -- block, line, word -- names positions in that answer.
//!
//! The fixture is built in-memory via `lopdf`, two pages of plain Helvetica
//! that tell their words apart. What each test refuses:
//!
//! - `extract_words_hands_back_every_word_with_its_box`: the delegation. If
//!   the facade stops calling the engine pass, no words come back at all.
//! - `a_page_hands_back_its_own_words`: the page window. Words of the other
//!   page, or document-wide block numbers, fail it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::prelude::*;

fn build_two_page_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_ids = Vec::new();
    for text in ["Alpha on page one", "Beta on page two"] {
        let content = format!("BT\n/F1 12 Tf\n72 720 Td\n({text}) Tj\nET\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        page_ids.push(page_id);
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialise in-memory PDF");
    bytes
}

fn open() -> PdfDocument {
    PdfDocument::from_bytes(&build_two_page_pdf()).expect("open fixture")
}

#[test]
fn extract_words_hands_back_every_word_with_its_box() {
    let doc = open();
    let options = ExtractOptions::default();
    let blocks = doc.extract_blocks(&options).expect("extract blocks");
    let words = doc.extract_words(&options).expect("extract words");
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        texts,
        vec!["Alpha", "on", "page", "one", "Beta", "on", "page", "two"]
    );
    for word in &words {
        assert!(
            word.bbox[2] > word.bbox[0] && word.bbox[3] > word.bbox[1],
            "the box of {:?} has no extent: {:?}",
            word.text,
            word.bbox
        );
        assert_eq!(word.dir, 0.0, "upright words carry no rotation");
        assert_eq!(word.page, word.block, "one block per page here");
        assert_eq!(word.line, 0);
        assert!(word.word < 4, "the word index counts within its line");
        assert_eq!(word.page, blocks[word.block].page);
    }
    assert_eq!(
        words[..4].iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(
        words[4..].iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn a_page_hands_back_its_own_words() {
    let doc = open();
    let words = doc.page(1).expect("page one").words().expect("its words");
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, vec!["Alpha", "on", "page", "one"]);
    for word in &words {
        assert_eq!(word.page, 0);
        assert_eq!(word.block, 0, "blocks are numbered within the page");
        assert_eq!(word.line, 0);
    }
    assert_eq!(
        words.iter().map(|w| w.word).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    let words = doc.page(2).expect("page two").words().expect("its words");
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, vec!["Beta", "on", "page", "two"]);
    assert!(words.iter().all(|w| w.page == 1));
}
