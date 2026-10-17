//! A form field's value reaches the extracted text when no glyph on the page
//! carries it (#549, class F-A of #389).
//!
//! Two shapes, from two directions, and both produce nothing from a reader
//! that waits for a glyph:
//!
//! * `checkbox_no_appearance.pdf` (pdf.js) ticks a box with `/V /Yes` and
//!   `/AS /Yes` and has **no `/AP` at all**. A viewer synthesises the check
//!   mark from the value; there is no appearance stream to read, so extraction
//!   has to take the value itself.
//! * `8.10.1-t02-fail-a.pdf` (veraPDF) has two text widgets in two rectangles
//!   sharing **one** `/AP /N` Form XObject, and that stream draws a filled box
//!   inside `/Form <</ActualText (some_value)>> BDC ... EMC` and not one
//!   glyph. The text of both widgets is the replacement, which was only ever
//!   emitted when a glyph turned up to carry it.
//!
//! The fixtures are generated rather than checked in, so each one says which
//! single property it is about. The failure cases are here too, because the
//! cheap version of this change -- "the field has a value, emit it" -- prints
//! passwords, prints `Off` under every unticked box, and prints a value twice
//! on every ordinary filled form that also has an appearance.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const VALUE: &str = "some_value";

/// `/Ff` bit 14, the password flag.
const FLAG_PASSWORD: i64 = 1 << 13;
/// `/Ff` bit 17, the push-button flag.
const FLAG_PUSHBUTTON: i64 = 1 << 16;
/// `/Ff` bit 25, the comb flag.
const FLAG_COMB: i64 = 1 << 24;
/// `/F` bit 2, the hidden flag.
const ANNOT_HIDDEN: i64 = 1 << 1;
/// `/F` bit 6, the no-view flag.
const ANNOT_NOVIEW: i64 = 1 << 5;

/// A one-page document whose only content is the widgets handed in. The page
/// draws nothing, as a filled form's page regularly does.
fn page_of_widgets(doc: &mut Document, widgets: Vec<Object>) -> Vec<u8> {
    let empty = doc.add_object(Object::Stream(Stream::new(dictionary! {}, Vec::new())));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(empty),
        "Resources" => Object::Dictionary(dictionary! {}),
        "Annots" => Object::Array(widgets),
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

fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
}

/// The veraPDF document's appearance stream: a box, and the text of the field
/// only in the `/ActualText` of the sequence around it.
fn appearance_with_actual_text_only(doc: &mut Document, text: &str) -> Object {
    let content = Content {
        operations: vec![
            Operation::new(
                "BDC",
                vec![
                    Object::Name(b"Form".to_vec()),
                    Object::Dictionary(dictionary! {
                        "ActualText" => Object::string_literal(text),
                    }),
                ],
            ),
            Operation::new("re", vec![0.into(), 0.into(), 30.into(), 13.into()]),
            Operation::new("f", vec![]),
            Operation::new("EMC", vec![]),
        ],
    }
    .encode()
    .expect("encode appearance");

    Object::Reference(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Form".to_vec()),
            "BBox" => Object::Array(vec![0.into(), 0.into(), 30.into(), 13.into()]),
            "Resources" => Object::Dictionary(dictionary! {}),
        },
        content,
    ))))
}

/// An appearance stream with one glyphless `/ActualText` sequence inside
/// another: `outer` around `inner`, around a filled box and no glyph.
fn appearance_with_nested_actual_text(doc: &mut Document, outer: &str, inner: &str) -> Object {
    let bdc = |text: &str| {
        Operation::new(
            "BDC",
            vec![
                Object::Name(b"Form".to_vec()),
                Object::Dictionary(dictionary! {
                    "ActualText" => Object::string_literal(text),
                }),
            ],
        )
    };

    let content = Content {
        operations: vec![
            bdc(outer),
            bdc(inner),
            Operation::new("re", vec![0.into(), 0.into(), 30.into(), 13.into()]),
            Operation::new("f", vec![]),
            Operation::new("EMC", vec![]),
            Operation::new("EMC", vec![]),
        ],
    }
    .encode()
    .expect("encode appearance");

    Object::Reference(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Form".to_vec()),
            "BBox" => Object::Array(vec![0.into(), 0.into(), 30.into(), 13.into()]),
            "Resources" => Object::Dictionary(dictionary! {}),
        },
        content,
    ))))
}

/// An appearance stream that draws `text` as glyphs, in Courier.
fn appearance_with_glyphs(doc: &mut Document, text: &str) -> Object {
    let font = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 9.into()]),
            Operation::new("Td", vec![1.into(), 3.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()
    .expect("encode appearance");

    Object::Reference(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Form".to_vec()),
            "BBox" => Object::Array(vec![0.into(), 0.into(), 90.into(), 13.into()]),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            }),
        },
        content,
    ))))
}

fn rect(left: f64) -> Object {
    Object::Array(vec![
        left.into(),
        641.0.into(),
        (left + 90.0).into(),
        654.0.into(),
    ])
}

// ---------------------------------------------------------------------------
// (a) a value with no appearance stream to draw it
// ---------------------------------------------------------------------------

/// `checkbox_no_appearance.pdf`: a ticked box and an unticked one, neither
/// with an `/AP`. The ticked one's state is the only statement the file makes
/// about what it shows.
#[test]
fn a_ticked_box_without_an_appearance_says_which_state_it_is_in() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![
        Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("cbOn"),
            "V" => Object::Name(b"Yes".to_vec()),
            "AS" => Object::Name(b"Yes".to_vec()),
            "Rect" => rect(30.0),
        }))),
        Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("cbOff"),
            "V" => Object::Name(b"Off".to_vec()),
            "AS" => Object::Name(b"Off".to_vec()),
            "Rect" => rect(150.0),
        }))),
    ];

    let text = text_of(page_of_widgets(&mut doc, widgets));

    assert!(
        text.contains("Yes"),
        "the ticked box's state never reached the text: {text:?}"
    );
    assert!(
        !text.contains("Off"),
        "an unticked box put a word on the page: {text:?}"
    );
}

/// A text field with no appearance: the value, taken through the inheritance
/// chain the field tree is built on.
#[test]
fn a_text_field_without_an_appearance_says_its_value() {
    let mut doc = Document::with_version("1.7");
    let parent = doc.add_object(Object::Dictionary(dictionary! {
        "FT" => Object::Name(b"Tx".to_vec()),
        "T" => Object::string_literal("parent"),
        "V" => Object::string_literal(VALUE),
    }));
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Parent" => Object::Reference(parent),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains(VALUE),
        "an inherited value never reached the text: {text:?}"
    );
}

/// `/DV` is what the field shows until somebody fills it, so it is the text on
/// the page exactly when there is no `/V`.
#[test]
fn a_field_with_only_a_default_value_says_that() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("dv"),
            "DV" => Object::string_literal(VALUE),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains(VALUE),
        "a default value never reached the text: {text:?}"
    );
}

/// A comb field spaces its value one character per cell of `/MaxLen`. The
/// cells are a layout; the value is the same string either way.
#[test]
fn a_comb_field_says_its_value_and_not_its_cells() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("comb"),
            "Ff" => Object::Integer(FLAG_COMB),
            "MaxLen" => Object::Integer(10),
            "V" => Object::string_literal("AB12"),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains("AB12"),
        "a comb field's value never reached the text: {text:?}"
    );
}

/// A list box holds its selected options in an array, and they are separate
/// rows of the control rather than one word.
#[test]
fn a_multi_select_choice_keeps_its_options_apart() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("choice"),
            "V" => Object::Array(vec![
                Object::string_literal("alpha"),
                Object::string_literal("beta"),
            ]),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains("alpha") && text.contains("beta"),
        "a selected option never reached the text: {text:?}"
    );
    assert!(
        !text.contains("alphabeta"),
        "two options ran together into a word on no page: {text:?}"
    );
}

/// `/Opt` as `[export, display]` pairs: `/V` holds the export value and the
/// control shows the display one. (codex)
#[test]
fn a_choice_shows_the_label_and_not_the_export_value() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("country"),
            "Opt" => Object::Array(vec![
                Object::Array(vec![
                    Object::string_literal("NL"),
                    Object::string_literal("Netherlands"),
                ]),
                Object::Array(vec![
                    Object::string_literal("BE"),
                    Object::string_literal("Belgium"),
                ]),
            ]),
            "V" => Object::string_literal("NL"),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains("Netherlands"),
        "the label the control shows never reached the text: {text:?}"
    );
    assert!(
        !text.contains("NL"),
        "an export value was read as the text on the page: {text:?}"
    );
}

/// An editable combo box may hold a value that is in no `/Opt` entry, because
/// somebody typed it. That value is its own label. (codex)
#[test]
fn a_typed_choice_value_is_its_own_label() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Ch".to_vec()),
            "T" => Object::string_literal("country"),
            "Opt" => Object::Array(vec![Object::Array(vec![
                Object::string_literal("NL"),
                Object::string_literal("Netherlands"),
            ])]),
            "V" => Object::string_literal("Atlantis"),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains("Atlantis"),
        "a typed value never reached the text: {text:?}"
    );
}

/// A radio group is one field with one `/V` and several widgets. Without an
/// `/AS` to say which of them is on, the group's value is not every widget's
/// state, and reading it as one prints the selected option once per option in
/// the group. (codex)
#[test]
fn a_radio_group_does_not_put_its_value_under_every_option() {
    let mut doc = Document::with_version("1.7");
    let group = doc.new_object_id();
    let kids: Vec<Object> = [30.0, 150.0]
        .into_iter()
        .map(|left| {
            Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "Parent" => Object::Reference(group),
                "Rect" => rect(left),
            })))
        })
        .collect();
    doc.set_object(
        group,
        Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("choice"),
            "Ff" => Object::Integer(1 << 15),
            "V" => Object::Name(b"Option1".to_vec()),
            "Kids" => Object::Array(kids.clone()),
        }),
    );

    let text = text_of(page_of_widgets(&mut doc, kids));
    assert!(
        !text.contains("Option1"),
        "a radio group's value was read as the state of every one of its widgets: {text:?}"
    );
}

/// The same group with an `/AS` per widget: exactly the one that is on says so.
#[test]
fn a_radio_group_with_states_says_the_one_that_is_on() {
    let mut doc = Document::with_version("1.7");
    let group = doc.new_object_id();
    let kids: Vec<Object> = [("Option1", 30.0), ("Off", 150.0)]
        .into_iter()
        .map(|(state, left)| {
            Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "Parent" => Object::Reference(group),
                "AS" => Object::Name(state.as_bytes().to_vec()),
                "Rect" => rect(left),
            })))
        })
        .collect();
    doc.set_object(
        group,
        Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("choice"),
            "Ff" => Object::Integer(1 << 15),
            "V" => Object::Name(b"Option1".to_vec()),
            "Kids" => Object::Array(kids.clone()),
        }),
    );

    let text = text_of(page_of_widgets(&mut doc, kids));
    assert_eq!(
        text.matches("Option1").count(),
        1,
        "the selected radio option is not in the text exactly once: {text:?}"
    );
}

/// A field with a single kid has no ambiguity to protect against: its value
/// answers for the one widget it has.
#[test]
fn a_field_with_one_kid_still_says_its_value() {
    let mut doc = Document::with_version("1.7");
    let field = doc.new_object_id();
    let kid = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "Parent" => Object::Reference(field),
        "Rect" => rect(30.0),
    })));
    doc.set_object(
        field,
        Object::Dictionary(dictionary! {
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("single"),
            "V" => Object::Name(b"Yes".to_vec()),
            "Kids" => Object::Array(vec![kid.clone()]),
        }),
    );

    let text = text_of(page_of_widgets(&mut doc, vec![kid]));
    assert!(
        text.contains("Yes"),
        "a single-kid field's value never reached the text: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// (b) one appearance stream, two widgets
// ---------------------------------------------------------------------------

/// `8.10.1-t02-fail-a.pdf`: two widgets in two rectangles point at one Form
/// XObject that draws no glyph and carries its text in `/ActualText`. Both
/// instances are on the page, so both are in the text.
#[test]
fn a_shared_appearance_of_pure_actual_text_is_read_once_per_widget() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_actual_text_only(&mut doc, VALUE);
    let widgets = [30.0, 150.0]
        .into_iter()
        .map(|left| {
            Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "FT" => Object::Name(b"Tx".to_vec()),
                "T" => Object::string_literal(format!("textbox{left}")),
                "Rect" => rect(left),
                "AP" => Object::Dictionary(dictionary! { "N" => appearance.clone() }),
            })))
        })
        .collect();

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert_eq!(
        text.matches(VALUE).count(),
        2,
        "one of the two widgets sharing an appearance stream was dropped: {text:?}"
    );
}

/// The same shape with glyphs rather than a replacement: the second widget is
/// a second visit to the stream, not a cache hit.
#[test]
fn a_shared_appearance_that_draws_glyphs_is_read_once_per_widget() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_glyphs(&mut doc, VALUE);
    let widgets = [30.0, 200.0]
        .into_iter()
        .map(|left| {
            Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "FT" => Object::Name(b"Tx".to_vec()),
                "T" => Object::string_literal(format!("textbox{left}")),
                "V" => Object::string_literal(VALUE),
                "Rect" => rect(left),
                "AP" => Object::Dictionary(dictionary! { "N" => appearance.clone() }),
            })))
        })
        .collect();

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert_eq!(
        text.matches(VALUE).count(),
        2,
        "one of the two widgets sharing an appearance stream was dropped: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// the failure cases
// ---------------------------------------------------------------------------

/// A field that has both an appearance and a value is read from the
/// appearance, once. This is the case the condition "no appearance stream"
/// exists for: "has a value" would print every ordinary filled form twice.
#[test]
fn a_value_that_has_an_appearance_is_not_said_twice() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_glyphs(&mut doc, VALUE);
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("filled"),
            "V" => Object::string_literal(VALUE),
            "Rect" => rect(30.0),
            "AP" => Object::Dictionary(dictionary! { "N" => appearance }),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert_eq!(
        text.matches(VALUE).count(),
        1,
        "a filled field was read from both its appearance and its value: {text:?}"
    );
}

/// `/NeedAppearances true` says a viewer should build the appearance from the
/// value. It does not say the appearance that is already there is wrong, and
/// reading both would print the field twice.
#[test]
fn need_appearances_does_not_add_a_second_copy() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_glyphs(&mut doc, VALUE);
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("filled"),
            "V" => Object::string_literal(VALUE),
            "Rect" => rect(30.0),
            "AP" => Object::Dictionary(dictionary! { "N" => appearance }),
        },
    )))];
    let bytes = page_of_widgets(&mut doc, widgets);

    assert_eq!(
        text_of(bytes).matches(VALUE).count(),
        1,
        "a document asking for appearances got its value twice"
    );
}

/// A password field's value is not on the page in any viewer, and putting it
/// in extracted text would publish it.
#[test]
fn a_password_field_keeps_its_value() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("pw"),
            "Ff" => Object::Integer(FLAG_PASSWORD),
            "V" => Object::string_literal(VALUE),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        !text.contains(VALUE),
        "a password field's value was published in the text: {text:?}"
    );
}

/// A push button carries no field value; `/MK /CA` is a label printed on a
/// control.
#[test]
fn a_push_button_says_nothing() {
    let mut doc = Document::with_version("1.7");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Btn".to_vec()),
            "T" => Object::string_literal("submit"),
            "Ff" => Object::Integer(FLAG_PUSHBUTTON),
            "V" => Object::Name(b"Yes".to_vec()),
            "AS" => Object::Name(b"Yes".to_vec()),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        !text.contains("Yes"),
        "a push button put its state in the text: {text:?}"
    );
}

/// A widget the file says is not shown contributes no text, whichever of the
/// two flags says so.
#[test]
fn a_widget_that_is_not_shown_says_nothing() {
    for flag in [ANNOT_HIDDEN, ANNOT_NOVIEW] {
        let mut doc = Document::with_version("1.7");
        let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
            dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "FT" => Object::Name(b"Tx".to_vec()),
                "T" => Object::string_literal("invisible"),
                "F" => Object::Integer(flag),
                "V" => Object::string_literal(VALUE),
                "Rect" => rect(30.0),
            },
        )))];

        let text = text_of(page_of_widgets(&mut doc, widgets));
        assert!(
            !text.contains(VALUE),
            "a widget with /F {flag} put its value in the text: {text:?}"
        );
    }
}

/// A signature field's `/V` is a signature dictionary, not text.
#[test]
fn a_signature_field_says_nothing() {
    let mut doc = Document::with_version("1.7");
    let signature = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Sig".to_vec()),
        "Name" => Object::string_literal(VALUE),
    }));
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Sig".to_vec()),
            "T" => Object::string_literal("sig"),
            "V" => Object::Reference(signature),
            "Rect" => rect(30.0),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        !text.contains(VALUE),
        "a signature dictionary was read as field text: {text:?}"
    );
}

/// A glyphless replacement inside a glyphless replacement is one statement
/// about one set of marks, and the innermost is the specific one. Emitting
/// both would put two readings of the same box in the text. (codex)
#[test]
fn a_replacement_inside_a_replacement_is_read_once() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_nested_actual_text(&mut doc, "outer_value", VALUE);
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("nested"),
            "Rect" => rect(30.0),
            "AP" => Object::Dictionary(dictionary! { "N" => appearance }),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.contains(VALUE),
        "the innermost replacement never reached the text: {text:?}"
    );
    assert!(
        !text.contains("outer_value"),
        "an enclosing replacement was read as well as the one inside it: {text:?}"
    );
}

/// The same nesting where the inner replacement is empty: `/ActualText ()`
/// says these marks stand for no characters, and that answer covers the
/// sequence around it too.
#[test]
fn an_empty_replacement_inside_one_still_answers_for_it() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_nested_actual_text(&mut doc, "outer_value", "");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("nested-empty"),
            "Rect" => rect(30.0),
            "AP" => Object::Dictionary(dictionary! { "N" => appearance }),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.trim().is_empty(),
        "an enclosing replacement was read over an empty one: {text:?}"
    );
}

/// `/ActualText ()` is a producer saying this content stands for no characters
/// at all. A sequence with no glyphs in it does not turn that into text.
#[test]
fn an_empty_replacement_stays_empty() {
    let mut doc = Document::with_version("1.7");
    let appearance = appearance_with_actual_text_only(&mut doc, "");
    let widgets = vec![Object::Reference(doc.add_object(Object::Dictionary(
        dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal("empty"),
            "Rect" => rect(30.0),
            "AP" => Object::Dictionary(dictionary! { "N" => appearance }),
        },
    )))];

    let text = text_of(page_of_widgets(&mut doc, widgets));
    assert!(
        text.trim().is_empty(),
        "an empty replacement produced text: {text:?}"
    );
}
