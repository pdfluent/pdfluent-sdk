//! Core JSON types for the XFA JSON-first API.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A JSON-friendly representation of an XFA form's data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormData {
    /// Field values keyed by SOM-style dotted path.
    pub fields: IndexMap<String, FieldValue>,
}

/// A typed form field value with automatic coercion from XFA string values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// Numeric value (coerced from string when parseable as f64).
    Number(f64),
    /// Boolean value (coerced from "true"/"false"/"1"/"0").
    Boolean(bool),
    /// Text string value.
    Text(String),
    /// Null value (from empty or missing fields).
    Null,
    /// Repeating section: array of sub-objects.
    Array(Vec<IndexMap<String, FieldValue>>),
}

/// Schema metadata for a form's fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSchema {
    /// Field schemas keyed by SOM-style dotted path.
    pub fields: IndexMap<String, FieldSchema>,
}

/// The semantic type of a form field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Free-text input.
    Text,
    /// Numeric input.
    Numeric,
    /// Boolean (checkbox/toggle).
    Boolean,
    /// Static content (Draw elements).
    Static,
}

/// Schema for a single form field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Full SOM-style path to the field.
    pub som_path: String,
    /// Semantic field type.
    pub field_type: FieldType,
    /// Whether this field is required (occur.min > 0).
    pub required: bool,
    /// Whether the parent section can repeat.
    pub repeatable: bool,
    /// Maximum occurrences (None = unlimited).
    pub max_occurrences: Option<u32>,
    /// FormCalc calculate script, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calculate: Option<String>,
    /// FormCalc validate script, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_value_json_roundtrip() {
        let val = FieldValue::Number(42.5);
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "42.5");

        let val = FieldValue::Boolean(true);
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "true");

        let val = FieldValue::Text("hello".to_string());
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "\"hello\"");

        let val = FieldValue::Null;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "null");
    }

    #[test]
    fn form_data_json_roundtrip() {
        let mut fields = IndexMap::new();
        fields.insert("name".to_string(), FieldValue::Text("Acme".to_string()));
        fields.insert("amount".to_string(), FieldValue::Number(100.0));
        let data = FormData { fields };

        let json = serde_json::to_string_pretty(&data).unwrap();
        let parsed: FormData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.fields.len(), 2);
    }

    #[test]
    fn field_schema_omits_none_scripts() {
        let schema = FieldSchema {
            som_path: "form1.Name".to_string(),
            field_type: FieldType::Text,
            required: true,
            repeatable: false,
            max_occurrences: Some(1),
            calculate: None,
            validate: None,
        };
        let json = serde_json::to_string(&schema).unwrap();
        assert!(!json.contains("calculate"));
        assert!(!json.contains("validate"));
    }
}
