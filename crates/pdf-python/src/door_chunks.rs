//! Door lane L3: chunks, JSON and form fields (SPEC sections 6, 14, 22).
//!
//! Native helpers behind `pdfluent_extract._chunks`: the file metadata and
//! outline a chunk carries, form fields with pages and xrefs, per-page image
//! records with placement, vector rulings and fills, link annotations and
//! table proposals. Coordinates leave here in the y-down frame the door
//! speaks (origin top-left); the engine's y-up boxes are flipped on the way
//! out.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::{BTreeMap, BTreeSet};

use pdf_engine::{BlockKind, ExtractOptions};
use pdf_forms::{parse_acroform, FieldValue};
use pdf_syntax::object::dict::keys::{
    A, D, DEST, FIRST, N, NAMES, NEXT, NM, OUTLINES, RECT, S, SUBTYPE, TITLE, URI,
};
use pdf_syntax::object::{Array, Dict, Name, ObjectIdentifier};
use pdf_syntax::object::{Stream, String as PdfString};
use pdf_syntax::Pdf;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::{engine_err_to_py, PyDocument};

/// Register this lane's native items on the module.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(door_file_metadata, m)?)?;
    m.add_function(wrap_pyfunction!(door_toc, m)?)?;
    m.add_function(wrap_pyfunction!(door_form_details, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_drawings, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_links, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_tables, m)?)?;
    Ok(())
}

/// Decode a PDF text string, the way document info and outline titles read.
///
/// A leading byte-order mark selects UTF-16BE (or UTF-8); anything else is
/// PDFDocEncoding, which is Latin-1 except for the punctuation block below
/// U+00A0. Decoding never fails: undefined codes become U+FFFD.
fn text_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let mut units = Vec::with_capacity(bytes.len() / 2);
        let mut rest = &bytes[2..];
        while rest.len() >= 2 {
            units.push(u16::from_be_bytes([rest[0], rest[1]]));
            rest = &rest[2..];
        }
        if let Some(&last) = rest.first() {
            units.push(u16::from_be_bytes([last, 0]));
        }
        return String::from_utf16_lossy(&units);
    }
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let code = match byte {
            0x18 => 0x02D8,
            0x19 => 0x02C7,
            0x1A => 0x02C6,
            0x1B => 0x02D9,
            0x1C => 0x02DD,
            0x1D => 0x02DB,
            0x1E => 0x02DA,
            0x1F => 0x02DC,
            0x80 => 0x2022,
            0x81 => 0x2020,
            0x82 => 0x2021,
            0x83 => 0x2026,
            0x84 => 0x2014,
            0x85 => 0x2013,
            0x86 => 0x0192,
            0x87 => 0x2044,
            0x88 => 0x2039,
            0x89 => 0x203A,
            0x8A => 0x2212,
            0x8B => 0x2030,
            0x8C => 0x201E,
            0x8D => 0x201C,
            0x8E => 0x201D,
            0x8F => 0x2018,
            0x90 => 0x2019,
            0x91 => 0x201A,
            0x92 => 0x2122,
            0x93 => 0xFB01,
            0x94 => 0xFB02,
            0x95 => 0x0141,
            0x96 => 0x0152,
            0x97 => 0x0160,
            0x98 => 0x0178,
            0x99 => 0x017D,
            0x9A => 0x0131,
            0x9B => 0x0142,
            0x9C => 0x0153,
            0x9D => 0x0161,
            0x9E => 0x017E,
            0xA0 => 0x20AC,
            0x7F | 0x9F => 0xFFFD,
            _ => byte as u32,
        };
        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
    }
    out
}

/// Flip a y-up box into the door's y-down frame against the page height.
fn flip_y(b: [f64; 4], height: f64) -> [f64; 4] {
    [b[0], height - b[3], b[2], height - b[1]]
}

/// Read a four-number rectangle array as floats.
pub(crate) fn array_rect(array: Array<'_>) -> Option<[f64; 4]> {
    let mut numbers = array.iter::<pdf_syntax::object::Number>();
    let x0 = numbers.next()?.as_f64();
    let y0 = numbers.next()?.as_f64();
    let x1 = numbers.next()?.as_f64();
    let y1 = numbers.next()?.as_f64();
    Some([x0, y0, x1, y1])
}

fn name_text(name: &Name) -> String {
    String::from_utf8_lossy(name.as_ref()).into_owned()
}

/// The eleven file keys every chunk metadata and JSON metadata carries.
///
/// Short dates render in full `D:YYYYMMDDHHmmSS` form with the recorded zone
/// suffix; partial file dates come back expanded, which no recorded fixture
/// distinguishes. `trapped` is always empty: the syntax layer keeps no raw
/// info-dict access to read it from. `encryption` is None unless the file
/// carries an Encrypt marker, in which case it is 0 (present but ungraded).
#[pyfunction]
fn door_file_metadata(py: Python<'_>, doc: &PyDocument) -> PyResult<PyObject> {
    let engine = doc.engine()?;
    let pdf = engine.pdf();
    let meta = pdf.metadata();
    let dict = PyDict::new(py);
    let version = match pdf.version() {
        pdf_syntax::PdfVersion::Pdf10 => "PDF 1.0",
        pdf_syntax::PdfVersion::Pdf11 => "PDF 1.1",
        pdf_syntax::PdfVersion::Pdf12 => "PDF 1.2",
        pdf_syntax::PdfVersion::Pdf13 => "PDF 1.3",
        pdf_syntax::PdfVersion::Pdf14 => "PDF 1.4",
        pdf_syntax::PdfVersion::Pdf15 => "PDF 1.5",
        pdf_syntax::PdfVersion::Pdf16 => "PDF 1.6",
        pdf_syntax::PdfVersion::Pdf17 => "PDF 1.7",
        pdf_syntax::PdfVersion::Pdf20 => "PDF 2.0",
    };
    dict.set_item("format", version)?;
    let string_or_empty = |value: &Option<Vec<u8>>| -> String {
        value.as_ref().map(|b| text_string(b)).unwrap_or_default()
    };
    dict.set_item("title", string_or_empty(&meta.title))?;
    dict.set_item("author", string_or_empty(&meta.author))?;
    dict.set_item("subject", string_or_empty(&meta.subject))?;
    dict.set_item("keywords", string_or_empty(&meta.keywords))?;
    dict.set_item("creator", string_or_empty(&meta.creator))?;
    dict.set_item("producer", string_or_empty(&meta.producer))?;
    let render_date = |value: &Option<pdf_syntax::object::DateTime>| -> String {
        let Some(date) = value else {
            return String::new();
        };
        let mut out = format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}",
            date.year, date.month, date.day, date.hour, date.minute, date.second
        );
        if date.has_timezone {
            let sign = if date.utc_offset_hour < 0 { '-' } else { '+' };
            out.push(sign);
            out.push_str(&format!(
                "{:02}'{:02}'",
                date.utc_offset_hour.abs(),
                date.utc_offset_minute
            ));
        }
        out
    };
    dict.set_item("creationDate", render_date(&meta.creation_date))?;
    dict.set_item("modDate", render_date(&meta.modification_date))?;
    dict.set_item("trapped", "")?;
    let bytes: &[u8] = pdf.data().as_ref();
    let encrypted = bytes
        .windows(b"/Encrypt".len())
        .any(|window| window == b"/Encrypt".as_slice());
    if encrypted {
        dict.set_item("encryption", 0)?;
    } else {
        dict.set_item("encryption", py.None())?;
    }
    Ok(dict.into())
}

/// The object id of every page, in page order; holes stay holes.
fn page_object_ids(pdf: &Pdf) -> Vec<Option<ObjectIdentifier>> {
    pdf.pages().iter().map(|p| p.raw().obj_id()).collect()
}

/// The 0-based page an explicit destination array points at.
fn explicit_dest_page(
    dest: Option<Array<'_>>,
    pages: &[Option<ObjectIdentifier>],
) -> Option<usize> {
    let first = dest?.raw_iter().next()?;
    let target = ObjectIdentifier::from(pdf_syntax::object::ObjRef::try_from(first).ok()?);
    pages.iter().position(|id| *id == Some(target))
}

/// Look a named destination up through the catalog's old dict.
///
/// The `/Names` name tree is consulted first only when `use_name_tree` holds:
/// the outline path resolves through `/Dests` alone, so string names that
/// live only in the tree resolve nowhere there (recorded as page 0).
fn named_dest_page(
    pdf: &Pdf,
    name: &[u8],
    pages: &[Option<ObjectIdentifier>],
    use_name_tree: bool,
) -> Option<usize> {
    let xref = pdf.xref();
    let catalog: Dict<'_> = xref.get(xref.root_id())?;
    if use_name_tree {
        if let Some(names) = catalog.get::<Dict<'_>>(b"Names".as_slice()) {
            if let Some(dests) = names.get::<Dict<'_>>(b"Dests".as_slice()) {
                if let Some(page) = name_tree_dest_page(pdf, &dests, name, pages, 0) {
                    return Some(page);
                }
            }
        }
    }
    let dests: Dict<'_> = catalog.get(b"Dests".as_slice())?;
    let dest = dests.get::<Array<'_>>(name)?;
    explicit_dest_page(Some(dest), pages)
}

/// Walk one name-tree node for a destination name (depth-capped).
fn name_tree_dest_page(
    pdf: &Pdf,
    node: &Dict<'_>,
    name: &[u8],
    pages: &[Option<ObjectIdentifier>],
    depth: usize,
) -> Option<usize> {
    if depth > 16 {
        return None;
    }
    if let Some(pairs) = node.get::<Array<'_>>(NAMES) {
        let mut items = pairs.raw_iter();
        while let (Some(key), Some(value)) = (items.next(), items.next()) {
            let key = match key {
                pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::String(s)) => {
                    s.as_bytes().to_vec()
                }
                pdf_syntax::object::MaybeRef::Ref(r) => pdf
                    .xref()
                    .get::<PdfString>(r.into())
                    .map(|s| s.as_bytes().to_vec())?,
                _ => continue,
            };
            if key != name {
                continue;
            }
            let array = match value {
                pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Array(array)) => {
                    Some(array)
                }
                pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Dict(dict)) => {
                    dict.get::<Array<'_>>(D)
                }
                pdf_syntax::object::MaybeRef::Ref(r) => {
                    pdf.xref().get::<Array<'_>>(r.into()).or_else(|| {
                        pdf.xref()
                            .get::<Dict<'_>>(r.into())
                            .and_then(|dict| dict.get::<Array<'_>>(D))
                    })
                }
                _ => None,
            };
            return explicit_dest_page(array, pages);
        }
    }
    if let Some(kids) = node.get::<Array<'_>>(b"Kids".as_slice()) {
        for kid in kids.iter::<Dict<'_>>() {
            if let Some(page) = name_tree_dest_page(pdf, &kid, name, pages, depth + 1) {
                return Some(page);
            }
        }
    }
    None
}

/// The 0-based page an outline item points at: explicit, action or named.
fn outline_item_page(
    item: &Dict<'_>,
    pdf: &Pdf,
    pages: &[Option<ObjectIdentifier>],
) -> Option<usize> {
    if let Some(dest) = item.get::<Array<'_>>(DEST) {
        if let Some(page) = explicit_dest_page(Some(dest), pages) {
            return Some(page);
        }
    }
    if let Some(name) = item.get::<PdfString>(DEST) {
        return named_dest_page(pdf, name.as_bytes(), pages, false);
    }
    if let Some(name) = item.get::<Name>(DEST) {
        return named_dest_page(pdf, name.as_ref(), pages, false);
    }
    let action = item.get::<Dict<'_>>(A)?;
    if action.get::<Name>(S).as_deref() != Some(b"GoTo".as_slice()) {
        return None;
    }
    if action.contains_key(b"F".as_slice()) {
        return None;
    }
    if let Some(dest) = action.get::<Array<'_>>(D) {
        if let Some(page) = explicit_dest_page(Some(dest), pages) {
            return Some(page);
        }
    }
    if let Some(name) = action.get::<PdfString>(D) {
        return named_dest_page(pdf, name.as_bytes(), pages, false);
    }
    if let Some(name) = action.get::<Name>(D) {
        return named_dest_page(pdf, name.as_ref(), pages, false);
    }
    None
}

/// Walk one outline level, flattening to `[level, title, page]` rows.
fn outline_rows(
    first: &Dict<'_>,
    level: i64,
    pdf: &Pdf,
    pages: &[Option<ObjectIdentifier>],
    visited: &mut BTreeSet<ObjectIdentifier>,
    out: &mut Vec<(i64, String, i64)>,
) {
    if level > 100 {
        return;
    }
    let mut current: Option<Dict<'_>> = Some(first.clone());
    while let Some(dict) = current {
        if let Some(id) = dict.obj_id() {
            if !visited.insert(id) {
                break;
            }
        }
        let title = dict
            .get::<PdfString>(TITLE)
            .map(|s| text_string(s.as_bytes()))
            .unwrap_or_default();
        let page = outline_item_page(&dict, pdf, pages)
            .map(|p| p as i64 + 1)
            .unwrap_or(0);
        out.push((level, title, page));
        if let Some(child) = dict.get::<Dict<'_>>(FIRST) {
            outline_rows(&child, level + 1, pdf, pages, visited, out);
        }
        current = dict.get::<Dict<'_>>(NEXT);
    }
}

/// The document outline as `[[level, title, page]]` rows.
///
/// Levels count from 1; pages count from 1 with 0 for entries that resolve
/// nowhere. Named destinations resolve through the catalog `/Dests` dict
/// alone; the `/Names` tree stays out of the outline path, as do remote
/// targets and non-go-to actions.
#[pyfunction]
fn door_toc(py: Python<'_>, doc: &PyDocument) -> PyResult<PyObject> {
    let engine = doc.engine()?;
    let pdf = engine.pdf();
    let mut out = Vec::new();
    let xref = pdf.xref();
    if let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) {
        if let Some(outlines) = catalog.get::<Dict<'_>>(OUTLINES) {
            if let Some(first) = outlines.get::<Dict<'_>>(FIRST) {
                let pages = page_object_ids(pdf);
                outline_rows(&first, 1, pdf, &pages, &mut BTreeSet::new(), &mut out);
            }
        }
    }
    let rows = PyList::empty(py);
    for (level, title, page) in out {
        let row = PyList::empty(py);
        row.append(level)?;
        row.append(title)?;
        row.append(page)?;
        rows.append(row)?;
    }
    Ok(rows.into())
}

/// One form field's merged value, pages and xref, keyed by field name.
#[pyfunction]
fn door_form_details(py: Python<'_>, doc: &PyDocument) -> PyResult<PyObject> {
    let engine = doc.engine()?;
    let pdf = engine.pdf();
    let rows = PyList::empty(py);
    let Some(tree) = parse_acroform(pdf) else {
        return Ok(rows.into());
    };
    // Terminal widgets merge under their fully qualified name: one field may
    // own widgets on several pages. First value and xref win; pages unite.
    let mut order: Vec<String> = Vec::new();
    let mut merged: BTreeMap<String, (Option<String>, BTreeSet<usize>, i64)> = BTreeMap::new();
    for id in tree.terminal_fields() {
        let name = tree.fully_qualified_name(id);
        let node = tree.get(id);
        let value = tree.effective_value(id).map(|v| match v {
            FieldValue::Text(s) => s.clone(),
            FieldValue::StringArray(items) => items.join(", "),
        });
        let xref = node.object_id.map(|(number, _)| number as i64).unwrap_or(0);
        let entry = merged.entry(name.clone()).or_insert_with(|| {
            order.push(name);
            (None, BTreeSet::new(), xref)
        });
        if entry.0.is_none() {
            entry.0.clone_from(&value);
        }
        if let Some(page) = node.page_index {
            entry.1.insert(page);
        }
    }
    for name in order {
        let (value, pages, xref) = merged.remove(&name).unwrap_or_default();
        let record = PyDict::new(py);
        record.set_item("name", name)?;
        match value {
            Some(text) => record.set_item("value", text)?,
            None => record.set_item("value", py.None())?,
        }
        let pages: Vec<u64> = pages.into_iter().map(|p| p as u64).collect();
        record.set_item("pages", pages)?;
        record.set_item("xref", xref)?;
        rows.append(record)?;
    }
    Ok(rows.into())
}

/// One placed raster image, details read off its XObject in content order.
struct PlacedImage {
    xref: i64,
    width: i64,
    height: i64,
    bpc: i64,
    cs_num: i64,
    cs_name: String,
    has_mask: bool,
    size: usize,
}

/// Component count of a plain color-space name; 0 when unknown.
fn plain_cs_components(name: &str) -> i64 {
    match name {
        "DeviceGray" | "CalGray" => 1,
        "DeviceRGB" | "CalRGB" => 3,
        "DeviceCMYK" | "CalCMYK" => 4,
        _ => 0,
    }
}

/// Component count and display name of an image color space.
///
/// Arrays resolve one level: ICC profiles read `/N`, separations and indexed
/// spaces defer to their alternate or base. Anything unreadable reports 0
/// components under its family name.
fn image_colorspace(pdf: &Pdf, dict: &Dict<'_>) -> (i64, String) {
    if let Some(name) = dict.get::<Name>(b"ColorSpace".as_slice()) {
        let text = name_text(&name);
        return (plain_cs_components(&text), text);
    }
    let Some(space) = dict.get::<Array<'_>>(b"ColorSpace".as_slice()) else {
        return (0, String::new());
    };
    let mut items = space.iter::<Name>();
    let Some(family) = items.next().map(|n| name_text(&n)) else {
        return (0, String::new());
    };
    match family.as_str() {
        "ICCBased" => {
            let mut raw = space.raw_iter();
            raw.next();
            let num = raw
                .next()
                .and_then(|item| match item {
                    pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Stream(
                        stream,
                    )) => Some(stream),
                    pdf_syntax::object::MaybeRef::Ref(r) => pdf.xref().get::<Stream>(r.into()),
                    _ => None,
                })
                .and_then(|stream| stream.dict().get::<pdf_syntax::object::Number>(N))
                .map(|n| n.as_i64())
                .unwrap_or(0);
            (num, family)
        }
        "Separation" | "DeviceN" => {
            let mut raw = space.raw_iter();
            raw.next();
            raw.next();
            let num = raw
                .next()
                .map(|item| match item {
                    pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Name(
                        name,
                    )) => plain_cs_components(&name_text(&name)),
                    pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Array(
                        inner,
                    )) => inner
                        .iter::<Name>()
                        .next()
                        .map(|n| plain_cs_components(&name_text(&n)))
                        .unwrap_or(0),
                    _ => 0,
                })
                .unwrap_or(0);
            (num, family)
        }
        "Indexed" => {
            let mut raw = space.raw_iter();
            raw.next();
            let num = raw
                .next()
                .map(|item| match item {
                    pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Name(
                        name,
                    )) => plain_cs_components(&name_text(&name)),
                    pdf_syntax::object::MaybeRef::NotRef(pdf_syntax::object::Object::Array(
                        inner,
                    )) => inner
                        .iter::<Name>()
                        .next()
                        .map(|n| plain_cs_components(&name_text(&n)))
                        .unwrap_or(0),
                    _ => 0,
                })
                .unwrap_or(0);
            (num, family)
        }
        _ => (plain_cs_components(&family), family),
    }
}

fn int_entry(dict: &Dict<'_>, key: &[u8]) -> Option<i64> {
    dict.get::<pdf_syntax::object::Number>(key)
        .map(|n| n.as_i64())
}

/// Read one image XObject's details; stencil masks ride along as 1-bit rows.
fn read_image(pdf: &Pdf, stream: &Stream<'_>) -> Option<PlacedImage> {
    let dict = stream.dict();
    let (cs_num, cs_name) = image_colorspace(pdf, dict);
    Some(PlacedImage {
        xref: stream.obj_id().map(|id| id.obj_number as i64).unwrap_or(0),
        width: int_entry(dict, b"Width").unwrap_or(0),
        height: int_entry(dict, b"Height").unwrap_or(0),
        bpc: int_entry(dict, b"BitsPerComponent").unwrap_or(1),
        cs_num,
        cs_name,
        has_mask: dict.contains_key(b"SMask".as_slice()) || dict.contains_key(b"Mask".as_slice()),
        size: stream.raw_data().len(),
    })
}

/// The resource names a `Do` operator paints, in content order.
///
/// Form XObjects expand in place with their own resources, so nested images
/// keep drawing order against the device boxes. Inline images have no name
/// and are skipped, matching the interpreter's placed-image list.
fn collect_placed(
    pdf: &Pdf,
    resources: &pdf_syntax::page::Resources<'_>,
    ops: &mut pdf_syntax::content::UntypedIter<'_>,
    visited: &mut BTreeSet<ObjectIdentifier>,
    depth: usize,
    out: &mut Vec<PlacedImage>,
) {
    if depth > 8 {
        return;
    }
    for instruction in ops.by_ref() {
        let operator: &[u8] = &instruction.operator;
        if operator != b"Do".as_slice() {
            continue;
        }
        let Some(pdf_syntax::object::Object::Name(name)) = instruction.operands().next() else {
            continue;
        };
        let Some(stream) = resources.get_x_object(name) else {
            continue;
        };
        let kind = stream
            .dict()
            .get::<Name>(b"Subtype".as_slice())
            .map(|n| name_text(&n))
            .unwrap_or_default();
        if kind == "Image" {
            if let Some(image) = read_image(pdf, &stream) {
                out.push(image);
            }
        } else if kind == "Form" {
            if let Some(id) = stream.obj_id() {
                if !visited.insert(id) {
                    continue;
                }
            }
            let Ok(bytes) = stream.decoded() else {
                continue;
            };
            if let Some(dict) = stream.dict().get::<Dict<'_>>(b"Resources".as_slice()) {
                let nested = pdf_syntax::page::Resources::new(dict);
                let mut inner = pdf_syntax::content::UntypedIter::new(&bytes);
                collect_placed(pdf, &nested, &mut inner, visited, depth + 1, out);
            }
        }
    }
}

/// One page's drawings: placed images, rulings, fills and graphic clusters.
///
/// Images pair with the interpreter's boxes in content order; whatever boxes
/// remain past the placed images are the vector clusters. All boxes come out
/// y-down.
///
/// The image record straddles two precisions on purpose: its `bbox` flips
/// with the engine height, the way the recorded layout picture boxes read
/// it, while its `transform` flips with the exact page height, the way the
/// recorded legacy transforms read it.
#[pyfunction]
fn door_page_drawings(py: Python<'_>, doc: &PyDocument, page: usize) -> PyResult<PyObject> {
    let engine = doc.engine()?;
    let pdf = engine.pdf();
    let out = PyDict::new(py);
    let images = PyList::empty(py);
    let rulings = PyList::empty(py);
    let fills = PyList::empty(py);
    let graphics = PyList::empty(py);
    out.set_item("images", &images)?;
    out.set_item("rulings", &rulings)?;
    out.set_item("fills", &fills)?;
    out.set_item("graphics", &graphics)?;
    let Some(page_handle) = pdf.pages().get(page) else {
        return Ok(out.into());
    };
    let height = crate::door_geometry::exact_page_size(&engine, page).1;
    let engine_height = engine
        .page_geometry(page)
        .map(|geometry| geometry.effective_dimensions().1)
        .unwrap_or(height);
    let mut placed = Vec::new();
    let mut ops = page_handle.operations();
    collect_placed(
        pdf,
        page_handle.resources(),
        &mut ops,
        &mut BTreeSet::new(),
        0,
        &mut placed,
    );
    let boxes = doc
        .engine()?
        .extract_image_boxes()
        .map_err(engine_err_to_py)?;
    let empty = Vec::new();
    let page_boxes = boxes.get(page).unwrap_or(&empty);
    let paired = placed.len().min(page_boxes.len());
    for (image, rect) in placed.iter().take(paired).zip(page_boxes.iter()) {
        let placed_rect = flip_y(*rect, height);
        let rect = flip_y(*rect, engine_height);
        let record = PyDict::new(py);
        record.set_item("number", image.xref)?;
        record.set_item("bbox", rect.to_vec())?;
        let width = placed_rect[2] - placed_rect[0];
        let tall = placed_rect[3] - placed_rect[1];
        record.set_item(
            "transform",
            [width, 0.0, -0.0, tall, placed_rect[0], placed_rect[1]].to_vec(),
        )?;
        record.set_item("width", image.width)?;
        record.set_item("height", image.height)?;
        record.set_item("colorspace", image.cs_num)?;
        record.set_item("cs-name", &image.cs_name)?;
        record.set_item("xres", 96)?;
        record.set_item("yres", 96)?;
        record.set_item("bpc", image.bpc)?;
        record.set_item("size", image.size as u64)?;
        record.set_item("has-mask", image.has_mask)?;
        images.append(record)?;
    }
    for rect in page_boxes.iter().skip(paired) {
        graphics.append(flip_y(*rect, engine_height).to_vec())?;
    }
    let (lines, areas) = doc
        .engine()?
        .page_rulings_and_fills(page)
        .map_err(engine_err_to_py)?;
    for line in lines {
        rulings.append(flip_y([line.x0, line.y0, line.x1, line.y1], height).to_vec())?;
    }
    for area in areas {
        fills.append(flip_y(area, height).to_vec())?;
    }
    Ok(out.into())
}

/// One page's web links: kind, xref, y-down box, target and identifier.
///
/// Only URI actions surface; internal jumps and remote go-tos stay out.
/// Entries without a rectangle or target are skipped.
#[pyfunction]
fn door_page_links(py: Python<'_>, doc: &PyDocument, page: usize) -> PyResult<PyObject> {
    let engine = doc.engine()?;
    let pdf = engine.pdf();
    let rows = PyList::empty(py);
    let Some(page_handle) = pdf.pages().get(page) else {
        return Ok(rows.into());
    };
    let height = crate::door_geometry::exact_page_size(&engine, page).1;
    for annot in page_handle.annots() {
        let is_link = annot
            .get::<Name>(SUBTYPE)
            .map(|n| name_text(&n) == "Link")
            .unwrap_or(false);
        if !is_link {
            continue;
        }
        let Some(rect) = annot.get::<Array<'_>>(RECT).and_then(array_rect) else {
            continue;
        };
        let Some(action) = annot.get::<Dict<'_>>(A) else {
            continue;
        };
        let is_uri = action
            .get::<Name>(S)
            .map(|n| name_text(&n) == "URI")
            .unwrap_or(false);
        if !is_uri {
            continue;
        }
        let Some(uri) = action
            .get::<PdfString>(URI)
            .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
        else {
            continue;
        };
        let id = annot
            .get::<PdfString>(NM)
            .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
            .unwrap_or_default();
        let record = PyDict::new(py);
        record.set_item("kind", 2)?;
        record.set_item(
            "xref",
            annot.obj_id().map(|id| id.obj_number as i64).unwrap_or(0),
        )?;
        record.set_item("from", flip_y(rect, height).to_vec())?;
        record.set_item("uri", uri)?;
        record.set_item("id", id)?;
        rows.append(record)?;
    }
    Ok(rows.into())
}

/// Table proposals for one page: runs of adjacent table cells.
///
/// Each run carries its cells (grid position, spans, header flag, y-down box
/// and text), the union region and the grid size. Runs restart wherever the
/// grid returns to row 0, column 0.
#[pyfunction]
fn door_page_tables(py: Python<'_>, doc: &PyDocument, page: usize) -> PyResult<PyObject> {
    let mut options = ExtractOptions::default();
    options.first_page = page;
    options.page_count = Some(1);
    let engine = doc.engine()?;
    let blocks = engine.extract_blocks(&options).map_err(engine_err_to_py)?;
    let height = crate::door_geometry::exact_page_size(&engine, page).1;
    let tables = PyList::empty(py);
    let mut cells: Vec<PyObject> = Vec::new();
    let mut region: Option<[f64; 4]> = None;
    let mut rows = 0usize;
    let mut columns = 0usize;
    let flush = |cells: &mut Vec<PyObject>,
                 region: &mut Option<[f64; 4]>,
                 rows: &mut usize,
                 columns: &mut usize|
     -> PyResult<()> {
        if cells.is_empty() {
            return Ok(());
        }
        let table = PyDict::new(py);
        table.set_item("cells", PyList::new(py, cells.drain(..))?)?;
        table.set_item("region", region.unwrap_or([0.0, 0.0, 0.0, 0.0]).to_vec())?;
        table.set_item("rows", *rows as u64)?;
        table.set_item("columns", *columns as u64)?;
        tables.append(table)?;
        *region = None;
        *rows = 0;
        *columns = 0;
        Ok(())
    };
    for block in &blocks {
        let BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } = &block.kind
        else {
            continue;
        };
        let (row, column, row_span, column_span, header) =
            (*row, *column, *row_span, *column_span, *header);
        if row == 0 && column == 0 && !cells.is_empty() {
            flush(&mut cells, &mut region, &mut rows, &mut columns)?;
        }
        let rect = flip_y(block.bbox, height);
        region = Some(match region {
            None => rect,
            Some(prior) => [
                prior[0].min(rect[0]),
                prior[1].min(rect[1]),
                prior[2].max(rect[2]),
                prior[3].max(rect[3]),
            ],
        });
        rows = rows.max((row + row_span) as usize);
        columns = columns.max((column + column_span) as usize);
        let cell = PyDict::new(py);
        cell.set_item("row", row as u64)?;
        cell.set_item("column", column as u64)?;
        cell.set_item("row_span", row_span as u64)?;
        cell.set_item("column_span", column_span as u64)?;
        cell.set_item("header", header)?;
        cell.set_item("bbox", rect.to_vec())?;
        cell.set_item("text", &block.text)?;
        cells.push(cell.into());
    }
    flush(&mut cells, &mut region, &mut rows, &mut columns)?;
    Ok(tables.into())
}
