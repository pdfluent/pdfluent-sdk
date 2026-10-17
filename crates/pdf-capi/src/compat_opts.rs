//! JSON overlays for the larger compatible option structs.
//!
//! Five option structs cross the C boundary as a single nullable JSON object
//! string: null selects defaults, otherwise each present key overrides the
//! default for the same-named field. Keys are exactly the Rust field names,
//! so the schemas below introduce no new vocabulary; unknown keys are
//! ignored, and mistyped values fail the call with invalid-argument.
//!
//! Smaller option structs cross as explicit C parameters instead; see the
//! per-function header docs. `Value` crosses the same way in both
//! directions: JSON in, JSON out (bytes become `{"bytes": [...]}`).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::Value;
use std::os::raw::c_char;

use super::compat::{fail_arg, optional_str, unknown_variant, PdfStatus};

// ---- JSON value ------------------------------------------------------------

/// Parse a nullable JSON object argument; null becomes `None` (defaults).
pub(crate) unsafe fn optional_json(
    ptr: *const c_char,
) -> Result<Option<serde_json::Value>, PdfStatus> {
    // SAFETY: the caller guarantees a valid null-terminated string or null.
    let text = unsafe { optional_str(ptr) }?;
    match text {
        None => Ok(None),
        Some(text) => serde_json::from_str(text)
            .map(Some)
            .map_err(|e| fail_arg(&format!("invalid JSON in options argument: {e}"))),
    }
}

fn get<'a>(json: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    json.get(key)
}

fn as_bool(json: &serde_json::Value, key: &str) -> Result<Option<bool>, String> {
    match get(json, key) {
        None => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("{key}: expected a boolean")),
    }
}

fn as_i32(json: &serde_json::Value, key: &str) -> Result<Option<i32>, String> {
    match get(json, key) {
        None => Ok(None),
        Some(v) => v
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("{key}: expected an integer")),
    }
}

fn as_f64(json: &serde_json::Value, key: &str) -> Result<Option<f64>, String> {
    match get(json, key) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("{key}: expected a number")),
    }
}

fn as_string(json: &serde_json::Value, key: &str) -> Result<Option<String>, String> {
    match get(json, key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| format!("{key}: expected a string")),
    }
}

fn as_f64_list(json: &serde_json::Value, key: &str) -> Result<Option<Vec<f64>>, String> {
    match get(json, key) {
        None => Ok(None),
        Some(v) => v
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .map(|item| item.as_f64())
                    .collect::<Option<Vec<_>>>()
            })
            .map(Some)
            .ok_or_else(|| format!("{key}: expected a list of numbers")),
    }
}

// ---- Value -----------------------------------------------------------------

/// Build a dynamic value from parsed JSON.
///
/// JSON null/boolean/number/string/array/object become the matching `Value`;
/// there is no JSON spelling for bytes, so byte inputs are unsupported.
pub(crate) fn value_from_json(json: &serde_json::Value) -> Result<Value, String> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err("number out of range".to_owned())
            }
        }
        serde_json::Value::String(s) => Ok(Value::Str(s.clone())),
        serde_json::Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_from_json(item)?);
            }
            Ok(Value::List(out))
        }
        serde_json::Value::Object(map) => {
            let mut out = std::collections::HashMap::with_capacity(map.len());
            for (key, item) in map {
                out.insert(key.clone(), value_from_json(item)?);
            }
            Ok(Value::Dict(out))
        }
    }
}

/// Render a dynamic value as JSON text.
///
/// Bytes have no JSON spelling of their own and become
/// `{"bytes": [...]}` with unsigned entries. A value kind this binding does
/// not know is an error, not a guess (see `compat::unknown_variant`).
pub(crate) fn value_to_json(value: &Value) -> Result<serde_json::Value, pdfluent::Error> {
    Ok(match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(bytes) => serde_json::json!({"bytes": bytes}),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect::<Result<_, _>>()?)
        }
        Value::Dict(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), value_to_json(v)?)))
                .collect::<Result<_, pdfluent::Error>>()?,
        ),
        _ => return Err(unknown_variant("compat value")),
    })
}

// ---- SaveSettings ----------------------------------------------------------

/// Overlay `SaveSettings` fields from JSON onto defaults.
///
/// Keys: `garbage` (int), `clean`, `deflate`, `deflate_images`,
/// `deflate_fonts`, `incremental`, `ascii`, `expand`, `linear`, `no_new_id`,
/// `appearance`, `pretty` (booleans), `encryption`, `permissions` (ints),
/// `owner_password`, `user_password` (strings),
/// `preserve_metadata`, `use_objstms` (booleans), `compression_effort`
/// (int), `raise_on_repair`, `reproducible` (booleans).
pub(crate) fn save_settings_from_json(
    json: Option<&serde_json::Value>,
) -> Result<pdfluent::compat::SaveSettings, PdfStatus> {
    let mut options = pdfluent::compat::SaveSettings::default();
    let Some(json) = json else {
        return Ok(options);
    };
    overlay(json, |json| {
        if let Some(v) = as_i32(json, "garbage")? {
            options.garbage = v;
        }
        for (key, slot) in [
            ("clean", &mut options.clean),
            ("deflate", &mut options.deflate),
            ("deflate_images", &mut options.deflate_images),
            ("deflate_fonts", &mut options.deflate_fonts),
            ("incremental", &mut options.incremental),
            ("ascii", &mut options.ascii),
            ("expand", &mut options.expand),
            ("linear", &mut options.linear),
            ("no_new_id", &mut options.no_new_id),
            ("appearance", &mut options.appearance),
            ("pretty", &mut options.pretty),
            ("preserve_metadata", &mut options.preserve_metadata),
            ("use_objstms", &mut options.use_objstms),
            ("raise_on_repair", &mut options.raise_on_repair),
            ("reproducible", &mut options.reproducible),
        ] {
            if let Some(v) = as_bool(json, key)? {
                *slot = v;
            }
        }
        for (key, slot) in [
            ("encryption", &mut options.encryption),
            ("permissions", &mut options.permissions),
            ("compression_effort", &mut options.compression_effort),
        ] {
            if let Some(v) = as_i32(json, key)? {
                *slot = v;
            }
        }
        for (key, slot) in [
            ("owner_password", &mut options.owner_password),
            ("user_password", &mut options.user_password),
        ] {
            if let Some(v) = as_string(json, key)? {
                *slot = Some(v);
            }
        }
        Ok(())
    })?;
    Ok(options)
}

// ---- DrawOptions -----------------------------------------------------------

/// Overlay `DrawOptions` fields from JSON onto defaults.
///
/// Keys: `color` (number list), `fill` (number list), `dashes` (string),
/// `width` (number), `line_cap`, `line_join` (ints), `morph` (number list),
/// `close_path`, `overlay` (booleans), `stroke_opacity`, `fill_opacity`
/// (numbers), `oc` (int).
pub(crate) fn draw_options_from_json(
    json: Option<&serde_json::Value>,
) -> Result<pdfluent::compat::DrawOptions, PdfStatus> {
    let mut options = pdfluent::compat::DrawOptions::default();
    let Some(json) = json else {
        return Ok(options);
    };
    overlay(json, |json| {
        if let Some(v) = as_f64_list(json, "color")? {
            options.color = v;
        }
        if let Some(v) = as_f64_list(json, "fill")? {
            options.fill = Some(v);
        }
        if let Some(v) = as_string(json, "dashes")? {
            options.dashes = Some(v);
        }
        if let Some(v) = as_f64(json, "width")? {
            options.width = v;
        }
        if let Some(v) = as_i32(json, "line_cap")? {
            options.line_cap = v;
        }
        if let Some(v) = as_i32(json, "line_join")? {
            options.line_join = v;
        }
        if let Some(v) = as_f64_list(json, "morph")? {
            options.morph = Some(v);
        }
        if let Some(v) = as_bool(json, "close_path")? {
            options.close_path = v;
        }
        if let Some(v) = as_bool(json, "overlay")? {
            options.overlay = v;
        }
        if let Some(v) = as_f64(json, "stroke_opacity")? {
            options.stroke_opacity = v;
        }
        if let Some(v) = as_f64(json, "fill_opacity")? {
            options.fill_opacity = v;
        }
        if let Some(v) = as_i32(json, "oc")? {
            options.oc = v;
        }
        Ok(())
    })?;
    Ok(options)
}

// ---- InsertTextOptions -----------------------------------------------------

/// Overlay `InsertTextOptions` fields from JSON onto defaults.
///
/// Keys: `fontsize` (number), `lineheight` (number), `fontname` (string),
/// `fontfile` (string), `set_simple` (boolean), `encoding` (int),
/// `color`, `fill` (number lists), `border_width`, `miter_limit` (numbers),
/// `render_mode`, `rotate` (ints), `morph` (number list), `overlay`
/// (boolean), `stroke_opacity`, `fill_opacity` (numbers), `oc` (int).
pub(crate) fn insert_text_options_from_json(
    json: Option<&serde_json::Value>,
) -> Result<pdfluent::compat::InsertTextOptions, PdfStatus> {
    let mut options = pdfluent::compat::InsertTextOptions::default();
    let Some(json) = json else {
        return Ok(options);
    };
    overlay(json, |json| {
        overlay_insert_text(json, &mut options)?;
        Ok(())
    })?;
    Ok(options)
}

fn overlay_insert_text(
    json: &serde_json::Value,
    options: &mut pdfluent::compat::InsertTextOptions,
) -> Result<(), String> {
    if let Some(v) = as_f64(json, "fontsize")? {
        options.fontsize = v;
    }
    if let Some(v) = as_f64(json, "lineheight")? {
        options.lineheight = Some(v);
    }
    if let Some(v) = as_string(json, "fontname")? {
        options.fontname = v;
    }
    if let Some(v) = as_string(json, "fontfile")? {
        options.fontfile = Some(v);
    }
    if let Some(v) = as_bool(json, "set_simple")? {
        options.set_simple = v;
    }
    if let Some(v) = as_i32(json, "encoding")? {
        options.encoding = v;
    }
    if let Some(v) = as_f64_list(json, "color")? {
        options.color = Some(v);
    }
    if let Some(v) = as_f64_list(json, "fill")? {
        options.fill = Some(v);
    }
    if let Some(v) = as_f64(json, "border_width")? {
        options.border_width = v;
    }
    if let Some(v) = as_f64(json, "miter_limit")? {
        options.miter_limit = v;
    }
    if let Some(v) = as_i32(json, "render_mode")? {
        options.render_mode = v;
    }
    if let Some(v) = as_i32(json, "rotate")? {
        options.rotate = v;
    }
    if let Some(v) = as_f64_list(json, "morph")? {
        options.morph = Some(v);
    }
    if let Some(v) = as_bool(json, "overlay")? {
        options.overlay = v;
    }
    if let Some(v) = as_f64(json, "stroke_opacity")? {
        options.stroke_opacity = v;
    }
    if let Some(v) = as_f64(json, "fill_opacity")? {
        options.fill_opacity = v;
    }
    if let Some(v) = as_i32(json, "oc")? {
        options.oc = v;
    }
    Ok(())
}

// ---- TextBoxOptions --------------------------------------------------------

/// Overlay `TextBoxOptions` fields from JSON onto defaults.
///
/// Keys: `text` (an `InsertTextOptions` object), `align`, `expandtabs`
/// (ints).
pub(crate) fn text_box_options_from_json(
    json: Option<&serde_json::Value>,
) -> Result<pdfluent::compat::TextBoxOptions, PdfStatus> {
    let mut options = pdfluent::compat::TextBoxOptions::default();
    let Some(json) = json else {
        return Ok(options);
    };
    overlay(json, |json| {
        if let Some(nested) = get(json, "text") {
            overlay_insert_text(nested, &mut options.text)?;
        }
        if let Some(v) = as_i32(json, "align")? {
            options.align = v;
        }
        if let Some(v) = as_i32(json, "expandtabs")? {
            options.expandtabs = v;
        }
        Ok(())
    })?;
    Ok(options)
}

// ---- ShapeFinishOptions ----------------------------------------------------

/// Overlay `ShapeFinishOptions` fields from JSON onto defaults.
///
/// Keys: `width` (number), `color` (number list), `fill` (number list),
/// `line_cap`, `line_join` (ints), `dashes` (string), `even_odd` (boolean),
/// `morph` (number list), `close_path` (boolean), `fill_opacity`,
/// `stroke_opacity` (numbers), `oc` (int).
pub(crate) fn shape_finish_options_from_json(
    json: Option<&serde_json::Value>,
) -> Result<pdfluent::compat::ShapeFinishOptions, PdfStatus> {
    let mut options = pdfluent::compat::ShapeFinishOptions::default();
    let Some(json) = json else {
        return Ok(options);
    };
    overlay(json, |json| {
        if let Some(v) = as_f64(json, "width")? {
            options.width = v;
        }
        if let Some(v) = as_f64_list(json, "color")? {
            options.color = v;
        }
        if let Some(v) = as_f64_list(json, "fill")? {
            options.fill = Some(v);
        }
        if let Some(v) = as_i32(json, "line_cap")? {
            options.line_cap = v;
        }
        if let Some(v) = as_i32(json, "line_join")? {
            options.line_join = v;
        }
        if let Some(v) = as_string(json, "dashes")? {
            options.dashes = Some(v);
        }
        if let Some(v) = as_bool(json, "even_odd")? {
            options.even_odd = v;
        }
        if let Some(v) = as_f64_list(json, "morph")? {
            options.morph = Some(v);
        }
        if let Some(v) = as_bool(json, "close_path")? {
            options.close_path = v;
        }
        if let Some(v) = as_f64(json, "fill_opacity")? {
            options.fill_opacity = v;
        }
        if let Some(v) = as_f64(json, "stroke_opacity")? {
            options.stroke_opacity = v;
        }
        if let Some(v) = as_i32(json, "oc")? {
            options.oc = v;
        }
        Ok(())
    })?;
    Ok(options)
}

fn overlay(
    json: &serde_json::Value,
    f: impl FnOnce(&serde_json::Value) -> Result<(), String>,
) -> Result<(), PdfStatus> {
    if !json.is_object() {
        return Err(fail_arg("options argument must be a JSON object"));
    }
    f(json).map_err(|detail| fail_arg(&format!("invalid options argument: {detail}")))
}
