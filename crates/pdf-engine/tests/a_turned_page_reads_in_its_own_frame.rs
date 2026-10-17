//! A page that is itself turned reads in the frame it was written in (#546,
//! class O-D of #389).
//!
//! A page carries a `/Rotate`, of its own or inherited from the `Pages` tree,
//! and every glyph on it reaches the text device turned with the page. Bands
//! are horizontal and they read one direction, so on such a page they lie
//! across its lines: each glyph lands in a band of its own and the page comes
//! out as letter salad -- `ro t a ti o n  9 0` where pdfbox `PDFBOX-6049` says
//! `rotation 90`, and `081 noitator` where its half-turned page says
//! `rotation 180`. #435 fixed exactly this for a run drawn AGAINST the
//! direction of its page, and could not fix it here, where the turned
//! direction is the page's own.
//!
//! The fixtures draw Courier, whose every glyph advances 600/1000 of the font
//! size, so a line's extent is what the arithmetic says it is.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const FONT_SIZE: f64 = 12.0;

/// Two lines of body text, one under the other, as any page carries them.
const FIRST_LINE: &str = "The first line of the body of this page";
const SECOND_LINE: &str = "and the second line carries on beneath it";

/// Where the `/Rotate` entry sits: on the page itself, or on the `Pages` node
/// it inherits from (§7.7.3.4 makes the entry inheritable, and a producer that
/// turns a whole document writes it once).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RotateOn {
    Page,
    Tree,
}

/// One run of text, and the angle its baseline is drawn at in PAGE space --
/// before the page's own `/Rotate` turns it.
struct Run {
    text: &'static str,
    degrees: f64,
    origin: (f64, f64),
}

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

/// A one-page PDF of `runs`, turned by `rotate` degrees.
fn turned_page(rotate: i64, on: RotateOn, runs: &[Run]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = courier(&mut doc);

    let mut operations = Vec::new();
    for run in runs {
        let (sin, cos) = run.degrees.to_radians().sin_cos();
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), FONT_SIZE.into()],
        ));
        operations.push(Operation::new(
            "Tm",
            vec![
                cos.into(),
                sin.into(),
                (-sin).into(),
                cos.into(),
                run.origin.0.into(),
                run.origin.1.into(),
            ],
        ));
        operations.push(Operation::new("Tj", vec![Object::string_literal(run.text)]));
        operations.push(Operation::new("ET", vec![]));
    }

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
        if on == RotateOn::Page {
            page.set("Rotate", Object::Integer(rotate));
        }
    }
    if on == RotateOn::Tree {
        if let Ok(Object::Dictionary(pages)) = doc.get_object_mut(pages_id) {
            pages.set("Rotate", Object::Integer(rotate));
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

fn text_of(rotate: i64, on: RotateOn, runs: &[Run]) -> String {
    PdfDocument::open(turned_page(rotate, on, runs))
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
}

/// Two lines of body text, drawn upright in page space.
fn body() -> Vec<Run> {
    vec![
        Run {
            text: FIRST_LINE,
            degrees: 0.0,
            origin: (72.0, 700.0),
        },
        Run {
            text: SECOND_LINE,
            degrees: 0.0,
            origin: (72.0, 684.0),
        },
    ]
}

/// Each line whole, and the second after the first. Whole is the half that
/// goes red without the turn: the letters come back either way, one band and
/// one block each.
fn assert_reads_as_body(text: &str, what: &str) {
    let first = text
        .find(FIRST_LINE)
        .unwrap_or_else(|| panic!("{what}: the first line is not whole: {text:?}"));
    let second = text
        .find(SECOND_LINE)
        .unwrap_or_else(|| panic!("{what}: the second line is not whole: {text:?}"));
    assert!(
        first < second,
        "{what}: the two lines came back the wrong way round: {text:?}"
    );
}

/// The ticket's own fixture, at each of the three turns a `/Rotate` can name.
#[test]
fn a_page_turned_by_its_rotate_reads_its_lines_whole_and_in_order() {
    for rotate in [90, 180, 270] {
        assert_reads_as_body(
            &text_of(rotate, RotateOn::Page, &body()),
            &format!("/Rotate {rotate}"),
        );
    }
}

/// `/Rotate` is inheritable, and a producer that turns a whole document writes
/// it on the `Pages` node rather than on each page.
#[test]
fn a_rotate_inherited_from_the_pages_tree_turns_the_page_just_the_same() {
    assert_reads_as_body(
        &text_of(90, RotateOn::Tree, &body()),
        "/Rotate 90 on the Pages node",
    );
}

/// A turned page reads what the same page reads upright: the turn is the whole
/// of the difference between them.
#[test]
fn a_turned_page_reads_what_the_upright_one_does() {
    let upright = text_of(0, RotateOn::Page, &body());
    for rotate in [90, 180, 270] {
        assert_eq!(
            text_of(rotate, RotateOn::Page, &body()),
            upright,
            "the page turned by {rotate} does not read as the upright one"
        );
    }
}

/// A turned page with a run drawn against ITS direction: the watermark of #435
/// on the page of #546. Turning the page is what makes the watermark the
/// minority direction it is, and both have to read.
#[test]
fn a_run_drawn_against_a_turned_page_still_reads_as_its_words() {
    let mut runs = body();
    // A quarter turn in page space, which is upright once the page's own
    // `/Rotate 90` has turned it -- the way a stamp applied after the turn
    // lands.
    runs.push(Run {
        text: "SANDBOX WATERMARK",
        degrees: 90.0,
        origin: (300.0, 200.0),
    });

    let text = text_of(90, RotateOn::Page, &runs);

    assert_reads_as_body(&text, "/Rotate 90 with a watermark");
    assert!(
        text.contains("SANDBOX WATERMARK"),
        "the watermark on a turned page was fragmented: {text:?}"
    );
}

/// An upright page is turned by nothing at all, and its own text is the
/// evidence: the same body, drawn on a page with no `/Rotate`, reads as it
/// always did.
#[test]
fn an_upright_page_is_left_alone() {
    assert_reads_as_body(&text_of(0, RotateOn::Page, &body()), "no /Rotate");
}
