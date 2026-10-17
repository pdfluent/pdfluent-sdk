// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Stacked figures in one cell are two rows (#574).
//!
//! DLR p61's body rows fuse two text lines into one row: the bands hold the
//! lines apart but the row-boundary model votes no boundary, `runs_on` vetoes
//! the split `split_stacked_rows` would make, and the markdown then reads the
//! fused figures as header. Two switches: B off by default, C on:
//!
//! * `table_runs_on_figures`: a complete-figure line never runs on from
//!   another, so the stacked rows come apart in the tidy.
//! * `markdown_header_block_figures`: cells of run-together figures count as
//!   figures for the header block, so fused rows read as body.
//!
//! The page is the defect's own: DLR p61, which fuses three body rows on the
//! shipped tree (gap 1.0 pt, model p 0.095). It is read through the whole
//! path -- interpreter, spans, cut, judge, decoder, tidy, serializer -- with
//! the switches given, so each on-test proves its switch and each off-test
//! pins what the switch changes. A synthetic pair cannot prove the switches:
//! the row-boundary model votes boundary on every operator-built tight pair
//! tried (p 0.104 at the closest), so the pair never fuses and the switches
//! have nothing to split; the synthetic shapes -- the two-column pair, the
//! wrapped label, the `December 31,` / `2004` header, the `(1)` footnote --
//! are proven shape by shape in the units instead, each red without its rule.

use pdf_engine::learned::{Switches, SWITCHES};
use pdf_engine::{to_markdown_with_switches, Block, BlockKind, ExtractOptions, PdfDocument};
use std::collections::BTreeMap;

/// DLR p61, the ParseBench tuning page whose body rows fuse. The copy under
/// `~/.local/share/pdfluent/pb-tuning/data/docs/table/` is the source.
fn dlr_blocks(switches: Switches) -> Vec<Block> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pb-tuning/stacked-figures-dlr-p61.pdf"
    ))
    .expect("fixture");
    PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks_with_switches(&ExtractOptions::default(), switches)
        .expect("blocks")
}

fn switches_off() -> Switches {
    Switches {
        table_runs_on_figures: false,
        markdown_header_block_figures: false,
        // Pinned, not inherited: this file isolates the figures switch, and
        // the header switches shipped in #636 decide DLR's head on their own
        // (mixed-body ends it at the nil row), which would hide C's effect.
        markdown_header_block_rowspan_crossing: false,
        markdown_header_block_word_table: false,
        markdown_header_block_mixed_body: false,
        markdown_headerless_html: false,
        markdown_header_block_stub_years: false,
        ..SWITCHES
    }
}

fn switches_b() -> Switches {
    Switches {
        table_runs_on_figures: true,
        ..SWITCHES
    }
}

fn switches_c() -> Switches {
    Switches {
        markdown_header_block_figures: true,
        markdown_header_block_rowspan_crossing: false,
        markdown_header_block_word_table: false,
        markdown_header_block_mixed_body: false,
        markdown_headerless_html: false,
        markdown_header_block_stub_years: false,
        ..SWITCHES
    }
}

/// Consecutive runs of table cells, each cell as `(row, column, text)`.
fn runs_of(blocks: &[Block]) -> Vec<Vec<(u16, u16, String)>> {
    let mut runs = Vec::new();
    let mut run = Vec::new();
    let mut page = 0;
    let mut in_run = false;
    for block in blocks {
        match &block.kind {
            BlockKind::TableCell { row, column, .. } if !in_run || block.page == page => {
                run.push((*row, *column, block.text.clone()));
                page = block.page;
                in_run = true;
            }
            _ => {
                if in_run {
                    runs.push(std::mem::take(&mut run));
                    in_run = false;
                }
            }
        }
    }
    if in_run {
        runs.push(run);
    }
    runs
}

/// Each row of `run` as its cells' texts in column order, joined.
fn signatures(run: &[(u16, u16, String)]) -> BTreeMap<u16, String> {
    let mut rows: BTreeMap<u16, Vec<(u16, String)>> = BTreeMap::new();
    for (row, column, text) in run {
        rows.entry(*row).or_default().push((*column, text.clone()));
    }
    rows.into_iter()
        .map(|(row, mut cells)| {
            cells.sort();
            let texts: Vec<&str> = cells.iter().map(|(_, text)| text.as_str()).collect();
            (row, texts.join(" | "))
        })
        .collect()
}

/// The run holding the fused pairs' stub.
fn pair_run(blocks: &[Block]) -> Vec<(u16, u16, String)> {
    runs_of(blocks)
        .into_iter()
        .find(|run| {
            run.iter()
                .any(|(_, _, text)| text.contains("Contributions"))
        })
        .expect("the run holding the pairs")
}

/// Whether the cell runs two figures together: a digit before the first
/// opening parenthesis, as in `86,090 (4,305)`.
fn is_fused_figures(text: &str) -> bool {
    text.find('(')
        .is_some_and(|at| text[..at].chars().any(|c| c.is_ascii_digit()))
}

/// The tight pairs fuse with the switches off: the control every on-test is
/// read against. Three body rows hold both labels, both figures run together
/// in two columns, and nil over nil in five.
#[test]
fn the_dlr_pairs_fuse_with_the_switches_off() {
    let blocks = dlr_blocks(switches_off());
    let run = pair_run(&blocks);
    let fused: Vec<u16> = run
        .iter()
        .filter(|(_, _, text)| text.contains("Contributions") && text.contains("Distributions"))
        .map(|(row, _, _)| *row)
        .collect();
    assert_eq!(fused.len(), 3, "three body rows fuse off");
    for row in fused {
        let figs: Vec<&str> = run
            .iter()
            .filter(|(r, _, text)| r == &row && is_fused_figures(text))
            .map(|(_, _, text)| text.as_str())
            .collect();
        assert_eq!(figs.len(), 2, "two fused figure columns in row {row}");
        let nils = run
            .iter()
            .filter(|(r, _, text)| r == &row && text == "— —")
            .count();
        assert_eq!(nils, 5, "five fused nil columns in row {row}");
    }
}

/// The three pairs split with B alone.
#[test]
fn stacked_figures_split_into_two_rows_under_b_alone() {
    assert_pairs_split(dlr_blocks(switches_b()), "B");
}

fn assert_pairs_split(blocks: Vec<Block>, name: &str) {
    assert!(
        !blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. })
                && b.text.contains("Contributions")
                && b.text.contains("Distributions")),
        "no fused pair under {name}"
    );
    let run = pair_run(&blocks);
    let ups: Vec<u16> = run
        .iter()
        .filter(|(_, _, text)| text.contains("Contributions"))
        .map(|(row, _, _)| *row)
        .collect();
    let downs: Vec<u16> = run
        .iter()
        .filter(|(_, _, text)| text.contains("Distributions"))
        .map(|(row, _, _)| *row)
        .collect();
    assert_eq!((ups.len(), downs.len()), (3, 3), "three pairs under {name}");
    let off = dlr_blocks(switches_off());
    let off_texts: std::collections::HashSet<&str> = off
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .collect();
    for (up, down) in ups.iter().zip(downs.iter()) {
        assert_eq!(
            down.checked_sub(*up),
            Some(1),
            "each pair in adjacent rows under {name}"
        );
        // Each column's halves run back together to a cell the off run
        // emitted: the split rows are the fused rows apart.
        let mut joined = 0;
        for column in 0..8 {
            let upper = run
                .iter()
                .find(|(r, c, _)| r == up && c == &column)
                .map(|(_, _, text)| text.as_str());
            let lower = run
                .iter()
                .find(|(r, c, _)| r == down && c == &column)
                .map(|(_, _, text)| text.as_str());
            if let (Some(upper), Some(lower)) = (upper, lower) {
                assert!(
                    off_texts.contains(format!("{upper} {lower}").as_str()),
                    "column {column} runs back together under {name}"
                );
                joined += 1;
            }
        }
        assert_eq!(joined, 8, "all eight columns split under {name}");
    }
}

/// Nothing but the three pairs moves under B alone.
#[test]
fn nothing_else_moves_under_b() {
    assert_only_pairs_move(dlr_blocks(switches_b()), "B");
}

fn assert_only_pairs_move(blocks: Vec<Block>, name: &str) {
    let off = dlr_blocks(switches_off());
    let plain = |blocks: &[Block]| -> Vec<(String, String)> {
        blocks
            .iter()
            .filter(|b| !matches!(b.kind, BlockKind::TableCell { .. }))
            .map(|b| (format!("{:?}", b.kind), b.text.clone()))
            .collect()
    };
    assert_eq!(
        plain(&blocks),
        plain(&off),
        "outside the tables nothing moves under {name}"
    );
    let holding = |blocks: &[Block]| {
        runs_of(blocks)
            .iter()
            .position(|run| {
                run.iter()
                    .any(|(_, _, text)| text.contains("Contributions"))
            })
            .expect("the run holding the pairs")
    };
    let (at, from) = (holding(&blocks), holding(&off));
    assert_eq!(at, from, "the pairs stay in one run under {name}");
    let (runs, off_runs) = (runs_of(&blocks), runs_of(&off));
    assert_eq!(runs.len(), off_runs.len(), "no table gained or lost");
    for (i, (run, off_run)) in runs.iter().zip(off_runs.iter()).enumerate() {
        if i == at {
            continue;
        }
        assert_eq!(run, off_run, "run {i} reads as off under {name}");
    }
    let mut sigs: Vec<String> = signatures(&runs[at]).values().cloned().collect();
    let mut off_sigs: Vec<String> = signatures(&off_runs[from]).values().cloned().collect();
    assert_eq!(
        sigs.len(),
        off_sigs.len() + 3,
        "three rows split in two under {name}"
    );
    let (kept, off_kept) = (sigs.len(), off_sigs.len());
    off_sigs.retain(|sig| !(sig.contains("Contributions") && sig.contains("Distributions")));
    sigs.retain(|sig| !sig.contains("Contributions") && !sig.contains("Distributions"));
    assert_eq!(off_kept - off_sigs.len(), 3, "three fused rows off");
    assert_eq!(kept - sigs.len(), 6, "six split rows under {name}");
    off_sigs.sort();
    sigs.sort();
    assert_eq!(sigs, off_sigs, "every other row reads as off under {name}");
}

/// The fused table opens with `<th>` body rows with C off, none under C:
// `86,090 (4,305)`, `— —`, `$—`, `$ —` and `$ 990` read as figures for the
/// header block, so only the head row stays header.
#[test]
fn fused_figure_rows_read_as_body_under_c() {
    let blocks = dlr_blocks(switches_off());
    let markdown = to_markdown_with_switches(&blocks, switches_off());
    for cell in [
        "<th>Contributions Distributions</th>",
        "<th>— —</th>",
        "<th>86,090 (4,305)</th>",
        "<th>$—</th>",
        "<th>$ —</th>",
        "<th>$ 990</th>",
    ] {
        assert!(
            markdown.contains(cell),
            "with C off the fused rows read as header ({cell:?}):\n{markdown}"
        );
    }
    let switched = to_markdown_with_switches(&blocks, switches_c());
    for cell in [
        "<th>Number of Common Shares</th>",
        "<th>Common Stock</th>",
        "<th>Additional Paid-in Capital</th>",
        "<th>Dividends In Excess of Earnings</th>",
        "<th>Accumulated Other Comprehensive Income, net</th>",
        "<th>Owner’s Equity</th>",
        "<th>Total</th>",
    ] {
        assert!(
            switched.contains(cell),
            "the head row stays header under C ({cell:?}):\n{switched}"
        );
    }
    for cell in [
        "<td>Contributions Distributions</td>",
        "<td>— —</td>",
        "<td>86,090 (4,305)</td>",
        "<td>$—</td>",
        "<td>$ 990</td>",
    ] {
        assert!(
            switched.contains(cell),
            "the fused rows read as body under C ({cell:?}):\n{switched}"
        );
    }
    assert!(
        !switched.contains("<th>Contributions"),
        "no header cell past the head under C:\n{switched}"
    );
    // And the shipped combination keeps C's fix: mixed-body ends the head at
    // the nil row on its own, so the fused rows stay body with all five on.
    let shipped = to_markdown_with_switches(&blocks, SWITCHES);
    assert!(
        shipped.contains("<td>Contributions Distributions</td>")
            && !shipped.contains("<th>Contributions"),
        "the fused rows read as body shipped:\n{shipped}"
    );
}
