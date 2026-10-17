//! Native side of the compatible Node API: one file per lane.
//!
//! Each lane adds its methods to the shared classes with its own `#[napi]`
//! impl blocks inside its own file; those blocks self-register at load and
//! need no call here. New `#[napi]` types a lane adds also live in that
//! lane's file. This module holds the document state shared by every view
//! plus the JSON bridge and option converters every lane uses.
//! Test builds only: the `#[napi]` macro gates its registration glue on
//! `not(test)`, so names only the JS side reaches read as dead there.
#![cfg_attr(test, allow(dead_code))]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod constants;
pub mod document;
pub mod edit;
pub mod functions;
pub mod geometry;
pub mod pixmap;
pub mod text;

use pdfluent::compat as fc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, MutexGuard,
};

/// Document state shared by every live view (pages, annots, shapes, text pages).
pub(crate) struct Shared {
    doc: Mutex<FacadePdfDocument>,
    closed: AtomicBool,
}

pub(crate) type FacadePdfDocument = pdfluent::PdfDocument;

impl Shared {
    pub(crate) fn new(doc: FacadePdfDocument) -> Self {
        Self {
            doc: Mutex::new(doc),
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) fn check_open(&self, op: &str) -> napi::Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: the document is closed"),
            ));
        }
        Ok(())
    }

    pub(crate) fn lock(&self, op: &str) -> napi::Result<MutexGuard<'_, FacadePdfDocument>> {
        self.check_open(op)?;
        self.doc
            .lock()
            .map_err(|_| crate::error::internal(op, format!("{op}: the document lock is poisoned")))
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn compat_error(op: &str, kind: fc::ErrorKind, detail: String) -> napi::Error {
    crate::error::facade(op)(kind.to_error(detail))
}

pub(crate) fn wrong_type(op: &str, detail: String) -> napi::Error {
    compat_error(op, fc::ErrorKind::WrongType, detail)
}

pub(crate) fn bad_value(op: &str, detail: String) -> napi::Error {
    compat_error(op, fc::ErrorKind::BadValue, detail)
}

pub(crate) fn json_to_compat(v: &serde_json::Value) -> fc::Value {
    match v {
        serde_json::Value::Null => fc::Value::Null,
        serde_json::Value::Bool(b) => fc::Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                fc::Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                fc::Value::Float(f)
            } else {
                fc::Value::Int(i64::MAX)
            }
        }
        serde_json::Value::String(s) => fc::Value::Str(s.clone()),
        serde_json::Value::Array(items) => {
            fc::Value::List(items.iter().map(json_to_compat).collect())
        }
        serde_json::Value::Object(map) => fc::Value::Dict(
            map.iter()
                .map(|(k, val)| (k.clone(), json_to_compat(val)))
                .collect(),
        ),
    }
}

/// The error for a value of a `#[non_exhaustive]` facade enum that this
/// binding does not know yet.
///
/// The facade's 1.x enums are `#[non_exhaustive]`, so a later minor release
/// may add a variant this build was not compiled against. Spelling it as a
/// neighbouring JSON value would misstate it; failing says what happened.
pub(crate) fn unknown_variant(what: &str) -> napi::Error {
    crate::error::unsupported(
        what,
        format!("{what}: this value is not supported by this version of the Node binding"),
    )
}

pub(crate) fn compat_to_json(v: &fc::Value) -> napi::Result<serde_json::Value> {
    Ok(match v {
        fc::Value::Null => serde_json::Value::Null,
        fc::Value::Bool(b) => serde_json::Value::Bool(*b),
        fc::Value::Int(i) => serde_json::Value::from(*i),
        fc::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        fc::Value::Str(s) => serde_json::Value::String(s.clone()),
        fc::Value::Bytes(b) => bytes_json(b),
        fc::Value::List(items) => json_array(
            items
                .iter()
                .map(compat_to_json)
                .collect::<napi::Result<_>>()?,
        ),
        fc::Value::Dict(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, val) in map {
                out.insert(k.clone(), compat_to_json(val)?);
            }
            serde_json::Value::Object(out)
        }
        _ => return Err(unknown_variant("compat value")),
    })
}

pub(crate) fn bytes_json(b: &[u8]) -> serde_json::Value {
    json_array(b.iter().map(|n| serde_json::Value::from(*n)).collect())
}

pub(crate) fn json_array(items: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Array(items)
}

pub(crate) fn opt_json<T>(o: Option<&T>, f: impl Fn(&T) -> serde_json::Value) -> serde_json::Value {
    o.map(f).unwrap_or(serde_json::Value::Null)
}

pub(crate) type JsonMap = serde_json::Map<String, serde_json::Value>;

pub(crate) fn opt_obj<'a>(
    op: &str,
    v: Option<&'a serde_json::Value>,
) -> napi::Result<Option<&'a JsonMap>> {
    match v {
        None => Ok(None),
        Some(x) => match x.as_object() {
            Some(map) => Ok(Some(map)),
            None => Err(wrong_type(op, format!("{op}: options must be an object"))),
        },
    }
}

pub(crate) fn req_obj<'a>(op: &str, v: &'a serde_json::Value) -> napi::Result<&'a JsonMap> {
    v.as_object()
        .ok_or_else(|| wrong_type(op, format!("{op}: expected an object")))
}

pub(crate) fn flip_case(name: &str) -> String {
    if name.contains('_') {
        let mut out = String::with_capacity(name.len());
        let mut upper = false;
        for ch in name.chars() {
            if ch == '_' {
                upper = true;
            } else if upper {
                out.extend(ch.to_uppercase());
                upper = false;
            } else {
                out.push(ch);
            }
        }
        out
    } else {
        let mut out = String::with_capacity(name.len() + 4);
        for ch in name.chars() {
            if ch.is_uppercase() {
                out.push('_');
                out.extend(ch.to_lowercase());
            } else {
                out.push(ch);
            }
        }
        out
    }
}

pub(crate) fn find_key<'a>(map: &'a JsonMap, name: &str) -> Option<&'a serde_json::Value> {
    if let Some(v) = map.get(name) {
        return Some(v);
    }
    map.get(&flip_case(name))
}

pub(crate) fn check_keys(op: &str, map: &JsonMap, known: &[&str]) -> napi::Result<()> {
    for key in map.keys() {
        if !known.contains(&key.as_str()) && !known.contains(&flip_case(key).as_str()) {
            return Err(wrong_type(op, format!("{op}: unknown option {key:?}")));
        }
    }
    Ok(())
}

pub(crate) fn forbid_key(op: &str, map: &JsonMap, key: &str, msg: &str) -> napi::Result<()> {
    if find_key(map, key).is_some_and(|v| !v.is_null()) {
        return Err(wrong_type(op, format!("{op}: {msg}")));
    }
    Ok(())
}

pub(crate) fn accept_default(
    op: &str,
    v: Option<&serde_json::Value>,
    dflt: serde_json::Value,
    name: &str,
) -> napi::Result<()> {
    match v {
        None => Ok(()),
        Some(x) if x.is_null() || *x == dflt => Ok(()),
        Some(_) => Err(bad_value(
            op,
            format!("{op}: {name} is not supported in this build"),
        )),
    }
}

pub(crate) fn opts_key_default(
    opts: Option<&serde_json::Value>,
    key: &str,
    dflt: serde_json::Value,
    name: &str,
) -> napi::Result<()> {
    match opts.and_then(|o| o.get(key)) {
        None => Ok(()),
        Some(x) if x.is_null() || *x == dflt => Ok(()),
        Some(_) => Err(bad_value(
            name,
            format!("{name}: option {key} is not supported in this build"),
        )),
    }
}

pub(crate) fn as_bool(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<bool> {
    v.as_bool()
        .ok_or_else(|| wrong_type(op, format!("{op}: {field} must be a boolean")))
}

pub(crate) fn as_i32(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<i32> {
    match v.as_i64() {
        Some(n) => {
            i32::try_from(n).map_err(|_| bad_value(op, format!("{op}: {field} is out of range")))
        }
        None => Err(wrong_type(op, format!("{op}: {field} must be an integer"))),
    }
}

pub(crate) fn as_f64(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<f64> {
    v.as_f64()
        .ok_or_else(|| wrong_type(op, format!("{op}: {field} must be a number")))
}

pub(crate) fn as_str(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<String> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| wrong_type(op, format!("{op}: {field} must be a string")))
}

pub(crate) fn as_vec_f64(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<Vec<f64>> {
    match v.as_array() {
        Some(items) => items.iter().map(|x| as_f64(op, x, field)).collect(),
        None => Err(wrong_type(
            op,
            format!("{op}: {field} must be an array of numbers"),
        )),
    }
}

pub(crate) fn get_bool(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<bool>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(as_bool(op, v, field)?)),
    }
}

pub(crate) fn get_i32(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<i32>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(as_i32(op, v, field)?)),
    }
}

pub(crate) fn get_f64(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<f64>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(as_f64(op, v, field)?)),
    }
}

pub(crate) fn get_str(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<String>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(as_str(op, v, field)?)),
    }
}

pub(crate) fn get_vec_f64(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<Vec<f64>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(as_vec_f64(op, v, field)?)),
    }
}

pub(crate) fn get_vec_i32(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<Vec<i32>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_array() {
            Some(items) => Ok(Some(
                items
                    .iter()
                    .map(|x| as_i32(op, x, field))
                    .collect::<napi::Result<Vec<_>>>()?,
            )),
            None => Err(wrong_type(
                op,
                format!("{op}: {field} must be an array of integers"),
            )),
        },
    }
}

pub(crate) fn get_vec_string(
    op: &str,
    map: &JsonMap,
    field: &str,
) -> napi::Result<Option<Vec<String>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_array() {
            Some(items) => Ok(Some(
                items
                    .iter()
                    .map(|x| as_str(op, x, field))
                    .collect::<napi::Result<Vec<_>>>()?,
            )),
            None => Err(wrong_type(
                op,
                format!("{op}: {field} must be an array of strings"),
            )),
        },
    }
}

pub(crate) fn as_u8(op: &str, v: &serde_json::Value, field: &str) -> napi::Result<u8> {
    v.as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .ok_or_else(|| wrong_type(op, format!("{op}: {field} must be a byte (0-255)")))
}

pub(crate) fn get_vec_u8(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<Vec<u8>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_array() {
            Some(items) => Ok(Some(
                items
                    .iter()
                    .map(|x| as_u8(op, x, field))
                    .collect::<napi::Result<Vec<_>>>()?,
            )),
            None => Err(wrong_type(
                op,
                format!("{op}: {field} must be an array of bytes"),
            )),
        },
    }
}

pub(crate) fn get_vec_point(
    op: &str,
    map: &JsonMap,
    field: &str,
) -> napi::Result<Option<Vec<fc::Point>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_array() {
            Some(items) => Ok(Some(
                items
                    .iter()
                    .map(|x| point_from_json(op, x))
                    .collect::<napi::Result<Vec<_>>>()?,
            )),
            None => Err(wrong_type(
                op,
                format!("{op}: {field} must be an array of points"),
            )),
        },
    }
}

pub(crate) fn get_vec_quad(
    op: &str,
    map: &JsonMap,
    field: &str,
) -> napi::Result<Option<Vec<fc::Quad>>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_array() {
            Some(items) => Ok(Some(
                items
                    .iter()
                    .map(|x| quad_from_json(op, x))
                    .collect::<napi::Result<Vec<_>>>()?,
            )),
            None => Err(wrong_type(
                op,
                format!("{op}: {field} must be an array of quads"),
            )),
        },
    }
}

pub(crate) fn get_first<T>(
    op: &str,
    map: &JsonMap,
    keys: &[&str],
    f: fn(&str, &JsonMap, &str) -> napi::Result<Option<T>>,
) -> napi::Result<Option<T>> {
    for key in keys {
        if let Some(v) = f(op, map, key)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

pub(crate) fn get_format(
    op: &str,
    map: &JsonMap,
    keys: &[&str],
) -> napi::Result<Option<fc::TextFormat>> {
    if let Some(name) = get_first(op, map, keys, get_str)? {
        return Ok(Some(
            fc::TextFormat::from_name(&name).map_err(crate::error::facade(op))?,
        ));
    }
    Ok(None)
}

pub(crate) fn get_vec_with<T>(
    op: &str,
    map: &JsonMap,
    field: &str,
    f: fn(&str, &serde_json::Value) -> napi::Result<T>,
) -> napi::Result<Vec<T>> {
    match find_key(map, field) {
        Some(v) => match v.as_array() {
            Some(items) => items.iter().map(|x| f(op, x)).collect(),
            None => Err(wrong_type(op, format!("{op}: {field} must be an array"))),
        },
        None => Err(wrong_type(op, format!("{op}: {field} is required"))),
    }
}

pub(crate) fn get_opt_with<T>(
    op: &str,
    map: &JsonMap,
    field: &str,
    f: fn(&str, &serde_json::Value) -> napi::Result<T>,
) -> napi::Result<Option<T>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(f(op, v)?)),
    }
}

macro_rules! req_getters {
    ($name:ident, $inner:ident, $ty:ty) => {
        pub(crate) fn $name(op: &str, map: &JsonMap, field: &str) -> napi::Result<$ty> {
            $inner(op, map, field)?
                .ok_or_else(|| wrong_type(op, format!("{op}: {field} is required")))
        }
    };
}

req_getters!(get_bool_req, get_bool, bool);
req_getters!(get_i32_req, get_i32, i32);
req_getters!(get_f64_req, get_f64, f64);
req_getters!(get_str_req, get_str, String);
req_getters!(get_point_req, get_point, fc::Point);
req_getters!(get_rect_req, get_rect, fc::Rect);

pub(crate) fn get_char_req(op: &str, map: &JsonMap, field: &str) -> napi::Result<char> {
    let s = get_str_req(op, map, field)?;
    s.chars()
        .next()
        .ok_or_else(|| wrong_type(op, format!("{op}: {field} must not be empty")))
}

fn num_at(op: &str, items: &[serde_json::Value], i: usize, what: &str) -> napi::Result<f64> {
    items
        .get(i)
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| wrong_type(op, format!("{op}: {what} needs numbers")))
}

pub(crate) fn point_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::Point> {
    if let Some(items) = v.as_array() {
        if items.len() == 2 {
            return Ok(fc::Point {
                x: num_at(op, items, 0, "point")?,
                y: num_at(op, items, 1, "point")?,
            });
        }
        return Err(wrong_type(op, format!("{op}: point needs [x, y]")));
    }
    let map = req_obj(op, v)?;
    Ok(fc::Point {
        x: get_f64_req(op, map, "x")?,
        y: get_f64_req(op, map, "y")?,
    })
}

pub(crate) fn rect_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::Rect> {
    if let Some(items) = v.as_array() {
        if items.len() == 4 {
            return Ok(fc::Rect {
                x0: num_at(op, items, 0, "rect")?,
                y0: num_at(op, items, 1, "rect")?,
                x1: num_at(op, items, 2, "rect")?,
                y1: num_at(op, items, 3, "rect")?,
            });
        }
        return Err(wrong_type(op, format!("{op}: rect needs [x0, y0, x1, y1]")));
    }
    let map = req_obj(op, v)?;
    Ok(fc::Rect {
        x0: get_f64_req(op, map, "x0")?,
        y0: get_f64_req(op, map, "y0")?,
        x1: get_f64_req(op, map, "x1")?,
        y1: get_f64_req(op, map, "y1")?,
    })
}

#[allow(dead_code)]
pub(crate) fn irect_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::IRect> {
    if let Some(items) = v.as_array() {
        if items.len() == 4 {
            return Ok(fc::IRect {
                x0: as_i32(op, &items[0], "x0")?,
                y0: as_i32(op, &items[1], "y0")?,
                x1: as_i32(op, &items[2], "x1")?,
                y1: as_i32(op, &items[3], "y1")?,
            });
        }
        return Err(wrong_type(op, format!("{op}: rect needs [x0, y0, x1, y1]")));
    }
    let map = req_obj(op, v)?;
    Ok(fc::IRect {
        x0: get_i32_req(op, map, "x0")?,
        y0: get_i32_req(op, map, "y0")?,
        x1: get_i32_req(op, map, "x1")?,
        y1: get_i32_req(op, map, "y1")?,
    })
}

pub(crate) fn matrix_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::Matrix> {
    if let Some(items) = v.as_array() {
        if items.len() == 6 {
            return Ok(fc::Matrix {
                a: num_at(op, items, 0, "matrix")?,
                b: num_at(op, items, 1, "matrix")?,
                c: num_at(op, items, 2, "matrix")?,
                d: num_at(op, items, 3, "matrix")?,
                e: num_at(op, items, 4, "matrix")?,
                f: num_at(op, items, 5, "matrix")?,
            });
        }
        return Err(wrong_type(
            op,
            format!("{op}: matrix needs [a, b, c, d, e, f]"),
        ));
    }
    let map = req_obj(op, v)?;
    Ok(fc::Matrix {
        a: get_f64_req(op, map, "a")?,
        b: get_f64_req(op, map, "b")?,
        c: get_f64_req(op, map, "c")?,
        d: get_f64_req(op, map, "d")?,
        e: get_f64_req(op, map, "e")?,
        f: get_f64_req(op, map, "f")?,
    })
}

pub(crate) fn quad_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::Quad> {
    if let Some(items) = v.as_array() {
        if items.len() == 4 {
            return Ok(fc::Quad {
                ul: point_from_json(op, &items[0])?,
                ur: point_from_json(op, &items[1])?,
                ll: point_from_json(op, &items[2])?,
                lr: point_from_json(op, &items[3])?,
            });
        }
        return Err(wrong_type(op, format!("{op}: quad needs four points")));
    }
    let map = req_obj(op, v)?;
    let corner = |field: &str| -> napi::Result<fc::Point> {
        match find_key(map, field) {
            Some(x) => point_from_json(op, x),
            None => Err(wrong_type(op, format!("{op}: {field} is required"))),
        }
    };
    Ok(fc::Quad {
        ul: corner("ul")?,
        ur: corner("ur")?,
        ll: corner("ll")?,
        lr: corner("lr")?,
    })
}

pub(crate) fn get_point(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<fc::Point>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(point_from_json(op, v)?)),
    }
}

pub(crate) fn get_rect(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<fc::Rect>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(rect_from_json(op, v)?)),
    }
}

#[allow(dead_code)]
pub(crate) fn get_irect(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<fc::IRect>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(irect_from_json(op, v)?)),
    }
}

pub(crate) fn get_matrix(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<fc::Matrix>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(matrix_from_json(op, v)?)),
    }
}

#[allow(dead_code)]
pub(crate) fn get_quad(op: &str, map: &JsonMap, field: &str) -> napi::Result<Option<fc::Quad>> {
    match find_key(map, field) {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => Ok(Some(quad_from_json(op, v)?)),
    }
}

pub(crate) fn geom_point(v: &fc::Point) -> serde_json::Value {
    serde_json::json!([v.x, v.y])
}

pub(crate) fn geom_rect(v: &fc::Rect) -> serde_json::Value {
    serde_json::json!([v.x0, v.y0, v.x1, v.y1])
}

#[allow(dead_code)]
pub(crate) fn geom_irect(v: &fc::IRect) -> serde_json::Value {
    serde_json::json!([v.x0, v.y0, v.x1, v.y1])
}

pub(crate) fn geom_matrix(v: &fc::Matrix) -> serde_json::Value {
    serde_json::json!([v.a, v.b, v.c, v.d, v.e, v.f])
}

pub(crate) fn geom_quad(v: &fc::Quad) -> serde_json::Value {
    serde_json::json!([
        [v.ul.x, v.ul.y],
        [v.ur.x, v.ur.y],
        [v.ll.x, v.ll.y],
        [v.lr.x, v.lr.y]
    ])
}

pub(crate) fn alpha_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::AlphaOptions> {
    let mut o = fc::AlphaOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("alphavalues", "values")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    if let Some(val) = find_key(map, "matte") {
        if !val.is_null() {
            o.matte = Some(json_to_compat(val));
        }
    }
    if let Some(val) = find_key(map, "opaque") {
        if !val.is_null() {
            o.opaque = Some(json_to_compat(val));
        }
    }
    if let Some(x) = get_bool(op, map, "premultiply")? {
        o.premultiply = x;
    }
    o.values = get_vec_u8(op, map, "values")?;
    check_keys(
        op,
        map,
        &["alphavalues", "matte", "opaque", "premultiply", "values"],
    )?;
    Ok(o)
}

pub(crate) fn annot_colors_from_json(
    op: &str,
    v: &serde_json::Value,
) -> napi::Result<fc::AnnotColors> {
    let map = req_obj(op, v)?;
    let mut stroke = get_vec_f64(op, map, "stroke")?.unwrap_or_default();
    let mut fill = get_vec_f64(op, map, "fill")?.unwrap_or_default();
    if let Some(pair) = find_key(map, "colors") {
        match pair.as_array().map(Vec::as_slice) {
            Some([s, f]) => {
                stroke = as_vec_f64(op, s, "colors[0]")?;
                fill = as_vec_f64(op, f, "colors[1]")?;
            }
            _ => return Err(wrong_type(op, format!("{op}: colors needs [stroke, fill]"))),
        }
    }
    check_keys(op, map, &["stroke", "fill", "colors"])?;
    Ok(fc::AnnotColors { stroke, fill })
}

pub(crate) fn annot_ident_from_json(
    op: &str,
    v: &serde_json::Value,
) -> napi::Result<fc::AnnotIdent> {
    if let Some(n) = v.as_i64() {
        return Ok(fc::AnnotIdent::Xref(n as i32));
    }
    if let Some(s) = v.as_str() {
        return Ok(fc::AnnotIdent::Name(s.to_string()));
    }
    Err(wrong_type(
        op,
        format!("{op}: ident needs a number or a name"),
    ))
}

pub(crate) fn annot_info_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::AnnotInfoOptions> {
    let mut o = fc::AnnotInfoOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    o.content = get_str(op, map, "content")?;
    o.creation_date = get_str(op, map, "creation_date")?;
    if let Some(sub) = find_key(map, "info") {
        o.info = Some(annot_info_from_json(op, sub)?);
    }
    o.mod_date = get_str(op, map, "mod_date")?;
    o.subject = get_str(op, map, "subject")?;
    o.title = get_str(op, map, "title")?;
    check_keys(
        op,
        map,
        &[
            "content",
            "creation_date",
            "info",
            "mod_date",
            "subject",
            "title",
        ],
    )?;
    Ok(o)
}

pub(crate) fn annot_update_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::AnnotUpdateOptions> {
    let mut o = fc::AnnotUpdateOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_str(op, map, "blend_mode")? {
        o = o.with_blend_mode(&x);
    }
    if let Some(x) = get_vec_f64(op, map, "border_color")? {
        o = o.with_border_color(x);
    }
    if let Some(x) = get_bool(op, map, "cross_out")? {
        o = o.with_cross_out(x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill_color")? {
        o = o.with_fill_color(x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_f64(op, map, "opacity")? {
        o = o.with_opacity(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(x) = get_vec_f64(op, map, "text_color")? {
        o = o.with_text_color(x);
    }
    check_keys(
        op,
        map,
        &[
            "blend_mode",
            "border_color",
            "cross_out",
            "fill_color",
            "fontname",
            "fontsize",
            "opacity",
            "rotate",
            "text_color",
        ],
    )?;
    Ok(o)
}

pub(crate) fn append_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::AppendOptions<'a>> {
    let mut o = fc::AppendOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(op, map, "font", "pass font as the font argument")?;
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_str(op, map, "language")? {
        o = o.with_language(&x);
    }
    if let Some(x) = get_bool(op, map, "right_to_left")? {
        o = o.with_right_to_left(x);
    }
    if let Some(x) = get_bool(op, map, "small_caps")? {
        o = o.with_small_caps(x);
    }
    check_keys(
        op,
        map,
        &[
            "font",
            "fontsize",
            "language",
            "right_to_left",
            "small_caps",
        ],
    )?;
    Ok(o)
}

pub(crate) fn bake_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::BakeOptions> {
    let mut o = fc::BakeOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "annots")? {
        o.annots = x;
    }
    if let Some(x) = get_bool(op, map, "widgets")? {
        o.widgets = x;
    }
    check_keys(op, map, &["annots", "widgets"])?;
    Ok(o)
}

pub(crate) fn block_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::BlockOptions<'a>> {
    let mut o = fc::BlockOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "textpage",
        "pass textpage as the textpage argument",
    )?;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_i32(op, map, "flags")? {
        o = o.with_flags(x);
    }
    if let Some(x) = get_bool(op, map, "sort")? {
        o = o.with_sort(x);
    }
    check_keys(op, map, &["clip", "flags", "sort", "textpage"])?;
    Ok(o)
}

pub(crate) fn border_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::BorderOptions> {
    let mut o = fc::BorderOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "clouds")? {
        o.clouds = x;
    }
    if let Some(x) = get_vec_i32(op, map, "dashes")? {
        o.dashes = x;
    }
    o.style = get_str(op, map, "style")?;
    if let Some(x) = get_f64(op, map, "width")? {
        o.width = x;
    }
    check_keys(op, map, &["clouds", "dashes", "style", "width"])?;
    Ok(o)
}

pub(crate) fn bulk_text_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::BulkTextOptions> {
    let mut o = fc::BulkTextOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("option", "format")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_i32(op, map, "concurrency")? {
        o = o.with_concurrency(x);
    }
    if let Some(x) = get_str(op, map, "delimiters")? {
        o = o.with_delimiters(&x);
    }
    if let Some(x) = get_i32(op, map, "flags")? {
        o = o.with_flags(x);
    }
    if let Some(fmt) = get_format(op, map, &["format", "option"])? {
        o = o.with_format(fmt);
    }
    if let Some(x) = get_str(op, map, "method")? {
        o = o.with_method(&x);
    }
    if let Some(x) = get_vec_i32(op, map, "pages")? {
        o = o.with_pages(x);
    }
    if let Some(x) = get_bool(op, map, "sort")? {
        o = o.with_sort(x);
    }
    check_keys(
        op,
        map,
        &[
            "clip",
            "concurrency",
            "delimiters",
            "flags",
            "format",
            "method",
            "option",
            "pages",
            "sort",
        ],
    )?;
    Ok(o)
}

pub(crate) fn cdrawings_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::CdrawingsOptions> {
    let mut o = fc::CdrawingsOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(op, map, "callback", "callbacks cannot cross into napi")?;
    if let Some(x) = get_bool(op, map, "extended")? {
        o.extended = x;
    }
    check_keys(op, map, &["callback", "extended"])?;
    Ok(o)
}

pub(crate) fn cluster_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ClusterOptions> {
    let mut o = fc::ClusterOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "drawings",
        "caller-supplied drawings are not supported in this build",
    )?;
    if let Some(g) = get_rect(op, map, "clip")? {
        o.clip = Some(g);
    }
    if let Some(x) = get_bool(op, map, "final_filter")? {
        o.final_filter = x;
    }
    if let Some(x) = get_f64(op, map, "x_tolerance")? {
        o.x_tolerance = x;
    }
    if let Some(x) = get_f64(op, map, "y_tolerance")? {
        o.y_tolerance = x;
    }
    check_keys(
        op,
        map,
        &[
            "clip",
            "drawings",
            "final_filter",
            "x_tolerance",
            "y_tolerance",
        ],
    )?;
    Ok(o)
}

pub(crate) fn dict_line_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::DictLine> {
    let map = req_obj(op, v)?;
    let f_bbox = get_rect_req(op, map, "bbox")?;
    let f_wmode = get_i32_req(op, map, "wmode")?;
    let f_direction = get_point_req(op, map, "direction")?;
    let f_spans = get_vec_with(op, map, "spans", dict_span_from_json)?;
    check_keys(op, map, &["bbox", "wmode", "direction", "spans"])?;
    Ok(fc::DictLine {
        bbox: f_bbox,
        wmode: f_wmode,
        direction: f_direction,
        spans: f_spans,
    })
}

pub(crate) fn dict_span_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::DictSpan> {
    let map = req_obj(op, v)?;
    let f_bbox = get_rect_req(op, map, "bbox")?;
    let f_origin = get_point_req(op, map, "origin")?;
    let f_font = get_str_req(op, map, "font")?;
    let f_size = get_f64_req(op, map, "size")?;
    let f_color = get_i32_req(op, map, "color")?;
    let f_alpha = get_i32_req(op, map, "alpha")?;
    let f_flags = get_i32_req(op, map, "flags")?;
    let f_bidi = get_i32_req(op, map, "bidi")?;
    let f_char_flags = get_i32_req(op, map, "char_flags")?;
    let f_ascender = get_f64_req(op, map, "ascender")?;
    let f_descender = get_f64_req(op, map, "descender")?;
    let f_text = get_str_req(op, map, "text")?;
    let f_chars = get_vec_with(op, map, "chars", raw_char_from_json)?;
    check_keys(
        op,
        map,
        &[
            "bbox",
            "origin",
            "font",
            "size",
            "color",
            "alpha",
            "flags",
            "bidi",
            "char_flags",
            "ascender",
            "descender",
            "text",
            "chars",
        ],
    )?;
    Ok(fc::DictSpan {
        bbox: f_bbox,
        origin: f_origin,
        font: f_font,
        size: f_size,
        color: f_color,
        alpha: f_alpha,
        flags: f_flags,
        bidi: f_bidi,
        char_flags: f_char_flags,
        ascender: f_ascender,
        descender: f_descender,
        text: f_text,
        chars: f_chars,
    })
}

pub(crate) fn doc_metadata_from_json(
    op: &str,
    v: &serde_json::Value,
) -> napi::Result<fc::DocMetadata> {
    let map = req_obj(op, v)?;
    let f_title = get_str_req(op, map, "title")?;
    let f_author = get_str_req(op, map, "author")?;
    let f_subject = get_str_req(op, map, "subject")?;
    let f_keywords = get_str_req(op, map, "keywords")?;
    let f_creator = get_str_req(op, map, "creator")?;
    let f_producer = get_str_req(op, map, "producer")?;
    let f_creation_date = get_str_req(op, map, "creation_date")?;
    let f_mod_date = get_str_req(op, map, "mod_date")?;
    let f_format = get_str_req(op, map, "format")?;
    let f_encryption = get_str(op, map, "encryption")?;
    let f_trapped = get_str_req(op, map, "trapped")?;
    check_keys(
        op,
        map,
        &[
            "title",
            "author",
            "subject",
            "keywords",
            "creator",
            "producer",
            "creation_date",
            "mod_date",
            "format",
            "encryption",
            "trapped",
        ],
    )?;
    Ok(fc::DocMetadata {
        title: f_title,
        author: f_author,
        subject: f_subject,
        keywords: f_keywords,
        creator: f_creator,
        producer: f_producer,
        creation_date: f_creation_date,
        mod_date: f_mod_date,
        format: f_format,
        encryption: f_encryption,
        trapped: f_trapped,
    })
}

pub(crate) fn draw_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::DrawOptions> {
    let mut o = fc::DrawOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "close_path")? {
        o = o.with_close_path(x);
    }
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_str(op, map, "dashes")? {
        o = o.with_dashes(&x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill")? {
        o = o.with_fill(x);
    }
    if let Some(x) = get_f64(op, map, "fill_opacity")? {
        o = o.with_fill_opacity(x);
    }
    if let Some(x) = get_i32(op, map, "line_cap")? {
        o = o.with_line_cap(x);
    }
    if let Some(x) = get_i32(op, map, "line_join")? {
        o = o.with_line_join(x);
    }
    if let Some(x) = get_vec_f64(op, map, "morph")? {
        o = o.with_morph(x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_f64(op, map, "stroke_opacity")? {
        o = o.with_stroke_opacity(x);
    }
    if let Some(x) = get_f64(op, map, "width")? {
        o = o.with_width(x);
    }
    check_keys(
        op,
        map,
        &[
            "close_path",
            "color",
            "dashes",
            "fill",
            "fill_opacity",
            "line_cap",
            "line_join",
            "morph",
            "oc",
            "overlay",
            "stroke_opacity",
            "width",
        ],
    )?;
    Ok(o)
}

pub(crate) fn emb_file_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::EmbFileOptions> {
    let mut o = fc::EmbFileOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("desc", "description")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    o.description = get_str(op, map, "description")?;
    o.display_name = get_str(op, map, "display_name")?;
    o.filename = get_str(op, map, "filename")?;
    check_keys(
        op,
        map,
        &["desc", "description", "display_name", "filename"],
    )?;
    Ok(o)
}

pub(crate) fn emb_item_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::EmbItem> {
    if let Some(n) = v.as_i64() {
        return Ok(fc::EmbItem::Index(n as i32));
    }
    if let Some(s) = v.as_str() {
        return Ok(fc::EmbItem::Name(s.to_string()));
    }
    Err(wrong_type(
        op,
        format!("{op}: item needs a number or a name"),
    ))
}

pub(crate) fn file_annot_options_from_json(
    op: &str,
    filename: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FileAnnotOptions> {
    let mut o = fc::FileAnnotOptions::new(filename);
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_first(op, map, &["display_name", "ufilename"], get_str)? {
        o = o.with_display_name(&x);
    }
    if let Some(x) = get_first(op, map, &["description", "desc"], get_str)? {
        o = o.with_description(&x);
    }
    if let Some(x) = get_str(op, map, "icon")? {
        o = o.with_icon(&x);
    }
    check_keys(
        op,
        map,
        &["display_name", "ufilename", "description", "desc", "icon"],
    )?;
    Ok(o)
}

pub(crate) fn fill_text_box_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FillTextBoxOptions<'a>> {
    let mut o = fc::FillTextBoxOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(op, map, "font", "pass font as the font argument")?;
    if let Some(x) = get_i32(op, map, "align")? {
        o = o.with_align(x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_f64(op, map, "lineheight")? {
        o = o.with_lineheight(x);
    }
    if let Some(g) = get_point(op, map, "pos")? {
        o = o.with_pos(g);
    }
    if let Some(x) = get_bool(op, map, "right_to_left")? {
        o = o.with_right_to_left(x);
    }
    if let Some(x) = get_bool(op, map, "small_caps")? {
        o = o.with_small_caps(x);
    }
    if let Some(x) = get_bool(op, map, "warn")? {
        o = o.with_warn(x);
    }
    check_keys(
        op,
        map,
        &[
            "align",
            "font",
            "fontsize",
            "lineheight",
            "pos",
            "right_to_left",
            "small_caps",
            "warn",
        ],
    )?;
    Ok(o)
}

pub(crate) fn flush_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FlushOptions> {
    let mut o = fc::FlushOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(g) = get_matrix(op, map, "matrix")? {
        o = o.with_matrix(g);
    }
    if let Some(x) = get_vec_f64(op, map, "morph")? {
        o = o.with_morph(x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_f64(op, map, "opacity")? {
        o = o.with_opacity(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_i32(op, map, "render_mode")? {
        o = o.with_render_mode(x);
    }
    check_keys(
        op,
        map,
        &[
            "color",
            "matrix",
            "morph",
            "oc",
            "opacity",
            "overlay",
            "render_mode",
        ],
    )?;
    Ok(o)
}

pub(crate) fn font_measure_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FontMeasureOptions> {
    let mut o = fc::FontMeasureOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "fallback")? {
        o = o.with_fallback(x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_str(op, map, "language")? {
        o = o.with_language(&x);
    }
    if let Some(x) = get_i32(op, map, "script")? {
        o = o.with_script(x);
    }
    if let Some(x) = get_bool(op, map, "small_caps")? {
        o = o.with_small_caps(x);
    }
    if let Some(x) = get_i32(op, map, "wmode")? {
        o = o.with_wmode(x);
    }
    check_keys(
        op,
        map,
        &[
            "fallback",
            "fontsize",
            "language",
            "script",
            "small_caps",
            "wmode",
        ],
    )?;
    Ok(o)
}

pub(crate) fn font_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FontOptions> {
    let mut o = fc::FontOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "fontbuffer",
        "pass fontbuffer as the fontbuffer argument",
    )?;
    if let Some(x) = get_bool(op, map, "embed")? {
        o = o.with_embed(x);
    }
    if let Some(x) = get_str(op, map, "fontfile")? {
        o = o.with_fontfile(&x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_bool(op, map, "is_bold")? {
        o = o.with_is_bold(x);
    }
    if let Some(x) = get_bool(op, map, "is_italic")? {
        o = o.with_is_italic(x);
    }
    if let Some(x) = get_bool(op, map, "is_serif")? {
        o = o.with_is_serif(x);
    }
    if let Some(x) = get_str(op, map, "language")? {
        o = o.with_language(&x);
    }
    if let Some(x) = get_i32(op, map, "ordering")? {
        o = o.with_ordering(x);
    }
    if let Some(x) = get_i32(op, map, "script")? {
        o = o.with_script(x);
    }
    check_keys(
        op,
        map,
        &[
            "embed",
            "fontbuffer",
            "fontfile",
            "fontname",
            "is_bold",
            "is_italic",
            "is_serif",
            "language",
            "ordering",
            "script",
        ],
    )?;
    Ok(o)
}

pub(crate) fn free_text_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::FreeTextOptions> {
    let mut o = fc::FreeTextOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "align")? {
        o = o.with_align(x);
    }
    if let Some(x) = get_vec_f64(op, map, "border_color")? {
        o = o.with_border_color(x);
    }
    if let Some(x) = get_f64(op, map, "border_width")? {
        o = o.with_border_width(x);
    }
    if let Some(v) = get_vec_point(op, map, "callout")? {
        o = o.with_callout(v);
    }
    if let Some(x) = get_vec_f64(op, map, "dashes")? {
        o = o.with_dashes(x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill_color")? {
        o = o.with_fill_color(x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_i32(op, map, "line_end")? {
        o = o.with_line_end(x);
    }
    if let Some(x) = get_f64(op, map, "opacity")? {
        o = o.with_opacity(x);
    }
    if let Some(x) = get_bool(op, map, "richtext")? {
        o = o.with_richtext(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(val) = find_key(map, "style") {
        o = o.with_style(json_to_compat(val));
    }
    if let Some(x) = get_vec_f64(op, map, "text_color")? {
        o = o.with_text_color(x);
    }
    check_keys(
        op,
        map,
        &[
            "align",
            "border_color",
            "border_width",
            "callout",
            "dashes",
            "fill_color",
            "fontname",
            "fontsize",
            "line_end",
            "opacity",
            "richtext",
            "rotate",
            "style",
            "text_color",
        ],
    )?;
    Ok(o)
}

pub(crate) fn highlight_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::HighlightOptions> {
    let mut o = fc::HighlightOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(g) = get_rect(op, map, "clip")? {
        o.clip = Some(g);
    }
    o.quads = get_vec_quad(op, map, "quads")?;
    if let Some(g) = get_point(op, map, "start")? {
        o.start = Some(g);
    }
    if let Some(g) = get_point(op, map, "stop")? {
        o.stop = Some(g);
    }
    check_keys(op, map, &["clip", "quads", "start", "stop"])?;
    Ok(o)
}

pub(crate) fn html_box_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::HtmlBoxOptions> {
    let mut o = fc::HtmlBoxOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(val) = find_key(map, "archive") {
        o = o.with_archive(json_to_compat(val));
    }
    if let Some(x) = get_str(op, map, "css")? {
        o = o.with_css(&x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_f64(op, map, "opacity")? {
        o = o.with_opacity(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(x) = get_f64(op, map, "scale_low")? {
        o = o.with_scale_low(x);
    }
    check_keys(
        op,
        map,
        &[
            "archive",
            "css",
            "oc",
            "opacity",
            "overlay",
            "rotate",
            "scale_low",
        ],
    )?;
    Ok(o)
}

pub(crate) fn image_insert_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ImageInsertOptions> {
    let mut o = fc::ImageInsertOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(op, map, "pixmap", "pass pixmap as the pixmap argument")?;
    forbid_key(op, map, "stream", "pass stream as the stream argument")?;
    let width = get_f64(op, map, "width")?;
    let height = get_f64(op, map, "height")?;
    if width.is_some() || height.is_some() {
        let cur_width = o.width;
        let cur_height = o.height;
        o = o.with_size(width.unwrap_or(cur_width), height.unwrap_or(cur_height));
    }
    if let Some(x) = get_i32(op, map, "alpha")? {
        o = o.with_alpha(x);
    }
    if let Some(x) = get_str(op, map, "filename")? {
        o = o.with_filename(&x);
    }
    if let Some(x) = get_bool(op, map, "keep_proportion")? {
        o = o.with_keep_proportion(x);
    }
    if let Some(val) = find_key(map, "mask") {
        o = o.with_mask(json_to_compat(val));
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(x) = get_i32(op, map, "xref")? {
        o = o.with_xref(x);
    }
    check_keys(
        op,
        map,
        &[
            "alpha",
            "filename",
            "height",
            "keep_proportion",
            "mask",
            "oc",
            "overlay",
            "pixmap",
            "rotate",
            "stream",
            "width",
            "xref",
        ],
    )?;
    Ok(o)
}

pub(crate) fn insert_font_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::InsertFontOptions> {
    let mut o = fc::InsertFontOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "encoding")? {
        o.encoding = x;
    }
    o.fontbuffer = get_vec_u8(op, map, "fontbuffer")?;
    o.fontfile = get_str(op, map, "fontfile")?;
    if let Some(x) = get_str(op, map, "fontname")? {
        o.fontname = x;
    }
    if let Some(x) = get_bool(op, map, "set_simple")? {
        o.set_simple = x;
    }
    if let Some(x) = get_i32(op, map, "wmode")? {
        o.wmode = x;
    }
    check_keys(
        op,
        map,
        &[
            "encoding",
            "fontbuffer",
            "fontfile",
            "fontname",
            "set_simple",
            "wmode",
        ],
    )?;
    Ok(o)
}

pub(crate) fn insert_page_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::InsertPageOptions> {
    let mut o = fc::InsertPageOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let width = get_f64(op, map, "width")?;
    let height = get_f64(op, map, "height")?;
    if width.is_some() || height.is_some() {
        let cur_width = o.width;
        let cur_height = o.height;
        o = o.with_size(width.unwrap_or(cur_width), height.unwrap_or(cur_height));
    }
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_str(op, map, "fontfile")? {
        o = o.with_fontfile(&x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_vec_string(op, map, "text")? {
        o = o.with_text(x);
    }
    check_keys(
        op,
        map,
        &[
            "color", "fontfile", "fontname", "fontsize", "height", "text", "width",
        ],
    )?;
    Ok(o)
}

pub(crate) fn insert_pdf_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::InsertPdfOptions> {
    let mut o = fc::InsertPdfOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("final", "finish")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    let from_page = get_i32(op, map, "from_page")?;
    let to_page = get_i32(op, map, "to_page")?;
    if from_page.is_some() || to_page.is_some() {
        let cur_from_page = o.from_page;
        let cur_to_page = o.to_page;
        o = o.with_span(
            from_page.unwrap_or(cur_from_page),
            to_page.unwrap_or(cur_to_page),
        );
    }
    if let Some(x) = get_bool(op, map, "annots")? {
        o = o.with_annots(x);
    }
    if let Some(x) = get_bool(op, map, "finish")? {
        o = o.with_finish(x);
    }
    if let Some(x) = get_bool(op, map, "join_duplicates")? {
        o = o.with_join_duplicates(x);
    }
    if let Some(x) = get_bool(op, map, "links")? {
        o = o.with_links(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(x) = get_bool(op, map, "show_progress")? {
        o = o.with_show_progress(x);
    }
    if let Some(x) = get_i32(op, map, "start_at")? {
        o = o.with_start_at(x);
    }
    if let Some(x) = get_bool(op, map, "widgets")? {
        o = o.with_widgets(x);
    }
    check_keys(
        op,
        map,
        &[
            "annots",
            "final",
            "finish",
            "from_page",
            "join_duplicates",
            "links",
            "rotate",
            "show_progress",
            "start_at",
            "to_page",
            "widgets",
        ],
    )?;
    Ok(o)
}

pub(crate) fn insert_text_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::InsertTextOptions> {
    let mut o = fc::InsertTextOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_f64(op, map, "border_width")? {
        o = o.with_border_width(x);
    }
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_i32(op, map, "encoding")? {
        o = o.with_encoding(x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill")? {
        o = o.with_fill(x);
    }
    if let Some(x) = get_f64(op, map, "fill_opacity")? {
        o = o.with_fill_opacity(x);
    }
    if let Some(x) = get_str(op, map, "fontfile")? {
        o = o.with_fontfile(&x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_f64(op, map, "lineheight")? {
        o = o.with_lineheight(x);
    }
    if let Some(x) = get_f64(op, map, "miter_limit")? {
        o = o.with_miter_limit(x);
    }
    if let Some(x) = get_vec_f64(op, map, "morph")? {
        o = o.with_morph(x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_i32(op, map, "render_mode")? {
        o = o.with_render_mode(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    if let Some(x) = get_bool(op, map, "set_simple")? {
        o = o.with_set_simple(x);
    }
    if let Some(x) = get_f64(op, map, "stroke_opacity")? {
        o = o.with_stroke_opacity(x);
    }
    check_keys(
        op,
        map,
        &[
            "border_width",
            "color",
            "encoding",
            "fill",
            "fill_opacity",
            "fontfile",
            "fontname",
            "fontsize",
            "lineheight",
            "miter_limit",
            "morph",
            "oc",
            "overlay",
            "render_mode",
            "rotate",
            "set_simple",
            "stroke_opacity",
        ],
    )?;
    Ok(o)
}

pub(crate) fn layer_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::LayerOptions> {
    let mut o = fc::LayerOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("basestate", "base_state")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    o.base_state = get_str(op, map, "base_state")?;
    if let Some(x) = get_vec_i32(op, map, "off")? {
        o.off = x;
    }
    if let Some(x) = get_vec_i32(op, map, "on")? {
        o.on = x;
    }
    check_keys(op, map, &["base_state", "basestate", "off", "on"])?;
    Ok(o)
}

pub(crate) fn layout_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::LayoutOptions> {
    let o = fc::LayoutOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if !map.is_empty() {
        let keys: Vec<&str> = map.keys().map(String::as_str).collect();
        return Err(wrong_type(
            op,
            format!("{op}: LayoutOptions options {keys:?} are not settable in this build"),
        ));
    }
    Ok(o)
}

pub(crate) fn log_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::LogOptions> {
    let mut o = fc::LogOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("path_append", "append")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    if let Some(x) = get_bool(op, map, "append")? {
        o.append = x;
    }
    o.path = get_str(op, map, "path")?;
    o.text = get_str(op, map, "text")?;
    check_keys(op, map, &["append", "path", "path_append", "text"])?;
    Ok(o)
}

pub(crate) fn ocmd_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::OcmdOptions> {
    let mut o = fc::OcmdOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_vec_i32(op, map, "ocgs")? {
        o.ocgs = x;
    }
    o.policy = get_str(op, map, "policy")?;
    check_keys(op, map, &["ocgs", "policy"])?;
    Ok(o)
}

pub(crate) fn ocr_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::OcrOptions> {
    let mut o = fc::OcrOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    o.language = get_str(op, map, "language")?;
    o.tessdata = get_str(op, map, "tessdata")?;
    check_keys(op, map, &["language", "tessdata"])?;
    Ok(o)
}

pub(crate) fn page_label_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::PageLabel> {
    let map = req_obj(op, v)?;
    let f_start_page = get_i32_req(op, map, "start_page")?;
    let f_prefix = get_str_req(op, map, "prefix")?;
    let f_first_number = get_i32_req(op, map, "first_number")?;
    let f_style = get_str_req(op, map, "style")?;
    check_keys(op, map, &["start_page", "prefix", "first_number", "style"])?;
    Ok(fc::PageLabel {
        start_page: f_start_page,
        prefix: f_prefix,
        first_number: f_first_number,
        style: f_style,
    })
}

pub(crate) fn page_merge_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::PageMergeOptions> {
    let mut o = fc::PageMergeOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "copy_annots")? {
        o.copy_annots = x;
    }
    if let Some(val) = find_key(map, "graft_map") {
        o.graft_map = json_to_compat(val);
    }
    if let Some(x) = get_bool(op, map, "links")? {
        o.links = x;
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o.rotate = x;
    }
    check_keys(op, map, &["copy_annots", "graft_map", "links", "rotate"])?;
    Ok(o)
}

pub(crate) fn page_selection_from_json(
    op: &str,
    v: &serde_json::Value,
) -> napi::Result<fc::PageSelection> {
    if let Some(n) = v.as_i64() {
        return Ok(fc::PageSelection::List(vec![n as i32]));
    }
    if let Some(items) = v.as_array() {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(as_i32(op, item, "pages")?);
        }
        return Ok(fc::PageSelection::List(out));
    }
    if let Some(map) = v.as_object() {
        let a = get_first(op, map, &["start", "from", "from_page"], get_i32)?;
        let b = get_first(op, map, &["stop", "to", "to_page"], get_i32)?;
        if let (Some(a), Some(b)) = (a, b) {
            check_keys(
                op,
                map,
                &["start", "from", "from_page", "stop", "to", "to_page"],
            )?;
            return Ok(fc::PageSelection::Range(a, b));
        }
    }
    Err(wrong_type(
        op,
        format!("{op}: pages needs a number, an array, or a start/stop object"),
    ))
}

pub(crate) fn raw_char_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::RawChar> {
    let map = req_obj(op, v)?;
    let f_bbox = get_rect_req(op, map, "bbox")?;
    let f_ch = get_char_req(op, map, "ch")?;
    let f_origin = get_point_req(op, map, "origin")?;
    let f_synthetic = get_bool_req(op, map, "synthetic")?;
    check_keys(op, map, &["bbox", "ch", "origin", "synthetic"])?;
    Ok(fc::RawChar {
        bbox: f_bbox,
        ch: f_ch,
        origin: f_origin,
        synthetic: f_synthetic,
    })
}

pub(crate) fn redact_annot_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::RedactAnnotOptions> {
    let mut o = fc::RedactAnnotOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "align")? {
        o = o.with_align(x);
    }
    if let Some(x) = get_bool(op, map, "cross_out")? {
        o = o.with_cross_out(x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill")? {
        o = o.with_fill(x);
    }
    if let Some(x) = get_str(op, map, "fontname")? {
        o = o.with_fontname(&x);
    }
    if let Some(x) = get_f64(op, map, "fontsize")? {
        o = o.with_fontsize(x);
    }
    if let Some(x) = get_str(op, map, "text")? {
        o = o.with_text(&x);
    }
    if let Some(x) = get_vec_f64(op, map, "text_color")? {
        o = o.with_text_color(x);
    }
    check_keys(
        op,
        map,
        &[
            "align",
            "cross_out",
            "fill",
            "fontname",
            "fontsize",
            "text",
            "text_color",
        ],
    )?;
    Ok(o)
}

pub(crate) fn redact_apply_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::RedactApplyOptions> {
    let mut o = fc::RedactApplyOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_i32(op, map, "graphics")? {
        o.graphics = x;
    }
    if let Some(x) = get_i32(op, map, "images")? {
        o.images = x;
    }
    if let Some(x) = get_i32(op, map, "text")? {
        o.text = x;
    }
    check_keys(op, map, &["graphics", "images", "text"])?;
    Ok(o)
}

pub(crate) fn render_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::RenderOptions> {
    let mut o = fc::RenderOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "colorspace",
        "pass colorspace as the colorspace argument",
    )?;
    if let Some(x) = get_bool(op, map, "alpha")? {
        o = o.with_alpha(x);
    }
    if let Some(x) = get_bool(op, map, "annots")? {
        o = o.with_annots(x);
    }
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_f64(op, map, "dpi")? {
        o = o.with_dpi(x);
    }
    if let Some(g) = get_matrix(op, map, "matrix")? {
        o = o.with_matrix(g);
    }
    check_keys(
        op,
        map,
        &["alpha", "annots", "clip", "colorspace", "dpi", "matrix"],
    )?;
    Ok(o)
}

pub(crate) fn replace_image_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ReplaceImageOptions> {
    let mut o = fc::ReplaceImageOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "pixmap",
        "pass pixmap through replaceImage is not supported in this build",
    )?;
    o.filename = get_str(op, map, "filename")?;
    o.stream = get_vec_u8(op, map, "stream")?;
    check_keys(op, map, &["filename", "pixmap", "stream"])?;
    Ok(o)
}

pub(crate) fn rewrite_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::RewriteOptions> {
    let mut o = fc::RewriteOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "bitonal")? {
        o = o.with_bitonal(x);
    }
    if let Some(x) = get_bool(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_i32(op, map, "dpi_target")? {
        o = o.with_dpi_target(x);
    }
    if let Some(x) = get_i32(op, map, "dpi_threshold")? {
        o = o.with_dpi_threshold(x);
    }
    if let Some(x) = get_bool(op, map, "gray")? {
        o = o.with_gray(x);
    }
    if let Some(x) = get_bool(op, map, "lossless")? {
        o = o.with_lossless(x);
    }
    if let Some(x) = get_bool(op, map, "lossy")? {
        o = o.with_lossy(x);
    }
    if let Some(x) = get_i32(op, map, "quality")? {
        o = o.with_quality(x);
    }
    if let Some(x) = get_bool(op, map, "set_to_gray")? {
        o = o.with_set_to_gray(x);
    }
    check_keys(
        op,
        map,
        &[
            "bitonal",
            "color",
            "dpi_target",
            "dpi_threshold",
            "gray",
            "lossless",
            "lossy",
            "quality",
            "set_to_gray",
        ],
    )?;
    Ok(o)
}

pub(crate) fn save_settings_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::SaveSettings> {
    let mut o = fc::SaveSettings::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("owner_pw", "owner_password"), ("user_pw", "user_password")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    if let Some(x) = get_bool(op, map, "appearance")? {
        o = o.with_appearance(x);
    }
    if let Some(x) = get_bool(op, map, "ascii")? {
        o = o.with_ascii(x);
    }
    if let Some(x) = get_bool(op, map, "clean")? {
        o = o.with_clean(x);
    }
    if let Some(x) = get_i32(op, map, "compression_effort")? {
        o = o.with_compression_effort(x);
    }
    if let Some(x) = get_bool(op, map, "deflate")? {
        o = o.with_deflate(x);
    }
    if let Some(x) = get_bool(op, map, "deflate_fonts")? {
        o = o.with_deflate_fonts(x);
    }
    if let Some(x) = get_bool(op, map, "deflate_images")? {
        o = o.with_deflate_images(x);
    }
    if let Some(x) = get_i32(op, map, "encryption")? {
        o = o.with_encryption(x);
    }
    if let Some(x) = get_bool(op, map, "expand")? {
        o = o.with_expand(x);
    }
    if let Some(x) = get_i32(op, map, "garbage")? {
        o = o.with_garbage(x);
    }
    if let Some(x) = get_bool(op, map, "incremental")? {
        o = o.with_incremental(x);
    }
    if let Some(x) = get_bool(op, map, "linear")? {
        o = o.with_linear(x);
    }
    if let Some(x) = get_bool(op, map, "no_new_id")? {
        o = o.with_no_new_id(x);
    }
    if let Some(x) = get_str(op, map, "owner_password")? {
        o = o.with_owner_password(&x);
    }
    if let Some(x) = get_i32(op, map, "permissions")? {
        o = o.with_permissions(x);
    }
    if let Some(x) = get_bool(op, map, "preserve_metadata")? {
        o = o.with_preserve_metadata(x);
    }
    if let Some(x) = get_bool(op, map, "pretty")? {
        o = o.with_pretty(x);
    }
    if let Some(x) = get_bool(op, map, "raise_on_repair")? {
        o = o.with_raise_on_repair(x);
    }
    if let Some(x) = get_bool(op, map, "reproducible")? {
        o = o.with_reproducible(x);
    }
    if let Some(x) = get_bool(op, map, "use_objstms")? {
        o = o.with_use_objstms(x);
    }
    if let Some(x) = get_str(op, map, "user_password")? {
        o = o.with_user_password(&x);
    }
    check_keys(
        op,
        map,
        &[
            "appearance",
            "ascii",
            "clean",
            "compression_effort",
            "deflate",
            "deflate_fonts",
            "deflate_images",
            "encryption",
            "expand",
            "garbage",
            "incremental",
            "linear",
            "no_new_id",
            "owner_password",
            "owner_pw",
            "permissions",
            "preserve_metadata",
            "pretty",
            "raise_on_repair",
            "reproducible",
            "use_objstms",
            "user_password",
            "user_pw",
        ],
    )?;
    Ok(o)
}

pub(crate) fn scrub_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ScrubOptions> {
    let mut o = fc::ScrubOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "attached_files")? {
        o = o.with_attached_files(x);
    }
    if let Some(x) = get_bool(op, map, "clean_pages")? {
        o = o.with_clean_pages(x);
    }
    if let Some(x) = get_bool(op, map, "embedded_files")? {
        o = o.with_embedded_files(x);
    }
    if let Some(x) = get_bool(op, map, "hidden_text")? {
        o = o.with_hidden_text(x);
    }
    if let Some(x) = get_bool(op, map, "javascript")? {
        o = o.with_javascript(x);
    }
    if let Some(x) = get_bool(op, map, "metadata")? {
        o = o.with_metadata(x);
    }
    if let Some(x) = get_i32(op, map, "redact_images")? {
        o = o.with_redact_images(x);
    }
    if let Some(x) = get_bool(op, map, "redactions")? {
        o = o.with_redactions(x);
    }
    if let Some(x) = get_bool(op, map, "remove_links")? {
        o = o.with_remove_links(x);
    }
    if let Some(x) = get_bool(op, map, "reset_fields")? {
        o = o.with_reset_fields(x);
    }
    if let Some(x) = get_bool(op, map, "reset_responses")? {
        o = o.with_reset_responses(x);
    }
    if let Some(x) = get_bool(op, map, "thumbnails")? {
        o = o.with_thumbnails(x);
    }
    if let Some(x) = get_bool(op, map, "xml_metadata")? {
        o = o.with_xml_metadata(x);
    }
    check_keys(
        op,
        map,
        &[
            "attached_files",
            "clean_pages",
            "embedded_files",
            "hidden_text",
            "javascript",
            "metadata",
            "redact_images",
            "redactions",
            "remove_links",
            "reset_fields",
            "reset_responses",
            "thumbnails",
            "xml_metadata",
        ],
    )?;
    Ok(o)
}

pub(crate) fn search_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::SearchOptions<'a>> {
    let mut o = fc::SearchOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "textpage",
        "pass textpage as the textpage argument",
    )?;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_i32(op, map, "flags")? {
        o = o.with_flags(x);
    }
    if let Some(x) = get_i32(op, map, "limit")? {
        o = o.with_limit(x);
    }
    if let Some(x) = get_bool(op, map, "quads")? {
        o = o.with_quads(x);
    }
    check_keys(op, map, &["clip", "flags", "limit", "quads", "textpage"])?;
    Ok(o)
}

pub(crate) fn shape_finish_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ShapeFinishOptions> {
    let mut o = fc::ShapeFinishOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_bool(op, map, "close_path")? {
        o = o.with_close_path(x);
    }
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_str(op, map, "dashes")? {
        o = o.with_dashes(&x);
    }
    if let Some(x) = get_bool(op, map, "even_odd")? {
        o = o.with_even_odd(x);
    }
    if let Some(x) = get_vec_f64(op, map, "fill")? {
        o = o.with_fill(x);
    }
    if let Some(x) = get_f64(op, map, "fill_opacity")? {
        o = o.with_fill_opacity(x);
    }
    if let Some(x) = get_i32(op, map, "line_cap")? {
        o = o.with_line_cap(x);
    }
    if let Some(x) = get_i32(op, map, "line_join")? {
        o = o.with_line_join(x);
    }
    if let Some(x) = get_vec_f64(op, map, "morph")? {
        o = o.with_morph(x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_f64(op, map, "stroke_opacity")? {
        o = o.with_stroke_opacity(x);
    }
    if let Some(x) = get_f64(op, map, "width")? {
        o = o.with_width(x);
    }
    check_keys(
        op,
        map,
        &[
            "close_path",
            "color",
            "dashes",
            "even_odd",
            "fill",
            "fill_opacity",
            "line_cap",
            "line_join",
            "morph",
            "oc",
            "stroke_opacity",
            "width",
        ],
    )?;
    Ok(o)
}

pub(crate) fn show_page_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::ShowPageOptions> {
    let mut o = fc::ShowPageOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("pno", "page_no")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_bool(op, map, "keep_proportion")? {
        o = o.with_keep_proportion(x);
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o = o.with_oc(x);
    }
    if let Some(x) = get_bool(op, map, "overlay")? {
        o = o.with_overlay(x);
    }
    if let Some(x) = get_i32(op, map, "page_no")? {
        o = o.with_page_no(x);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o = o.with_rotate(x);
    }
    check_keys(
        op,
        map,
        &[
            "clip",
            "keep_proportion",
            "oc",
            "overlay",
            "page_no",
            "pno",
            "rotate",
        ],
    )?;
    Ok(o)
}

pub(crate) fn table_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::TableOptions> {
    let o = fc::TableOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if !map.is_empty() {
        let keys: Vec<&str> = map.keys().map(String::as_str).collect();
        return Err(wrong_type(
            op,
            format!("{op}: TableOptions options {keys:?} are not settable in this build"),
        ));
    }
    Ok(o)
}

pub(crate) fn text_arg_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::TextArg> {
    if let Some(s) = v.as_str() {
        return Ok(fc::TextArg::One(s.to_string()));
    }
    if let Some(items) = v.as_array() {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(as_str(op, item, "text")?);
        }
        return Ok(fc::TextArg::Many(out));
    }
    Err(wrong_type(
        op,
        format!("{op}: text needs a string or an array of strings"),
    ))
}

pub(crate) fn text_box_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::TextBoxOptions> {
    let mut o = fc::TextBoxOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(op, map, "text", "pass text as the text argument")?;
    let mut sub = serde_json::Map::new();
    for k in [
        "fontsize",
        "lineheight",
        "fontname",
        "fontfile",
        "set_simple",
        "encoding",
        "color",
        "fill",
        "border_width",
        "miter_limit",
        "render_mode",
        "rotate",
        "morph",
        "overlay",
        "stroke_opacity",
        "fill_opacity",
        "oc",
    ] {
        if let Some(val) = find_key(map, k) {
            sub.insert(k.to_string(), val.clone());
        }
    }
    if !sub.is_empty() {
        let sub_v = serde_json::Value::Object(sub);
        o = o.with_text(insert_text_options_from_json(op, Some(&sub_v))?);
    }
    if let Some(x) = get_i32(op, map, "align")? {
        o = o.with_align(x);
    }
    if let Some(x) = get_i32(op, map, "expandtabs")? {
        o = o.with_expandtabs(x);
    }
    check_keys(
        op,
        map,
        &[
            "align",
            "border_width",
            "color",
            "encoding",
            "expandtabs",
            "fill",
            "fill_opacity",
            "fontfile",
            "fontname",
            "fontsize",
            "lineheight",
            "miter_limit",
            "morph",
            "oc",
            "overlay",
            "render_mode",
            "rotate",
            "set_simple",
            "stroke_opacity",
            "text",
        ],
    )?;
    Ok(o)
}

pub(crate) fn text_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::TextOptions<'a>> {
    let mut o = fc::TextOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("option", "format")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    forbid_key(
        op,
        map,
        "textpage",
        "pass textpage as the textpage argument",
    )?;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_i32(op, map, "flags")? {
        o = o.with_flags(x);
    }
    if let Some(fmt) = get_format(op, map, &["format", "option"])? {
        o = o.with_format(fmt);
    }
    if let Some(x) = get_bool(op, map, "sort")? {
        o = o.with_sort(x);
    }
    check_keys(
        op,
        map,
        &["clip", "flags", "format", "option", "sort", "textpage"],
    )?;
    Ok(o)
}

pub(crate) fn text_writer_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::TextWriterOptions> {
    let mut o = fc::TextWriterOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    if let Some(x) = get_vec_f64(op, map, "color")? {
        o = o.with_color(x);
    }
    if let Some(x) = get_f64(op, map, "opacity")? {
        o = o.with_opacity(x);
    }
    check_keys(op, map, &["color", "opacity"])?;
    Ok(o)
}

pub(crate) fn toc_entry_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::TocEntry> {
    if let Some(row) = v.as_array() {
        if row.len() == 3 {
            let level = as_i32(op, &row[0], "level")?;
            let title = as_str(op, &row[1], "title")?;
            let page = as_i32(op, &row[2], "page")?;
            return Ok(fc::TocEntry {
                level,
                title,
                page,
                dest: None,
            });
        }
        return Err(wrong_type(
            op,
            format!("{op}: toc rows need [level, title, page]"),
        ));
    }
    let map = req_obj(op, v)?;
    let level = get_i32_req(op, map, "level")?;
    let title = get_str_req(op, map, "title")?;
    let page = get_i32_req(op, map, "page")?;
    let dest = get_opt_with(op, map, "dest", toc_dest_from_json)?;
    check_keys(op, map, &["level", "title", "page", "dest"])?;
    Ok(fc::TocEntry {
        level,
        title,
        page,
        dest,
    })
}

pub(crate) fn toc_item_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::TocItemOptions> {
    let mut o = fc::TocItemOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    let mut owned = map.clone();
    for (alias, field) in [("pno", "page"), ("to", "target")] {
        if owned.get(field).is_none() {
            if let Some(v) = find_key(&owned, alias).cloned() {
                owned.insert(field.to_string(), v);
            }
        }
    }
    let map = &owned;
    o.filename = get_str(op, map, "filename")?;
    o.kind = get_i32(op, map, "kind")?;
    o.page = get_i32(op, map, "page")?;
    if let Some(g) = get_point(op, map, "target")? {
        o.target = Some(g);
    }
    o.title = get_str(op, map, "title")?;
    o.uri = get_str(op, map, "uri")?;
    if let Some(x) = get_f64(op, map, "zoom")? {
        o.zoom = x;
    }
    check_keys(
        op,
        map,
        &[
            "filename", "kind", "page", "pno", "target", "title", "to", "uri", "zoom",
        ],
    )?;
    Ok(o)
}

pub(crate) fn word_options_from_json<'a>(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::WordOptions<'a>> {
    let mut o = fc::WordOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    forbid_key(
        op,
        map,
        "textpage",
        "pass textpage as the textpage argument",
    )?;
    if let Some(g) = get_rect(op, map, "clip")? {
        o = o.with_clip(g);
    }
    if let Some(x) = get_str(op, map, "delimiters")? {
        o = o.with_delimiters(&x);
    }
    if let Some(x) = get_i32(op, map, "flags")? {
        o = o.with_flags(x);
    }
    if let Some(x) = get_bool(op, map, "sort")? {
        o = o.with_sort(x);
    }
    check_keys(
        op,
        map,
        &["clip", "delimiters", "flags", "sort", "textpage"],
    )?;
    Ok(o)
}

pub(crate) fn write_text_options_from_json(
    op: &str,
    v: Option<&serde_json::Value>,
) -> napi::Result<fc::WriteTextOptions> {
    let mut o = fc::WriteTextOptions::default();
    let Some(v) = v else {
        return Ok(o);
    };
    let map = opt_obj(op, Some(v))?.expect("object checked");
    o.color = get_vec_f64(op, map, "color")?;
    if let Some(x) = get_bool(op, map, "keep_proportion")? {
        o.keep_proportion = x;
    }
    if let Some(x) = get_i32(op, map, "oc")? {
        o.oc = x;
    }
    o.opacity = get_f64(op, map, "opacity")?;
    if let Some(x) = get_bool(op, map, "overlay")? {
        o.overlay = x;
    }
    if let Some(g) = get_rect(op, map, "rect")? {
        o.rect = Some(g);
    }
    if let Some(x) = get_i32(op, map, "rotate")? {
        o.rotate = x;
    }
    if let Some(val) = find_key(map, "writers") {
        if !val.is_null() {
            o.writers = Some(json_to_compat(val));
        }
    }
    check_keys(
        op,
        map,
        &[
            "color",
            "keep_proportion",
            "oc",
            "opacity",
            "overlay",
            "rect",
            "rotate",
            "writers",
        ],
    )?;
    Ok(o)
}

pub(crate) fn annot_info_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::AnnotInfo> {
    let map = req_obj(op, v)?;
    let f_content = get_str_req(op, map, "content")?;
    let f_creation_date = get_str_req(op, map, "creation_date")?;
    let f_mod_date = get_str_req(op, map, "mod_date")?;
    let f_name = get_str_req(op, map, "name")?;
    let f_subject = get_str_req(op, map, "subject")?;
    let f_title = get_str_req(op, map, "title")?;
    let f_id = get_str_req(op, map, "id")?;
    check_keys(
        op,
        map,
        &[
            "content",
            "creation_date",
            "mod_date",
            "name",
            "subject",
            "title",
            "id",
        ],
    )?;
    Ok(fc::AnnotInfo {
        content: f_content,
        creation_date: f_creation_date,
        mod_date: f_mod_date,
        name: f_name,
        subject: f_subject,
        title: f_title,
        id: f_id,
    })
}

pub(crate) fn toc_dest_from_json(op: &str, v: &serde_json::Value) -> napi::Result<fc::TocDest> {
    let map = req_obj(op, v)?;
    let f_kind = get_i32_req(op, map, "kind")?;
    let f_page = get_i32_req(op, map, "page")?;
    let f_target = get_point_req(op, map, "target")?;
    let f_zoom = get_f64_req(op, map, "zoom")?;
    check_keys(op, map, &["kind", "page", "target", "zoom"])?;
    Ok(fc::TocDest {
        kind: f_kind,
        page: f_page,
        target: f_target,
        zoom: f_zoom,
    })
}

pub(crate) fn annot_colors_to_json(v: &fc::AnnotColors) -> serde_json::Value {
    serde_json::json!({
        "stroke": v.stroke,
        "fill": v.fill,
    })
}

pub(crate) fn annot_entry_to_json(v: &fc::AnnotEntry) -> serde_json::Value {
    serde_json::json!({
        "xref": v.xref,
        "type": annot_type_to_json(&v.annot_type),
        "rect": geom_rect(&v.rect),
    })
}

pub(crate) fn annot_info_to_json(v: &fc::AnnotInfo) -> serde_json::Value {
    serde_json::json!({
        "content": v.content,
        "creationDate": v.creation_date,
        "modDate": v.mod_date,
        "name": v.name,
        "subject": v.subject,
        "title": v.title,
        "id": v.id,
    })
}

pub(crate) fn annot_type_to_json(v: &fc::AnnotType) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.code), serde_json::json!(v.name)])
}

pub(crate) fn annot_xref_to_json(v: &fc::AnnotXref) -> serde_json::Value {
    serde_json::json!({
        "xref": v.xref,
        "kindCode": v.kind_code,
        "name": v.name,
    })
}

pub(crate) fn border_to_json(v: &fc::Border) -> serde_json::Value {
    serde_json::json!({
        "width": v.width,
        "style": v.style,
        "dashes": v.dashes,
        "clouds": v.clouds,
    })
}

pub(crate) fn drawing_to_json(v: &fc::Drawing) -> napi::Result<serde_json::Value> {
    let items = v
        .items
        .iter()
        .map(drawing_item_to_json)
        .collect::<napi::Result<Vec<_>>>()?;
    Ok(serde_json::json!({
        "rect": geom_rect(&v.rect),
        "seqno": v.seqno,
        "type": v.drawing_type,
        "color": v.color,
        "fill": v.fill,
        "width": v.width,
        "lineCap": v.line_cap,
        "lineJoin": v.line_join,
        "dashes": v.dashes,
        "closePath": v.close_path,
        "even_odd": v.even_odd,
        "fill_opacity": v.fill_opacity,
        "stroke_opacity": v.stroke_opacity,
        "layer": v.layer,
        "level": v.level,
        "items": json_array(items),
    }))
}

pub(crate) fn drawing_cluster_to_json(v: &fc::DrawingCluster) -> napi::Result<serde_json::Value> {
    let drawings = v
        .drawings
        .iter()
        .map(drawing_to_json)
        .collect::<napi::Result<Vec<_>>>()?;
    Ok(serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "drawings": json_array(drawings),
    }))
}

pub(crate) fn embedded_file_to_json(v: &fc::EmbeddedFile) -> serde_json::Value {
    serde_json::json!({
        "name": v.name,
        "filename": v.filename,
        "description": v.description,
        "size": (v.size as f64),
    })
}

pub(crate) fn file_info_to_json(v: &fc::FileInfo) -> serde_json::Value {
    serde_json::json!({
        "filename": v.filename,
        "displayName": v.display_name,
        "description": v.description,
        "size": (v.size as f64),
    })
}

pub(crate) fn font_data_to_json(v: &fc::FontData) -> serde_json::Value {
    serde_json::json!({
        "name": v.name,
        "data": bytes_json(&v.data),
    })
}

pub(crate) fn font_flags_to_json(v: &fc::FontFlags) -> serde_json::Value {
    serde_json::json!({
        "bold": v.bold,
        "cjk": v.cjk,
        "cjkLang": v.cjk_lang,
        "embed": v.embed,
        "fakeBold": v.fake_bold,
        "fakeItalic": v.fake_italic,
        "invalidBbox": v.invalid_bbox,
        "italic": v.italic,
        "mono": v.mono,
        "neverEmbed": v.never_embed,
        "opentype": v.opentype,
        "serif": v.serif,
        "stretch": v.stretch,
        "substitute": v.substitute,
    })
}

pub(crate) fn font_info_to_json(v: &fc::FontInfo) -> serde_json::Value {
    serde_json::json!({
        "xref": v.xref,
        "name": v.name,
    })
}

pub(crate) fn font_subset_report_to_json(v: &pdfluent::FontSubsetReport) -> serde_json::Value {
    serde_json::json!({
        "fontsProcessed": (v.fonts_processed as f64),
        "fontsSubsetted": (v.fonts_subsetted as f64),
        "bytesSaved": (v.bytes_saved as f64),
    })
}

pub(crate) fn image_data_to_json(v: &fc::ImageData) -> serde_json::Value {
    serde_json::json!({
        "bpc": v.bits,
        "colorspace": v.colorspace,
        "cs-name": v.colorspace_name,
        "ext": v.ext,
        "height": v.height,
        "image": bytes_json(&v.data),
        "size": v.size,
        "smask": if v.masked { 1 } else { 0 },
        "width": v.width,
        "xres": v.xres,
        "yres": v.yres,
    })
}

pub(crate) fn image_profile_to_json(v: &fc::ImageProfile) -> serde_json::Value {
    serde_json::json!({
        "width": v.width,
        "height": v.height,
        "ext": v.ext,
    })
}

pub(crate) fn image_ref_to_json(v: &fc::ImageRef) -> serde_json::Value {
    serde_json::Value::Array(vec![
        serde_json::json!(v.xref),
        serde_json::json!(v.mask),
        serde_json::json!(v.width),
        serde_json::json!(v.height),
        serde_json::json!(v.bits),
        serde_json::json!(v.colorspace_name),
        serde_json::json!(v.alt),
        serde_json::json!(v.name),
        serde_json::json!(v.filter),
    ])
}

pub(crate) fn link_dest_to_json(v: &fc::LinkDest) -> serde_json::Value {
    serde_json::json!({
        "page": v.page,
        "target": geom_point(&v.target),
        "zoom": v.zoom,
    })
}

pub(crate) fn location_to_json(v: &fc::Location) -> serde_json::Value {
    serde_json::json!({
        "chapter": v.chapter,
        "page": v.page,
    })
}

pub(crate) fn ocg_to_json(v: &fc::Ocg) -> serde_json::Value {
    serde_json::json!({
        "name": v.name,
        "on": v.on,
    })
}

pub(crate) fn ocmd_info_to_json(v: &fc::OcmdInfo) -> serde_json::Value {
    serde_json::json!({
        "policy": v.policy,
        "members": v.members,
    })
}

pub(crate) fn page_label_to_json(v: &fc::PageLabel) -> serde_json::Value {
    serde_json::json!({
        "startPage": v.start_page,
        "prefix": v.prefix,
        "firstNumber": v.first_number,
        "style": v.style,
    })
}

pub(crate) fn placed_image_to_json(v: &fc::PlacedImage) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "transform": geom_matrix(&v.transform),
        "width": v.width,
        "height": v.height,
        "bits": v.bits,
        "size": v.size,
        "number": v.number,
        "colorspaceName": v.colorspace_name,
        "hasMask": v.has_mask,
        "xres": v.xres,
        "yres": v.yres,
        "digest": opt_json(v.digest.as_ref(), |x| bytes_json(x)),
    })
}

pub(crate) fn search_hits_to_json(v: &fc::SearchHits) -> napi::Result<serde_json::Value> {
    match v {
        fc::SearchHits::Rects(rects) => Ok(json_array(rects.iter().map(geom_rect).collect())),
        fc::SearchHits::Quads(quads) => Ok(json_array(quads.iter().map(geom_quad).collect())),
        _ => Err(unknown_variant("search hits")),
    }
}

pub(crate) fn table_cell_to_json(v: &fc::TableCell) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "text": v.text,
    })
}

pub(crate) fn table_finder_to_json(v: &fc::TableFinder) -> serde_json::Value {
    serde_json::json!({
        "tables": json_array(v.tables.iter().map(table_to_json).collect()),
    })
}

pub(crate) fn text_block_to_json(v: &fc::TextBlock) -> serde_json::Value {
    serde_json::Value::Array(vec![
        geom_rect(&v.rect),
        serde_json::json!(v.text),
        serde_json::json!(v.number),
        serde_json::json!(v.block_type),
    ])
}

pub(crate) fn raw_char_to_json(v: &fc::RawChar) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "c": v.ch.to_string(),
        "origin": geom_point(&v.origin),
        "synthetic": v.synthetic,
    })
}

pub(crate) fn dict_span_to_json(v: &fc::DictSpan, raw: bool) -> serde_json::Value {
    let mut out = serde_json::json!({
        "alpha": v.alpha,
        "ascender": v.ascender,
        "bbox": geom_rect(&v.bbox),
        "bidi": v.bidi,
        "char_flags": v.char_flags,
        "color": v.color,
        "descender": v.descender,
        "flags": v.flags,
        "font": v.font,
        "origin": geom_point(&v.origin),
        "size": v.size,
    });
    if raw {
        out["chars"] = json_array(v.chars.iter().map(raw_char_to_json).collect());
    } else {
        out["text"] = serde_json::Value::String(v.text.clone());
    }
    out
}

pub(crate) fn dict_line_to_json(v: &fc::DictLine, raw: bool) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "dir": geom_point(&v.direction),
        "spans": json_array(v.spans.iter().map(|s| dict_span_to_json(s, raw)).collect()),
        "wmode": v.wmode,
    })
}

pub(crate) fn dict_block_to_json(v: &fc::DictBlock, raw: bool) -> serde_json::Value {
    match v {
        fc::DictBlock::Text {
            bbox,
            number,
            flags,
            lines,
        } => serde_json::json!({
            "bbox": geom_rect(bbox),
            "flags": flags,
            "lines": json_array(lines.iter().map(|l| dict_line_to_json(l, raw)).collect()),
            "number": number,
            "type": 0,
        }),
        fc::DictBlock::Image {
            bbox,
            number,
            width,
            height,
            bpc,
            colorspace,
            ext,
            image_data,
            transform,
            xres,
            yres,
        } => serde_json::json!({
            "bbox": geom_rect(bbox),
            "bpc": bpc,
            "colorspace": colorspace,
            "ext": ext,
            "height": height,
            "image": bytes_json(image_data),
            "mask": serde_json::Value::Null,
            "number": number,
            "size": image_data.len() as i32,
            "transform": geom_matrix(transform),
            "type": 1,
            "width": width,
            "xres": xres,
            "yres": yres,
        }),
        _ => {
            unreachable!("future dict block variant");
        }
    }
}

pub(crate) fn text_dict_to_json(v: &fc::TextDict, raw: bool) -> serde_json::Value {
    serde_json::json!({
        "blocks": json_array(v.blocks.iter().map(|b| dict_block_to_json(b, raw)).collect()),
        "height": v.height,
        "width": v.width,
    })
}

pub(crate) fn text_output_to_json(
    v: &fc::TextOutput,
    raw: bool,
) -> napi::Result<serde_json::Value> {
    match v {
        fc::TextOutput::Text(s) => Ok(serde_json::Value::String(s.clone())),
        fc::TextOutput::Words(words) => {
            Ok(json_array(words.iter().map(text_word_to_json).collect()))
        }
        fc::TextOutput::Blocks(blocks) => {
            Ok(json_array(blocks.iter().map(text_block_to_json).collect()))
        }
        fc::TextOutput::Dict(dict) => Ok(text_dict_to_json(dict, raw)),
        fc::TextOutput::Json(s)
        | fc::TextOutput::RawJson(s)
        | fc::TextOutput::Html(s)
        | fc::TextOutput::Xhtml(s)
        | fc::TextOutput::Xml(s) => Ok(serde_json::Value::String(s.clone())),
        _ => Err(unknown_variant("text output")),
    }
}

pub(crate) fn text_trace_to_json(v: &fc::TextTrace) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "ascender": v.ascender,
        "bidi_dir": v.bidi_dir,
        "bidi_lvl": v.bidi_level,
        "chars": json_array(v.chars.iter().map(trace_char_to_json).collect()),
    })
}

pub(crate) fn text_word_to_json(v: &fc::TextWord) -> serde_json::Value {
    serde_json::Value::Array(vec![
        geom_rect(&v.rect),
        serde_json::json!(v.text),
        serde_json::json!(v.block_no),
        serde_json::json!(v.line_no),
        serde_json::json!(v.word_no),
    ])
}

pub(crate) fn toc_entry_to_json(v: &fc::TocEntry) -> serde_json::Value {
    serde_json::Value::Array(vec![
        serde_json::json!(v.level),
        serde_json::json!(v.title),
        serde_json::json!(v.page),
    ])
}

pub(crate) fn tuple_to_json_rect_point(v: &(fc::Rect, fc::Point)) -> serde_json::Value {
    serde_json::Value::Array(vec![geom_rect(&v.0), geom_point(&v.1)])
}

pub(crate) fn tuple_to_json_string_string(v: &(String, String)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), serde_json::json!(v.1)])
}

pub(crate) fn tuple_to_json_string_f64(v: &(String, f64)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), serde_json::json!(v.1)])
}

pub(crate) fn tuple_to_json_bool_bool(v: &(bool, bool)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), serde_json::json!(v.1)])
}

pub(crate) fn tuple_to_json_f64_f64(v: &(f64, f64)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), serde_json::json!(v.1)])
}

pub(crate) fn tuple_to_json_f64_f64_f64(v: &(f64, f64, f64)) -> serde_json::Value {
    serde_json::Value::Array(vec![
        serde_json::json!(v.0),
        serde_json::json!(v.1),
        serde_json::json!(v.2),
    ])
}

pub(crate) fn tuple_to_json_i32_vecu8(v: &(i32, Vec<u8>)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), bytes_json(&v.1)])
}

pub(crate) fn tuple_to_json_i32_i32(v: &(i32, i32)) -> serde_json::Value {
    serde_json::Value::Array(vec![serde_json::json!(v.0), serde_json::json!(v.1)])
}

pub(crate) fn tuple_to_json_usize_usize(v: &(usize, usize)) -> serde_json::Value {
    serde_json::Value::Array(vec![
        serde_json::json!(v.0 as f64),
        serde_json::json!(v.1 as f64),
    ])
}

pub(crate) fn drawing_item_to_json(v: &fc::DrawingItem) -> napi::Result<serde_json::Value> {
    Ok(serde_json::Value::Array(vec![
        serde_json::json!(v.op),
        compat_to_json(&v.params)?,
    ]))
}

pub(crate) fn table_to_json(v: &fc::Table) -> serde_json::Value {
    serde_json::json!({
        "bbox": geom_rect(&v.bbox),
        "rowCount": v.row_count,
        "colCount": v.col_count,
        "cells": json_array(v.cells.iter().map(table_cell_to_json).collect()),
    })
}

pub(crate) fn trace_char_to_json(v: &fc::TraceChar) -> serde_json::Value {
    serde_json::Value::Array(vec![
        serde_json::json!(v.codepoint),
        serde_json::json!(v.glyph),
        geom_point(&v.origin),
        geom_rect(&v.bbox),
    ])
}
