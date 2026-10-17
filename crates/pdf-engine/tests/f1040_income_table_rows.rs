// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The f1040 shape the merged-span re-pin broke (#636).
//!
//! After the spacing and font-switch fixes merged spans, five product
//! regressions shipped in the re-pinned `e1e2-f1040` golden. Four are fixed
//! here and pinned by the running tests below: the `MM / DD / YYYY`
//! slashes were displaced, a stray leader dot appeared in the line 9
//! `total income`, the `d`/`7a` rows fell out of the Income table, and the
//! `b Check if` option row stopped being a table row.
//!
//! Two more are fixed here: the page-0 `d` line and the `7a` row are
//! reclaimed as Income table rows, and the `b Check if` option row reads as
//! a table row again. The reclaim never touches the learned judge or its
//! threshold (`p` around 0.08 on the covering sets stays refused): it only
//! adopts marker-led paragraphs directly above or below an already accepted
//! table whose own marker, amount and ruling columns confirm them.
//!
//! The last one is fixed here too: the page-1 `27a` token sat between the
//! `qualifying child,` line and the `you may need to` line, and the
//! half-height cut put it on its own line ahead of `you may need to`. The
//! final by-line cell text now lets glyph overlap win where it disagrees
//! with the cut -- the token shares most of its box with the lower line --
//! while `in_reading_order` and the grid decisions stay untouched.
//!
//! Every test pins the correct shape through the public path on the real
//! form, selected by the page geometry the assessment measured.

use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

/// The default blocks of the real form, in document order.
fn f1040_blocks() -> Vec<pdf_engine::Block> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/f1040.pdf"
    ))
    .expect("read the f1040 fixture");
    let document = PdfDocument::open(bytes).expect("open the f1040 fixture");
    document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options")
}

/// Whether the block is a table cell.
fn is_cell(block: &pdf_engine::Block) -> bool {
    matches!(block.kind, BlockKind::TableCell { .. })
}

/// The `d` checkbox line belongs to the Income table: the stub describing
/// married filing separately and the `7a` capital-gain row beside it are
/// table rows, not one paragraph spanning both visual lines.
///
/// Red while the rows stay paragraphs: without the edge-row reclaim the two
/// visual lines merge into one paragraph block.
#[test]
fn the_d_line_and_7a_row_stay_in_the_income_table() {
    let blocks = f1040_blocks();
    let page0: Vec<&pdf_engine::Block> = blocks
        .iter()
        .filter(|b| b.page == 0 && b.bbox[1] > 80.0 && b.bbox[1] < 125.0)
        .collect();
    assert!(!page0.is_empty(), "the income band reads nothing");
    let d = page0
        .iter()
        .find(|b| b.text.contains("lived apart"))
        .expect("the d line reads");
    assert!(
        is_cell(d),
        "the d line is a table row, got {:?}: {}",
        d.kind,
        d.text
    );
    assert!(
        d.bbox[3] < 113.0,
        "the d line keeps its own visual line, got bbox {:?}",
        d.bbox
    );
    let gain = page0
        .iter()
        .find(|b| b.text.contains("Capital gain"))
        .expect("the 7a row reads");
    assert!(
        is_cell(gain),
        "the 7a row is a table row, got {:?}: {}",
        gain.kind,
        gain.text
    );
    assert!(
        gain.bbox[1] < 101.0,
        "the 7a row keeps its own visual line, got bbox {:?}",
        gain.bbox
    );
    let amount = blocks
        .iter()
        .filter(|b| {
            b.page == 0
                && is_cell(b)
                && b.text.trim() == "7a"
                && b.bbox[0] > 480.0
                && b.bbox[1] > 85.0
                && b.bbox[1] < 105.0
        })
        .count();
    assert!(amount >= 1, "the 7a amount column stays a cell of the row");
}

/// The `b Check if` grid option row (ruled checkboxes, like its 8/9/10/11a
/// siblings) is a table row, not a paragraph.
///
/// Red while the row stays a paragraph: without the edge-row reclaim the
/// option line never rejoins its table.
#[test]
fn the_schedule_d_option_row_stays_a_table_row() {
    let blocks = f1040_blocks();
    let row = blocks
        .iter()
        .filter(|b| b.page == 0)
        .find(|b| b.text.contains("Schedule D not required"))
        .expect("the option row reads");
    assert!(
        is_cell(row),
        "the option row is a table row, got {:?}: {}",
        row.kind,
        row.text
    );
}

/// The date placeholder reads with its slashes in place: `MM / DD / YYYY`.
#[test]
fn the_date_placeholders_keep_their_slashes_in_order() {
    let blocks = f1040_blocks();
    let line = blocks
        .iter()
        .filter(|b| b.page == 0)
        .find(|b| b.text.contains("MM") && b.text.contains("YYYY"))
        .expect("the date line reads");
    let spaceless: String = line.text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        spaceless.contains("MM/DD/YYYY"),
        "slashes interleave the groups in order, got: {}",
        line.text
    );
}

/// Line 9 totals without a stray leader dot: `total income`, not
/// `total . income`.
#[test]
fn the_line_9_total_reads_without_a_stray_dot() {
    let blocks = f1040_blocks();
    let total = blocks
        .iter()
        .filter(|b| b.page == 0 && b.bbox[1] > 50.0 && b.bbox[1] < 70.0)
        .find(|b| b.text.contains("total") && b.text.contains("income"))
        .expect("the line 9 total reads");
    assert!(
        total.text.contains("total income"),
        "no stray dot inside the label, got: {}",
        total.text
    );
}

/// The `27a` token stays on its row: after `you may need to` in the stub,
/// not moved ahead of it.
///
/// Red while `27a` opens its own line: without the overlap rule the
/// half-height cut reads the token ahead of `need to`.
#[test]
fn the_27a_token_stays_on_its_row() {
    let blocks = f1040_blocks();
    let stub = blocks
        .iter()
        .filter(|b| b.page == 1 && b.bbox[1] > 405.0 && b.bbox[1] < 445.0)
        .find(|b| b.text.contains("qualifying child"))
        .expect("the 27a stub reads");
    let need_to = stub
        .text
        .find("need to")
        .expect("the stub keeps its wording");
    let token = stub.text.find("27a").expect("the stub keeps 27a");
    assert!(
        token > need_to,
        "27a stays after `need to`, got: {}",
        stub.text
    );
}
