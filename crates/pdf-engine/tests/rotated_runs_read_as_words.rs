//! A run of text drawn at an angle reads as its words (#435, class O-D of #389).
//!
//! Glyphs are merged into a run while each one carries on from the last, and
//! "carries on" used to mean "further right, on the same y". A run drawn at an
//! angle satisfies neither: every letter of a diagonal watermark lands on its
//! own y, so each one became a span of its own, each span landed in a different
//! horizontal band, and the bands interleaved the letters with whatever upright
//! text shared them. pdfjs `issue17069.pdf` came out as `rk Xa Orm test value
//! aBte D df wN t pA sS te` where the document says `test pdf watermark` and
//! `SANDBOX` -- letter salad, and unsearchable.
//!
//! The fixtures below draw Courier, whose every glyph advances 600/1000 of the
//! font size, so a run's extent is exactly what the arithmetic here says it is.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const FONT_SIZE: f64 = 12.0;

/// The upright line the page is written in. Longer than every rotated run on
/// the page put together, which is what makes upright the direction of the
/// page: what is turned on a page of upright text is the exception, and the
/// exception is what gets assembled along its own baseline.
const UPRIGHT_LINE: &str =
    "The body of the page is written upright and holds more letters than anything turned";

/// One run of text, and the angle its baseline is drawn at.
struct Run {
    text: &'static str,
    degrees: f64,
    /// Where the first glyph's baseline starts, in page coordinates.
    origin: (f64, f64),
}

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

/// A one-page PDF that draws each run with its own text matrix.
///
/// `Tm` is the rotation itself: `[cos, sin, -sin, cos, x, y]` scaled by the
/// font size, which is how every producer of a watermark or a rotated table
/// header writes one.
fn page_of_runs(runs: &[Run]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = courier(&mut doc);

    let mut operations = Vec::new();
    for run in runs {
        let radians = run.degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
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

fn text_of(runs: &[Run]) -> String {
    PdfDocument::open(page_of_runs(runs))
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
}

/// The ticket's own fixture: one upright line and one 45-degree watermark.
#[test]
fn a_forty_five_degree_watermark_reads_as_its_words() {
    let text = text_of(&[
        Run {
            text: UPRIGHT_LINE,
            degrees: 0.0,
            origin: (72.0, 700.0),
        },
        Run {
            text: "SANDBOX WATERMARK",
            degrees: 45.0,
            origin: (120.0, 200.0),
        },
    ]);

    assert!(
        text.contains("SANDBOX WATERMARK"),
        "the watermark was fragmented instead of read as its words: {text:?}"
    );
    assert!(
        text.contains(UPRIGHT_LINE),
        "the upright line did not survive the rotated run: {text:?}"
    );
}

/// The quarter turns, which is what a rotated table header or a page of
/// marginalia is drawn at, and 180 degrees, which is upright text upside down
/// and shares every y with the line above it.
#[test]
fn the_quarter_turns_and_the_half_turn_each_read_as_their_own_run() {
    for (degrees, origin) in [
        (90.0_f64, (500.0, 300.0)),
        (180.0_f64, (400.0, 400.0)),
        (270.0_f64, (100.0, 500.0)),
    ] {
        let text = text_of(&[
            Run {
                text: UPRIGHT_LINE,
                degrees: 0.0,
                origin: (72.0, 700.0),
            },
            Run {
                text: "TURNED RUN",
                degrees,
                origin,
            },
        ]);

        assert!(
            text.contains("TURNED RUN"),
            "a run at {degrees} degrees was fragmented: {text:?}"
        );
        assert!(
            text.contains(UPRIGHT_LINE),
            "the upright line did not survive a run at {degrees} degrees: {text:?}"
        );
    }
}

/// Two angles on one page, neither of them the page's, and their glyphs
/// interleave: the runs cross each other, so a rule that assembled by proximity
/// alone would splice one into the other.
#[test]
fn two_angles_that_cross_each_other_stay_two_runs() {
    let text = text_of(&[
        Run {
            text: UPRIGHT_LINE,
            degrees: 0.0,
            origin: (72.0, 720.0),
        },
        Run {
            text: "CLIMBING RUN",
            degrees: 40.0,
            origin: (150.0, 200.0),
        },
        Run {
            text: "FALLING RUN",
            degrees: -40.0,
            origin: (150.0, 500.0),
        },
    ]);

    assert!(
        text.contains("CLIMBING RUN"),
        "the run at 40 degrees was fragmented or spliced: {text:?}"
    );
    assert!(
        text.contains("FALLING RUN"),
        "the run at -40 degrees was fragmented or spliced: {text:?}"
    );
    assert!(
        text.contains(UPRIGHT_LINE),
        "the upright line did not survive two crossing runs: {text:?}"
    );
}

/// A free angle, which is what pdfjs issue17069 draws its `SANDBOX` at
/// (`0.6157 -0.7880`, a little over 52 degrees), and two lines of it: the
/// second line must not be spliced into the first, and both must read.
#[test]
fn a_free_angle_reads_line_by_line() {
    let degrees = -52.0_f64;
    let radians = degrees.to_radians();
    // One line below the other in the rotated frame: a step across the
    // baseline, which is the direction the run's left-hand normal points in.
    let leading = 16.0;
    let (sin, cos) = radians.sin_cos();
    let second = (150.0 + sin * leading, 500.0 - cos * leading);

    let text = text_of(&[
        Run {
            text: UPRIGHT_LINE,
            degrees: 0.0,
            origin: (72.0, 720.0),
        },
        Run {
            text: "FIRST DIAGONAL LINE",
            degrees,
            origin: (150.0, 500.0),
        },
        Run {
            text: "SECOND DIAGONAL LINE",
            degrees,
            origin: second,
        },
    ]);

    assert!(
        text.contains("FIRST DIAGONAL LINE"),
        "the first diagonal line was fragmented: {text:?}"
    );
    assert!(
        text.contains("SECOND DIAGONAL LINE"),
        "the second diagonal line was fragmented or spliced into the first: {text:?}"
    );
}
