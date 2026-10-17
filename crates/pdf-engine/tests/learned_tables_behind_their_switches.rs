// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The two table models, each behind its own switch, and a page at a time
//! (#600).
//!
//! Every switch in the learned pass has an operator test that is RED with it
//! off -- otherwise the switch is decoration and nobody finds out until the
//! day it is turned on. These are those tests for `table_region` and
//! `table_structure`, on a real financial-report page, plus the same
//! concurrency guarantee the rest of the pass is held to: a page is decided
//! from its own blocks and the document's facts and nothing else, so pages run
//! in any order and at the same time and come out as they do one by one.

use pdf_engine::learned::tables::{refine_gated, Weakness};
use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{Block, BlockKind, PdfDocument};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
/// A PAGE WITH A REAL TABLE ON IT, and it has to be one the models ACT on or
/// the operator tests below test nothing. The DocLayNet marketing page that
/// this used first stopped qualifying the moment the region model learned to
/// leave columns of bullets alone -- which is the improvement it was asked
/// for, so the test moved rather than the model.
const PAGE: &str = "corpus/f1040.pdf";

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

fn both_on() -> Switches {
    Switches {
        table_region: true,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    }
}

/// THE OFF STATE, SAID EXPLICITLY AND NOT TAKEN FROM `SWITCHES`. These are
/// operator tests: what they compare is the pass with a switch and without it,
/// and reading the "without" from the shipped constant makes them depend on
/// what happens to ship. They passed, then failed on the day the shipped value
/// changed, having tested nothing in between.
fn both_off() -> Switches {
    Switches {
        table_region: false,
        table_structure: false,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    }
}

/// A builder that admits it had nothing to go on, so the hybrid gate lets the
/// model speak. The gate itself is tested in `the_hybrid_gate_on_a_weak_grid`;
/// these tests are about what the pass DOES once it is through, and with the
/// real record still to come from #624 they would otherwise all assert that a
/// stub returns `None`.
fn always_weak(_run: &[usize], _page: &[&Block]) -> Option<Weakness> {
    Some(Weakness {
        thin_rows: 2,
        fill: 0.31,
        retried_from: 1,
        full_rows: 0,
        columns: 5,
    })
}

/// The pass, with the gate open. `facts` is the document's.
fn regrid(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &pdf_engine::learned::DocumentFacts,
    switches: Switches,
) -> Vec<Block> {
    refine_gated(page, size, facts, switches, &always_weak).0
}

fn cells(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .count()
}

fn seen(blocks: &[Block]) -> Vec<String> {
    blocks
        .iter()
        .map(|b| format!("{:?}|{}|{:?}", b.kind, b.text, b.bbox))
        .collect()
}

#[test]
fn a_page_is_regridded_only_with_the_switches_on() {
    let (page, size) = page_of(PAGE);
    let facts = document_facts(&page);
    let off = regrid(page.clone(), size, &facts, both_off());
    let on = regrid(page.clone(), size, &facts, both_on());
    assert_ne!(
        seen(&off),
        seen(&on),
        "the table models change nothing on this page, so it tests nothing"
    );
    assert!(
        cells(&on) > 0,
        "the pass produced no table cells at all: {} blocks",
        on.len()
    );
}

/// THE REGION SWITCH ON ITS OWN. With `table_structure` on and `table_region`
/// off the pass works on the runs of cells the heuristics already found; with
/// both on it works on the model's regions. They must differ, or the region
/// model is doing nothing on a page built of tables.
#[test]
fn the_region_model_changes_which_blocks_a_table_is_made_of() {
    let (page, size) = page_of(PAGE);
    let facts = document_facts(&page);
    let ours = regrid(
        page.clone(),
        size,
        &facts,
        Switches {
            table_structure: true,
            table_judge: false,
            table_lattice: false,
            ..both_off()
        },
    );
    let theirs = regrid(page, size, &facts, both_on());
    assert_ne!(
        seen(&ours),
        seen(&theirs),
        "the region model picks exactly the blocks our heuristics do, on a \
         page of financial statements, which would be a first"
    );
}

/// THE STRUCTURE SWITCH ON ITS OWN. With `table_region` on and
/// `table_structure` off the pass finds the regions and leaves them alone, so
/// the page must come back as the heuristics left it.
#[test]
fn the_region_model_alone_changes_nothing() {
    let (page, size) = page_of(PAGE);
    let facts = document_facts(&page);
    let before = refine_page(page.clone(), size, &facts, both_off());
    let region_only = refine_page(
        page,
        size,
        &facts,
        Switches {
            table_region: true,
            ..both_off()
        },
    );
    assert_eq!(
        seen(&region_only),
        seen(&before),
        "finding a region is not an action; only the grid is"
    );
}

#[test]
fn table_pages_decided_at_the_same_time_come_out_as_they_do_one_by_one() {
    let (one, size) = page_of(PAGE);
    // Two pages of the same content, so the document's facts see each text
    // twice and a page that read anything but its own blocks would differ.
    let mut blocks = Vec::new();
    for number in 0..2usize {
        blocks.extend(one.iter().cloned().map(|mut b| {
            b.page = number;
            b
        }));
    }
    for (i, b) in blocks.iter_mut().enumerate() {
        b.index = i;
    }
    let facts = document_facts(&blocks);
    let pages: Vec<Vec<Block>> = (0..2).map(|n| page_blocks(&blocks, n)).collect();
    let sizes = [size, size];

    let serial: Vec<Vec<Block>> = pages
        .iter()
        .zip(&sizes)
        .map(|(page, size)| refine_page(page.clone(), *size, &facts, both_on()))
        .collect();

    let parallel: Vec<Vec<Block>> = std::thread::scope(|scope| {
        let running: Vec<_> = pages
            .iter()
            .zip(&sizes)
            .map(|(page, size)| {
                let facts = &facts;
                scope.spawn(move || refine_page(page.clone(), *size, facts, both_on()))
            })
            .collect();
        running
            .into_iter()
            .map(|t| t.join().expect("a page's thread panicked"))
            .collect()
    });

    let mut reversed: Vec<Vec<Block>> = pages
        .iter()
        .zip(&sizes)
        .rev()
        .map(|(page, size)| refine_page(page.clone(), *size, &facts, both_on()))
        .collect();
    reversed.reverse();

    for n in 0..2 {
        assert_ne!(
            seen(&serial[n]),
            seen(&pages[n]),
            "the models change nothing on page {n}, so it tests nothing"
        );
        assert_eq!(seen(&parallel[n]), seen(&serial[n]), "page {n}: parallel");
        assert_eq!(seen(&reversed[n]), seen(&serial[n]), "page {n}: reversed");
    }
    assert_eq!(
        seen(&serial[0]),
        seen(&serial[1]),
        "two copies of one page decided differently"
    );
}

/// THE REGION MODEL READS `repeats_in_document`, AND IT IS THE ONE FEATURE A
/// PAGE CANNOT KNOW ON ITS OWN (#583, #600). It comes from `DocumentFacts`,
/// and `page_features` leaves it missing until something fills it. The model
/// was trained on a dump where it is a real count -- 3 for a running head, 1
/// for a line of body text -- so a table pass that did not fill it would feed
/// the model an input it never saw in training, which is the defect this
/// ticket spent its day on in the other lane. It shipped that way for an hour.
///
/// Two documents of the same page: in one the page stands alone, in the other
/// it stands twice, so every text repeats. If the pass reads the facts, the
/// two must differ somewhere; if it does not, they cannot.
#[test]
fn the_table_pass_reads_the_document_s_facts_and_not_a_hole() {
    let (one, size) = page_of(PAGE);
    // THE BUILDER'S TABLES TAKEN OUT (#574). A run that holds part of a table
    // the builder made may not rebuild it, and on this page every run the
    // region model draws cuts one -- so with the cells in place nothing is
    // rebuilt in either document and the two cannot differ, whatever the model
    // read. As paragraphs, every run is the region model's own and is rebuilt,
    // which is what lets this see the feature.
    let one: Vec<Block> = one
        .into_iter()
        .map(|mut b| {
            if matches!(b.kind, BlockKind::TableCell { .. }) {
                b.kind = BlockKind::Paragraph;
            }
            b
        })
        .collect();

    let alone = document_facts(&one);
    let single = regrid(one.clone(), size, &alone, both_on());

    let mut twice = Vec::new();
    for number in 0..2usize {
        twice.extend(one.iter().cloned().map(|mut b| {
            b.page = number;
            b
        }));
    }
    for (i, b) in twice.iter_mut().enumerate() {
        b.index = i;
    }
    let doubled = document_facts(&twice);
    let head = &one[0].text;
    assert_eq!(
        alone.repeats(head),
        1,
        "the single document sees {head:?} once"
    );
    assert_eq!(
        doubled.repeats(head),
        2,
        "the doubled document sees {head:?} twice"
    );

    let repeated = regrid(one, size, &doubled, both_on());
    assert_ne!(
        seen(&single),
        seen(&repeated),
        "the same page decided identically whether its texts occur on one page \
         or two, so `repeats_in_document` is reaching the model as a hole"
    );
}
