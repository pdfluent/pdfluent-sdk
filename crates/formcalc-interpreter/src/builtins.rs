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

        // --- Date/Time ---
        "date" => ok_some(builtin_date(args)?),
        "date2num" => ok_some(builtin_date2num(args)?),
        "dategmt" => ok_some(builtin_date(args)?), // alias
        "isodatetime" => ok_some(builtin_isodate(args)?),
        "num2date" => ok_some(builtin_num2date(args)?),
        "time" => ok_some(builtin_time(args)?),
        "time2num" => ok_some(builtin_time2num(args)?),
        "timegmt" => ok_some(builtin_time(args)?), // alias
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
    jdn - 2415021
}

/// Convert days from epoch (1900-01-01) back to (year, month, day).
fn days_to_date(days: i64) -> (i32, u32, u32) {
    let jdn = days + 2415021;
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
    // Return current date as days since 1900-01-01
    // Use a fixed date for determinism in tests; real impl would use system time
    // 2026-03-04 = 46,081 days since 1900-01-01
    Ok(Value::Number(date_to_days(2026, 3, 4) as f64))
}

fn builtin_date2num(args: &[Value]) -> Result<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "Date2Num".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let date_str = args[0].to_string_val();
    let format = if args.len() > 1 {
        args[1].to_string_val()
    } else {
        "YYYY-MM-DD".to_string()
    };

    // Simple parser for common formats
    let days = parse_date_string(&date_str, &format)
        .ok_or_else(|| FormCalcError::RuntimeError(format!("cannot parse date: '{date_str}'")))?;
    Ok(Value::Number(days as f64))
}

fn builtin_num2date(args: &[Value]) -> Result<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "Num2Date".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let days = args[0].to_number() as i64;
    let format = if args.len() > 1 {
        args[1].to_string_val()
    } else {
        "YYYY-MM-DD".to_string()
    };

    let (y, m, d) = days_to_date(days);
    let result = format_date(y, m, d, &format);
    Ok(Value::String(result))
}

fn builtin_isodate(args: &[Value]) -> Result<Value> {
    if !args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "IsoDateTime".to_string(),
            expected: "0".to_string(),
            got: args.len(),
        });
    }
    Ok(Value::String("2026-03-04T00:00:00".to_string()))
}

fn builtin_time(args: &[Value]) -> Result<Value> {
    if !args.is_empty() {
        return Err(FormCalcError::ArityError {
            name: "Time".to_string(),
            expected: "0".to_string(),
            got: args.len(),
        });
    }
    // Milliseconds since midnight; return fixed value for determinism
    Ok(Value::Number(43200000.0)) // 12:00:00
}

fn builtin_time2num(args: &[Value]) -> Result<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "Time2Num".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let time_str = args[0].to_string_val();
    let ms = parse_time_string(&time_str)
        .ok_or_else(|| FormCalcError::RuntimeError(format!("cannot parse time: '{time_str}'")))?;
    Ok(Value::Number(ms as f64))
}

fn builtin_num2time(args: &[Value]) -> Result<Value> {
    if args.is_empty() || args.len() > 2 {
        return Err(FormCalcError::ArityError {
            name: "Num2Time".to_string(),
            expected: "1 or 2".to_string(),
            got: args.len(),
        });
    }
    let ms = args[0].to_number() as u64;
    let secs = (ms / 1000) % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    Ok(Value::String(format!("{h:02}:{m:02}:{s:02}")))
}

fn parse_date_string(s: &str, _format: &str) -> Option<i64> {
    // Parse YYYY-MM-DD or MM/DD/YYYY
    let parts: Vec<&str> = s.split(['-', '/']).collect();
    if parts.len() != 3 {
        return None;
    }
    let (year, month, day) = if parts[0].len() == 4 {
        // YYYY-MM-DD
        (
            parts[0].parse::<i32>().ok()?,
            parts[1].parse::<u32>().ok()?,
            parts[2].parse::<u32>().ok()?,
        )
    } else {
        // MM/DD/YYYY
        (
            parts[2].parse::<i32>().ok()?,
            parts[0].parse::<u32>().ok()?,
            parts[1].parse::<u32>().ok()?,
        )
    };
    Some(date_to_days(year, month, day))
}

fn format_date(y: i32, m: u32, d: u32, format: &str) -> String {
    format
        .replace("YYYY", &format!("{y:04}"))
        .replace("MM", &format!("{m:02}"))
        .replace("DD", &format!("{d:02}"))
}

fn parse_time_string(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let s: u64 = if parts.len() > 2 {
        parts[2].parse().ok()?
    } else {
        0
    };
    Some((h * 3600 + m * 60 + s) * 1000)
}

// ============================================================
// Financial
// ============================================================

fn builtin_apr(args: &[Value]) -> Result<Value> {
    arity("Apr", args, 3)?;
    let pmt = args[0].to_number();
    let pv = args[1].to_number();
    let nper = args[2].to_number();
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
    let rate = args[0].to_number();
    let fv = args[1].to_number();
    let pv = args[2].to_number();
    if rate <= 0.0 || pv <= 0.0 || fv <= 0.0 {
        return Ok(Value::Number(0.0));
    }
    // n = ln(FV/PV) / ln(1+rate)
    Ok(Value::Number((fv / pv).ln() / (1.0 + rate).ln()))
}

fn builtin_fv(args: &[Value]) -> Result<Value> {
    arity("FV", args, 3)?;
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if rate == 0.0 {
        return Ok(Value::Number(pmt * nper));
    }
    // FV = PMT * ((1+r)^n - 1) / r
    Ok(Value::Number(pmt * ((1.0 + rate).powf(nper) - 1.0) / rate))
}

fn builtin_ipmt(args: &[Value]) -> Result<Value> {
    arity("IPmt", args, 5)?;
    let pv = args[0].to_number();
    let rate = args[1].to_number();
    let pmt = args[2].to_number();
    let first_period = args[3].to_number() as usize;
    let last_period = args[4].to_number() as usize;

    let mut balance = pv;
    let mut total_interest = 0.0;
    for period in 1..=last_period {
        let interest = balance * rate;
        if period >= first_period {
            total_interest += interest;
        }
        balance = balance + interest - pmt;
    }
    Ok(Value::Number(total_interest))
}

fn builtin_npv(args: &[Value]) -> Result<Value> {
    arity_min("NPV", args, 2)?;
    let rate = args[0].to_number();
    let mut npv = 0.0;
    for (i, arg) in args[1..].iter().enumerate() {
        npv += arg.to_number() / (1.0 + rate).powf(i as f64 + 1.0);
    }
    Ok(Value::Number(npv))
}

fn builtin_pmt(args: &[Value]) -> Result<Value> {
    arity("Pmt", args, 3)?;
    let pv = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if rate == 0.0 {
        return Ok(Value::Number(pv / nper));
    }
    // PMT = PV * r / (1 - (1+r)^-n)
    Ok(Value::Number(pv * rate / (1.0 - (1.0 + rate).powf(-nper))))
}

fn builtin_ppmt(args: &[Value]) -> Result<Value> {
    arity("PPmt", args, 5)?;
    let pv = args[0].to_number();
    let rate = args[1].to_number();
    let pmt = args[2].to_number();
    let first_period = args[3].to_number() as usize;
    let last_period = args[4].to_number() as usize;

    let mut balance = pv;
    let mut total_principal = 0.0;
    for period in 1..=last_period {
        let interest = balance * rate;
        let principal = pmt - interest;
        if period >= first_period {
            total_principal += principal;
        }
        balance -= principal;
    }
    Ok(Value::Number(total_principal))
}

fn builtin_pv(args: &[Value]) -> Result<Value> {
    arity("PV", args, 3)?;
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let nper = args[2].to_number();
    if rate == 0.0 {
        return Ok(Value::Number(pmt * nper));
    }
    // PV = PMT * (1 - (1+r)^-n) / r
    Ok(Value::Number(pmt * (1.0 - (1.0 + rate).powf(-nper)) / rate))
}

fn builtin_rate(args: &[Value]) -> Result<Value> {
    arity("Rate", args, 3)?;
    let fv = args[0].to_number();
    let pv = args[1].to_number();
    let nper = args[2].to_number();
    if nper == 0.0 || pv == 0.0 {
        return Ok(Value::Number(0.0));
    }
    // rate = (FV/PV)^(1/n) - 1
    Ok(Value::Number((fv / pv).powf(1.0 / nper) - 1.0))
}

fn builtin_term(args: &[Value]) -> Result<Value> {
    arity("Term", args, 3)?;
    let pmt = args[0].to_number();
    let rate = args[1].to_number();
    let fv = args[2].to_number();
    if rate <= 0.0 || pmt <= 0.0 {
        return Ok(Value::Number(0.0));
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

    // --- Date/Time tests ---

    #[test]
    fn test_date2num_and_num2date() {
        let days = call_builtin("Date2Num", &[Value::String("2026-03-04".to_string())])
            .unwrap()
            .unwrap();
        // Round-trip
        let date = call_builtin("Num2Date", &[days.clone()]).unwrap().unwrap();
        assert_eq!(date, Value::String("2026-03-04".to_string()));
    }

    #[test]
    fn test_time2num_and_num2time() {
        let ms = call_builtin("Time2Num", &[Value::String("14:30:00".to_string())])
            .unwrap()
            .unwrap();
        assert_eq!(ms, Value::Number(52200000.0)); // 14*3600000 + 30*60000

        let time = call_builtin("Num2Time", &[ms]).unwrap().unwrap();
        assert_eq!(time, Value::String("14:30:00".to_string()));
    }

    #[test]
    fn test_date_epoch() {
        // 1900-01-01 should be day 0
        let d = call_builtin("Date2Num", &[Value::String("1900-01-01".to_string())])
            .unwrap()
            .unwrap();
        assert_eq!(d, Value::Number(0.0));
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
