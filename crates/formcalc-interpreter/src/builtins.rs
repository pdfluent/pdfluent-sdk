//! FormCalc built-in functions.
//!
//! Implements the standard function library from XFA 3.3 §25.
//! Functions are case-insensitive (caller normalizes via lookup).

use crate::error::{FormCalcError, Result};
use crate::value::Value;

/// Try to call a built-in function by name.
///
/// Returns `Ok(None)` if `name` is not a recognized built-in,
/// allowing the caller to fall through to user-defined functions.
pub fn call_builtin(name: &str, args: &[Value]) -> Result<Option<Value>> {
    // FormCalc built-in names are case-insensitive
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        // --- Arithmetic ---
        "abs" => ok_some(builtin_abs(args)?),
        "avg" => ok_some(builtin_avg(args)?),
        "ceil" => ok_some(builtin_ceil(args)?),
        "count" => ok_some(builtin_count(args)?),
        "floor" => ok_some(builtin_floor(args)?),
        "max" => ok_some(builtin_max(args)?),
        "min" => ok_some(builtin_min(args)?),
        "mod" => ok_some(builtin_mod(args)?),
        "round" => ok_some(builtin_round(args)?),
        "sum" => ok_some(builtin_sum(args)?),

        // --- String ---
        "at" => ok_some(builtin_at(args)?),
        "concat" => ok_some(builtin_concat(args)?),
        "left" => ok_some(builtin_left(args)?),
        "len" => ok_some(builtin_len(args)?),
        "lower" => ok_some(builtin_lower(args)?),
        "ltrim" => ok_some(builtin_ltrim(args)?),
        "replace" => ok_some(builtin_replace(args)?),
        "right" => ok_some(builtin_right(args)?),
        "rtrim" => ok_some(builtin_rtrim(args)?),
        "space" => ok_some(builtin_space(args)?),
        "stuff" => ok_some(builtin_stuff(args)?),
        "substr" => ok_some(builtin_substr(args)?),
        "upper" => ok_some(builtin_upper(args)?),
        "uuid" => ok_some(builtin_uuid(args)?),
        "wordnum" => ok_some(builtin_wordnum(args)?),

        // --- Logical ---
        "choose" => ok_some(builtin_choose(args)?),
        "if" => ok_some(builtin_if(args)?),
        "oneof" => ok_some(builtin_oneof(args)?),
        "within" => ok_some(builtin_within(args)?),

        // --- Misc ---
        "hasvalue" => ok_some(builtin_hasvalue(args)?),
        "null" => ok_some(Value::Null),

        _ => Ok(None),
    }
}

fn ok_some(v: Value) -> Result<Option<Value>> {
    Ok(Some(v))
}

fn arity(name: &str, args: &[Value], expected: usize) -> Result<()> {
    if args.len() != expected {
        return Err(FormCalcError::ArityError {
            name: name.to_string(),
            expected: expected.to_string(),
            got: args.len(),
        });
    }
    Ok(())
}

fn arity_min(name: &str, args: &[Value], min: usize) -> Result<()> {
    if args.len() < min {
        return Err(FormCalcError::ArityError {
            name: name.to_string(),
            expected: format!("at least {min}"),
            got: args.len(),
        });
    }
    Ok(())
}

// ============================================================
// Arithmetic
// ============================================================

fn builtin_abs(args: &[Value]) -> Result<Value> {
    arity("Abs", args, 1)?;
    Ok(Value::Number(args[0].to_number().abs()))
}

fn builtin_avg(args: &[Value]) -> Result<Value> {
    arity_min("Avg", args, 1)?;
    let sum: f64 = args.iter().map(|a| a.to_number()).sum();
    Ok(Value::Number(sum / args.len() as f64))
}

fn builtin_ceil(args: &[Value]) -> Result<Value> {
    arity("Ceil", args, 1)?;
    Ok(Value::Number(args[0].to_number().ceil()))
}

fn builtin_count(args: &[Value]) -> Result<Value> {
    Ok(Value::Number(args.len() as f64))
}

fn builtin_floor(args: &[Value]) -> Result<Value> {
    arity("Floor", args, 1)?;
    Ok(Value::Number(args[0].to_number().floor()))
}

fn builtin_max(args: &[Value]) -> Result<Value> {
    arity_min("Max", args, 1)?;
    let mut max = args[0].to_number();
    for arg in &args[1..] {
        let n = arg.to_number();
        if n > max {
            max = n;
        }
    }
    Ok(Value::Number(max))
}

fn builtin_min(args: &[Value]) -> Result<Value> {
    arity_min("Min", args, 1)?;
    let mut min = args[0].to_number();
    for arg in &args[1..] {
        let n = arg.to_number();
        if n < min {
            min = n;
        }
    }
    Ok(Value::Number(min))
}

fn builtin_mod(args: &[Value]) -> Result<Value> {
    arity("Mod", args, 2)?;
    let divisor = args[1].to_number();
    if divisor == 0.0 {
        return Err(FormCalcError::DivisionByZero);
    }
    Ok(Value::Number(args[0].to_number() % divisor))
}

fn builtin_round(args: &[Value]) -> Result<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "Round".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let n = args[0].to_number();
    let decimals = if args.len() == 2 {
        args[1].to_number() as i32
    } else {
        0
    };
    let factor = 10_f64.powi(decimals);
    Ok(Value::Number((n * factor).round() / factor))
}

fn builtin_sum(args: &[Value]) -> Result<Value> {
    arity_min("Sum", args, 1)?;
    let sum: f64 = args.iter().map(|a| a.to_number()).sum();
    Ok(Value::Number(sum))
}

// ============================================================
// String
// ============================================================

fn builtin_at(args: &[Value]) -> Result<Value> {
    arity("At", args, 2)?;
    let haystack = args[0].to_string_val();
    let needle = args[1].to_string_val();
    match haystack.find(&needle) {
        Some(pos) => Ok(Value::Number((pos + 1) as f64)), // 1-based
        None => Ok(Value::Number(0.0)),
    }
}

fn builtin_concat(args: &[Value]) -> Result<Value> {
    let mut result = String::new();
    for arg in args {
        result.push_str(&arg.to_string_val());
    }
    Ok(Value::String(result))
}

fn builtin_left(args: &[Value]) -> Result<Value> {
    arity("Left", args, 2)?;
    let s = args[0].to_string_val();
    let n = args[1].to_number() as usize;
    let result: String = s.chars().take(n).collect();
    Ok(Value::String(result))
}

fn builtin_len(args: &[Value]) -> Result<Value> {
    arity("Len", args, 1)?;
    Ok(Value::Number(args[0].to_string_val().len() as f64))
}

fn builtin_lower(args: &[Value]) -> Result<Value> {
    arity("Lower", args, 1)?;
    Ok(Value::String(args[0].to_string_val().to_lowercase()))
}

fn builtin_ltrim(args: &[Value]) -> Result<Value> {
    arity("Ltrim", args, 1)?;
    Ok(Value::String(
        args[0].to_string_val().trim_start().to_string(),
    ))
}

fn builtin_replace(args: &[Value]) -> Result<Value> {
    arity("Replace", args, 3)?;
    let s = args[0].to_string_val();
    let from = args[1].to_string_val();
    let to = args[2].to_string_val();
    Ok(Value::String(s.replace(&from, &to)))
}

fn builtin_right(args: &[Value]) -> Result<Value> {
    arity("Right", args, 2)?;
    let s = args[0].to_string_val();
    let n = args[1].to_number() as usize;
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    Ok(Value::String(chars[start..].iter().collect()))
}

fn builtin_rtrim(args: &[Value]) -> Result<Value> {
    arity("Rtrim", args, 1)?;
    Ok(Value::String(
        args[0].to_string_val().trim_end().to_string(),
    ))
}

fn builtin_space(args: &[Value]) -> Result<Value> {
    arity("Space", args, 1)?;
    let n = args[0].to_number() as usize;
    Ok(Value::String(" ".repeat(n)))
}

fn builtin_stuff(args: &[Value]) -> Result<Value> {
    arity("Stuff", args, 4)?;
    let s = args[0].to_string_val();
    let start = (args[1].to_number() as usize).saturating_sub(1); // 1-based to 0-based
    let delete_len = args[2].to_number() as usize;
    let insert = args[3].to_string_val();

    let chars: Vec<char> = s.chars().collect();
    let end = (start + delete_len).min(chars.len());
    let mut result: String = chars[..start].iter().collect();
    result.push_str(&insert);
    result.extend(chars[end..].iter());
    Ok(Value::String(result))
}

fn builtin_substr(args: &[Value]) -> Result<Value> {
    arity("Substr", args, 3)?;
    let s = args[0].to_string_val();
    let start = (args[1].to_number() as usize).saturating_sub(1); // 1-based to 0-based
    let len = args[2].to_number() as usize;
    let result: String = s.chars().skip(start).take(len).collect();
    Ok(Value::String(result))
}

fn builtin_upper(args: &[Value]) -> Result<Value> {
    arity("Upper", args, 1)?;
    Ok(Value::String(args[0].to_string_val().to_uppercase()))
}

fn builtin_uuid(args: &[Value]) -> Result<Value> {
    // Simple UUID v4-like generation without external crate
    if !args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Uuid".to_string(),
            expected: "0".to_string(),
            got: args.len(),
        });
    }
    // Return a placeholder — real UUID requires randomness
    Ok(Value::String(
        "00000000-0000-4000-8000-000000000000".to_string(),
    ))
}

fn builtin_wordnum(args: &[Value]) -> Result<Value> {
    // Simplified: convert number to English words for integers
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "WordNum".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let n = args[0].to_number() as i64;
    Ok(Value::String(number_to_words(n)))
}

// ============================================================
// Logical
// ============================================================

fn builtin_choose(args: &[Value]) -> Result<Value> {
    arity_min("Choose", args, 2)?;
    let idx = args[0].to_number() as usize;
    if idx == 0 || idx >= args.len() {
        return Ok(Value::Null);
    }
    Ok(args[idx].clone())
}

fn builtin_if(args: &[Value]) -> Result<Value> {
    arity("If", args, 3)?;
    if args[0].to_bool() {
        Ok(args[1].clone())
    } else {
        Ok(args[2].clone())
    }
}

fn builtin_oneof(args: &[Value]) -> Result<Value> {
    arity_min("Oneof", args, 2)?;
    let target = &args[0];
    for arg in &args[1..] {
        if target == arg {
            return Ok(Value::Number(1.0));
        }
    }
    Ok(Value::Number(0.0))
}

fn builtin_within(args: &[Value]) -> Result<Value> {
    arity("Within", args, 3)?;
    let val = args[0].to_number();
    let low = args[1].to_number();
    let high = args[2].to_number();
    Ok(Value::Number(if val >= low && val <= high {
        1.0
    } else {
        0.0
    }))
}

// ============================================================
// Misc
// ============================================================

fn builtin_hasvalue(args: &[Value]) -> Result<Value> {
    arity("HasValue", args, 1)?;
    let has = match &args[0] {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(_) => true,
    };
    Ok(Value::Number(if has { 1.0 } else { 0.0 }))
}

// ============================================================
// Helpers
// ============================================================

fn number_to_words(n: i64) -> String {
    if n == 0 {
        return "Zero".to_string();
    }

    let is_negative = n < 0;
    let n = n.unsigned_abs();

    let ones = [
        "",
        "One",
        "Two",
        "Three",
        "Four",
        "Five",
        "Six",
        "Seven",
        "Eight",
        "Nine",
        "Ten",
        "Eleven",
        "Twelve",
        "Thirteen",
        "Fourteen",
        "Fifteen",
        "Sixteen",
        "Seventeen",
        "Eighteen",
        "Nineteen",
    ];
    let tens = [
        "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
    ];

    fn chunk_to_words(n: u64, ones: &[&str], tens: &[&str]) -> String {
        if n == 0 {
            return String::new();
        }
        if n < 20 {
            return ones[n as usize].to_string();
        }
        if n < 100 {
            let t = tens[(n / 10) as usize].to_string();
            let o = chunk_to_words(n % 10, ones, tens);
            return if o.is_empty() { t } else { format!("{t} {o}") };
        }
        let h = format!("{} Hundred", ones[(n / 100) as usize]);
        let rest = chunk_to_words(n % 100, ones, tens);
        if rest.is_empty() {
            h
        } else {
            format!("{h} {rest}")
        }
    }

    let scales = ["", "Thousand", "Million", "Billion", "Trillion"];
    let mut parts = Vec::new();
    let mut remaining = n;
    let mut scale_idx = 0;

    while remaining > 0 {
        let chunk = remaining % 1000;
        if chunk != 0 {
            let words = chunk_to_words(chunk, &ones, &tens);
            if scales[scale_idx].is_empty() {
                parts.push(words);
            } else {
                parts.push(format!("{} {}", words, scales[scale_idx]));
            }
        }
        remaining /= 1000;
        scale_idx += 1;
    }

    parts.reverse();
    let result = parts.join(" ");
    if is_negative {
        format!("Negative {result}")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abs() {
        let r = call_builtin("Abs", &[Value::Number(-5.0)]).unwrap();
        assert_eq!(r, Some(Value::Number(5.0)));
    }

    #[test]
    fn test_sum() {
        let r = call_builtin(
            "Sum",
            &[Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)],
        )
        .unwrap();
        assert_eq!(r, Some(Value::Number(6.0)));
    }

    #[test]
    fn test_round() {
        let r = call_builtin("Round", &[Value::Number(3.456), Value::Number(2.0)]).unwrap();
        assert_eq!(r, Some(Value::Number(3.46)));
    }

    #[test]
    fn test_len() {
        let r = call_builtin("Len", &[Value::String("hello".to_string())]).unwrap();
        assert_eq!(r, Some(Value::Number(5.0)));
    }

    #[test]
    fn test_substr() {
        let r = call_builtin(
            "Substr",
            &[
                Value::String("hello world".to_string()),
                Value::Number(7.0),
                Value::Number(5.0),
            ],
        )
        .unwrap();
        assert_eq!(r, Some(Value::String("world".to_string())));
    }

    #[test]
    fn test_if_builtin() {
        let r = call_builtin(
            "If",
            &[Value::Number(1.0), Value::Number(42.0), Value::Number(99.0)],
        )
        .unwrap();
        assert_eq!(r, Some(Value::Number(42.0)));
    }

    #[test]
    fn test_unknown_returns_none() {
        let r = call_builtin("NoSuchFunc", &[]).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn test_number_to_words() {
        assert_eq!(number_to_words(0), "Zero");
        assert_eq!(number_to_words(42), "Forty Two");
        assert_eq!(number_to_words(100), "One Hundred");
        assert_eq!(
            number_to_words(1234),
            "One Thousand Two Hundred Thirty Four"
        );
    }

    #[test]
    fn test_case_insensitive() {
        let r1 = call_builtin("abs", &[Value::Number(-1.0)]).unwrap();
        let r2 = call_builtin("ABS", &[Value::Number(-1.0)]).unwrap();
        let r3 = call_builtin("Abs", &[Value::Number(-1.0)]).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }
}
