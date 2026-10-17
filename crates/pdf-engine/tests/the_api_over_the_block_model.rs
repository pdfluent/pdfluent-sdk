//! The two methods a caller reaches the block model through (#573).
//!
//! `extract_blocks` and `extract_markdown` are what every binding wraps, and
//! `fixtures/structured.pdf` -- built here -- is the document all six suites
//! read. One document, so that a heading, a list item and a table cell mean
//! the same three things in Rust, Python, Node, Java, .NET and the browser,
//! and a change that breaks the crossing in one of them shows up next to five
//! that still work.
//!
//! WHY THE FIXTURE IS BUILT BY A TEST AND COMMITTED. The five binding suites
//! run without a Rust build having happened first -- `mvn test`, `dotnet
//! test`, `pytest` against a wheel -- so the file has to be in the tree. A
//! committed blob with no code behind it is a file nobody can explain, so the
//! code that writes it is here and
//! [`the_committed_fixture_is_the_one_this_test_builds`] compares the two byte
//! for byte. Regenerate with `PDFLUENT_WRITE_GOLDEN=1`, which is the same
//! switch the serializer goldens use.
//!
//! Every field a PDF writer normally takes from the clock -- `/CreationDate`,
//! `/ModDate`, both halves of `/ID` -- is pinned below rather than normalised
//! away at comparison time, for the reason
//! `crates/pdf-manip/examples/java_binding_fixture.rs` pins them: the bytes
//! most likely to drift are the ones a lenient comparison stops reading.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, OptionsError, PdfDocument};
use std::path::PathBuf;

const BODY: f64 = 10.0;
const TITLE: f64 = 16.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 680.0;
/// One row of the table to the next.
const PITCH: f64 = 18.0;
/// Fixed so two runs produce the same bytes. The day #573 landed the API.
const DATE: &str = "D:20260920000000Z";
/// Fixed for the same reason: a real writer derives `/ID` from the clock and
/// the file's contents, which is exactly what must not happen here.
const ID: &[u8] = b"pdfluent-structured-extraction-fixture-001";

/// Where the committed fixture lives, from this crate's manifest.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/structured.pdf")
}

// ------------------------------------------------------------- the document

/// The face a run of glyphs is set in.
#[derive(Clone, Copy)]
enum Face {
    Regular,
    Bold,
}

impl Face {
    fn resource(self) -> &'static [u8] {
        match self {
            Face::Regular => b"F1",
            Face::Bold => b"F2",
        }
    }
}

/// One run of glyphs at a point, in a size and a face of its own.
struct Piece {
    bytes: Vec<u8>,
    face: Face,
    x: f64,
    y: f64,
    size: f64,
}

/// A horizontal rule: a filled rectangle a fraction of a point high, which is
/// how a document rules a table.
struct Rule {
    left: f64,
    right: f64,
    y: f64,
}

struct Page {
    pieces: Vec<Piece>,
    rules: Vec<Rule>,
}

fn piece(body: &str, x: f64, y: f64, size: f64, face: Face) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        face,
        x,
        y,
        size,
    }
}

fn text(body: &str, x: f64, y: f64) -> Piece {
    piece(body, x, y, BODY, Face::Regular)
}

/// An item marked with the WinAnsi bullet: the marker in a run of its own, the
/// text hanging off it, which is how a word processor sets a list.
fn bullet(body: &str, y: f64) -> Vec<Piece> {
    vec![
        Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: MARGIN,
            y,
            size: BODY,
        },
        text(body, MARGIN + 14.0, y),
    ]
}

/// One row of the table: the cells at the column positions given.
fn row(cells: [&str; 3], y: f64, face: Face) -> Vec<Piece> {
    [MARGIN, 220.0, 360.0]
        .iter()
        .zip(cells)
        .map(|(x, body)| piece(body, *x, y, BODY, face))
        .collect()
}

/// A rule the width of the table, over the row at `index`.
fn rule_over(index: usize) -> Rule {
    Rule {
        left: MARGIN - 6.0,
        right: 430.0,
        y: TOP - index as f64 * PITCH + 11.0,
    }
}

/// A list under a heading, which is page 0.
fn rules_page() -> Page {
    let mut pieces = vec![piece("Safety rules", MARGIN, TOP, TITLE, Face::Bold)];
    pieces.extend(bullet("Wear the issued boots on the floor.", TOP - 24.0));
    pieces.extend(bullet("Keep the gangways clear of stock.", TOP - 36.0));
    pieces.extend(bullet("Report a spill before you clean it.", TOP - 48.0));
    Page {
        pieces,
        rules: Vec::new(),
    }
}

/// A ruled table under a heading, which is page 1.
fn tariff_page() -> Page {
    let mut pieces = row(["Band", "Units", "Rate"], TOP, Face::Bold);
    for (index, cells) in [
        ["Standing", "none", "18.90"],
        ["Day", "0 to 500", "24.31"],
        ["Night", "0 to 500", "11.07"],
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            [cells[0], cells[1], cells[2]],
            TOP - (index + 1) as f64 * PITCH,
            Face::Regular,
        ));
    }
    Page {
        pieces,
        rules: (0..5).map(rule_over).collect(),
    }
}

fn stream_of(page: &Page) -> Vec<u8> {
    let mut operations = Vec::new();
    for drawn in &page.rules {
        operations.push(Operation::new(
            "re",
            vec![
                drawn.left.into(),
                drawn.y.into(),
                (drawn.right - drawn.left).into(),
                0.4.into(),
            ],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for piece in &page.pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(piece.face.resource().to_vec()),
                piece.size.into(),
            ],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(piece.bytes.clone(), StringFormat::Literal)],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    Content { operations }.encode().expect("encode content")
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

/// The fixture: a list under a heading on page 0, a ruled table on page 1.
fn structured_pdf() -> Vec<u8> {
    let pages = [rules_page(), tariff_page()];
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let strong = font(&mut doc, b"Helvetica-Bold");

    let mut page_ids = Vec::new();
    for page in &pages {
        let content_id =
            doc.add_object(Object::Stream(Stream::new(dictionary! {}, stream_of(page))));
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), 612.into(), 792.into(),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "F1" => regular.clone(),
                    "F2" => strong.clone(),
                }),
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
        "Creator" => Object::string_literal("pdf-engine/tests/the_api_over_the_block_model.rs"),
        "Title" => Object::string_literal("PDFluent structured-extraction fixture"),
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

fn document() -> PdfDocument {
    PdfDocument::open(structured_pdf()).expect("open the fixture")
}

// ------------------------------------------------------------------- tests

/// The file the five binding suites read is the file this test builds.
///
/// They cannot build it themselves -- `mvn test` and `dotnet test` run without
/// a Rust step -- so it is committed, and this is what keeps the committed
/// bytes and the code that explains them from drifting apart.
#[test]
fn the_committed_fixture_is_the_one_this_test_builds() {
    let built = structured_pdf();
    let path = fixture_path();
    if std::env::var_os("PDFLUENT_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &built).expect("write the fixture");
        return;
    }
    let committed = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{} is missing ({e}). Regenerate it with PDFLUENT_WRITE_GOLDEN=1 \
             cargo test -p pdf-engine --test the_api_over_the_block_model",
            path.display()
        )
    });
    assert_eq!(
        committed.len(),
        built.len(),
        "the committed fixture is not what this test builds; regenerate it with \
         PDFLUENT_WRITE_GOLDEN=1 and read the diff"
    );
    assert!(
        committed == built,
        "the committed fixture is not what this test builds; regenerate it with \
         PDFLUENT_WRITE_GOLDEN=1 and read the diff"
    );
}

/// The three structures the milestone is about, through the markdown method.
///
/// This is the assertion every binding makes in its own language, and it is
/// here first because a binding that fails it has to be able to rule out the
/// engine.
#[test]
fn the_markdown_carries_a_heading_a_list_item_and_a_table_cell() {
    let markdown = document()
        .extract_markdown(&ExtractOptions::default())
        .expect("extract markdown");

    assert!(
        markdown.contains("# Safety rules"),
        "no heading in:\n{markdown}"
    );
    assert!(
        markdown.contains("- Wear the issued boots on the floor."),
        "no list item in:\n{markdown}"
    );
    assert!(
        markdown.contains("| Band | Units | Rate |"),
        "no table in:\n{markdown}"
    );
}

/// The same three, through the block method, where they are kinds rather than
/// syntax.
#[test]
fn the_blocks_carry_a_heading_a_list_item_and_a_table_cell() {
    let blocks = document()
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");

    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Heading { level: 1 }) && b.text == "Safety rules"),
        "no level-1 heading among {} blocks",
        blocks.len()
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::ListItem { .. })
                && b.text == "Wear the issued boots on the floor."),
        "no list item among {} blocks",
        blocks.len()
    );
    assert!(
        blocks.iter().any(|b| matches!(
            b.kind,
            BlockKind::TableCell {
                row: 0,
                column: 0,
                header: true,
                ..
            }
        ) && b.text == "Band"),
        "no header cell among {} blocks",
        blocks.len()
    );
}

/// A window hands back the pages it asks for and nothing else.
#[test]
fn a_window_holds_the_pages_it_asks_for() {
    let document = document();
    let whole = document
        .extract_blocks(&ExtractOptions::default())
        .expect("whole document");
    let second = document
        .extract_blocks(&ExtractOptions::window(1, 1))
        .expect("second page");

    assert!(second.iter().all(|b| b.page == 1), "a block outside page 1");
    assert!(
        second.iter().any(|b| b.text == "Band"),
        "the table is not in the window"
    );
    assert!(
        !second.iter().any(|b| b.text == "Safety rules"),
        "page 0's heading is in a window that does not hold page 0"
    );
    assert_eq!(
        second.len(),
        whole.iter().filter(|b| b.page == 1).count(),
        "the window lost or gained blocks against the whole document"
    );
}

/// And the numbering in it is still the document's.
///
/// Two windows of one document have to agree about the same block, which
/// renumbering from zero per window is exactly what breaks.
#[test]
fn a_window_keeps_the_documents_own_numbering() {
    let document = document();
    let whole = document
        .extract_blocks(&ExtractOptions::default())
        .expect("whole document");
    let second = document
        .extract_blocks(&ExtractOptions::window(1, 1))
        .expect("second page");

    let from_whole: Vec<usize> = whole
        .iter()
        .filter(|b| b.page == 1)
        .map(|b| b.index)
        .collect();
    let from_window: Vec<usize> = second.iter().map(|b| b.index).collect();
    assert_eq!(from_whole, from_window);
    assert!(
        from_window.first().is_some_and(|&first| first > 0),
        "the second page opens at index 0, so the numbering was restarted"
    );
}

/// A window past the last page is empty, and so is one of no pages -- because
/// a caller stepping through a document a window at a time stops on an empty
/// answer, and an error there would read like a broken file.
#[test]
fn a_window_past_the_end_is_empty_rather_than_an_error() {
    let document = document();
    assert!(document
        .extract_blocks(&ExtractOptions::window(9, 4))
        .expect("a window past the end")
        .is_empty());
    assert!(document
        .extract_blocks(&ExtractOptions::window(0, 0))
        .expect("a window of no pages")
        .is_empty());
    assert!(document
        .extract_markdown(&ExtractOptions::window(9, 4))
        .expect("markdown past the end")
        .is_empty());
}

/// The default is the whole document, which is the case the goldens pin.
#[test]
fn the_default_is_every_page() {
    let document = document();
    let pages: std::collections::BTreeSet<usize> = document
        .extract_blocks(&ExtractOptions::default())
        .expect("whole document")
        .iter()
        .map(|b| b.page)
        .collect();
    assert_eq!(pages, [0, 1].into_iter().collect());
    assert_eq!(ExtractOptions::all(), ExtractOptions::default());
}

/// An explicit list of pages hands back those pages and nothing else, in the
/// document's reading order rather than the order the list names them.
///
/// DOCUMENT ORDER, NOT LIST ORDER. `[2, 0]` reads as "pages 0 and 2", the
/// same answer a window over those pages would give: the list selects and
/// the reading order stands, so `Block::index` keeps the document's
/// numbering and `Block::continues_previous` keeps its meaning. A list that
/// reordered would break both.
///
/// `fixtures/multi-page.pdf` has fifty pages, so `[2, 0]` is a genuine
/// out-of-order ask rather than the whole document backwards.
///
/// A page the document does not have is an error, where a window past the
/// end is empty: the list names its pages, and a name that is not there is
/// a mistake. Naming them beside a window is an error too.
#[test]
fn a_page_list_holds_the_pages_it_names_in_reading_order() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/multi-page.pdf");
    let bytes = std::fs::read(&path).expect("read multi-page.pdf");
    let document = PdfDocument::open(bytes).expect("open multi-page.pdf");
    assert_eq!(
        document.page_count(),
        50,
        "the fixture changed its page count"
    );

    let whole = document
        .extract_blocks(&ExtractOptions::default())
        .expect("whole document");
    let mut listed_options = ExtractOptions::default();
    listed_options.pages = Some(vec![2, 0]);
    let listed = document
        .extract_blocks(&listed_options)
        .expect("pages [2, 0]");

    assert!(
        listed.iter().all(|b| b.page == 0 || b.page == 2),
        "a block outside pages 0 and 2"
    );
    assert!(
        listed.iter().any(|b| b.page == 0) && listed.iter().any(|b| b.page == 2),
        "pages 0 and 2 are not both in the answer"
    );
    assert_eq!(
        listed.len(),
        whole.iter().filter(|b| b.page == 0 || b.page == 2).count(),
        "the list lost or gained blocks against the whole document"
    );
    let pages: Vec<usize> = listed.iter().map(|b| b.page).collect();
    let mut ordered = pages.clone();
    ordered.sort_unstable();
    assert_eq!(
        pages, ordered,
        "the list came back in list order rather than reading order"
    );

    let mut past_the_end = ExtractOptions::default();
    past_the_end.pages = Some(vec![50]);
    assert!(
        document.extract_blocks(&past_the_end).is_err(),
        "page 50 of a 50-page document is not an error"
    );
    let mut beside_a_window = ExtractOptions::default();
    beside_a_window.pages = Some(vec![0]);
    beside_a_window.page_count = Some(1);
    let err = document
        .extract_blocks(&beside_a_window)
        .expect_err("a list beside a window is not an error");
    // The conflict rides inside EngineError::Io; the message callers read
    // is the one the old EngineError variant carried (#574).
    assert_eq!(
        err.options(),
        Some(&OptionsError::ConflictingPageSelection),
        "a list beside a window is not an OptionsError"
    );
    assert_eq!(
        err.to_string(),
        "cannot combine pages with first_page/page_count: pass one or the other"
    );
}
