//! FormCalc built-in functions.
//!
//! Implements the currently supported subset of the XFA 3.3 §25 built-in library.
//! Functions are case-insensitive (caller normalizes via lookup).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::error::{FormCalcError, Result};
use crate::value::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
        "decode" => ok_some(builtin_decode(args)?),
        "encode" => ok_some(builtin_encode(args)?),
        "format" => ok_some(builtin_format(args)?),
        "left" => ok_some(builtin_left(args)?),
        "len" => ok_some(builtin_len(args)?),
        "lower" => ok_some(builtin_lower(args)?),
        "ltrim" => ok_some(builtin_ltrim(args)?),
        "parse" => ok_some(builtin_parse(args)?),
        "replace" => ok_some(builtin_replace(args)?),
        "right" => ok_some(builtin_right(args)?),
        "rtrim" => ok_some(builtin_rtrim(args)?),
        "space" => ok_some(builtin_space(args)?),
        "str" => ok_some(builtin_str(args)?),
        "stuff" => ok_some(builtin_stuff(args)?),
        "substr" => ok_some(builtin_substr(args)?),
        "unittype" => ok_some(builtin_unit_type(args)?),
        "unitvalue" => ok_some(builtin_unit_value(args)?),
        "upper" => ok_some(builtin_upper(args)?),
        "uuid" => ok_some(builtin_uuid(args)?),
        "wordnum" => ok_some(builtin_wordnum(args)?),

        // --- Logical ---
        "choose" => ok_some(builtin_choose(args)?),
        "exists" => ok_some(builtin_exists(args)?),
        "if" => ok_some(builtin_if(args)?),
        "oneof" => ok_some(builtin_oneof(args)?),
        "within" => ok_some(builtin_within(args)?),

        // --- Date/Time ---
        "date" => ok_some(builtin_date(args)?),
        "date2num" => ok_some(builtin_date2num(args)?),
        "datefmt" => ok_some(builtin_datefmt(args)?),
        "dategmt" => ok_some(builtin_date(args)?), // alias
        "isodate2num" => ok_some(builtin_isodate2num(args)?),
        "isotime2num" => ok_some(builtin_isotime2num(args)?),
        "localdatefmt" => ok_some(builtin_localdatefmt(args)?),
        "localtimefmt" => ok_some(builtin_localtimefmt(args)?),
        "num2date" => ok_some(builtin_num2date(args)?),
        "num2gmtime" => ok_some(builtin_num2gmtime(args)?),
        "time" => ok_some(builtin_time(args)?),
        "time2num" => ok_some(builtin_time2num(args)?),
        "timegmt" => ok_some(builtin_time(args)?), // alias
        "timefmt" => ok_some(builtin_timefmt(args)?),
        "num2time" => ok_some(builtin_num2time(args)?),

        // --- Financial ---
        "apr" => ok_some(builtin_apr(args)?),
        "cterm" => ok_some(builtin_cterm(args)?),
        "fv" => ok_some(builtin_fv(args)?),
        "ipmt" => ok_some(builtin_ipmt(args)?),
        "npv" => ok_some(builtin_npv(args)?),
        "pmt" => ok_some(builtin_pmt(args)?),
        "ppmt" => ok_some(builtin_ppmt(args)?),
        "pv" => ok_some(builtin_pv(args)?),
        "rate" => ok_some(builtin_rate(args)?),
        "term" => ok_some(builtin_term(args)?),

        // --- Misc ---
        "eval" => ok_some(builtin_eval(args)?),
        "hasvalue" => ok_some(builtin_hasvalue(args)?),
        "null" => ok_some(Value::Null),
        "ref" => ok_some(builtin_ref(args)?),
        "get" => ok_some(builtin_get(args)?),
        "post" => ok_some(builtin_post(args)?),
        "put" => ok_some(builtin_put(args)?),

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

fn arity_range(name: &str, args: &[Value], min: usize, max: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        return Err(FormCalcError::ArityError {
            name: name.to_string(),
            expected: format!("{min} to {max}"),
            got: args.len(),
        });
    }
    Ok(())
}

fn any_null(args: &[Value]) -> bool {
    args.iter().any(Value::is_null)
}

fn all_null(args: &[Value]) -> bool {
    args.iter().all(Value::is_null)
}

fn todo_builtin(name: &str, section: &str, page: u16, signature: &str) -> Result<Value> {
    Err(FormCalcError::RuntimeError(format!(
        "XFA Spec 3.3 {section} (p{page}) TODO: {name}{signature} is not fully implemented"
    )))
}

fn clamp_string_start(s: &str, one_based_start: i64) -> usize {
    if s.is_empty() || one_based_start <= 1 {
        0
    } else {
        let len = s.chars().count();
        usize::min((one_based_start - 1) as usize, len.saturating_sub(1))
    }
}

fn parse_style_arg(arg: Option<&Value>) -> i32 {
    arg.map_or(0, |value| value.to_number() as i32)
}

// ============================================================
// Arithmetic
// ============================================================

// XFA Spec 3.3 §25.3 "Abs" (p1081) — Abs(n1)
// Returns the absolute value or null when n1 is null.
fn builtin_abs(args: &[Value]) -> Result<Value> {
    arity("Abs", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::Number(args[0].to_number().abs()))
    }
}

// XFA Spec 3.3 §25.3 "Avg" (p1082) — Avg(n1 [, n2...])
// Averages only non-null values and returns null when all arguments are null.
fn builtin_avg(args: &[Value]) -> Result<Value> {
    arity_min("Avg", args, 1)?;
    let values: Vec<f64> = args
        .iter()
        .filter(|arg| !arg.is_null())
        .map(Value::to_number)
        .collect();
    if values.is_empty() {
        Ok(Value::Null)
    } else {
        let sum: f64 = values.iter().sum();
        Ok(Value::Number(sum / values.len() as f64))
    }
}

fn builtin_ceil(args: &[Value]) -> Result<Value> {
    arity("Ceil", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::Number(args[0].to_number().ceil()))
    }
}

fn builtin_count(args: &[Value]) -> Result<Value> {
    Ok(Value::Number(
        args.iter().filter(|arg| !arg.is_null()).count() as f64,
    ))
}

fn builtin_floor(args: &[Value]) -> Result<Value> {
    arity("Floor", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::Number(args[0].to_number().floor()))
    }
}

fn builtin_max(args: &[Value]) -> Result<Value> {
    arity_min("Max", args, 1)?;
    let mut values = args
        .iter()
        .filter(|arg| !arg.is_null())
        .map(Value::to_number);
    let Some(mut max) = values.next() else {
        return Ok(Value::Null);
    };
    for n in values {
        if n > max {
            max = n;
        }
    }
    Ok(Value::Number(max))
}

fn builtin_min(args: &[Value]) -> Result<Value> {
    arity_min("Min", args, 1)?;
    let mut values = args
        .iter()
        .filter(|arg| !arg.is_null())
        .map(Value::to_number);
    let Some(mut min) = values.next() else {
        return Ok(Value::Null);
    };
    for n in values {
        if n < min {
            min = n;
        }
    }
    Ok(Value::Number(min))
}

fn builtin_mod(args: &[Value]) -> Result<Value> {
    arity("Mod", args, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let divisor = args[1].to_number();
    if divisor == 0.0 {
        return Err(FormCalcError::DivisionByZero);
    }
    Ok(Value::Number(args[0].to_number() % divisor))
}

fn builtin_round(args: &[Value]) -> Result<Value> {
    arity_range("Round", args, 1, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let n = args[0].to_number();
    let decimals = args
        .get(1)
        .map_or(0, |value| value.to_number() as i32)
        .clamp(0, 12);
    let factor = 10_f64.powi(decimals);
    Ok(Value::Number((n * factor).round() / factor))
}

fn builtin_sum(args: &[Value]) -> Result<Value> {
    arity_min("Sum", args, 1)?;
    let mut seen = false;
    let sum: f64 = args
        .iter()
        .filter(|arg| !arg.is_null())
        .map(|arg| {
            seen = true;
            arg.to_number()
        })
        .sum();
    if seen {
        Ok(Value::Number(sum))
    } else {
        Ok(Value::Null)
    }
}

// ============================================================
// String
// ============================================================

fn builtin_at(args: &[Value]) -> Result<Value> {
    arity("At", args, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let haystack = args[0].to_string_val();
    let needle = args[1].to_string_val();
    if needle.is_empty() {
        return Ok(Value::Number(1.0));
    }
    match haystack.find(&needle) {
        Some(pos) => Ok(Value::Number((pos + 1) as f64)), // 1-based
        None => Ok(Value::Number(0.0)),
    }
}

// XFA Spec 3.3 §25.7 "Concat" (p1122) — Concat(s1, s2, ...)
// Returns the concatenation of all arguments converted to strings.
// Takes 1 or more string parameters. Null arguments are treated as "".
fn builtin_concat(args: &[Value]) -> Result<Value> {
    arity_min("Concat", args, 1)?;
    if all_null(args) {
        return Ok(Value::Null);
    }
    let mut result = String::new();
    for arg in args {
        result.push_str(&arg.to_string_val());
    }
    Ok(Value::String(result))
}

// XFA Spec 3.3 §25.7 "Decode" (p1123) — Decode(s1 [, s2])
// Decode s1 using encoding s2 ("url", "html", "xml").  Default = "url".
fn builtin_decode(args: &[Value]) -> Result<Value> {
    arity_range("Decode", args, 1, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let enc = args
        .get(1)
        .map(|v| v.to_string_val())
        .unwrap_or_else(|| "url".to_string());
    match enc.to_ascii_lowercase().as_str() {
        "url" => Ok(Value::String(decode_url(&s))),
        "html" | "xml" => Ok(Value::String(decode_xml_html(&s))),
        _ => Ok(Value::String(s)),
    }
}

// XFA Spec 3.3 §25.7 "Encode" (p1124) — Encode(s1 [, s2])
// Encode s1 using encoding s2 ("url", "html", "xml").  Default = "url".
fn builtin_encode(args: &[Value]) -> Result<Value> {
    arity_range("Encode", args, 1, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let enc = args
        .get(1)
        .map(|v| v.to_string_val())
        .unwrap_or_else(|| "url".to_string());
    match enc.to_ascii_lowercase().as_str() {
        "url" => Ok(Value::String(encode_url(&s))),
        "html" => Ok(Value::String(encode_html(&s))),
        "xml" => Ok(Value::String(encode_xml(&s))),
        _ => Ok(Value::String(s)),
    }
}

// XFA Spec 3.3 §25.7 "Format" (p1125) — Format(s1, s2)
// Format value s2 using picture clause s1.
fn builtin_format(args: &[Value]) -> Result<Value> {
    arity_range("Format", args, 2, 10)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    let picture = args[0].to_string_val();
    let value = args[1].to_string_val();
    Ok(Value::String(format_picture(&picture, &value)))
}

// ---- Decode/Encode helpers ----

fn decode_url(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            result.push(b' ');
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn encode_url(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

fn decode_xml_html(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            for ec in chars.by_ref() {
                if ec == ';' {
                    break;
                }
                entity.push(ec);
            }
            match entity.as_str() {
                "amp" => result.push('&'),
                "lt" => result.push('<'),
                "gt" => result.push('>'),
                "apos" => result.push('\''),
                "quot" => result.push('"'),
                "nbsp" => result.push('\u{00A0}'),
                _ if entity.starts_with('#') => {
                    let code = if entity.starts_with("#x") || entity.starts_with("#X") {
                        u32::from_str_radix(&entity[2..], 16).ok()
                    } else {
                        entity[1..].parse::<u32>().ok()
                    };
                    if let Some(c) = code.and_then(char::from_u32) {
                        result.push(c);
                    } else {
                        result.push('&');
                        result.push_str(&entity);
                        result.push(';');
                    }
                }
                _ => {
                    result.push('&');
                    result.push_str(&entity);
                    result.push(';');
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn encode_html(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

fn encode_xml(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\'' => result.push_str("&apos;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

// ---- Format helper ----

/// Format a value according to a FormCalc picture clause.
/// Supports `num{...}`, `date{...}`, `text{...}` patterns.
fn format_picture(picture: &str, value: &str) -> String {
    // Extract category and pattern from "category{pattern}"
    if let Some(inner) = extract_picture_body(picture, "num") {
        format_num(inner, value)
    } else if let Some(inner) = extract_picture_body(picture, "date") {
        format_date_picture(inner, value)
    } else if let Some(inner) = extract_picture_body(picture, "text") {
        format_text(inner, value)
    } else {
        // Fallback: return original value
        value.to_string()
    }
}

fn extract_picture_body<'a>(picture: &'a str, category: &str) -> Option<&'a str> {
    let lower = picture.to_ascii_lowercase();
    let prefix = format!("{}{{", category);
    if lower.starts_with(&prefix) && picture.ends_with('}') {
        Some(&picture[prefix.len()..picture.len() - 1])
    } else {
        None
    }
}

/// Numeric formatting: z = suppress leading zero, 9 = always show digit.
fn format_num(pattern: &str, value: &str) -> String {
    let num: f64 = value.parse().unwrap_or(0.0);
    let is_neg = num < 0.0;
    let abs_num = num.abs();

    // Count integer and decimal pattern digits
    let (int_pat, dec_pat) = if let Some(dot_pos) = pattern.find('.') {
        (&pattern[..dot_pos], Some(&pattern[dot_pos + 1..]))
    } else {
        (pattern, None)
    };

    let dec_digits = dec_pat.map_or(0, |p| {
        p.chars()
            .filter(|&c| c == '9' || c == 'z' || c == 'Z')
            .count()
    });
    let rounded = if dec_digits > 0 {
        let factor = 10f64.powi(dec_digits as i32);
        (abs_num * factor).round() / factor
    } else {
        abs_num.round()
    };

    let int_part = rounded.trunc() as u64;
    let dec_part = ((rounded.fract() * 10f64.powi(dec_digits as i32)).round()) as u64;

    let int_str = int_part.to_string();
    let int_pat_clean: String = int_pat
        .chars()
        .filter(|&c| c == '9' || c == 'z' || c == 'Z')
        .collect();
    let int_width = int_pat_clean.len().max(int_str.len());

    // Build integer portion
    let padded_int = format!("{:0>width$}", int_str, width = int_width);
    let mut int_result = String::new();
    let suppress_zeros = int_pat_clean.starts_with(['z', 'Z']);

    let mut leading = true;
    let pat_chars: Vec<char> = int_pat_clean.chars().collect();
    let padded_chars: Vec<char> = padded_int.chars().collect();
    let offset = padded_chars.len().saturating_sub(pat_chars.len());

    for (i, &pc) in padded_chars.iter().enumerate() {
        let pat_idx = i.saturating_sub(offset);
        let is_z =
            pat_idx < pat_chars.len() && (pat_chars[pat_idx] == 'z' || pat_chars[pat_idx] == 'Z');

        if leading && pc == '0' && suppress_zeros && is_z {
            int_result.push(' ');
        } else {
            leading = false;
            int_result.push(pc);
        }
    }

    // Re-inject separators from the original pattern (commas, etc.)
    let mut formatted = String::new();
    let mut result_idx = 0;
    let result_chars: Vec<char> = int_result.chars().collect();
    for ch in int_pat.chars() {
        if ch == '9' || ch == 'z' || ch == 'Z' {
            if result_idx < result_chars.len() {
                formatted.push(result_chars[result_idx]);
                result_idx += 1;
            }
        } else {
            formatted.push(ch);
        }
    }
    while result_idx < result_chars.len() {
        formatted.push(result_chars[result_idx]);
        result_idx += 1;
    }

    // Add decimal part
    if let Some(dp) = dec_pat {
        formatted.push('.');
        let dec_str = format!("{:0>width$}", dec_part, width = dec_digits);
        let mut dec_idx = 0;
        for ch in dp.chars() {
            if ch == '9' || ch == 'z' || ch == 'Z' {
                if dec_idx < dec_str.len() {
                    formatted.push(dec_str.as_bytes()[dec_idx] as char);
                    dec_idx += 1;
                }
            } else {
                formatted.push(ch);
            }
        }
    }

    if is_neg {
        format!("-{}", formatted.trim_start())
    } else {
        formatted
    }
}

/// Date formatting: YYYY, MM, DD, MMM, etc.
fn format_date_picture(pattern: &str, value: &str) -> String {
    // Try to parse value as days-since-1900 (number) or ISO date
    let (y, m, d) = if let Ok(days) = value.parse::<f64>() {
        days_to_date(days as i64)
    } else if value.len() >= 10 && value.as_bytes()[4] == b'-' {
        // ISO date: YYYY-MM-DD
        let parts: Vec<&str> = value.split('-').collect();
        if parts.len() >= 3 {
            (
                parts[0].parse::<i32>().unwrap_or(2000),
                parts[1].parse::<u32>().unwrap_or(1),
                parts[2].parse::<u32>().unwrap_or(1),
            )
        } else {
            return value.to_string();
        }
    } else {
        return value.to_string();
    };

    let month_names = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_full = [
        "",
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    let mut result = pattern.to_string();
    result = result.replace("YYYY", &format!("{:04}", y));
    result = result.replace("YY", &format!("{:02}", y % 100));
    if result.contains("MMMM") {
        result = result.replace("MMMM", month_full.get(m as usize).unwrap_or(&""));
    } else if result.contains("MMM") {
        result = result.replace("MMM", month_names.get(m as usize).unwrap_or(&""));
    } else {
        result = result.replace("MM", &format!("{:02}", m));
    }
    result = result.replace("DD", &format!("{:02}", d));
    result
}

/// Text formatting: each A/X/9 in pattern accepts one character.
fn format_text(pattern: &str, value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    for p in pattern.chars() {
        match p {
            'A' | 'X' | 'O' | '9' | '0' => {
                if let Some(c) = chars.next() {
                    result.push(c);
                }
            }
            _ => result.push(p),
        }
    }
    result
}

fn builtin_left(args: &[Value]) -> Result<Value> {
    arity("Left", args, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let n = args[1].to_number() as i64;
    if n <= 0 {
        Ok(Value::String(String::new()))
    } else {
        let result: String = s.chars().take(n as usize).collect();
        Ok(Value::String(result))
    }
}

fn builtin_len(args: &[Value]) -> Result<Value> {
    arity("Len", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::Number(args[0].to_string_val().chars().count() as f64))
    }
}

fn builtin_lower(args: &[Value]) -> Result<Value> {
    arity_range("Lower", args, 1, 2)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::String(args[0].to_string_val().to_lowercase()))
    }
}

fn builtin_ltrim(args: &[Value]) -> Result<Value> {
    arity("Ltrim", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::String(
            args[0].to_string_val().trim_start().to_string(),
        ))
    }
}

fn builtin_parse(_args: &[Value]) -> Result<Value> {
    todo_builtin("Parse", "§25.7", 1131, "(s1, s2)")
}

fn builtin_replace(args: &[Value]) -> Result<Value> {
    arity_range("Replace", args, 2, 3)?;
    if args[0].is_null() || args[1].is_null() {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let from = args[1].to_string_val();
    let to = args.get(2).map_or_else(String::new, Value::to_string_val);
    Ok(Value::String(s.replace(&from, &to)))
}

fn builtin_right(args: &[Value]) -> Result<Value> {
    arity("Right", args, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let n = args[1].to_number() as i64;
    if n <= 0 {
        return Ok(Value::String(String::new()));
    }
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n as usize);
    Ok(Value::String(chars[start..].iter().collect()))
}

fn builtin_rtrim(args: &[Value]) -> Result<Value> {
    arity("Rtrim", args, 1)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::String(
            args[0].to_string_val().trim_end().to_string(),
        ))
    }
}

fn builtin_space(args: &[Value]) -> Result<Value> {
    arity("Space", args, 1)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    let n = (args[0].to_number() as i64).max(0) as usize;
    Ok(Value::String(" ".repeat(n)))
}

// XFA Spec 3.3 §25.7 "Str" (p1136) — Str(n1 [, n2 [, n3]])
// Formats a number into a fixed-width ASCII string using '.' as radix.
fn builtin_str(args: &[Value]) -> Result<Value> {
    arity_range("Str", args, 1, 3)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }

    let value = args[0].to_number();
    let width = args.get(1).map_or(10usize, |arg| {
        usize::try_from((arg.to_number() as i64).max(0)).unwrap_or(0)
    });
    let precision = args
        .get(2)
        .map_or(0, |arg| (arg.to_number() as i32).max(0))
        .min(12);
    let factor = 10_f64.powi(precision);
    let rounded = (value * factor).round() / factor;
    let rendered = if precision == 0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.prec$}", prec = precision as usize)
    };

    if rendered.len() > width {
        Ok(Value::String("*".repeat(width)))
    } else if width > rendered.len() {
        Ok(Value::String(format!(
            "{}{}",
            " ".repeat(width - rendered.len()),
            rendered
        )))
    } else {
        Ok(Value::String(rendered))
    }
}

fn builtin_stuff(args: &[Value]) -> Result<Value> {
    arity_range("Stuff", args, 3, 4)?;
    if args[0].is_null() || args[1].is_null() || args[2].is_null() {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let start = clamp_string_start(&s, args[1].to_number() as i64);
    let delete_len = (args[2].to_number() as i64).max(0) as usize;
    let insert = args.get(3).map_or_else(String::new, Value::to_string_val);

    let chars: Vec<char> = s.chars().collect();
    let end = (start + delete_len).min(chars.len());
    let mut result: String = chars[..start].iter().collect();
    result.push_str(&insert);
    result.extend(chars[end..].iter());
    Ok(Value::String(result))
}

fn builtin_substr(args: &[Value]) -> Result<Value> {
    arity("Substr", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let s = args[0].to_string_val();
    let start = clamp_string_start(&s, args[1].to_number() as i64);
    let len = (args[2].to_number() as i64).max(0) as usize;
    let result: String = s.chars().skip(start).take(len).collect();
    Ok(Value::String(result))
}

fn builtin_upper(args: &[Value]) -> Result<Value> {
    arity_range("Upper", args, 1, 2)?;
    if args[0].is_null() {
        Ok(Value::Null)
    } else {
        Ok(Value::String(args[0].to_string_val().to_uppercase()))
    }
}

fn builtin_uuid(args: &[Value]) -> Result<Value> {
    arity_range("Uuid", args, 0, 1)?;
    let format_style = args.first().map_or(0, |value| value.to_number() as i32);
    let ticks = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| FormCalcError::RuntimeError(format!("system clock error: {e}")))?
        .as_nanos();
    let counter = UUID_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let raw = ticks ^ (counter << 64) ^ 0xa3ac_0000_3dde_f352_96c4_00a0_c9c8_6dd5_u128;
    let hex = format!("{raw:032x}");
    if format_style == 1 {
        Ok(Value::String(format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )))
    } else {
        Ok(Value::String(hex))
    }
}

fn builtin_wordnum(args: &[Value]) -> Result<Value> {
    arity_range("WordNum", args, 1, 3)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    let value = match &args[0] {
        Value::Number(n) => *n,
        Value::String(s) => match s.trim().parse::<f64>() {
            Ok(n) => n,
            Err(_) => return Ok(Value::String("*".to_string())),
        },
        Value::Null => return Ok(Value::Null),
    };
    let option = args.get(1).map_or(0, |value| value.to_number() as i32);
    let whole = value.trunc();
    if !(0.0..=922_337_203_685_477_550.0).contains(&whole) {
        return Ok(Value::String("*".to_string()));
    }

    let whole_words = number_to_words(whole as i64);
    let rendered = match option {
        1 => format!("{whole_words} Dollars"),
        2 => {
            let cents = ((value.fract().abs() * 100.0).round() as i64).clamp(0, 99);
            format!("{whole_words} Dollars And {} Cents", number_to_words(cents))
        }
        _ => whole_words,
    };
    Ok(Value::String(rendered))
}

// ============================================================
// Logical
// ============================================================

fn builtin_choose(args: &[Value]) -> Result<Value> {
    arity_min("Choose", args, 2)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    let idx = args[0].to_number() as isize;
    if idx < 1 || idx as usize >= args.len() {
        return Ok(Value::String(String::new()));
    }
    Ok(args[idx as usize].clone())
}

fn builtin_exists(args: &[Value]) -> Result<Value> {
    arity("Exists", args, 1)?;
    Ok(Value::Number(0.0))
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
    if args[0].is_null() {
        return Ok(Value::Null);
    }

    let is_numeric = matches!(&args[0], Value::Number(_))
        || matches!(&args[0], Value::String(s) if s.trim().parse::<f64>().is_ok());

    let result = if is_numeric {
        let val = args[0].to_number();
        let low = args[1].to_number();
        let high = args[2].to_number();
        val >= low && val <= high
    } else {
        let val = args[0].to_string_val();
        let low = args[1].to_string_val();
        let high = args[2].to_string_val();
        val >= low && val <= high
    };

    Ok(Value::Number(if result { 1.0 } else { 0.0 }))
}

// ============================================================
// Date/Time
// ============================================================

/// Days from epoch (1900-01-01) to a given date.
fn date_to_days(year: i32, month: u32, day: u32) -> i64 {
    // Julian Day Number calculation, then offset to 1900-01-01 epoch
    let a = (14 - month as i64) / 12;
    let y = year as i64 + 4800 - a;
    let m = month as i64 + 12 * a - 3;
    let jdn = day as i64 + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    // JDN for 1900-01-01 is 2415021
    // Day 1 = 1900-01-01 per XFA spec
    jdn - 2415020
}

/// Convert days from epoch (1900-01-01) back to (year, month, day).
fn days_to_date(days: i64) -> (i32, u32, u32) {
    let jdn = days + 2415020;
    let a = jdn + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = (e - (153 * m + 2) / 5 + 1) as u32;
    let month = (m + 3 - 12 * (m / 10)) as u32;
    let year = (100 * b + d - 4800 + m / 10) as i32;
    (year, month, day)
}

fn builtin_date(args: &[Value]) -> Result<Value> {
    if !args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Date".to_string(),
            expected: "0".to_string(),
            got: args.len(),
        });
    }
    // XFA Spec 3.3 §25.4 "Date" (p1091) uses the current system date.
    // This implementation uses the current UTC date; prevailing-locale date
    // selection remains a TODO tracked in spec_review_cx_formcalc.md.
    let unix_days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| FormCalcError::RuntimeError(format!("system clock error: {e}")))?
        .as_secs()
        / 86_400;
    let days = date_to_days(1970, 1, 1) + unix_days as i64;
    Ok(Value::Number(days as f64))
}

fn builtin_date2num(args: &[Value]) -> Result<Value> {
    arity_range("Date2Num", args, 1, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let date_str = args[0].to_string_val();
    let format = if args.len() > 1 {
        args[1].to_string_val()
    } else {
        builtin_datefmt(&[])?.to_string_val()
    };

    let days = parse_date_string(&date_str, &format).unwrap_or(0);
    Ok(Value::Number(days as f64))
}

fn builtin_datefmt(args: &[Value]) -> Result<Value> {
    arity_range("DateFmt", args, 0, 2)?;
    let style = parse_style_arg(args.first());
    let format = match style {
        1 => "M/D/YY",
        2 | 0 => "MMM D, YYYY",
        3 => "MMMM D, YYYY",
        4 => "EEEE, MMMM D, YYYY",
        _ => "MMM D, YYYY",
    };
    Ok(Value::String(format.to_string()))
}

fn builtin_num2date(args: &[Value]) -> Result<Value> {
    arity_range("Num2Date", args, 1, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let days = args[0].to_number() as i64;
    let format = if args.len() > 1 {
        args[1].to_string_val()
    } else {
        builtin_datefmt(&[])?.to_string_val()
    };

    if days < 1 {
        return Ok(Value::String(String::new()));
    }
    let (y, m, d) = days_to_date(days);
    let result = format_date(y, m, d, &format);
    Ok(Value::String(result))
}

fn builtin_isodate2num(args: &[Value]) -> Result<Value> {
    arity("IsoDate2Num", args, 1)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    Ok(Value::Number(
        parse_iso_date_string(&args[0].to_string_val()).unwrap_or(0) as f64,
    ))
}

fn builtin_isotime2num(args: &[Value]) -> Result<Value> {
    arity("IsoTime2Num", args, 1)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    Ok(Value::Number(
        parse_time_string(&args[0].to_string_val())
            .map(|ms| ms + 1)
            .unwrap_or(0) as f64,
    ))
}

fn builtin_localdatefmt(args: &[Value]) -> Result<Value> {
    builtin_datefmt(args)
}

fn builtin_localtimefmt(args: &[Value]) -> Result<Value> {
    builtin_timefmt(args)
}

fn builtin_time(args: &[Value]) -> Result<Value> {
    if !args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Time".to_string(),
            expected: "0".to_string(),
            got: args.len(),
        });
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| FormCalcError::RuntimeError(format!("system clock error: {e}")))?
        .as_millis();
    Ok(Value::Number((millis % 86_400_000) as f64 + 1.0))
}

fn builtin_time2num(args: &[Value]) -> Result<Value> {
    arity_range("Time2Num", args, 1, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let time_str = args[0].to_string_val();
    let ms = parse_time_string(&time_str).map(|ms| ms + 1).unwrap_or(0);
    Ok(Value::Number(ms as f64))
}

fn builtin_timefmt(args: &[Value]) -> Result<Value> {
    arity_range("TimeFmt", args, 0, 2)?;
    let style = parse_style_arg(args.first());
    let format = match style {
        1 => "h:MM A",
        2 => "HH:MM:SS",
        3 => "HH:MM:SS Z",
        4 => "H.MM' Uhr 'Z",
        _ => "h:MM:SS A",
    };
    Ok(Value::String(format.to_string()))
}

fn builtin_num2gmtime(args: &[Value]) -> Result<Value> {
    builtin_num2time(args)
}

fn builtin_num2time(args: &[Value]) -> Result<Value> {
    arity_range("Num2Time", args, 1, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let ms = args[0].to_number() as i64;
    if ms < 1 {
        return Ok(Value::String(String::new()));
    }
    let secs = ((ms as u64 - 1) / 1000) % 86_400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let format = args.get(1).map_or_else(
        || builtin_timefmt(&[]).map(|v| v.to_string_val()),
        |v| Ok(v.to_string_val()),
    )?;
    let result = format_time_string(h, m, s, &format);
    Ok(Value::String(result))
}

fn parse_date_string(s: &str, format: &str) -> Option<i64> {
    let text = s.trim();
    if text.is_empty() {
        return None;
    }
    if format.contains("MMM") {
        return parse_named_month_date(text);
    }

    let parts: Vec<&str> = text.split(['-', '/', '.']).collect();
    if parts.len() != 3 {
        return parse_iso_date_string(text);
    }
    let (year, month, day) = if format.starts_with('Y') || parts[0].len() == 4 {
        (
            parts[0].parse::<i32>().ok()?,
            parts[1].parse::<u32>().ok()?,
            parts[2].parse::<u32>().ok()?,
        )
    } else if format.starts_with('D') {
        (
            normalize_year(parts[2].parse::<i32>().ok()?),
            parts[1].parse::<u32>().ok()?,
            parts[0].parse::<u32>().ok()?,
        )
    } else {
        (
            normalize_year(parts[2].parse::<i32>().ok()?),
            parts[0].parse::<u32>().ok()?,
            parts[1].parse::<u32>().ok()?,
        )
    };
    Some(date_to_days(year, month, day))
}

fn format_date(y: i32, m: u32, d: u32, format: &str) -> String {
    let month_short = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_long = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let weekday = weekday_name(date_to_days(y, m, d));

    format
        .replace("EEEE", weekday)
        .replace("MMMM", month_long[(m.saturating_sub(1)) as usize])
        .replace("MMM", month_short[(m.saturating_sub(1)) as usize])
        .replace("YYYY", &format!("{y:04}"))
        .replace("YY", &format!("{:02}", y.rem_euclid(100)))
        .replace("MM", &format!("{m:02}"))
        .replace("M", &m.to_string())
        .replace("DD", &format!("{d:02}"))
        .replace("D", &d.to_string())
}

fn parse_time_string(s: &str) -> Option<u64> {
    let mut text = s.trim();
    if let Some((_, rhs)) = text.rsplit_once('T') {
        text = rhs;
    }

    let mut tz_offset_minutes = 0i32;
    let mut upper = text.to_ascii_uppercase();
    if let Some(stripped) = upper.strip_suffix('Z') {
        upper = stripped.trim_end().to_string();
    } else if let Some((time, tz)) = upper.split_once(" GMT") {
        let time = time.trim().to_string();
        let tz = tz.trim().to_string();
        upper = time;
        tz_offset_minutes = parse_timezone_offset(&tz).unwrap_or(0);
    } else if let Some((time, offset)) = split_trailing_offset(&upper) {
        let time = time.to_string();
        let offset = offset.to_string();
        upper = time;
        tz_offset_minutes = parse_timezone_offset(&offset).unwrap_or(0);
    }

    let mut meridiem = None;
    if let Some(stripped) = upper.strip_suffix(" AM") {
        upper = stripped.trim_end().to_string();
        meridiem = Some("AM");
    } else if let Some(stripped) = upper.strip_suffix(" PM") {
        upper = stripped.trim_end().to_string();
        meridiem = Some("PM");
    }

    let (mut hour, minute, second, millis) = if upper.contains(':') {
        let mut parts = upper.split(':');
        let hour = parts.next()?.parse::<u64>().ok()?;
        let minute = parts.next().unwrap_or("0").parse::<u64>().ok()?;
        let second_part = parts.next().unwrap_or("0");
        let (second, millis) = parse_second_fraction(second_part)?;
        (hour, minute, second, millis)
    } else {
        parse_compact_time(&upper)?
    };

    if meridiem == Some("AM") && hour == 12 {
        hour = 0;
    } else if meridiem == Some("PM") && hour < 12 {
        hour += 12;
    }

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let local_ms = ((hour * 3600 + minute * 60 + second) * 1000) + millis;
    Some(((local_ms as i64) - (tz_offset_minutes as i64 * 60_000)).rem_euclid(86_400_000) as u64)
}

fn parse_iso_date_string(s: &str) -> Option<i64> {
    let text = s.trim();
    let date = text.split_once('T').map_or(text, |(date, _)| date);
    let digits: String = date.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        4 => Some(date_to_days(digits.parse().ok()?, 1, 1)),
        6 => Some(date_to_days(
            digits[..4].parse().ok()?,
            digits[4..6].parse().ok()?,
            1,
        )),
        8 => Some(date_to_days(
            digits[..4].parse().ok()?,
            digits[4..6].parse().ok()?,
            digits[6..8].parse().ok()?,
        )),
        _ => None,
    }
}

fn parse_named_month_date(s: &str) -> Option<i64> {
    let cleaned = s.replace(',', " ");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    if let Some(month) = lookup_month(parts[0]) {
        return Some(date_to_days(
            normalize_year(parts[2].parse().ok()?),
            month,
            parts[1].parse().ok()?,
        ));
    }
    if let Some(month) = lookup_month(parts[1]) {
        return Some(date_to_days(
            normalize_year(parts[2].parse().ok()?),
            month,
            parts[0].parse().ok()?,
        ));
    }
    None
}

fn lookup_month(name: &str) -> Option<u32> {
    match name.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn normalize_year(year: i32) -> i32 {
    if (0..100).contains(&year) {
        1900 + year
    } else {
        year
    }
}

fn weekday_name(days_since_epoch_1900: i64) -> &'static str {
    let weekdays = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let idx = (days_since_epoch_1900 - 1).rem_euclid(7) as usize;
    weekdays[idx]
}

fn parse_second_fraction(text: &str) -> Option<(u64, u64)> {
    if let Some((sec, frac)) = text.split_once('.') {
        let second = sec.parse::<u64>().ok()?;
        let millis = format!("{frac:0<3}").chars().take(3).collect::<String>();
        Some((second, millis.parse::<u64>().ok()?))
    } else {
        Some((text.parse::<u64>().ok()?, 0))
    }
}

fn parse_compact_time(text: &str) -> Option<(u64, u64, u64, u64)> {
    let digits: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let (whole, frac) = digits.split_once('.').unwrap_or((&digits, ""));
    let millis = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<3}")
            .chars()
            .take(3)
            .collect::<String>()
            .parse()
            .ok()?
    };

    match whole.len() {
        2 => Some((whole.parse().ok()?, 0, 0, millis)),
        4 => Some((
            whole[..2].parse().ok()?,
            whole[2..4].parse().ok()?,
            0,
            millis,
        )),
        6 => Some((
            whole[..2].parse().ok()?,
            whole[2..4].parse().ok()?,
            whole[4..6].parse().ok()?,
            millis,
        )),
        _ => None,
    }
}

fn parse_timezone_offset(offset: &str) -> Option<i32> {
    let offset = offset.trim();
    if offset.is_empty() {
        return Some(0);
    }
    let sign = match offset.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = offset[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    let (hours, minutes) = match digits.len() {
        2 => (digits[..2].parse::<i32>().ok()?, 0),
        4 => (
            digits[..2].parse::<i32>().ok()?,
            digits[2..4].parse::<i32>().ok()?,
        ),
        _ => return None,
    };
    Some(sign * (hours * 60 + minutes))
}

fn split_trailing_offset(text: &str) -> Option<(&str, &str)> {
    for (idx, ch) in text.char_indices().rev() {
        if ch == '+' || ch == '-' {
            return Some((&text[..idx], &text[idx..]));
        }
        if !ch.is_ascii_digit() && ch != ':' && ch != '.' {
            break;
        }
    }
    None
}

fn format_time_string(h: u64, m: u64, s: u64, format: &str) -> String {
    if format.contains('A') {
        let meridiem = if h < 12 { "AM" } else { "PM" };
        let display_h = match h % 12 {
            0 => 12,
            value => value,
        };
        if format.contains('Z') {
            format!("{display_h}:{m:02}:{s:02} {meridiem} GMT")
        } else if format.contains("SS") {
            format!("{display_h}:{m:02}:{s:02} {meridiem}")
        } else {
            format!("{display_h}:{m:02} {meridiem}")
        }
    } else if format.contains('Z') {
        format!("{h:02}:{m:02}:{s:02} GMT")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

// ============================================================
// Financial
// ============================================================

fn builtin_apr(args: &[Value]) -> Result<Value> {
    arity("Apr", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pv = args[0].to_number();
    let pmt = args[1].to_number();
    let nper = args[2].to_number();
    if pv <= 0.0 || pmt <= 0.0 || nper <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "Apr requires positive principal, payment, and period count".to_string(),
        ));
    }
    // Newton's method to find rate where PV = PMT * (1-(1+r)^-n) / r
    let mut rate: f64 = 0.1;
    for _ in 0..100 {
        let factor = (1.0 + rate).powf(-nper);
        let f = pmt * (1.0 - factor) / rate - pv;
        let df = pmt * (nper * factor / (rate * (1.0 + rate)) - (1.0 - factor) / (rate * rate));
        if df.abs() < 1e-15 {
            break;
        }
        let new_rate = rate - f / df;
        if (new_rate - rate).abs() < 1e-10 {
            rate = new_rate;
            break;
        }
        rate = new_rate;
    }
    Ok(Value::Number(rate * 12.0)) // annualized
}

fn builtin_cterm(args: &[Value]) -> Result<Value> {
    arity("CTerm", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let rate = args[0].to_number();
    let fv = args[1].to_number();
    let pv = args[2].to_number();
    if rate <= 0.0 || pv <= 0.0 || fv <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "CTerm requires positive rate, future value, and present value".to_string(),
        ));
    }
    // n = ln(FV/PV) / ln(1+rate)
    Ok(Value::Number((fv / pv).ln() / (1.0 + rate).ln()))
}

fn builtin_fv(args: &[Value]) -> Result<Value> {
    arity("FV", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if pmt <= 0.0 || nper <= 0.0 || rate < 0.0 {
        return Err(FormCalcError::RuntimeError(
            "FV requires positive payment and periods, and a non-negative rate".to_string(),
        ));
    }
    if rate == 0.0 {
        return Ok(Value::Number(pmt * nper));
    }
    // FV = PMT * ((1+r)^n - 1) / r
    Ok(Value::Number(pmt * ((1.0 + rate).powf(nper) - 1.0) / rate))
}

fn builtin_ipmt(args: &[Value]) -> Result<Value> {
    arity("IPmt", args, 5)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pv = args[0].to_number();
    let rate = args[1].to_number() / 12.0;
    let pmt = args[2].to_number();
    let first_period = args[3].to_number() as usize;
    let month_count = args[4].to_number() as usize;
    if pv <= 0.0 || rate <= 0.0 || pmt <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "IPmt requires positive principal, annual rate, and payment".to_string(),
        ));
    }
    if first_period == 0 || month_count == 0 {
        return Err(FormCalcError::RuntimeError(
            "IPmt requires positive month indexes".to_string(),
        ));
    }
    if pmt <= pv * rate {
        return Ok(Value::Number(0.0));
    }

    let mut balance = pv;
    let mut total_interest = 0.0;
    for period in 1..(first_period + month_count) {
        let interest = balance * rate;
        if period >= first_period {
            total_interest += interest;
        }
        balance += interest - pmt;
    }
    Ok(Value::Number(total_interest))
}

fn builtin_npv(args: &[Value]) -> Result<Value> {
    arity_min("NPV", args, 2)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let rate = args[0].to_number();
    if rate <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "NPV requires a positive discount rate".to_string(),
        ));
    }
    let mut npv = 0.0;
    for (i, arg) in args[1..].iter().enumerate() {
        npv += arg.to_number() / (1.0 + rate).powf(i as f64 + 1.0);
    }
    Ok(Value::Number(npv))
}

fn builtin_pmt(args: &[Value]) -> Result<Value> {
    arity("Pmt", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pv = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if pv <= 0.0 || rate <= 0.0 || nper <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "Pmt requires positive principal, rate, and periods".to_string(),
        ));
    }
    if rate == 0.0 {
        return Ok(Value::Number(pv / nper));
    }
    // PMT = PV * r / (1 - (1+r)^-n)
    Ok(Value::Number(pv * rate / (1.0 - (1.0 + rate).powf(-nper))))
}

fn builtin_ppmt(args: &[Value]) -> Result<Value> {
    arity("PPmt", args, 5)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pv = args[0].to_number();
    let rate = args[1].to_number() / 12.0;
    let pmt = args[2].to_number();
    let first_period = args[3].to_number() as usize;
    let month_count = args[4].to_number() as usize;
    if pv <= 0.0 || rate <= 0.0 || pmt <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "PPmt requires positive principal, annual rate, and payment".to_string(),
        ));
    }
    if first_period == 0 || month_count == 0 {
        return Err(FormCalcError::RuntimeError(
            "PPmt requires positive month indexes".to_string(),
        ));
    }

    let mut balance = pv;
    let mut total_principal = 0.0;
    for period in 1..(first_period + month_count) {
        let interest = balance * rate;
        let principal = pmt - interest;
        if principal <= 0.0 {
            return Err(FormCalcError::RuntimeError(
                "PPmt payment must exceed the monthly interest load".to_string(),
            ));
        }
        if period >= first_period {
            total_principal += principal;
        }
        balance -= principal;
    }
    Ok(Value::Number(total_principal))
}

fn builtin_pv(args: &[Value]) -> Result<Value> {
    arity("PV", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if pmt <= 0.0 || rate <= 0.0 || nper <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "PV requires positive payment, rate, and periods".to_string(),
        ));
    }
    if rate == 0.0 {
        return Ok(Value::Number(pmt * nper));
    }
    // PV = PMT * (1 - (1+r)^-n) / r
    Ok(Value::Number(pmt * (1.0 - (1.0 + rate).powf(-nper)) / rate))
}

fn builtin_rate(args: &[Value]) -> Result<Value> {
    arity("Rate", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let fv = args[0].to_number();
    let pv = args[1].to_number();
    let nper = args[2].to_number();
    if fv <= 0.0 || pv <= 0.0 || nper <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "Rate requires positive future value, present value, and periods".to_string(),
        ));
    }
    // rate = (FV/PV)^(1/n) - 1
    Ok(Value::Number((fv / pv).powf(1.0 / nper) - 1.0))
}

fn builtin_term(args: &[Value]) -> Result<Value> {
    arity("Term", args, 3)?;
    if any_null(args) {
        return Ok(Value::Null);
    }
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let fv = args[2].to_number();
    if rate <= 0.0 || pmt <= 0.0 || fv <= 0.0 {
        return Err(FormCalcError::RuntimeError(
            "Term requires positive payment, rate, and future value".to_string(),
        ));
    }
    // n = ln(1 + FV*r/PMT) / ln(1+r)
    Ok(Value::Number(
        (1.0 + fv * rate / pmt).ln() / (1.0 + rate).ln(),
    ))
}

// ============================================================
// Misc
// ============================================================

fn builtin_hasvalue(args: &[Value]) -> Result<Value> {
    arity("HasValue", args, 1)?;
    Ok(Value::Number(if args[0].is_blankish() { 0.0 } else { 1.0 }))
}

fn builtin_eval(_args: &[Value]) -> Result<Value> {
    todo_builtin("Eval", "§25.9", 1147, "(...)")
}

fn builtin_ref(_args: &[Value]) -> Result<Value> {
    todo_builtin("Ref", "§25.9", 1147, "(v1)")
}

fn builtin_get(_args: &[Value]) -> Result<Value> {
    todo_builtin("Get", "§25.8", 1143, "(s1)")
}

fn builtin_post(_args: &[Value]) -> Result<Value> {
    todo_builtin("Post", "§25.8", 1144, "(s1, s2[, s3[, s4[, s5]]])")
}

fn builtin_put(_args: &[Value]) -> Result<Value> {
    todo_builtin("Put", "§25.8", 1146, "(s1, s2[, s3])")
}

// ============================================================
// Helpers
// ============================================================

fn builtin_unit_value(args: &[Value]) -> Result<Value> {
    arity_range("UnitValue", args, 1, 2)?;
    if args[0].is_null() || args.get(1).is_some_and(Value::is_null) {
        return Ok(Value::Null);
    }

    let (value, source_unit) = parse_unit_span(&args[0].to_string_val()).unwrap_or((0.0, "pt"));
    let target_unit = args.get(1).map_or(source_unit, |value| {
        normalize_unit_name(&value.to_string_val()).unwrap_or(source_unit)
    });
    let points = value * unit_to_points(source_unit);
    Ok(Value::Number(points / unit_to_points(target_unit)))
}

fn builtin_unit_type(args: &[Value]) -> Result<Value> {
    arity("UnitType", args, 1)?;
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    Ok(Value::String(
        parse_unit_span(&args[0].to_string_val())
            .map(|(_, unit)| unit.to_string())
            .unwrap_or_default(),
    ))
}

static UUID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse_unit_span(text: &str) -> Option<(f64, &'static str)> {
    let trimmed = text.trim();
    let split_at = trimmed
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.')))
        .unwrap_or(trimmed.len());
    let value = trimmed[..split_at].trim().parse::<f64>().ok()?;
    let unit = normalize_unit_name(trimmed[split_at..].trim())?;
    Some((value, unit))
}

fn normalize_unit_name(unit: &str) -> Option<&'static str> {
    match unit.to_ascii_lowercase().as_str() {
        "in" | "inch" | "inches" => Some("in"),
        "mm" | "millimeter" | "millimeters" => Some("mm"),
        "cm" | "centimeter" | "centimeters" => Some("cm"),
        "pt" | "point" | "points" => Some("pt"),
        "mp" | "millipoint" | "millipoints" => Some("mp"),
        _ => None,
    }
}

fn unit_to_points(unit: &str) -> f64 {
    match unit {
        "in" => 72.0,
        "mm" => 72.0 / 25.4,
        "cm" => 72.0 / 2.54,
        "pt" => 1.0,
        "mp" => 1.0 / 1000.0,
        _ => 1.0,
    }
}

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
            return if o.is_empty() { t } else { format!("{t}-{o}") };
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
        assert_eq!(number_to_words(42), "Forty-Two");
        assert_eq!(number_to_words(100), "One Hundred");
        assert_eq!(
            number_to_words(1234),
            "One Thousand Two Hundred Thirty-Four"
        );
    }

    // --- Date/Time tests ---

    #[test]
    fn test_date2num_and_num2date() {
        let days = call_builtin(
            "Date2Num",
            &[
                Value::String("2026-03-04".to_string()),
                Value::String("YYYY-MM-DD".to_string()),
            ],
        )
        .unwrap()
        .unwrap();
        // Round-trip
        let date = call_builtin(
            "Num2Date",
            &[days.clone(), Value::String("YYYY-MM-DD".to_string())],
        )
        .unwrap()
        .unwrap();
        assert_eq!(date, Value::String("2026-03-04".to_string()));
    }

    #[test]
    fn test_time2num_and_num2time() {
        let ms = call_builtin("Time2Num", &[Value::String("14:30:00".to_string())])
            .unwrap()
            .unwrap();
        assert_eq!(ms, Value::Number(52200001.0)); // 1-based epoch

        let time = call_builtin("Num2Time", &[ms, Value::String("HH:MM:SS".to_string())])
            .unwrap()
            .unwrap();
        assert_eq!(time, Value::String("14:30:00".to_string()));
    }

    #[test]
    fn test_date_epoch() {
        // 1900-01-01 should be day 1
        let d = call_builtin(
            "Date2Num",
            &[
                Value::String("1900-01-01".to_string()),
                Value::String("YYYY-MM-DD".to_string()),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(d, Value::Number(1.0));
    }

    // --- Financial tests ---

    #[test]
    fn test_pmt() {
        // $10000 loan at 1% monthly for 12 months
        let r = call_builtin(
            "Pmt",
            &[
                Value::Number(10000.0),
                Value::Number(0.01),
                Value::Number(12.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 888.49).abs() < 0.01);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_pv() {
        // PV of $1000/month at 1% for 12 months
        let r = call_builtin(
            "PV",
            &[
                Value::Number(1000.0),
                Value::Number(0.01),
                Value::Number(12.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 11255.08).abs() < 0.01);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_fv() {
        // FV of $100/month at 1% for 12 months
        let r = call_builtin(
            "FV",
            &[
                Value::Number(100.0),
                Value::Number(0.01),
                Value::Number(12.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 1268.25).abs() < 0.01);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_rate() {
        // Rate to go from 1000 to 2000 in 10 periods
        let r = call_builtin(
            "Rate",
            &[
                Value::Number(2000.0),
                Value::Number(1000.0),
                Value::Number(10.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 0.07177).abs() < 0.001);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_cterm() {
        // Periods to go from 1000 to 2000 at 7%
        let r = call_builtin(
            "CTerm",
            &[
                Value::Number(0.07),
                Value::Number(2000.0),
                Value::Number(1000.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 10.24).abs() < 0.01);
        } else {
            panic!("expected number");
        }
    }

    #[test]
    fn test_npv() {
        // NPV at 10% discount for cash flows 100, 200, 300
        let r = call_builtin(
            "NPV",
            &[
                Value::Number(0.10),
                Value::Number(100.0),
                Value::Number(200.0),
                Value::Number(300.0),
            ],
        )
        .unwrap()
        .unwrap();
        if let Value::Number(n) = r {
            assert!((n - 481.59).abs() < 0.01);
        } else {
            panic!("expected number");
        }
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
