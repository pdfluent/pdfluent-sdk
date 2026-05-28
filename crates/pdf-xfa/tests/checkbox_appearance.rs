//! Tests for checkbox value-to-appearance mapping (XFA 3.3 §11.2.1 / §17.8).
//!
//! Verifies that `checkbox_raw_value_is_checked` and `checkbox_appearance`
//! implement the correct Adobe semantics:
//!   - rawValue "0"     → unchecked (no X mark in appearance stream)
//!   - rawValue ""      → unchecked
//!   - no rawValue      → unchecked (represented as "" in the binding layer)
//!   - rawValue "1"     → checked   (X mark drawn)
//!   - rawValue non-0   → checked   (custom on-value)

use pdf_xfa::appearance_bridge::{checkbox_appearance, checkbox_raw_value_is_checked};

// ── helper ────────────────────────────────────────────────────────────────────

/// Returns true when the appearance stream content contains X-mark drawing
/// operators.  The default cross mark uses two diagonal `m`/`l` pairs
/// followed by an `S` stroke operator.
fn has_x_mark(ap: &pdf_xfa::appearance_bridge::AppearanceStream) -> bool {
    let content = String::from_utf8_lossy(&ap.content);
    // checkbox_appearance draws diagonals like "2.40 2.40 m\n9.60 9.60 l\nS\n"
    content.contains(" m\n") && content.contains(" l\n") && content.contains("S\n")
}

// ── Case 1: rawValue "0" → unchecked ─────────────────────────────────────────

#[test]
fn raw_value_zero_is_unchecked() {
    assert!(
        !checkbox_raw_value_is_checked("0"),
        "rawValue=\"0\" must map to unchecked"
    );
}

#[test]
fn raw_value_zero_appearance_has_no_x_mark() {
    let checked = checkbox_raw_value_is_checked("0");
    let ap = checkbox_appearance(checked, 12.0, 12.0);
    assert!(
        !has_x_mark(&ap),
        "rawValue=\"0\" must not produce an X mark in the appearance stream"
    );
}

// ── Case 2: rawValue "" → unchecked ──────────────────────────────────────────

#[test]
fn raw_value_empty_is_unchecked() {
    assert!(
        !checkbox_raw_value_is_checked(""),
        "rawValue=\"\" must map to unchecked"
    );
}

// ── Case 3: no rawValue (absent binding → treated as "") → unchecked ─────────

#[test]
fn raw_value_missing_is_unchecked() {
    // In the binding layer an unbound / absent value arrives as an empty string.
    let bound_value: &str = "";
    assert!(
        !checkbox_raw_value_is_checked(bound_value),
        "absent rawValue (represented as \"\") must map to unchecked"
    );
}

// ── Case 4: rawValue "1" → checked ───────────────────────────────────────────

#[test]
fn raw_value_one_is_checked() {
    assert!(
        checkbox_raw_value_is_checked("1"),
        "rawValue=\"1\" must map to checked"
    );
}

#[test]
fn raw_value_one_appearance_has_x_mark() {
    let checked = checkbox_raw_value_is_checked("1");
    let ap = checkbox_appearance(checked, 12.0, 12.0);
    assert!(
        has_x_mark(&ap),
        "rawValue=\"1\" must produce an X mark in the appearance stream"
    );
}

// ── Case 5: rawValue not "0"/""/missing → checked ────────────────────────────

#[test]
fn raw_value_custom_on_is_checked() {
    // XFA forms sometimes use "Yes", "X", "true", or form-specific values.
    for val in &["Yes", "true", "X", "on", "checked", "2"] {
        assert!(
            checkbox_raw_value_is_checked(val),
            "rawValue=\"{val}\" must map to checked (custom on-value)"
        );
    }
}
