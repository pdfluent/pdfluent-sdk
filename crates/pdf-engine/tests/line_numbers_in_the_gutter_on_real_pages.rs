// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Line numbers in the gutter of a US patent, on the pages themselves (#1973).
//!
//! On the rig, one document carried the whole of the text gap to the reference:
//! 9840656_AdobeOCR, a US patent read across both of its columns, 0.340
//! against 0.886. On DocLayNet's validation half the same happens on 45 of the
//! 65 pages of US patents. `a_margin_column` (#582) looks past a line number
//! standing on its own in the gutter; these pages defeated it three ways, and
//! each is a rule in `crate::text`:
//!
//! - the OCR layer draws the number INTO the right column's line, one span
//!   from the gutter to the margin (`10 example, aluminum and steel`) --
//!   `peel_line_numbers`;
//! - a number beside the first line of a paragraph, where the left column's
//!   paragraph has just ended, has nothing before it -- the column takes it in;
//! - the running head, `US 7,268,176 B2`, is centred over the gutter and set
//!   too close to the body for a horizontal cut to take it off first.
//!
//! And three pages where the first versions of those rules broke a table,
//! kept so that they cannot again. All five are DocLayNet v1.2 VALIDATION
//! pages (registered in `scripts/ci/corpus_herkomst.py`), never test, which
//! the rig is drawn from.

use pdf_engine::{ExtractOptions, PdfDocument};

fn page(name: &str) -> PdfDocument {
    let path = format!(
        "{}/tests/fixtures/doclaynet/{name}.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open")
}

fn texts(name: &str) -> Vec<String> {
    page(name)
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks")
        .into_iter()
        .map(|b| b.text)
        .collect()
}

fn markdown(name: &str) -> String {
    page(name)
        .extract_markdown(&ExtractOptions::default())
        .expect("markdown")
}

/// Whether any block holds text from both sides of a pair.
fn together(blocks: &[String], left: &str, right: &str) -> bool {
    blocks.iter().any(|b| b.contains(left) && b.contains(right))
}

#[test]
fn a_patent_page_with_its_numbers_drawn_into_its_lines_reads_column_by_column() {
    // 7268176, page 9: the head over the gutter, `10 example, aluminum and
    // steel` drawn as one run, and `40` beside the first line of a paragraph
    // with the left column's paragraph already ended. Any one of the three
    // left in the way and the page is read across the gutter again.
    let blocks = texts("us_patent_7268176_p9");
    for (left, right) in [
        ("nepheline syenite", "film-forming resin"),
        ("commercially avail", "aluminum and steel substrates"),
        ("present additives, and the like", "anhydride curing agent"),
    ] {
        assert!(
            !together(&blocks, left, right),
            "`{left}` and `{right}` came back in one block: {blocks:?}"
        );
    }
    assert!(
        blocks
            .iter()
            .any(|b| b.contains("example, aluminum and steel substrates")),
        "the line the number was drawn into lost its words: {blocks:?}"
    );
}

#[test]
fn a_line_number_the_ocr_read_as_a_letter_is_still_one() {
    // 9017715, page 26: `hormone- s dependent` -- the `5` in the gutter came
    // back from the OCR as an `s`. A column that is figures in every member it
    // was found by takes it in anyway.
    let blocks = texts("us_patent_9017715_p26");
    assert!(
        !together(&blocks, "COMPOSITIONS", "are for, among other things"),
        "{blocks:?}"
    );
}

#[test]
fn list_markers_hung_out_of_a_table_are_not_a_column_of_line_numbers() {
    // OTC_SDXHF 2013, page 360: `i`, `ii`, `iii` down the left of a
    // reconciliation table stand like line numbers beside no line. Taken into
    // the margin column they let the cut through the table, 0.774 -> 0.455.
    let blocks = texts("otc_sdxhf_2013_p360");
    // ONE ROW, READ AS A ROW: in one block while the page reads the table as
    // prose, or as neighbouring cells once the table judge (#574) makes it a
    // table. What must not happen is the cut down the columns, which puts
    // the page numbers after the whole column of labels.
    let at = |needle: &str| blocks.iter().position(|b| b.contains(needle));
    let row_in_order = matches!(
        (at("preventive or corrective actions"), at("115")),
        (Some(label), Some(page)) if page == label + 1
    );
    assert!(
        together(&blocks, "preventive or corrective actions", "115") || row_in_order,
        "the table was cut down its columns: {blocks:?}"
    );
}

#[test]
fn a_title_over_a_table_is_not_a_running_head() {
    // uksi_20200121, page 50: `SCHEDULE 3` over the two columns of a schedule
    // is the shape of a patent's head. Set aside, it handed the table to the
    // column cut, 0.939 -> 0.627; it may only be set aside on a page that
    // numbers its lines in the gutter.
    let md = markdown("uksi_20200121_p50");
    // A CELL OF THE SCHEDULE, in whichever form the table is written: HTML
    // while its cells were decoded from the text and a wrapped cell came out
    // as two rows, a pipe table once the lattice (#574) reads the grid the
    // page draws and keeps each cell whole.
    assert!(
        md.contains("<td>A30 (T) between point A on sheet 1 and point")
            || md.contains("| A30 (T) between point A on sheet 1 and point"),
        "the schedule is no longer a table: {md}"
    );
}

#[test]
fn a_column_of_tab_runs_does_not_number_a_page() {
    // A 10-K's reserves table, page 95: the margin scan looks past a column of
    // tab runs, and counted as line numbers they set the heading aside and cut
    // the figures off their labels, 0.641 -> 0.535.
    //
    // THE ASSERTION IS THE INTENT AND NOT THE RENDERING (T4, #600). It read
    // `md.contains("Other\tliability $ 3,998,511")`, which is how this page
    // came out when the table models were off: a tab run, not a table, with a
    // stray tab inside the label itself. With them on the same figures read
    //
    //     | Other liability | $ | 3,998,511 |
    //
    // -- a table row, and the label joined. The label did not lose its figure,
    // which is what this test is for; only the shape it is written in changed.
    // So the check is now that one line holds both, whichever shape the page
    // takes -- AND that a second pair is on a line of its own, because
    // `lines().any()` alone cannot tell "the label kept its figure" from
    // "the whole page collapsed onto one line", and the defect this guards
    // against is precisely one that moves text between lines (T3, #602).
    let md = markdown("form_10k_0000011544_2019_p95");
    let holds = |left: &str, right: &str| -> Option<usize> {
        md.lines().position(|line| {
            let flat = line.replace('\t', " ");
            flat.contains(left) && flat.contains(right)
        })
    };
    let first = holds("Other liability", "3,998,511");
    let second = holds("Professional liability", "1,034,256");
    assert!(first.is_some(), "a label lost its figure: {md}");
    assert!(second.is_some(), "a label lost its figure: {md}");
    assert_ne!(
        first, second,
        "both rows are on one line, so the page collapsed rather than read: {md}"
    );
}

#[test]
fn a_line_number_drawn_at_the_end_of_a_left_column_line_is_peeled() {
    // 8287919, page 14: where a left-column line runs close to the gutter the
    // OCR layer joins the number into its run -- `market, citric acid and/or
    // 35` -- and it reached nine points into a gutter of fourteen. Before
    // `peel_trailing` the page came back line by line across both columns, two
    // of its seventeen annotated blocks recognisable (#574).
    // Read as Markdown: that is where the page came back interleaved, one
    // paragraph holding a line of each column in turn.
    let md = markdown("us_patent_8287919_p14");
    let blocks: Vec<String> = md.split("\n\n").map(str::to_string).collect();
    for (left, right) in [
        (
            "systematically named propanoic",
            "Alternatively, the pH can be titrated",
        ),
        ("with a pungent", "sulfuric acid, hydrochloric acid"),
    ] {
        assert!(
            !together(&blocks, left, right),
            "`{left}` and `{right}` came back in one block: {blocks:?}"
        );
    }
    assert!(
        blocks.iter().any(|b| b.contains("citric acid and/or")),
        "the line the number was peeled from lost its words: {blocks:?}"
    );
}

#[test]
fn line_numbers_beside_a_table_of_figures_still_number_the_page() {
    // 6420562, page 27: a table's banks of figures (`0.0`, `4.0`, `64.9`) and
    // the gutter's `5` ... `65` are all columns of narrow runs, and "more than
    // one column is a table" refused them all. Only the line numbers COUNT --
    // whole numbers rising by a common step -- and the page reads by column
    // again. `15` and `40` stand where the right column has no line, with
    // nothing after them; they are the column's too (#574).
    let blocks = texts("us_patent_6420562_p27");
    for (left, right) in [
        ("Ciba [bis(2,2,6,6-", "Finishing, Inc."),
        ("dichloro-6-tert-octylamino-", "Samples are tested"),
    ] {
        assert!(
            !together(&blocks, left, right),
            "`{left}` and `{right}` came back in one block: {blocks:?}"
        );
    }
}
