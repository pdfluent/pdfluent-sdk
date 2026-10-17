// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The engine's table inside a given region (#574, T1's teacher adapter).
//!
//! A teacher table region with no matching engine table falls back to one HTML
//! cell; the engine builds its real table there instead, through the same
//! rebuild the learned pass emits for a chosen region. Take `rebuild_region`
//! out of `refine_judged` and the pass and the region entry drift; take the
//! line rule or the pad out of `table_in_region` and a loose box halves a word
//! or a tight one drops a header line.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

/// A thin line: a filled rectangle 0.5 pt across, as producers draw rulings.
fn rule(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
    let (w, h) = if (y1 - y0).abs() < 1e-9 {
        (x1 - x0, 0.5)
    } else {
        (0.5, y1 - y0)
    };
    vec![
        Operation::new("re", vec![x0.into(), y0.into(), w.into(), h.into()]),
        Operation::new("f", vec![]),
    ]
}

fn run(ops: &mut Vec<Operation>, body: &str, x: f64, y: f64, size: i64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new(
        "Tj",
        vec![Object::String(
            body.as_bytes().to_vec(),
            StringFormat::Literal,
        )],
    ));
    ops.push(Operation::new("ET", vec![]));
}

fn pdf_of(ops: Vec<Operation>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
        }),
    }));
    let pages = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
        p.set("Parent", Object::Reference(pages));
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// A ruled table, isolated on its page: a box round the lot, a line between
/// every row and column, and a head spanning all three because its inner lines
/// stop under it.
fn ruled_pdf() -> Vec<u8> {
    let xs = [72.0, 172.0, 372.0, 540.0];
    let ys = [700.0, 680.0, 640.0, 600.0, 560.0];
    let mut ops = Vec::new();
    for &y in &ys {
        ops.extend(rule(xs[0], y, xs[3], y));
    }
    ops.extend(rule(xs[0], ys[4], xs[0], ys[0]));
    ops.extend(rule(xs[3], ys[4], xs[3], ys[0]));
    ops.extend(rule(xs[1], ys[4], xs[1], ys[1]));
    ops.extend(rule(xs[2], ys[4], xs[2], ys[1]));
    run(&mut ops, "Feed materials of plant origin", 76.0, 686.0, 9);
    for (k, (a, b, c, d)) in [
        (
            "1.1.1",
            "Barley",
            "Grains of Hordeum vulgare L.",
            "It may be",
        ),
        (
            "1.1.2",
            "Barley, puffed",
            "Product obtained from milled",
            "or broken barley",
        ),
        (
            "1.1.3",
            "Barley, toasted",
            "Product of barley roasting",
            "partially burnt",
        ),
    ]
    .iter()
    .enumerate()
    {
        let top = ys[k + 1] - 14.0;
        run(&mut ops, a, 76.0, top, 9);
        run(&mut ops, b, 76.0, top - 12.0, 9);
        run(&mut ops, c, 176.0, top, 9);
        run(&mut ops, d, 376.0, top, 9);
    }
    pdf_of(ops)
}

/// A borderless table, isolated on its page: three columns of figures under a
/// head row, nothing drawn.
fn unruled_pdf() -> Vec<u8> {
    let cols = [72.0, 250.0, 400.0];
    let mut ops = Vec::new();
    let mut y = 650.0;
    for (a, b, c) in [
        ("Item", "2024", "2023"),
        ("Revenue", "13,110", "12,166"),
        ("Cost of sales", "(8,406)", "(7,737)"),
        ("Gross profit", "4,704", "4,429"),
        ("Selling expenses", "(3,145)", "(2,963)"),
        ("Net income", "1,559", "1,466"),
    ] {
        for (x, body) in cols.iter().zip([a, b, c]) {
            run(&mut ops, body, *x, y, 10);
        }
        y -= 18.0;
    }
    pdf_of(ops)
}

/// Four lines of running text, no gutters, no rules.
fn prose_pdf() -> Vec<u8> {
    let mut ops = Vec::new();
    for (i, line) in [
        "The quick brown fox jumps over the lazy dog and keeps going.",
        "Pack my box with five dozen liquor jugs and a few more words.",
        "How vexingly quick daft zebras jump over the brown fox nearby.",
        "The five boxing wizards jump quickly over the lazy dogs outside.",
    ]
    .iter()
    .enumerate()
    {
        run(&mut ops, line, 72.0, 700.0 - i as f64 * 14.0, 10);
    }
    pdf_of(ops)
}

/// One cell's reading: where it sits, what it says, and the spans it was made
/// of.
fn cell_key(block: &pdf_engine::Block) -> Option<(u16, u16, u16, u16, String, Vec<String>)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            ..
        } => Some((
            row,
            column,
            row_span,
            column_span,
            block.text.clone(),
            block.spans.iter().map(|s| s.text.clone()).collect(),
        )),
        _ => None,
    }
}

fn cells_sorted(blocks: &[pdf_engine::Block]) -> Vec<(u16, u16, u16, u16, String, Vec<String>)> {
    let mut cells: Vec<_> = blocks.iter().filter_map(cell_key).collect();
    cells.sort();
    cells
}

fn union_of(blocks: &[pdf_engine::Block]) -> [f64; 4] {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for block in blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
    {
        bbox[0] = bbox[0].min(block.bbox[0]);
        bbox[1] = bbox[1].min(block.bbox[1]);
        bbox[2] = bbox[2].max(block.bbox[2]);
        bbox[3] = bbox[3].max(block.bbox[3]);
    }
    bbox
}

/// The region entry returns the pass's own cells for the table's own box: a
/// ruled grid read from its lines.
#[test]
fn a_ruled_table_in_its_own_box_is_the_pass_cells() {
    let doc = PdfDocument::open(ruled_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        !cells_sorted(&blocks).is_empty(),
        "the page holds no table at all"
    );
    let bbox = union_of(&blocks);
    let region = doc.table_in_region(0, bbox).expect("region");
    assert_eq!(
        cells_sorted(&region),
        cells_sorted(&blocks),
        "the region rebuild drifted from the pass"
    );
}

/// The same, borderless: the decoder's grid over the same words.
#[test]
fn an_unruled_table_in_its_own_box_is_the_pass_cells() {
    let doc = PdfDocument::open(unruled_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        !cells_sorted(&blocks).is_empty(),
        "the page holds no table at all"
    );
    let bbox = union_of(&blocks);
    let region = doc.table_in_region(0, bbox).expect("region");
    assert_eq!(
        cells_sorted(&region),
        cells_sorted(&blocks),
        "the region rebuild drifted from the pass"
    );
}

/// The same on a real page where the judge chooses the table: the crystal page
/// the key-value split was written for, decoded and split as the pass emits
/// it.
#[test]
fn a_judged_table_in_its_own_box_is_the_pass_cells() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pubtables/pmc2959354_p3_crystal_data.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let want = cells_sorted(&blocks);
    assert!(!want.is_empty(), "the page holds no table at all");
    let bbox = union_of(&blocks);
    let region = doc.table_in_region(0, bbox).expect("region");
    assert_eq!(
        cells_sorted(&region),
        want,
        "the region rebuild drifted from the judged table"
    );
}

/// A loose box and a tight one read the same ruled table: the line rule keeps
/// a word the loose box cuts, and the pad keeps a header line the tight box
/// clips.
#[test]
fn a_loose_and_a_tight_box_read_the_same_ruled_table() {
    let doc = PdfDocument::open(ruled_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    let loose = [
        bbox[0] - 10.0,
        bbox[1] - 10.0,
        bbox[2] + 10.0,
        bbox[3] + 10.0,
    ];
    let tight = [bbox[0] + 2.0, bbox[1] + 2.0, bbox[2] - 2.0, bbox[3] - 2.0];
    let exact = cells_sorted(&doc.table_in_region(0, bbox).expect("exact"));
    assert!(!exact.is_empty(), "the exact box holds no table");
    assert_eq!(
        cells_sorted(&doc.table_in_region(0, loose).expect("loose")),
        exact,
        "a loose box halved or dropped a word"
    );
    assert_eq!(
        cells_sorted(&doc.table_in_region(0, tight).expect("tight")),
        exact,
        "a tight box dropped a line"
    );
}

/// The same, borderless.
#[test]
fn a_loose_and_a_tight_box_read_the_same_unruled_table() {
    let doc = PdfDocument::open(unruled_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    let loose = [
        bbox[0] - 10.0,
        bbox[1] - 10.0,
        bbox[2] + 10.0,
        bbox[3] + 10.0,
    ];
    let tight = [bbox[0] + 2.0, bbox[1] + 2.0, bbox[2] - 2.0, bbox[3] - 2.0];
    let exact = cells_sorted(&doc.table_in_region(0, bbox).expect("exact"));
    assert!(!exact.is_empty(), "the exact box holds no table");
    assert_eq!(
        cells_sorted(&doc.table_in_region(0, loose).expect("loose")),
        exact,
        "a loose box halved or dropped a word"
    );
    assert_eq!(
        cells_sorted(&doc.table_in_region(0, tight).expect("tight")),
        exact,
        "a tight box dropped a line"
    );
}

/// A box over prose answers without panicking, and a box over nothing answers
/// empty.
#[test]
fn prose_answers_and_nothing_is_empty() {
    let doc = PdfDocument::open(prose_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for block in &blocks {
        bbox[0] = bbox[0].min(block.bbox[0]);
        bbox[1] = bbox[1].min(block.bbox[1]);
        bbox[2] = bbox[2].max(block.bbox[2]);
        bbox[3] = bbox[3].max(block.bbox[3]);
    }
    let _ = doc.table_in_region(0, bbox).expect("prose answers");
    let empty = doc
        .table_in_region(0, [500.0, 50.0, 550.0, 80.0])
        .expect("nothing answers");
    assert!(empty.is_empty(), "nothing came back as a table: {empty:?}");
}

/// The region's table through the markdown writer, as the example prints it:
/// HTML where the shape needs it, and nothing else on the page.
#[test]
fn a_region_prints_through_the_markdown_writer() {
    let doc = PdfDocument::open(ruled_pdf()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let region = doc.table_in_region(0, union_of(&blocks)).expect("region");
    let markdown = pdf_engine::to_markdown(&region);
    assert!(
        markdown.contains("<table"),
        "a spanning head prints as HTML: {markdown:?}"
    );
    assert!(
        !markdown.contains("page"),
        "nothing else on the page prints: {markdown:?}"
    );
}
