//! JS value type returned from script execution.

/// A JavaScript value returned by [`crate::XfaJsRuntime::execute_calculate`].
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    /// A string value.
    String(String),
    /// A numeric value.
    Number(f64),
    /// A boolean value.
    Bool(bool),
    /// `null` or an expression that evaluated to null.
    Null,
    /// `undefined` or an expression with no meaningful return value.
    Undefined,
}

impl JsValue {
    /// Convert to a raw string suitable for use as an XFA field `rawValue`.
    pub fn to_raw_string(&self) -> String {
        match self {
            JsValue::String(s) => s.clone(),
            JsValue::Number(n) => {
                if n.is_finite() && *n == n.floor() {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            JsValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
            JsValue::Null | JsValue::Undefined => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_number_formats_without_decimal() {
        assert_eq!(JsValue::Number(42.0).to_raw_string(), "42");
    }

    #[test]
    fn float_number_formats_with_decimal() {
        assert_eq!(JsValue::Number(3.14).to_raw_string(), "3.14");
    }

    #[test]
    fn bool_maps_to_1_0() {
        assert_eq!(JsValue::Bool(true).to_raw_string(), "1");
        assert_eq!(JsValue::Bool(false).to_raw_string(), "0");
    }

    #[test]
    fn null_and_undefined_are_empty() {
        assert_eq!(JsValue::Null.to_raw_string(), "");
        assert_eq!(JsValue::Undefined.to_raw_string(), "");
    }
}
