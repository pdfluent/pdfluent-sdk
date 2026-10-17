//! Module-level entry points for the compatible API (lane L5 helpers).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Free functions: paper sizes, colour and string helpers, geometry
//! utilities, and logging setup. Names are Rust spellings of the recorded
//! module members; unshaped values travel as [`Value`] until a recording
//! pins them down.

use super::constants;
use super::document::InsertPdfOptions;
use super::edit::Annot;
use super::geometry::{IRect, Matrix, Point, Quad, Rect};
use super::text::ApplyPagesOptions;
use super::{ErrorKind, Page, Value};
use crate::document::PdfDocument;
use crate::error::Result;

/// Options for log setup: message sinks.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LogOptions {
    /// Log through this message text sink.
    pub text: Option<String>,
    /// Log to this path.
    pub path: Option<String>,
    /// Append to the path instead of replacing it.
    pub append: bool,
}

/// Options for page merging: rotation, carried objects, and grafting.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PageMergeOptions {
    /// Rotation applied to the merged page.
    pub rotate: i32,
    /// Carry links over.
    pub links: bool,
    /// Copy annotations over.
    pub copy_annots: bool,
    /// Graft map.
    pub graft_map: Value,
}

impl Default for PageMergeOptions {
    fn default() -> Self {
        Self {
            rotate: -1,
            links: true,
            copy_annots: true,
            graft_map: Value::Null,
        }
    }
}

/// Whether a value is a PDF number.
fn is_number(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_))
}

/// Whether a value reads as a 4-number rectangle.
fn looks_like_rect(value: &Value) -> bool {
    match value {
        Value::List(items) => items.len() == 4 && items.iter().all(is_number),
        _ => false,
    }
}

/// Whether a value reads as an 8-number quadrilateral.
fn looks_like_quad(value: &Value) -> bool {
    match value {
        Value::List(items) => {
            (items.len() == 8 && items.iter().all(is_number))
                || (items.len() == 4
                    && items.iter().all(|corner| {
                        matches!(corner, Value::List(pair)
                            if pair.len() == 2 && pair.iter().all(is_number))
                    }))
        }
        _ => false,
    }
}

/// Fail unless a PDF invariant holds.
pub fn assert_pdf(cond: bool) {
    assert!(cond, "PDF invariant failed");
}

/// Validate colour components.
pub fn check_color(color: Option<&[f64]>) -> Result<()> {
    let Some(components) = color else {
        return Ok(());
    };
    if !matches!(components.len(), 1 | 3 | 4) {
        return Err(ErrorKind::BadValue.to_error(format!(
            "need 1, 3 or 4 colour components, got {}",
            components.len()
        )));
    }
    if components.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return Err(ErrorKind::BadValue.to_error("colour components leave 0..=1".to_string()));
    }
    Ok(())
}

/// Validate a marker argument.
pub fn check_marker_arg(quads: &Value) -> Result<Value> {
    match quads {
        Value::List(items) if items.iter().all(looks_like_quad) => Ok(quads.clone()),
        single if looks_like_quad(single) => Ok(Value::List(vec![single.clone()])),
        _ => Err(ErrorKind::WrongType.to_error("marker needs quadruples".to_string())),
    }
}

/// Whether a morph sequence fits.
pub fn check_morph(value: &Value) -> bool {
    matches!(value, Value::List(pair) if pair.len() == 2)
}

/// Validate a parent handle.
///
/// The recordings accept every value here (none, numbers, text, rects all
/// pass); the check exists for call-shape compatibility only.
pub fn check_parent(_value: &Value) -> Result<()> {
    Ok(())
}

/// Whether a quad value fits.
pub fn check_quad(value: &Value) -> bool {
    looks_like_quad(value)
}

/// Whether a rect value fits.
pub fn check_rect(value: &Value) -> bool {
    looks_like_rect(value)
}

/// One number in PDF content-stream spelling: `%g` with six significant
/// digits, then the leading zero dropped (`.5` rather than `0.5`).
fn pdf_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if !value.is_finite() {
        return format!("{value}");
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    let exponent = magnitude.log10().floor() as i32;
    let mut text = if !(-4..6).contains(&exponent) {
        let scaled = magnitude / 10_f64.powi(exponent);
        let mut digits = format!("{scaled:.5}");
        while digits.ends_with('0') {
            digits.pop();
        }
        if digits.ends_with('.') {
            digits.pop();
        }
        format!("{digits}e{exponent:+03}")
    } else {
        let after = (5 - exponent).max(0) as usize;
        let mut digits = format!("{magnitude:.after$}");
        if digits.contains('.') {
            while digits.ends_with('0') {
                digits.pop();
            }
            if digits.ends_with('.') {
                digits.pop();
            }
        }
        digits
    };
    if let Some(rest) = text.strip_prefix("0.") {
        text = format!(".{rest}");
    }
    format!("{sign}{text}")
}

/// One colour component as a number, else nothing.
fn component_number(component: &Value) -> Option<f64> {
    match component {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// Colour code in a format: grey, rgb or cmyk components followed by the
/// non-stroking operator and a trailing blank (`1 0 0 rg `). Absent colour
/// writes nothing; anything that fits no colourscape writes nothing here
/// and the bindings reject it before calling.
pub fn color_code(color: &Value, format: &str) -> String {
    if matches!(color, Value::Null) {
        return String::new();
    }
    let operator = if format.eq_ignore_ascii_case("g") {
        "g"
    } else if format.eq_ignore_ascii_case("rgb") {
        "rg"
    } else if format.eq_ignore_ascii_case("cmyk") {
        "k"
    } else {
        return String::new();
    };
    let want = match operator {
        "g" => 1,
        "rg" => 3,
        _ => 4,
    };
    let mut components = Vec::with_capacity(want);
    match color {
        single @ (Value::Int(_) | Value::Float(_)) => {
            if want != 1 {
                return String::new();
            }
            components.push(component_number(single).unwrap_or(f64::NAN));
        }
        Value::List(items) => {
            if items.len() != want {
                return String::new();
            }
            for item in items {
                let Some(number) = component_number(item) else {
                    return String::new();
                };
                components.push(number);
            }
        }
        _ => return String::new(),
    }
    let mut out: Vec<String> = components.iter().map(|c| pdf_number(*c)).collect();
    out.push(operator.to_string());
    out.join(" ") + " "
}

/// Conversion header text: the recorded XHTML prologue (which ignores the
/// filename) or the XML prologue naming the file, else nothing.
pub fn conversion_header(kind: &str, filename: Option<&str>) -> String {
    match kind {
        "xhtml" => "\n<?xml version=\"1.0\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\n<head>\n<style>\nbody{background-color:gray}\ndiv{background-color:white;margin:1em;padding:1em}\np{white-space:pre-wrap}\n</style>\n</head>\n<body>\n".to_string(),
        "xml" => {
            let name = filename.unwrap_or("unknown");
            format!("\n<?xml version=\"1.0\"?>\n<document name=\"{name}\">\n")
        }
        _ => String::new(),
    }
}

/// Conversion trailer text: the recorded XHTML or XML epilogue, else nothing.
pub fn conversion_trailer(kind: &str) -> String {
    match kind {
        "xhtml" => "</body>\n</html>\n".to_string(),
        "xml" => "</document>\n".to_string(),
        _ => String::new(),
    }
}

/// Empty integer rectangle: the inverted infinite box, which no finite
/// rectangle meets.
pub fn empty_irect() -> IRect {
    IRect {
        x0: constants::FZ_MAX_INF_RECT,
        y0: constants::FZ_MAX_INF_RECT,
        x1: constants::FZ_MIN_INF_RECT,
        y1: constants::FZ_MIN_INF_RECT,
    }
}

/// Empty quadrilateral: the corners of the empty rectangle.
pub fn empty_quad() -> Quad {
    let rect = empty_rect();
    Quad {
        ul: Point {
            x: rect.x0,
            y: rect.y0,
        },
        ur: Point {
            x: rect.x1,
            y: rect.y0,
        },
        ll: Point {
            x: rect.x0,
            y: rect.y1,
        },
        lr: Point {
            x: rect.x1,
            y: rect.y1,
        },
    }
}

/// Empty rectangle: the inverted infinite box.
pub fn empty_rect() -> Rect {
    Rect {
        x0: constants::FZ_MAX_INF_RECT as f64,
        y0: constants::FZ_MAX_INF_RECT as f64,
        x1: constants::FZ_MIN_INF_RECT as f64,
        y1: constants::FZ_MIN_INF_RECT as f64,
    }
}

/// Ensure a live document.
pub fn ensure_operation(_doc: &PdfDocument) -> Result<()> {
    // A shared borrow is live by construction; nothing to check.
    Ok(())
}

/// Infinite integer rectangle.
pub fn infinite_irect() -> IRect {
    IRect {
        x0: constants::FZ_MIN_INF_RECT,
        y0: constants::FZ_MIN_INF_RECT,
        x1: constants::FZ_MAX_INF_RECT,
        y1: constants::FZ_MAX_INF_RECT,
    }
}

/// Infinite quadrilateral.
pub fn infinite_quad() -> Quad {
    let rect = infinite_rect();
    Quad {
        ul: Point {
            x: rect.x0,
            y: rect.y0,
        },
        ur: Point {
            x: rect.x1,
            y: rect.y0,
        },
        ll: Point {
            x: rect.x0,
            y: rect.y1,
        },
        lr: Point {
            x: rect.x1,
            y: rect.y1,
        },
    }
}

/// Infinite rectangle.
pub fn infinite_rect() -> Rect {
    Rect {
        x0: constants::FZ_MIN_INF_RECT as f64,
        y0: constants::FZ_MIN_INF_RECT as f64,
        x1: constants::FZ_MAX_INF_RECT as f64,
        y1: constants::FZ_MAX_INF_RECT as f64,
    }
}

/// PDF name object text.
pub fn pdf_name(value: &Value) -> String {
    match value {
        Value::Str(text) => format!("/{text}"),
        _ => String::new(),
    }
}

/// Whether a sequence value fits.
pub fn py_sequence_check(value: &Value) -> bool {
    matches!(value, Value::List(_))
}

/// Sequence value length.
pub fn py_sequence_size(value: &Value) -> usize {
    match value {
        Value::List(items) => items.len(),
        _ => 0,
    }
}

/// Decode one `\uXXXX` / `\UXXXXXXXX` / `\xXX` escape at the head of `chars`.
fn decode_escape(chars: &[char], errors: &str) -> (Option<char>, usize) {
    let width = match chars.first() {
        Some('u') => 5,
        Some('U') => 9,
        Some('x') => 3,
        _ => return (None, 0),
    };
    if chars.len() < width {
        return (
            match errors {
                "ignore" => None,
                _ => Some(char::REPLACEMENT_CHARACTER),
            },
            1,
        );
    }
    let hex: String = chars[1..width].iter().collect();
    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
        Some(ch) => (Some(ch), width),
        None => match errors {
            "ignore" => (None, width),
            _ => (Some(char::REPLACEMENT_CHARACTER), width),
        },
    }
}

/// Decode raw unicode escapes.
pub fn py_unicode_decode_raw_unicode_escape(text: &str, errors: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let (decoded, width) = decode_escape(&chars[i + 1..], errors);
            if width > 0 {
                if let Some(ch) = decoded {
                    out.push(ch);
                } else if errors != "ignore" {
                    out.push('\\');
                }
                i += 1 + width;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Raise a lane error.
pub fn raisepy(message: &str, exc: &Value) -> Result<()> {
    let kind = match exc {
        Value::Str(name) => match name.as_str() {
            "TypeError" | "AttributeError" => ErrorKind::WrongType,
            "KeyError" => ErrorKind::UnknownKey,
            "IndexError" => ErrorKind::IndexOutOfRange,
            "FileNotFoundError" => ErrorKind::MissingFile,
            "FileDataError" | "EmptyFileError" => ErrorKind::UnreadableFile,
            "ModuleNotFoundError" => ErrorKind::MissingComponent,
            _ => ErrorKind::BadValue,
        },
        _ => ErrorKind::BadValue,
    };
    Err(kind.to_error(message.to_string()))
}

/// Finish an annotation edit.
pub fn annot_postprocess(page: &Page<'_>, _annot: &Annot<'_>) -> Result<()> {
    if page.index() >= page.document().page_count() {
        return Err(ErrorKind::IndexOutOfRange
            .to_error(format!("page index {} out of range", page.index())));
    }
    Ok(())
}

/// Prepare an annotation edit.
pub fn annot_preprocess(page: &Page<'_>) -> Result<i32> {
    page.document().page_xref(page.index() as i32)
}

/// Run a callback over pages.
pub fn apply_pages(
    path: &str,
    options: &ApplyPagesOptions,
    page_fn: &mut dyn FnMut(&Page<'_>) -> Result<()>,
) -> Result<()> {
    let doc = PdfDocument::open(path)
        .map_err(|_| ErrorKind::MissingFile.to_error(format!("cannot open {path}")))?;
    let count = doc.page_count();
    let indices: Vec<usize> = match &options.pages {
        Some(pages) => {
            let mut out = Vec::with_capacity(pages.len());
            for page in pages {
                let resolved = if *page < 0 {
                    count as i64 + *page as i64
                } else {
                    *page as i64
                };
                if resolved < 0 || resolved >= count as i64 {
                    return Err(ErrorKind::IndexOutOfRange
                        .to_error(format!("page index {page} out of range")));
                }
                out.push(resolved as usize);
            }
            out
        }
        None => (0..count).collect(),
    };
    for index in indices {
        page_fn(&Page::new(&doc, index))?;
    }
    Ok(())
}

/// Whether arguments fit types.
pub fn args_match(args: &[Value], types: &[Value]) -> bool {
    if args.len() != types.len() {
        return false;
    }
    args.iter().zip(types.iter()).all(|(arg, want)| {
        let Value::Str(name) = want else {
            return false;
        };
        matches!(
            (arg, name.as_str()),
            (Value::Null, "None" | "NoneType")
                | (Value::Bool(_), "bool")
                | (Value::Int(_), "int")
                | (Value::Float(_), "float" | "int")
                | (Value::Str(_), "str")
                | (Value::Bytes(_), "bytes")
                | (Value::List(_), "list" | "tuple" | "sequence")
                | (Value::Dict(_), "dict")
        )
    })
}

/// PDF colour table.
pub fn colors_pdf_dict() -> Value {
    let mut map = std::collections::HashMap::new();
    for (name, rgb) in constants::pdfcolor {
        map.insert(
            (*name).to_string(),
            Value::List(vec![
                Value::Float(rgb[0]),
                Value::Float(rgb[1]),
                Value::Float(rgb[2]),
            ]),
        );
    }
    Value::Dict(map)
}

/// Colour name list.
pub fn colors_wx_list() -> Vec<String> {
    let mut names: Vec<String> = constants::pdfcolor
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    names.sort();
    names
}

/// Device scissor box.
pub fn compute_scissor(_device: &Value) -> Value {
    Value::Null
}

/// Whether a destination page reference fits.
fn page_ref_fits(obj: &Value, page_count: i32, page_object_nums: &[i32]) -> bool {
    match obj {
        Value::Int(page) => {
            (0 <= *page && *page < page_count as i64) || page_object_nums.contains(&(*page as i32))
        }
        _ => false,
    }
}

/// Whether a destination fits.
pub fn dest_is_valid(
    dest: &Value,
    page_count: i32,
    page_object_nums: &[i32],
    names: &[String],
) -> bool {
    let Value::List(items) = dest else {
        return false;
    };
    let Some(first) = items.first() else {
        return false;
    };
    match first {
        Value::Str(name) => names.iter().any(|n| n == name),
        other => page_ref_fits(other, page_count, page_object_nums),
    }
}

/// Whether a destination page fits.
pub fn dest_is_valid_page(obj: &Value, page_object_nums: &[i32], page_count: i32) -> bool {
    page_ref_fits(obj, page_count, page_object_nums)
}

/// Whether a character is superscript.
pub fn detect_super_script(_line: &Value, _ch: &Value) -> bool {
    false
}

/// Direction value text.
pub fn dir_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Str(text) => text.clone(),
        Value::Bytes(bytes) => format!("{} bytes", bytes.len()),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(dir_str).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Dict(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{k}: {}", dir_str(&map[*k])))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// Last lane error facts.
pub fn exception_info() -> Value {
    Value::Null
}

/// Copy one object graph from `src` into `dst`, remapping ids.
fn graft_object(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    obj: &lopdf::Object,
    map: &mut std::collections::HashMap<lopdf::ObjectId, lopdf::ObjectId>,
) -> lopdf::Object {
    match obj {
        lopdf::Object::Reference(id) => {
            if let Some(remapped) = map.get(id) {
                return lopdf::Object::Reference(*remapped);
            }
            dst.max_id += 1;
            let fresh = (dst.max_id, 0);
            map.insert(*id, fresh);
            let target = src.objects.get(id).cloned().unwrap_or(lopdf::Object::Null);
            let copied = graft_object(src, dst, &target, map);
            dst.objects.insert(fresh, copied);
            lopdf::Object::Reference(fresh)
        }
        lopdf::Object::Array(items) => lopdf::Object::Array(
            items
                .iter()
                .map(|item| graft_object(src, dst, item, map))
                .collect(),
        ),
        lopdf::Object::Dictionary(dict) => {
            let mut out = lopdf::Dictionary::new();
            for (key, value) in dict.iter() {
                out.set(key.clone(), graft_object(src, dst, value, map));
            }
            lopdf::Object::Dictionary(out)
        }
        lopdf::Object::Stream(stream) => {
            let mut dict = lopdf::Dictionary::new();
            for (key, value) in stream.dict.iter() {
                dict.set(key.clone(), graft_object(src, dst, value, map));
            }
            lopdf::Object::Stream(lopdf::Stream::new(dict, stream.content.clone()))
        }
        other => other.clone(),
    }
}

/// Graft one page dict (never its `/Parent` chain) into `dst`.
fn graft_page_dict(
    src: &lopdf::Document,
    dst: &mut lopdf::Document,
    page_id: lopdf::ObjectId,
    map: &mut std::collections::HashMap<lopdf::ObjectId, lopdf::ObjectId>,
) -> Option<lopdf::ObjectId> {
    let dict = src.get_dictionary(page_id).ok()?.clone();
    dst.max_id += 1;
    let fresh = (dst.max_id, 0);
    map.insert(page_id, fresh);
    let mut out = lopdf::Dictionary::new();
    for (key, value) in dict.iter() {
        if key.as_slice() == b"Parent" {
            continue;
        }
        out.set(key.clone(), graft_object(src, dst, value, map));
    }
    dst.objects.insert(fresh, lopdf::Object::Dictionary(out));
    Some(fresh)
}

/// Leaf `/Pages` node holding 0-based `position`: the node, the ancestor
/// chain, and the remaining offset inside the leaf.
fn leaf_for_position(
    dst: &lopdf::Document,
    root: lopdf::ObjectId,
    mut position: usize,
) -> Option<(lopdf::ObjectId, Vec<lopdf::ObjectId>, usize)> {
    let mut node = root;
    let mut chain = vec![root];
    for _ in 0..64 {
        let dict = dst.get_dictionary(node).ok()?;
        let kids = dict.get(b"Kids").ok()?.as_array().ok()?.clone();
        let mut descended = false;
        for kid in &kids {
            let Ok(kid_id) = kid.as_reference() else {
                continue;
            };
            let Ok(kid_dict) = dst.get_dictionary(kid_id) else {
                continue;
            };
            let is_pages = matches!(
                kid_dict.get(b"Type").ok(),
                Some(lopdf::Object::Name(name)) if name.as_slice() == b"Pages"
            );
            if is_pages {
                let count = kid_dict
                    .get(b"Count")
                    .ok()
                    .and_then(|c| c.as_i64().ok())
                    .unwrap_or(0)
                    .max(0) as usize;
                if position < count {
                    node = kid_id;
                    chain.push(kid_id);
                    descended = true;
                    break;
                }
                position -= count;
            } else if position == 0 {
                // Offset among this leaf's direct kids.
                let offset = kids
                    .iter()
                    .position(|k| k.as_reference().ok() == Some(kid_id))
                    .unwrap_or(0);
                return Some((node, chain, offset));
            } else {
                position -= 1;
            }
        }
        if !descended {
            let offset = dst
                .get_dictionary(node)
                .ok()?
                .get(b"Kids")
                .ok()?
                .as_array()
                .ok()?
                .len();
            return Some((node, chain, offset));
        }
    }
    None
}

/// Insert `page_id` into the page tree at 0-based `position`.
fn tree_insert(dst: &mut lopdf::Document, page_id: lopdf::ObjectId, position: usize) {
    let root = match dst
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|root| {
            dst.get_dictionary(root)
                .ok()?
                .get(b"Pages")
                .ok()?
                .as_reference()
                .ok()
        }) {
        Some(root) => root,
        None => return,
    };
    let (leaf, chain, offset) = match leaf_for_position(dst, root, position) {
        Some(found) => found,
        None => return,
    };
    if let Ok(dict) = dst.get_dictionary_mut(leaf) {
        if let Ok(kids) = dict.get_mut(b"Kids").and_then(|k| k.as_array_mut()) {
            let at = offset.min(kids.len());
            kids.insert(at, lopdf::Object::Reference(page_id));
        }
    }
    if let Ok(dict) = dst.get_dictionary_mut(page_id) {
        dict.set(b"Parent", lopdf::Object::Reference(leaf));
    }
    for ancestor in chain {
        if let Ok(dict) = dst.get_dictionary_mut(ancestor) {
            let count = dict
                .get(b"Count")
                .ok()
                .and_then(|c| c.as_i64().ok())
                .unwrap_or(0);
            dict.set(b"Count", lopdf::Object::Integer(count + 1));
        }
    }
}

/// Drop annotation references of the given subtypes from a grafted page.
fn strip_annots_by_subtype(
    dst: &mut lopdf::Document,
    page_id: lopdf::ObjectId,
    drop_all: bool,
    drop_subtypes: &[&[u8]],
) {
    let annots_id = dst
        .get_dictionary(page_id)
        .ok()
        .and_then(|dict| dict.get(b"Annots").ok().cloned());
    if drop_all {
        if let Ok(dict) = dst.get_dictionary_mut(page_id) {
            dict.remove(b"Annots");
        }
        return;
    }
    if drop_subtypes.is_empty() {
        return;
    }
    let Some(lopdf::Object::Array(items)) = annots_id else {
        return;
    };
    let mut kept = Vec::with_capacity(items.len());
    for item in &items {
        let drop = item
            .as_reference()
            .ok()
            .and_then(|id| dst.get_dictionary(id).ok())
            .and_then(|dict| dict.get(b"Subtype").ok().cloned())
            .is_some_and(|subtype| match subtype {
                lopdf::Object::Name(name) => drop_subtypes.contains(&name.as_slice()),
                _ => false,
            });
        if !drop {
            kept.push(item.clone());
        }
    }
    if let Ok(dict) = dst.get_dictionary_mut(page_id) {
        dict.set(b"Annots", lopdf::Object::Array(kept));
    }
}

/// Insert pages from a document.
pub fn extra_insert_pdf(
    doc: &mut PdfDocument,
    src: &PdfDocument,
    options: &InsertPdfOptions,
) -> Result<()> {
    let src_count = src.page_count() as i32;
    let from = if options.from_page < 0 {
        0
    } else {
        options.from_page
    };
    let to = if options.to_page < 0 {
        src_count - 1
    } else {
        options.to_page
    };
    if from < 0 || to < from || to >= src_count {
        return Err(ErrorKind::BadValue.to_error(format!("bad page span {from}..={to}")));
    }
    let mut scratch = doc.lopdf().clone();
    let src_lopdf = src.lopdf();
    let src_pages = src_lopdf.get_pages();
    let mut src_numbers: Vec<u32> = src_pages.keys().copied().collect();
    src_numbers.sort_unstable();
    let mut map = std::collections::HashMap::new();
    let mut position = if options.start_at < 0 {
        doc.page_count()
    } else {
        options.start_at as usize
    };
    for number in from..=to {
        let page_id = src_pages[&src_numbers[number as usize]];
        let Some(fresh) = graft_page_dict(src_lopdf, &mut scratch, page_id, &mut map) else {
            continue;
        };
        if !options.annots {
            strip_annots_by_subtype(&mut scratch, fresh, true, &[]);
        } else {
            let mut drop: Vec<&[u8]> = Vec::new();
            if !options.links {
                drop.push(b"Link");
            }
            if !options.widgets {
                drop.push(b"Widget");
            }
            strip_annots_by_subtype(&mut scratch, fresh, false, &drop);
        }
        if options.rotate >= 0 {
            if let Ok(dict) = scratch.get_dictionary_mut(fresh) {
                dict.set(
                    b"Rotate",
                    lopdf::Object::Integer(options.rotate.rem_euclid(360) as i64),
                );
            }
        }
        tree_insert(&mut scratch, fresh, position);
        position += 1;
    }
    *doc = PdfDocument::from_lopdf(scratch)?;
    Ok(())
}

/// Byte offset of a needle.
pub fn find_string(text: &str, needle: &str) -> Option<usize> {
    text.find(needle)
}

/// Format a float compactly.
pub fn format_g(value: f64, fmt: &str) -> String {
    if fmt == "%g" {
        let text = format!("{value}");
        if text.contains('.') && !text.contains('e') {
            let trimmed = text.trim_end_matches('0').trim_end_matches('.');
            return if trimmed.is_empty() {
                "0".to_string()
            } else {
                trimmed.to_string()
            };
        }
        return text;
    }
    // `%.Nf` / `%.Ne` / `%.Ng`: honour the precision, default the shape.
    if let Some(precision) = fmt
        .strip_prefix("%.")
        .and_then(|rest| rest.find(['f', 'e', 'g']).map(|end| &rest[..end]))
        .and_then(|digits| digits.parse::<usize>().ok())
    {
        if fmt.ends_with('f') {
            return format!("{value:.precision$}");
        }
        if fmt.ends_with('e') {
            return format!("{value:.precision$e}");
        }
        return format!("{value:.precision$}");
    }
    format!("{value}")
}

/// Escape one PDF string literal body: printable ASCII rides through,
/// the five whitespace escapes keep their short spellings, and every other
/// Latin-1 byte leaves as a three-digit octal (`é` as `\351`). Characters
/// past Latin-1 have no single byte there and leave as octal UTF-8.
fn escape_pdf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            ch if ch.is_ascii_graphic() || ch == ' ' => out.push(ch),
            ch if (ch as u32) <= 0xFF => {
                out.push_str(&format!("\\{:03o}", ch as u32));
            }
            ch => {
                let mut bytes = [0u8; 4];
                for byte in ch.encode_utf8(&mut bytes).bytes() {
                    out.push_str(&format!("\\{byte:03o}"));
                }
            }
        }
    }
    out
}

/// Typeset string in hex spelling: `[<616263>]`, one byte per character
/// for simple fonts and one 16-bit unit otherwise. The glyph ids ride along
/// for call-shape compatibility; the text decides the bytes.
pub fn get_tjstr(text: &str, _glyphs: Option<&[i32]>, simple: bool, _ordering: i32) -> String {
    let mut hex = String::with_capacity(text.len() * 2);
    if simple {
        for ch in text.chars() {
            if (ch as u32) <= 0xFF {
                hex.push_str(&format!("{:02x}", ch as u32));
            } else {
                let mut bytes = [0u8; 4];
                for byte in ch.encode_utf8(&mut bytes).bytes() {
                    hex.push_str(&format!("{byte:02x}"));
                }
            }
        }
    } else {
        let mut units = [0u16; 2];
        for ch in text.chars() {
            for unit in ch.encode_utf16(&mut units) {
                hex.push_str(&format!("{unit:04x}"));
            }
        }
    }
    format!("[<{hex}>]")
}

/// Boolean environment switch.
pub fn get_env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

/// Integer environment switch.
pub fn get_env_int(name: &str, default: i32) -> i32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(default)
}

/// Highlight quads of a selection.
///
/// One quad per spanned line span: the selected characters' union box,
/// with the first and last rows clamped to the given points' x edges.
/// Missing points default to the page corners (the whole page when both
/// are missing); gap filler spans draw no quad. Line spanning and the
/// character filter follow the text selection renderer.
pub fn get_highlight_selection(
    _page: &Page<'_>,
    _start: Option<Point>,
    _stop: Option<Point>,
    _clip: Option<Rect>,
) -> Result<Vec<Quad>> {
    use super::text::{capture_page, engine_for, orient_quad, union_all};
    let engine = engine_for(_page.document())?;
    let capture = capture_page(&engine, _page.index(), false)?;
    let frame = _page.rect()?;
    let begin = _start.unwrap_or(Point {
        x: frame.x0,
        y: frame.y0,
    });
    let end = _stop.unwrap_or(Point {
        x: frame.x1,
        y: frame.y1,
    });
    let (lo, hi) = if (begin.y, begin.x) <= (end.y, end.x) {
        (begin, end)
    } else {
        (end, begin)
    };
    let mut rows: Vec<&super::text::CapturedLine> = Vec::new();
    for block in &capture.blocks {
        for line in &block.lines {
            if line.bbox.y1 > lo.y && line.bbox.y0 < hi.y {
                rows.push(line);
            }
        }
    }
    let last = rows.len().saturating_sub(1);
    let mut quads = Vec::new();
    for (i, line) in rows.iter().enumerate() {
        for span in line.spans.iter().filter(|s| !s.gap) {
            let boxes = span.chars.iter().filter_map(|ch| {
                if _clip.is_some_and(|r| !overlaps(&ch.bbox, &r)) {
                    return None;
                }
                let cx = (ch.bbox.x0 + ch.bbox.x1) / 2.0;
                if i == 0 && cx < lo.x {
                    return None;
                }
                if i == last && cx > hi.x {
                    return None;
                }
                Some(ch.bbox)
            });
            let Some(bbox) = union_all(boxes) else {
                continue;
            };
            let mut bbox = bbox;
            if i == 0 && _start.is_some() {
                bbox.x0 = bbox.x0.max(lo.x);
            }
            if i == last && _stop.is_some() {
                bbox.x1 = bbox.x1.min(hi.x);
            }
            if bbox.x1 < bbox.x0 {
                continue;
            }
            quads.push(orient_quad(bbox, line.dir));
        }
    }
    Ok(quads)
}

/// One text line for selection: its box plus its span boxes in order.
#[derive(Debug, Clone)]
pub struct SpanLine {
    /// The line box, which decides whether the line falls in the range.
    pub bbox: Rect,
    /// The span boxes in reading order.
    pub spans: Vec<Rect>,
}

/// Whether two boxes share a positive area.
fn overlaps(one: &Rect, two: &Rect) -> bool {
    one.x0.max(two.x0) < one.x1.min(two.x1) && one.y0.max(two.y0) < one.y1.min(two.y1)
}

/// Highlight boxes of a span selection: the lines from the one holding
/// `start` through the one holding `stop` (open ends run to the page edge),
/// the first span starting no earlier than `start.x`, the last span ending
/// no later than `stop.x`, every box cut to `clip` when one is given.
pub fn highlight_span_rects(
    lines: &[SpanLine],
    start: Option<Point>,
    stop: Option<Point>,
    clip: Option<Rect>,
) -> Vec<Rect> {
    if lines.is_empty() {
        return Vec::new();
    }
    let mut first = 0;
    let mut last = lines.len() - 1;
    if let Some(origin) = start {
        let Some(index) = lines.iter().position(|line| line.bbox.y1 > origin.y) else {
            return Vec::new();
        };
        first = index;
    }
    if let Some(end) = stop {
        let Some(index) = lines.iter().rposition(|line| line.bbox.y0 < end.y) else {
            return Vec::new();
        };
        last = index;
    }
    if first > last {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate().take(last + 1).skip(first) {
        if let Some(clip) = clip.as_ref() {
            if !overlaps(&line.bbox, clip) {
                continue;
            }
        }
        let mut spans: Vec<Rect> = line.spans.clone();
        if index == first {
            if let Some(origin) = start {
                spans.retain(|span| span.x1 > origin.x);
                if let Some(head) = spans.first_mut() {
                    head.x0 = head.x0.max(origin.x);
                }
            }
        }
        if index == last {
            if let Some(end) = stop {
                spans.retain(|span| span.x0 < end.x);
                if let Some(tail) = spans.last_mut() {
                    tail.x1 = tail.x1.min(end.x);
                }
            }
        }
        for mut span in spans {
            if let Some(clip) = clip.as_ref() {
                span = util_intersect_rect(span, *clip);
                if span.x0 >= span.x1 || span.y0 >= span.y1 {
                    continue;
                }
            }
            out.push(span);
        }
    }
    out
}

/// Current PDF date stamp.
pub fn get_pdf_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86_400) as i64;
    let time = now.as_secs() % 86_400;
    // Days to civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let mut month = mp + 3;
    if month > 12 {
        month -= 12;
        year += 1;
    }
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}Z",
        time / 3_600,
        time % 3_600 / 60,
        time % 60,
    )
}

/// PDF string literal text.
pub fn get_pdf_str(text: &str) -> String {
    format!("({})", escape_pdf_text(text))
}

/// Horizontal distance.
pub fn hdist(_direction: &Value, a: &Value, b: &Value) -> f64 {
    pair_distance(a, b, true)
}

/// Pull an x/y pair out of a value.
fn as_pair(value: &Value) -> Option<(f64, f64)> {
    let Value::List(items) = value else {
        return None;
    };
    if items.len() != 2 {
        return None;
    }
    let x = match &items[0] {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => return None,
    };
    let y = match &items[1] {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => return None,
    };
    Some((x, y))
}

/// Absolute axial distance between two points.
fn pair_distance(a: &Value, b: &Value, horizontal: bool) -> f64 {
    match (as_pair(a), as_pair(b)) {
        (Some((ax, ay)), Some((bx, by))) => {
            if horizontal {
                (ax - bx).abs()
            } else {
                (ay - by).abs()
            }
        }
        _ => 0.0,
    }
}

/// Write a log line.
pub fn log(text: &str, caller: i32) {
    eprintln!("[compat:{caller}] {text}");
}

/// Escape a code point: ASCII rides through raw (the newline stays a
/// newline), NUL cannot ride a C string and leaves as `\u0000`, and anything
/// past ASCII leaves as a lowercase `\u` or `\U` escape.
pub fn make_escape(ch: char) -> String {
    let code = ch as u32;
    if code == 0 {
        return "\\u0000".to_string();
    }
    if code <= 0x7F {
        return ch.to_string();
    }
    if code <= 0xFFFF {
        return format!("\\u{code:04x}");
    }
    format!("\\U{code:08x}")
}

/// Fresh story position.
pub fn make_story_elpos() -> Value {
    Value::Null
}

/// Whether the needle is a prefix of the haystack: a longer needle, a fork
/// after a shared head, or a miss all read as no match.
pub fn match_string(haystack: &Value, needle: &Value) -> bool {
    match (haystack, needle) {
        (Value::Str(a), Value::Str(b)) => a.starts_with(b.as_str()),
        _ => false,
    }
}

/// Write a message.
pub fn message(text: &str) {
    println!("{text}");
}

/// Write a warning.
pub fn message_warning(text: &str) {
    eprintln!("warning: {text}");
}

/// Whether layout is off.
pub fn no_recommend_layout() -> bool {
    false
}

/// Merge one page over.
pub fn page_merge(
    target: &mut PdfDocument,
    source: &PdfDocument,
    page_from: i32,
    page_to: i32,
    options: &PageMergeOptions,
) -> Result<()> {
    let resolve = |index: i32, count: usize| -> Result<usize> {
        let count_i64 = count as i64;
        let resolved = if index < 0 {
            count_i64 + index as i64
        } else {
            index as i64
        };
        if resolved < 0 || resolved >= count_i64 {
            return Err(
                ErrorKind::IndexOutOfRange.to_error(format!("page index {index} out of range"))
            );
        }
        Ok(resolved as usize)
    };
    let from = resolve(page_from, source.page_count())?;
    let to = resolve(page_to, target.page_count())?;
    let mut scratch = target.lopdf().clone();
    let src_lopdf = source.lopdf();
    let src_pages = src_lopdf.get_pages();
    let mut src_numbers: Vec<u32> = src_pages.keys().copied().collect();
    src_numbers.sort_unstable();
    let dst_pages = scratch.get_pages();
    let mut dst_numbers: Vec<u32> = dst_pages.keys().copied().collect();
    dst_numbers.sort_unstable();
    let src_id = src_pages[&src_numbers[from]];
    let dst_id = dst_pages[&dst_numbers[to]];
    let mut map = std::collections::HashMap::new();
    // Graft the source content streams and point the target page at them.
    let src_contents: Vec<lopdf::Object> = src_lopdf
        .get_dictionary(src_id)
        .ok()
        .and_then(|dict| dict.get(b"Contents").ok().cloned())
        .map(|obj| match obj {
            lopdf::Object::Array(items) => items,
            single => vec![single],
        })
        .unwrap_or_default();
    let grafted: Vec<lopdf::Object> = src_contents
        .iter()
        .map(|item| graft_object(src_lopdf, &mut scratch, item, &mut map))
        .collect();
    // Merge the source resources under the target ones.
    if let Some(src_resources) = src_lopdf
        .get_dictionary(src_id)
        .ok()
        .and_then(|dict| dict.get(b"Resources").ok().cloned())
    {
        let merged = graft_object(src_lopdf, &mut scratch, &src_resources, &mut map);
        if let Ok(dict) = scratch.get_dictionary_mut(dst_id) {
            match dict.get(b"Resources").ok().cloned() {
                Some(lopdf::Object::Dictionary(mut have)) => {
                    if let lopdf::Object::Dictionary(extra) = merged {
                        for (key, value) in extra.iter() {
                            if have.get(key).is_err() {
                                have.set(key.clone(), value.clone());
                            }
                        }
                    }
                    dict.set(b"Resources", lopdf::Object::Dictionary(have));
                }
                _ => {
                    dict.set(b"Resources", merged);
                }
            }
        }
    }
    if let Ok(dict) = scratch.get_dictionary_mut(dst_id) {
        let mut contents: Vec<lopdf::Object> = match dict.get(b"Contents").ok().cloned() {
            Some(lopdf::Object::Array(items)) => items,
            Some(single) => vec![single],
            None => Vec::new(),
        };
        contents.extend(grafted);
        dict.set(b"Contents", lopdf::Object::Array(contents));
        if options.rotate >= 0 {
            dict.set(
                b"Rotate",
                lopdf::Object::Integer(options.rotate.rem_euclid(360) as i64),
            );
        }
    }
    if options.copy_annots || options.links {
        let src_annots: Vec<lopdf::Object> = src_lopdf
            .get_dictionary(src_id)
            .ok()
            .and_then(|dict| dict.get(b"Annots").ok()?.as_array().ok().cloned())
            .unwrap_or_default();
        let mut grafted_annots = Vec::new();
        for item in &src_annots {
            let is_link = item
                .as_reference()
                .ok()
                .and_then(|id| src_lopdf.get_dictionary(id).ok())
                .and_then(|dict| dict.get(b"Subtype").ok().cloned())
                .is_some_and(|subtype| {
                    matches!(subtype, lopdf::Object::Name(name) if name.as_slice() == b"Link")
                });
            if options.copy_annots || (options.links && is_link) {
                grafted_annots.push(graft_object(src_lopdf, &mut scratch, item, &mut map));
            }
        }
        if !grafted_annots.is_empty() {
            if let Ok(dict) = scratch.get_dictionary_mut(dst_id) {
                let mut have: Vec<lopdf::Object> = match dict.get(b"Annots").ok().cloned() {
                    Some(lopdf::Object::Array(items)) => items,
                    Some(single) => vec![single],
                    None => Vec::new(),
                };
                have.extend(grafted_annots);
                dict.set(b"Annots", lopdf::Object::Array(have));
            }
        }
    }
    *target = PdfDocument::from_lopdf(scratch)?;
    Ok(())
}

/// Named paper sizes in points: ISO 216 A/B/C plus the US sizes, exactly
/// the recorded table (note the portrait ledger and the small-A roundings).
const PAPER_SIZES: &[(&str, f64, f64)] = &[
    ("a0", 2384.0, 3370.0),
    ("a1", 1684.0, 2384.0),
    ("a2", 1191.0, 1684.0),
    ("a3", 842.0, 1191.0),
    ("a4", 595.0, 842.0),
    ("a5", 420.0, 595.0),
    ("a6", 298.0, 420.0),
    ("a7", 210.0, 298.0),
    ("a8", 147.0, 210.0),
    ("a9", 105.0, 147.0),
    ("a10", 74.0, 105.0),
    ("b0", 2835.0, 4008.0),
    ("b1", 2004.0, 2835.0),
    ("b2", 1417.0, 2004.0),
    ("b3", 1001.0, 1417.0),
    ("b4", 709.0, 1001.0),
    ("b5", 499.0, 709.0),
    ("b6", 354.0, 499.0),
    ("b7", 249.0, 354.0),
    ("b8", 176.0, 249.0),
    ("b9", 125.0, 176.0),
    ("b10", 88.0, 125.0),
    ("c0", 2599.0, 3677.0),
    ("c1", 1837.0, 2599.0),
    ("c2", 1298.0, 1837.0),
    ("c3", 918.0, 1298.0),
    ("c4", 649.0, 918.0),
    ("c5", 459.0, 649.0),
    ("c6", 323.0, 459.0),
    ("c7", 230.0, 323.0),
    ("c8", 162.0, 230.0),
    ("c9", 113.0, 162.0),
    ("c10", 79.0, 113.0),
    ("card-4x6", 288.0, 432.0),
    ("card-5x7", 360.0, 504.0),
    ("commercial", 297.0, 684.0),
    ("executive", 522.0, 756.0),
    ("invoice", 396.0, 612.0),
    ("ledger", 792.0, 1224.0),
    ("legal", 612.0, 1008.0),
    ("legal-13", 612.0, 936.0),
    ("letter", 612.0, 792.0),
    ("monarch", 279.0, 540.0),
    ("tabloid-extra", 864.0, 1296.0),
];

/// Box of a paper size.
pub fn paper_rect(name: &str) -> Result<Rect> {
    let (width, height) = paper_size(name)?;
    Ok(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: width,
        y1: height,
    })
}

/// Size of a paper name.
pub fn paper_size(name: &str) -> Result<(f64, f64)> {
    let key = name.trim().to_ascii_lowercase();
    PAPER_SIZES
        .iter()
        .find(|(known, _, _)| *known == key)
        .map(|(_, width, height)| (*width, *height))
        .ok_or_else(|| ErrorKind::BadValue.to_error(format!("unknown paper size {name:?}")))
}

/// Known paper names.
pub fn paper_sizes() -> Vec<String> {
    PAPER_SIZES
        .iter()
        .map(|(name, _, _)| (*name).to_string())
        .collect()
}

/// Page position of a needle.
pub fn pdf_lookup_page_loc(doc: &PdfDocument, needle: &Value) -> Result<Value> {
    let page = match needle {
        Value::Int(page) => {
            if *page < 0 || *page >= doc.page_count() as i64 {
                return Err(
                    ErrorKind::IndexOutOfRange.to_error(format!("page index {page} out of range"))
                );
            }
            *page
        }
        Value::Str(label) => doc
            .page_labels()
            .into_iter()
            .position(|shown| &shown == label)
            .ok_or_else(|| ErrorKind::UnknownKey.to_error(format!("no page {label:?}")))?
            as i64,
        _ => {
            return Err(ErrorKind::WrongType.to_error("needle is no page".to_string()));
        }
    };
    let mut map = std::collections::HashMap::new();
    map.insert("chapter".to_string(), Value::Int(0));
    map.insert("page".to_string(), Value::Int(page));
    Ok(Value::Dict(map))
}

/// Object source text.
pub fn pdfobj_string(obj: &Value, prefix: &str) -> String {
    format!("{prefix}{}", dir_str(obj))
}

/// Matrix flattening a line.
pub fn planish_line(p1: Point, p2: Point) -> Matrix {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let length = dx.hypot(dy);
    if length == 0.0 {
        return Matrix {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        };
    }
    let cos = dx / length;
    let sin = dy / length;
    Matrix {
        a: cos,
        b: -sin,
        c: sin,
        d: cos,
        e: -(cos * p1.x + sin * p1.y),
        f: sin * p1.x - cos * p1.y,
    }
}

/// Restore name aliases.
pub fn restore_aliases() {
    // No alias registry exists on this side; nothing to restore.
}

/// PDF components of an sRGB value.
pub fn srgb_to_pdf(srgb: i32) -> (f64, f64, f64) {
    srgb_to_rgb(srgb)
}

/// RGB components of an sRGB value.
pub fn srgb_to_rgb(srgb: i32) -> (f64, f64, f64) {
    let red = ((srgb >> 16) & 0xFF) as f64 / 255.0;
    let green = ((srgb >> 8) & 0xFF) as f64 / 255.0;
    let blue = (srgb & 0xFF) as f64 / 255.0;
    (red, green, blue)
}

/// Set log sinks.
pub fn set_log(_options: &LogOptions) {
    // No logging backend is wired; the call is accepted and ignored.
}

/// Set message sinks.
pub fn set_messages(_options: &LogOptions) {
    // No message backend is wired; the call is accepted and ignored.
}

/// Whether a name is listed.
pub fn string_in_names_list(text: &str, names: &[String]) -> bool {
    names.iter().any(|name| name == text)
}

/// One stripped outline row: level, title, and 1-based page.
struct StrippedRow {
    level: i32,
    title: String,
    page: i32,
}

/// Read outline rows out of the unshaped value.
fn outline_rows(outlines: &Value) -> Vec<StrippedRow> {
    let Value::List(items) = outlines else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for item in items {
        let Value::List(row) = item else { continue };
        if row.len() < 3 {
            continue;
        }
        let level = match &row[0] {
            Value::Int(level) => *level as i32,
            _ => continue,
        };
        let title = match &row[1] {
            Value::Str(title) => title.clone(),
            _ => continue,
        };
        let page = match &row[2] {
            Value::Int(page) => *page as i32,
            _ => continue,
        };
        rows.push(StrippedRow { level, title, page });
    }
    rows
}

/// Drop rows whose destination cannot resolve, then rebuild the tree.
fn strip_and_rebuild(
    doc: &mut PdfDocument,
    outlines: &Value,
    page_count: i32,
    page_object_nums: &[i32],
    names: &[String],
) -> Result<()> {
    use crate::structure::Outline;
    let rows = outline_rows(outlines)
        .into_iter()
        .filter(|row| {
            row.level >= 1
                && (row.page == 0
                    || (1 <= row.page && row.page <= page_count)
                    || page_object_nums.contains(&row.page)
                    || names.iter().any(|n| n == &row.title))
        })
        .collect::<Vec<_>>();
    // Nest by level with a stack of open parents.
    let mut roots: Vec<Outline> = Vec::new();
    // (level, index path) of the current chain.
    let mut stack: Vec<(i32, Vec<usize>)> = Vec::new();
    for row in rows {
        while stack.last().is_some_and(|(level, _)| *level >= row.level) {
            stack.pop();
        }
        let node = Outline {
            title: row.title,
            page: if row.page >= 1 {
                Some(row.page as usize - 1)
            } else {
                None
            },
            children: Vec::new(),
        };
        let parent = stack.last().map(|(_, path)| path.clone());
        if let Some(path) = parent {
            let mut siblings = &mut roots;
            for step in path.iter() {
                siblings = &mut siblings[*step].children;
            }
            siblings.push(node);
            let mut child_path = path;
            child_path.push(siblings.len() - 1);
            stack.push((row.level, child_path));
        } else {
            roots.push(node);
            stack.push((row.level, vec![roots.len() - 1]));
        }
    }
    doc.set_outlines(&roots)
}

/// Strip broken outline entries.
pub fn strip_outline(
    doc: &mut PdfDocument,
    outlines: &Value,
    page_count: i32,
    page_object_nums: &[i32],
    names: &[String],
) -> Result<()> {
    strip_and_rebuild(doc, outlines, page_count, page_object_nums, names)
}

/// Strip broken outlines.
pub fn strip_outlines(
    doc: &mut PdfDocument,
    outlines: &Value,
    page_count: i32,
    page_object_nums: &[i32],
    names: &[String],
) -> Result<()> {
    strip_and_rebuild(doc, outlines, page_count, page_object_nums, names)
}

/// Product of two matrices: apply `one` first, then `two`.
pub fn util_concat_matrix(one: Matrix, two: Matrix) -> Matrix {
    Matrix {
        a: two.a * one.a + two.c * one.b,
        b: two.b * one.a + two.d * one.b,
        c: two.a * one.c + two.c * one.d,
        d: two.b * one.c + two.d * one.d,
        e: two.a.mul_add(one.e, two.c.mul_add(one.f, two.e)),
        f: two.b.mul_add(one.e, two.d.mul_add(one.f, two.f)),
    }
}

/// Ensure widget calculations.
///
/// Takes the annotation by value: the call mutates the document behind
/// the handle, which a shared borrow cannot reach.
pub fn util_ensure_widget_calc(_annot: Annot<'_>) -> Result<()> {
    _annot.ensure_calc_order()
}

/// Matrix levelling a point.
pub fn util_hor_matrix(center: Point, point: Point) -> Matrix {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    let length = dx.hypot(dy);
    if length == 0.0 {
        return Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        };
    }
    // Shift the centre to the origin, then rotate by -angle so the
    // centre-to-point ray lies on the +x axis.
    let cos = dx / length;
    let sin = dy / length;
    Matrix {
        a: cos,
        b: -sin,
        c: sin,
        d: cos,
        e: -(cos * center.x + sin * center.y),
        f: sin * center.x - cos * center.y,
    }
}

/// Rectangle grown over a point.
pub fn util_include_point_in_rect(rect: Rect, point: Point) -> Rect {
    Rect {
        x0: rect.x0.min(point.x),
        y0: rect.y0.min(point.y),
        x1: rect.x1.max(point.x),
        y1: rect.y1.max(point.y),
    }
}

/// Overlap of two rectangles.
pub fn util_intersect_rect(one: Rect, two: Rect) -> Rect {
    Rect {
        x0: one.x0.max(two.x0),
        y0: one.y0.max(two.y0),
        x1: one.x1.min(two.x1),
        y1: one.y1.min(two.y1),
    }
}

/// Inverse of a matrix.
pub fn util_invert_matrix(matrix: Matrix) -> Matrix {
    let det = matrix.a * matrix.d - matrix.b * matrix.c;
    if det == 0.0 {
        return Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        };
    }
    Matrix {
        a: matrix.d / det,
        b: -matrix.b / det,
        c: -matrix.c / det,
        d: matrix.a / det,
        e: (matrix.c * matrix.f - matrix.d * matrix.e) / det,
        f: (matrix.b * matrix.e - matrix.a * matrix.f) / det,
    }
}

/// Point containment code.
pub fn util_is_point_in_rect(point: Point, rect: Rect) -> i32 {
    i32::from(
        rect.x0.min(rect.x1) <= point.x
            && point.x <= rect.x0.max(rect.x1)
            && rect.y0.min(rect.y1) <= point.y
            && point.y <= rect.y0.max(rect.y1),
    )
}

/// Integer rectangle from edges.
pub fn util_make_irect(x0: i32, y0: i32, x1: i32, y1: i32) -> IRect {
    IRect { x0, y0, x1, y1 }
}

/// Rectangle from edges.
pub fn util_make_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect { x0, y0, x1, y1 }
}

/// The standard face a measurement font name asks for: the fourteen
/// PostScript names, case-insensitively. Short aliases (`helv`, `hebo`)
/// are not measurement names and read as unknown.
pub fn resolve_measure_font(fontname: &str) -> Option<pdf_interpret::font::StandardFont> {
    use pdf_interpret::font::StandardFont as Face;
    match fontname.to_ascii_lowercase().as_str() {
        "helvetica" => Some(Face::Helvetica),
        "helvetica-bold" => Some(Face::HelveticaBold),
        "helvetica-oblique" => Some(Face::HelveticaOblique),
        "helvetica-boldoblique" => Some(Face::HelveticaBoldOblique),
        "courier" => Some(Face::Courier),
        "courier-bold" => Some(Face::CourierBold),
        "courier-oblique" => Some(Face::CourierOblique),
        "courier-boldoblique" => Some(Face::CourierBoldOblique),
        "times-roman" => Some(Face::TimesRoman),
        "times-bold" => Some(Face::TimesBold),
        "times-italic" => Some(Face::TimesItalic),
        "times-bolditalic" => Some(Face::TimesBoldItalic),
        "symbol" => Some(Face::Symbol),
        "zapfdingbats" => Some(Face::ZapfDingBats),
        _ => None,
    }
}

/// Typeset width of a string: the face's thousandths-of-an-em advances
/// summed over the characters, scaled by the point size. Characters the
/// face does not cover ride at the `.notdef` advance; an unknown face
/// measures zero and the bindings reject it before calling.
pub fn util_measure_string(text: &str, fontname: &str, fontsize: f64, _encoding: i32) -> f64 {
    let Some(face) = resolve_measure_font(fontname) else {
        return 0.0;
    };
    let mut thousandths = 0.0;
    for ch in text.chars() {
        thousandths += face.char_width(ch).unwrap_or(250.0) as f64;
    }
    thousandths * fontsize / 1000.0
}

/// Whether a quad covers a point.
pub fn util_point_in_quad(point: Point, quad: Quad) -> bool {
    // Convex-quad same-side test over the ul -> ur -> lr -> ll loop.
    let corners = [quad.ul, quad.ur, quad.lr, quad.ll];
    let mut sign = 0.0;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross != 0.0 {
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return false;
            }
        }
    }
    true
}

/// Rounded integer copy: the low edges floor down and the high edges ceil
/// up, so the integer box covers the float one.
pub fn util_round_rect(rect: Rect) -> IRect {
    IRect {
        x0: rect.x0.floor() as i32,
        y0: rect.y0.floor() as i32,
        x1: rect.x1.ceil() as i32,
        y1: rect.y1.ceil() as i32,
    }
}

/// Sine of half the angle between the centre-to-point and
/// centre-to-other rays: a right angle reads about 0.7071, a shared ray
/// reads zero, and a degenerate ray reads zero.
pub fn util_sine_between(center: Point, point: Point, other: Point) -> f64 {
    let ax = point.x - center.x;
    let ay = point.y - center.y;
    let bx = other.x - center.x;
    let by = other.y - center.y;
    let lengths = ax.hypot(ay) * bx.hypot(by);
    if lengths == 0.0 {
        0.0
    } else {
        let cosine = ((ax * bx + ay * by) / lengths).clamp(-1.0, 1.0);
        ((1.0 - cosine) / 2.0).sqrt()
    }
}

/// Transformed point.
pub fn util_transform_point(point: Point, matrix: Matrix) -> Point {
    Point {
        x: matrix
            .a
            .mul_add(point.x, matrix.c.mul_add(point.y, matrix.e)),
        y: matrix
            .b
            .mul_add(point.x, matrix.d.mul_add(point.y, matrix.f)),
    }
}

/// Transformed rectangle.
pub fn util_transform_rect(rect: Rect, matrix: Matrix) -> Rect {
    let corners = [
        util_transform_point(
            Point {
                x: rect.x0,
                y: rect.y0,
            },
            matrix,
        ),
        util_transform_point(
            Point {
                x: rect.x1,
                y: rect.y0,
            },
            matrix,
        ),
        util_transform_point(
            Point {
                x: rect.x0,
                y: rect.y1,
            },
            matrix,
        ),
        util_transform_point(
            Point {
                x: rect.x1,
                y: rect.y1,
            },
            matrix,
        ),
    ];
    Rect {
        x0: corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        y0: corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        x1: corners
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max),
        y1: corners
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max),
    }
}

/// Cover of two rectangles.
pub fn util_union_rect(one: Rect, two: Rect) -> Rect {
    Rect {
        x0: one.x0.min(two.x0),
        y0: one.y0.min(two.y0),
        x1: one.x1.max(two.x1),
        y1: one.y1.max(two.y1),
    }
}

/// Vertical distance.
pub fn vdist(_direction: &Value, a: &Value, b: &Value) -> f64 {
    pair_distance(a, b, false)
}

#[cfg(test)]
mod recorded {
    //! The recorded pins for the behaviours fixed here: each test fails
    //! when its behaviour is removed or drifts.

    use super::*;

    /// The gate's own float bar (`float_rel` in tolerance.toml): the
    /// recordings carry float-32 dust the float-64 maths need not repeat.
    fn close(got: f64, want: f64) -> bool {
        (got - want).abs() <= 1e-6 * want.abs().max(1.0)
    }

    #[test]
    fn colour_codes_match() {
        let rgb = Value::List(vec![Value::Int(1), Value::Int(0), Value::Int(0)]);
        assert_eq!(color_code(&rgb, "RGB"), "1 0 0 rg ");
        assert_eq!(color_code(&Value::Null, "RGB"), "");
        assert_eq!(color_code(&Value::Float(0.5), "G"), ".5 g ");
        let cmyk = Value::List(vec![
            Value::Float(0.0),
            Value::Float(0.5),
            Value::Float(1.0),
            Value::Int(0),
        ]);
        assert_eq!(color_code(&cmyk, "CMYK"), "0 .5 1 0 k ");
    }

    #[test]
    fn conversion_texts_match() {
        let head = conversion_header("xhtml", None);
        assert!(head.starts_with("\n<?xml version=\"1.0\"?>\n<!DOCTYPE html"));
        assert!(head.ends_with("<body>\n"));
        assert_eq!(conversion_header("xhtml", Some("f.pdf")), head);
        assert_eq!(
            conversion_header("xml", None),
            "\n<?xml version=\"1.0\"?>\n<document name=\"unknown\">\n"
        );
        assert_eq!(conversion_header("bogus", None), "");
        assert_eq!(conversion_trailer("xhtml"), "</body>\n</html>\n");
        assert_eq!(conversion_trailer("xml"), "</document>\n");
        assert_eq!(conversion_trailer("bogus"), "");
    }

    #[test]
    fn empty_and_infinite_boxes_match() {
        let empty = empty_rect();
        assert_eq!(
            (empty.x0, empty.y0, empty.x1, empty.y1),
            (2147483520.0, 2147483520.0, -2147483648.0, -2147483648.0)
        );
        let full = infinite_rect();
        assert_eq!(
            (full.x0, full.y0, full.x1, full.y1),
            (-2147483648.0, -2147483648.0, 2147483520.0, 2147483520.0)
        );
        let ei = empty_irect();
        assert_eq!(
            (ei.x0, ei.y0, ei.x1, ei.y1),
            (2147483520, 2147483520, -2147483648, -2147483648)
        );
        let quad = empty_quad();
        assert_eq!((quad.ul.x, quad.ul.y), (2147483520.0, 2147483520.0));
        assert_eq!((quad.lr.x, quad.lr.y), (-2147483648.0, -2147483648.0));
    }

    #[test]
    fn tj_strings_match() {
        assert_eq!(get_tjstr("abc", None, true, 0), "[<616263>]");
        assert_eq!(get_tjstr("abc", None, false, 0), "[<006100620063>]");
        assert_eq!(
            get_tjstr("abc", Some(&[1, 2, 3]), false, 0),
            "[<006100620063>]"
        );
        assert_eq!(get_tjstr("", None, true, 0), "[<>]");
    }

    #[test]
    fn pdf_strings_match() {
        assert_eq!(get_pdf_str("Metadata"), "(Metadata)");
        assert_eq!(get_pdf_str(""), "()");
        assert_eq!(get_pdf_str("a(b)c"), "(a\\(b\\)c)");
        assert_eq!(get_pdf_str("a\\b"), "(a\\\\b)");
        assert_eq!(get_pdf_str("café"), "(caf\\351)");
        assert_eq!(get_pdf_str("a\nb"), "(a\\nb)");
    }

    #[test]
    fn paper_table_matches() {
        assert_eq!(paper_sizes().len(), 44);
        assert_eq!(paper_size("a4").unwrap(), (595.0, 842.0));
        assert_eq!(paper_size("A4").unwrap(), (595.0, 842.0));
        assert_eq!(paper_size("ledger").unwrap(), (792.0, 1224.0));
        assert_eq!(paper_size("a6").unwrap(), (298.0, 420.0));
        assert_eq!(paper_size("a8").unwrap(), (147.0, 210.0));
        assert!(paper_size("bogus-size").is_err());
        let rect = paper_rect("letter").unwrap();
        assert_eq!(
            (rect.x0, rect.y0, rect.x1, rect.y1),
            (0.0, 0.0, 612.0, 792.0)
        );
    }

    #[test]
    fn planish_and_hor_matrices_match() {
        let flat = planish_line(Point { x: 0.0, y: 0.0 }, Point { x: 3.0, y: 4.0 });
        assert!(close(flat.a, 0.6) && close(flat.b, -0.8));
        assert!(close(flat.c, 0.8) && close(flat.d, 0.6));
        assert_eq!((flat.e, flat.f), (0.0, 0.0));
        let zero = planish_line(Point { x: 1.0, y: 1.0 }, Point { x: 1.0, y: 1.0 });
        assert_eq!(
            (zero.a, zero.b, zero.c, zero.d, zero.e, zero.f),
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        );
        let hor = util_hor_matrix(Point { x: 10.0, y: 10.0 }, Point { x: 20.0, y: 30.0 });
        assert!(close(hor.a, 0.4472135901451111));
        assert!(close(hor.b, -0.8944271802902222));
        assert!(close(hor.e, -13.41640853881836));
        assert!(close(hor.f, 4.4721360206604));
    }

    #[test]
    fn round_and_sine_match() {
        let rounded = util_round_rect(Rect {
            x0: 1.2,
            y0: 2.7,
            x1: 3.5,
            y1: 4.4,
        });
        assert_eq!(
            (rounded.x0, rounded.y0, rounded.x1, rounded.y1),
            (1, 2, 4, 5)
        );
        let sine = util_sine_between(
            Point { x: 0.0, y: 0.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: 0.0, y: 1.0 },
        );
        assert!(close(sine, 0.7071068286895752));
        let same = util_sine_between(
            Point { x: 0.0, y: 0.0 },
            Point { x: 1.0, y: 0.0 },
            Point { x: 2.0, y: 0.0 },
        );
        assert_eq!(same, 0.0);
    }

    #[test]
    fn measured_widths_match() {
        assert!(resolve_measure_font("helv").is_none());
        assert!(resolve_measure_font("Helvetica").is_some());
        assert!(close(
            util_measure_string("hello", "Helvetica", 11.0, 0),
            23.23199987411499
        ));
        assert!(close(
            util_measure_string("hello", "Courier", 11.0, 0),
            33.000001311302185
        ));
        assert!(close(
            util_measure_string("hello", "Helvetica", 20.0, 0),
            42.239999771118164
        ));
        assert_eq!(util_measure_string("", "Helvetica", 11.0, 0), 0.0);
    }

    #[test]
    fn match_and_escape_match() {
        let hello = Value::Str("hello".to_string());
        assert!(match_string(&hello, &Value::Str("hell".to_string())));
        assert!(!match_string(&hello, &Value::Str("hx".to_string())));
        assert!(!match_string(
            &hello,
            &Value::Str("hello world".to_string())
        ));
        assert_eq!(make_escape('\n'), "\n");
        assert_eq!(make_escape('A'), "A");
        assert_eq!(make_escape('\0'), "\\u0000");
        assert_eq!(make_escape('\u{20AC}'), "\\u20ac");
        assert!(check_parent(&Value::Int(42)).is_ok());
    }

    #[test]
    fn highlight_selection_trims_ends() {
        let lines = vec![
            SpanLine {
                bbox: Rect {
                    x0: 72.0,
                    y0: 94.0,
                    x1: 163.0,
                    y1: 117.0,
                },
                spans: vec![Rect {
                    x0: 72.0,
                    y0: 94.0,
                    x1: 163.0,
                    y1: 117.0,
                }],
            },
            SpanLine {
                bbox: Rect {
                    x0: 72.0,
                    y0: 125.0,
                    x1: 241.0,
                    y1: 139.0,
                },
                spans: vec![
                    Rect {
                        x0: 72.0,
                        y0: 125.0,
                        x1: 75.5,
                        y1: 139.0,
                    },
                    Rect {
                        x0: 86.0,
                        y0: 125.0,
                        x1: 241.0,
                        y1: 139.0,
                    },
                ],
            },
        ];
        let whole = highlight_span_rects(&lines, None, None, None);
        assert_eq!(whole.len(), 3);
        let span = highlight_span_rects(
            &lines,
            Some(Point { x: 72.0, y: 100.0 }),
            Some(Point { x: 200.0, y: 130.0 }),
            None,
        );
        assert_eq!(span.len(), 3);
        assert_eq!(span[2].x1, 200.0);
        let past = highlight_span_rects(&lines, Some(Point { x: 0.0, y: 500.0 }), None, None);
        assert!(past.is_empty());
    }
}
