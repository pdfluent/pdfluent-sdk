//! Drift-guard for the canonical text-span wire contract (SDK single source of
//! truth).
//!
//! These tests pin the JSON shape produced by [`pdf_engine::TextSpanInfo`] so
//! the Node (napi), Tauri (serde) and TypeScript bindings cannot silently
//! diverge. The committed fixture `fixtures/text_span_info.sample.json` is the
//! contract anchor the TypeScript interface guard compares against.
//!
//! Requires the `serde` feature: `cargo test -p pdf-engine --features serde`.
#![cfg(feature = "serde")]

use pdf_engine::{TextSpan, TextSpanInfo, WidthSource};

/// A fully-populated span exercising every field, including the optional ones.
fn populated_span() -> TextSpan {
    TextSpan {
        text: "Hi".to_string(),
        x: 10.0,
        y: 20.0,
        width: 30.0,
        // Intentionally != font_size: the wire contract reports font_size here.
        height: 99.0,
        font_size: 12.0,
        font_name: Some("Helvetica-Bold".to_string()),
        is_bold: true,
        is_italic: false,
        color: Some([255, 0, 0, 255]),
        width_source: WidthSource::Metric,
        char_bounds: vec![[10.0, 20.0, 25.0, 32.0]],
    }
}

#[test]
fn fully_populated_span_matches_committed_fixture() {
    let produced = serde_json::to_value(TextSpanInfo::from(populated_span())).expect("serialize");
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/text_span_info.sample.json"))
            .expect("parse fixture");
    assert_eq!(
        produced, expected,
        "wire contract drifted from the committed fixture — update the DTO, every \
         binding, and the fixture together"
    );
}

#[test]
fn wire_key_set_is_stable() {
    let value = serde_json::to_value(TextSpanInfo::from(populated_span())).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "charBounds",
            "color",
            "fontName",
            "font_size",
            "height",
            "isBold",
            "isItalic",
            "text",
            "width",
            "widthSource",
            "x",
            "y",
        ]
    );
}

#[test]
fn optional_fields_are_omitted_when_absent() {
    let span = TextSpan {
        text: "x".to_string(),
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        font_size: 1.0,
        font_name: None,
        is_bold: false,
        is_italic: false,
        color: None,
        width_source: WidthSource::Estimate,
        char_bounds: Vec::new(),
    };
    let value = serde_json::to_value(TextSpanInfo::from(span)).unwrap();
    let obj = value.as_object().unwrap();
    assert!(
        !obj.contains_key("fontName"),
        "fontName must be omitted when None"
    );
    assert!(
        !obj.contains_key("color"),
        "color must be omitted when None"
    );
    assert!(
        !obj.contains_key("charBounds"),
        "charBounds must be omitted when empty"
    );
    // Non-optional fields are always present.
    assert_eq!(obj["widthSource"], "Estimate");
    assert!(obj.contains_key("isBold"));
}
