// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The learned pass decides each page on its own (#599).
//!
//! A page reads its own blocks and the document's facts, which are computed
//! once and handed over behind a shared reference (`DocumentFacts`), and
//! nothing else: no cache, no state between pages. So pages can be run in
//! parallel later, and this holds that open: the same pages run serially, in
//! reverse, and on threads of their own at the same time come out the same,
//! and the same as the document pass the engine ships.
//!
//! The document is three pages built from two real ones on which the models
//! change something: the DocLayNet page whose bulleted columns the line model
//! splits, and the first page of the IRS form fw7. The DocLayNet page is there
//! twice, so every one of its texts repeats on two pages and the document's
//! facts are not the facts of one page.

use pdf_engine::learned::{document_facts, page_blocks, refine_page, refine_with, SWITCHES};
use pdf_engine::{Block, PdfDocument};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn page_of(pdf: &str) -> (Vec<Block>, (f64, f64)) {
    let doc =
        PdfDocument::open(std::fs::read(format!("{REPO}/{pdf}")).expect("pdf")).expect("open");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let blocks = doc.extract_blocks_before_learning().expect("blocks");
    (blocks.into_iter().filter(|b| b.page == 0).collect(), size)
}

/// Three pages, numbered as one document's.
fn document() -> (Vec<Block>, Vec<(f64, f64)>) {
    let (wwd, wwd_size) =
        page_of("crates/pdf-engine/tests/fixtures/doclaynet/nasdaq_wwd_2002_p7.pdf");
    let (form, form_size) = page_of("corpus/fw7.pdf");
    let mut blocks = Vec::new();
    for (number, page) in [wwd.clone(), form, wwd].into_iter().enumerate() {
        blocks.extend(page.into_iter().map(|mut b| {
            b.page = number;
            b
        }));
    }
    for (i, b) in blocks.iter_mut().enumerate() {
        b.index = i;
    }
    (blocks, vec![wwd_size, form_size, wwd_size])
}

fn seen(blocks: &[Block]) -> Vec<String> {
    blocks.iter().map(|b| format!("{b:?}")).collect()
}

#[test]
fn pages_decided_at_the_same_time_come_out_as_they_do_one_by_one() {
    let (blocks, sizes) = document();
    let facts = document_facts(&blocks);
    let heading = &blocks[0].text;
    assert_eq!(
        facts.repeats(heading),
        2,
        "the document's facts do not see both copies of {heading:?}"
    );
    let pages: Vec<Vec<Block>> = (0..sizes.len()).map(|n| page_blocks(&blocks, n)).collect();

    let serial: Vec<Vec<Block>> = pages
        .iter()
        .zip(&sizes)
        .map(|(page, size)| refine_page(page.clone(), *size, &facts, SWITCHES))
        .collect();

    let mut reversed: Vec<Vec<Block>> = pages
        .iter()
        .zip(&sizes)
        .rev()
        .map(|(page, size)| refine_page(page.clone(), *size, &facts, SWITCHES))
        .collect();
    reversed.reverse();

    let parallel: Vec<Vec<Block>> = std::thread::scope(|scope| {
        let running: Vec<_> = pages
            .iter()
            .zip(&sizes)
            .map(|(page, size)| {
                let facts = &facts;
                scope.spawn(move || refine_page(page.clone(), *size, facts, SWITCHES))
            })
            .collect();
        running
            .into_iter()
            .map(|t| t.join().expect("a page's thread panicked"))
            .collect()
    });

    for (n, page) in pages.iter().enumerate() {
        assert_ne!(
            seen(&serial[n]),
            seen(page),
            "the models change nothing on page {n}, so it tests nothing"
        );
        assert_eq!(seen(&parallel[n]), seen(&serial[n]), "page {n}: parallel");
        assert_eq!(seen(&reversed[n]), seen(&serial[n]), "page {n}: reversed");
    }

    // And the document pass is the pages' passes: the same blocks, page by
    // page, whatever order the document keeps them in.
    let whole = refine_with(blocks, &sizes, SWITCHES);
    for (n, page) in serial.iter().enumerate() {
        let from_whole: Vec<String> = whole
            .iter()
            .filter(|b| b.page == n)
            .map(|b| format!("{:?}|{}", b.kind, b.text))
            .collect();
        let from_page: Vec<String> = page
            .iter()
            .map(|b| format!("{:?}|{}", b.kind, b.text))
            .collect();
        assert_eq!(from_whole, from_page, "page {n}: document pass");
    }
}

#[test]
fn the_document_facts_can_be_shared_between_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<pdf_engine::learned::DocumentFacts>();
}
