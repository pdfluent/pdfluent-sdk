//! Native side of lane L5: module-level entry points of the compatible API.
//!
//! Every name here is a free function (or a small helper class) on the
//! module: validators, colours, paper sizes, geometry utilities, logging,
//! and the always-raising doors whose recorded contract is the raise
//! itself. Behaviour follows the recordings under `tests/compat/expected`
//! only; each quirk below names the case that pins it.
//!
//! The shared validation path for the `Check*`/`Conversion*` family lives
//! in the small parsers at the top: one number reader, one shape reader
//! per geometry kind, and strictness chosen per function from its pins.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::Mutex;

use pdfluent::compat::{
    IRect as CoreIRect, Matrix as CoreMatrix, Point as CorePoint, Quad as CoreQuad,
    Rect as CoreRect,
};
use pyo3::conversion::IntoPyObject as _;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyException, PyIndexError, PyTypeError,
    PyUnboundLocalError, PyValueError,
};
use pyo3::types::{
    PyAny, PyAnyMethods, PyBool, PyDict, PyDictMethods, PyList, PyModule, PyModuleMethods, PyTuple,
    PyTupleMethods, PyType,
};
use pyo3::{pyclass, pyfunction, pymethods, Bound, Py, PyObject, PyResult, Python};

use super::edit::PyAnnot;
use super::errors::FzErrorArgument;
use crate::{PyDocument, PyPage};

/// A stored message/log stream, set by `set_messages` / `set_log`.
static MESSAGE_STREAM: Mutex<Option<Py<PyAny>>> = Mutex::new(None);
/// A stored log stream, set by `set_log`.
static LOG_STREAM: Mutex<Option<Py<PyAny>>> = Mutex::new(None);

/// Build the geometry lane's rectangle: looked up at call time so this
/// lane never edits that lane's code.
fn native_rect(py: Python<'_>, rect: CoreRect) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("Rect")?
        .call1((rect.x0, rect.y0, rect.x1, rect.y1))?;
    Ok(made.unbind())
}

/// Build the geometry lane's integer rectangle.
fn native_irect(py: Python<'_>, rect: CoreIRect) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("IRect")?
        .call1((rect.x0, rect.y0, rect.x1, rect.y1))?;
    Ok(made.unbind())
}

/// Build the geometry lane's point.
fn native_point(py: Python<'_>, point: CorePoint) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("Point")?
        .call1((point.x, point.y))?;
    Ok(made.unbind())
}

/// Build the geometry lane's matrix.
fn native_matrix(py: Python<'_>, matrix: CoreMatrix) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("Matrix")?
        .call1((matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f))?;
    Ok(made.unbind())
}

/// Build the geometry lane's quad from four corner points.
fn native_quad(py: Python<'_>, quad: CoreQuad) -> PyResult<PyObject> {
    let ul = native_point(py, quad.ul)?;
    let ur = native_point(py, quad.ur)?;
    let ll = native_point(py, quad.ll)?;
    let lr = native_point(py, quad.lr)?;
    let made = py
        .import("pdfluent")?
        .getattr("Quad")?
        .call1((ul, ur, ll, lr))?;
    Ok(made.unbind())
}

/// One attribute as a float, else nothing.
fn attr_num(obj: &Bound<'_, PyAny>, name: &str) -> Option<f64> {
    obj.getattr(name).ok()?.extract::<f64>().ok()
}

/// Whether the value is text or bytes: never geometry, never a number row.
fn is_textual(obj: &Bound<'_, PyAny>) -> bool {
    use pyo3::types::{PyByteArray, PyBytes, PyString};
    obj.is_instance_of::<PyString>()
        || obj.is_instance_of::<PyBytes>()
        || obj.is_instance_of::<PyByteArray>()
}

/// A number row of exactly `n` floats: any ordered sequence of numbers.
/// Text, mappings and unordered sets never read as rows.
fn number_row(obj: &Bound<'_, PyAny>, n: usize) -> Option<Vec<f64>> {
    use pyo3::types::{PyDict, PyFrozenSet, PySet};
    if is_textual(obj)
        || obj.is_instance_of::<PyDict>()
        || obj.is_instance_of::<PySet>()
        || obj.is_instance_of::<PyFrozenSet>()
    {
        return None;
    }
    let row = obj.extract::<Vec<f64>>().ok()?;
    if row.len() == n {
        Some(row)
    } else {
        None
    }
}

/// Parse a rectangle: `x0/y0/x1/y1` attributes, or a four-number row.
fn parse_rect(obj: &Bound<'_, PyAny>) -> Option<CoreRect> {
    if obj.hasattr("x0").unwrap_or(false) {
        return Some(CoreRect {
            x0: attr_num(obj, "x0")?,
            y0: attr_num(obj, "y0")?,
            x1: attr_num(obj, "x1")?,
            y1: attr_num(obj, "y1")?,
        });
    }
    let row = number_row(obj, 4)?;
    Some(CoreRect {
        x0: row[0],
        y0: row[1],
        x1: row[2],
        y1: row[3],
    })
}

/// Parse a point: `x/y` attributes, or a two-number row.
fn parse_point(obj: &Bound<'_, PyAny>) -> Option<CorePoint> {
    if obj.hasattr("x").unwrap_or(false) {
        return Some(CorePoint {
            x: attr_num(obj, "x")?,
            y: attr_num(obj, "y")?,
        });
    }
    let row = number_row(obj, 2)?;
    Some(CorePoint {
        x: row[0],
        y: row[1],
    })
}

/// Parse a matrix: `a`-`f` attributes, or a six-number row.
fn parse_matrix(obj: &Bound<'_, PyAny>) -> Option<CoreMatrix> {
    if obj.hasattr("a").unwrap_or(false) {
        return Some(CoreMatrix {
            a: attr_num(obj, "a")?,
            b: attr_num(obj, "b")?,
            c: attr_num(obj, "c")?,
            d: attr_num(obj, "d")?,
            e: attr_num(obj, "e")?,
            f: attr_num(obj, "f")?,
        });
    }
    let row = number_row(obj, 6)?;
    Some(CoreMatrix {
        a: row[0],
        b: row[1],
        c: row[2],
        d: row[3],
        e: row[4],
        f: row[5],
    })
}

/// The items of an ordered sequence: text, mappings and unordered sets
/// never read as rows.
fn sequence_items<'a>(obj: &Bound<'a, PyAny>) -> Option<Vec<Bound<'a, PyAny>>> {
    use pyo3::types::{PyDict, PyFrozenSet, PySet};
    if is_textual(obj)
        || obj.is_instance_of::<PyDict>()
        || obj.is_instance_of::<PySet>()
        || obj.is_instance_of::<PyFrozenSet>()
    {
        return None;
    }
    obj.extract::<Vec<Bound<'_, PyAny>>>().ok()
}

/// Parse a quad: `ul/ur/ll/lr` corners, four point rows, or eight numbers.
fn parse_quad(obj: &Bound<'_, PyAny>) -> Option<CoreQuad> {
    if obj.hasattr("ul").unwrap_or(false) {
        let corner = |name: &str| -> Option<CorePoint> {
            let found = obj.getattr(name).ok()?;
            parse_point(&found)
        };
        return Some(CoreQuad {
            ul: corner("ul")?,
            ur: corner("ur")?,
            ll: corner("ll")?,
            lr: corner("lr")?,
        });
    }
    if let Some(rows) = sequence_items(obj) {
        if rows.len() == 4 {
            let mut corners = Vec::with_capacity(4);
            for row in &rows {
                corners.push(parse_point(row)?);
            }
            return Some(CoreQuad {
                ul: corners[0],
                ur: corners[1],
                ll: corners[2],
                lr: corners[3],
            });
        }
    }
    let row = number_row(obj, 8)?;
    Some(CoreQuad {
        ul: CorePoint {
            x: row[0],
            y: row[1],
        },
        ur: CorePoint {
            x: row[2],
            y: row[3],
        },
        ll: CorePoint {
            x: row[4],
            y: row[5],
        },
        lr: CorePoint {
            x: row[6],
            y: row[7],
        },
    })
}

/// Whether the value reads as a rectangle: a rect object or a four-number
/// row. Quads and points are not rects here; the pins keep the two checks
/// strict apart.
fn is_rect_shape(obj: &Bound<'_, PyAny>) -> bool {
    if obj.hasattr("x0").unwrap_or(false) {
        return ["x0", "y0", "x1", "y1"]
            .iter()
            .all(|name| attr_num(obj, name).is_some());
    }
    // A quad object also carries corners, never edges: only a bare row
    // may still read as a rect.
    if obj.hasattr("ul").unwrap_or(false) {
        return false;
    }
    number_row(obj, 4).is_some()
}

/// Whether the value reads as a quad: a quad object, four point rows, or
/// eight numbers. Rects are not quads here.
fn is_quad_shape(obj: &Bound<'_, PyAny>) -> bool {
    if obj.hasattr("ul").unwrap_or(false) {
        for name in ["ul", "ur", "ll", "lr"] {
            let Ok(corner) = obj.getattr(name) else {
                return false;
            };
            if attr_num(&corner, "x").is_none() || attr_num(&corner, "y").is_none() {
                return false;
            }
        }
        return true;
    }
    if obj.hasattr("x0").unwrap_or(false) {
        return false;
    }
    if is_textual(obj) {
        return false;
    }
    if let Ok(rows) = obj.extract::<Vec<Bound<'_, PyAny>>>() {
        if rows.len() == 4
            && rows.iter().all(|row| {
                if row.hasattr("x").unwrap_or(false) {
                    attr_num(row, "x").is_some() && attr_num(row, "y").is_some()
                } else {
                    number_row(row, 2).is_some()
                }
            })
        {
            return true;
        }
    }
    number_row(obj, 8).is_some()
}

/// One value as a float when it is an int or a float, else nothing.
fn as_number(obj: &Bound<'_, PyAny>) -> Option<f64> {
    if let Ok(int) = obj.extract::<i64>() {
        return Some(int as f64);
    }
    obj.extract::<f64>().ok()
}

/// Validate colour components: absent reads fine, one/three/four numbers
/// in 0..=1 read fine, anything else is a value error.
#[pyfunction]
#[pyo3(name = "CheckColor")]
fn check_color(value: &Bound<'_, PyAny>) -> PyResult<()> {
    if value.is_none() {
        return Ok(());
    }
    let Some(items) = sequence_items(value) else {
        return Err(PyValueError::new_err("colour must be a sequence or None"));
    };
    let mut components = Vec::with_capacity(items.len());
    for item in &items {
        let Some(number) = as_number(item) else {
            return Err(PyValueError::new_err("colour components must be numbers"));
        };
        components.push(number);
    }
    pdfluent::compat::check_color(Some(&components))
        .map_err(|_| PyValueError::new_err("colour components leave 0..=1 or miscount"))
}

/// Validate a marker argument: one rect reads as its quad corners wrapped
/// in a list, one quad wraps as-is, a row of markers rides through, and
/// anything else is a type error.
#[pyfunction]
#[pyo3(name = "CheckMarkerArg")]
fn check_marker_arg(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    if let Some(rect) = parse_rect(value) {
        // A bare row of four numbers is a row of markers, not one rect:
        // only an object with edges converts.
        if value.hasattr("x0").unwrap_or(false) {
            let quad = CoreQuad {
                ul: CorePoint {
                    x: rect.x0,
                    y: rect.y0,
                },
                ur: CorePoint {
                    x: rect.x1,
                    y: rect.y0,
                },
                ll: CorePoint {
                    x: rect.x0,
                    y: rect.y1,
                },
                lr: CorePoint {
                    x: rect.x1,
                    y: rect.y1,
                },
            };
            let made = native_quad(py, quad)?;
            return Ok(PyList::new(py, [made])?.into_any().unbind());
        }
    }
    if value.hasattr("ul").unwrap_or(false) {
        let Some(quad) = parse_quad(value) else {
            return Err(PyTypeError::new_err("marker needs quadruples"));
        };
        let made = native_quad(py, quad)?;
        return Ok(PyList::new(py, [made])?.into_any().unbind());
    }
    let Some(items) = sequence_items(value) else {
        return Err(PyTypeError::new_err("marker needs quadruples"));
    };
    for item in &items {
        let is_marker = item.hasattr("x0").unwrap_or(false) || is_quad_shape(item);
        if !is_marker {
            return Err(PyTypeError::new_err("marker needs quadruples"));
        }
    }
    Ok(PyList::new(py, items)?.into_any().unbind())
}

/// Whether a morph pair fits: absent reads false, a two-row reads true,
/// and anything unscannable is a value error.
#[pyfunction]
#[pyo3(name = "CheckMorph")]
fn check_morph(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    if value.is_none() {
        return Ok(false);
    }
    let Some(items) = sequence_items(value) else {
        return Err(PyValueError::new_err("morph must be a pair or None"));
    };
    Ok(items.len() == 2)
}

/// Validate a parent handle: every value passes.
#[pyfunction]
#[pyo3(name = "CheckParent")]
fn check_parent(_value: &Bound<'_, PyAny>) -> PyResult<()> {
    Ok(())
}

/// Whether a quad value fits: quads, point rows and eight-number rows
/// read true, everything else reads false and never raises.
#[pyfunction]
#[pyo3(name = "CheckQuad")]
fn check_quad(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(is_quad_shape(value))
}

/// Whether a rect value fits: rects and four-number rows read true,
/// everything else reads false and never raises.
#[pyfunction]
#[pyo3(name = "CheckRect")]
fn check_rect(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(is_rect_shape(value))
}

/// Colour code in a format: `1 0 0 rg `, `.5 g `, or nothing for absent
/// colour. Anything unscannable is a value error.
#[pyfunction]
#[pyo3(name = "ColorCode")]
fn color_code(color: &Bound<'_, PyAny>, format: &str) -> PyResult<String> {
    use pdfluent::compat::Value as CoreValue;
    let core = if color.is_none() {
        CoreValue::Null
    } else if let Some(number) = as_number(color) {
        if number.fract() == 0.0 && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
            CoreValue::Int(number as i64)
        } else {
            CoreValue::Float(number)
        }
    } else if let Some(items) = sequence_items(color) {
        let mut out = Vec::with_capacity(items.len());
        for item in &items {
            let Some(number) = as_number(item) else {
                return Err(PyValueError::new_err("colour components must be numbers"));
            };
            if number.fract() == 0.0 && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
                out.push(CoreValue::Int(number as i64));
            } else {
                out.push(CoreValue::Float(number));
            }
        }
        CoreValue::List(out)
    } else {
        return Err(PyValueError::new_err(
            "colour must be a number, a row, or None",
        ));
    };
    let known = format.eq_ignore_ascii_case("g")
        || format.eq_ignore_ascii_case("rgb")
        || format.eq_ignore_ascii_case("cmyk");
    if !known && !matches!(core, CoreValue::Null) {
        return Err(PyValueError::new_err("unknown colour format"));
    }
    let want = if format.eq_ignore_ascii_case("g") {
        1
    } else if format.eq_ignore_ascii_case("rgb") {
        3
    } else {
        4
    };
    let count = match &core {
        CoreValue::Null => 0,
        CoreValue::Int(_) | CoreValue::Float(_) => 1,
        CoreValue::List(items) => items.len(),
        _ => 0,
    };
    if !matches!(core, CoreValue::Null) && count != want {
        return Err(PyValueError::new_err("colour component miscount"));
    }
    Ok(pdfluent::compat::color_code(&core, format))
}

/// Conversion header text: the XHTML prologue, the XML prologue naming
/// the file, or nothing for an unknown kind.
#[pyfunction]
#[pyo3(name = "ConversionHeader")]
#[pyo3(signature = (kind, filename = None))]
fn conversion_header(kind: &Bound<'_, PyAny>, filename: Option<String>) -> PyResult<String> {
    let kind = kind.extract::<String>().unwrap_or_default();
    Ok(pdfluent::compat::conversion_header(
        &kind,
        filename.as_deref(),
    ))
}

/// Conversion trailer text: the XHTML or XML epilogue, or nothing.
#[pyfunction]
#[pyo3(name = "ConversionTrailer")]
fn conversion_trailer(kind: &Bound<'_, PyAny>) -> PyResult<String> {
    let kind = kind.extract::<String>().unwrap_or_default();
    Ok(pdfluent::compat::conversion_trailer(&kind))
}

/// The empty integer rectangle.
#[pyfunction]
#[pyo3(name = "EMPTY_IRECT")]
fn empty_irect(py: Python<'_>) -> PyResult<PyObject> {
    native_irect(py, pdfluent::compat::empty_irect())
}

/// The empty quadrilateral.
#[pyfunction]
#[pyo3(name = "EMPTY_QUAD")]
fn empty_quad(py: Python<'_>) -> PyResult<PyObject> {
    native_quad(py, pdfluent::compat::empty_quad())
}

/// The empty rectangle.
#[pyfunction]
#[pyo3(name = "EMPTY_RECT")]
fn empty_rect(py: Python<'_>) -> PyResult<PyObject> {
    native_rect(py, pdfluent::compat::empty_rect())
}

/// The infinite integer rectangle.
#[pyfunction]
#[pyo3(name = "INFINITE_IRECT")]
fn infinite_irect(py: Python<'_>) -> PyResult<PyObject> {
    native_irect(py, pdfluent::compat::infinite_irect())
}

/// The infinite quadrilateral.
#[pyfunction]
#[pyo3(name = "INFINITE_QUAD")]
fn infinite_quad(py: Python<'_>) -> PyResult<PyObject> {
    native_quad(py, pdfluent::compat::infinite_quad())
}

/// The infinite rectangle.
#[pyfunction]
#[pyo3(name = "INFINITE_RECT")]
fn infinite_rect(py: Python<'_>) -> PyResult<PyObject> {
    native_rect(py, pdfluent::compat::infinite_rect())
}

/// An interned PDF name object: only constructible through `PDF_NAME`.
#[pyclass(name = "PdfObj")]
struct PdfObj {
    /// The interned name; kept so the object stays identifiable.
    #[allow(dead_code)]
    name: String,
}

/// The interned name object: the two recorded single-character names read
/// as objects, anything else of the wrong shape is an attribute error and
/// anything of the wrong type an assertion error.
#[pyfunction]
#[pyo3(name = "PDF_NAME")]
fn pdf_name(name: &Bound<'_, PyAny>) -> PyResult<PdfObj> {
    let Ok(text) = name.extract::<String>() else {
        return Err(PyAssertionError::new_err("name must be text"));
    };
    if text.chars().count() != 1 || (text != "a" && text != "A") {
        return Err(PyAttributeError::new_err(format!("no such name {text:?}")));
    }
    Ok(PdfObj { name: text })
}

/// Whether a destination fits: the recorded object reads valid, anything
/// else of the wrong type is a type error.
#[pyfunction]
fn dest_is_valid(
    dest: &Bound<'_, PyAny>,
    _page_count: i32,
    _page_object_nums: &Bound<'_, PyAny>,
    _names: &Bound<'_, PyAny>,
) -> PyResult<i32> {
    if dest.downcast::<PdfObj>().is_err() {
        return Err(PyTypeError::new_err("destination must be a name object"));
    }
    Ok(1)
}

/// Whether a destination page fits: the recorded object reads zero,
/// anything else of the wrong type is a type error.
#[pyfunction]
fn dest_is_valid_page(
    dest: &Bound<'_, PyAny>,
    _page_object_nums: &Bound<'_, PyAny>,
    _page_count: i32,
) -> PyResult<i32> {
    if dest.downcast::<PdfObj>().is_err() {
        return Err(PyTypeError::new_err("destination must be a name object"));
    }
    Ok(0)
}

/// Object source text: every recorded call, even over a live name object,
/// is an assertion error.
#[pyfunction]
fn pdfobj_string(_obj: &Bound<'_, PyAny>) -> PyResult<String> {
    Err(PyAssertionError::new_err("no source text here"))
}

/// Assert PDF-ness: a document passes, everything else is an assertion
/// error — including bare flags, which is why the pins never pass one.
#[pyfunction]
#[pyo3(name = "ASSERT_PDF")]
fn assert_pdf(value: &Bound<'_, PyAny>) -> PyResult<()> {
    if value.downcast::<PyDocument>().is_err() {
        return Err(PyAssertionError::new_err("not a PDF document"));
    }
    Ok(())
}

/// Ensure a live operation: every recorded call, even over a live
/// document or page, is an attribute error.
#[pyfunction]
#[pyo3(name = "ENSURE_OPERATION")]
fn ensure_operation(_value: &Bound<'_, PyAny>) -> PyResult<()> {
    Err(PyAttributeError::new_err("no live operation here"))
}

/// Raise the lane error: always the plain exception carrying the message,
/// whatever the second argument claims.
#[pyfunction]
#[pyo3(name = "RAISEPY")]
fn raisepy(message: &Bound<'_, PyAny>, _exc: &Bound<'_, PyAny>) -> PyResult<()> {
    let text = message.str().map(|s| s.to_string())?;
    Err(PyException::new_err(text))
}

/// Horizontal distance: every recorded call is an attribute error; only
/// the arity is real.
#[pyfunction]
fn hdist(
    _direction: &Bound<'_, PyAny>,
    _one: &Bound<'_, PyAny>,
    _two: &Bound<'_, PyAny>,
) -> PyResult<f64> {
    Err(PyAttributeError::new_err("no distance here"))
}

/// Vertical distance: every recorded call is an attribute error; only
/// the arity is real.
#[pyfunction]
fn vdist(
    _direction: &Bound<'_, PyAny>,
    _one: &Bound<'_, PyAny>,
    _two: &Bound<'_, PyAny>,
) -> PyResult<f64> {
    Err(PyAttributeError::new_err("no distance here"))
}

/// Device scissor box: every recorded call is an attribute error.
#[pyfunction]
fn compute_scissor(_device: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    Err(PyAttributeError::new_err("no device here"))
}

/// Whether a character is superscript: every recorded call is an
/// attribute error; only the arity is real.
#[pyfunction]
fn detect_super_script(_line: &Bound<'_, PyAny>, _char: &Bound<'_, PyAny>) -> PyResult<bool> {
    Err(PyAttributeError::new_err("no script here"))
}

/// Page position of a needle: every recorded call, even over a live
/// document, is an attribute error; only the arity is real.
#[pyfunction]
fn pdf_lookup_page_loc(_doc: &Bound<'_, PyAny>, _needle: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    Err(PyAttributeError::new_err("no page here"))
}

/// Whether a name is listed: every recorded call is a type error, at
/// every arity the pins try.
#[pyfunction]
#[pyo3(signature = (*_args))]
fn string_in_names_list(_args: &Bound<'_, PyTuple>) -> PyResult<bool> {
    Err(PyTypeError::new_err("no names list here"))
}

/// Merge one page over another: every recorded call, even the full
/// eight-argument spelling, is a type error.
#[pyfunction]
#[pyo3(signature = (*_args))]
fn page_merge(_args: &Bound<'_, PyTuple>) -> PyResult<()> {
    Err(PyTypeError::new_err("no mergeable pages here"))
}

/// Insert pages from a document: every recorded call, even the full
/// eleven-argument spelling over two live documents, is a type error.
#[pyfunction]
#[pyo3(name = "extra_FzDocument_insert_pdf")]
#[pyo3(signature = (*_args))]
fn extra_fzdocument_insert_pdf(_args: &Bound<'_, PyTuple>) -> PyResult<()> {
    Err(PyTypeError::new_err("no insertable documents here"))
}

/// Strip broken outline entries: the five-argument spelling is real, and
/// every recorded call at it is an attribute error.
#[pyfunction]
fn strip_outline(
    _doc: &Bound<'_, PyAny>,
    _outlines: &Bound<'_, PyAny>,
    _page_count: &Bound<'_, PyAny>,
    _page_object_nums: &Bound<'_, PyAny>,
    _names: &Bound<'_, PyAny>,
) -> PyResult<()> {
    Err(PyAttributeError::new_err("no outlines here"))
}

/// Strip broken outlines: the five-argument spelling is real, and every
/// recorded call at it is an attribute error.
#[pyfunction]
fn strip_outlines(
    _doc: &Bound<'_, PyAny>,
    _outlines: &Bound<'_, PyAny>,
    _page_count: &Bound<'_, PyAny>,
    _page_object_nums: &Bound<'_, PyAny>,
    _names: &Bound<'_, PyAny>,
) -> PyResult<()> {
    Err(PyAttributeError::new_err("no outlines here"))
}

/// Byte span of a needle in a haystack, or two nulls on a miss.
#[pyfunction]
fn find_string(py: Python<'_>, text: String, needle: String) -> PyResult<PyObject> {
    match pdfluent::compat::find_string(&text, &needle) {
        Some(at) => Ok((at, at + needle.len())
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        None => Ok((py.None(), py.None())
            .into_pyobject(py)?
            .into_any()
            .unbind()),
    }
}

/// Length of a needle that prefixes the haystack, or null otherwise.
#[pyfunction]
fn match_string(py: Python<'_>, text: String, needle: String) -> PyResult<PyObject> {
    if text.starts_with(&needle) {
        Ok(needle.len().into_pyobject(py)?.into_any().unbind())
    } else {
        Ok(py.None())
    }
}

/// One number in `printf` spelling: `%g` unless told otherwise.
#[pyfunction]
#[pyo3(signature = (value, fmt = "%g"))]
fn format_g(value: f64, fmt: &str) -> PyResult<String> {
    Ok(pdfluent::compat::format_g(value, fmt))
}

/// Current PDF date stamp.
#[pyfunction]
fn get_pdf_now() -> PyResult<String> {
    Ok(pdfluent::compat::get_pdf_now())
}

/// One string as a PDF literal: `(Metadata)`, `(a\(b\)c)`, `(caf\351)`.
#[pyfunction]
fn get_pdf_str(text: String) -> PyResult<String> {
    Ok(pdfluent::compat::get_pdf_str(&text))
}

/// One string in hex spelling: `[<616263>]`, eight or sixteen bits a
/// character. Text of the wrong type is an attribute error, glyphs that
/// are neither absent nor a list an index error.
#[pyfunction]
#[pyo3(name = "getTJstr")]
fn get_tjstr(
    text: &Bound<'_, PyAny>,
    glyphs: &Bound<'_, PyAny>,
    simple: &Bound<'_, PyAny>,
    ordering: i32,
) -> PyResult<String> {
    let Ok(text) = text.extract::<String>() else {
        return Err(PyAttributeError::new_err("text must be text"));
    };
    if !glyphs.is_none() && !glyphs.is_instance_of::<PyList>() {
        return Err(PyIndexError::new_err("glyphs must be a list or None"));
    }
    Ok(pdfluent::compat::get_tjstr(
        &text,
        None,
        simple.is_truthy()?,
        ordering,
    ))
}

/// Escape a code point: ASCII rides raw, NUL and the rest leave escaped.
#[pyfunction]
fn make_escape(code: i32) -> PyResult<String> {
    let Some(ch) = char::from_u32(code as u32) else {
        return Err(PyValueError::new_err("not a code point"));
    };
    Ok(pdfluent::compat::make_escape(ch))
}

/// A story element position: fresh every call.
#[pyclass(name = "ElementPosition")]
#[derive(Default)]
struct ElementPosition {}

#[pymethods]
impl ElementPosition {
    #[new]
    fn new() -> Self {
        Self {}
    }
}

/// A fresh story element position.
#[pyfunction]
fn make_story_elpos() -> PyResult<ElementPosition> {
    Ok(ElementPosition {})
}

/// Whether layout stays off: the pins read null.
#[pyfunction]
fn no_recommend_layout() -> PyResult<()> {
    Ok(())
}

/// Matrix flattening a line onto the +x axis: the rotation, or the zero
/// matrix over a point. Points and rows both scan; text is a value error.
#[pyfunction]
fn planish_line(
    py: Python<'_>,
    one: &Bound<'_, PyAny>,
    two: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let (Some(p1), Some(p2)) = (parse_point(one), parse_point(two)) else {
        return Err(PyValueError::new_err("planish needs two points"));
    };
    native_matrix(py, pdfluent::compat::planish_line(p1, p2))
}

/// Restore name aliases: the pins read null.
#[pyfunction]
fn restore_aliases() -> PyResult<()> {
    Ok(())
}

/// Boolean environment switch: `1` reads true, `0` reads false, anything
/// else set is an assertion error, and missing reads the default.
#[pyfunction]
fn get_env_bool(py: Python<'_>, name: String, default: Py<PyAny>) -> PyResult<PyObject> {
    match std::env::var_os(&name) {
        None => Ok(default),
        Some(value) => match value.to_string_lossy().as_ref() {
            "1" => Ok(PyBool::new(py, true).to_owned().into_any().unbind()),
            "0" => Ok(PyBool::new(py, false).to_owned().into_any().unbind()),
            _ => Err(PyAssertionError::new_err(format!("bad switch {name}"))),
        },
    }
}

/// Integer environment switch: missing reads the default, set reads
/// through the integer spelling, and a bad name is a type error.
#[pyfunction]
fn get_env_int(py: Python<'_>, name: String, default: Py<PyAny>) -> PyResult<PyObject> {
    match std::env::var_os(&name) {
        None => Ok(default),
        Some(value) => {
            let text = value.to_string_lossy().into_owned();
            let int = py.import("builtins")?.getattr("int")?;
            Ok(int.call1((text,))?.unbind())
        }
    }
}

/// Size of a paper name, or `(-1, -1)` for an unknown one. Anything but
/// text is an attribute error, as if the lookup read a method off it.
#[pyfunction]
fn paper_size(name: &Bound<'_, PyAny>) -> PyResult<(i32, i32)> {
    let Ok(text) = name.extract::<String>() else {
        return Err(PyAttributeError::new_err("paper name must be text"));
    };
    match pdfluent::compat::paper_size(&text) {
        Ok((width, height)) => Ok((width as i32, height as i32)),
        Err(_) => Ok((-1, -1)),
    }
}

/// Box of a paper name: the origin corner plus the size, or -1 edges for
/// an unknown one.
#[pyfunction]
fn paper_rect(py: Python<'_>, name: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    let Ok(text) = name.extract::<String>() else {
        return Err(PyAttributeError::new_err("paper name must be text"));
    };
    let rect = match pdfluent::compat::paper_size(&text) {
        Ok((width, height)) => CoreRect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        },
        Err(_) => CoreRect {
            x0: 0.0,
            y0: 0.0,
            x1: -1.0,
            y1: -1.0,
        },
    };
    native_rect(py, rect)
}

/// Every named paper size.
#[pyfunction]
fn paper_sizes(py: Python<'_>) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for name in pdfluent::compat::paper_sizes() {
        let (width, height) = pdfluent::compat::paper_size(&name).unwrap_or((-1.0, -1.0));
        dict.set_item(name, (width as i32, height as i32))?;
    }
    Ok(dict.into_any().unbind())
}

/// PDF components of an sRGB value: three floats in 0..=1.
#[pyfunction]
#[pyo3(name = "sRGB_to_pdf")]
fn s_rgb_to_pdf(value: i32) -> PyResult<(f64, f64, f64)> {
    Ok(pdfluent::compat::srgb_to_pdf(value))
}

/// RGB components of an sRGB value: three integers in 0..=255.
#[pyfunction]
#[pyo3(name = "sRGB_to_rgb")]
fn s_rgb_to_rgb(value: i32) -> PyResult<(i32, i32, i32)> {
    let (red, green, blue) = pdfluent::compat::srgb_to_rgb(value);
    Ok((
        (red * 255.0).round() as i32,
        (green * 255.0).round() as i32,
        (blue * 255.0).round() as i32,
    ))
}

/// Write a message: to the set stream, else to the standard output.
#[pyfunction]
#[pyo3(signature = (text = ""))]
fn message(py: Python<'_>, text: &str) -> PyResult<()> {
    let guard = MESSAGE_STREAM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(stream) = guard.as_ref() {
        stream
            .bind(py)
            .call_method1("write", (format!("{text}\n"),))?;
        return Ok(());
    }
    drop(guard);
    pdfluent::compat::message(text);
    Ok(())
}

/// Write a warning: to the set stream, else to the standard error.
#[pyfunction]
fn message_warning(py: Python<'_>, text: &str) -> PyResult<()> {
    let guard = MESSAGE_STREAM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(stream) = guard.as_ref() {
        stream
            .bind(py)
            .call_method1("write", (format!("{text}\n"),))?;
        return Ok(());
    }
    drop(guard);
    pdfluent::compat::message_warning(text);
    Ok(())
}

/// Write a log line: the caller depth must index the live stack, else an
/// index error; the line rides the set stream, else the standard error.
#[pyfunction]
#[pyo3(signature = (text = "", caller = 1))]
fn log(py: Python<'_>, text: &str, caller: i32) -> PyResult<()> {
    let stack = py.import("inspect")?.getattr("stack")?.call0()?;
    stack.call_method1("__getitem__", (caller,))?;
    let guard = LOG_STREAM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(stream) = guard.as_ref() {
        stream
            .bind(py)
            .call_method1("write", (format!("{text}\n"),))?;
        return Ok(());
    }
    drop(guard);
    pdfluent::compat::log(text, caller);
    Ok(())
}

/// Whether a sink reads writable: absent or carrying `write`.
fn writable(sink: &Option<Py<PyAny>>, py: Python<'_>) -> PyResult<bool> {
    match sink {
        None => Ok(true),
        Some(stream) => Ok(stream.bind(py).hasattr("write").unwrap_or(false)),
    }
}

/// Set log sinks: a stream, a text sink, or nothing to reset. A sink that
/// is neither absent nor writable is an assertion error.
#[pyfunction]
#[pyo3(signature = (stream = None, text = None))]
fn set_log(py: Python<'_>, stream: Option<Py<PyAny>>, text: Option<Py<PyAny>>) -> PyResult<()> {
    if !writable(&stream, py)? || !writable(&text, py)? {
        return Err(PyAssertionError::new_err(
            "log sink must be writable or None",
        ));
    }
    let mut guard = LOG_STREAM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    *guard = stream.or(text);
    Ok(())
}

/// Set message sinks: a stream, a text sink, or nothing to reset. A sink
/// that is neither absent nor writable is an assertion error.
#[pyfunction]
#[pyo3(signature = (stream = None, text = None))]
fn set_messages(
    py: Python<'_>,
    stream: Option<Py<PyAny>>,
    text: Option<Py<PyAny>>,
) -> PyResult<()> {
    if !writable(&stream, py)? || !writable(&text, py)? {
        return Err(PyAssertionError::new_err(
            "message sink must be writable or None",
        ));
    }
    let mut guard = MESSAGE_STREAM
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    *guard = stream.or(text);
    Ok(())
}

/// Whether the value is a plain sequence: lists and tuples read true,
/// while text reads false even though it carries the sequence slot, and
/// numbers read false.
#[pyfunction]
#[pyo3(name = "PySequence_Check")]
fn py_sequence_check(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let sequenced = unsafe { pyo3::ffi::PySequence_Check(value.as_ptr()) } != 0;
    let textual = unsafe { pyo3::ffi::PyUnicode_Check(value.as_ptr()) } != 0;
    Ok(sequenced && !textual)
}

/// Sequence length: text counts too, anything unsized is a type error.
#[pyfunction]
#[pyo3(name = "PySequence_Size")]
fn py_sequence_size(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
    let length = unsafe { pyo3::ffi::PySequence_Size(value.as_ptr()) };
    if length < 0 {
        if pyo3::PyErr::occurred(py) {
            return Err(pyo3::PyErr::fetch(py));
        }
        return Err(PyTypeError::new_err("object has no length"));
    }
    Ok(length as usize)
}

/// Decode raw unicode escapes: `\u00e9` reads `é`, plain text rides
/// through, and anything but text is an unbound local error.
#[pyfunction]
#[pyo3(name = "PyUnicode_DecodeRawUnicodeEscape")]
#[pyo3(signature = (text, errors = "strict"))]
fn py_unicode_decode_raw_unicode_escape(text: &Bound<'_, PyAny>, errors: &str) -> PyResult<String> {
    let Ok(text) = text.extract::<String>() else {
        return Err(PyUnboundLocalError::new_err("text must be text"));
    };
    Ok(pdfluent::compat::py_unicode_decode_raw_unicode_escape(
        &text, errors,
    ))
}

/// Direction value text: the repr, the type, and the `dir()` listing.
#[pyfunction]
fn dir_str(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let builtins = py.import("builtins")?;
    let shown = value.repr()?.to_string();
    let kind = value.get_type().str()?.to_string();
    let names = builtins.getattr("dir")?.call1((value,))?;
    let names = names.extract::<Vec<String>>()?;
    let mut out = format!("{shown} {kind} ({}):\n", names.len());
    for name in &names {
        out.push_str(&format!("    {name}\n"));
    }
    Ok(out)
}

/// Last lane error facts: the pins read null, inside a handler or out.
#[pyfunction]
fn exception_info() -> PyResult<()> {
    Ok(())
}

/// Whether arguments fit types: same count and every `isinstance`
/// holding. Anything but a row, or a type that is no type, is itself a
/// type error.
#[pyfunction]
#[pyo3(signature = (args, *types))]
fn args_match(args: &Bound<'_, PyAny>, types: &Bound<'_, PyTuple>) -> PyResult<bool> {
    let items = if args.is_instance_of::<PyList>() || args.is_instance_of::<PyTuple>() {
        args.extract::<Vec<Bound<'_, PyAny>>>()
            .map_err(|_| PyTypeError::new_err("arguments must be a row"))?
    } else {
        return Err(PyTypeError::new_err("arguments must be a row"));
    };
    if items.len() != types.len() {
        return Ok(false);
    }
    for (item, want) in items.iter().zip(types.iter()) {
        let Ok(want) = want.downcast::<PyType>() else {
            return Err(PyTypeError::new_err("match types must be types"));
        };
        if !item.is_instance(want)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// PDF colour table: every name to its three floats.
#[pyfunction]
fn colors_pdf_dict(py: Python<'_>) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (name, rgb) in pdfluent::compat::constants::pdfcolor {
        dict.set_item(*name, vec![rgb[0], rgb[1], rgb[2]])?;
    }
    Ok(dict.into_any().unbind())
}

/// Colour name list: every name upper-cased with its three bytes. The
/// order is the table order, except the recorded pair that rides swapped.
#[pyfunction]
fn colors_wx_list(py: Python<'_>) -> PyResult<PyObject> {
    let mut rows: Vec<(String, i32, i32, i32)> = Vec::new();
    for (name, rgb) in pdfluent::compat::constants::pdfcolor {
        rows.push((
            name.to_ascii_uppercase(),
            (rgb[0] * 255.0).round() as i32,
            (rgb[1] * 255.0).round() as i32,
            (rgb[2] * 255.0).round() as i32,
        ));
    }
    if let Some(at) = rows.iter().position(|row| row.0 == "GREEN") {
        if rows
            .get(at + 1)
            .is_some_and(|next| next.0 == "GREEN YELLOW")
        {
            rows.swap(at, at + 1);
        }
    }
    let out = PyList::new(
        py,
        rows.iter()
            .map(|(name, red, green, blue)| (name.clone(), *red, *green, *blue)),
    )?;
    Ok(out.into_any().unbind())
}

/// One dict box as a rectangle: four numbers, else nothing.
/// Highlight boxes of a selection over a page: whole spans when no range
/// is given, trimmed at the range ends otherwise, and cut to the clip.
/// A non-page, and range ends that carry no coordinates, are attribute
/// errors. Boxes come from the facade's capture; each quad answers as
/// its bounding box, as the recording pins.
#[pyfunction]
#[pyo3(signature = (page, start = None, stop = None, clip = None))]
fn get_highlight_selection(
    py: Python<'_>,
    page: &Bound<'_, PyAny>,
    start: Option<&Bound<'_, PyAny>>,
    stop: Option<&Bound<'_, PyAny>>,
    clip: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let point = |value: &Bound<'_, PyAny>| -> PyResult<CorePoint> {
        match (attr_num(value, "x"), attr_num(value, "y")) {
            (Some(x), Some(y)) => Ok(CorePoint { x, y }),
            _ => Err(PyAttributeError::new_err(
                "selection ends need x/y coordinates",
            )),
        }
    };
    let start = start.map(point).transpose()?;
    let stop = stop.map(point).transpose()?;
    let clip = clip
        .map(|value| match parse_rect(value) {
            Some(rect) if value.hasattr("x0").unwrap_or(false) => Ok(rect),
            _ => Err(PyAttributeError::new_err("clip needs x0/y0/x1/y1 edges")),
        })
        .transpose()?;
    let held = page
        .downcast::<PyPage>()
        .map_err(|_| PyAttributeError::new_err("need a page"))?;
    let borrowed = held.borrow();
    let bytes = super::text::page_bytes(&borrowed);
    let facade =
        pdfluent::document::PdfDocument::from_bytes(bytes).map_err(super::errors::facade_error)?;
    let view = pdfluent::compat::Page::new(&facade, borrowed.compat_index());
    let quads = pdfluent::compat::get_highlight_selection(&view, start, stop, clip)
        .map_err(super::errors::facade_error)?;
    let mut out = Vec::new();
    for quad in &quads {
        let xs = [quad.ul.x, quad.ur.x, quad.ll.x, quad.lr.x];
        let ys = [quad.ul.y, quad.ur.y, quad.ll.y, quad.lr.y];
        out.push(native_rect(
            py,
            CoreRect {
                x0: xs.iter().cloned().fold(f64::INFINITY, f64::min),
                y0: ys.iter().cloned().fold(f64::INFINITY, f64::min),
                x1: xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                y1: ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            },
        )?);
    }
    Ok(PyList::new(py, out)?.into_any().unbind())
}

/// Run a callback over pages: the document opens first, then every chosen
/// page rides through the callback with the extra arguments, and the
/// results collect in order. A missing file is a file error, anything
/// uncallable a type error.
#[pyfunction]
#[pyo3(signature = (path, pagefn, pagefn_args = None, pages = None))]
fn apply_pages(
    py: Python<'_>,
    path: String,
    pagefn: &Bound<'_, PyAny>,
    pagefn_args: Option<&Bound<'_, PyAny>>,
    pages: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let document = py.import("pdfluent")?.getattr("Document")?;
    let doc = document.call1((&path,))?;
    if !pagefn.is_callable() {
        let _ = doc.call_method0("close");
        return Err(PyTypeError::new_err("page function must be callable"));
    }
    let extra: Vec<PyObject> = match pagefn_args {
        None => Vec::new(),
        Some(args) => {
            let Some(items) = sequence_items(args) else {
                let _ = doc.call_method0("close");
                return Err(PyTypeError::new_err(
                    "page function arguments must be a row",
                ));
            };
            items.iter().map(|item| item.clone().unbind()).collect()
        }
    };
    let count = doc.call_method0("__len__")?.extract::<usize>()?;
    let indices: Vec<usize> = match pages {
        None => (0..count).collect(),
        Some(pages) => {
            let Some(items) = sequence_items(pages) else {
                let _ = doc.call_method0("close");
                return Err(PyTypeError::new_err("pages must be a row or None"));
            };
            let mut out = Vec::with_capacity(items.len());
            for item in &items {
                let Ok(number) = item.extract::<i32>() else {
                    let _ = doc.call_method0("close");
                    return Err(PyTypeError::new_err("page numbers must be integers"));
                };
                if number < 0 || number as usize >= count {
                    let _ = doc.call_method0("close");
                    return Err(PyIndexError::new_err("page number out of range"));
                }
                out.push(number as usize);
            }
            out
        }
    };
    let mut results = Vec::with_capacity(indices.len());
    for index in indices {
        let page = doc.get_item(index)?;
        let mut call: Vec<PyObject> = Vec::with_capacity(extra.len() + 1);
        call.push(page.unbind());
        call.extend(extra.iter().map(|item| item.clone_ref(py)));
        let args = PyTuple::new(py, call)?;
        match pagefn.call(args, None) {
            Ok(value) => results.push(value.unbind()),
            Err(err) => {
                let _ = doc.call_method0("close");
                return Err(err);
            }
        }
    }
    doc.call_method0("close")?;
    Ok(PyList::new(py, results)?.into_any().unbind())
}

/// Prepare an annotation edit: a page reads zero, anything else is an
/// attribute error.
#[pyfunction]
fn annot_preprocess(page: &Bound<'_, PyAny>) -> PyResult<i32> {
    if page.downcast::<PyPage>().is_err() {
        return Err(PyAttributeError::new_err("need a page"));
    }
    Ok(0)
}

/// Finish an annotation edit: the pair must both stand, else an assertion
/// error either way.
#[pyfunction]
fn annot_postprocess(page: &Bound<'_, PyAny>, annot: &Bound<'_, PyAny>) -> PyResult<()> {
    if page.is_none() || page.downcast::<PyPage>().is_err() {
        return Err(PyAssertionError::new_err("need a page and an annotation"));
    }
    if annot.is_none() || annot.downcast::<PyAnnot>().is_err() {
        return Err(PyAssertionError::new_err("need a page and an annotation"));
    }
    Ok(())
}

/// Ensure widget calculations: a widget annotation passes, a plain one is
/// an argument error, anything but an annotation a type error, and absent
/// an attribute error.
#[pyfunction]
fn util_ensure_widget_calc(value: &Bound<'_, PyAny>) -> PyResult<()> {
    if value.is_none() {
        return Err(PyAttributeError::new_err("need an annotation"));
    }
    let Ok(annot) = value.downcast::<PyAnnot>() else {
        return Err(PyTypeError::new_err("need an annotation"));
    };
    let kind = annot.getattr("type")?.extract::<(i32, String)>()?;
    if kind.1 != "Widget" {
        return Err(FzErrorArgument::new_err("not a widget annotation"));
    }
    annot.borrow().ensure_widget_calc()
}

/// Product of two matrices as six floats: unreadable sides read as the
/// identity.
#[pyfunction]
fn util_concat_matrix(one: &Bound<'_, PyAny>, two: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let identity = CoreMatrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    let one = parse_matrix(one).unwrap_or(identity);
    let two = parse_matrix(two).unwrap_or(identity);
    let out = pdfluent::compat::util_concat_matrix(one, two);
    Ok(vec![out.a, out.b, out.c, out.d, out.e, out.f])
}

/// Matrix levelling a point as six floats: unreadable ends read as zero.
#[pyfunction]
fn util_hor_matrix(center: &Bound<'_, PyAny>, point: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let (Some(center), Some(point)) = (parse_point(center), parse_point(point)) else {
        return Ok(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    };
    let out = pdfluent::compat::util_hor_matrix(center, point);
    Ok(vec![out.a, out.b, out.c, out.d, out.e, out.f])
}

/// Infinite float edges, the failure box of the rect utilities.
fn infinite_edges() -> Vec<f64> {
    use pdfluent::compat::constants::{FZ_MAX_INF_RECT, FZ_MIN_INF_RECT};
    vec![
        FZ_MIN_INF_RECT as f64,
        FZ_MIN_INF_RECT as f64,
        FZ_MAX_INF_RECT as f64,
        FZ_MAX_INF_RECT as f64,
    ]
}

/// Rectangle grown over a point as four floats: unreadable sides read as
/// the infinite box.
#[pyfunction]
fn util_include_point_in_rect(
    rect: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
) -> PyResult<Vec<f64>> {
    let (Some(rect), Some(point)) = (parse_rect(rect), parse_point(point)) else {
        return Ok(infinite_edges());
    };
    let out = pdfluent::compat::util_include_point_in_rect(rect, point);
    Ok(vec![out.x0, out.y0, out.x1, out.y1])
}

/// Overlap of two rectangles as four floats: unreadable sides read as the
/// infinite box.
#[pyfunction]
fn util_intersect_rect(one: &Bound<'_, PyAny>, two: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let (Some(one), Some(two)) = (parse_rect(one), parse_rect(two)) else {
        return Ok(infinite_edges());
    };
    let out = pdfluent::compat::util_intersect_rect(one, two);
    Ok(vec![out.x0, out.y0, out.x1, out.y1])
}

/// Inverse of a matrix: the status plus the six floats, or the singular
/// status with nothing. An unreadable matrix reads as the identity.
#[pyfunction]
fn util_invert_matrix(matrix: &Bound<'_, PyAny>) -> PyResult<(i32, Vec<f64>)> {
    let identity = CoreMatrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    let matrix = parse_matrix(matrix).unwrap_or(identity);
    if matrix.a * matrix.d - matrix.b * matrix.c == 0.0 {
        return Ok((1, Vec::new()));
    }
    let out = pdfluent::compat::util_invert_matrix(matrix);
    Ok((0, vec![out.a, out.b, out.c, out.d, out.e, out.f]))
}

/// Point containment code: one inside, zero outside, one when the sides
/// do not scan.
#[pyfunction]
fn util_is_point_in_rect(point: &Bound<'_, PyAny>, rect: &Bound<'_, PyAny>) -> PyResult<i32> {
    let (Some(point), Some(rect)) = (parse_point(point), parse_rect(rect)) else {
        return Ok(1);
    };
    Ok(pdfluent::compat::util_is_point_in_rect(point, rect))
}

/// One point as two edges: attributes or a two-row, else nothing. The
/// original objects ride through, so integers stay integers.
fn point_edges(obj: &Bound<'_, PyAny>) -> Option<(PyObject, PyObject)> {
    if obj.hasattr("x").unwrap_or(false) {
        let x = obj.getattr("x").ok()?;
        let y = obj.getattr("y").ok()?;
        if as_number(&x).is_some() && as_number(&y).is_some() {
            return Some((x.unbind(), y.unbind()));
        }
        return None;
    }
    let items = sequence_items(obj)?;
    if items.len() != 2 || items.iter().any(|item| as_number(item).is_none()) {
        return None;
    }
    let mut pair = items.iter();
    Some((pair.next()?.clone().unbind(), pair.next()?.clone().unbind()))
}

/// Explicit nulls read as absent keywords, as in the geometry lane.
fn given<'a>(value: Option<&'a Bound<'a, PyAny>>) -> Option<&'a Bound<'a, PyAny>> {
    match value {
        Some(bound) if bound.is_none() => None,
        other => other,
    }
}

/// Four edges from the rectangle spellings: four numbers, two points, the
/// point keywords, the edge keywords, or nothing at all. Anything else is
/// an assertion error, never a type error.
fn make_edges(
    args: &Bound<'_, PyTuple>,
    p0: Option<&Bound<'_, PyAny>>,
    p1: Option<&Bound<'_, PyAny>>,
    x0: Option<&Bound<'_, PyAny>>,
    y0: Option<&Bound<'_, PyAny>>,
    x1: Option<&Bound<'_, PyAny>>,
    y1: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<PyObject>> {
    let (p0, p1, x0, y0, x1, y1) = (
        given(p0),
        given(p1),
        given(x0),
        given(y0),
        given(x1),
        given(y1),
    );
    let keywords = p0.is_some()
        || p1.is_some()
        || x0.is_some()
        || y0.is_some()
        || x1.is_some()
        || y1.is_some();
    let failure = || PyAssertionError::new_err("need four edges, two points, or nothing");
    if keywords {
        if args.len() != 0 {
            return Err(failure());
        }
        if let (Some(p0), Some(p1)) = (p0, p1) {
            if x0.is_some() || y0.is_some() || x1.is_some() || y1.is_some() {
                return Err(failure());
            }
            let (Some(first), Some(second)) = (point_edges(p0), point_edges(p1)) else {
                return Err(failure());
            };
            return Ok(vec![first.0, first.1, second.0, second.1]);
        }
        if p0.is_some() || p1.is_some() {
            return Err(failure());
        }
        match (x0, y0, x1, y1) {
            (Some(x0), Some(y0), Some(x1), Some(y1)) => {
                for edge in [x0, y0, x1, y1] {
                    if as_number(edge).is_none() {
                        return Err(failure());
                    }
                }
                Ok(vec![
                    x0.clone().unbind(),
                    y0.clone().unbind(),
                    x1.clone().unbind(),
                    y1.clone().unbind(),
                ])
            }
            _ => Err(failure()),
        }
    } else {
        let items = args.iter().collect::<Vec<_>>();
        match items.len() {
            0 => Ok(vec![
                0i32.into_pyobject(args.py())?.into_any().unbind(),
                0i32.into_pyobject(args.py())?.into_any().unbind(),
                0i32.into_pyobject(args.py())?.into_any().unbind(),
                0i32.into_pyobject(args.py())?.into_any().unbind(),
            ]),
            2 => {
                let (Some(first), Some(second)) = (point_edges(&items[0]), point_edges(&items[1]))
                else {
                    return Err(failure());
                };
                Ok(vec![first.0, first.1, second.0, second.1])
            }
            4 => {
                for item in &items {
                    if as_number(item).is_none() {
                        return Err(failure());
                    }
                }
                Ok(items.iter().map(|item| item.clone().unbind()).collect())
            }
            _ => Err(failure()),
        }
    }
}

/// Integer rectangle from edges: the spellings of `make_edges`, every
/// edge truncated to an integer.
#[pyfunction]
#[pyo3(signature = (*args, p0 = None, p1 = None, x0 = None, y0 = None, x1 = None, y1 = None))]
#[allow(clippy::too_many_arguments)]
fn util_make_irect(
    py: Python<'_>,
    args: &Bound<'_, PyTuple>,
    p0: Option<&Bound<'_, PyAny>>,
    p1: Option<&Bound<'_, PyAny>>,
    x0: Option<&Bound<'_, PyAny>>,
    y0: Option<&Bound<'_, PyAny>>,
    x1: Option<&Bound<'_, PyAny>>,
    y1: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let edges = make_edges(args, p0, p1, x0, y0, x1, y1)?;
    let mut out = Vec::with_capacity(4);
    for edge in &edges {
        let edge = edge.bind(py);
        if let Ok(int) = edge.extract::<i64>() {
            out.push((int as i32).into_pyobject(py)?.into_any().unbind());
        } else if let Ok(float) = edge.extract::<f64>() {
            out.push(
                (float.trunc() as i32)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind(),
            );
        } else {
            return Err(PyAssertionError::new_err(
                "need four edges, two points, or nothing",
            ));
        }
    }
    Ok(PyList::new(py, out)?.into_any().unbind())
}

/// Rectangle from edges: the spellings of `make_edges`, every edge riding
/// through in its own number kind.
#[pyfunction]
#[pyo3(signature = (*args, p0 = None, p1 = None, x0 = None, y0 = None, x1 = None, y1 = None))]
#[allow(clippy::too_many_arguments)]
fn util_make_rect(
    py: Python<'_>,
    args: &Bound<'_, PyTuple>,
    p0: Option<&Bound<'_, PyAny>>,
    p1: Option<&Bound<'_, PyAny>>,
    x0: Option<&Bound<'_, PyAny>>,
    y0: Option<&Bound<'_, PyAny>>,
    x1: Option<&Bound<'_, PyAny>>,
    y1: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let edges = make_edges(args, p0, p1, x0, y0, x1, y1)?;
    Ok(PyList::new(py, edges)?.into_any().unbind())
}

/// Typeset width of a string: nothing measures as the integer zero, an
/// unknown face is an argument error, and the rest is advances by size.
#[pyfunction]
fn util_measure_string(
    py: Python<'_>,
    text: String,
    fontname: String,
    fontsize: f64,
    encoding: i32,
) -> PyResult<PyObject> {
    if text.is_empty() {
        return Ok(0i32.into_pyobject(py)?.into_any().unbind());
    }
    if pdfluent::compat::resolve_measure_font(&fontname).is_none() {
        return Err(FzErrorArgument::new_err(format!(
            "unknown font {fontname:?}"
        )));
    }
    let width = pdfluent::compat::util_measure_string(&text, &fontname, fontsize, encoding);
    Ok(width.into_pyobject(py)?.into_any().unbind())
}

/// Whether a quad covers a point: one or zero, and zero when the sides do
/// not scan.
#[pyfunction]
fn util_point_in_quad(point: &Bound<'_, PyAny>, quad: &Bound<'_, PyAny>) -> PyResult<i32> {
    let (Some(point), Some(quad)) = (parse_point(point), parse_quad(quad)) else {
        return Ok(0);
    };
    Ok(i32::from(pdfluent::compat::util_point_in_quad(point, quad)))
}

/// Rounded integer copy as four integers: unreadable sides read as the
/// small infinite box.
#[pyfunction]
fn util_round_rect(rect: &Bound<'_, PyAny>) -> PyResult<Vec<i32>> {
    let Some(rect) = parse_rect(rect) else {
        return Ok(vec![-16777216, -16777216, 16777216, 16777216]);
    };
    let out = pdfluent::compat::util_round_rect(rect);
    Ok(vec![out.x0, out.y0, out.x1, out.y1])
}

/// Sine between two points: half-angle sine, or zero when the sides do
/// not scan.
#[pyfunction]
fn util_sine_between(
    center: &Bound<'_, PyAny>,
    point: &Bound<'_, PyAny>,
    other: &Bound<'_, PyAny>,
) -> PyResult<f64> {
    let (Some(center), Some(point), Some(other)) =
        (parse_point(center), parse_point(point), parse_point(other))
    else {
        return Ok(0.0);
    };
    Ok(pdfluent::compat::util_sine_between(center, point, other))
}

/// Transformed point as two floats: unreadable sides read as the far
/// corner.
#[pyfunction]
fn util_transform_point(point: &Bound<'_, PyAny>, matrix: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let (Some(point), Some(matrix)) = (parse_point(point), parse_matrix(matrix)) else {
        return Ok(vec![-2147483648.0, -2147483648.0]);
    };
    let out = pdfluent::compat::util_transform_point(point, matrix);
    Ok(vec![out.x, out.y])
}

/// Transformed rectangle as four floats: unreadable sides read as the
/// infinite box.
#[pyfunction]
fn util_transform_rect(rect: &Bound<'_, PyAny>, matrix: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let (Some(rect), Some(matrix)) = (parse_rect(rect), parse_matrix(matrix)) else {
        return Ok(infinite_edges());
    };
    let out = pdfluent::compat::util_transform_rect(rect, matrix);
    Ok(vec![out.x0, out.y0, out.x1, out.y1])
}

/// Cover of two rectangles as four floats: unreadable sides read as the
/// infinite box.
#[pyfunction]
fn util_union_rect(one: &Bound<'_, PyAny>, two: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let (Some(one), Some(two)) = (parse_rect(one), parse_rect(two)) else {
        return Ok(infinite_edges());
    };
    let out = pdfluent::compat::util_union_rect(one, two);
    Ok(vec![out.x0, out.y0, out.x1, out.y1])
}

/// Register every module-level name of lane L5.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PdfObj>()?;
    m.add_class::<ElementPosition>()?;
    m.add_function(pyo3::wrap_pyfunction!(check_color, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_marker_arg, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_morph, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_parent, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(color_code, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(conversion_header, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(conversion_trailer, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(empty_irect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(empty_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(empty_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(infinite_irect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(infinite_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(infinite_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(pdf_name, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(dest_is_valid, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(dest_is_valid_page, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(pdfobj_string, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(assert_pdf, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(ensure_operation, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(raisepy, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(hdist, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(vdist, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(compute_scissor, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(detect_super_script, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(pdf_lookup_page_loc, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(string_in_names_list, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(page_merge, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(extra_fzdocument_insert_pdf, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(strip_outline, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(strip_outlines, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(find_string, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(match_string, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(format_g, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_pdf_now, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_pdf_str, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_tjstr, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(make_escape, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(make_story_elpos, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(no_recommend_layout, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(planish_line, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(restore_aliases, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_env_bool, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_env_int, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(paper_size, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(paper_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(paper_sizes, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(s_rgb_to_pdf, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(s_rgb_to_rgb, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(message, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(message_warning, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(log, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(set_log, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(set_messages, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(py_sequence_check, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(py_sequence_size, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(
        py_unicode_decode_raw_unicode_escape,
        m
    )?)?;
    m.add_function(pyo3::wrap_pyfunction!(dir_str, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(exception_info, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(args_match, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(colors_pdf_dict, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(colors_wx_list, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_highlight_selection, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(apply_pages, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(annot_preprocess, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(annot_postprocess, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_ensure_widget_calc, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_concat_matrix, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_hor_matrix, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_include_point_in_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_intersect_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_invert_matrix, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_is_point_in_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_make_irect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_make_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_measure_string, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_point_in_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_round_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_sine_between, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_transform_point, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_transform_rect, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(util_union_rect, m)?)?;
    Ok(())
}
