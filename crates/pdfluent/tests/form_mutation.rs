//! Integration tests for Epic 3 #1245 — form mutation surface.
//!
//! Covers the four `PdfFormMut` setters end-to-end:
//! 1. `set_text` on a text field
//! 2. `set_checkbox` on a checkbox field
//! 3. `set_radio` on a radio group
//! 4. `set_dropdown` on a combobox field
//! 5. save → reopen → value persisted via the round-trip
//! 6. error paths (unknown field, wrong field type)
//!
//! Fixtures are built in-memory via `lopdf` so the test file stays
//! self-contained and covers field-type permutations the hand-rolled
//! `tests/fixtures/form.pdf` does not include (radio + dropdown).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::prelude::*;

fn dev_doc(bytes: &[u8]) -> PdfDocument {
    PdfDocument::from_bytes_with(bytes, OpenOptions::new()).expect("parse fixture")
}

// ---------------------------------------------------------------------------
// Fixture construction
// ---------------------------------------------------------------------------

/// Build a minimal PDF with one page and an AcroForm containing every
/// field type exercised by `PdfFormMut`. Returns the serialised bytes.
fn build_full_form_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    // An empty content stream keeps the page legal without forcing us to
    // ship a font or real drawing commands.
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));

    let pages_id = doc.new_object_id();

    let text_field = doc.add_object(dictionary! {
        "FT" => "Tx",
        "T" => Object::string_literal("first_name"),
        "V" => Object::string_literal(""),
    });

    let checkbox_field = doc.add_object(dictionary! {
        "FT" => "Btn",
        "T" => Object::string_literal("agree_terms"),
        "V" => Object::Name(b"Off".to_vec()),
    });

    // Radio group: `Ff` bit 16 (0x8000) is the Radio flag (§12.7.4.2.1).
    let radio_field = doc.add_object(dictionary! {
        "FT" => "Btn",
        "Ff" => 0x8000i64,
        "T" => Object::string_literal("preferred_color"),
        "V" => Object::Name(b"Off".to_vec()),
    });

    // Combo dropdown: `Ff` bit 18 (0x20000) is the Combo flag (§12.7.4.4.1).
    let dropdown_field = doc.add_object(dictionary! {
        "FT" => "Ch",
        "Ff" => 0x20000i64,
        "T" => Object::string_literal("country"),
        "V" => Object::string_literal(""),
        // NB: includes a non-ASCII option — the writeback chain validates
        // non-editable combo values against /Opt (ISO 32000 §12.7.4.4), so
        // the unicode round-trip test must use a declared option.
        "Opt" => vec![
            Object::string_literal("US"),
            Object::string_literal("NL"),
            Object::string_literal("DE"),
            lopdf::text_string("日本"),
        ],
    });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![
            text_field.into(),
            checkbox_field.into(),
            radio_field.into(),
            dropdown_field.into(),
        ],
    });

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });

    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf
}

/// Locate the value of a named field via the public `form_fields()` read
/// surface. Returns `None` if no field with that name exists.
fn field_value(doc: &PdfDocument, name: &str) -> Option<String> {
    doc.form_fields()
        .ok()?
        .into_iter()
        .find(|f| f.name == name)
        .map(|f| f.value)
    // NB: `/V` for button fields is serialised as `Object::Name`, which
    // `lopdf::decode_text_string` happens to also decode (names are bytes
    // without NUL). This lets us compare checkbox/radio values to their
    // expected state names via `field_value` without an extra helper.
}

// ---------------------------------------------------------------------------
// Positive paths
// ---------------------------------------------------------------------------

#[test]
fn set_text_updates_field_value() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_text("first_name", "Jane")
        .expect("set_text");

    assert_eq!(field_value(&doc, "first_name").as_deref(), Some("Jane"));
}

#[test]
fn set_checkbox_updates_field_value() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_checkbox("agree_terms", true)
        .expect("set_checkbox on");

    assert_eq!(field_value(&doc, "agree_terms").as_deref(), Some("Yes"));

    doc.form_mut()
        .set_checkbox("agree_terms", false)
        .expect("set_checkbox off");

    assert_eq!(field_value(&doc, "agree_terms").as_deref(), Some("Off"));
}

#[test]
fn set_radio_updates_field_value() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_radio("preferred_color", "Blue")
        .expect("set_radio");

    assert_eq!(
        field_value(&doc, "preferred_color").as_deref(),
        Some("Blue"),
    );
}

#[test]
fn set_dropdown_updates_field_value() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_dropdown("country", "NL")
        .expect("set_dropdown");

    assert_eq!(field_value(&doc, "country").as_deref(), Some("NL"));
}

// ---------------------------------------------------------------------------
// Chaining
// ---------------------------------------------------------------------------

#[test]
fn setters_chain_via_mut_self_return() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_text("first_name", "Jane")
        .and_then(|f| f.set_checkbox("agree_terms", true))
        .and_then(|f| f.set_radio("preferred_color", "Green"))
        .and_then(|f| f.set_dropdown("country", "DE"))
        .expect("chain");

    assert_eq!(field_value(&doc, "first_name").as_deref(), Some("Jane"));
    assert_eq!(field_value(&doc, "agree_terms").as_deref(), Some("Yes"));
    assert_eq!(
        field_value(&doc, "preferred_color").as_deref(),
        Some("Green"),
    );
    assert_eq!(field_value(&doc, "country").as_deref(), Some("DE"));
}

#[test]
fn setters_try_chain_via_question_mark_propagates_errors() {
    fn apply_with_question_mark(form: &mut pdfluent::PdfFormMut<'_>) -> pdfluent::Result<()> {
        form.set_text("first_name", "Jane")?
            .set_checkbox("agree_terms", true)?
            .set_radio("preferred_color", "Green")?
            .set_dropdown("does_not_exist", "DE")?;
        Ok(())
    }

    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);
    let original_country = field_value(&doc, "country");

    let err = {
        let mut form = doc.form_mut();
        apply_with_question_mark(&mut form).unwrap_err()
    };

    assert!(
        matches!(err, pdfluent::Error::Internal { .. }),
        "expected the `?` try-chain to return the setter error, got {err:?}",
    );
    assert_eq!(field_value(&doc, "first_name").as_deref(), Some("Jane"));
    assert_eq!(field_value(&doc, "agree_terms").as_deref(), Some("Yes"));
    assert_eq!(
        field_value(&doc, "preferred_color").as_deref(),
        Some("Green"),
    );
    assert_eq!(field_value(&doc, "country"), original_country);
}

// ---------------------------------------------------------------------------
// Round-trip: save → reopen → values preserved
// ---------------------------------------------------------------------------

#[test]
fn save_and_reopen_preserves_all_mutations() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_text("first_name", "Jane")
        .and_then(|f| f.set_checkbox("agree_terms", true))
        .and_then(|f| f.set_radio("preferred_color", "Blue"))
        .and_then(|f| f.set_dropdown("country", "NL"))
        .expect("chain");

    let serialised = doc.to_bytes().expect("to_bytes");
    let reopened = dev_doc(&serialised);

    assert_eq!(
        field_value(&reopened, "first_name").as_deref(),
        Some("Jane"),
    );
    assert_eq!(
        field_value(&reopened, "agree_terms").as_deref(),
        Some("Yes"),
    );
    assert_eq!(
        field_value(&reopened, "preferred_color").as_deref(),
        Some("Blue"),
    );
    assert_eq!(field_value(&reopened, "country").as_deref(), Some("NL"));
}

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

#[test]
fn set_on_unknown_field_errors() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    let err = doc
        .form_mut()
        .set_text("does_not_exist", "x")
        .expect_err("unknown field must error");

    // `field not found` currently surfaces as Error::Internal via the
    // centralised internal_error helper — verify via the public Display.
    let msg = format!("{err}");
    assert!(
        msg.contains("does_not_exist"),
        "error message should mention the missing field name, got: {msg}",
    );
    assert_eq!(err.code(), "E-INTERNAL");
}

#[test]
fn set_text_on_checkbox_errors() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    let err = doc
        .form_mut()
        .set_text("agree_terms", "x")
        .expect_err("wrong type must error");

    let msg = format!("{err}");
    assert!(
        msg.contains("agree_terms") && msg.contains("text"),
        "error should mention field name and requested operation, got: {msg}",
    );
}

#[test]
fn set_checkbox_on_text_errors() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    let err = doc
        .form_mut()
        .set_checkbox("first_name", true)
        .expect_err("wrong type must error");

    let msg = format!("{err}");
    assert!(msg.contains("first_name"));
    assert!(msg.contains("checkbox"));
}

#[test]
fn set_radio_on_dropdown_errors() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    let err = doc
        .form_mut()
        .set_radio("country", "US")
        .expect_err("wrong type must error");

    let msg = format!("{err}");
    assert!(msg.contains("country"));
    assert!(msg.contains("radio"));
}

// ---------------------------------------------------------------------------
// form_mut() on a document without a form
// ---------------------------------------------------------------------------

#[test]
fn form_mut_is_always_constructable() {
    // sample.pdf has no AcroForm; the handle must still be returnable
    // (per the documented contract on `form_mut`). Errors surface on
    // the individual setter calls.
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let _handle = doc.form_mut();
}

#[test]
fn setter_on_formless_document_errors_cleanly() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");

    let err = doc
        .form_mut()
        .set_text("anything", "x")
        .expect_err("no form ⇒ error");

    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("acroform") || msg.contains("not found"),
        "should explain the document has no form, got: {msg}",
    );
}

// ---------------------------------------------------------------------------
// Codex P1 regression tests
// ---------------------------------------------------------------------------

/// Non-ASCII text/dropdown values must round-trip through save + reopen
/// without corruption. PDFDocEncoding mangles non-ASCII; `lopdf::text_string`
/// switches to UTF-16BE automatically and `decode_text_string` recognises
/// the BOM on read-back.
#[test]
fn unicode_text_values_roundtrip() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_text("first_name", "Renée 北京")
        .and_then(|f| f.set_dropdown("country", "日本"))
        .expect("fill unicode");

    let serialised = doc.to_bytes().expect("to_bytes");
    let reopened = dev_doc(&serialised);

    assert_eq!(
        field_value(&reopened, "first_name").as_deref(),
        Some("Renée 北京"),
    );
    assert_eq!(field_value(&reopened, "country").as_deref(), Some("日本"));
}

/// Build a checkbox whose on-state is declared on a widget kid — the
/// common real-world shape. The resolver must walk kids; falling back
/// to `/Yes` would produce a `/V /Yes` that doesn't match any declared
/// appearance, leaving the box visually unchecked.
fn build_kid_widget_checkbox_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let pages_id = doc.new_object_id();

    // Kid widget with its own /AP/N carrying the on-state /On1.
    let kid_widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Null,
                "On1" => Object::Null,
            },
        },
    });

    // Parent checkbox field with no /AP of its own — only via /Kids.
    let checkbox_field = doc.add_object(dictionary! {
        "FT" => "Btn",
        "T" => Object::string_literal("subscribe"),
        "V" => Object::Name(b"Off".to_vec()),
        "Kids" => vec![kid_widget.into()],
    });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![kid_widget.into()],
    });

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![checkbox_field.into()],
    });

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });

    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf
}

#[test]
fn checkbox_on_state_resolves_from_kid_widget() {
    let bytes = build_kid_widget_checkbox_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_checkbox("subscribe", true)
        .expect("set_checkbox");

    assert_eq!(
        field_value(&doc, "subscribe").as_deref(),
        Some("On1"),
        "resolver should have picked up the kid's /AP/N on-state instead of defaulting to Yes",
    );
}

// ---------------------------------------------------------------------------
// Multi-select list box (set_multi_select → apply_choice_multi)
// ---------------------------------------------------------------------------

/// A PDF with one multi-select list box (`Ff` MultiSelect bit 22 = 0x200000).
fn build_multiselect_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let pages_id = doc.new_object_id();

    let listbox = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Ch",
        "Ff" => 0x200000i64, // MultiSelect
        "T" => Object::string_literal("languages"),
        "Rect" => vec![100.into(), 400.into(), 300.into(), 520.into()],
        "Opt" => vec![
            Object::string_literal("EN"),
            Object::string_literal("NL"),
            Object::string_literal("DE"),
            Object::string_literal("FR"),
        ],
    });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![listbox.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![listbox.into()],
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf
}

#[test]
fn set_multi_select_writes_array_and_index_cache_through_facade() {
    let bytes = build_multiselect_pdf();
    let mut doc = dev_doc(&bytes);

    doc.form_mut()
        .set_multi_select("languages", &["FR", "EN"])
        .expect("set_multi_select");

    // Re-serialise and inspect /V + /I structurally.
    let saved = doc.to_bytes().expect("to_bytes");
    let reloaded = Document::load_mem(&saved).expect("reload");

    // Find the field object.
    let af = match reloaded.catalog().unwrap().get(b"AcroForm").unwrap() {
        Object::Reference(id) => reloaded.get_object(*id).unwrap().as_dict().unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("acroform"),
    };
    let field_id = match &af.get(b"Fields").unwrap().as_array().unwrap()[0] {
        Object::Reference(id) => *id,
        _ => panic!("field ref"),
    };
    let fld = reloaded.get_object(field_id).unwrap().as_dict().unwrap();

    let v: Vec<String> = match fld.get(b"V").unwrap() {
        Object::Array(a) => a
            .iter()
            .filter_map(|o| lopdf::decode_text_string(o).ok())
            .collect(),
        _ => panic!("/V must be an array for multi-select"),
    };
    assert_eq!(v, vec!["FR".to_string(), "EN".to_string()]);

    let i: Vec<i64> = match fld.get(b"I").unwrap() {
        Object::Array(a) => a
            .iter()
            .filter_map(|o| match o {
                Object::Integer(n) => Some(*n),
                _ => None,
            })
            .collect(),
        _ => panic!("/I must be present"),
    };
    // EN=0, FR=3 → sorted [0, 3].
    assert_eq!(i, vec![0, 3]);
}

#[test]
fn set_multi_select_rejects_unknown_option() {
    let bytes = build_multiselect_pdf();
    let mut doc = dev_doc(&bytes);
    let err = doc
        .form_mut()
        .set_multi_select("languages", &["KL"])
        .expect_err("unknown option must error");
    // Mapped onto the facade error surface.
    assert!(!err.to_string().is_empty());
}

/// The inverse of what this asserted until #226: a document opened with no
/// licence of any kind fills its form.
///
/// It used to open on Trial -- which did not grant `AcroFormFill` -- and expect
/// `E-LICENSE-FEATURE-NOT-IN-TIER`. There is no tier and no such code now, so
/// the property worth pinning is that the plain `OpenOptions::new()` path, the
/// one every caller who has never heard of licensing takes, works.
#[test]
fn set_multi_select_needs_no_licence() {
    let bytes = build_multiselect_pdf();
    let mut doc = PdfDocument::from_bytes_with(&bytes, OpenOptions::new()).expect("parse");
    doc.form_mut()
        .set_multi_select("languages", &["EN"])
        .expect("an unlicensed caller must be able to fill a form");
}

// ---------------------------------------------------------------------------
// GA blocker (FASE B) — flatten_forms no longer panics
// ---------------------------------------------------------------------------

#[test]
fn flatten_forms_names_the_fields_it_could_not_flatten() {
    let bytes = build_full_form_pdf();
    let mut doc = dev_doc(&bytes);

    // Until 23-08-2026 this asserted that the call failed with
    // E-ENV-MISSING-DEPENDENCY and a note pointing at issue #1223. The runtime
    // was never deferred -- pdf-forms implements the whole pipeline -- so the
    // note went stale and this test kept the stale state pinned in place.
    //
    // These fields are bare field dictionaries with no /Rect: there is nowhere
    // on the page to draw them, so skipping is right. What matters is that the
    // report says so by name. "Flattened 0 of 4" and "flattened 0" are
    // different facts, and only one of them tells a caller that the document
    // they believe is final is still editable.
    let report = doc.flatten_forms().expect("flatten_forms");
    assert_eq!(report.fields_flattened, 0);
    assert!(
        !report.is_complete(),
        "skipped fields were reported as complete"
    );
    assert!(
        report.skipped.contains(&"first_name".to_string()),
        "the skipped field is not named: {:?}",
        report.skipped
    );
}
