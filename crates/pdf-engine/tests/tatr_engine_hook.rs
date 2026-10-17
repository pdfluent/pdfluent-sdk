// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! TATR as the first structure stage inside `rebuild_region` (track B, P3).
//!
//! The hook offers every table region to TATR before the current grid runs:
//! where it fires its cells are the region's table, and where it declines,
//! or the region fails, the grid runs unchanged. The fake seam
//! (`TatrTestHook`) proves the wiring without a model or a dylib, on every
//! build; the real structurer is exercised behind `ort-tables`, against the
//! pinned model where one is installed, and skipped loudly otherwise.
//!
//! Take the hook out of `rebuild_region` and the fake's cells never land;
//! take the fallback out and a declining structurer stops reading as the
//! base.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
#[cfg(not(feature = "ort-tables"))]
use pdf_engine::api_error::PdfError;
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument, TatrOptions, TatrTestCell, TatrTestHook};

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

/// Helvetica advance widths in units per em, for the twin's glyph advances.
fn helv_width(ch: char) -> f64 {
    match ch {
        ' ' => 0.278,
        '0'..='9' => 0.556,
        'A' | 'B' | 'E' | 'K' | 'N' | 'R' | 'S' | 'X' | 'Y' | 'Z' => 0.667,
        'C' | 'D' | 'H' | 'U' => 0.722,
        'F' | 'T' => 0.611,
        'G' | 'O' | 'Q' => 0.778,
        'I' => 0.278,
        'J' => 0.500,
        'L' => 0.556,
        'M' => 0.833,
        'P' => 0.667,
        'V' => 0.667,
        'W' => 0.944,
        'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' => 0.556,
        'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 0.500,
        'f' | 't' => 0.278,
        'i' | 'j' | 'l' => 0.222,
        'm' | 'w' => 0.833,
        'r' => 0.333,
        '.' | ',' => 0.278,
        '(' | ')' | '-' => 0.333,
        _ => 0.556,
    }
}

/// One glyph per show operator, as a character-spaced PDF sets text: each
/// glyph advances past its own ink, words part by a real space advance, and
/// the shade alternates per glyph so the engine keeps one span per glyph --
/// same-style neighbours would merge into word spans before the fill ever
/// sees them, which is what the twin is a twin against. Words carrying
/// punctuation ride one show operator whole: the engine's consensus spaces
/// a digit-to-punctuation boundary across operators, which would make the
/// twin read differently at the span level rather than the fill level.
fn glyph_run(ops: &mut Vec<Operation>, body: &str, x: f64, y: f64, size: i64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    let em = size as f64;
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    let mut dark = false;
    let mut face = |ops: &mut Vec<Operation>| {
        dark = !dark;
        ops.push(Operation::new(
            "g",
            vec![if dark { 0.0.into() } else { 0.03.into() }],
        ));
    };
    let mut words = body.split(' ').peekable();
    while let Some(word) = words.next() {
        if word.chars().all(|c| c.is_ascii_alphanumeric()) {
            for ch in word.chars() {
                face(ops);
                ops.push(Operation::new(
                    "Tj",
                    vec![Object::String(vec![ch as u8], StringFormat::Literal)],
                ));
                let advance = helv_width(ch) * em;
                ops.push(Operation::new("Td", vec![advance.into(), 0.into()]));
            }
        } else {
            face(ops);
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    word.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ));
            let advance: f64 = word.chars().map(|c| helv_width(c) * em).sum();
            ops.push(Operation::new("Td", vec![advance.into(), 0.into()]));
        }
        if words.peek().is_some() {
            let advance = helv_width(' ') * em + em * 0.30;
            ops.push(Operation::new("Td", vec![advance.into(), 0.into()]));
        }
    }
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

/// The ruled table's twin, set glyph by glyph: the same rulings, the same
/// words at the same starts, but one show operator per glyph, as a
/// character-spaced PDF sets text (letter-spacing, per-glyph Tj).
fn char_spaced_pdf() -> Vec<u8> {
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
    glyph_run(&mut ops, "Feed materials of plant origin", 76.0, 686.0, 9);
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
        glyph_run(&mut ops, a, 76.0, top, 9);
        glyph_run(&mut ops, b, 76.0, top - 12.0, 9);
        glyph_run(&mut ops, c, 176.0, top, 9);
        glyph_run(&mut ops, d, 376.0, top, 9);
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

/// A ruled 3x3 with tall rows and a prose strip inside the box: three fox
/// lines at the right, in the rows' text gaps, one region with the table.
///
/// The lattice run takes every block whose centre falls in the box, prose
/// included, so the region holds 22 words -- 11 table words interleaved
/// with 11 fox words in page order -- and a TATR result that claims the
/// table words must hand the fox lines back as the three paragraphs they
/// are. The strip stands past the table words' right edge, so it is outside
/// the claimed extent and counts neither for coverage nor for the gap fill.
fn strip_pdf() -> Vec<u8> {
    let (left, right) = (60.0, 590.0);
    let tops = [700.0, 633.0, 567.0, 500.0];
    let mut ops = Vec::new();
    for &y in &tops {
        ops.extend(rule(left, y, right, y));
    }
    for &x in &[60.0, 200.0, 340.0, 480.0, 590.0] {
        ops.extend(rule(x, 500.0, x, 700.0));
    }
    for (r, (a, b, c)) in [
        ("Item", "2024", "2023"),
        ("Revenue", "13,110", "12,166"),
        ("Cost of sales", "(8,406)", "(7,737)"),
    ]
    .iter()
    .enumerate()
    {
        for (x, body) in [64.0, 204.0, 344.0].iter().zip([a, b, c]) {
            run(&mut ops, body, *x, tops[r] - 14.0, 10);
        }
    }
    for (body, y) in [
        ("Fox jumps far away", 650.0),
        ("Daft zebras run fast", 583.0),
        ("Pack boxes quickly", 517.0),
    ] {
        run(&mut ops, body, 486.0, y, 9);
    }
    pdf_of(ops)
}

/// One run drawn at 90 degrees, baseline up the page, the way a landscape
/// table sets its words: a rotated text matrix per run.
fn run90(ops: &mut Vec<Operation>, body: &str, x: f64, y: f64, size: i64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    ops.push(Operation::new(
        "Tm",
        vec![
            0.0.into(),
            1.0.into(),
            (-1.0).into(),
            0.0.into(),
            x.into(),
            y.into(),
        ],
    ));
    ops.push(Operation::new(
        "Tj",
        vec![Object::String(
            body.as_bytes().to_vec(),
            StringFormat::Literal,
        )],
    ));
    ops.push(Operation::new("ET", vec![]));
}

/// A ruled 3x3 turned 90 degrees on an upright page: the box, the row and
/// column rules and the cell words of a ruled table, all drawn under a
/// rotated text matrix, the way a journal sets a landscape table on a
/// portrait page. Reading-frame `(rx, ry)` lands at page `(500 - ry, 50 +
/// rx)` -- a reading row rule is a vertical page rule, a reading column rule
/// a horizontal one -- baseline up the page. The upright body above it keeps
/// the page's own direction upright, so the turned runs assemble along their
/// own baseline (#574) and keep their rotated text matrix.
fn rotated_pdf() -> Vec<u8> {
    let (tx, ty) = (500.0, 50.0);
    let at = |rx: f64, ry: f64| (tx - ry, ty + rx);
    let rxs = [72.0, 200.0, 330.0, 460.0];
    let rys = [200.0, 182.0, 164.0, 146.0];
    let mut ops = Vec::new();
    for (i, line) in [
        "The body of the page is written upright and holds more letters than anything turned",
        "A second line of upright body text keeps the page direction horizontal throughout",
        "And a third line still, so the turned table below stays the clear minority here",
    ]
    .iter()
    .enumerate()
    {
        run(&mut ops, line, 72.0, 740.0 - i as f64 * 14.0, 10);
    }
    // Reading row rules run along rx: vertical rules on the page.
    for &ry in &rys {
        let (x, y0) = at(rxs[0], ry);
        let (_, y1) = at(rxs[3], ry);
        ops.extend(rule(x, y0.min(y1), x, y0.max(y1)));
    }
    // Reading column rules run along ry: horizontal rules on the page.
    for &rx in &rxs {
        let (x0, y) = at(rx, rys[0]);
        let (x1, _) = at(rx, rys[3]);
        ops.extend(rule(x0.min(x1), y, x0.max(x1), y));
    }
    for (r, (a, b, c)) in [
        ("Item", "2024", "2023"),
        ("Revenue", "13,110", "12,166"),
        ("Cost of sales", "(8,406)", "(7,737)"),
    ]
    .iter()
    .enumerate()
    {
        let ry = rys[r] - 13.0;
        for (column, body) in [a, b, c].iter().enumerate() {
            let (x, y) = at(rxs[column] + 4.0, ry);
            run90(&mut ops, body, x, y, 10);
        }
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

/// One cell's reading: where it sits, what it says, and its header flag.
fn cell_key(block: &pdf_engine::Block) -> Option<(u16, u16, u16, u16, bool, String)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => Some((
            row,
            column,
            row_span,
            column_span,
            header,
            block.text.clone(),
        )),
        _ => None,
    }
}

fn cells_sorted(blocks: &[pdf_engine::Block]) -> Vec<(u16, u16, u16, u16, bool, String)> {
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

/// The base output: no option, the current grid.
fn base_of(pdf: &[u8]) -> (Vec<pdf_engine::Block>, String) {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let markdown = doc
        .extract_markdown(&ExtractOptions::default())
        .expect("markdown");
    (blocks, markdown)
}

fn with_hook(pdf: &[u8], hook: TatrTestHook) -> (Vec<pdf_engine::Block>, String) {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    let mut tatr = TatrOptions::default();
    tatr.test_hook = Some(hook);
    let options = ExtractOptions::default().with_tatr(tatr);
    let blocks = doc.extract_blocks(&options).expect("blocks");
    let markdown = doc.extract_markdown(&options).expect("markdown");
    (blocks, markdown)
}

/// A 2x2 over the region's first words: a shape the base grid never emits
/// for these pages, so every cell of it in the output is the fake's.
fn two_by_two() -> TatrTestHook {
    let cell = |row: usize, col: usize, header: bool, words: Vec<usize>| {
        TatrTestCell::new(vec![row], vec![col], header, words)
    };
    TatrTestHook::Cells(vec![
        cell(0, 0, true, (0..8).collect()),
        cell(0, 1, true, (8..16).collect()),
        cell(1, 0, false, (16..24).collect()),
        cell(1, 1, false, (24..32).collect()),
    ])
}

/// The fake's cells land in the learned pass's table: a 2x2 where the base
/// reads a ruled 4x3, with the header flags the fake set.
#[test]
fn fake_cells_land_in_the_learned_tables() {
    let pdf = ruled_pdf();
    let (base, _) = base_of(&pdf);
    let base_cells = cells_sorted(&base);
    assert!(
        base_cells.len() > 4,
        "the page holds no ruled table to replace: {base_cells:?}"
    );
    let (blocks, _) = with_hook(&pdf, two_by_two());
    let cells = cells_sorted(&blocks);
    assert_eq!(
        cells
            .iter()
            .map(|(row, col, _, _, _, _)| (*row, *col))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)],
        "the fake's 2x2 did not land: {cells:?}"
    );
    assert!(
        cells.iter().take(2).all(|(_, _, _, _, header, _)| *header),
        "the head row lost its header flag: {cells:?}"
    );
    assert!(
        cells.iter().skip(2).all(|(_, _, _, _, header, _)| !*header),
        "the body row gained a header flag: {cells:?}"
    );
    // The texts are the region's words in page order, eight per cell: the
    // head's five words, then the first body row left to right. The region
    // holds two words past the fake's 32 claims ("partially burnt", the last
    // body row's tail); they stand inside the cells' extent, so the gap fill
    // lands them in the last cell rather than dropping them (P3b).
    assert_eq!(
        cells
            .iter()
            .map(|(_, _, _, _, _, text)| text.clone())
            .collect::<Vec<_>>(),
        vec![
            "Feed materials of plant origin 1.1.1 Grains of",
            "Hordeum vulgare L. It may be Barley 1.1.2",
            "Barley, puffed Product obtained from milled or broken",
            "barley 1.1.3 Barley, toasted Product of barley roasting partially burnt",
        ],
        "the fill drifted from the region's words"
    );
}

/// A rotated 3x3 falls back to the grid: TATR decodes pixels, so a
/// landscape table is declined (`fallback(rotated)`, proved by the verdict
/// unit test) and the region reads exactly as the base. Red without the
/// decline, where the fake's 2x2 lands on the turned words.
#[test]
fn a_rotated_table_falls_back_to_the_grid() {
    let pdf = rotated_pdf();
    let (base, base_markdown) = base_of(&pdf);
    let base_cells = cells_sorted(&base);
    assert!(
        base_cells.len() > 4,
        "the turned page holds no grid table to fall back to: {base_cells:?}"
    );
    let (blocks, markdown) = with_hook(&pdf, two_by_two());
    let cells = cells_sorted(&blocks);
    assert_eq!(
        cells
            .iter()
            .map(|(row, col, _, _, _, _)| (*row, *col))
            .collect::<Vec<_>>(),
        base_cells
            .iter()
            .map(|(row, col, _, _, _, _)| (*row, *col))
            .collect::<Vec<_>>(),
        "the fake fired on a rotated region: {cells:?}"
    );
    assert_eq!(
        format!("{blocks:?}"),
        format!("{base:?}"),
        "a rotated region drifted from the grid"
    );
    assert_eq!(markdown, base_markdown, "markdown drifted");
}

/// A character-spaced table reads its words joined with TATR on: the twin
/// sets every glyph apart and the fill reads the engine's joined words, the
/// same words the ordinary twin reads -- no word lost, none split. The
/// fragmented lines group into builder blocks in a different order than the
/// ordinary twin's, so the fake's claims land in different cells; the words
/// are what the fill owes, not the order. Red on the raw fill, where the
/// glyphs join with spaces.
#[test]
fn a_char_spaced_table_reads_joined_words_with_tatr_on() {
    let (ordinary_blocks, _) = with_hook(&ruled_pdf(), two_by_two());
    assert_eq!(
        cells_sorted(&ordinary_blocks).len(),
        4,
        "the ordinary twin lost the fake"
    );
    let (blocks, _) = with_hook(&char_spaced_pdf(), two_by_two());
    let cells = cells_sorted(&blocks);
    assert_eq!(
        cells
            .iter()
            .map(|(row, col, _, _, _, _)| (*row, *col))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)],
        "the fake's 2x2 did not land on the char-spaced twin: {cells:?}"
    );
    assert!(
        cells.iter().all(|(_, _, _, _, _, text)| {
            !text.split_whitespace().any(|w| {
                let mut chars = w.chars();
                chars
                    .next()
                    .is_some_and(|c| c.is_alphabetic() && chars.next().is_none())
            })
        }),
        "a cell still reads glyph by glyph: {cells:?}"
    );
    assert_eq!(
        sorted_words(&blocks),
        sorted_words(&ordinary_blocks),
        "the char-spaced twin lost or split a word"
    );
}

/// One cell claiming every word: the whole region's text lands in one cell,
/// word for word what the base table holds. Through the teacher entry: the
/// learned pass's final-table judge refuses a one-cell table, and the
/// teacher entry refuses nothing.
#[test]
fn one_fake_cell_claims_the_whole_region() {
    let pdf = ruled_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    let base = doc.table_in_region(0, bbox).expect("region");
    let mut base_words: Vec<&str> = base
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .flat_map(|b| b.text.split_whitespace())
        .collect();
    base_words.sort_unstable();
    assert!(!base_words.is_empty(), "the box holds no table");

    let hook = TatrTestHook::Cells(vec![TatrTestCell::new(
        vec![0],
        vec![0],
        false,
        (0..1000).collect(),
    )]);
    let mut tatr = TatrOptions::default();
    tatr.test_hook = Some(hook);
    let region = doc
        .table_in_region_with_tatr(0, bbox, &tatr)
        .expect("region");
    let cells = cells_sorted(&region);
    assert_eq!(cells.len(), 1, "one cell in, one cell out: {cells:?}");
    let mut got: Vec<&str> = cells[0].5.split_whitespace().collect();
    got.sort_unstable();
    assert_eq!(got, base_words, "a word went missing in the fill");
}

/// Every block's words, sorted: the conservation comparison.
fn sorted_words(blocks: &[pdf_engine::Block]) -> Vec<String> {
    let mut words: Vec<String> = blocks
        .iter()
        .flat_map(|b| b.text.split_whitespace().map(str::to_string))
        .collect();
    words.sort();
    words
}

/// A fired TATR stage loses no word: the strip page's region holds a 3x3
/// table and three fox lines, the fake claims the 11 table words, and the
/// fox lines come back as three paragraphs. Red on the pre-P3b hook, where
/// the splice ate every word no cell claimed.
#[test]
fn a_fired_tatr_stage_loses_no_word() {
    let pdf = strip_pdf();
    let (base, _) = base_of(&pdf);
    let base_words = sorted_words(&base);
    assert_eq!(base_words.len(), 22, "the strip page holds 22 words");
    // The region's 22 words in page order: each table row's words, then the
    // fox line under it. The fake claims the 11 table words; the fox words
    // stand past the table's right edge, outside the claimed extent.
    let table: Vec<usize> = vec![0, 1, 2, 7, 8, 9, 14, 15, 16, 17, 18];
    assert_eq!(table.len(), 11, "eleven table words");
    let hook = TatrTestHook::Cells(vec![
        TatrTestCell::new(vec![0], vec![0], true, vec![0]),
        TatrTestCell::new(vec![0], vec![1], true, vec![1]),
        TatrTestCell::new(vec![0], vec![2], true, vec![2]),
        TatrTestCell::new(vec![1], vec![0], false, vec![7]),
        TatrTestCell::new(vec![1], vec![1], false, vec![8]),
        TatrTestCell::new(vec![1], vec![2], false, vec![9]),
        TatrTestCell::new(vec![2], vec![0], false, vec![14, 15, 16]),
        TatrTestCell::new(vec![2], vec![1], false, vec![17]),
        TatrTestCell::new(vec![2], vec![2], false, vec![18]),
    ]);
    let (blocks, markdown) = with_hook(&pdf, hook);
    let cells = cells_sorted(&blocks);
    assert_eq!(
        cells
            .iter()
            .map(|(row, col, _, _, _, text)| (*row, *col, text.clone()))
            .collect::<Vec<_>>(),
        vec![
            (0, 0, "Item".to_string()),
            (0, 1, "2024".to_string()),
            (0, 2, "2023".to_string()),
            (1, 0, "Revenue".to_string()),
            (1, 1, "13,110".to_string()),
            (1, 2, "12,166".to_string()),
            (2, 0, "Cost of sales".to_string()),
            (2, 1, "(8,406)".to_string()),
            (2, 2, "(7,737)".to_string()),
        ],
        "the table did not land as the fake's 3x3: {cells:?}"
    );
    let paras: Vec<String> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .map(|b| b.text.clone())
        .collect();
    assert_eq!(
        paras,
        vec![
            "Fox jumps far away".to_string(),
            "Daft zebras run fast".to_string(),
            "Pack boxes quickly".to_string(),
        ],
        "the fox lines did not come back as three paragraphs"
    );
    assert_eq!(
        sorted_words(&blocks),
        base_words,
        "a word went missing in the fire"
    );
    assert!(
        markdown.contains("Fox jumps far away"),
        "the markdown lost the conserved prose"
    );
}

/// A TATR result that claims almost nothing of a wide extent falls back:
/// two words of a 34-word region read as the base, blocks and markdown.
/// Red on the pre-P3b hook, where the two-word table landed and the rest of
/// the region vanished.
#[test]
fn a_low_coverage_tatr_result_falls_back() {
    let pdf = ruled_pdf();
    let (base, base_markdown) = base_of(&pdf);
    let hook = TatrTestHook::Cells(vec![
        TatrTestCell::new(vec![0], vec![0], false, vec![0]),
        TatrTestCell::new(vec![0], vec![1], false, vec![33]),
        TatrTestCell::new(vec![1], vec![0], false, vec![]),
        TatrTestCell::new(vec![1], vec![1], false, vec![]),
    ]);
    let (blocks, markdown) = with_hook(&pdf, hook);
    assert_eq!(
        format!("{blocks:?}"),
        format!("{base:?}"),
        "a low-coverage fire did not fall back to the base"
    );
    assert_eq!(markdown, base_markdown, "markdown drifted");
}

/// A word in a gap between cells is filled, not dropped: the fake claims
/// every region word but two mid-region words, and those two land in cells.
/// Red on the pre-P3b hook, where they vanished.
#[test]
fn a_word_in_a_gap_between_cells_is_filled() {
    let pdf = ruled_pdf();
    let (base, _) = base_of(&pdf);
    let base_words = sorted_words(&base);
    assert_eq!(base_words.len(), 34, "the ruled region holds 34 words");
    let cell = |row: usize, col: usize, words: Vec<usize>| {
        TatrTestCell::new(vec![row], vec![col], row == 0, words)
    };
    // Words 15 ("1.1.2") and 16 ("Barley,") stand unclaimed inside the
    // extent; coverage is 32 of 34 and the fire stands.
    let hook = TatrTestHook::Cells(vec![
        cell(0, 0, (0..8).collect()),
        cell(0, 1, (8..15).collect()),
        cell(1, 0, (17..25).collect()),
        cell(1, 1, (25..34).collect()),
    ]);
    let (blocks, _) = with_hook(&pdf, hook);
    assert_eq!(
        sorted_words(&blocks),
        base_words,
        "a word went missing in the fire"
    );
    let joined = cells_sorted(&blocks)
        .into_iter()
        .map(|(_, _, _, _, _, text)| text)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("1.1.2"),
        "the gap word 1.1.2 is not in any cell: {joined:?}"
    );
    assert!(
        joined.contains("Barley,"),
        "the gap word Barley, is not in any cell: {joined:?}"
    );
}

/// A declining structurer reads exactly as the base: blocks and markdown,
/// on a ruled table, a borderless one, and prose.
#[test]
fn a_declining_structurer_gives_the_base_output() {
    for (name, pdf) in [
        ("ruled", ruled_pdf()),
        ("unruled", unruled_pdf()),
        ("prose", prose_pdf()),
    ] {
        let (base, base_markdown) = base_of(&pdf);
        let (blocks, markdown) = with_hook(&pdf, TatrTestHook::Declined);
        assert_eq!(
            format!("{blocks:?}"),
            format!("{base:?}"),
            "{name}: a decline drifted from the base"
        );
        assert_eq!(markdown, base_markdown, "{name}: markdown drifted");
    }
}

/// A failing region falls back to the base grid for that region: same
/// blocks, same markdown.
#[test]
fn a_failed_region_falls_back_to_base() {
    for (name, pdf) in [
        ("ruled", ruled_pdf()),
        ("unruled", unruled_pdf()),
        ("prose", prose_pdf()),
    ] {
        let (base, base_markdown) = base_of(&pdf);
        let (blocks, markdown) = with_hook(&pdf, TatrTestHook::Failed);
        assert_eq!(
            format!("{blocks:?}"),
            format!("{base:?}"),
            "{name}: a failure drifted from the base"
        );
        assert_eq!(markdown, base_markdown, "{name}: markdown drifted");
    }
}

/// `detector_regions_only` skips the learned pass's own regions: the fake
/// fires nowhere and the base comes back untouched.
#[test]
fn detector_regions_only_skips_the_learned_pass() {
    let pdf = ruled_pdf();
    let (base, base_markdown) = base_of(&pdf);
    assert!(
        !cells_sorted(&base).is_empty(),
        "the page holds no table at all"
    );
    let doc = PdfDocument::open(pdf).expect("open");
    let mut tatr = TatrOptions::default();
    tatr.detector_regions_only = true;
    tatr.test_hook = Some(two_by_two());
    let options = ExtractOptions::default().with_tatr(tatr);
    let blocks = doc.extract_blocks(&options).expect("blocks");
    let markdown = doc.extract_markdown(&options).expect("markdown");
    assert_eq!(
        format!("{blocks:?}"),
        format!("{base:?}"),
        "a restricted hook rebuilt a learned region"
    );
    assert_eq!(markdown, base_markdown, "markdown drifted");
}

/// The teacher box is a detector box: the same restricted fake that the
/// learned pass skips fires inside `table_in_region`.
#[test]
fn a_teacher_box_is_a_detector_box() {
    let pdf = ruled_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    assert!(bbox[0] <= bbox[2], "the page holds no table to box");
    let mut tatr = TatrOptions::default();
    tatr.detector_regions_only = true;
    tatr.test_hook = Some(two_by_two());
    let region = doc
        .table_in_region_with_tatr(0, bbox, &tatr)
        .expect("region");
    let cells = cells_sorted(&region);
    assert_eq!(
        cells
            .iter()
            .map(|(row, col, _, _, _, _)| (*row, *col))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)],
        "the restricted fake did not fire in the teacher box: {cells:?}"
    );
}

/// The teacher entry with a declining structurer is the teacher entry.
#[test]
fn a_declining_structurer_in_a_teacher_box_is_the_base() {
    let pdf = ruled_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    let base = doc.table_in_region(0, bbox).expect("region");
    assert!(!cells_sorted(&base).is_empty(), "no table in the box");
    let mut tatr = TatrOptions::default();
    tatr.test_hook = Some(TatrTestHook::Declined);
    let region = doc
        .table_in_region_with_tatr(0, bbox, &tatr)
        .expect("region");
    assert_eq!(
        format!("{region:?}"),
        format!("{base:?}"),
        "a decline drifted from the teacher base"
    );
}

/// Asking for TATR on a build without it is refused, never run as fast.
#[cfg(not(feature = "ort-tables"))]
#[test]
fn asking_for_tatr_without_the_feature_refuses() {
    let pdf = ruled_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let options = ExtractOptions::default().with_tatr(TatrOptions::default());
    let err = doc.extract_blocks(&options).expect_err("must refuse");
    assert_eq!(
        err.options(),
        Some(&pdf_engine::OptionsError::TatrNeedsOrtTables),
        "the refusal names the missing build: {err:?}"
    );
    assert_eq!(err.code(), "INVALID_OPTIONS");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let bbox = union_of(&blocks);
    let err = doc
        .table_in_region_with_tatr(0, bbox, &TatrOptions::default())
        .expect_err("the teacher entry must refuse too");
    assert_eq!(
        err.options(),
        Some(&pdf_engine::OptionsError::TatrNeedsOrtTables),
        "the teacher refusal names the missing build: {err:?}"
    );
}

/// The installer's tables spec names the same package the stage loads: the
/// name, the file and the digest `models install tables` writes MUST match
/// the stage's own spec, or an install lands where nothing looks. (The
/// install lines differ on purpose: the stage's names the manual route, the
/// installer's the command.)
#[cfg(feature = "ort-tables")]
#[test]
fn the_installer_tables_spec_matches_the_stage_spec() {
    let installed = &pdf_engine::model::TABLES_TATR;
    let staged = &pdf_engine::learned::tables_tatr::TATR_SPEC;
    assert_eq!(installed.name, staged.name);
    assert_eq!(installed.file, staged.file);
    assert_eq!(installed.sha256, staged.sha256);
}

/// A missing model refuses up front with the shared layer's error: the kind,
/// the model, and the install line. No dylib: the file is checked before
/// the library.
#[cfg(feature = "ort-tables")]
#[test]
fn a_missing_model_refuses_with_its_install_line() {
    let pdf = prose_pdf();
    let doc = PdfDocument::open(pdf).expect("open");
    let dir = std::env::temp_dir().join("tatr-hook-no-such-model-dir");
    let mut tatr = TatrOptions::default();
    tatr.model_dir = Some(dir);
    let options = ExtractOptions::default().with_tatr(tatr);
    // Prose on purpose: the refusal fires before any region runs, so even a
    // table-less page refuses rather than running a silent fast pass.
    let err = doc.extract_blocks(&options).expect_err("must refuse");
    match err {
        pdf_engine::EngineError::Io(io) => {
            let back =
                pdf_ocr::runtime::model_error(&io).expect("the chain carries the ModelError");
            assert_eq!(
                back.kind,
                pdf_ocr::runtime::ModelErrorKind::ModelNotAvailable
            );
            assert_eq!(back.model.as_deref(), Some("tatr-v1.1-all"));
            let install = back.install_command.as_deref().unwrap_or_default();
            assert!(
                install.contains("tatr-v1.1-all"),
                "the install line names the package: {install:?}"
            );
        }
        other => panic!("a missing model rides as Io, got {other:?}"),
    }
}

#[cfg(feature = "ort-tables")]
fn test_dylib() -> Option<std::path::PathBuf> {
    match std::env::var("PDFLUENT_ORT_TEST_DYLIB") {
        Ok(path) if !path.is_empty() => Some(std::path::PathBuf::from(path)),
        _ => None,
    }
}

#[cfg(feature = "ort-tables")]
fn test_model_dir() -> Option<std::path::PathBuf> {
    match std::env::var("PDFLUENT_TATR_TEST_MODEL_DIR") {
        Ok(dir) if !dir.is_empty() => Some(std::path::PathBuf::from(dir)),
        _ => None,
    }
}

#[cfg(feature = "ort-tables")]
fn test_pages_dir() -> Option<std::path::PathBuf> {
    match std::env::var("PDFLUENT_TATR_TEST_PAGES_DIR") {
        Ok(dir) if !dir.is_empty() => Some(std::path::PathBuf::from(dir)),
        _ => None,
    }
}

/// Three train-side FinTabNet pages through the real structurer: where TATR
/// fires the tables come from it, and the milliseconds per table are
/// reported. Skipped loudly without a dylib, a model package, or the pages.
#[cfg(feature = "ort-tables")]
#[test]
fn tatr_rebuilds_three_ftn_pages_end_to_end() {
    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };
    let Some(model_dir) = test_model_dir() else {
        eprintln!("skipped: PDFLUENT_TATR_TEST_MODEL_DIR is not set");
        return;
    };
    let Some(pages_dir) = test_pages_dir() else {
        eprintln!("skipped: PDFLUENT_TATR_TEST_PAGES_DIR is not set");
        return;
    };
    // Train-side FinTabNet pages holding tables the base pass finds, so a
    // region reaches the structurer on each.
    let pages = [
        "AAL_2015_page_207.pdf",
        "AAPL_2006_page_47.pdf",
        "AAPL_2014_page_69.pdf",
    ];
    let mut differed = 0usize;
    for name in pages {
        let path = pages_dir.join(name);
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipped: {name} is not in {}", pages_dir.display());
            return;
        };
        let doc = PdfDocument::open(bytes).expect("open");
        let base = doc
            .extract_blocks(&ExtractOptions::default())
            .expect("base blocks");
        let base_tables = cells_sorted(&base);
        assert!(
            !base_tables.is_empty(),
            "{name}: the base finds no table, so nothing reaches TATR"
        );
        let mut tatr = TatrOptions::default();
        tatr.model_dir = Some(model_dir.clone());
        tatr.ort_dylib = Some(dylib.clone());
        let options = ExtractOptions::default().with_tatr(tatr);
        let started = std::time::Instant::now();
        let arm = doc.extract_blocks(&options).expect("arm blocks");
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let arm_tables = cells_sorted(&arm);
        // Each table opens at row 0, column 0: one opener per table.
        let tables = arm_tables
            .iter()
            .filter(|(row, col, _, _, _, _)| *row == 0 && *col == 0)
            .count()
            .max(1);
        eprintln!(
            "{name}: {tables} tables ({} cells), {ms:.0} ms total, {:.1} ms/table",
            arm_tables.len(),
            ms / tables as f64
        );
        if format!("{arm:?}") != format!("{base:?}") {
            differed += 1;
        }
    }
    assert!(
        differed >= 1,
        "TATR changed nothing on any page: a decline reads as the base, so this proves no firing"
    );
}
