// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Header rows mark through the production path (#636).
//!
//! On the TUNING right-shape grids whose header rows mismatch the truth, the
//! modal failure is labelling, not shape: too few header rows (a stacked or
//! word header read as body), too many (a label-like body row promoted), and
//! headerless grids forced into a pipe table's header. Each rule below ships
//! behind its own switch, and each test names the arm it reads with, so the
//! off arm documents what master does and the on arm the fix.
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, judge, decoder, serializer. Each test is
//! red with its feature removed.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{Switches, SWITCHES};
use pdf_engine::{to_markdown_with_switches, Block, BlockKind, ExtractOptions, PdfDocument};

/// Every header switch off: what master does.
fn off() -> Switches {
    Switches {
        markdown_header_block_figures: false,
        markdown_header_block_rowspan_crossing: false,
        markdown_header_block_word_table: false,
        markdown_header_block_mixed_body: false,
        markdown_headerless_html: false,
        markdown_header_block_stub_years: false,
        ..SWITCHES
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

fn pdf_of(pieces: &[(String, bool, f64, f64)], hlines: &[(f64, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let heavy = font(&mut doc, b"Helvetica-Bold");
    let mut ops = Vec::new();
    for (text, bold, x, y) in pieces {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if *bold { b"F2" } else { b"F1" }.to_vec()),
                10.0.into(),
            ],
        ));
        ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        ops.push(Operation::new("ET", vec![]));
    }
    for (x0, x1, y) in hlines {
        ops.push(Operation::new(
            "re",
            vec![(*x0).into(), (*y).into(), (x1 - x0).into(), 0.5.into()],
        ));
        ops.push(Operation::new("f", vec![]));
    }
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
                "F2" => heavy.clone(),
            }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
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

fn blocks_of(pieces: &[(String, bool, f64, f64)], hlines: &[(f64, f64, f64)]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces, hlines))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// A two-row stacked header over a grouped numeric body marks two `<th>` rows
/// under `markdown_header_block_rowspan_crossing`: the row group's span sits
/// in the body, clear of the head boundary, so it no longer vetoes. Red on
/// master, where any rowspan cancels the header block and only the decoder's
/// bold first row marks.
#[test]
fn a_stacked_header_over_a_grouped_body_marks_two_rows() {
    let pieces = vec![
        ("Region".into(), true, 40.0, 700.0),
        ("North".into(), true, 150.0, 700.0),
        ("South".into(), true, 230.0, 700.0),
        ("Share".into(), true, 310.0, 700.0),
        ("Area".into(), true, 40.0, 682.0),
        ("First".into(), true, 150.0, 682.0),
        ("Second".into(), true, 230.0, 682.0),
        ("Third".into(), true, 310.0, 682.0),
        ("East".into(), false, 40.0, 664.0),
        ("12".into(), false, 150.0, 664.0),
        ("34".into(), false, 230.0, 664.0),
        ("56".into(), false, 310.0, 664.0),
        ("West".into(), false, 40.0, 646.0),
        ("9".into(), false, 150.0, 646.0),
        ("8".into(), false, 230.0, 646.0),
        ("7".into(), false, 310.0, 646.0),
        ("5".into(), false, 150.0, 628.0),
        ("6".into(), false, 230.0, 628.0),
        ("4".into(), false, 310.0, 628.0),
    ];
    let hlines = vec![
        (40.0, 380.0, 693.0),
        (40.0, 380.0, 675.0),
        (40.0, 380.0, 657.0),
        (130.0, 380.0, 639.0),
    ];
    let blocks = blocks_of(&pieces, &hlines);
    assert!(
        blocks.iter().any(
            |b| matches!(b.kind, BlockKind::TableCell { row_span: 2, .. }) && b.text == "West"
        ),
        "the group reads as a rowspan, or there is no veto to narrow: {blocks:?}"
    );
    let master = to_markdown_with_switches(&blocks, off());
    assert!(
        master.contains("<tr><td>Area</td>"),
        "master reads the stacked second row as body: {master:?}"
    );
    let on = Switches {
        markdown_header_block_rowspan_crossing: true,
        ..off()
    };
    let fixed = to_markdown_with_switches(&blocks, on);
    assert!(
        fixed.contains("<tr><th>Area</th><th>First</th><th>Second</th><th>Third</th></tr>"),
        "both header rows mark: {fixed:?}"
    );
    assert!(
        fixed.contains("<td rowspan=\"2\">West</td>"),
        "the body keeps its row group: {fixed:?}"
    );
}

/// A table of words heads its first row under `markdown_header_block_word_table`.
/// Red on master, where no figures row means no header at all.
#[test]
fn a_word_table_heads_its_first_row() {
    let pieces = vec![
        ("Feature".into(), false, 40.0, 700.0),
        ("Pro".into(), false, 200.0, 700.0),
        ("Con".into(), false, 330.0, 700.0),
        ("Hiring".into(), false, 40.0, 682.0),
        ("Referrals".into(), false, 200.0, 682.0),
        ("Costs".into(), false, 330.0, 682.0),
        ("Keeping".into(), false, 40.0, 664.0),
        ("Pay and bonuses everywhere".into(), false, 200.0, 664.0),
        ("Leaving".into(), false, 40.0, 646.0),
        ("Offers".into(), false, 200.0, 646.0),
        ("Moves".into(), false, 330.0, 646.0),
    ];
    // The wide middle cell spans two columns, so the table is HTML either
    // way and the header marks -- not the pipe form -- are what is compared.
    let blocks = blocks_of(&pieces, &[]);
    let master = to_markdown_with_switches(&blocks, off());
    assert!(
        master.contains("<tr><td>Feature</td>"),
        "master marks no header on a word grid: {master:?}"
    );
    let on = Switches {
        markdown_header_block_word_table: true,
        ..off()
    };
    let fixed = to_markdown_with_switches(&blocks, on);
    assert!(
        fixed.contains("<tr><th>Feature</th><th>Pro</th><th>Con</th></tr>"),
        "the first row heads by position: {fixed:?}"
    );
}

/// A first body row of label and numbers stays `<td>` under
/// `markdown_header_block_mixed_body`: it has already turned from label to
/// value. Red on master, where one figure of four reads as labels and the
/// row is promoted.
#[test]
fn a_first_body_row_of_label_and_numbers_stays_body() {
    let pieces = vec![
        ("Alpha".into(), false, 40.0, 700.0),
        ("beta".into(), false, 150.0, 700.0),
        ("gamma".into(), false, 230.0, 700.0),
        ("12".into(), false, 310.0, 700.0),
        ("Beta".into(), false, 40.0, 682.0),
        ("56".into(), false, 150.0, 682.0),
        ("78".into(), false, 230.0, 682.0),
        ("90".into(), false, 310.0, 682.0),
        ("Gamma".into(), false, 40.0, 664.0),
        ("eleven".into(), false, 150.0, 664.0),
        ("wide words here".into(), false, 230.0, 664.0),
        ("Delta".into(), false, 40.0, 646.0),
        ("44".into(), false, 150.0, 646.0),
        ("55".into(), false, 230.0, 646.0),
        ("66".into(), false, 310.0, 646.0),
    ];
    let blocks = blocks_of(&pieces, &[]);
    let master = to_markdown_with_switches(&blocks, off());
    assert!(
        master.contains("<tr><th>Alpha</th>"),
        "master promotes the mixed first row: {master:?}"
    );
    let on = Switches {
        markdown_header_block_mixed_body: true,
        ..off()
    };
    let fixed = to_markdown_with_switches(&blocks, on);
    assert!(
        !fixed.contains("<th"),
        "no row of a headerless grid marks: {fixed:?}"
    );
}

/// A headerless rectangle is HTML under `markdown_headerless_html`, with the
/// mixed-body switch ending the header: the pipe form would force the first
/// row into a header the table does not have. Red on master, which writes
/// pipes.
#[test]
fn a_headerless_rectangle_is_html() {
    let pieces = vec![
        ("Alpha".into(), false, 40.0, 700.0),
        ("beta".into(), false, 150.0, 700.0),
        ("gamma".into(), false, 230.0, 700.0),
        ("12".into(), false, 310.0, 700.0),
        ("Beta".into(), false, 40.0, 682.0),
        ("56".into(), false, 150.0, 682.0),
        ("78".into(), false, 230.0, 682.0),
        ("90".into(), false, 310.0, 682.0),
        ("Gamma".into(), false, 40.0, 664.0),
        ("11".into(), false, 150.0, 664.0),
        ("22".into(), false, 230.0, 664.0),
        ("33".into(), false, 310.0, 664.0),
        ("Delta".into(), false, 40.0, 646.0),
        ("44".into(), false, 150.0, 646.0),
        ("55".into(), false, 230.0, 646.0),
        ("66".into(), false, 310.0, 646.0),
    ];
    let blocks = blocks_of(&pieces, &[]);
    let master = to_markdown_with_switches(&blocks, off());
    assert!(
        master.starts_with('|'),
        "master writes the rectangle as pipes: {master:?}"
    );
    let on = Switches {
        markdown_header_block_mixed_body: true,
        markdown_headerless_html: true,
        ..off()
    };
    let fixed = to_markdown_with_switches(&blocks, on);
    assert!(
        fixed.starts_with("<table>"),
        "a headerless table is HTML, not pipes: {fixed:?}"
    );
    assert!(!fixed.contains("<th"), "no cell promotes: {fixed:?}");
}

/// A stubbed year row heads its columns under
/// `markdown_header_block_stub_years`: the years in the value columns head
/// them, whatever the stub cell holds. Red on master, where two years of
/// three read as figures in the first row and no header marks.
#[test]
fn a_stubbed_year_row_heads_its_columns() {
    let pieces = vec![
        ("(in millions)".into(), false, 40.0, 700.0),
        ("2010".into(), false, 200.0, 700.0),
        ("2009".into(), false, 330.0, 700.0),
        ("Sales".into(), false, 40.0, 682.0),
        ("914".into(), false, 200.0, 682.0),
        ("850".into(), false, 330.0, 682.0),
        ("Costs".into(), false, 40.0, 664.0),
        ("wide words here".into(), false, 200.0, 664.0),
        ("Net".into(), false, 40.0, 646.0),
        ("12".into(), false, 200.0, 646.0),
        ("34".into(), false, 330.0, 646.0),
    ];
    let blocks = blocks_of(&pieces, &[]);
    let master = to_markdown_with_switches(&blocks, off());
    assert!(
        master.contains("<tr><td>(in millions)</td>"),
        "master reads the stubbed years as figures: {master:?}"
    );
    let on = Switches {
        markdown_header_block_stub_years: true,
        ..off()
    };
    let fixed = to_markdown_with_switches(&blocks, on);
    assert!(
        fixed.contains("<tr><th>(in millions)</th><th>2010</th><th>2009</th></tr>"),
        "the stubbed years head: {fixed:?}"
    );
}

/// A group header over three numeric columns spans exactly those three:
/// `colspan="3"`, neither 1 nor the full width. A pin, green on master --
/// the span rules live in the decoder, and this shape holds them honest
/// while the labelling around them changes.
#[test]
fn a_group_header_spans_three_numeric_columns() {
    let mut pieces = vec![("Sales by quarter in euros".into(), true, 175.0, 700.0)];
    for (r, stub) in ["East", "West", "North", "South"].iter().enumerate() {
        let y = 682.0 - r as f64 * 18.0;
        pieces.push((stub.to_string(), false, 40.0, y));
        pieces.push((format!("{}", 10 + r), false, 150.0, y));
        pieces.push((format!("{}", 20 + r), false, 230.0, y));
        pieces.push((format!("{}", 30 + r), false, 310.0, y));
    }
    let blocks = blocks_of(&pieces, &[]);
    let head: Vec<(u16, u16, String)> = blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell {
                row: 0,
                column,
                column_span,
                ..
            } => Some((column, column_span, b.text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        head,
        vec![(1, 3, "Sales by quarter in euros".to_string())],
        "the label spans the three numeric columns, not one, not all four: {head:?}"
    );
    for switches in [
        off(),
        Switches {
            markdown_header_block_rowspan_crossing: true,
            markdown_header_block_word_table: true,
            markdown_header_block_mixed_body: true,
            markdown_headerless_html: true,
            markdown_header_block_stub_years: true,
            ..off()
        },
    ] {
        let md = to_markdown_with_switches(&blocks, switches);
        assert!(
            md.contains("colspan=\"3\""),
            "the span survives the header rules: {md:?}"
        );
    }
}
