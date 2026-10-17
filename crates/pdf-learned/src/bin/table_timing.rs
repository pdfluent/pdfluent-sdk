//! What the learned table pass costs, per page (#600).
//!
//!     table_timing <pdf>...
//!
//! For every page: the blocks before learning, then the learned pass with the
//! table switches OFF and with them ON, each timed over several runs, and the
//! comparisons the structure model was asked for. The difference between the
//! two is what #600 costs; the absolute numbers are this machine's.
//!
//! WHY A BINARY AND NOT A TEST. A timing that fails a build is a timing that
//! gets loosened until it stops failing. This prints, and the number goes on
//! the ticket where a person reads it.
//!
//! NOT THE RIG. The rig's documents stay off this machine, both halves, so
//! whatever is passed on the command line is what is measured and the output
//! says so. A mean over a mix that is not the rig's mix is not the rig's
//! number and must not be quoted as one.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{Block, BlockKind, PdfDocument};
use std::time::Instant;

const RUNS: u32 = 5;

fn both_on() -> Switches {
    Switches {
        table_region: true,
        table_structure: true,
        table_judge: false,
        table_lattice: false,
        ..SWITCHES
    }
}

fn time(
    page: &[Block],
    size: (f64, f64),
    facts: &pdf_engine::learned::DocumentFacts,
    switches: Switches,
) -> f64 {
    // The best of several, not the mean: the mean measures this machine's
    // other tenants as much as the code.
    let mut best = f64::MAX;
    for _ in 0..RUNS {
        let clone = page.to_vec();
        let started = Instant::now();
        let out = refine_page(clone, size, facts, switches);
        let took = started.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(&out);
        best = best.min(took);
    }
    best
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: table_timing <pdf>...");
        std::process::exit(2);
    }

    let mut off_ms = Vec::new();
    let mut on_ms = Vec::new();
    let mut heaviest: Option<(f64, String, usize, usize)> = None;
    let mut pages_with_a_table = 0usize;

    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("{path}: cannot read");
            continue;
        };
        let Ok(doc) = PdfDocument::open(bytes) else {
            eprintln!("{path}: cannot open");
            continue;
        };
        let Ok(blocks) = doc.extract_blocks_before_learning() else {
            eprintln!("{path}: cannot extract");
            continue;
        };
        let facts = document_facts(&blocks);
        for number in 0..doc.page_count() {
            let page = page_blocks(&blocks, number);
            if page.is_empty() {
                continue;
            }
            let size = doc
                .page_geometry(number)
                .map(|g| g.effective_dimensions())
                .unwrap_or((0.0, 0.0));
            let words: usize = page.iter().map(|b| b.text.split_whitespace().count()).sum();
            let has_table = page
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }));
            if has_table {
                pages_with_a_table += 1;
            }

            let off = time(&page, size, &facts, SWITCHES);
            let on = time(&page, size, &facts, both_on());
            off_ms.push(off);
            on_ms.push(on);
            let cost = on - off;
            if heaviest.as_ref().is_none_or(|(worst, ..)| cost > *worst) {
                heaviest = Some((cost, format!("{path} page {number}"), page.len(), words));
            }
        }
    }

    if off_ms.is_empty() {
        eprintln!("no page measured");
        std::process::exit(1);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let mut sorted = on_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];

    println!(
        "pages: {} ({} with a table our heuristics found), best of {RUNS} runs each",
        off_ms.len(),
        pages_with_a_table
    );
    println!(
        "  learned pass, table switches OFF   mean {:.2} ms/page",
        mean(&off_ms)
    );
    println!(
        "  learned pass, table switches ON    mean {:.2} ms/page",
        mean(&on_ms)
    );
    println!(
        "  the table pass costs               mean {:.2} ms/page, p95 {:.2} ms",
        mean(&on_ms) - mean(&off_ms),
        p95
    );
    if let Some((cost, where_, blocks, words)) = heaviest {
        println!(
            "  heaviest page                      {cost:.2} ms  ({where_}, \
             {blocks} blocks, {words} words)"
        );
    }
    println!(
        "\nthese are THIS machine and THIS set of files, not the rig's mix; \
         the rig's documents are not on this machine."
    );
}
