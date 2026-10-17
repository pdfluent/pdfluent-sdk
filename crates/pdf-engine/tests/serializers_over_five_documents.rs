//! Both serialisations of five documents, byte for byte (#572).
//!
//! Every page here is built from operators, so what is pinned is the whole
//! path: interpreter, spans, cut, paragraph merge, headings, lists, tables,
//! furniture, reading order, serializer. The golden files beside this one are
//! the contract -- change what `to_markdown` or `to_json` writes and all ten
//! go red at once, which is the only way a serializer with no single caller
//! inside the tree can be held still.
//!
//! WHAT EACH DOCUMENT IS FOR. `report` is three pages of a report: a running
//! head and a page number that must not reach the markdown, two heading
//! levels, the page marker, and a sentence the page break cut in two, which
//! must come back as one paragraph with no marker inside it. `handbook` is a
//! nested list under a heading. `tariff` is a ruled rectangular table, which
//! is a pipe table. `merged` is the same table with its header reaching over
//! two columns, which markdown cannot hold and HTML can. `notice` is a
//! two-column page with prose that opens like markup and must be escaped.
//!
//! THE GOLDEN FILES ARE REGENERATED, NOT EDITED: run the suite with
//! `PDFLUENT_WRITE_GOLDEN=1` and read the diff. A golden rewritten without
//! reading the diff is a test that agrees with whatever the code now does.
//!
//! THE COORDINATES IN THE JSON GOLDENS ARE EXACT, to the last digit a `f64`
//! prints. Everything between a Helvetica advance width and the number in the
//! file is addition, multiplication and division on `f64`, which IEEE-754
//! fixes on every platform, and Rust contracts none of it into a fused
//! multiply-add. So a golden that moves on another machine and not on this one
//! is not this test being brittle: it is a document whose geometry depends on
//! where it was read, in a product whose output is supposed to be the same
//! bytes everywhere.
//!
//! THE THIRD FORM IS DOCLING'S `DoclingDocument` (#629), golden as
//! `<name>.docling.json`. Those five files are also the fixture set
//! `crates/pdf-python/tests/test_docling_document.py` validates against the
//! pinned docling-core schema and round-trips through docling-core's own
//! model, so a golden that moves here is checked there too.
//!
//! AND THE LAST TWO CASES ARE NOT GOLDEN AT ALL. `the_seeded_documents_never_
//! panic` drives both serializers over documents drawn from one fixed seed --
//! empty pages, pages with nothing on them but an image, text at hostile
//! coordinates -- and checks that the JSON a parser gets back is JSON. A
//! golden cannot ask that question, because the answer is about the documents
//! nobody wrote down.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_json, to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};
use serde_json::Value;

const BODY: f64 = 10.0;
const TITLE: f64 = 16.0;
const SUBTITLE: f64 = 12.0;
const MARGIN: f64 = 72.0;
/// The top of the text frame, clear of the running head's band.
const TOP: f64 = 680.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;
/// One row of a table to the next.
const PITCH: f64 = 18.0;
const HEAD_Y: f64 = 720.0;
const FOOT_Y: f64 = 80.0;

/// The face a run of glyphs is set in.
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// A horizontal rule: a filled rectangle a third of a point high, which is how
/// a document rules a table.
struct Rule {
    left: f64,
    right: f64,
    y: f64,
}

/// One page: what is drawn on it, the rules under it, whether it carries the
/// image, and the box it is drawn in.
struct Page {
    pieces: Vec<Piece>,
    rules: Vec<Rule>,
    image: bool,
    width: f64,
    height: f64,
}

impl Page {
    fn new(pieces: Vec<Piece>) -> Self {
        Page {
            pieces,
            rules: Vec::new(),
            image: false,
            width: 612.0,
            height: 792.0,
        }
    }

    fn ruled(mut self, rules: Vec<Rule>) -> Self {
        self.rules = rules;
        self
    }
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

fn bold(body: &str, x: f64, y: f64, size: f64) -> Piece {
    piece(body, x, y, size, Face::Bold)
}

/// A column of body text set solid from `y`, at `x`.
fn column(lines: &[&str], x: f64, y: f64) -> Vec<Piece> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| text(line, x, y - i as f64 * LEADING))
        .collect()
}

/// An item marked with the WinAnsi bullet at nesting depth `depth`: the marker
/// in a run of its own, the text hanging off it.
fn bullet(body: &str, y: f64, depth: usize) -> Vec<Piece> {
    let left = MARGIN + depth as f64 * 28.0;
    vec![
        Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: left,
            y,
            size: BODY,
        },
        text(body, left + 14.0, y),
    ]
}

/// An item marked with `marker` -- `1.`, `2.` -- at the margin.
fn numbered(marker: &str, body: &str, y: f64) -> Vec<Piece> {
    vec![text(marker, MARGIN, y), text(body, MARGIN + 14.0, y)]
}

/// One row of a table: the cells at the column positions given.
fn row(cells: &[(&str, f64)], y: f64, face: Face) -> Vec<Piece> {
    cells
        .iter()
        .map(|(body, x)| piece(body, *x, y, BODY, face))
        .collect()
}

/// A rule the width of the table, drawn between the row at `index` and the one
/// above it, which is where a document rules a table.
fn rule_over(index: usize, right: f64) -> Rule {
    Rule {
        left: MARGIN - 6.0,
        right,
        y: TOP - index as f64 * PITCH + 11.0,
    }
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

/// A one-pixel grey image, which is all a page needs to have a picture on it
/// and no text at all.
fn image(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(1),
            "Height" => Object::Integer(1),
            "ColorSpace" => Object::Name(b"DeviceGray".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        },
        vec![0x80],
    ))))
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
    if page.image {
        operations.push(Operation::new("q", vec![]));
        operations.push(Operation::new(
            "cm",
            vec![
                200.into(),
                0.into(),
                0.into(),
                120.into(),
                MARGIN.into(),
                400.into(),
            ],
        ));
        operations.push(Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]));
        operations.push(Operation::new("Q", vec![]));
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

fn pdf_of(pages: &[Page]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let strong = font(&mut doc, b"Helvetica-Bold");
    let picture = image(&mut doc);

    let mut page_ids = Vec::new();
    for page in pages {
        let content_id =
            doc.add_object(Object::Stream(Stream::new(dictionary! {}, stream_of(page))));
        let mut resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
                "F2" => strong.clone(),
            }),
        };
        if page.image {
            resources.set(
                "XObject",
                Object::Dictionary(dictionary! { "Im0" => picture.clone() }),
            );
        }
        page_ids.push(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.into(), 0.into(), page.width.into(), page.height.into(),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
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
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

fn blocks_of(pages: &[Page]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pages))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

// ---------------------------------------------------------------- documents

/// Three pages of a report: the same line at the top of each, a page number at
/// the foot of each, two heading levels, and a sentence the second page break
/// cut in two.
fn report() -> Vec<Page> {
    let head = |page: usize, total: usize| {
        vec![
            text("Quarterly Report 2026", MARGIN, HEAD_Y),
            text(&format!("Page {} of {total}", page + 1), 290.0, FOOT_Y),
        ]
    };

    let mut first = head(0, 3);
    first.push(bold("Operations", MARGIN, TOP, TITLE));
    first.extend(column(
        &[
            "The plant ran to schedule through the quarter and the two",
            "lines commissioned in March reached their rated output in",
            "the first week of May.",
        ],
        MARGIN,
        TOP - 24.0,
    ));
    first.push(bold("Maintenance", MARGIN, TOP - 78.0, SUBTITLE));
    first.extend(column(
        &[
            "The shutdown planned for August was brought forward to July",
            "so that it would fall in the same week as the inspection",
        ],
        MARGIN,
        TOP - 96.0,
    ));

    let mut second = head(1, 3);
    second.extend(column(
        &[
            "already booked for the boiler house, and the two were carried",
            "out together over five days.",
        ],
        MARGIN,
        TOP,
    ));
    second.push(bold("Finance", MARGIN, TOP - 42.0, TITLE));
    second.extend(column(
        &[
            "Revenue for the quarter was ahead of the budget laid before",
            "the board in January, and the whole of the difference is the",
            "contract signed in February.",
        ],
        MARGIN,
        TOP - 66.0,
    ));

    let mut third = head(2, 3);
    third.extend(column(
        &[
            "No further business was brought before the meeting, which",
            "rose at four o'clock.",
        ],
        MARGIN,
        TOP,
    ));

    vec![Page::new(first), Page::new(second), Page::new(third)]
}

/// A heading, a bulleted list with a nested pair inside it, and a numbered
/// list under that.
fn handbook() -> Vec<Page> {
    let mut pieces = vec![bold("Safety rules", MARGIN, TOP, TITLE)];
    pieces.extend(bullet("Wear the issued boots on the floor.", TOP - 24.0, 0));
    pieces.extend(bullet("Keep the gangways clear of stock.", TOP - 36.0, 0));
    pieces.extend(bullet(
        "Pallets are stacked two high, no more.",
        TOP - 48.0,
        1,
    ));
    pieces.extend(bullet(
        "Drums are stacked one high, on a mat.",
        TOP - 60.0,
        1,
    ));
    pieces.extend(bullet("Report a spill before you clean it.", TOP - 72.0, 0));
    pieces.extend(numbered(
        "1.",
        "Stop the line at the nearest pull cord.",
        TOP - 108.0,
    ));
    pieces.extend(numbered(
        "2.",
        "Tell the shift supervisor at once.",
        TOP - 120.0,
    ));
    pieces.extend(numbered(
        "3.",
        "Write the entry in the day book.",
        TOP - 132.0,
    ));
    vec![Page::new(pieces)]
}

/// The columns of a ruled table that IS a rectangle: a pipe table.
fn tariff() -> Vec<Page> {
    let columns = [MARGIN, 220.0, 360.0];
    let mut pieces = row(
        &[
            ("Band", columns[0]),
            ("Units", columns[1]),
            ("Rate", columns[2]),
        ],
        TOP,
        Face::Bold,
    );
    for (index, cells) in [
        ["Standing", "none", "18.90"],
        ["Day", "0 to 500", "24.31"],
        ["Night", "0 to 500", "11.07"],
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[
                (cells[0], columns[0]),
                (cells[1], columns[1]),
                (cells[2], columns[2]),
            ],
            TOP - (index + 1) as f64 * PITCH,
            Face::Regular,
        ));
    }
    let rules = (0..5).map(|i| rule_over(i, 430.0)).collect();
    vec![Page::new(pieces).ruled(rules)]
}

/// A table whose header reaches over two of its columns: markdown has no way
/// to say that and HTML has, so this one comes out as HTML.
///
/// Unruled, which is the commoner picture and the one the column consensus of
/// #570 has to carry on its own.
fn merged() -> Vec<Page> {
    let mut pieces = row(
        &[("Region", MARGIN), ("Reported figures", 240.0)],
        TOP,
        Face::Bold,
    );
    for (index, cells) in [
        ["North", "120", "41%"],
        ["South", "95", "32%"],
        ["East", "78", "27%"],
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(cells[0], MARGIN), (cells[1], 240.0), (cells[2], 360.0)],
            TOP - (index + 1) as f64 * PITCH,
            Face::Regular,
        ));
    }
    vec![Page::new(pieces)]
}

/// Two columns under a full-width title, with prose that opens like markup in
/// each of them.
fn notice() -> Vec<Page> {
    let mut pieces = vec![bold("Notice to occupiers", MARGIN, TOP, TITLE)];
    pieces.extend(column(
        &[
            "# 14 and # 16 are served by the",
            "rear staircase while the lift is",
            "out of use. The work is expected",
            "to run to the end of the month.",
        ],
        MARGIN,
        TOP - 30.0,
    ));
    pieces.extend(column(
        &[
            "- deliveries are taken at the",
            "front desk until six o'clock,",
            "and 1998 was the last year the",
            "yard gate was in use at all.",
        ],
        360.0,
        TOP - 30.0,
    ));
    vec![Page::new(pieces)]
}

// ------------------------------------------------------------------ goldens

/// The five, by the name their golden files carry.
fn documents() -> Vec<(&'static str, Vec<Page>)> {
    vec![
        ("report", report()),
        ("handbook", handbook()),
        ("tariff", tariff()),
        ("merged", merged()),
        ("notice", notice()),
    ]
}

fn golden_path(name: &str, extension: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.{extension}"))
}

/// The golden file's contents against what the serializer wrote now.
///
/// With `PDFLUENT_WRITE_GOLDEN=1` in the environment it writes instead of
/// comparing, and the diff is what the author reads before committing.
fn against_golden(name: &str, extension: &str, produced: &str) {
    let path = golden_path(name, extension);
    if std::env::var_os("PDFLUENT_WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden directory")).expect("make directory");
        std::fs::write(&path, produced).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("no golden at {}: {why}", path.display()));
    assert_eq!(
        produced,
        expected,
        "{} has moved. Read the diff; regenerate with PDFLUENT_WRITE_GOLDEN=1 only once you \
         have decided the new output is right.",
        path.display()
    );
}

#[test]
fn the_markdown_of_five_documents_is_what_it_was() {
    for (name, pages) in documents() {
        against_golden(name, "md", &to_markdown(&blocks_of(&pages)));
    }
}

#[test]
fn the_json_of_five_documents_is_what_it_was() {
    for (name, pages) in documents() {
        against_golden(name, "json", &to_json(&blocks_of(&pages)));
    }
}

fn docling_of(name: &str, pages: &[Page]) -> String {
    PdfDocument::open(pdf_of(pages))
        .expect("open pdf")
        .extract_docling(name, &ExtractOptions::default())
        .expect("extract docling")
}

#[test]
fn the_docling_json_of_five_documents_is_what_it_was() {
    for (name, pages) in documents() {
        against_golden(name, "docling.json", &docling_of(name, &pages));
    }
}

/// The node a Docling `cref` names, `#/texts/3` or `#/body`.
fn resolve<'a>(document: &'a Value, reference: &str) -> Option<&'a Value> {
    document.pointer(reference.strip_prefix('#')?)
}

/// Every reference in a `DoclingDocument` names a node, and the tree is one
/// tree: each child's `parent` is the node that lists it, each node is listed
/// by exactly one parent, and every item is reached from one of the two roots.
/// docling-core's loader does not check the second and third, and a document
/// that breaks them loads and then walks wrong.
fn assert_one_tree(name: &str, document: &Value) {
    let mut listed: std::collections::BTreeMap<String, usize> = Default::default();
    let mut pending = vec!["#/body".to_string(), "#/furniture".to_string()];
    while let Some(reference) = pending.pop() {
        let node = resolve(document, &reference)
            .unwrap_or_else(|| panic!("{name}: {reference} names no node"));
        for child in node["children"].as_array().expect("children") {
            let child_ref = child["cref"].as_str().expect("cref").to_string();
            let child_node = resolve(document, &child_ref)
                .unwrap_or_else(|| panic!("{name}: {child_ref} names no node"));
            assert_eq!(
                child_node["parent"]["cref"].as_str(),
                Some(reference.as_str()),
                "{name}: {child_ref} does not name {reference} as its parent"
            );
            *listed.entry(child_ref.clone()).or_default() += 1;
            pending.push(child_ref);
        }
    }
    for collection in ["texts", "groups", "tables"] {
        for (i, node) in document[collection]
            .as_array()
            .expect(collection)
            .iter()
            .enumerate()
        {
            let reference = format!("#/{collection}/{i}");
            assert_eq!(
                node["self_ref"].as_str(),
                Some(reference.as_str()),
                "{name}"
            );
            assert_eq!(
                listed.get(&reference).copied(),
                Some(1),
                "{name}: {reference} is not listed by exactly one parent"
            );
        }
    }
}

#[test]
fn the_golden_docling_json_is_one_tree_of_resolving_references() {
    for (name, _) in documents() {
        let path = golden_path(name, "docling.json");
        let text = std::fs::read_to_string(&path).expect("read golden");
        let parsed: Value = serde_json::from_str(&text)
            .unwrap_or_else(|why| panic!("{} is not JSON: {why}", path.display()));
        assert_eq!(parsed["schema_name"], "DoclingDocument", "{name}");
        assert_one_tree(name, &parsed);
    }
}

/// What each Docling fixture must hold, so a golden regenerated over a broken
/// mapping cannot pass by agreeing with itself.
#[test]
fn the_docling_fixtures_hold_what_each_document_is_for() {
    let report: Value = serde_json::from_str(&docling_of("report", &report())).expect("json");
    let labels: Vec<&str> = report["texts"]
        .as_array()
        .expect("texts")
        .iter()
        .map(|t| t["label"].as_str().expect("label"))
        .collect();
    assert!(labels.contains(&"section_header"), "{labels:?}");
    assert!(labels.contains(&"page_header"), "{labels:?}");
    assert!(labels.contains(&"page_footer"), "{labels:?}");
    assert_eq!(report["pages"].as_object().expect("pages").len(), 3);
    assert!(
        report["texts"]
            .as_array()
            .expect("texts")
            .iter()
            .any(|t| t["prov"].as_array().expect("prov").len() == 2),
        "the paragraph the page break cut in two is not one item over two pages"
    );
    assert!(
        !report["furniture"]["children"]
            .as_array()
            .expect("furniture")
            .is_empty(),
        "the running heads are not under furniture"
    );

    let handbook: Value = serde_json::from_str(&docling_of("handbook", &handbook())).expect("json");
    let nested = handbook["texts"]
        .as_array()
        .expect("texts")
        .iter()
        .any(|t| t["label"] == "list_item" && !t["children"].as_array().expect("c").is_empty());
    assert!(nested, "no list item carries its sub-list");

    let merged: Value = serde_json::from_str(&docling_of("merged", &merged())).expect("json");
    let data = &merged["tables"][0]["data"];
    assert!(
        data["table_cells"]
            .as_array()
            .expect("cells")
            .iter()
            .any(|c| c["col_span"].as_u64() == Some(2) && c["column_header"] == true),
        "the header spanning two columns is lost: {data}"
    );
}

/// The same blocks, twice, in one process: the bytes are the same bytes.
///
/// Not the same claim as the goldens. A serializer that walked a `HashMap`
/// would pass a golden written on the same machine in the same run and fail
/// this, and a reader of the goldens could not tell which of the two had
/// happened.
#[test]
fn the_same_blocks_write_the_same_bytes_twice() {
    for (name, pages) in documents() {
        let blocks = blocks_of(&pages);
        assert_eq!(
            to_markdown(&blocks),
            to_markdown(&blocks),
            "{name} markdown"
        );
        assert_eq!(to_json(&blocks), to_json(&blocks), "{name} json");
    }
}

/// Every golden JSON is JSON, read by a parser that is not ours.
#[test]
fn the_golden_json_parses() {
    for (name, _) in documents() {
        let path = golden_path(name, "json");
        let text = std::fs::read_to_string(&path).expect("read golden");
        let parsed: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|why| panic!("{} is not JSON: {why}", path.display()));
        assert!(parsed["blocks"].is_array(), "{name} has no blocks array");
    }
}

/// The running head is in the JSON and not in the markdown, which is the whole
/// difference between the two forms and the reason the blocks are MARKED as
/// furniture rather than dropped (#571).
#[test]
fn the_furniture_is_in_one_form_and_not_the_other() {
    let blocks = blocks_of(&report());
    let markdown = to_markdown(&blocks);
    let json = to_json(&blocks);
    assert!(
        !markdown.contains("Quarterly Report 2026"),
        "the running head reached the markdown: {markdown}"
    );
    assert!(json.contains("Quarterly Report 2026"), "{json}");
    assert!(json.contains("\"kind\": \"header\""), "{json}");
    assert!(json.contains("\"kind\": \"footer\""), "{json}");
}

// --------------------------------------------------------------- the seed

/// The seed both fuzz cases are drawn from. One number, written down, so that
/// a failure on any machine is a failure every machine can reproduce.
const SEED: u64 = 0x0000_0572_0001_9A3B;

/// How many documents the seed draws.
const DRAWS: usize = 120;

/// xorshift64*, four lines and no dependency. The quality of the stream does
/// not matter here -- what matters is that it is the SAME stream everywhere,
/// which a hashing PRNG out of the standard library is not promised to be.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    /// A float in `low..high`, to a hundredth.
    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (self.next() % 10_000) as f64 / 10_000.0 * (high - low)
    }
}

/// The words a drawn page is set from, including the ones that mean something
/// to markdown.
const WORDS: [&str; 12] = [
    "alpha",
    "# beta",
    "- gamma",
    "1. delta",
    "|epsilon|",
    "```zeta",
    "eta",
    "<theta>",
    "iota\u{00a0}kappa",
    "λ",
    "mu-",
    "",
];

/// One page drawn from the stream: empty, a picture and nothing else, or text
/// at coordinates nobody would set type at.
fn drawn_page(rng: &mut Rng) -> Page {
    match rng.below(4) {
        // A page with nothing on it at all.
        0 => Page::new(Vec::new()),
        // A page with nothing on it but a picture: no glyph, no block, and the
        // serializers still have to answer.
        1 => {
            let mut page = Page::new(Vec::new());
            page.image = true;
            page
        }
        _ => {
            let count = 1 + rng.below(8);
            let mut pieces = Vec::new();
            for _ in 0..count {
                let word = WORDS[rng.below(WORDS.len())];
                pieces.push(piece(
                    word,
                    rng.between(-200.0, 800.0),
                    rng.between(-200.0, 1000.0),
                    rng.between(0.01, 400.0),
                    if rng.below(2) == 0 {
                        Face::Regular
                    } else {
                        Face::Bold
                    },
                ));
            }
            let mut page = Page::new(pieces);
            page.image = rng.below(3) == 0;
            page
        }
    }
}

/// A page with nothing on it, written down rather than left to the draw:
/// it serialises to nothing.
#[test]
fn an_empty_page_serialises_to_nothing() {
    let pages = vec![Page::new(Vec::new())];
    let blocks = blocks_of(&pages);
    assert!(blocks.is_empty(), "a page with no glyph has no block");
    assert_eq!(to_markdown(&blocks), "");
    assert_eq!(to_json(&blocks), "{\n  \"blocks\": []\n}\n");
}

/// A page with nothing on it but an image, written down rather than left to
/// the draw: since 53a5c406a a textless scan carries its image figures, so
/// it serialises to exactly one figure block, never to nothing.
#[test]
fn an_image_only_page_serialises_to_one_figure() {
    let mut picture = Page::new(Vec::new());
    picture.image = true;
    let blocks = blocks_of(&[picture]);
    assert_eq!(blocks.len(), 1, "one figure block: {blocks:?}");
    assert!(
        matches!(blocks[0].kind, BlockKind::Figure),
        "the block is a figure: {:?}",
        blocks[0].kind
    );
    assert_eq!(to_markdown(&blocks), "![](figure-0001-01.png)\n");
}

/// Documents drawn from `SEED`: neither serializer panics, and what the JSON
/// one writes is JSON.
///
/// The parse is the point. "It did not panic" is satisfied by a serializer
/// that writes a control character straight into a string or `NaN` where a
/// number should be -- both of which produce a file that every consumer
/// rejects and no assertion inside this crate would notice.
#[test]
fn the_seeded_documents_never_panic_and_always_parse() {
    let mut rng = Rng(SEED);
    for draw in 0..DRAWS {
        let pages: Vec<Page> = (0..1 + rng.below(3))
            .map(|_| drawn_page(&mut rng))
            .collect();
        let blocks = match PdfDocument::open(pdf_of(&pages)) {
            Ok(document) => document
                .extract_blocks(&ExtractOptions::default())
                .unwrap_or_default(),
            Err(_) => continue,
        };
        let markdown = to_markdown(&blocks);
        assert!(
            !markdown.contains('\u{0}'),
            "seed {SEED:#x} draw {draw}: a NUL reached the markdown"
        );
        let json = to_json(&blocks);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .unwrap_or_else(|why| panic!("seed {SEED:#x} draw {draw}: not JSON: {why}\n{json}"));
        assert!(
            parsed["blocks"].is_array(),
            "seed {SEED:#x} draw {draw}: no blocks array"
        );
        let docling = pdf_engine::to_docling_json("seeded", &[], &blocks);
        let parsed: Value = serde_json::from_str(&docling).unwrap_or_else(|why| {
            panic!("seed {SEED:#x} draw {draw}: docling not JSON: {why}\n{docling}")
        });
        assert_one_tree("seeded", &parsed);
    }
}
