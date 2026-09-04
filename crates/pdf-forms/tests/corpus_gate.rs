//! AcroForm corpus gate: fill → save → reopen → verify for every field-type
//! category in the support contract. Each category is a synthetic fixture
//! (see `tests/common/acroform_fixtures.rs`) exercised through the real
//! writeback chain and re-parsed from the saved bytes before asserting.
//!
//! This is the in-CI structural gate. The companion
//! `examples/gen_acroform_corpus.rs` emits the same fixtures to disk for
//! external (pikepdf) validation.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[path = "common/acroform_fixtures.rs"]
mod fx;

use lopdf::{Document, Object, ObjectId};
use pdfluent_forms::{
    apply_choice_multi, apply_field_value, build_form_model, parse_acroform,
    regenerate_appearances, FormFieldKind, WriteValue, WritebackError,
};

// ---------------------------------------------------------------------------
// Helpers: re-parse saved bytes and inspect a field by fully-qualified name.
// ---------------------------------------------------------------------------

fn reopen(bytes: &[u8]) -> Document {
    Document::load_mem(bytes).expect("reload saved bytes")
}

fn field_dict<'a>(doc: &'a Document, name: &str) -> &'a lopdf::Dictionary {
    fn walk<'a>(
        doc: &'a Document,
        id: ObjectId,
        prefix: &str,
        target: &str,
    ) -> Option<&'a lopdf::Dictionary> {
        let dict = doc.get_object(id).ok()?.as_dict().ok()?;
        let partial = dict
            .get(b"T")
            .ok()
            .and_then(|o| lopdf::decode_text_string(o).ok())
            .unwrap_or_default();
        let fqn = if prefix.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}.{partial}")
        };
        if fqn == target && dict.get(b"FT").is_ok() {
            return Some(dict);
        }
        if let Ok(Object::Array(kids)) = dict.get(b"Kids") {
            for kid in kids {
                if let Object::Reference(kid_id) = kid {
                    if let Some(found) = walk(doc, *kid_id, &fqn, target) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    let catalog = doc.catalog().expect("catalog");
    let af = match catalog.get(b"AcroForm").expect("acroform") {
        Object::Reference(id) => doc.get_object(*id).unwrap().as_dict().unwrap(),
        Object::Dictionary(d) => d,
        _ => panic!("bad acroform"),
    };
    let fields = match af.get(b"Fields").expect("fields") {
        Object::Array(a) => a.clone(),
        _ => panic!("bad fields"),
    };
    for f in &fields {
        if let Object::Reference(id) = f {
            if let Some(found) = walk(doc, *id, "", name) {
                return found;
            }
        }
    }
    panic!("field '{name}' not found in saved document");
}

fn v_string(d: &lopdf::Dictionary) -> String {
    match d.get(b"V") {
        Ok(Object::String(..)) => lopdf::decode_text_string(d.get(b"V").unwrap()).unwrap(),
        Ok(Object::Name(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    }
}

fn v_bytes(d: &lopdf::Dictionary) -> Vec<u8> {
    match d.get(b"V") {
        Ok(Object::String(b, _)) => b.clone(),
        Ok(Object::Name(b)) => b.clone(),
        _ => Vec::new(),
    }
}

fn has_ap(d: &lopdf::Dictionary) -> bool {
    matches!(
        d.get(b"AP"),
        Ok(Object::Dictionary(_)) | Ok(Object::Reference(_))
    )
}

fn save(doc: &mut Document) -> Vec<u8> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save");
    buf
}

// ---------------------------------------------------------------------------
// Per-category gate tests
// ---------------------------------------------------------------------------

#[test]
fn gate_pure_text() {
    let mut doc = reopen(&fx::pure_text());
    let out = apply_field_value(&mut doc, "full_name", WriteValue::Text("Jane Doe")).unwrap();
    assert_eq!(out.appearances_generated, 1);
    let saved = reopen(&save(&mut doc));
    let f = field_dict(&saved, "full_name");
    assert_eq!(v_string(f), "Jane Doe");
    assert!(has_ap(f), "text field must have a regenerated /AP");
}

#[test]
fn gate_multiline_and_comb() {
    let mut doc = reopen(&fx::multiline_comb());
    apply_field_value(
        &mut doc,
        "notes",
        WriteValue::Text("line one is quite long and should wrap across the field"),
    )
    .unwrap();
    apply_field_value(&mut doc, "postcode", WriteValue::Text("1234AB")).unwrap();
    let saved = reopen(&save(&mut doc));
    assert!(v_string(field_dict(&saved, "notes")).starts_with("line one"));
    assert_eq!(v_string(field_dict(&saved, "postcode")), "1234AB");
    assert!(has_ap(field_dict(&saved, "postcode")), "comb field /AP");
}

#[test]
fn gate_checkbox() {
    let mut doc = reopen(&fx::checkbox());
    let out = apply_field_value(&mut doc, "agree", WriteValue::Checkbox(true)).unwrap();
    assert!(out.appearance_states_set >= 1);
    let saved = reopen(&save(&mut doc));
    let f = field_dict(&saved, "agree");
    assert_eq!(v_bytes(f), b"Yes".to_vec());
    assert_eq!(
        f.get(b"AS").unwrap().as_name().unwrap(),
        b"Yes",
        "/AS must track /V"
    );
}

#[test]
fn gate_radio() {
    let mut doc = reopen(&fx::radio());
    apply_field_value(&mut doc, "gender", WriteValue::Radio("M")).unwrap();
    let saved = reopen(&save(&mut doc));
    let f = field_dict(&saved, "gender");
    assert_eq!(v_bytes(f), b"M".to_vec());
    // Exactly one kid is "M", the other "Off".
    let mut states = Vec::new();
    if let Ok(Object::Array(kids)) = f.get(b"Kids") {
        for k in kids {
            if let Object::Reference(id) = k {
                let kd = saved.get_object(*id).unwrap().as_dict().unwrap();
                states.push(kd.get(b"AS").unwrap().as_name().unwrap().to_vec());
            }
        }
    }
    assert!(states.contains(&b"M".to_vec()) && states.contains(&b"Off".to_vec()));
}

#[test]
fn gate_combo_clears_index() {
    let mut doc = reopen(&fx::combo());
    // Invalid option rejected.
    assert!(matches!(
        apply_field_value(&mut doc, "country", WriteValue::Choice("XX")),
        Err(WritebackError::InvalidOption { .. })
    ));
    apply_field_value(&mut doc, "country", WriteValue::Choice("NL")).unwrap();
    let saved = reopen(&save(&mut doc));
    assert_eq!(v_string(field_dict(&saved, "country")), "NL");
}

#[test]
fn gate_listbox_single() {
    let mut doc = reopen(&fx::listbox_single());
    apply_field_value(&mut doc, "city", WriteValue::Choice("Berlin")).unwrap();
    let saved = reopen(&save(&mut doc));
    assert_eq!(v_string(field_dict(&saved, "city")), "Berlin");
}

#[test]
fn gate_multiselect_v_array_and_index() {
    let mut doc = reopen(&fx::multiselect());
    apply_choice_multi(&mut doc, "languages", &["FR".to_string(), "EN".to_string()]).unwrap();
    let saved = reopen(&save(&mut doc));
    let f = field_dict(&saved, "languages");
    let v: Vec<String> = match f.get(b"V").unwrap() {
        Object::Array(a) => a
            .iter()
            .filter_map(|o| lopdf::decode_text_string(o).ok())
            .collect(),
        _ => panic!("/V array"),
    };
    assert_eq!(v, vec!["FR".to_string(), "EN".to_string()]);
    let i: Vec<i64> = match f.get(b"I").unwrap() {
        Object::Array(a) => a
            .iter()
            .filter_map(|o| match o {
                Object::Integer(n) => Some(*n),
                _ => None,
            })
            .collect(),
        _ => panic!("/I array"),
    };
    assert_eq!(i, vec![0, 3], "/I = sorted /Opt indices (EN=0, FR=3)");
}

#[test]
fn gate_nonascii_winansi_and_utf16_fallback() {
    let mut doc = reopen(&fx::nonascii());
    // Latin-1 representable → WinAnsi appearance, no NeedAppearances fallback.
    let out1 = apply_field_value(&mut doc, "latin1_name", WriteValue::Text("Café Zürich")).unwrap();
    assert!(out1.appearances_generated >= 1);
    assert!(!out1.need_appearances_fallback);
    // CJK not WinAnsi-representable → /V is UTF-16BE, appearance deferred.
    let out2 = apply_field_value(&mut doc, "cjk_name", WriteValue::Text("日本語")).unwrap();
    assert!(out2.need_appearances_fallback, "CJK must fall back");
    let saved = reopen(&save(&mut doc));
    let latin = field_dict(&saved, "latin1_name");
    assert_eq!(
        lopdf::decode_text_string(latin.get(b"V").unwrap()).unwrap(),
        "Café Zürich"
    );
    let cjk = field_dict(&saved, "cjk_name");
    assert!(v_bytes(cjk).starts_with(b"\xFE\xFF"), "UTF-16BE BOM");
    assert!(!has_ap(cjk), "stale /AP dropped on fallback");
}

#[test]
fn gate_needappearances_regenerates_ap() {
    let mut doc = reopen(&fx::needappearances_missing_ap());
    let out = regenerate_appearances(&mut doc).unwrap();
    assert!(
        out.appearances_generated >= 1,
        "regenerate_appearances must materialise /AP for the prefilled field"
    );
    let saved = reopen(&save(&mut doc));
    let f = field_dict(&saved, "prefilled");
    assert_eq!(v_string(f), "existing value");
    assert!(has_ap(f), "/AP materialised");
}

#[test]
fn gate_signature_is_modeled_but_not_fillable() {
    let bytes = fx::signature();
    // Modeled as a Signature field.
    let pdf = pdf_syntax::Pdf::new(std::sync::Arc::new(bytes.clone())).unwrap();
    let model = parse_acroform(&pdf).map(|t| build_form_model(&t)).unwrap();
    assert!(
        model
            .iter()
            .any(|f| matches!(f.kind, FormFieldKind::Signature)),
        "signature field must be modeled"
    );
    // Not fillable: text write is rejected with WrongType.
    let mut doc = reopen(&bytes);
    assert!(matches!(
        apply_field_value(&mut doc, "signature1", WriteValue::Text("x")),
        Err(WritebackError::WrongType { .. })
    ));
}

#[test]
fn gate_xfa_shell_acroform_side_is_fillable() {
    // A static-XFA shell: parse_acroform must still expose the AcroForm field,
    // and the writeback must fill it.
    let bytes = fx::xfa_shell();
    let pdf = pdf_syntax::Pdf::new(std::sync::Arc::new(bytes.clone())).unwrap();
    let model = parse_acroform(&pdf)
        .map(|t| build_form_model(&t))
        .unwrap_or_default();
    assert!(
        model.iter().any(|f| f.name == "acro_name"),
        "AcroForm field must be visible despite the /XFA shell"
    );
    let mut doc = reopen(&bytes);
    apply_field_value(&mut doc, "acro_name", WriteValue::Text("hybrid")).unwrap();
    let saved = reopen(&save(&mut doc));
    assert_eq!(v_string(field_dict(&saved, "acro_name")), "hybrid");
}

#[test]
fn gate_hierarchical_names() {
    let mut doc = reopen(&fx::hierarchical());
    apply_field_value(&mut doc, "address.street", WriteValue::Text("Damrak 1")).unwrap();
    apply_field_value(&mut doc, "address.city", WriteValue::Text("Amsterdam")).unwrap();
    let saved = reopen(&save(&mut doc));
    assert_eq!(v_string(field_dict(&saved, "address.street")), "Damrak 1");
    assert_eq!(v_string(field_dict(&saved, "address.city")), "Amsterdam");
}

#[test]
fn gate_readonly_rejected() {
    let mut doc = reopen(&fx::readonly());
    assert!(matches!(
        apply_field_value(&mut doc, "locked", WriteValue::Text("x")),
        Err(WritebackError::ReadOnly(_))
    ));
}

/// Sweep all categories through the model parser and assert none panics and
/// every fixture is a parseable AcroForm with at least one field (except the
/// XFA shell, which still has its AcroForm side).
#[test]
fn gate_all_fixtures_parse() {
    for fixture in fx::all_fixtures() {
        let pdf = pdf_syntax::Pdf::new(std::sync::Arc::new(fixture.bytes.clone()))
            .unwrap_or_else(|e| panic!("category {} unparseable: {e:?}", fixture.category));
        let model = parse_acroform(&pdf)
            .map(|t| build_form_model(&t))
            .unwrap_or_default();
        assert!(
            !model.is_empty(),
            "category {} produced an empty form model",
            fixture.category
        );
    }
}
