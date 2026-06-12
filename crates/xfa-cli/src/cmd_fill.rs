//! Fill AcroForm fields from a JSON file.
//!
//! Delegates to [`pdf_forms::apply_field_value`] — the single SDK writeback
//! chain that keeps `/V`, `/AS` and `/AP` consistent and falls back to
//! `/NeedAppearances` only when a value cannot be drawn with the Standard-14
//! WinAnsi fonts.

use anyhow::{Context, Result};
use pdf_forms::{apply_field_value, WriteValue, WritebackError};
use std::path::Path;

pub fn run(input: &Path, output: &Path, data: &Path) -> Result<()> {
    let json_str = std::fs::read_to_string(data).context("failed to read data JSON")?;
    let values: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&json_str).context("failed to parse JSON as object")?;

    let pdf_bytes = std::fs::read(input).context("failed to read input PDF")?;
    let mut doc = lopdf::Document::load_mem(&pdf_bytes).context("failed to parse PDF")?;

    let mut filled = 0usize;
    for (name, val) in &values {
        let value_str = match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        };
        match fill_one(&mut doc, name, val, &value_str) {
            Ok(outcome) => {
                filled += 1;
                let mut notes = Vec::new();
                if outcome.appearances_generated > 0 {
                    notes.push(format!("{} appearance(s)", outcome.appearances_generated));
                }
                if outcome.appearance_states_set > 0 {
                    notes.push(format!("{} widget state(s)", outcome.appearance_states_set));
                }
                if outcome.need_appearances_fallback {
                    notes.push("NeedAppearances fallback".to_string());
                }
                let suffix = if notes.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", notes.join(", "))
                };
                println!("  {name} = {value_str}{suffix}");
            }
            Err(e) => eprintln!("  warning: {name}: {e}"),
        }
    }

    doc.save(output).context("failed to save output PDF")?;
    println!(
        "Filled {filled}/{} fields -> {}",
        values.len(),
        output.display()
    );
    Ok(())
}

/// Apply one JSON value with type-aware dispatch: booleans drive checkboxes,
/// strings try text -> radio -> choice in order of the field's actual type.
fn fill_one(
    doc: &mut lopdf::Document,
    name: &str,
    raw: &serde_json::Value,
    value_str: &str,
) -> Result<pdf_forms::WriteOutcome, WritebackError> {
    if let serde_json::Value::Bool(b) = raw {
        return apply_field_value(doc, name, WriteValue::Checkbox(*b));
    }
    // Strings: let the writeback's type check route us. Try text first (the
    // common case), then the other families on a type mismatch.
    match apply_field_value(doc, name, WriteValue::Text(value_str)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Radio(value_str)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Choice(value_str)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    // Checkbox via string ("true"/"Yes"/"Off"/"false").
    let on = !matches!(value_str, "false" | "Off" | "0" | "");
    apply_field_value(doc, name, WriteValue::Checkbox(on))
}
