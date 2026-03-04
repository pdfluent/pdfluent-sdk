//! FormCalc value type with automatic coercion.
//!
//! FormCalc has three value types: Number, String, and Null.
//! Values are automatically coerced between types as needed.

use std::fmt;

/// A FormCalc runtime value.
#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Null,
}

impl Value {
    /// Coerce to number. Empty string and null become 0.
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
            Value::Null => 0.0,
        }
    }

    /// Coerce to string. Numbers are formatted, null becomes empty.
    pub fn to_string_val(&self) -> String {
        match self {
            Value::Number(n) => {
                if *n == n.trunc() && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
        }
    }

    /// Coerce to boolean. 0, empty string, and null are false.
    pub fn to_bool(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Null => false,
        }
    }

    /// Returns true if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_val())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Null, _) | (_, Value::Null) => false,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            // Mixed: coerce to number for comparison
            (Value::Number(a), Value::String(b)) => *a == b.trim().parse::<f64>().unwrap_or(0.0),
            (Value::String(a), Value::Number(b)) => a.trim().parse::<f64>().unwrap_or(0.0) == *b,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Null, Value::Null) => Some(std::cmp::Ordering::Equal),
            (Value::Null, _) | (_, Value::Null) => None,
            (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            // Mixed: coerce to number
            _ => self.to_number().partial_cmp(&other.to_number()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_coercion() {
        assert_eq!(Value::String("42".to_string()).to_number(), 42.0);
        assert_eq!(Value::String("".to_string()).to_number(), 0.0);
        assert_eq!(Value::String("abc".to_string()).to_number(), 0.0);
        assert_eq!(Value::Null.to_number(), 0.0);
    }

    #[test]
    fn string_coercion() {
        assert_eq!(Value::Number(42.0).to_string_val(), "42");
        assert_eq!(Value::Number(3.14).to_string_val(), "3.14");
        assert_eq!(Value::Null.to_string_val(), "");
    }

    #[test]
    fn bool_coercion() {
        assert!(!Value::Number(0.0).to_bool());
        assert!(Value::Number(1.0).to_bool());
        assert!(!Value::String("".to_string()).to_bool());
        assert!(Value::String("x".to_string()).to_bool());
        assert!(!Value::Null.to_bool());
    }

    #[test]
    fn equality() {
        assert_eq!(Value::Number(42.0), Value::Number(42.0));
        assert_eq!(
            Value::String("hello".to_string()),
            Value::String("hello".to_string())
        );
        assert_eq!(Value::Null, Value::Null);
        // Mixed: number == numeric string
        assert_eq!(Value::Number(42.0), Value::String("42".to_string()));
    }
}
