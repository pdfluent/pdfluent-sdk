//! Type coercion for XFA field values.
//!
//! XFA stores all field values as strings. This module converts them to
//! appropriate JSON types: numbers, booleans, or nulls.

use crate::types::FieldValue;

/// Coerce an XFA string value into a typed `FieldValue`.
///
/// Coercion rules (in order of precedence):
/// 1. Empty string → `Null`
/// 2. "true" / "false" (case-insensitive) → `Boolean`
/// 3. "1" / "0" (standalone) → `Boolean`
/// 4. Parseable as f64 → `Number`
/// 5. Otherwise → `Text`
pub(crate) fn coerce_value(s: &str) -> FieldValue {
    let trimmed = s.trim();

    // Empty → Null
    if trimmed.is_empty() {
        return FieldValue::Null;
    }

    // Boolean literals
    match trimmed.to_ascii_lowercase().as_str() {
        "true" => return FieldValue::Boolean(true),
        "false" => return FieldValue::Boolean(false),
        _ => {}
    }

    // Numeric: try parsing as f64
    // Exclude strings that are just "0" or "1" — treat those as booleans
    if trimmed == "0" {
        return FieldValue::Boolean(false);
    }
    if trimmed == "1" {
        return FieldValue::Boolean(true);
    }

    if let Ok(n) = trimmed.parse::<f64>() {
        if n.is_finite() {
            return FieldValue::Number(n);
        }
    }

    FieldValue::Text(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_null() {
        assert_eq!(coerce_value(""), FieldValue::Null);
        assert_eq!(coerce_value("  "), FieldValue::Null);
    }

    #[test]
    fn boolean_literals() {
        assert_eq!(coerce_value("true"), FieldValue::Boolean(true));
        assert_eq!(coerce_value("false"), FieldValue::Boolean(false));
        assert_eq!(coerce_value("TRUE"), FieldValue::Boolean(true));
        assert_eq!(coerce_value("False"), FieldValue::Boolean(false));
    }

    #[test]
    fn zero_one_are_boolean() {
        assert_eq!(coerce_value("0"), FieldValue::Boolean(false));
        assert_eq!(coerce_value("1"), FieldValue::Boolean(true));
    }

    #[test]
    fn numbers() {
        assert_eq!(coerce_value("42"), FieldValue::Number(42.0));
        assert_eq!(coerce_value("3.14"), FieldValue::Number(3.14));
        assert_eq!(coerce_value("-100.5"), FieldValue::Number(-100.5));
        assert_eq!(coerce_value("0.0"), FieldValue::Number(0.0));
    }

    #[test]
    fn text_fallback() {
        assert_eq!(
            coerce_value("Hello World"),
            FieldValue::Text("Hello World".to_string())
        );
        assert_eq!(
            coerce_value("123 Main St"),
            FieldValue::Text("123 Main St".to_string())
        );
    }

    #[test]
    fn preserves_original_string() {
        // Text values preserve the original (untrimmed) string
        assert_eq!(
            coerce_value(" hello "),
            FieldValue::Text(" hello ".to_string())
        );
    }
}
