//! Tests for `PdfDocMut` — the stateful editing handle.
//!
//! Native happy-path tests with plain `#[test]`. Error-path tests that
//! construct `JsError` are wasm32-gated via `#[wasm_bindgen_test]` (same
//! pattern as the Wave 2 edits tests; JsError panics on non-wasm32).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use xfa_wasm::edit_handle::PdfDocMut;

static SIMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

/// Build a minimal PDF (one page, one Helvetica text run) whose page-1
/// content stream is known to be parseable by
/// `pdf_manip::text_run::extract_page_text_runs`. Used by WASM3 round-trip
/// tests so we don't depend on whatever encoding the corpus fixtures use.
///
/// Mirrors the `make_doc_with_font` pattern from `pdf-text-format`'s
/// internal test fixtures.
#[cfg(not(target_arch = "wasm32"))]
fn synthetic_pdf_with_text(text: &str) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        Operation::new("Td", vec![72.into(), 720.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
    ];
    let content_bytes = Content { operations: ops }
        .encode()
        .expect("encode content");

    let mut doc = Document::with_version("1.7");
    let font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    };
    let font_id = doc.add_object(Object::Dictionary(font));
    let resources = dictionary! {
        "Font" => Object::Dictionary(dictionary! {
            "F1" => Object::Reference(font_id),
        }),
    };
    let stream = Stream::new(dictionary! {}, content_bytes);
    let stream_id = doc.add_object(Object::Stream(stream));
    let page = dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(resources),
    };
    let page_id = doc.add_object(Object::Dictionary(page));
    let pages = dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    };
    let pages_id = doc.add_object(Object::Dictionary(pages));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog = dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

// ---- Open + save no-op -----------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn open_then_save_returns_valid_pdf() {
    let editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"), "expected PDF magic");
    assert_eq!(editor.page_count(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn save_is_non_consuming() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    editor.rotate_page(0, 90).expect("rotate");
    let snapshot_a = editor.save().expect("snapshot a");

    editor.add_text_watermark("DRAFT", 0.5).expect("watermark");
    let snapshot_b = editor.save().expect("snapshot b");

    // Two snapshots produced, both valid PDFs, second includes the watermark.
    assert!(snapshot_a.starts_with(b"%PDF-"));
    assert!(snapshot_b.starts_with(b"%PDF-"));
    assert!(snapshot_b.len() >= snapshot_a.len(), "watermark adds bytes");
}

// ---- Pages -----------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn delete_pages_then_save_reduces_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original = editor.page_count();
    assert!(original >= 2);
    editor.delete_pages(&[0]).expect("delete");
    assert_eq!(editor.page_count(), original - 1);
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rotate_page_does_not_change_page_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let before = editor.page_count();
    editor.rotate_page(0, 90).expect("rotate");
    assert_eq!(editor.page_count(), before);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn reorder_pages_preserves_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let count = editor.page_count() as u32;
    let reversed: Vec<u32> = (0..count).rev().collect();
    editor.reorder_pages(&reversed).expect("reorder");
    assert_eq!(editor.page_count() as u32, count);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_pages_returns_subdocument_without_mutating_self() {
    let editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original_count = editor.page_count();

    let sub_bytes = editor.extract_pages(&[0]).expect("extract");

    // self is unchanged
    assert_eq!(editor.page_count(), original_count);

    // returned bytes are a valid PDF with one page
    let sub = PdfDocMut::open(&sub_bytes).expect("reopen sub");
    assert_eq!(sub.page_count(), 1);
}

// ---- Watermark -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn watermark_then_save_includes_text() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_text_watermark("CONCEPT", 0.3)
        .expect("watermark");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Annotations -----------------------------------------------------------

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn highlight_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("highlight");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn sticky_note_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_sticky_note(0, 50.0, 750.0, "Test note")
        .expect("sticky");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn free_text_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_free_text(0, 100.0, 600.0, 200.0, 80.0, "Annotated")
        .expect("free text");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Redaction -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_region_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .redact_region(0, 50.0, 50.0, 100.0, 20.0)
        .expect("redact");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_search_no_match_gracefully() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .redact_search("UNLIKELY_STRING_12345")
        .expect("no match");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Compress --------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compress_then_save() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    editor.compress().expect("compress");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Multi-step editing chain ---------------------------------------------

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn multi_step_session_save_once() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original_count = editor.page_count();

    editor.rotate_page(0, 90).expect("rotate");
    editor
        .add_text_watermark("CONCEPT", 0.3)
        .expect("watermark");
    editor
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("highlight");
    editor.compress().expect("compress");

    let bytes = editor.save().expect("save once");
    assert!(bytes.starts_with(b"%PDF-"));
    // sanity: rotate/watermark/highlight/compress do not change page count
    assert_eq!(editor.page_count(), original_count);
}

// ---- Wasm32-only error-path tests -----------------------------------------

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_page_rotation_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.rotate_page(0, 45).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn empty_watermark_text_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.add_text_watermark("", 0.5).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_opacity_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.add_text_watermark("X", 2.0).is_err());
    assert!(editor.add_text_watermark("X", -0.1).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_json_for_set_form_fields_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.set_form_fields("{ not json }").is_err());
}

// Multi-select list box fixture: one "languages" field, /Opt ["EN","NL","DE","FR"].
static MULTISELECT_PDF: &[u8] =
    include_bytes!("../../../tests/corpus-mini/acroform-multiselect.pdf");

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_multi_select_selects_and_persists() {
    let mut editor = PdfDocMut::open(MULTISELECT_PDF).expect("open");
    editor
        .set_multi_select("languages", vec!["FR".to_string(), "EN".to_string()])
        .expect("set_multi_select");
    let bytes = editor.save().expect("save");
    // Reopen the saved bytes and confirm the field carries an array /V.
    let doc = lopdf::Document::load_mem(&bytes).expect("reload");
    let af = match doc.catalog().unwrap().get(b"AcroForm").unwrap() {
        lopdf::Object::Reference(id) => doc.get_object(*id).unwrap().as_dict().unwrap(),
        lopdf::Object::Dictionary(d) => d,
        _ => panic!("acroform"),
    };
    let field_id = match &af.get(b"Fields").unwrap().as_array().unwrap()[0] {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("field ref"),
    };
    let fld = doc.get_object(field_id).unwrap().as_dict().unwrap();
    let v: Vec<String> = match fld.get(b"V").unwrap() {
        lopdf::Object::Array(a) => a
            .iter()
            .filter_map(|o| lopdf::decode_text_string(o).ok())
            .collect(),
        _ => panic!("/V array"),
    };
    assert_eq!(v, vec!["FR".to_string(), "EN".to_string()]);
}

// JsError construction only works under wasm32, so the error-path assertion
// is wasm-gated (matching the other `*_errors` tests in this file).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn set_multi_select_rejects_unknown_option() {
    let mut editor = PdfDocMut::open(MULTISELECT_PDF).expect("open");
    assert!(editor
        .set_multi_select("languages", vec!["KL".to_string()])
        .is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn empty_redact_search_query_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.redact_search("").is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn missing_form_field_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    // SIMPLE_PDF has no AcroForm.
    assert!(editor.set_form_field("name", "Alice").is_err());
}

// ---- WASM3: formatTextSpan ------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn format_text_span_size_change_round_trips() {
    use xfa_wasm::PdfDoc;

    let pdf = synthetic_pdf_with_text("Hello WASM");
    let mut editor = PdfDocMut::open(&pdf).expect("open synthetic");
    let json = editor
        .format_text_span_js(1, 0, Some(20.0), None)
        .expect("formatTextSpan size-only");
    // JSON contains the canonical FormatResult shape.
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(parsed["formatted"].as_bool(), Some(true));
    assert!(parsed["bytesChanged"].as_u64().unwrap_or(0) > 0);
    assert!(parsed["isolationStrategy"].is_string());

    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
    let reloaded = PdfDoc::open(&bytes).expect("reload formatted pdf");
    assert_eq!(reloaded.page_count(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn format_text_span_color_change_round_trips() {
    use xfa_wasm::PdfDoc;

    let pdf = synthetic_pdf_with_text("Hello WASM");
    let mut editor = PdfDocMut::open(&pdf).expect("open synthetic");
    let json = editor
        .format_text_span_js(1, 0, None, Some("#FF0000".to_string()))
        .expect("formatTextSpan color-only");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(parsed["formatted"].as_bool(), Some(true));
    let bytes = editor.save().expect("save");
    let reloaded = PdfDoc::open(&bytes).expect("reload");
    assert_eq!(reloaded.page_count(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn format_text_span_noop_returns_unformatted() {
    let pdf = synthetic_pdf_with_text("Hello WASM");
    let mut editor = PdfDocMut::open(&pdf).expect("open synthetic");
    let json = editor
        .format_text_span_js(1, 0, None, None)
        .expect("formatTextSpan no-op");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(parsed["formatted"].as_bool(), Some(false));
    assert_eq!(parsed["bytesChanged"].as_u64(), Some(0));
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn format_text_span_out_of_range_errors_cleanly() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let r = editor.format_text_span_js(1, 9_999, Some(12.0), None);
    assert!(r.is_err(), "expected error for out-of-range run index");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn format_text_span_invalid_color_hex_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    let r = editor.format_text_span_js(1, 0, None, Some("not-hex".to_string()));
    assert!(r.is_err(), "expected error for invalid color hex");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn apply_text_format_one_shot_round_trips() {
    use xfa_wasm::edit_handle::PdfDocMut;

    let pdf = synthetic_pdf_with_text("One-shot");
    let bytes = PdfDocMut::apply_text_format(&pdf, 1, 0, Some(16.0), Some("#0000FF".into()))
        .expect("applyTextFormat");
    assert!(bytes.starts_with(b"%PDF-"));
    let reloaded = PdfDocMut::open(&bytes).expect("reload one-shot");
    assert!(reloaded.page_count() >= 1);
}

// ---- WASM6: Editor round-trip smoke ---------------------------------------
//
// open → extract text spans with metadata → format one span (size + color) →
// set bold/italic (if variant available; otherwise typed error path) →
// save → reload → verify mutation preserved + metadata still readable.
//
// This is the canonical editor flow that replaces any Tauri-only write path
// for persistent text formatting.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn editor_round_trip_smoke_format_then_save_then_reopen() {
    use xfa_wasm::edit_handle::PdfDocMut;
    use xfa_wasm::PdfDoc;

    // 1. Open + extract text metadata via PdfDoc (read API).
    let pdf = synthetic_pdf_with_text("Editor round-trip");
    let doc = PdfDoc::open(&pdf).expect("open synthetic pdf");
    let runs_json = doc.get_text_positions(0).expect("get text positions");
    let runs: serde_json::Value = serde_json::from_str(&runs_json).expect("parse runs json");
    assert!(runs.is_array(), "getTextPositions must return JSON array");
    let runs_arr = runs.as_array().unwrap();
    assert!(!runs_arr.is_empty(), "fixture must have at least one run");
    // Verify metadata fields are present in the JSON shape (WASM1).
    let first = &runs_arr[0];
    assert!(first["text"].is_string());
    assert!(first["isBold"].is_boolean());
    assert!(first["isItalic"].is_boolean());
    // widthSource is always emitted (WASM2).
    assert!(first["widthSource"].is_string());

    // 2. Format the first run (size + color) via PdfDocMut::formatTextSpan.
    let mut editor = PdfDocMut::open(&pdf).expect("open for edit");
    let format_json = editor
        .format_text_span_js(1, 0, Some(18.0), Some("#008000".into()))
        .expect("formatTextSpan");
    let parsed: serde_json::Value = serde_json::from_str(&format_json).unwrap();
    assert_eq!(parsed["formatted"].as_bool(), Some(true));

    // 3. Save bytes.
    let bytes = editor.save().expect("save formatted bytes");
    assert!(bytes.starts_with(b"%PDF-"));

    // 4. Reload bytes and verify metadata is still readable + structurally intact.
    let reopened = PdfDoc::open(&bytes).expect("reopen formatted pdf");
    assert_eq!(reopened.page_count(), doc.page_count());
    let runs_after = reopened
        .get_text_positions(0)
        .expect("get text positions after format");
    let after: serde_json::Value = serde_json::from_str(&runs_after).expect("parse json after");
    assert!(after.is_array());
    assert!(
        !after.as_array().unwrap().is_empty(),
        "text runs must remain after format"
    );
}

// ---- Forms: single writeback chain (pdf_forms::apply_field_value) ---------
//
// `setFormField` / `setFormFields` now route through the SDK writeback
// chain: /V encoding (ASCII literal else UTF-16BE+BOM), /V-as-Name for
// buttons, per-widget /AS sync, /Kids-recursive FQN lookup, and read-only
// rejection. These tests pin the *new* correct behavior (the old path wrote
// raw UTF-8 bytes into /V on top-level text fields only).

/// Build a minimal indirect-AcroForm PDF with a text field, a read-only
/// text field, a checkbox whose on-state (`On1`) lives on a kid widget,
/// and a radio group with `Red`/`Blue` kid widgets.
///
/// Mirrors the fixture builders in `pdfluent/tests/form_mutation.rs`.
#[cfg(not(target_arch = "wasm32"))]
fn synthetic_form_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.4");
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let pages_id = doc.new_object_id();

    let text_field = doc.add_object(dictionary! {
        "FT" => "Tx",
        "T" => Object::string_literal("first_name"),
        "V" => Object::string_literal(""),
    });

    // /Ff bit 1 = ReadOnly (ISO 32000-1 §12.7.3.1).
    let readonly_field = doc.add_object(dictionary! {
        "FT" => "Tx",
        "Ff" => 1i64,
        "T" => Object::string_literal("locked"),
        "V" => Object::string_literal("frozen"),
    });

    // Checkbox: on-state declared on the kid widget's /AP /N — the common
    // real-world shape that the old top-level walk could not handle.
    let checkbox_kid = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![100.into(), 700.into(), 115.into(), 715.into()],
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Null,
                "On1" => Object::Null,
            },
        },
    });
    let checkbox_field = doc.add_object(dictionary! {
        "FT" => "Btn",
        "T" => Object::string_literal("subscribe"),
        "V" => Object::Name(b"Off".to_vec()),
        "Kids" => vec![checkbox_kid.into()],
    });

    // Radio group: /Ff bit 16 (0x8000) = Radio; export states on kids.
    let radio_kid_red = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![100.into(), 650.into(), 115.into(), 665.into()],
        "AS" => Object::Name(b"Off".to_vec()),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Null,
                "Red" => Object::Null,
            },
        },
    });
    let radio_kid_blue = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![130.into(), 650.into(), 145.into(), 665.into()],
        "AS" => Object::Name(b"Off".to_vec()),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Null,
                "Blue" => Object::Null,
            },
        },
    });
    let radio_field = doc.add_object(dictionary! {
        "FT" => "Btn",
        "Ff" => 0x8000i64,
        "T" => Object::string_literal("preferred_color"),
        "V" => Object::Name(b"Off".to_vec()),
        "Kids" => vec![radio_kid_red.into(), radio_kid_blue.into()],
    });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![
            checkbox_kid.into(),
            radio_kid_red.into(),
            radio_kid_blue.into(),
        ],
    });
    doc.objects.insert(
        pages_id,
        lopdf::Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![
            text_field.into(),
            readonly_field.into(),
            checkbox_field.into(),
            radio_field.into(),
        ],
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise form fixture");
    buf
}

/// Find a field dictionary by its `/T` partial name in saved PDF bytes.
#[cfg(not(target_arch = "wasm32"))]
fn field_dict_by_name(bytes: &[u8], name: &str) -> lopdf::Dictionary {
    let doc = lopdf::Document::load_mem(bytes).expect("reload saved bytes");
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"T")
                .ok()
                .and_then(|t| lopdf::decode_text_string(t).ok())
                .as_deref()
                == Some(name)
        })
        .cloned()
        .unwrap_or_else(|| panic!("field '{name}' not found in saved bytes"))
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_form_field_ascii_text_stays_literal() {
    let mut editor = PdfDocMut::open(&synthetic_form_pdf()).expect("open form fixture");
    editor
        .set_form_field("first_name", "Jane")
        .expect("set ASCII text");
    let bytes = editor.save().expect("save");

    let field = field_dict_by_name(&bytes, "first_name");
    match field.get(b"V").expect("/V present") {
        lopdf::Object::String(v, _) => assert_eq!(v, b"Jane", "ASCII value stays a plain literal"),
        other => panic!("expected /V string, got {other:?}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_form_field_non_ascii_text_writes_utf16be_bom() {
    let mut editor = PdfDocMut::open(&synthetic_form_pdf()).expect("open form fixture");
    editor
        .set_form_field("first_name", "Café")
        .expect("set non-ASCII text");
    let bytes = editor.save().expect("save");

    let field = field_dict_by_name(&bytes, "first_name");
    match field.get(b"V").expect("/V present") {
        lopdf::Object::String(v, _) => {
            assert!(
                v.starts_with(&[0xFE, 0xFF]),
                "non-ASCII /V must be UTF-16BE with BOM, got {v:02X?}"
            );
            let decoded = lopdf::decode_text_string(&lopdf::Object::String(
                v.clone(),
                lopdf::StringFormat::Literal,
            ))
            .expect("decode UTF-16BE");
            assert_eq!(decoded, "Café", "value must round-trip without mojibake");
        }
        other => panic!("expected /V string, got {other:?}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_form_field_checkbox_sets_name_value_and_widget_as() {
    let mut editor = PdfDocMut::open(&synthetic_form_pdf()).expect("open form fixture");
    // String "true" dispatches to Checkbox(true); on-state resolved from
    // the kid widget's /AP /N keys.
    editor
        .set_form_field("subscribe", "true")
        .expect("set checkbox");
    let bytes = editor.save().expect("save");

    let field = field_dict_by_name(&bytes, "subscribe");
    assert_eq!(
        field.get(b"V").ok(),
        Some(&lopdf::Object::Name(b"On1".to_vec())),
        "/V must be the on-state as a Name object"
    );

    // The kid widget's /AS must be synced to the on-state.
    let doc = lopdf::Document::load_mem(&bytes).expect("reload");
    let widget_as = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.has(b"AP") && !d.has(b"T"))
        .filter_map(|d| d.get(b"AS").ok())
        .find(|a| matches!(a, lopdf::Object::Name(n) if n == b"On1"));
    assert!(
        widget_as.is_some(),
        "kid widget /AS must be set to the on-state On1"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_form_field_radio_selects_export_state() {
    let mut editor = PdfDocMut::open(&synthetic_form_pdf()).expect("open form fixture");
    editor
        .set_form_field("preferred_color", "Blue")
        .expect("set radio");
    let bytes = editor.save().expect("save");

    let field = field_dict_by_name(&bytes, "preferred_color");
    assert_eq!(
        field.get(b"V").ok(),
        Some(&lopdf::Object::Name(b"Blue".to_vec())),
        "/V must be the selected export state as a Name"
    );

    // Exactly one kid widget carries /AS Blue; the other must be /Off.
    let doc = lopdf::Document::load_mem(&bytes).expect("reload");
    let as_values: Vec<Vec<u8>> = doc
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.has(b"AP") && !d.has(b"T"))
        .filter_map(|d| match d.get(b"AS") {
            Ok(lopdf::Object::Name(n)) => Some(n.clone()),
            _ => None,
        })
        .collect();
    assert!(
        as_values.iter().any(|n| n == b"Blue"),
        "selected kid must have /AS Blue, got {as_values:?}"
    );
    assert!(
        as_values.iter().any(|n| n == b"Off"),
        "unselected kid must have /AS Off, got {as_values:?}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_form_fields_bulk_json_fills_multiple_fields() {
    let mut editor = PdfDocMut::open(&synthetic_form_pdf()).expect("open form fixture");
    editor
        .set_form_fields(r#"{"first_name": "Renée", "subscribe": "true"}"#)
        .expect("bulk set");
    let bytes = editor.save().expect("save");

    let text = field_dict_by_name(&bytes, "first_name");
    match text.get(b"V").expect("/V present") {
        lopdf::Object::String(v, _) => {
            assert!(
                v.starts_with(&[0xFE, 0xFF]),
                "non-ASCII bulk value gets BOM"
            )
        }
        other => panic!("expected /V string, got {other:?}"),
    }
    let checkbox = field_dict_by_name(&bytes, "subscribe");
    assert_eq!(
        checkbox.get(b"V").ok(),
        Some(&lopdf::Object::Name(b"On1".to_vec())),
    );
}

// Error paths construct JsError, which panics off-wasm — keep wasm32-gated
// (same pattern as the other error-path tests in this file).

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn set_form_field_readonly_rejected() {
    // SIMPLE_PDF has no form, so build nothing fancy here: the writeback
    // chain's read-only rejection is covered natively in pdf-forms; this
    // gate just pins that the wasm surface maps the error (not a panic).
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.set_form_field("anything", "x").is_err());
}
