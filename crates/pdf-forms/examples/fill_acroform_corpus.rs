//! Fill every AcroForm corpus category through the real SDK writeback chain
//! and write the filled output to disk, for external (pikepdf/qpdf/mutool)
//! verification.
//!
//! Usage: `cargo run -p pdfluent-forms --example fill_acroform_corpus -- <out_dir>`
//!
//! Emits `<category>.filled.pdf` plus a `manifest.json` describing the value
//! written per field, so the external verifier can assert the saved structure
//! independently of our own parser.

#[path = "../tests/common/acroform_fixtures.rs"]
mod fx;

use pdfluent_forms::{apply_choice_multi, apply_field_value, regenerate_appearances, WriteValue};
use std::fmt::Write as _;

fn fill(category: &str, bytes: &[u8]) -> (Vec<u8>, String) {
    let mut doc = lopdf::Document::load_mem(bytes).expect("load fixture");
    // `expect` per category since fixtures are known-good.
    let manifest: String = match category {
        "pure_text" => {
            apply_field_value(&mut doc, "full_name", WriteValue::Text("Jane Doe")).unwrap();
            r#"{"full_name":{"v":"Jane Doe","ap":true}}"#.into()
        }
        "multiline_comb" => {
            apply_field_value(
                &mut doc,
                "notes",
                WriteValue::Text("a long multiline note here"),
            )
            .unwrap();
            apply_field_value(&mut doc, "postcode", WriteValue::Text("1234AB")).unwrap();
            r#"{"notes":{"ap":true},"postcode":{"v":"1234AB","ap":true}}"#.into()
        }
        "checkbox" => {
            apply_field_value(&mut doc, "agree", WriteValue::Checkbox(true)).unwrap();
            r#"{"agree":{"v_name":"Yes","as":"Yes"}}"#.into()
        }
        "radio" => {
            apply_field_value(&mut doc, "gender", WriteValue::Radio("M")).unwrap();
            r#"{"gender":{"v_name":"M"}}"#.into()
        }
        "combo" => {
            apply_field_value(&mut doc, "country", WriteValue::Choice("NL")).unwrap();
            r#"{"country":{"v":"NL"}}"#.into()
        }
        "listbox_single" => {
            apply_field_value(&mut doc, "city", WriteValue::Choice("Berlin")).unwrap();
            r#"{"city":{"v":"Berlin"}}"#.into()
        }
        "multiselect" => {
            apply_choice_multi(&mut doc, "languages", &["FR".to_string(), "EN".to_string()])
                .unwrap();
            r#"{"languages":{"v_array":["FR","EN"],"i":[0,3]}}"#.into()
        }
        "nonascii" => {
            apply_field_value(&mut doc, "latin1_name", WriteValue::Text("Café Zürich")).unwrap();
            apply_field_value(&mut doc, "cjk_name", WriteValue::Text("日本語")).unwrap();
            r#"{"latin1_name":{"v":"Café Zürich","ap":true},"cjk_name":{"utf16":true,"needs_appearances":true}}"#.into()
        }
        "needappearances_missing_ap" => {
            regenerate_appearances(&mut doc).unwrap();
            r#"{"prefilled":{"v":"existing value","ap":true}}"#.into()
        }
        "signature" => {
            // Not fillable — write nothing, just round-trip.
            r#"{"signature1":{"fillable":false}}"#.into()
        }
        "xfa_shell" => {
            apply_field_value(&mut doc, "acro_name", WriteValue::Text("hybrid")).unwrap();
            r#"{"acro_name":{"v":"hybrid"}}"#.into()
        }
        "hierarchical" => {
            apply_field_value(&mut doc, "address.street", WriteValue::Text("Damrak 1")).unwrap();
            apply_field_value(&mut doc, "address.city", WriteValue::Text("Amsterdam")).unwrap();
            r#"{"address.street":{"v":"Damrak 1"},"address.city":{"v":"Amsterdam"}}"#.into()
        }
        "readonly" => {
            // ReadOnly — write rejected; round-trip the original.
            r#"{"locked":{"writable":false}}"#.into()
        }
        other => panic!("unknown category {other}"),
    };
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save filled");
    (buf, manifest)
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .expect("usage: fill_acroform_corpus <out_dir>");
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    let mut manifest = String::from("{\n");
    for (i, fixture) in fx::all_fixtures().iter().enumerate() {
        let (filled, fields) = fill(fixture.category, &fixture.bytes);
        let path = format!("{out_dir}/{}.filled.pdf", fixture.category);
        std::fs::write(&path, &filled).expect("write filled");
        if i > 0 {
            manifest.push_str(",\n");
        }
        let _ = write!(manifest, "  \"{}\": {}", fixture.category, fields);
        println!("{path}\t{} bytes", filled.len());
    }
    manifest.push_str("\n}\n");
    std::fs::write(format!("{out_dir}/manifest.json"), manifest).expect("write manifest");
}
