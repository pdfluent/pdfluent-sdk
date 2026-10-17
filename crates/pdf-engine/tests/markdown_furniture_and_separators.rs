//! The markdown switches for running heads and page separators.
//!
//! `extract_markdown` always dropped the running heads and always wrote
//! `<!-- page N -->`. [`ExtractOptions::include_headers`] writes the heads,
//! [`ExtractOptions::include_footers`] the page numbers, as plain paragraphs
//! at their reading-order position, and [`ExtractOptions::page_separators`]
//! drops the marker. The defaults are today's output, byte for byte, which
//! the pinned cases below hold.
//!
//! `fixtures/running-head.pdf` -- built here -- is the four-page report every
//! suite reads: the same head at the top of each page, a page number at the
//! foot, two lines of body. The binding suites cannot build it themselves,
//! so it is committed, and
//! [`the_committed_fixture_is_the_one_this_test_builds`] keeps the committed
//! bytes and the code that explains them from drifting apart. Regenerate with
//! `PDFLUENT_WRITE_GOLDEN=1`.
//!
//! Every field a PDF writer normally takes from the clock is pinned below,
//! for the reason
//! `crates/pdf-engine/tests/the_api_over_the_block_model.rs` pins them.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PageSeparators, PdfDocument};
use std::path::PathBuf;

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
/// The top of the text frame, well below the running head's band.
const TOP: f64 = 680.0;
/// One line of the body to the next.
const LEADING: f64 = 12.0;
/// Where the running header sits.
const HEAD_Y: f64 = 720.0;
/// Where the running footer sits.
const FOOT_Y: f64 = 80.0;
/// The line every page repeats at the top.
const HEAD: &str = "Annual Report 2026";
/// Fixed so two runs produce the same bytes.
const DATE: &str = "D:20261002000000Z";
/// Fixed for the same reason: a real writer derives `/ID` from the clock and
/// the file's contents, which is exactly what must not happen here.
const ID: &[u8] = b"pdfluent-running-head-fixture-001-pdfluent-";

/// One run of glyphs at a point.
struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
    }
}

/// A column of body text at the standard leading, from `TOP` down.
fn body(lines: &[String]) -> Vec<Piece> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| at(line, MARGIN, TOP - i as f64 * LEADING))
        .collect()
}

fn stream_of(pieces: &[Piece]) -> Vec<u8> {
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), BODY_SIZE.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    Content { operations }.encode().expect("encode content")
}

/// Four pages of a report: the same head at the top, a page number at the
/// foot, two lines of body that differ per page.
fn running_head_pdf() -> Vec<u8> {
    const PAGES: usize = 4;
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));

    let mut page_ids = Vec::new();
    for page in 0..PAGES {
        let mut pieces = vec![at(HEAD, MARGIN, HEAD_Y)];
        pieces.extend(body(&[
            format!("The {page}th section opens here and says something."),
            "It runs on for a second line of its own body type.".to_string(),
        ]));
        pieces.push(at(&format!("Page {} of {PAGES}", page + 1), 290.0, FOOT_Y));
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            stream_of(&pieces),
        )));
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), 612.into(), 792.into(),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => regular.clone() }),
            }),
        })));
    }

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>()),
        "Count" => Object::Integer(page_ids.len() as i64),
    }));
    for page_id in &page_ids {
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(*page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    let info_id = doc.add_object(dictionary! {
        "Producer" => Object::string_literal("PDFluent"),
        "Creator" => Object::string_literal("pdf-engine/tests/markdown_furniture_and_separators.rs"),
        "Title" => Object::string_literal("PDFluent running-head fixture"),
        "CreationDate" => Object::string_literal(DATE),
        "ModDate" => Object::string_literal(DATE),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.trailer.set("Info", Object::Reference(info_id));
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(ID.to_vec(), StringFormat::Hexadecimal),
            Object::String(ID.to_vec(), StringFormat::Hexadecimal),
        ]),
    );

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save the fixture");
    buf
}

/// Where the committed fixture lives, from this crate's manifest.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/running-head.pdf")
}

fn structured_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/structured.pdf")
}

fn open_fixture() -> PdfDocument {
    let bytes = std::fs::read(fixture_path()).expect("read fixtures/running-head.pdf");
    PdfDocument::open(bytes).expect("open the fixture")
}

fn furniture_options() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.include_headers = true;
    options.include_footers = true;
    options
}

fn headers_options() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.include_headers = true;
    options
}

fn footers_options() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.include_footers = true;
    options
}

fn no_separators_options() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.page_separators = PageSeparators::None;
    options
}

// ------------------------------------------------------------------- tests

/// The file the binding suites read is the file this test builds.
#[test]
fn the_committed_fixture_is_the_one_this_test_builds() {
    let built = running_head_pdf();
    let path = fixture_path();
    if std::env::var_os("PDFLUENT_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &built).expect("write the fixture");
        return;
    }
    let committed = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{} is missing ({e}). Regenerate it with PDFLUENT_WRITE_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(
        committed,
        built,
        "{} has drifted from the code that builds it. Regenerate with \
         PDFLUENT_WRITE_GOLDEN=1 and read the diff",
        path.display()
    );
}

/// The default markdown of `fixtures/structured.pdf`, byte for byte.
///
/// The rig measures what a caller of the default gets, so the switches this
/// ticket adds must not move a single default byte. This literal is the
/// output from before they existed.
#[test]
fn the_default_markdown_of_structured_pdf_is_pinned_byte_for_byte() {
    let bytes = std::fs::read(structured_path()).expect("read fixtures/structured.pdf");
    let document = PdfDocument::open(bytes).expect("open structured.pdf");
    let markdown = document
        .extract_markdown(&ExtractOptions::default())
        .expect("extract markdown");
    assert_eq!(
        markdown,
        "# Safety rules\n\
         \n\
         - Wear the issued boots on the floor.\n\
         - Keep the gangways clear of stock.\n\
         - Report a spill before you clean it.\n\
         \n\
         <!-- page 1 -->\n\
         \n\
         | Band | Units | Rate |\n\
         | --- | --- | --- |\n\
         | Standing | none | 18.90 |\n\
         | Day | 0 to 500 | 24.31 |\n\
         | Night | 0 to 500 | 11.07 |\n"
    );
}

/// The fixture really has furniture to drop: four running heads and four
/// page numbers in the blocks.
#[test]
fn the_fixture_marks_four_running_heads_and_four_page_numbers() {
    let blocks = open_fixture()
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    let heads = blocks
        .iter()
        .filter(|block| block.text == HEAD)
        .collect::<Vec<_>>();
    assert_eq!(heads.len(), 4, "every page repeats the head");
    for head in heads {
        assert_eq!(head.kind, BlockKind::Header);
    }
    let feet = blocks
        .iter()
        .filter(|block| block.text.starts_with("Page "))
        .collect::<Vec<_>>();
    assert_eq!(feet.len(), 4, "every page numbers itself");
    for foot in feet {
        assert_eq!(foot.kind, BlockKind::Footer);
    }
}

/// By default the running heads stay out of the markdown, and every page
/// change is a comment.
#[test]
fn the_default_drops_the_running_head_and_marks_every_page() {
    let markdown = open_fixture()
        .extract_markdown(&ExtractOptions::default())
        .expect("extract markdown");
    assert!(
        !markdown.contains(HEAD),
        "the running head is not in the default markdown:\n{markdown}"
    );
    assert!(
        !markdown.contains("Page 1 of 4"),
        "the page number is not in the default markdown:\n{markdown}"
    );
    for page in 1..4 {
        assert!(
            markdown.contains(&format!("<!-- page {page} -->")),
            "page {page} is marked:\n{markdown}"
        );
    }
}

/// With both furniture switches the running head and the page number are in
/// the markdown -- as plain paragraphs, never as headings.
#[test]
fn furniture_is_written_as_plain_paragraphs_at_its_reading_order_position() {
    let markdown = open_fixture()
        .extract_markdown(&furniture_options())
        .expect("extract markdown");
    assert_eq!(
        markdown.matches(HEAD).count(),
        4,
        "every page's head is written:\n{markdown}"
    );
    assert!(
        markdown.contains("Page 1 of 4"),
        "the page number is written:\n{markdown}"
    );
    for line in markdown.lines() {
        assert!(
            !(line.starts_with('#') && line.contains("Annual Report")),
            "a running head is not a section title: {line:?}"
        );
    }
    // At its reading-order position: the reading order keeps the furniture
    // after the last block of the body (#571), page by page, and the
    // serializer writes the blocks where they stand.
    let last_body = markdown
        .find("The 3th section opens here")
        .expect("the body is written");
    let head = markdown.find(HEAD).expect("the head is written");
    assert!(
        last_body < head,
        "the furniture follows the body:\n{markdown}"
    );
    let mut feet = Vec::new();
    for page in 1..=4 {
        feet.push(
            markdown
                .find(&format!("Page {page} of 4"))
                .unwrap_or_else(|| panic!("page {page}'s number is written:\n{markdown}")),
        );
    }
    assert!(
        feet.windows(2).all(|pair| pair[0] < pair[1]),
        "the numbers read in page order:\n{markdown}"
    );
}

/// Headers on and footers off keeps only the running head: every page's
/// title is written, and no page number is.
#[test]
fn headers_on_footers_off_keeps_only_the_running_head() {
    let markdown = open_fixture()
        .extract_markdown(&headers_options())
        .expect("extract markdown");
    assert_eq!(
        markdown.matches(HEAD).count(),
        4,
        "every page's head is written:\n{markdown}"
    );
    assert!(
        !markdown.contains("Page 1 of 4"),
        "no page number is written:\n{markdown}"
    );
    assert!(
        !markdown.contains("Page 2 of 4"),
        "no page number is written:\n{markdown}"
    );
}

/// Footers on and headers off keeps only the page numbers: every page
/// numbers itself, and the running head stays out.
#[test]
fn footers_on_headers_off_keeps_only_the_page_number() {
    let markdown = open_fixture()
        .extract_markdown(&footers_options())
        .expect("extract markdown");
    assert!(
        !markdown.contains(HEAD),
        "the running head is not written:\n{markdown}"
    );
    for page in 1..=4 {
        assert!(
            markdown.contains(&format!("Page {page} of 4")),
            "page {page}'s number is written:\n{markdown}"
        );
    }
}

/// Both furniture switches off is the default, byte for byte: what the rig
/// measures does not move.
#[test]
fn both_furniture_switches_off_is_the_default_bytes() {
    let fixture = open_fixture();
    let switched = {
        let mut options = ExtractOptions::default();
        options.include_headers = false;
        options.include_footers = false;
        fixture
            .extract_markdown(&options)
            .expect("extract markdown")
    };
    let default = fixture
        .extract_markdown(&ExtractOptions::default())
        .expect("extract markdown");
    assert_eq!(switched, default);
}

/// With `page_separators` off there is no marker anywhere, and the paragraphs
/// on either side of a page change stand separated by a blank line.
#[test]
fn without_separators_no_marker_and_paragraphs_still_apart() {
    let markdown = open_fixture()
        .extract_markdown(&no_separators_options())
        .expect("extract markdown");
    assert!(
        !markdown.contains("<!-- page"),
        "no page marker anywhere:\n{markdown}"
    );
    assert!(
        markdown.contains(
            "It runs on for a second line of its own body type.\n\
             \n\
             The 1th section opens here and says something."
        ),
        "the pages stand separated by a blank line:\n{markdown}"
    );
}

/// The switches compose: furniture in, markers out.
#[test]
fn furniture_without_separators() {
    let mut options = furniture_options();
    options.page_separators = PageSeparators::None;
    let markdown = open_fixture()
        .extract_markdown(&options)
        .expect("extract markdown");
    assert!(markdown.contains(HEAD));
    assert!(!markdown.contains("<!-- page"));
}

/// The options-aware serializers with default options write what the plain
/// ones always wrote.
#[test]
fn default_options_write_todays_bytes() {
    use pdf_engine::markdown::{to_markdown_with_options, MarkdownOptions};
    use pdf_engine::to_markdown;

    let blocks = open_fixture()
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    let options = MarkdownOptions::default();
    assert_eq!(
        to_markdown_with_options(&blocks, &options),
        to_markdown(&blocks)
    );
}
