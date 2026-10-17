// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Content runs keep their boundaries under raw order (E2c).
//!
//! A turned run joins along the way it reads, a squashed size reports the
//! area scale of its matrix, and every span says whether its content run
//! ends after it: style, size, declared face or direction end one, while a
//! segment, a text object or a bare gap never does on its own. A consumer
//! joining same-style spans merges past the open boundaries and stops at
//! the closed ones, which reproduces the recorded spans.

use pdf_engine::{Block, ExtractOptions, PdfDocument, TextSpan};
use std::path::PathBuf;

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn raw_blocks_on_first_page(relative: &str) -> Vec<Block> {
    let bytes = std::fs::read(workspace_path(relative)).expect("read the fixture");
    let document = PdfDocument::open(bytes).expect("open the fixture");
    let mut options = ExtractOptions::window(0, 1);
    options.raw_order = true;
    document
        .extract_blocks(&options)
        .expect("extract in raw order")
}

fn first_holding(blocks: &[Block], landmark: &str) -> usize {
    blocks
        .iter()
        .position(|b| b.text.contains(landmark))
        .unwrap_or_else(|| panic!("no raw block holds {landmark:?}"))
}

fn span_holding<'a>(block: &'a Block, text: &str) -> &'a TextSpan {
    block
        .spans
        .iter()
        .find(|s| s.text == text)
        .unwrap_or_else(|| panic!("no raw span reads {text:?}"))
}

/// The turned caption reads as one line: its four glyphs join along the way
/// down the page draws them.
#[test]
fn the_turned_caption_reads_as_one_line() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(blocks[0].text, "Form");
}

/// A squashed size reports the area scale of its matrix: `Deceased` is set
/// 6.86 by 7 and reads 6.9296, and the squeezed space 4.9497, not the first
/// columns 6.86 and 3.5.
#[test]
fn the_squashed_line_reports_its_set_size() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let block = &blocks[first_holding(&blocks, "Deceased")];
    let size = span_holding(block, "Deceased").font_size;
    assert!(
        (size - 6.9296).abs() < 1e-3,
        "Deceased reads {size}, not its set 6.9296"
    );
    let squeezed = block
        .spans
        .iter()
        .find(|s| s.text == " " && (s.font_size - 4.9497).abs() < 1e-3)
        .unwrap_or_else(|| panic!("no raw span is the squeezed space"));
    assert!(
        (squeezed.font_size - 4.9497).abs() < 1e-3,
        "the squeezed space reads {}, not 4.9497",
        squeezed.font_size
    );
}

/// Same-style runs stay split where the style or the size does: every span
/// of the date row ends its run, so `MM     ` never merges into the space
/// beside it.
#[test]
fn same_style_runs_stay_split() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let block = &blocks[first_holding(&blocks, "MM     ")];
    for text in ["Deceased", "MM     ", "DD     ", "YYYY"] {
        assert!(
            span_holding(block, text).complete_run,
            "{text:?} leaves its run open"
        );
    }
    for span in block.spans.iter().filter(|s| s.text == " ") {
        assert!(span.complete_run, "a date-row space leaves its run open");
    }
}

/// A segment alone keeps the run open: `4a` spans two segments and reads as
/// one span downstream, and so do the foreign-address fields across their
/// text objects.
#[test]
fn a_segment_alone_keeps_the_run_open() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let block = &blocks[first_holding(&blocks, "IRA distributions")];
    assert!(
        !span_holding(block, "4").complete_run,
        "`4` ends its run before `a`"
    );
    let block = &blocks[first_holding(&blocks, "Foreign postal code")];
    let province = block
        .spans
        .iter()
        .find(|s| s.text.starts_with("Foreign province"))
        .expect("no raw span reads the province field");
    assert!(
        !province.complete_run,
        "the province field ends its run before the postal one"
    );
}

/// A declared face ends the run where the program's own face cannot tell
/// the two apart: `20` never merges into `25`.
#[test]
fn a_declared_face_ends_the_run() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let block = &blocks[first_holding(&blocks, "U.S. Individual")];
    assert!(
        span_holding(block, "20").complete_run,
        "`20` leaves its run open into `25`"
    );
}

/// The default path leaves every span open: the flag is raw-only.
#[test]
fn the_default_path_leaves_every_span_open() {
    let bytes = std::fs::read(workspace_path("corpus/f1040.pdf")).expect("read the fixture");
    let document = PdfDocument::open(bytes).expect("open the fixture");
    let options = ExtractOptions::window(0, 1);
    let blocks = document.extract_blocks(&options).expect("extract");
    assert!(
        blocks
            .iter()
            .flat_map(|b| &b.spans)
            .all(|s| !s.complete_run),
        "a default span ends its run"
    );
}
