// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! When the learned grid may overrule the heuristic one (#600).
//!
//! The owner's ruling: the heuristic builder leads, and the model overrules it
//! only where the builder is demonstrably weak. The model wins on TEDS over
//! PubTables -- 0.6432 against 0.6093 -- and loses 7 of the 23 tests in
//! `tables_from_columns_and_spans.rs`, which encode cases somebody measured
//! and fixed. A model that loses those is not better, it is differently wrong.
//!
//! These tests pin the GATE, not the thresholds. The three numbers in
//! `may_overrule` have nothing behind them yet and will be fitted against
//! those 23 tests and PubTables TEDS when T2's per-table record lands. What
//! must not move without somebody noticing is the SHAPE: all three conditions
//! together, and no record meaning no overruling.

use pdf_engine::learned::tables::{may_overrule, Weakness};

/// THE OFF STATE, SAID EXPLICITLY. Reading it from `SWITCHES` makes the test
/// depend on what happens to ship: the day the table switches went on, this
/// compared the shipped state with itself and reported that the gate had never
/// opened. Twice now, in two files.
fn both_off() -> pdf_engine::learned::Switches {
    pdf_engine::learned::Switches {
        table_region: false,
        table_structure: false,
        table_judge: false,
        table_lattice: false,
        ..pdf_engine::learned::SWITCHES
    }
}

/// A grid the builder had good evidence for: full rows showing the columns,
/// ink filling the rectangle, no thin row admitted, found first time.
fn strong() -> Weakness {
    Weakness {
        thin_rows: 0,
        fill: 0.92,
        retried_from: 0,
        full_rows: 4,
        columns: 3,
    }
}

/// The case the gate is for: a thin row let in, almost nothing showing where
/// the columns are, and a rectangle mostly empty.
fn weak() -> Weakness {
    Weakness {
        thin_rows: 2,
        fill: 0.31,
        retried_from: 1,
        full_rows: 0,
        columns: 5,
    }
}

#[test]
fn a_grid_the_builder_was_sure_of_is_not_overruled() {
    assert!(!may_overrule(&strong()));
}

#[test]
fn a_grid_the_builder_was_short_of_evidence_for_is_overruled() {
    assert!(may_overrule(&weak()));
}

/// BOTH, NOT EITHER. Each condition alone describes a table that is merely
/// unusual: a wide sparse table fills little of its rectangle, a two-row table
/// has few full rows, and a section head inside a table is a thin row. Only
/// together do they say the builder had nothing to go on -- and the difference
/// between `&&` and `||` here is the difference between overruling a handful
/// of tables and overruling most of them.
#[test]
fn one_weakness_alone_is_not_enough() {
    for change in [
        Weakness {
            fill: 0.31,
            ..strong()
        },
        Weakness {
            full_rows: 0,
            ..strong()
        },
    ] {
        assert!(
            !may_overrule(&change),
            "one weakness on its own let the model in: {change:?}"
        );
    }
}

/// And each condition must be able to VETO, or it is decoration.
#[test]
fn each_condition_can_veto_on_its_own() {
    for change in [
        Weakness {
            fill: 0.81,
            ..weak()
        },
        Weakness {
            full_rows: 3,
            ..weak()
        },
    ] {
        assert!(
            !may_overrule(&change),
            "this condition cannot refuse, so it is not a condition: {change:?}"
        );
    }
}

/// NO RECORD MEANS NO OVERRULING, which is what makes the gate safe on a page
/// the builder never made a table of. A run of blocks the region model
/// assembled out of prose has no `GridDoubt` on any of them, and `None` means
/// the builder said nothing rather than that it was confident -- so the model
/// stands down.
///
/// AND THE REAL RECORD DOES OPEN IT, or the assertion above would be about a
/// gate that can never fire. This test was written when `weakness_of` was a
/// stub returning `None`, and it then asserted that the whole pass was inert.
/// With T2's record on master (#1989) it is not inert: `corpus/f1040.pdf`
/// carries tables the builder doubted, and the model rebuilds them. That is
/// the hybrid working, so the test now says both halves.
#[test]
fn the_gate_opens_on_a_doubted_grid_and_not_on_a_silent_one() {
    use pdf_engine::learned::tables::refine_gated;
    use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
    use pdf_engine::{Block, PdfDocument};

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/f1040.pdf");
    let doc = PdfDocument::open(std::fs::read(path).expect("corpus/f1040.pdf")).expect("open");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let page = page_blocks(&all, 0);

    let both_on = Switches {
        table_region: true,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    };
    let seen = |blocks: &[Block]| -> Vec<String> {
        blocks
            .iter()
            .map(|b| format!("{:?}|{}", b.kind, b.text))
            .collect()
    };

    let off = refine_page(page.clone(), size, &facts, both_off());

    // A builder that says nothing: the model must stand down. Compared with
    // the page AS GIVEN and not with `off`, because `refine_gated` is the
    // table pass alone while `refine_page` also runs the list and heading
    // models -- comparing the two measures those, which is how this assertion
    // first failed.
    let silent = refine_gated(page.clone(), size, &facts, both_on, &|_, _| None).0;
    assert_eq!(
        seen(&silent),
        seen(&page),
        "the builder said nothing about these tables and the model touched \
         them anyway"
    );

    // The record as the builder actually wrote it: the gate opens somewhere on
    // this page, or the assertion above is about a gate that cannot fire.
    let real = refine_page(page, size, &facts, both_on);
    assert_ne!(
        seen(&real),
        seen(&off),
        "the gate never opened on a page of doubted tables, so the hybrid is \
         wired to nothing"
    );

    // AND IT IS THE GATE THAT OPENED IT, not the region model alone: with the
    // structure switch off there is no grid to rebuild and the page must come
    // back as the heuristics left it.
    let region_only = refine_page(
        page_blocks(&all, 0),
        size,
        &facts,
        Switches {
            table_region: true,
            ..both_off()
        },
    );
    assert_eq!(
        seen(&region_only),
        seen(&off),
        "finding a region is not an action; only the grid is"
    );
}

/// Rebuilding a table invents no headers and repeats no spans.
///
/// TWO THINGS A TEDS AVERAGE CANNOT SEE, both found by review (codex, #600).
/// The rebuilt cells took `header` from the row number, so every table the
/// gate opened got a header row whether or not anything on the page said so --
/// `th` in the HTML and `column_header: true` in Docling, for data. And each
/// cell took every span of every block its words came from, so a source block
/// the decoder split across two cells put ALL its glyphs in BOTH: the public
/// `spans` then disagreed with the cell's own text and box.
#[test]
fn a_rebuilt_table_invents_no_header_and_repeats_no_span() {
    use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
    use pdf_engine::{BlockKind, PdfDocument};

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/f1040.pdf");
    let doc = PdfDocument::open(std::fs::read(path).expect("corpus/f1040.pdf")).expect("open");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let page = page_blocks(&all, 0);

    let both_on = Switches {
        table_region: true,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    };
    let headers = |blocks: &[pdf_engine::Block]| -> usize {
        blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::TableCell { header: true, .. }))
            .count()
    };

    let before = headers(&page);
    let after = refine_page(page.clone(), size, &facts, both_on);
    assert!(
        headers(&after) <= before,
        "the pass made {} header cells out of {}: a header is evidence the \
         builder found, not the number of the row",
        headers(&after),
        before
    );

    // A span belongs to one cell. Identity is its text and its origin, which
    // is what a consumer reading the text layer sees twice when this breaks.
    let mut seen: Vec<(String, u64, u64)> = Vec::new();
    for block in after
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
    {
        for span in &block.spans {
            let key = (span.text.clone(), span.x.to_bits(), span.y.to_bits());
            assert!(
                !seen.contains(&key),
                "the span {:?} at ({}, {}) is in two table cells at once",
                span.text,
                span.x,
                span.y
            );
            seen.push(key);
        }
    }
}
