//! A list is a run of markers at one left edge, and its nesting is the
//! column's own indent steps (#569).
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, paragraph merge, list rule, serializer. Take
//! the rule out of `crate::lists` and each of these goes red the same way --
//! the items come back as `Paragraph`, and the commonest list of all, one line
//! per item set solid at one edge, comes back as a SINGLE paragraph with every
//! bullet inside it.
//!
//! The failure cases the ticket names each have a case of their own: a bullet
//! drawn as a vector path, numbers that are not a list (a date, a section
//! number, a clause reference), an item wrapping to a second line, and a list
//! inside a table.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
/// Where an item's own text begins: far enough right of the marker to be a
/// hanging indent, which is the other half of what a list says about itself.
const TEXT_INDENT: f64 = 14.0;
/// One step of nesting, as this page sets it. Nothing reads the number -- the
/// level comes from the steps the column uses, not from a measurement.
const NEST_STEP: f64 = 28.0;

/// The face a piece of text is set in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Regular,
    Bold,
    /// ZapfDingbats, whose codes draw pictures: the bullet a document sets in
    /// it does not extract as a bullet character, and the font name is the only
    /// evidence left.
    Dingbat,
}

impl Face {
    fn resource(self) -> &'static [u8] {
        match self {
            Face::Regular => b"F1",
            Face::Bold => b"F2",
            Face::Dingbat => b"FZ",
        }
    }
}

/// One run of glyphs at a point: the bytes the page shows, not a string, so a
/// bullet can be written as the WinAnsi code a real document writes it as.
struct Piece {
    bytes: Vec<u8>,
    face: Face,
    x: f64,
    y: f64,
    size: f64,
}

/// A run of ordinary text.
fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        face: Face::Regular,
        x,
        y,
        size: BODY_SIZE,
    }
}

/// A line of body text at the margin.
fn line(body: &str, y: f64) -> Piece {
    text(body, MARGIN, y)
}

/// An item marked with the WinAnsi bullet, at a nesting depth of `depth`
/// steps: the marker in its own run, the text hanging off it.
fn bullet_item(body: &str, y: f64, depth: usize) -> Vec<Piece> {
    let left = MARGIN + depth as f64 * NEST_STEP;
    vec![
        Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: left,
            y,
            size: BODY_SIZE,
        },
        text(body, left + TEXT_INDENT, y),
    ]
}

/// An item marked with `marker` -- `1.`, `iv.`, `(c)` -- at the margin.
fn numbered_item(marker: &str, body: &str, y: f64, depth: usize) -> Vec<Piece> {
    let left = MARGIN + depth as f64 * NEST_STEP;
    vec![text(marker, left, y), text(body, left + TEXT_INDENT, y)]
}

/// A rectangle the page fills, which is how a document draws a bullet that is
/// not a glyph at all.
struct Box2 {
    x: f64,
    y: f64,
}

fn font(doc: &mut Document, base: &[u8], winansi: bool) -> Object {
    let mut dict = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
    };
    if winansi {
        dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    }
    Object::Reference(doc.add_object(Object::Dictionary(dict)))
}

/// A one-page PDF of the pieces given, with any filled boxes drawn under them.
fn pdf_of(pieces: &[Piece], boxes: &[Box2]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica", true);
    let bold = font(&mut doc, b"Helvetica-Bold", true);
    let dingbat = font(&mut doc, b"ZapfDingbats", false);

    let mut operations = Vec::new();
    for filled in boxes {
        operations.push(Operation::new(
            "re",
            vec![filled.x.into(), filled.y.into(), 4.into(), 4.into()],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for piece in pieces {
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

    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular,
                "F2" => bold,
                "FZ" => dingbat,
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

fn blocks_of(pieces: &[Piece]) -> Vec<Block> {
    blocks_with_boxes(pieces, &[])
}

fn blocks_with_boxes(pieces: &[Piece], boxes: &[Box2]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces, boxes))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

fn items(blocks: &[Block]) -> Vec<&Block> {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }))
        .collect()
}

/// The block whose text starts with `prefix`, or a failure naming every block
/// there was.
fn block_starting<'a>(blocks: &'a [Block], prefix: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.text.starts_with(prefix))
        .unwrap_or_else(|| panic!("no block opens {prefix:?}: {:?}", shape(blocks)))
}

/// Five lines of prose at the margin, set solid from `y`. Long enough that a
/// list item beside it is short of the measure, which is how the merge knows
/// an item of one line has ended.
fn prose(y: f64) -> Vec<Piece> {
    [
        "The committee considered the proposals put before it in the order",
        "they had been received, and resolved that each of them be referred",
        "to the officers for a report before the next ordinary meeting.",
    ]
    .iter()
    .enumerate()
    .map(|(i, body)| line(body, y - i as f64 * LEADING))
    .collect()
}

#[test]
fn bullets_set_solid_are_one_item_each() {
    // The case the paragraph merge on its own cannot see: one line per item,
    // one left edge, one leading. Every rail #567 has says "one paragraph".
    let mut pieces = Vec::new();
    for (i, body) in ["apples and pears", "quinces and medlars", "sloes"]
        .iter()
        .enumerate()
    {
        pieces.extend(bullet_item(body, TOP - i as f64 * LEADING, 0));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the three items are not three items: {:?}",
        shape(&blocks)
    );
    for block in items(&blocks) {
        assert_eq!(
            block.kind,
            BlockKind::ListItem {
                level: 0,
                ordinal: None
            },
            "{:?}",
            shape(&blocks)
        );
        assert!(
            !block.text.contains('\u{2022}'),
            "the marker is still in the text: {:?}",
            block.text
        );
    }
    assert_eq!(block_starting(&blocks, "apples").text, "apples and pears");
}

#[test]
fn an_item_that_wraps_is_still_one_item() {
    // The second item runs to a second line, hanging off its own marker. The
    // hang is what says the line belongs to the item above it and not to the
    // list.
    let mut pieces = bullet_item("apples and pears", TOP, 0);
    pieces.extend(bullet_item(
        "quinces, medlars and the several other fruits",
        TOP - LEADING,
        0,
    ));
    pieces.push(text(
        "that will not ripen on the tree",
        MARGIN + TEXT_INDENT,
        TOP - 2.0 * LEADING,
    ));
    pieces.extend(bullet_item("sloes", TOP - 3.0 * LEADING, 0));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "a wrapped item was cut in two, or the list was merged: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        block_starting(&blocks, "quinces").text,
        "quinces, medlars and the several other fruits that will not ripen on the tree",
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn numbering_keeps_the_number_the_document_wrote() {
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [("3.", "apples"), ("4.", "pears"), ("5.", "quinces")]
        .iter()
        .enumerate()
    {
        pieces.extend(numbered_item(marker, body, TOP - i as f64 * LEADING, 0));
    }

    let blocks = blocks_of(&pieces);
    let ordinals: Vec<Option<u32>> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { ordinal, .. } => ordinal,
            _ => unreachable!("filtered to items"),
        })
        .collect();
    assert_eq!(
        ordinals,
        vec![Some(3), Some(4), Some(5)],
        "a list that opens at three was renumbered: {:?}",
        shape(&blocks)
    );
    assert_eq!(block_starting(&blocks, "apples").text, "apples");
}

#[test]
fn a_bare_letter_is_read_the_way_its_own_run_reads_it() {
    // `i.` is the ninth letter and the first roman numeral. Nothing in the
    // marker says which; the run beside it does.
    let mut roman = Vec::new();
    for (i, (marker, body)) in [("i.", "apples"), ("ii.", "pears"), ("iii.", "quinces")]
        .iter()
        .enumerate()
    {
        roman.extend(numbered_item(marker, body, TOP - i as f64 * LEADING, 0));
    }
    let blocks = blocks_of(&roman);
    assert_eq!(
        block_starting(&blocks, "apples").kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: Some(1)
        },
        "i, ii, iii did not count from one: {:?}",
        shape(&blocks)
    );

    let mut latin = Vec::new();
    for (i, (marker, body)) in [("g.", "apples"), ("h.", "pears"), ("i.", "quinces")]
        .iter()
        .enumerate()
    {
        latin.extend(numbered_item(marker, body, TOP - i as f64 * LEADING, 0));
    }
    let blocks = blocks_of(&latin);
    assert_eq!(
        block_starting(&blocks, "quinces").kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: Some(9)
        },
        "the same marker `i.` was read as roman in an alphabetic run: {:?}",
        shape(&blocks)
    );
}

/// An outer list of two items with an inner list of two between them.
///
/// The items are long enough to overlap each other horizontally: two short
/// lines at two different indents are two COLUMNS to the cut that produced
/// them (#567), and a list nested inside a column is still one column.
fn nested_orchard() -> Vec<Piece> {
    let mut pieces = bullet_item("apples of several varieties", TOP, 0);
    pieces.extend(bullet_item("bramley, for cooking", TOP - LEADING, 1));
    pieces.extend(bullet_item("russet, for keeping", TOP - 2.0 * LEADING, 1));
    pieces.extend(bullet_item(
        "pears of two varieties",
        TOP - 3.0 * LEADING,
        0,
    ));
    pieces
}

#[test]
fn nesting_comes_from_the_columns_own_indent_steps() {
    let blocks = blocks_of(&nested_orchard());
    let levels: Vec<u8> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { level, .. } => level,
            _ => unreachable!("filtered to items"),
        })
        .collect();
    assert_eq!(
        levels,
        vec![0, 1, 1, 0],
        "the indent step is not a level: {:?}",
        shape(&blocks)
    );
}

#[test]
fn short_nested_items_are_still_one_list() {
    // Found by review. The items are short enough that an outer one ends before
    // the inner one begins, which is the picture `column_runs` cuts a page on:
    // confirmed inside those slices, each outer marker is alone and the inner
    // pair comes out as a list of its own at level 0. A list is one of the few
    // things on a page whose lines are meant not to line up.
    let mut pieces = bullet_item("Fruit", TOP, 0);
    pieces.extend(bullet_item("Apple", TOP - LEADING, 1));
    pieces.extend(bullet_item("Pear", TOP - 2.0 * LEADING, 1));
    pieces.extend(bullet_item("Vegetables", TOP - 3.0 * LEADING, 0));

    let blocks = blocks_of(&pieces);
    let levels: Vec<u8> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { level, .. } => level,
            _ => unreachable!("filtered to items"),
        })
        .collect();
    assert_eq!(
        levels,
        vec![0, 1, 1, 0],
        "the outer items were lost, or the inner ones took level 0: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_full_width_item_still_ends_where_its_text_column_does() {
    // Found by review. Both rails that ended an item read its WIDTH: an item
    // that ran the full measure was still open, and the paragraph under it --
    // at the marker's own edge, at ordinary leading -- was appended to it. What
    // ends an item is its content column, whatever its first line measured.
    let mut pieces = bullet_item(
        "The committee considered the proposals put before it in turn",
        TOP,
        0,
    );
    pieces.extend(bullet_item(
        "The officers were asked to report before the annual meeting",
        TOP - LEADING,
        0,
    ));
    pieces.extend(prose(TOP - 2.0 * LEADING));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        block_starting(&blocks, "The officers").text,
        "The officers were asked to report before the annual meeting",
        "the paragraph under the list was appended to the last item: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks.iter().any(|b| b.kind == BlockKind::Paragraph
            && b.text
                .starts_with("The committee considered the proposals put before it in the order")),
        "the prose after the list is not a paragraph of its own: {:?}",
        shape(&blocks)
    );
}

/// A paragraph whose first line is indented, set solid from `y`. Three of them
/// are what make a page first-line indented to `indent_style`.
fn indented_paragraph(first: &str, rest: &[&str], y: f64) -> Vec<Piece> {
    let mut pieces = vec![text(first, MARGIN + TEXT_INDENT, y)];
    for (i, body) in rest.iter().enumerate() {
        pieces.push(line(body, y - (i + 1) as f64 * LEADING));
    }
    pieces
}

#[test]
fn an_items_hang_outranks_a_first_line_indented_page() {
    // Found by review. On a page the rest of which is set first-line indented,
    // a hanging continuation is an indented line -- and that rail starts a
    // paragraph on one. The hang is the list's own statement about itself and
    // it has to outrank the page's, or every conventionally hanging item on
    // such a page is split at its second line.
    let mut pieces = indented_paragraph(
        "The committee considered the proposals put before it in the",
        &[
            "order they had been received, and resolved that each of them",
            "be referred to the officers.",
        ],
        TOP,
    );
    pieces.extend(indented_paragraph(
        "The officers were asked to report to the members before the",
        &["next ordinary meeting of the society."],
        TOP - 4.0 * LEADING,
    ));
    pieces.extend(indented_paragraph(
        "The treasurer was asked to lay the accounts of the year before",
        &["the same meeting."],
        TOP - 7.0 * LEADING,
    ));
    pieces.extend(bullet_item(
        "apples and pears of the season",
        TOP - 10.0 * LEADING,
        0,
    ));
    pieces.extend(bullet_item(
        "quinces, medlars and the several other fruits",
        TOP - 11.0 * LEADING,
        0,
    ));
    pieces.push(text(
        "that will not ripen on the tree",
        MARGIN + TEXT_INDENT,
        TOP - 12.0 * LEADING,
    ));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        block_starting(&blocks, "quinces").text,
        "quinces, medlars and the several other fruits that will not ripen on the tree",
        "the item was split at its own hang: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_dingbat_bullet_is_a_bullet_whatever_it_decodes_to() {
    // ZapfDingbats code 0x6C draws a filled circle and extracts as the letter
    // it happens to sit at. The character is no evidence; the font is.
    let mut pieces = Vec::new();
    for (i, body) in ["apples and pears", "quinces and medlars"]
        .iter()
        .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        pieces.push(Piece {
            bytes: vec![0x6C],
            face: Face::Dingbat,
            x: MARGIN,
            y,
            size: BODY_SIZE,
        });
        pieces.push(text(body, MARGIN + TEXT_INDENT, y));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        2,
        "a bullet set in a picture font was not read as one: {:?}",
        shape(&blocks)
    );
    assert_eq!(block_starting(&blocks, "apples").text, "apples and pears");
}

#[test]
fn a_bullet_drawn_as_a_path_leaves_a_paragraph() {
    // The refusal the module states out loud: a square filled with `re f` is
    // not a glyph, never reaches the text device, and leaves nothing to find.
    // The text is still whole -- what is lost is the kind, not the words.
    let mut pieces = Vec::new();
    let mut boxes = Vec::new();
    for (i, body) in ["apples and pears", "quinces and medlars", "sloes"]
        .iter()
        .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        boxes.push(Box2 { x: MARGIN, y });
        pieces.push(text(body, MARGIN + TEXT_INDENT, y));
    }

    let blocks = blocks_with_boxes(&pieces, &boxes);
    assert!(
        items(&blocks).is_empty(),
        "a filled rectangle was read as a marker: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks.iter().any(|b| b.text.contains("apples and pears")),
        "the text went missing with the kind: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_section_number_is_not_a_marker() {
    // `2.1` and `1.5` carry a digit and a full stop and neither opens a list:
    // an outline's clause starts afresh, with a capital, and a sentence that
    // wrapped before a decimal carries on in lower case -- so `2.2 shall
    // report` is not a clause, and one `2.1 The committee` is no run (#599).
    let pieces = vec![
        line(
            "2.1 The committee shall meet four times in each year and",
            TOP,
        ),
        line(
            "2.2 shall report to the members before each annual meeting.",
            TOP - LEADING,
        ),
        line(
            "1.5 million tonnes were landed in the year under review.",
            TOP - 3.0 * LEADING,
        ),
    ];

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a section number opened a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_clause_reference_repeated_is_not_a_list() {
    // Two markers that agree in kind at one edge, and the value does not
    // climb: the run rule is what tells a list from a reference used twice.
    let pieces = vec![
        line(
            "(a) the officers shall keep a register of the members and",
            TOP,
        ),
        line(
            "(a) above shall be read with the regulations made under it.",
            TOP - 3.0 * LEADING,
        ),
    ];

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a repeated clause reference was read as a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn one_marker_on_its_own_is_not_a_list() {
    // A single candidate is a coincidence, and the page has to say it twice.
    let mut pieces = vec![line(
        "12. January was the coldest month the station had recorded,",
        TOP,
    )];
    pieces.extend(prose(TOP - LEADING));

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "one line opening with a number made a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_numbered_heading_is_not_an_item() {
    // The composition with #568, end to end: a run of agreeing markers at one
    // left edge that is NOT a list, because the page sets it above its own
    // body. On a page like this one the cut alone is enough -- it puts each
    // heading in a region of its own, where no second marker can agree with it
    // -- so the rail that refuses a heading INSIDE a column is pinned where it
    // can be reached, in `blocks::tests::a_numbered_heading_inside_a_column_is_
    // not_a_list_item`. What this asserts is the outcome both rails exist for.
    let heading = |body: &str, y: f64| Piece {
        bytes: body.as_bytes().to_vec(),
        face: Face::Bold,
        x: MARGIN,
        y,
        size: 14.0,
    };
    let mut pieces = vec![heading("3. Method", TOP)];
    pieces.extend(prose(TOP - 2.0 * LEADING));
    pieces.push(heading("4. Results", TOP - 8.0 * LEADING));
    pieces.extend(prose(TOP - 10.0 * LEADING));

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a numbered heading was read as a list item: {:?}",
        shape(&blocks)
    );
    for heading in ["3. Method", "4. Results"] {
        assert!(
            matches!(
                block_starting(&blocks, heading).kind,
                BlockKind::Heading { .. }
            ),
            "{heading:?} lost its heading: {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn a_list_in_a_table_stays_text() {
    // Three columns and two rows, the first cell of each row bulleted. The
    // page has said "row and column" with a gutter, and that is the stronger
    // statement: the line is a row of cells, not an item.
    const COLUMN_X: [f64; 3] = [60.0, 260.0, 440.0];
    let mut pieces = Vec::new();
    for (row, cells) in [
        ["apples", "in season", "Kent"],
        ["pears", "in season", "Sussex"],
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - row as f64 * LEADING;
        pieces.push(Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: COLUMN_X[0],
            y,
            size: BODY_SIZE,
        });
        for (column, cell) in cells.iter().enumerate() {
            let x = COLUMN_X[column] + if column == 0 { TEXT_INDENT } else { 0.0 };
            pieces.push(text(cell, x, y));
        }
    }

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a bulleted cell was read as a list item: {:?}",
        shape(&blocks)
    );
}

#[test]
fn the_prose_after_a_list_is_not_the_last_item() {
    // A one-line item that stopped well short of the measure is finished, and
    // the paragraph under it is at the item's own left edge rather than under
    // its hang.
    let mut pieces = bullet_item("apples and pears", TOP, 0);
    pieces.extend(bullet_item("quinces and medlars", TOP - LEADING, 0));
    pieces.extend(prose(TOP - 2.0 * LEADING));

    let blocks = blocks_of(&pieces);
    assert_eq!(items(&blocks).len(), 2, "{:?}", shape(&blocks));
    assert_eq!(
        block_starting(&blocks, "quinces").text,
        "quinces and medlars",
        "the paragraph after the list was swallowed by the last item: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks
            .iter()
            .any(|b| b.kind == BlockKind::Paragraph && b.text.starts_with("The committee")),
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn markdown_carries_the_items_and_their_nesting() {
    let blocks = blocks_of(&nested_orchard());
    assert_eq!(
        to_markdown(&blocks),
        "- apples of several varieties\n  - bramley, for cooking\n  - russet, for keeping\n- pears of two varieties\n",
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn markdown_nests_under_a_wide_marker_by_its_own_width() {
    // Found by review. A child indented by a constant falls out of a parent
    // whose marker is wider than the constant: `100. ` is five columns, and
    // four spaces under it is a sibling paragraph rather than a nested list.
    // A three-digit marker is wider than `TEXT_INDENT`, so this page sets its
    // own: what is under test is the column the SERIALIZER writes.
    let wide = 26.0;
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [
        ("100.", "apples of several varieties"),
        ("101.", "pears of two varieties which keep"),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        pieces.push(text(marker, MARGIN, y));
        pieces.push(text(body, MARGIN + wide, y));
    }
    pieces.extend(bullet_item(
        "bramley, a cooking apple",
        TOP - 2.0 * LEADING,
        1,
    ));
    pieces.extend(bullet_item(
        "russet, a keeping apple",
        TOP - 3.0 * LEADING,
        1,
    ));

    let markdown = to_markdown(&blocks_of(&pieces));
    assert!(
        markdown.contains("\n     - bramley, a cooking apple\n"),
        "a child of `101. ` is not at its parent's content column: {markdown:?}"
    );
}

#[test]
fn markdown_numbers_an_ordered_list_the_way_the_page_does() {
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [("3.", "apples"), ("4.", "pears")].iter().enumerate() {
        pieces.extend(numbered_item(marker, body, TOP - i as f64 * LEADING, 0));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        to_markdown(&blocks),
        "3. apples\n4. pears\n",
        "{:?}",
        shape(&blocks)
    );
}

// ---------------------------------------------------------------------------
// The three shapes #578 found in the structured benchmark's tail: a
// bibliography, a reference numbered with a raised figure, and a bullet drawn
// as a letter in a font whose name says nothing. Each page is built from
// operators, so what goes red without the fix is the whole path.
// ---------------------------------------------------------------------------

/// A piece at a size and a face of its own, which is what a raised figure and
/// a marker set apart both need.
fn set(body: &str, x: f64, y: f64, size: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        face: Face::Regular,
        x,
        y,
        size,
    }
}

#[test]
fn a_bibliography_numbered_in_brackets_is_a_list() {
    // The commonest list in a paper, and the one the grammar could not read:
    // the brackets are the delimiter. Take the square pair out of
    // `lists::ordinal_at` and these come back as two paragraphs -- the merge
    // runs them together, because nothing tells it where one reference ends.
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [
        ("[3]", "S. Khalil, J. Phys. G 35, 055001 (2008)."),
        (
            "[4]",
            "S. Iso, N. Okada and Y. Orikasa, Phys. Lett. B 676, 81 (2009).",
        ),
        (
            "[5]",
            "W. Emam and S. Khalil, Eur. Phys. J. C 522, 625 (2007).",
        ),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        pieces.push(text(marker, MARGIN, y));
        pieces.push(text(body, MARGIN + 18.0, y));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the three references are not three items: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        blocks[0].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: Some(3),
        },
        "the page's own number was not kept: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks[0].text.starts_with("S. Khalil"),
        "the marker is still in the item's text: {:?}",
        blocks[0].text
    );
}

#[test]
fn a_reference_numbered_with_a_raised_figure_is_a_list() {
    // No delimiter at all, which is how a physics journal sets it. The
    // evidence is the type -- smaller than the line and above its baseline --
    // so the same figure set IN the line is not a marker, which is the case
    // below.
    let raised = |marker: &str, body: &str, y: f64| {
        vec![
            set(marker, MARGIN, y + 3.0, BODY_SIZE * 0.7),
            set(body, MARGIN + 10.0, y, BODY_SIZE),
        ]
    };
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [
        (
            "1",
            "Y. K. Kato, R. C. Myers, A. C. Gossard, Science 306, 1910 (2004).",
        ),
        (
            "2",
            "V. Sih, R. C. Myers, Y. K. Kato, Nature Physics 1, 31 (2005).",
        ),
        (
            "3",
            "J. Sinova, D. Culcer, Q. Niu, Phys. Rev. Lett. 92, 126603 (2004).",
        ),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(raised(marker, body, TOP - i as f64 * LEADING));
    }
    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the three references are not three items: {:?}",
        shape(&blocks)
    );

    // The same page with the figures set in the line: no raised type, no
    // marker, and the mutation test for the rule above.
    let mut flat = Vec::new();
    for (i, (marker, body)) in [
        (
            "1",
            "Y. K. Kato, R. C. Myers, A. C. Gossard, Science 306, 1910 (2004).",
        ),
        (
            "2",
            "V. Sih, R. C. Myers, Y. K. Kato, Nature Physics 1, 31 (2005).",
        ),
        (
            "3",
            "J. Sinova, D. Culcer, Q. Niu, Phys. Rev. Lett. 92, 126603 (2004).",
        ),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        flat.push(set(marker, MARGIN, y, BODY_SIZE));
        flat.push(set(body, MARGIN + 10.0, y, BODY_SIZE));
    }
    assert!(
        items(&blocks_of(&flat)).is_empty(),
        "a figure set in the line was read as a marker: {:?}",
        shape(&blocks_of(&flat))
    );
}

#[test]
fn a_bullet_drawn_as_a_letter_is_a_bullet_when_the_page_sets_it_apart() {
    // A dingbat subset whose name says nothing: the bullet decodes to `t`.
    // What is left is the page -- one glyph, in a run of its own, with a gap
    // after it no word space could be -- and the rule pairs it only with the
    // same glyph again.
    let apart = |glyph: &str, body: &str, y: f64| {
        vec![text(glyph, MARGIN, y), text(body, MARGIN + TEXT_INDENT, y)]
    };
    let mut pieces = Vec::new();
    for (i, body) in [
        "Gross margins in North America improved over the prior year.",
        "Income from our Asia Pacific Zone, which includes China and Japan.",
        "Our transformational reorganization efforts in Europe gained pace.",
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(apart("t", body, TOP - i as f64 * LEADING));
    }
    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the three items are not three items: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        blocks[0].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: None,
        },
        "a bullet was given a number: {:?}",
        shape(&blocks)
    );

    // Two different letters at one edge are a numbering scheme this file does
    // not read, and never a bulleted list.
    let mut headings = Vec::new();
    for (i, (glyph, body)) in [
        (
            "A",
            "Introduction to the year under review and its results.",
        ),
        ("B", "Method of consolidation and the basis of preparation."),
    ]
    .iter()
    .enumerate()
    {
        headings.extend(apart(glyph, body, TOP - i as f64 * LEADING));
    }
    assert!(
        items(&blocks_of(&headings)).is_empty(),
        "two different letters were read as one bulleted list: {:?}",
        shape(&blocks_of(&headings))
    );
}

#[test]
fn a_bibliography_past_its_hundredth_entry_keeps_its_markers() {
    // The cap on a raised figure is the grammar's three digits and not the two
    // a picture-font glyph is allowed: a paper's reference list passes a
    // hundred entries often enough, and a marker lost at 100 splits one list
    // into two.
    let mut pieces = Vec::new();
    for (i, (marker, body)) in [
        (
            "100",
            "Y. K. Kato, R. C. Myers, A. C. Gossard, Science 306, 1910 (2004).",
        ),
        (
            "101",
            "V. Sih, R. C. Myers, Y. K. Kato, Nature Physics 1, 31 (2005).",
        ),
        (
            "102",
            "J. Sinova, D. Culcer, Q. Niu, Phys. Rev. Lett. 92, 126603 (2004).",
        ),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - i as f64 * LEADING;
        pieces.push(set(marker, MARGIN, y + 3.0, BODY_SIZE * 0.7));
        pieces.push(set(body, MARGIN + 17.0, y, BODY_SIZE));
    }
    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the three references are not three items: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        blocks[0].kind,
        BlockKind::ListItem {
            level: 0,
            ordinal: Some(100),
        },
        "the page's own number was not kept: {:?}",
        shape(&blocks)
    );
}

/// A bullet that takes its trailing space with it is still a bullet (#578).
///
/// A dingbat drawn in a run of its own often carries the spacing that sets it
/// apart INSIDE that run, so the span reads `"t "` rather than `"t"`. The rule
/// in `crate::blocks` that offers the candidate counts characters and ignores
/// the space; the grammar that reads it counted the space as a second glyph
/// and threw the marker away. Every item of such a list came back as a
/// paragraph.
///
/// Stop trimming the run before it is read and the three items are three
/// paragraphs again.
#[test]
fn a_bullet_that_carries_its_own_spacing_is_still_a_bullet() {
    let apart = |glyph: &str, body: &str, y: f64| {
        vec![text(glyph, MARGIN, y), text(body, MARGIN + TEXT_INDENT, y)]
    };
    let mut pieces = Vec::new();
    for (i, body) in [
        "Gross margins in North America improved over the prior year.",
        "Income from our Asia Pacific Zone, which includes China and Japan.",
        "Our transformational reorganization efforts in Europe gained pace.",
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(apart("t ", body, TOP - i as f64 * LEADING));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "a bullet drawn with its own spacing lost its list: {:?}",
        shape(&blocks)
    );
}

/// A bullet whose gap is a space run of its own is still set apart (#574).
///
/// NYSE_MSA on the rig draws its bullet as a bold `t`, then the space after it
/// as a run of its own, then the item: the gap the rule measures was to the
/// space, which starts where the glyph ends, so none of the six items was
/// found. Measure to the next span that draws a character and they are.
#[test]
fn a_bullet_whose_gap_is_a_space_run_is_still_a_bullet() {
    let apart = |body: &str, y: f64| {
        vec![
            Piece {
                bytes: b"t".to_vec(),
                face: Face::Bold,
                x: MARGIN,
                y,
                size: BODY_SIZE,
            },
            text(" ", MARGIN + 3.5, y),
            text(body, MARGIN + TEXT_INDENT, y),
        ]
    };
    let mut pieces = Vec::new();
    for (i, body) in [
        "Gross margins in North America improved over the prior year.",
        "Income from our Asia Pacific Zone, which includes China and Japan.",
        "Our transformational reorganization efforts in Europe gained pace.",
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(apart(body, TOP - i as f64 * LEADING));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "a bullet set apart by a space run lost its list: {:?}",
        shape(&blocks)
    );
}

/// A head of figures opens a section as a head of words does (codex, #2041).
///
/// `2024` set large over a list in small type low on the page: the list is a
/// section of its own, not the page's foot, and its entries are items.
#[test]
fn a_list_under_a_head_of_figures_is_not_a_foot() {
    let mut pieces = prose(TOP);
    pieces.push(set("2024", MARGIN, 160.0, 14.0));
    for (k, words) in [
        "(a) Revenue from the sale of vessels to third parties.",
        "(b) Charter income earned under long-term contracts.",
        "(c) Management fees charged to joint ventures.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(set(words, MARGIN, 145.0 - k as f64 * 10.0, 8.0));
    }
    let mut options = ExtractOptions::default();
    options.learned = false;
    let blocks = PdfDocument::open(pdf_of(&pieces, &[]))
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks");
    assert_eq!(
        items(&blocks).len(),
        3,
        "a list under a head of figures was taken for footnotes: {:?}",
        shape(&blocks)
    );
}

/// Four numbered paragraphs of three lines each, the marker glued into the
/// first line and the rest set back at the margin, `gap` apart.
fn numbered_paragraphs(top: f64, gap: f64) -> Vec<Piece> {
    let bodies = [
        [
            "Certain access revenue rate elements billed by the Company to the",
            "interexchange carriers were disputed and subsequently settled in the",
            "fourth quarter, which increased revenue by one million dollars.",
        ],
        [
            "The Company changed its employee vacation policy so that employees",
            "now earn and use their vacation benefits in the same calendar year",
            "and the accrued liability was reduced accordingly in the quarter.",
        ],
        [
            "The Company adjusted its estimate of deferred revenue for the PCS",
            "operation in the fourth quarter of the year, which resulted in the",
            "recognition of additional revenue that had previously been held.",
        ],
        [
            "The Company received a reimbursement from Sprint related to the",
            "prior year for its portion of the charges billed to the operation",
            "and recorded the amount as a reduction of operating expenses here.",
        ],
    ];
    let mut pieces = Vec::new();
    let mut y = top;
    for (n, body) in bodies.iter().enumerate() {
        for (k, words) in body.iter().enumerate() {
            let words = if k == 0 {
                format!("{}. {words}", n + 1)
            } else {
                (*words).to_string()
            };
            pieces.push(line(&words, y));
            y -= LEADING;
        }
        y -= gap;
    }
    pieces
}

#[test]
fn numbered_paragraphs_a_blank_line_apart_are_still_one_list() {
    // The XY-cut splits a column at every band of white a line tall, so each of
    // these paragraphs arrives as a region of its own -- and a run of one marker
    // confirms nothing. The page's own column is what the run is read over
    // (#589): an annual report lost 12 of its 16 list items to this.
    let mut pieces = prose(TOP);
    pieces.extend(numbered_paragraphs(TOP - 3.0 * LEADING - LEADING, LEADING));

    let blocks = blocks_of(&pieces);
    let found: Vec<Option<u32>> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { ordinal, .. } => ordinal,
            _ => None,
        })
        .collect();
    assert_eq!(
        found,
        vec![Some(1), Some(2), Some(3), Some(4)],
        "the four paragraphs are not four items: {:?}",
        shape(&blocks)
    );
    assert!(
        block_starting(&blocks, "Certain access")
            .text
            .ends_with("one million dollars."),
        "an item lost its continuation lines: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_list_in_the_next_column_is_not_nested_under_this_one() {
    // Reading down one column and then the next is the stack; reading ACROSS is
    // not. A list in the left column and another in the right are two lists at
    // level 0 each. Read as one stack, the right column's edge becomes an
    // indent step of the left column's list and its items come out one level
    // deep. The right list is set just lower than the left one ends -- close
    // enough that no gap ends the stack -- so "it stands below" is true of it
    // and only "it stands in the same column" can tell the two apart (#589).
    let right = 330.0;
    let lower = TOP - 2.5 * LEADING;
    let mut pieces = Vec::new();
    for (k, body) in ["apples and pears", "quinces and medlars"]
        .iter()
        .enumerate()
    {
        pieces.extend(bullet_item(body, TOP - k as f64 * LEADING, 0));
    }
    for (k, body) in ["plums and damsons", "cherries and sloes"]
        .iter()
        .enumerate()
    {
        let y = lower - k as f64 * LEADING;
        pieces.push(Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: right,
            y,
            size: BODY_SIZE,
        });
        pieces.push(text(body, right + TEXT_INDENT, y));
    }

    let blocks = blocks_of(&pieces);
    let levels: Vec<u8> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { level, .. } => level,
            _ => u8::MAX,
        })
        .collect();
    assert_eq!(
        levels,
        vec![0, 0, 0, 0],
        "the second column's list was nested under the first: {:?}",
        shape(&blocks)
    );
}

/// Notes to a set of accounts: a marked sub-head of one short line, then its
/// paragraph, set either back at the marker's edge or hanging under the
/// sub-head's text.
fn notes_to_the_accounts(hanging: bool) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = TOP;
    for (marker, head, body) in [
        (
            "(a)",
            "Accounts Payable",
            [
                "Trade payables and other accounts payable are recognised when",
                "the consolidated entity becomes obliged to make future payments.",
            ],
        ),
        (
            "(b)",
            "Acquisition of Assets",
            [
                "Assets acquired are recorded at the cost of acquisition, being",
                "the purchase consideration determined as at the date of purchase.",
            ],
        ),
        (
            "(c)",
            "Borrowings",
            [
                "Bills of exchange and loans are recorded at the principal amount",
                "and interest is recognised as an expense as it accrues to date.",
            ],
        ),
    ] {
        pieces.extend(numbered_item(marker, head, y, 0));
        y -= LEADING;
        let left = if hanging {
            MARGIN + TEXT_INDENT
        } else {
            MARGIN
        };
        for words in body {
            pieces.push(text(words, left, y));
            y -= LEADING;
        }
        y -= LEADING;
    }
    pieces
}

#[test]
fn numbered_sub_heads_over_their_own_paragraphs_are_not_a_list() {
    // `(a) Accounts Payable`, a paragraph, `(b) Acquisition of Assets`, a
    // paragraph -- all in body type, the markers climbing. Each marked line is
    // short and the next line is not a marker: it is the sub-head's paragraph,
    // wherever the page starts it. The annotation draws these as sub-heads,
    // and an annual report gave us 40 false items of exactly this shape
    // (#589).
    for hanging in [false, true] {
        let blocks = blocks_of(&notes_to_the_accounts(hanging));
        assert!(
            items(&blocks).is_empty(),
            "numbered sub-heads were read as a list (paragraph hanging: {hanging}): {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn a_column_of_money_is_not_a_bulleted_list() {
    // `$` set apart from its figure, row after row down one edge, is every
    // property a glyph bullet has. It is the first column of a table of money,
    // and an annual report's statements gave us 27 false items of it once the
    // list was read down the whole column (#589).
    let mut pieces = prose(TOP);
    for (k, (first, second)) in [
        ("5,091", "2,927"),
        ("152,585", "133,644"),
        ("28,696", "2,209"),
        ("40,963", "20,494"),
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - 4.0 * LEADING - k as f64 * LEADING;
        pieces.push(text("$", MARGIN, y));
        pieces.push(text(&format!("{first} $ {second}"), MARGIN + 11.0, y));
    }

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a column of dollar signs was read as bullets: {:?}",
        shape(&blocks)
    );
}

#[test]
fn footnotes_under_the_body_are_not_a_list() {
    // Legislation: body text, then under it `(a)`, `(b)`, `(c)` in smaller type.
    // In a region of their own the small type is that region's body, so only
    // the page can say they are small -- and set below everything in the
    // page's body size, they are its foot (#589).
    let mut pieces = prose(TOP);
    let small = 8.0;
    for (k, words) in [
        "(a) 1990 c. 8. Amendments have been made that are not relevant here.",
        "(b) S.I. 2015/596, which has been amended by S.I. 2016/332.",
        "(c) 2006 c. 41.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(set(words, MARGIN, 120.0 - k as f64 * 10.0, small));
    }
    // And under them, in body size, what legislation prints at the foot of
    // every page: the page number. The last body-size LINE is then below the
    // footnotes, and a rule that measured from it saw no foot at all.
    pieces.push(text("3", 300.0, 60.0));

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "footnotes were read as a list: {:?}",
        shape(&blocks)
    );
}

/// The rule over a page's footnotes, drawn as a line of blank glyphs, is not
/// a head over them (#574).
///
/// UK legislation sets the separator as a run of spaces in a size larger than
/// the body. Read as a line, that is larger type with no words -- the shape a
/// section head has to the foot rule -- and every footnote under it was a
/// list item again: twelve false items on one rig page. A head says something.
#[test]
fn footnotes_under_a_rule_drawn_in_blanks_are_not_a_list() {
    let mut pieces = prose(TOP);
    // The rule's baseline sits less than a line of its own size above the
    // first footnote, as on the page, so the "head" reaches down over it.
    pieces.push(set(&" ".repeat(80), MARGIN, 140.0, 12.0));
    let small = 8.0;
    for (k, words) in [
        "(a) S.I. 2013/376.",
        "(b) Regulation 62 was substituted by S.I. 2014/2888 and amended.",
        "(c) S.I. 2020/234.",
        "(d) S.I. 2014/1230. Regulations 54 and 55 were inserted by S.I. 2019/1152.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(set(words, MARGIN, 130.4 - k as f64 * 9.0, small));
    }

    // The block model's own reading, before the learned pass: that is where
    // the foot is decided, and the learned list model can only undo so much.
    let mut options = ExtractOptions::default();
    options.learned = false;
    let blocks = PdfDocument::open(pdf_of(&pieces, &[]))
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks");
    assert!(
        items(&blocks).is_empty(),
        "footnotes under a rule of blanks were read as a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_glued_marker_keeps_its_hanging_continuation() {
    // Legislation sets `(i) the height of ...` with the marker inside the line's
    // first span, so there is no text column to measure the hang against, and
    // the item's second line hangs under its text. On a page set first-line
    // indented that second line is an indented line, and the page's rail
    // started a paragraph on it: an order of 2020 had five items cut after
    // their first line, each leaving its continuation as a stray paragraph
    // (#589).
    let mut pieces = indented_paragraph(
        "The committee considered the proposals put before it in the",
        &[
            "order they had been received, and resolved that each of them",
            "be referred to the officers.",
        ],
        TOP,
    );
    pieces.extend(indented_paragraph(
        "The officers were asked to report to the members before the",
        &["next ordinary meeting of the society."],
        TOP - 4.0 * LEADING,
    ));
    let hang = MARGIN + 20.0;
    let y = TOP - 7.0 * LEADING;
    pieces.push(line(
        "(i) the height of the highest part of the roof of the original building,",
        y,
    ));
    pieces.push(text(
        "or a height of 6 metres above the ground, whichever is the greater;",
        hang,
        y - LEADING,
    ));
    pieces.push(line(
        "(ii) the height of the highest part of the roof of the original building,",
        y - 2.0 * LEADING,
    ));
    pieces.push(text(
        "or a height of 18 metres above the ground, whichever is the greater.",
        hang,
        y - 3.0 * LEADING,
    ));

    let blocks = blocks_of(&pieces);
    assert_eq!(items(&blocks).len(), 2, "{:?}", shape(&blocks));
    assert!(
        block_starting(
            &blocks,
            "the height of the highest part of the roof of the original building, or a height of 6"
        )
        .text
        .ends_with("whichever is the greater;"),
        "the first item was cut at its first line: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_short_clause_that_runs_on_is_an_item_not_a_label() {
    // `(a) where the land is,` is as short as a sub-head and has a non-marker
    // line under it, which is everything the label rule looks at on the page.
    // What it says in its words is that the clause is not finished: it ends
    // in a comma, and the line under it is the rest of it (#589).
    let mut pieces = prose(TOP);
    let hang = MARGIN + 20.0;
    let y = TOP - 4.0 * LEADING;
    pieces.push(line("(a) where the land is,", y));
    pieces.push(text(
        "used for agriculture, the date on which that use ceased;",
        hang,
        y - LEADING,
    ));
    pieces.push(line("(b) where the building is,", y - 2.0 * LEADING));
    pieces.push(text(
        "occupied, the date on which it was first occupied.",
        hang,
        y - 3.0 * LEADING,
    ));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        2,
        "two clauses that run on were read as sub-heads: {:?}",
        shape(&blocks)
    );
}

#[test]
fn bullets_with_a_description_under_each_are_still_a_list() {
    // `• Apples`, its description hanging under it, `• Pears`, its description:
    // every item is a short line with an unmarked line under it, which is the
    // shape of a numbered sub-head -- but a bullet counts nothing, and this is
    // the commonest list there is (codex, #1950).
    let mut pieces = prose(TOP);
    let y = TOP - 4.0 * LEADING;
    pieces.extend(bullet_item("Apples", y, 0));
    pieces.push(text(
        "Picked in the autumn and kept in a cool store over winter.",
        MARGIN + TEXT_INDENT,
        y - LEADING,
    ));
    pieces.extend(bullet_item("Pears", y - 2.0 * LEADING, 0));
    pieces.push(text(
        "Picked before they are ripe and ripened indoors for a week.",
        MARGIN + TEXT_INDENT,
        y - 3.0 * LEADING,
    ));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        2,
        "wrapped bullets were refused: {:?}",
        shape(&blocks)
    );
}

#[test]
fn references_under_their_heading_at_the_foot_of_a_page_are_a_list() {
    // A bibliography that opens low on a page: body text, a `References` head,
    // then the entries in smaller type. The entries are smaller than the page's
    // body and below its last line of running text, which is everything a
    // footnote is -- except that a heading stands between the two, and nothing
    // opens a block of footnotes with a heading (codex, #1950).
    let mut pieces = prose(TOP);
    pieces.push(Piece {
        bytes: b"References".to_vec(),
        face: Face::Bold,
        x: MARGIN,
        y: TOP - 4.0 * LEADING,
        size: BODY_SIZE,
    });
    let small = 8.0;
    for (k, words) in [
        "[1] A. Smith, On the reading of pages, J. Doc. Eng. 12 (2004) 1-20.",
        "[2] B. Jones, Lists and their markers, Proc. DocEng (2008) 33-41.",
        "[3] C. Brown, The layout of legislation, Law Libr. J. 9 (2011) 7-19.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(set(
            words,
            MARGIN,
            TOP - 5.5 * LEADING - k as f64 * 10.0,
            small,
        ));
    }

    let blocks = blocks_of(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the references were read as a foot: {:?}",
        shape(&blocks)
    );
}

#[test]
fn an_indented_paragraph_after_a_short_glued_item_is_not_its_continuation() {
    // `1. Apples`, `2. Pears`, then the page's next paragraph, first-line
    // indented. The hang rule for a glued marker takes a line that hangs under
    // the item as the item's own -- but a short item that says nothing more
    // is finished, and the indent under it is the page's paragraph shape
    // (codex, #1950).
    let mut pieces = indented_paragraph(
        "The committee considered the proposals put before it in the",
        &[
            "order they had been received, and resolved that each of them",
            "be referred to the officers.",
        ],
        TOP,
    );
    pieces.extend(indented_paragraph(
        "The officers were asked to report to the members before the",
        &["next ordinary meeting of the society."],
        TOP - 4.0 * LEADING,
    ));
    let y = TOP - 7.0 * LEADING;
    pieces.push(line("1. Apples", y));
    pieces.push(line("2. Pears", y - LEADING));
    pieces.extend(indented_paragraph(
        "The treasurer was asked to lay the accounts of the year before",
        &["the same meeting."],
        y - 2.0 * LEADING,
    ));

    let blocks = blocks_of(&pieces);
    assert_eq!(
        block_starting(&blocks, "Pears").text,
        "Pears",
        "the paragraph after the list was merged into its last item: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_second_list_further_down_the_column_starts_at_level_zero() {
    // Two lists in one column, well apart, the second set further in. Read as
    // one stack, the second list's edge became an indent step of the first
    // and it came out nested. A stack is a list's own spacing, not a whole
    // column: a gap of several lines ends it (codex, #1950).
    let mut pieces = Vec::new();
    for (k, body) in ["apples and pears", "quinces and medlars"]
        .iter()
        .enumerate()
    {
        pieces.extend(bullet_item(body, TOP - k as f64 * LEADING, 0));
    }
    pieces.extend(prose(TOP - 6.0 * LEADING));
    for (k, body) in ["plums and damsons", "cherries and sloes"]
        .iter()
        .enumerate()
    {
        pieces.extend(bullet_item(body, TOP - (14.0 + k as f64) * LEADING, 1));
    }

    let blocks = blocks_of(&pieces);
    let levels: Vec<u8> = items(&blocks)
        .iter()
        .map(|b| match b.kind {
            BlockKind::ListItem { level, .. } => level,
            _ => u8::MAX,
        })
        .collect();
    assert_eq!(levels, vec![0, 0, 0, 0], "{:?}", shape(&blocks));
}

#[test]
fn footnotes_longer_than_the_body_are_still_a_foot() {
    // Near the end of a section the footnotes can hold more characters than
    // the text above them, and a character-weighted median then calls the
    // footnote size the page's body -- after which nothing is smaller than
    // the body and the foot is read as a list again (codex, #1950).
    let mut pieces = prose(TOP);
    let small = 8.0;
    for (k, words) in [
        "(a) 1990 c. 8. Amendments have been made to section 59 which are not relevant to this Order.",
        "(b) S.I. 2015/596, which has been amended by S.I. 2016/332, S.I. 2016/1040 and S.I. 2017/391.",
        "(c) The Care Quality Commission was established by section 1 of the Social Care Act 2008.",
        "(d) Health Education England was established by section 96 of the Care Act 2014 (c. 23).",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(set(words, MARGIN, 140.0 - k as f64 * 10.0, small));
    }
    pieces.push(text("3", 300.0, 60.0));

    let blocks = blocks_of(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "footnotes were read as a list: {:?}",
        shape(&blocks)
    );
}

/// The blocks with the block model switched as given and the line model off,
/// so each test here isolates the one switch it names (#599).
fn blocks_learned(pieces: &[Piece], list_item: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(pieces, &[])).expect("open pdf");
    let before = doc.extract_blocks_before_learning().expect("blocks");
    pdf_engine::learned::refine_with(
        before,
        &[(612.0, 792.0)],
        pdf_engine::learned::Switches {
            list_item,
            list_opens: false,
            heading_lines: false,
            captions: false,
            line_gnn: false,
            line_image: false,
            line_gnn_v2: false,
            demote_per_cell: false,
            table_region: false,
            table_structure: false,
            table_judge: false,
            table_lattice: false,
            table_stitch_header: false,
            table_stitch_rows: false,
            table_stitch_merge: false,
            table_is_v2: false,
            table_drop_empty_columns: false,
            table_cell_text_by_line: false,
            table_v2_release_rejoin: false,
            table_v2_release_column_order: false,
            table_split_side_by_side: false,
            table_split_stacked: false,
            table_runs_on_figures: false,
            markdown_header_block_figures: false,
            table_corridor_split: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
        },
    )
}

#[test]
fn one_bullet_after_a_colon_is_an_item_when_the_model_says_so() {
    // A single bulleted line after "the following matters:" -- one marker is
    // no run, so the heuristics leave it a paragraph (`one_marker_on_its_own_
    // is_not_a_list`). The block model, trained on DocLayNet, reads the bullet
    // at its head and the short, set-apart line and calls it an item (#599).
    // Red with the list_item switch off.
    let mut pieces = vec![
        line(
            "The committee considered the proposals put before it in the order",
            TOP,
        ),
        line(
            "they had been received, and resolved that the officers report on",
            TOP - LEADING,
        ),
        line(
            "the following matters before the next ordinary meeting of it:",
            TOP - 2.0 * LEADING,
        ),
    ];
    pieces.push(Piece {
        bytes: b"\x95  that the accounts of the year be laid before the members.".to_vec(),
        face: Face::Regular,
        x: MARGIN,
        y: TOP - 3.7 * LEADING,
        size: BODY_SIZE,
    });
    pieces.push(line(
        "The treasurer agreed to do so and the meeting then closed at nine.",
        TOP - 6.0 * LEADING,
    ));

    let on = blocks_learned(&pieces, true);
    let item = block_starting(&on, "that the accounts");
    assert!(
        matches!(item.kind, BlockKind::ListItem { ordinal: None, .. }),
        "the bullet after the colon is not an item with the model on: {:?}",
        shape(&on)
    );
    let off = blocks_learned(&pieces, false);
    assert!(
        items(&off).is_empty(),
        "the heuristics alone made it an item: {:?}",
        shape(&off)
    );
}

#[test]
fn a_numbered_column_of_figures_is_taken_out_of_the_list_whole() {
    // `1. 4,512` and `2. 5,118` in a narrow column beside the body: two
    // agreeing, climbing markers, so the heuristics confirm a list. It is a
    // column of numbered figures. The model scores the run low (a mean near
    // 0.10, under the 0.15 bound) and takes it out -- both rows, never one
    // (#599). Red with the list_item switch off.
    let pieces = vec![
        line(
            "Body text of the page runs here for a while, filling the measure out.",
            TOP + 20.0,
        ),
        text("1. 4,512", 480.0, TOP),
        text("2. 5,118", 480.0, TOP - LEADING),
    ];
    let off = blocks_learned(&pieces, false);
    assert_eq!(
        items(&off).len(),
        2,
        "the heuristics no longer confirm the column: {:?}",
        shape(&off)
    );
    let on = blocks_learned(&pieces, true);
    assert!(
        items(&on).is_empty(),
        "the column of figures stayed a list: {:?}",
        shape(&on)
    );
    assert!(
        on.iter().any(|b| b.text == "1. 4,512") && on.iter().any(|b| b.text == "2. 5,118"),
        "a demoted item lost its number from the text: {:?}",
        shape(&on)
    );
}

/// A bibliography set double-spaced: each reference's second line two ems
/// under its first, starting under the text and not under the marker.
/// `bullets` sets the marker in a run of its own, which gives the item a text
/// column; `[1]` is drawn in the same run as the text, which does not.
fn double_spaced_references(pitch: f64, bullets: bool) -> Vec<Piece> {
    let references = [
        (
            "[1]",
            "A. Mizel, M. W. Mitchell and M. L. Cohen. Energy barrier to",
            "decoherence. Phys. Rev. A 63, 040302 (2001).",
        ),
        (
            "[2]",
            "P. Aliferis and B. M. Terhal. Fault-tolerant quantum computation",
            "for local leakage faults. Quant. Inf. Comput. 7, 139 (2007).",
        ),
        (
            "[3]",
            "D. Gottesman and I. L. Chuang. Quantum teleportation is a",
            "universal computational primitive. Nature 402, 390 (1999).",
        ),
    ];
    let mut pieces = Vec::new();
    let mut y = TOP;
    for (marker, first, second) in references {
        if bullets {
            pieces.extend(bullet_item(first, y, 0));
        } else {
            pieces.extend(numbered_item(marker, first, y, 0));
        }
        pieces.push(text(second, MARGIN + TEXT_INDENT, y - pitch));
        y -= 2.0 * pitch;
    }
    pieces.push(line(
        "The references above are cited in the order of the text.",
        y,
    ));
    pieces
}

#[test]
fn a_double_spaced_reference_keeps_its_second_line() {
    // DocLayNet validation, arXiv 1002.0846 and 1002.2672: references set at
    // twice their type size, so the second line of each is a "line of white"
    // below its first and came out a block of its own -- which the block model
    // then made an item of its own (#599). The line starts under the item's
    // text, where only a continuation starts.
    for bullets in [false, true] {
        let blocks = blocks_of(&double_spaced_references(2.0 * BODY_SIZE, bullets));
        let items: Vec<&str> = items(&blocks).iter().map(|b| b.text.as_str()).collect();
        assert_eq!(
        items,
        vec![
            "A. Mizel, M. W. Mitchell and M. L. Cohen. Energy barrier to decoherence. Phys. Rev. A 63, 040302 (2001).",
            "P. Aliferis and B. M. Terhal. Fault-tolerant quantum computation for local leakage faults. Quant. Inf. Comput. 7, 139 (2007).",
            "D. Gottesman and I. L. Chuang. Quantum teleportation is a universal computational primitive. Nature 402, 390 (1999).",
        ],
        "bullets: {bullets}, {:?}",
        shape(&blocks)
    );
        // The prose after the list starts at the marker's edge: not the item's.
        assert!(
            matches!(
                block_starting(&blocks, "The references above").kind,
                BlockKind::Paragraph
            ),
            "bullets: {bullets}, {:?}",
            shape(&blocks)
        );
    }
}

/// Numbered references with no hang indent: each entry's second line starts
/// back at the marker's own edge, at normal leading.
fn flush_references() -> Vec<Piece> {
    let references = [
        (
            "9.",
            "Summermeyer, D et al: Chimeric antigen receptor-modified T cells derived from defined CD44",
            "and CD8+ subsets confer superior antitumor reactivity in vivo; Leukemia, Vol 30 (2016).",
        ),
        (
            "10.",
            "Willinger, T et al: Molecular signatures distinguishing human central memory from effector",
            "memory CD8 T cell subsets; J. Immunology, Vol 175 Page 5895 (2005).",
        ),
        (
            "11.",
            "Friedberg, JW et al: Ex vivo B cell depletion using ELIGIX B cell SC system and autologous",
            "peripheral blood stem cell transplantation in follicular lymphoma patients here.",
        ),
    ];
    let mut pieces = Vec::new();
    let mut y = TOP;
    for (marker, first, second) in references {
        // The marker carries its own trailing space: a two-figure number ends
        // where the text begins, and with no space after it `10.Willinger`
        // opens no item.
        pieces.extend(numbered_item(&format!("{marker} "), first, y, 0));
        pieces.push(text(second, MARGIN, y - LEADING));
        y -= 2.0 * LEADING;
    }
    pieces
}

#[test]
fn a_numbered_reference_keeps_its_flush_second_line() {
    // A patent bibliography (WO 2020/210023): entries of two lines each, the
    // first running the full measure, the second starting back at the
    // marker's edge with no hang indent. The paragraph merge cuts the
    // continuation out of its item -- a line at the marker's edge is under
    // nothing -- and only the learned merge puts it back (#574, T5).
    let blocks = blocks_of(&flush_references());
    let found: Vec<&str> = items(&blocks).iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        found,
        vec![
            "Summermeyer, D et al: Chimeric antigen receptor-modified T cells derived from defined CD44 and CD8+ subsets confer superior antitumor reactivity in vivo; Leukemia, Vol 30 (2016).",
            "Willinger, T et al: Molecular signatures distinguishing human central memory from effector memory CD8 T cell subsets; J. Immunology, Vol 175 Page 5895 (2005).",
            "Friedberg, JW et al: Ex vivo B cell depletion using ELIGIX B cell SC system and autologous peripheral blood stem cell transplantation in follicular lymphoma patients here.",
        ],
        "{:?}",
        shape(&blocks)
    );
}

#[test]
fn a_short_item_does_not_take_a_flush_paragraph() {
    // `2. Pears from the orchard` over a flush line that reads as its next
    // clause: the item stopped well short of the measure, so the flush line
    // is the paragraph after the list, not the item's second line (#574, T5).
    let mut pieces = Vec::new();
    for (k, body) in [
        "The committee considered the proposals put before it in the order",
        "they had been received, and resolved that each of them be referred",
        "to the officers, who are to report on them at the next meeting.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(line(body, TOP - k as f64 * LEADING));
    }
    pieces.push(line("1. Apples from the orchard", TOP - 4.0 * LEADING));
    pieces.push(line("2. Pears from the orchard", TOP - 5.0 * LEADING));
    pieces.push(line(
        "and quinces from the walled garden were gathered in before the frost.",
        TOP - 6.0 * LEADING,
    ));
    let blocks = blocks_of(&pieces);
    assert_eq!(
        block_starting(&blocks, "Pears").text,
        "Pears from the orchard",
        "the flush paragraph after the list was taken into its last item: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks
            .iter()
            .any(|b| b.text
                == "and quinces from the walled garden were gathered in before the frost."),
        "the flush paragraph after the list is gone: {:?}",
        shape(&blocks)
    );
}

#[test]
fn white_wider_than_double_spacing_still_ends_an_item() {
    // The hang allows a double-spaced step and nothing wider: three ems of
    // white under an item is a break, whatever column the next line starts in.
    for bullets in [false, true] {
        let blocks = blocks_of(&double_spaced_references(3.0 * BODY_SIZE, bullets));
        assert!(
            blocks.iter().any(|b| b.text.starts_with("decoherence.")),
            "a line three ems down was taken into the item (bullets: {bullets}): {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn double_spaced_references_in_a_column_set_solid_keep_their_second_lines() {
    // Codex on #1967: where the rest of the column is set solid its measured
    // leading is one line, and the leading rail refused a double-spaced
    // continuation before the white rail was asked. The list's own pitch --
    // two lines from one reference to the next -- is what the step is held to.
    for bullets in [false, true] {
        let mut pieces = Vec::new();
        // Twelve lines set solid, so the column's measured leading is one line
        // and not the references' two.
        let body: Vec<String> = (0..12)
            .map(|k| {
                format!(
                    "Line {k} of the argument set solid, as the text of the section runs on and"
                )
            })
            .collect();
        for (k, text_line) in body.iter().enumerate() {
            pieces.push(line(text_line, TOP - k as f64 * LEADING));
        }
        // The list follows at one line's distance, in the same run.
        let top = TOP - body.len() as f64 * LEADING;
        let references = double_spaced_references(2.0 * BODY_SIZE, bullets);
        pieces.extend(references.into_iter().map(|mut piece| {
            piece.y -= TOP - top;
            piece
        }));
        let blocks = blocks_of(&pieces);
        assert!(
            blocks.iter().any(|b| b
                .text
                .ends_with("Energy barrier to decoherence. Phys. Rev. A 63, 040302 (2001).")),
            "bullets: {bullets}, {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn a_paragraph_at_the_lists_indent_after_a_list_set_solid_is_its_own() {
    // The other side of the list's pitch: items one line apart, then a
    // paragraph's white, then a paragraph set at the list's indent. The last
    // item runs to the measure, so only the pitch says the paragraph is not
    // its continuation.
    for bullets in [false, true] {
        let mut pieces = Vec::new();
        let items = [
            "the staff of the contractor and the staff of its subcontractors,",
            "any natural person with the power to represent it or to decide,",
            "third parties involved in the performance of this contract, and",
        ];
        for (k, body) in items.iter().enumerate() {
            let y = TOP - k as f64 * LEADING;
            if bullets {
                pieces.extend(bullet_item(body, y, 0));
            } else {
                pieces.extend(numbered_item(&format!("{}.", k + 1), body, y, 0));
            }
        }
        let below = TOP - 2.0 * LEADING - 1.8 * BODY_SIZE;
        for (k, text_line) in [
            "The contractor must also ensure that those persons are not put in",
            "a situation which could give rise to a conflict of their interests.",
        ]
        .iter()
        .enumerate()
        {
            pieces.push(text(
                text_line,
                MARGIN + TEXT_INDENT,
                below - k as f64 * LEADING,
            ));
        }
        let blocks = blocks_of(&pieces);
        assert_eq!(
            block_starting(&blocks, "third parties").text,
            "third parties involved in the performance of this contract, and",
            "bullets: {bullets}, {:?}",
            shape(&blocks)
        );
    }
}

#[test]
fn a_wrapped_clause_keeps_every_line_of_its_hang() {
    // DocLayNet validation, a Japanese ordinance: each numbered paragraph
    // opens at the margin and runs on a step in, and its clauses `(i)`, `(ii)`
    // open at that step and run on a step further. The clause `(i)
    // Implementation of training ... medical` stops short of the measure
    // because its next word would not fit; the width rail read that as
    // finished, and every further line at the hang went into a paragraph of
    // its own (#599).
    const STEP: f64 = 11.0;
    let mut pieces = Vec::new();
    let mut y = TOP;
    for paragraph in [
        [
            "(1) The support organizations for medical accident investigation",
            "provided in the article of the act may jointly organize a council",
            "in order to promote the measures necessary for providing support.",
        ],
        [
            "(2) In order to achieve the purpose provided in the preceding one,",
            "the council shall share information on the status of report and",
            "investigation and exchange the opinions that are necessary to it.",
        ],
        [
            "(3) Based on the results of the sharing of information provided in",
            "the preceding paragraph, the council shall perform the following",
            "matters in accordance with the guidelines issued by the ministry.",
        ],
    ] {
        for (k, body) in paragraph.iter().enumerate() {
            let x = if k == 0 { MARGIN } else { MARGIN + STEP };
            pieces.push(text(body, x, y));
            y -= LEADING;
        }
    }
    let clause = [
        "(i) Implementation of training so that report and medical",
        "accidentinvestigation by the administrator of a hospital, etc.",
        "provided in the article of the act, and support by support",
        "organizations can be smoothly implemented.",
    ];
    for (k, body) in clause.iter().enumerate() {
        let x = if k == 0 {
            MARGIN + STEP
        } else {
            MARGIN + 2.0 * STEP
        };
        pieces.push(text(body, x, y));
        y -= LEADING;
    }
    pieces.push(text(
        "(ii) Referral of support organizations to the administrator.",
        MARGIN + STEP,
        y,
    ));
    let blocks = blocks_of(&pieces);
    let first = block_starting(&blocks, "Implementation");
    assert!(
        first.text.ends_with("smoothly implemented."),
        "the clause was cut at its hang: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_short_item_is_not_wrapped_into_the_paragraph_after_it() {
    // Codex on #1981: the wrap signature measured the room an item left
    // against the item and the next line alone, so a short paragraph after
    // `2. Pears` set that measure itself and its long first word passed for
    // the word the item could not fit. The measure is the column's.
    let mut pieces = Vec::new();
    for (k, body) in [
        "The committee considered the proposals put before it in the order",
        "they had been received, and resolved that each of them be referred",
        "to the officers, who are to report on them at the next meeting.",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(line(body, TOP - k as f64 * LEADING));
    }
    pieces.push(line("1. Apples", TOP - 4.0 * LEADING));
    pieces.push(line("2. Pears", TOP - 5.0 * LEADING));
    pieces.push(text(
        "Notwithstanding.",
        MARGIN + TEXT_INDENT,
        TOP - 6.0 * LEADING,
    ));
    let blocks = blocks_of(&pieces);
    assert!(
        blocks.iter().any(|b| b.text == "Notwithstanding."),
        "the paragraph after the list was taken into its last item: {:?}",
        shape(&blocks)
    );
}

/// The heuristics' own verdict, before the learned pass reads it.
fn blocks_before_learning(pieces: &[Piece]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces, &[]))
        .expect("open pdf")
        .extract_blocks_before_learning()
        .expect("extract blocks")
}

/// Clauses of the general conditions of an EU supply contract (DocLayNet
/// validation, #599), each numbered by its place in the outline.
const CLAUSES: [&str; 3] = [
    "II.4.1 The contractor must comply with the minimum requirements provided",
    "II.4.2 All periods specified in the contract are calculated in calendar",
    "II.6.1 The contracting authority is not liable for any damage or loss",
];

#[test]
fn a_contract_numbered_in_outline_is_a_list_of_its_clauses() {
    // The heuristics confirm the run; whether it stays a list is the learned
    // pass's call, which on these three lines alone -- no page around them --
    // it does not make. The number stays in the item's text: no one ordinal
    // says `II.4.1`, and a cross-reference names the clause by it.
    let pieces: Vec<Piece> = CLAUSES
        .iter()
        .enumerate()
        .map(|(n, clause)| line(clause, TOP - n as f64 * 3.0 * LEADING))
        .collect();

    let blocks = blocks_before_learning(&pieces);
    let found = items(&blocks);
    assert_eq!(found.len(), 3, "the clauses: {:?}", shape(&blocks));
    for (item, clause) in found.iter().zip(CLAUSES) {
        assert_eq!(item.text, clause, "the clause lost its number");
        assert!(
            matches!(item.kind, BlockKind::ListItem { ordinal: None, .. }),
            "an outline number was written as one ordinal: {:?}",
            item.kind
        );
    }
}

#[test]
fn a_contents_page_numbered_in_outline_is_not_a_list() {
    // The same contract opens with its contents: each clause's number and
    // title, a dot leader, and the page (#599).
    let pieces: Vec<Piece> = [
        "I.4.1. Price of the contract and maximum amount ............ 6",
        "I.4.2. Price revision index ................................ 7",
        "I.5.1. Pre-financing ....................................... 7",
    ]
    .iter()
    .enumerate()
    .map(|(n, entry)| line(entry, TOP - n as f64 * 2.0 * LEADING))
    .collect();

    let blocks = blocks_before_learning(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a contents page was read as a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn numbered_sub_heads_in_body_type_are_not_clauses() {
    // `2.1 Materials` over its paragraph, and `2.2 Methods` over the next: an
    // outline's numbers, set in the body's type, on short lines that stop
    // where the title stops. Sub-heads, not a list (#599).
    let pieces = vec![
        line("2.1 Materials", TOP),
        line(
            "Samples were drawn from the three sites over the season and stored",
            TOP - LEADING,
        ),
        line(
            "at four degrees until they were read in the laboratory the next day.",
            TOP - 2.0 * LEADING,
        ),
        line("2.2 Methods", TOP - 4.0 * LEADING),
        line(
            "Each sample was read twice by two readers who did not see the other",
            TOP - 5.0 * LEADING,
        ),
        line(
            "reading, and the two were reconciled by a third where they differed.",
            TOP - 6.0 * LEADING,
        ),
    ];

    let blocks = blocks_before_learning(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a numbered sub-head was read as a clause: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_numbered_sub_head_is_not_counted_into_the_clauses_beside_it() {
    // `2.1 Materials` is a sub-head over its paragraph; `2.2 Each sample ...`
    // runs on over the measure and the line under it. One of the two is not
    // a label, which is enough to confirm a run -- and the sub-head would be
    // counted into it as its first item (#599).
    let pieces = vec![
        line("2.1 Materials", TOP),
        line(
            "Samples were drawn from the three sites over the season and stored",
            TOP - LEADING,
        ),
        line(
            "2.2 Each sample was read twice by two readers who did not see the",
            TOP - 3.0 * LEADING,
        ),
        line(
            "other reading, and the two were reconciled by a third reader.",
            TOP - 4.0 * LEADING,
        ),
    ];

    let blocks = blocks_before_learning(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "a numbered sub-head was counted into a run: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_long_numbered_sub_head_is_not_a_clause() {
    // The EU contract's `II.4.11 Conformity of the supplies delivered with the
    // contract`: most of the measure, but a title -- no full stop, and the
    // paragraph under it starts afresh rather than wrapping out of it (#599).
    let pieces = vec![
        line(
            "II.4.10 The contractor must deliver the supplies within the period set.",
            TOP,
        ),
        line(
            "II.4.11 Conformity of the supplies delivered with the contract",
            TOP - 3.0 * LEADING,
        ),
        line(
            "The supplies delivered by the contractor to the contracting authority",
            TOP - 5.0 * LEADING,
        ),
        line(
            "must be in conformity in quantity, quality, price and packaging with",
            TOP - 6.0 * LEADING,
        ),
    ];

    let blocks = blocks_before_learning(&pieces);
    let head = block_starting(&blocks, "II.4.11");
    assert!(
        !matches!(head.kind, BlockKind::ListItem { .. }),
        "a long numbered sub-head was read as a clause: {:?}",
        shape(&blocks)
    );
}

#[test]
fn figures_a_sentence_wrapped_onto_are_not_clauses() {
    // `3.5 Million` and `3.7 Percent` climb under one head the way `II.4.1`
    // and `II.4.2` do, and open two lines at one edge -- each of them the
    // continuation of a sentence the line above ran out of room for. The
    // paragraph's own continuation rules keep them in it; this holds that
    // they still do now that an outline number is a marker (codex, #1986).
    let pieces = vec![
        line(
            "During the year under review the fleet landed a catch that came to some",
            TOP,
        ),
        line(
            "3.5 Million tonnes, a little under the year before it, and the revenue",
            TOP - LEADING,
        ),
        line(
            "that catch brought in rose again, to the point where it now accounts for",
            TOP - 2.0 * LEADING,
        ),
        line(
            "3.7 Percent of the revenue of the whole group over the same twelve months.",
            TOP - 3.0 * LEADING,
        ),
    ];

    let blocks = blocks_before_learning(&pieces);
    assert!(
        items(&blocks).is_empty(),
        "figures in running text were read as clauses: {:?}",
        shape(&blocks)
    );
}

/// A run of text in WinAnsi, the encoding the fixture's font declares: `§`,
/// `ü` and `•` are one byte each there and two or three in UTF-8.
fn latin(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        bytes: body
            .chars()
            .map(|c| {
                if c == '\u{2022}' {
                    0x95
                } else {
                    c as u32 as u8
                }
            })
            .collect(),
        ..text("", x, y)
    }
}

/// Two runs per line: a marker at one x and what it marks at another.
fn two_column_rows(rows: &[(&str, &str)], marker_x: f64, text_x: f64) -> Vec<Piece> {
    rows.iter()
        .enumerate()
        .flat_map(|(n, (marker, body))| {
            let y = TOP - n as f64 * 1.6 * LEADING;
            [latin(marker, marker_x, y), latin(body, text_x, y)]
        })
        .collect()
}

fn table_cells(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .count()
}

#[test]
fn a_contents_page_set_in_two_columns_is_a_list_not_a_table() {
    // FamFG's contents (DocLayNet validation, #1973): each section's sign and
    // number at one x, its title at another -- a grid by every measure the
    // table builder has, and a list to the annotation and to a reader.
    let rows = [
        ("\u{a7} 70", "Statthaftigkeit der Rechtsbeschwerde"),
        ("\u{a7} 71", "Frist und Form der Rechtsbeschwerde"),
        ("\u{a7} 72", "Gr\u{fc}nde der Rechtsbeschwerde"),
        ("\u{a7} 73", "Anschlussrechtsbeschwerde"),
        ("\u{a7} 74", "Entscheidung \u{fc}ber die Rechtsbeschwerde"),
        ("\u{a7} 75", "Sprungrechtsbeschwerde"),
    ];
    let blocks = blocks_before_learning(&two_column_rows(&rows, MARGIN, MARGIN + 60.0));
    assert_eq!(
        table_cells(&blocks),
        0,
        "read as a table: {:?}",
        shape(&blocks)
    );
    let found = items(&blocks);
    assert_eq!(found.len(), rows.len(), "the entries: {:?}", shape(&blocks));
    assert!(
        found[0].text.starts_with("\u{a7} 70") && found[0].text.contains("Statthaftigkeit"),
        "an entry lost its number or its title: {:?}",
        found[0].text
    );
}

#[test]
fn a_table_whose_first_column_is_bare_numbers_stays_a_table() {
    // Row labels, ids and line numbers down a first column are a table's, and
    // the rule that reads a list set as a table must not take them (#1973).
    let rows = [
        ("1", "Fluren und Treppenh\u{e4}user in Geschossh\u{e4}usern"),
        ("2", "Trenndecken unter Gemeinschaftsr\u{e4}umen"),
        ("3", "T\u{fc}ren in W\u{e4}nden nach Zeile 1"),
        ("4", "T\u{fc}ren in W\u{e4}nden nach Zeile 2"),
        ("5", "Decken unter Hausfluren und Laubeng\u{e4}ngen"),
    ];
    let before = blocks_before_learning(&two_column_rows(&rows, MARGIN, MARGIN + 60.0));
    assert!(
        table_cells(&before) > 0,
        "a table's row labels were refused as a list: {:?}",
        shape(&before)
    );
}

#[test]
fn a_bank_of_bullets_down_a_three_column_table_stays_a_table() {
    // `• | Generator circuit breaker CLOSED | 1 NO`: the third column is the
    // table's data, and a list set as a table has two (#1973).
    let labels = [
        "Generator circuit breaker CLOSED",
        "Generator circuit breaker OPEN",
        "Mains circuit breaker CLOSED",
        "Mains circuit breaker OPEN",
    ];
    let mut pieces = Vec::new();
    for (n, label) in labels.iter().enumerate() {
        let y = TOP - n as f64 * 1.6 * LEADING;
        pieces.push(latin("\u{2022}", MARGIN, y));
        pieces.push(latin(label, MARGIN + 40.0, y));
        pieces.push(latin("1 NO", MARGIN + 300.0, y));
    }
    let blocks = blocks_before_learning(&pieces);
    assert!(table_cells(&blocks) > 0, "no table: {:?}", shape(&blocks));
}

#[test]
fn a_contents_page_keeps_its_short_last_entry() {
    // A section list whose last title is short and has a paragraph under it:
    // an outline's sub-head has that shape, and the rule that leaves those out
    // of a run must not be asked of a statute's sections (codex, #2000).
    let rows = [
        ("\u{a7} 70", "Statthaftigkeit der Rechtsbeschwerde"),
        ("\u{a7} 71", "Frist und Form der Rechtsbeschwerde"),
        ("\u{a7} 72", "Kosten"),
    ];
    let mut pieces = two_column_rows(&rows, MARGIN, MARGIN + 60.0);
    pieces.push(latin(
        "Die Vorschriften dieses Abschnitts gelten auch im weiteren Verfahren.",
        MARGIN,
        TOP - 3.0 * 1.6 * LEADING,
    ));
    let blocks = blocks_before_learning(&pieces);
    let found = items(&blocks);
    assert_eq!(found.len(), rows.len(), "the entries: {:?}", shape(&blocks));
    assert!(
        found[2].text.starts_with("\u{a7} 72"),
        "{:?}",
        found[2].text
    );
}

// ---------------------------------------------------------------------------
// Fast-tier lists, first pass: the vetoed real lists. A marker column the cut
// parts from its text comes back as lines of lone bullets ordered away from
// their own baseline, and a bullet was asked the weight and the foot of a
// numbered heading. Each of these goes red without its rule: the items come
// back as paragraphs, the lone bullets as paragraphs of their own.
// ---------------------------------------------------------------------------

/// A two-column page whose right column is a list: prose down the left, and
/// down the right a bullet column standing a hanging indent from its text.
/// The cut reads the marker column as its margin and sweeps it aside into
/// lines of its own, ordered away from the text they open.
fn marker_column_list() -> Vec<Piece> {
    const LEFT: [&str; 11] = [
        "The committee considered the proposals put",
        "before it in the order they had arrived and",
        "resolved that each of them be referred to",
        "the officers for a report before the next",
        "ordinary meeting of the society to be held",
        "in the spring of the coming year at noon",
        "in the great hall of the assembly rooms",
        "with the treasurer laying the accounts and",
        "the secretary reading the minutes aloud",
        "before the members vote on the motion",
        "to adjourn until the autumn session",
    ];
    const RIGHT: [&str; 11] = [
        "check your current holding balances",
        "choose your annual report options",
        "update your address details",
        "update your bank details",
        "confirm your exemption status",
        "register with the share registry",
        "check transaction and dividend history",
        "enter your email address",
        "check the share prices and graphs",
        "download the instruction forms",
        "subscribe to email announcements",
    ];
    const LIST_X: f64 = 330.0;
    let mut pieces = Vec::new();
    for (n, (left, right)) in LEFT.iter().zip(RIGHT.iter()).enumerate() {
        let y = TOP - n as f64 * 1.5 * LEADING;
        pieces.push(text(left, MARGIN, y));
        pieces.push(Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: LIST_X,
            y,
            size: BODY_SIZE,
        });
        pieces.push(text(right, LIST_X + 12.0, y));
    }
    pieces
}

#[test]
fn lone_bullets_rejoin_the_line_they_open() {
    // The cut sweeps the marker column aside as its margin: without the
    // fusion the bullets come back as lines of their own -- no text after
    // the marker, so no marker at all -- and the list is paragraphs. With it
    // each bullet is back on its own baseline and the run confirms.
    let blocks = blocks_before_learning(&marker_column_list());
    assert_eq!(
        items(&blocks).len(),
        11,
        "the marker column did not rejoin its text: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks.iter().all(|b| b.text.trim() != "\u{2022}"),
        "a lone bullet survived as a block of its own: {:?}",
        shape(&blocks)
    );
}

#[test]
fn bold_bullets_are_still_a_list() {
    // The weight rail is for numbered headings, and a bullet is not one: a
    // bold bullet run in regular prose is a list set apart the way lists are.
    let mut pieces = prose(TOP);
    for (n, body) in [
        "Development of the gas fields well advanced",
        "Oil flowing from the offshore field",
        "Exploration continues in two countries",
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - 4.0 * LEADING - n as f64 * LEADING;
        pieces.push(Piece {
            bytes: vec![0x95],
            face: Face::Bold,
            x: MARGIN,
            y,
            size: BODY_SIZE,
        });
        pieces.push(Piece {
            bytes: body.as_bytes().to_vec(),
            face: Face::Bold,
            x: MARGIN + TEXT_INDENT,
            y,
            size: BODY_SIZE,
        });
    }

    let blocks = blocks_before_learning(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the bold bullets were refused as a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn a_small_bullet_list_under_the_body_is_still_a_list() {
    // The foot rail is for footnote marks, and a bullet is not one: a
    // small-type bullet list under the body is a list, not footnotes.
    let mut pieces = prose(TOP);
    for (n, body) in [
        "Checking accounts",
        "Money market accounts",
        "Certificates of deposit",
    ]
    .iter()
    .enumerate()
    {
        let y = TOP - 4.0 * LEADING - n as f64 * LEADING;
        pieces.push(Piece {
            bytes: vec![0x95],
            face: Face::Regular,
            x: MARGIN,
            y,
            size: 8.0,
        });
        pieces.push(Piece {
            bytes: body.as_bytes().to_vec(),
            face: Face::Regular,
            x: MARGIN + TEXT_INDENT,
            y,
            size: 8.0,
        });
    }

    let blocks = blocks_before_learning(&pieces);
    assert_eq!(
        items(&blocks).len(),
        3,
        "the small-type bullets were refused as a list: {:?}",
        shape(&blocks)
    );
}

#[test]
fn two_bullets_are_already_a_list() {
    // The shortest run there is: one line opening with a bullet is a
    // coincidence, two at one edge are a list.
    let mut pieces = bullet_item("apples and pears", TOP, 0);
    pieces.extend(bullet_item("quinces and medlars", TOP - LEADING, 0));

    let blocks = blocks_before_learning(&pieces);
    assert_eq!(
        items(&blocks).len(),
        2,
        "the two items are not two items: {:?}",
        shape(&blocks)
    );
}
