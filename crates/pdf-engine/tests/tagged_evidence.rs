// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A tagged PDF's structure tree as extraction evidence (#636).
//!
//! The fixtures are built in this file: one page of Helvetica drawn through
//! marked content (`/H1 <</MCID 0>> BDC ... EMC`), with the structure tree
//! written by `pdf_compliance::tagged_gen` -- the same builder a producer
//! would use -- and table spans and malformed trees added by editing the
//! resulting objects directly.
//!
//! What each test would do without the feature is asserted beside what it
//! does with it: the tagged answer must differ from the geometric one where
//! the tree earns it, and equal it where the tree is refused or the option
//! is off. A test whose two answers could agree proves nothing, so none of
//! them can.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_compliance::tagged_gen::StructureTreeBuilder;
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const SIZE: f64 = 12.0;
const MARGIN: f64 = 72.0;

/// One run of text, at a size, at a place.
struct Run {
    text: &'static str,
    x: f64,
    y: f64,
}

fn helvetica(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })))
}

fn show(operations: &mut Vec<Operation>, run: &Run) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), SIZE.into()],
    ));
    operations.push(Operation::new("Td", vec![run.x.into(), run.y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(run.text)]));
    operations.push(Operation::new("ET", vec![]));
}

/// `run` drawn as marked content `tag` with `mcid`.
fn tagged_show(operations: &mut Vec<Operation>, tag: &str, mcid: i32, run: &Run) {
    operations.push(Operation::new(
        "BDC",
        vec![
            Object::Name(tag.as_bytes().to_vec()),
            Object::Dictionary(dictionary! { "MCID" => Object::Integer(mcid as i64) }),
        ],
    ));
    show(operations, run);
    operations.push(Operation::new("EMC", vec![]));
}

/// `run` drawn as `/Artifact`: pagination furniture, outside the tree.
fn artifact_show(operations: &mut Vec<Operation>, run: &Run) {
    operations.push(Operation::new(
        "BMC",
        vec![Object::Name(b"Artifact".to_vec())],
    ));
    show(operations, run);
    operations.push(Operation::new("EMC", vec![]));
}

/// A one-page document of whatever the caller drew, before the tree.
fn doc_of(operations: Vec<Operation>) -> Document {
    let mut doc = Document::with_version("1.7");
    let font = helvetica(&mut doc);
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
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
    doc
}

fn bytes_of(doc: &mut Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save synthetic pdf");
    bytes
}

fn tagged_options() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.use_tags = true;
    options
}

fn blocks_of(bytes: &[u8], options: &ExtractOptions) -> Vec<Block> {
    PdfDocument::open(bytes.to_vec())
        .expect("open pdf")
        .extract_blocks(options)
        .expect("extract blocks")
}

/// The texts of every block, in order.
fn texts(blocks: &[Block]) -> Vec<&str> {
    blocks.iter().map(|block| block.text.as_str()).collect()
}

/// A heading, a paragraph, three list items and a paragraph -- the heading
/// drawn last on the page but first in the tree, everything set in one size
/// so that only the tags say which line is which.
fn heading_list_fixture() -> (Vec<u8>, Vec<u8>) {
    let runs = [
        Run {
            text: "Revenue grew in every region this quarter.",
            x: MARGIN,
            y: 700.0,
        },
        Run {
            text: "1. Retail revenue rose first",
            x: MARGIN,
            y: 676.0,
        },
        Run {
            text: "2. Wholesale followed in March",
            x: MARGIN,
            y: 652.0,
        },
        Run {
            text: "3. Services held steady overall",
            x: MARGIN,
            y: 628.0,
        },
        Run {
            text: "Costs stayed within budget all year.",
            x: MARGIN,
            y: 604.0,
        },
        Run {
            text: "Quarterly Report",
            x: MARGIN,
            y: 580.0,
        },
    ];
    let tags = ["P", "LI", "LI", "LI", "P", "H1"];
    let mut tagged = Vec::new();
    let mut plain = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        tagged_show(&mut tagged, tags[index], index as i32, run);
        show(&mut plain, run);
    }
    let mut doc = doc_of(tagged);
    let mut builder = StructureTreeBuilder::new();
    builder.set_lang("en-US");
    // The tree's order, not the page's: the heading first.
    builder.add_heading(1, 5, 1);
    builder.add_paragraph(0, 1);
    builder.add_list(&[1, 2, 3], 1);
    builder.add_paragraph(4, 1);
    builder.build(&mut doc).expect("build structure tree");
    (bytes_of(&mut doc), bytes_of(&mut doc_of(plain)))
}

#[test]
fn tagged_heading_list_and_order_come_from_the_tree() {
    let (tagged, _) = heading_list_fixture();
    let blocks = blocks_of(&tagged, &tagged_options());

    assert_eq!(
        texts(&blocks),
        vec![
            "Quarterly Report",
            "Revenue grew in every region this quarter.",
            "Retail revenue rose first",
            "Wholesale followed in March",
            "Services held steady overall",
            "Costs stayed within budget all year.",
        ]
    );
    assert!(matches!(blocks[0].kind, BlockKind::Heading { level: 1 }));
    for (block, ordinal) in blocks[2..5].iter().zip([1, 2, 3]) {
        assert!(
            matches!(
                block.kind,
                BlockKind::ListItem { level: 0, ordinal: Some(n) } if n == ordinal
            ),
            "item kind: {:?}",
            block.kind
        );
    }
    assert!(matches!(blocks[1].kind, BlockKind::Paragraph));
    assert!(matches!(blocks[5].kind, BlockKind::Paragraph));
    // Numbered in the tree's order, from zero.
    for (index, block) in blocks.iter().enumerate() {
        assert_eq!(block.index, index);
    }

    // Without the feature the heading stays where the page drew it -- last
    // -- and in the body size it is a paragraph: the tagged answer differs
    // in both kind and order, so this test cannot pass on geometry alone.
    let untagged = blocks_of(&tagged, &ExtractOptions::default());
    assert_ne!(texts(&untagged), texts(&blocks));
    assert!(
        !matches!(
            untagged.last().expect("six blocks").kind,
            BlockKind::Heading { level: 1 }
        ),
        "geometry must not find the same-size heading on its own"
    );
}

/// The same page as [`heading_list_fixture`], but the heading and the first
/// paragraph claim one MCID: an impossible tree.
fn duplicate_mcid_fixture() -> Vec<u8> {
    let (tagged, _) = heading_list_fixture();
    let mut doc = Document::load_mem(&tagged).expect("reload fixture");
    // The H1 element owns MCID 5; hand it MCID 0, the paragraph's.
    for object in doc.objects.values_mut() {
        let Object::Dictionary(dict) = object else {
            continue;
        };
        let is_h1 = matches!(dict.get(b"S"), Ok(Object::Name(name)) if name == b"H1");
        if is_h1 {
            dict.set("K", Object::Integer(0));
        }
    }
    bytes_of(&mut doc)
}

/// The same page, but the tree lists the content bottom to top: the exact
/// reverse of the geometric order.
fn reversed_tree_fixture() -> Vec<u8> {
    let runs = [
        ("Revenue grew in every region this quarter.", 700.0),
        ("Costs stayed within budget all year.", 676.0),
        ("Services held steady overall.", 652.0),
        ("Wholesale followed in March.", 628.0),
        ("Retail revenue rose first.", 604.0),
        ("Quarterly Report.", 580.0),
    ];
    let mut operations = Vec::new();
    for (index, (text, y)) in runs.iter().enumerate() {
        tagged_show(
            &mut operations,
            "P",
            index as i32,
            &Run {
                text,
                x: MARGIN,
                y: *y,
            },
        );
    }
    let mut doc = doc_of(operations);
    let mut builder = StructureTreeBuilder::new();
    for mcid in (0..6).rev() {
        builder.add_paragraph(mcid, 1);
    }
    builder.build(&mut doc).expect("build structure tree");
    bytes_of(&mut doc)
}

#[test]
fn malformed_trees_are_refused() {
    for (name, bytes) in [
        ("duplicate MCID", duplicate_mcid_fixture()),
        ("reversed order", reversed_tree_fixture()),
    ] {
        let tagged = blocks_of(&bytes, &tagged_options());
        let untagged = blocks_of(&bytes, &ExtractOptions::default());
        assert_eq!(
            format!("{tagged:?}"),
            format!("{untagged:?}"),
            "{name}: a refused tree must leave the output untouched"
        );
        assert_eq!(
            pdf_engine::to_markdown(&tagged),
            pdf_engine::to_markdown(&untagged),
            "{name}: markdown agrees too"
        );
    }
}

#[test]
fn option_off_ignores_tags_byte_identical() {
    assert!(
        !ExtractOptions::default().use_tags,
        "tags decide only when asked"
    );
    let (tagged, twin) = heading_list_fixture();
    // The twin draws the same runs with no marked content and no tree: what
    // master -- which has no tagged path at all -- reads from the same page.
    let from_tagged = blocks_of(&tagged, &ExtractOptions::default());
    let from_twin = blocks_of(&twin, &ExtractOptions::default());
    assert_eq!(
        format!("{from_tagged:?}"),
        format!("{from_twin:?}"),
        "option off: the tree on the page changes nothing"
    );
    assert_eq!(
        pdf_engine::to_markdown(&from_tagged),
        pdf_engine::to_markdown(&from_twin)
    );
}

/// Two columns, four rows: a `TH` header row, a `ColSpan 2` cell, a `RowSpan
/// 2` cell holding the first column through the last row, and a last row of
/// one cell standing in the second column.
fn table_fixture() -> Vec<u8> {
    let runs = [
        ("Name", 72.0, 700.0),
        ("Amount", 300.0, 700.0),
        ("Citrus fruit", 72.0, 676.0),
        ("Oranges", 72.0, 652.0),
        ("Twelve", 300.0, 652.0),
        ("Nine", 300.0, 628.0),
    ];
    let mut operations = Vec::new();
    for (index, (text, x, y)) in runs.iter().enumerate() {
        tagged_show(
            &mut operations,
            if index < 2 { "TH" } else { "TD" },
            index as i32,
            &Run { text, x: *x, y: *y },
        );
    }
    let mut doc = doc_of(operations);
    let mut builder = StructureTreeBuilder::new();
    builder.add_table(&[vec![0, 1], vec![2], vec![3, 4], vec![5]], 1);
    builder.build(&mut doc).expect("build structure tree");
    // The builder writes plain TDs; the header row and the spans are edits
    // on the elements it wrote, found by the MCID each owns.
    for object in doc.objects.values_mut() {
        let Object::Dictionary(dict) = object else {
            continue;
        };
        let is_cell = matches!(dict.get(b"S"), Ok(Object::Name(name)) if name == b"TD");
        if !is_cell {
            continue;
        }
        let mcid = match dict.get(b"K") {
            Ok(Object::Integer(mcid)) => *mcid,
            _ => continue,
        };
        if mcid == 0 || mcid == 1 {
            dict.set("S", Object::Name(b"TH".to_vec()));
        }
        if mcid == 2 {
            dict.set(
                "A",
                Object::Dictionary(dictionary! {
                    "O" => Object::Name(b"Table".to_vec()),
                    "ColSpan" => Object::Integer(2),
                }),
            );
        }
        if mcid == 3 {
            dict.set(
                "A",
                Object::Dictionary(dictionary! {
                    "O" => Object::Name(b"Table".to_vec()),
                    "RowSpan" => Object::Integer(2),
                }),
            );
        }
    }
    bytes_of(&mut doc)
}

#[test]
fn tagged_table_cells_spans_and_headers() {
    let bytes = table_fixture();
    let blocks = blocks_of(&bytes, &tagged_options());

    let cells: Vec<(u16, u16, u16, u16, bool, &str)> = blocks
        .iter()
        .map(|block| match block.kind {
            BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                header,
            } => (
                row,
                column,
                row_span,
                column_span,
                header,
                block.text.as_str(),
            ),
            kind => panic!("expected a table cell, got {kind:?}"),
        })
        .collect();
    assert_eq!(
        cells,
        vec![
            (0, 0, 1, 1, true, "Name"),
            (0, 1, 1, 1, true, "Amount"),
            (1, 0, 1, 2, false, "Citrus fruit"),
            (2, 0, 2, 1, false, "Oranges"),
            (2, 1, 1, 1, false, "Twelve"),
            (3, 1, 1, 1, false, "Nine"),
        ]
    );

    // The geometry never wrote this table: no ruling lines, no bold header,
    // and a last row the grid cannot place -- so the tagged answer differs,
    // and this test cannot pass on geometry alone.
    let untagged = blocks_of(&bytes, &ExtractOptions::default());
    assert_ne!(format!("{untagged:?}"), format!("{blocks:?}"));
}

/// Two paragraphs and a page number drawn as `/Artifact`, outside the tree.
fn artifact_fixture() -> Vec<u8> {
    let mut operations = Vec::new();
    tagged_show(
        &mut operations,
        "P",
        0,
        &Run {
            text: "Body text on the page here.",
            x: MARGIN,
            y: 700.0,
        },
    );
    tagged_show(
        &mut operations,
        "P",
        1,
        &Run {
            text: "More body text for coverage.",
            x: MARGIN,
            y: 676.0,
        },
    );
    artifact_show(
        &mut operations,
        &Run {
            text: "Page 1 of 12",
            x: MARGIN,
            y: 60.0,
        },
    );
    let mut doc = doc_of(operations);
    let mut builder = StructureTreeBuilder::new();
    builder.add_paragraph(0, 1);
    builder.add_paragraph(1, 1);
    builder.build(&mut doc).expect("build structure tree");
    bytes_of(&mut doc)
}

#[test]
fn artifact_content_becomes_furniture() {
    let bytes = artifact_fixture();
    let blocks = blocks_of(&bytes, &tagged_options());

    assert_eq!(
        texts(&blocks),
        vec![
            "Body text on the page here.",
            "More body text for coverage.",
            "Page 1 of 12"
        ]
    );
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
    assert!(matches!(blocks[1].kind, BlockKind::Paragraph));
    assert!(
        matches!(blocks[2].kind, BlockKind::Footer),
        "artifact kind: {:?}",
        blocks[2].kind
    );
    assert!(
        !pdf_engine::to_markdown(&blocks).contains("Page 1"),
        "furniture is not in the markdown"
    );

    // On geometry alone the page number is a paragraph in the flow: the
    // tagged answer differs, so this test cannot pass without the feature.
    let untagged = blocks_of(&bytes, &ExtractOptions::default());
    assert!(pdf_engine::to_markdown(&untagged).contains("Page 1 of 12"));
}
