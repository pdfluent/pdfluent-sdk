// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The caller chooses the cell-path word rules (#659) and the `/ActualText`
//! replacements (#663): two features, two flags.
//!
//! [`ExtractOptions::cell_path_words`] defaults to false: leader dots fold
//! with a space, close cell words join, the TATR fill aligns fragments onto
//! text tokens one way, and a word space at exactly a quarter em reads as
//! joined. With the option on, abutting leader dots join their label, a
//! spaced pair the gap parts stays parted, the fill aligns both ways, and
//! the quarter-em boundary reads as a space. It never applies `/ActualText`.
//!
//! [`ExtractOptions::actual_text`] defaults to false too: marked runs read
//! what the page drew. With the option on, an `/ActualText` replacement
//! reads instead of its marked run. It never moves the word rules.
//!
//! Two word rules are pinned here through the public path: the fold on the
//! rate cells of `fixtures/structured.pdf` and one DocLayNet cell, and the
//! cell join on an operator-built ruled table whose close words part with
//! the option. The arxiv prose page the old close-words pin read as a table
//! carries none -- its rules are chart panel edges -- so it pins the
//! negative instead: no cell is detected there with or without the option,
//! and its `FIG. 2` caption still reads as paragraph prose. The quarter-em
//! boundary needs a gap at exactly a quarter em, which PDF
//! floats and device advances cannot place robustly through the public
//! path; the fill's reverse direction needs fragments joined where the
//! text splits, which glyph and span boxes never diverge into on a
//! consistent path. Both branches are read by the same flag on the same
//! path as the two pinned here. `/ActualText` is pinned on a marked run
//! with its own flag, and each flag alone is pinned to move only its own
//! behaviour.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};
use std::path::PathBuf;

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn options_on() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.cell_path_words = true;
    options
}

fn options_actual_text() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.actual_text = true;
    options
}

fn blocks_of(pdf: &[u8], options: &ExtractOptions) -> Vec<pdf_engine::Block> {
    let document = PdfDocument::open(pdf.to_vec()).expect("open");
    document.extract_blocks(options).expect("extract")
}

/// The text of one table cell, by row and column.
fn cell_text(blocks: &[pdf_engine::Block], row: u16, column: u16) -> String {
    blocks
        .iter()
        .find_map(|block| match &block.kind {
            BlockKind::TableCell {
                row: r, column: c, ..
            } if *r == row && *c == column => Some(block.text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "no cell at row {row} column {column} in {:?}",
                blocks
                    .iter()
                    .map(|block| (&block.kind, block.text.as_str()))
                    .collect::<Vec<_>>()
            )
        })
}

/// The option defaults off, and the options stay comparable.
#[test]
fn the_cell_path_is_opt_in() {
    fn assert_is_eq<T: Eq>() {}
    assert_is_eq::<ExtractOptions>();
    assert!(
        !ExtractOptions::default().cell_path_words,
        "the default reads as before"
    );
}

/// The replacement defaults off too: marked runs read what the page drew.
#[test]
fn the_actual_text_is_opt_in() {
    assert!(
        !ExtractOptions::default().actual_text,
        "the default reads as before"
    );
}

/// Decimal dots folded onto their labels: joined with the option, and since
/// the decimal fold also runs without it, joined there too. Through the
/// public path on real fixtures.
#[test]
fn folded_decimal_dots_join_with_the_option() {
    let bytes = std::fs::read(workspace_path("fixtures/structured.pdf")).expect("read fixture");
    let off = blocks_of(&bytes, &ExtractOptions::default());
    let on = blocks_of(&bytes, &options_on());
    for (row, spaced, joined) in [
        (1, "18.90", "18.90"),
        (2, "24.31", "24.31"),
        (3, "11.07", "11.07"),
    ] {
        assert_eq!(
            cell_text(&off, row, 2),
            spaced,
            "row {row} without the option"
        );
        assert_eq!(cell_text(&on, row, 2), joined, "row {row} with the option");
    }

    let bytes =
        std::fs::read(fixture_path("doclaynet/otc_sdxhf_2013_p360.pdf")).expect("read fixture");
    let off = blocks_of(&bytes, &ExtractOptions::default());
    let on = blocks_of(&bytes, &options_on());
    assert_eq!(cell_text(&off, 10, 0), "3.");
    assert_eq!(cell_text(&on, 10, 0), "3.");
}

/// Close words in a rebuilt cell: joined without the option, parted with
/// it. The spans are the same either way; only the cell join differs.
///
/// Through the public path on an operator-built ruled table: a stroked 3 x 3
/// lattice whose row-1 cell holds `0` with a bold `max` set 1.5 pt into its
/// advance -- closer than the 2.78 pt word space, so the default join reads
/// one word and the option parts the pair. The bold face keeps the two runs
/// as separate lattice fragments (same-face touching runs merge at
/// placement); the gap is what the option reads.
#[test]
fn close_cell_words_part_with_the_option() {
    let pdf = close_words_table_pdf();
    let off = blocks_of(&pdf, &ExtractOptions::default());
    let on = blocks_of(&pdf, &options_on());
    assert_eq!(cell_text(&off, 1, 1), "0max");
    assert_eq!(cell_text(&on, 1, 1), "0 max");
}

/// The ruled table for the close-words pin: stroked lattice, header plus two
/// rows, the close pair in row 1 column 1. Operator by operator, no corpus.
fn close_words_table_pdf() -> Vec<u8> {
    const BODY: f64 = 10.0;
    /// Advance of `0` in Helvetica at body size: the `max` run starts
    /// this far past the `0` run's origin, less the overlap below.
    const ZERO_ADVANCE: f64 = 5.56;
    /// How far the `max` run sits inside the `0` run's advance: past the
    /// tenth-em overlap the option parts on, clear of a full word space.
    const CLOSE_OVERLAP: f64 = 1.5;
    const LEFT: [f64; 3] = [60.0, 226.0, 393.0];
    const TOP: f64 = 700.0;
    const PITCH: f64 = 117.0;

    let mut doc = Document::with_version("1.7");
    let regular = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let strong = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
    });
    let mut operations = Vec::new();
    for x in [50.0, 216.0, 383.0, 550.0] {
        operations.push(Operation::new("m", vec![x.into(), 400.0.into()]));
        operations.push(Operation::new("l", vec![x.into(), 750.0.into()]));
        operations.push(Operation::new("S", vec![]));
    }
    for y in [400.0, 516.0, 633.0, 750.0] {
        operations.push(Operation::new("m", vec![50.0.into(), y.into()]));
        operations.push(Operation::new("l", vec![550.0.into(), y.into()]));
        operations.push(Operation::new("S", vec![]));
    }
    let mut show = |body: &[u8], x: f64, y: f64, bold: bool| {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if bold { b"F2".to_vec() } else { b"F1".to_vec() }),
                BODY.into(),
            ],
        ));
        operations.push(Operation::new("Td", vec![x.into(), y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(body.to_vec(), lopdf::StringFormat::Literal)],
        ));
        operations.push(Operation::new("ET", vec![]));
    };
    for (left, body) in LEFT.iter().zip(["Band", "Units", "Rate"]) {
        show(body.as_bytes(), *left, TOP, true);
    }
    show(b"Standing", LEFT[0], TOP - PITCH, false);
    show(b"0", LEFT[1], TOP - PITCH, false);
    show(
        b"max",
        LEFT[1] + ZERO_ADVANCE - CLOSE_OVERLAP,
        TOP - PITCH,
        true,
    );
    show(b"18.90", LEFT[2], TOP - PITCH, false);
    show(b"Day", LEFT[0], TOP - 2.0 * PITCH, false);
    show(b"none", LEFT[1], TOP - 2.0 * PITCH, false);
    show(b"24.31", LEFT[2], TOP - 2.0 * PITCH, false);
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => regular, "F2" => strong } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// No table on the arxiv prose page, with or without the option. The page is
/// prose and figures: its 51 drawn rules are chart panel edges (every one 84
/// points wide, inside the two figure bands), and the `FIG. 2` caption the
/// old pin read as row 36 of a table is paragraph prose. Through the public
/// path on the real fixture.
#[test]
fn no_table_is_detected_on_the_arxiv_prose_page() {
    let bytes =
        std::fs::read(fixture_path("doclaynet/arxiv_1001_0809_prose.pdf")).expect("read fixture");
    for options in [ExtractOptions::default(), options_on()] {
        let blocks = blocks_of(&bytes, &options);
        assert!(
            blocks
                .iter()
                .all(|block| !matches!(&block.kind, BlockKind::TableCell { .. })),
            "a table was detected on the arxiv prose page: {:?}",
            blocks
                .iter()
                .map(|block| (&block.kind, block.text.as_str()))
                .collect::<Vec<_>>()
        );
        for caption in ["FIG. 2", "Markovian approximation", "= 0"] {
            assert!(
                blocks.iter().any(|block| block.text.contains(caption)),
                "the FIG. 2 caption left the page (cell_path_words {}): {:?}",
                options.cell_path_words,
                blocks
                    .iter()
                    .map(|block| block.text.as_str())
                    .collect::<Vec<_>>()
            );
        }
    }
}

/// One page drawing `QQQ` inside a marked run replaced by `/ActualText`.
fn page_with_an_actual_text_run() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let ops = vec![
        Operation::new(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Dictionary(dictionary! {
                    "ActualText" => Object::string_literal("REPLACEMENT"),
                }),
            ],
        ),
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new("Tj", vec![Object::string_literal("QQQ")]),
        Operation::new("ET", vec![]),
        Operation::new("EMC", vec![]),
    ];
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// An `/ActualText` replacement reads instead of its marked run with
/// `actual_text`, and is ignored without it.
#[test]
fn actual_text_applies_with_the_option() {
    let pdf = page_with_an_actual_text_run();
    let off = blocks_of(&pdf, &ExtractOptions::default());
    let on = blocks_of(&pdf, &options_actual_text());
    assert_eq!(off.len(), 1);
    assert_eq!(on.len(), 1);
    assert_eq!(off[0].text, "QQQ");
    assert_eq!(on[0].text, "REPLACEMENT");
}

/// The cell-path flag alone leaves `/ActualText` ignored: the marked run
/// reads what the page drew.
#[test]
fn cell_path_words_alone_ignores_actual_text() {
    let pdf = page_with_an_actual_text_run();
    let blocks = blocks_of(&pdf, &options_on());
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "QQQ");
}

/// The replacement flag alone leaves the cell words at their default: the
/// fold stays spaced and the close words stay joined.
#[test]
fn actual_text_alone_leaves_cell_words_default() {
    let bytes = std::fs::read(workspace_path("fixtures/structured.pdf")).expect("read fixture");
    let blocks = blocks_of(&bytes, &options_actual_text());
    for (row, spaced) in [(1, "18.90"), (2, "24.31"), (3, "11.07")] {
        assert_eq!(cell_text(&blocks, row, 2), spaced, "row {row} stays spaced");
    }

    let pdf = close_words_table_pdf();
    let blocks = blocks_of(&pdf, &options_actual_text());
    assert_eq!(cell_text(&blocks, 1, 1), "0max");
}
