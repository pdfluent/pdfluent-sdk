//! Compatible text extraction (lane L1): `Page`/`Document` text methods.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;

use pdfluent::compat::constants::{TEXTFLAGS_SEARCH, TEXTFLAGS_TEXT};
use pdfluent::compat::{
    CapturedPage, DictBlock, DictLine, DictSpan, Matrix as CompatMatrix, Point as CompatPoint,
    Quad as CompatQuad, RawChar, Rect as CompatRect, SearchHits, Table as CompatTable,
    TableCell as CompatTableCell, TableFinder as CompatTableFinder, TextDict, TextFormat,
    TextOutput, TextTrace, TextWord, Value as CompatValue,
};
use pdfluent::compat::{TEXT_DEHYPHENATE, TEXT_INHIBIT_SPACES};
use pdfluent::Error as FacadeError;
use pyo3::conversion::IntoPyObject as _;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyFileNotFoundError, PyIndexError, PyKeyError,
    PyRuntimeError, PyTypeError, PyValueError, PyZeroDivisionError,
};
use pyo3::types::{
    PyAny, PyAnyMethods, PyBool, PyBytes, PyDict, PyDictMethods, PyList, PyModule, PyModuleMethods,
    PyTuple, PyTupleMethods,
};
use pyo3::{pyclass, pyfunction, pymethods, Bound, PyObject, PyResult, Python};

use super::edit::PyFont;
use crate::{PyDocument, PyPage};

/// Open a model-free engine handle over document bytes, keeping the
/// ligature code points the recordings read.
fn compat_engine(bytes: &[u8]) -> Result<pdf_engine::PdfDocument, pyo3::PyErr> {
    let mut engine = pdf_engine::PdfDocument::open(bytes.to_vec())
        .map_err(|e| PyRuntimeError::new_err(format!("cannot read document: {e}")))?;
    engine.set_keep_ligatures(true);
    Ok(engine)
}

/// Map a facade error onto the recorded exception classes.
fn compat_err(err: FacadeError) -> pyo3::PyErr {
    crate::compat::errors::member_error(crate::compat::errors::Member::TextCapture, err)
}

/// Capture one page through a fresh engine handle, off the GIL.
fn capture_page_off_gil(
    py: Python<'_>,
    bytes: &[u8],
    page: usize,
    with_figures: bool,
) -> PyResult<CapturedPage> {
    let owned = bytes.to_vec();
    py.allow_threads(|| {
        let engine = compat_engine(&owned)?;
        pdfluent::compat::capture_page(&engine, page, with_figures).map_err(compat_err)
    })
}

/// The source bytes behind a page: the bytes its engine handle parses.
pub(crate) fn page_bytes(page: &PyPage) -> &[u8] {
    page.compat_engine().pdf().data().as_ref()
}

/// Read one float attribute, or fail.
fn attr_f64(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<f64> {
    obj.getattr(name)?
        .extract::<f64>()
        .map_err(|_| PyTypeError::new_err(format!("{name} must be a number, on {obj:?}")))
}

/// Parse a rectangle: `x0/y0/x1/y1` attributes, or a four-number sequence.
fn parse_rect(obj: &Bound<'_, PyAny>) -> PyResult<CompatRect> {
    if obj.hasattr("x0").unwrap_or(false) {
        return Ok(CompatRect {
            x0: attr_f64(obj, "x0")?,
            y0: attr_f64(obj, "y0")?,
            x1: attr_f64(obj, "x1")?,
            y1: attr_f64(obj, "y1")?,
        });
    }
    if let Ok(seq) = obj.extract::<Vec<f64>>() {
        if seq.len() == 4 {
            return Ok(CompatRect {
                x0: seq[0],
                y0: seq[1],
                x1: seq[2],
                y1: seq[3],
            });
        }
    }
    Err(PyTypeError::new_err(
        "rect must have x0/y0/x1/y1 attributes or be a 4-number sequence",
    ))
}

/// Parse an optional rectangle: `None` stays `None`.
fn parse_opt_rect(obj: &Bound<'_, PyAny>) -> PyResult<Option<CompatRect>> {
    if obj.is_none() {
        Ok(None)
    } else {
        parse_rect(obj).map(Some)
    }
}

/// Parse a point: `x/y` attributes, or a two-number sequence.
fn parse_point(obj: &Bound<'_, PyAny>) -> PyResult<CompatPoint> {
    if obj.hasattr("x").unwrap_or(false) {
        return Ok(CompatPoint {
            x: attr_f64(obj, "x")?,
            y: attr_f64(obj, "y")?,
        });
    }
    if let Ok(seq) = obj.extract::<Vec<f64>>() {
        if seq.len() == 2 {
            return Ok(CompatPoint {
                x: seq[0],
                y: seq[1],
            });
        }
    }
    Err(PyTypeError::new_err(
        "point must have x/y attributes or be a 2-number sequence",
    ))
}

/// Parse a matrix: `a`-`f` attributes, or a six-number sequence.
fn parse_matrix(obj: &Bound<'_, PyAny>) -> PyResult<CompatMatrix> {
    if obj.hasattr("a").unwrap_or(false) {
        return Ok(CompatMatrix {
            a: attr_f64(obj, "a")?,
            b: attr_f64(obj, "b")?,
            c: attr_f64(obj, "c")?,
            d: attr_f64(obj, "d")?,
            e: attr_f64(obj, "e")?,
            f: attr_f64(obj, "f")?,
        });
    }
    if let Ok(seq) = obj.extract::<Vec<f64>>() {
        if seq.len() == 6 {
            return Ok(CompatMatrix {
                a: seq[0],
                b: seq[1],
                c: seq[2],
                d: seq[3],
                e: seq[4],
                f: seq[5],
            });
        }
    }
    Err(PyTypeError::new_err(
        "matrix must have a/b/c/d/e/f attributes or be a 6-number sequence",
    ))
}

/// Parse a two-number direction.
fn parse_direction(obj: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    if let Ok(seq) = obj.extract::<Vec<f64>>() {
        if seq.len() == 2 {
            return Ok((seq[0], seq[1]));
        }
    }
    Err(PyValueError::new_err(
        "direction must be a 2-number sequence",
    ))
}

/// Build the geometry lane's rectangle: looked up at call time so this
/// lane compiles and loads before that lane lands.
fn native_rect(py: Python<'_>, rect: CompatRect) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("Rect")?
        .call1((rect.x0, rect.y0, rect.x1, rect.y1))?;
    Ok(made.unbind())
}

/// Build the geometry lane's point.
fn native_point(py: Python<'_>, point: CompatPoint) -> PyResult<PyObject> {
    let made = py
        .import("pdfluent")?
        .getattr("Point")?
        .call1((point.x, point.y))?;
    Ok(made.unbind())
}

/// Build the geometry lane's quad from four corner points.
fn native_quad(py: Python<'_>, quad: CompatQuad) -> PyResult<PyObject> {
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

/// A dynamic value as a Python object.
fn value_to_py(py: Python<'_>, value: &CompatValue) -> PyResult<PyObject> {
    match value {
        CompatValue::Null => Ok(py.None()),
        CompatValue::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any().unbind()),
        CompatValue::Int(v) => Ok(v.into_pyobject(py)?.into_any().unbind()),
        CompatValue::Float(v) => Ok(v.into_pyobject(py)?.into_any().unbind()),
        CompatValue::Str(s) => Ok(s.clone().into_pyobject(py)?.into_any().unbind()),
        CompatValue::Bytes(b) => Ok(PyBytes::new(py, b).into_any().unbind()),
        CompatValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_to_py(py, item)?);
            }
            Ok(PyList::new(py, out)?.into_any().unbind())
        }
        CompatValue::Dict(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map {
                dict.set_item(key, value_to_py(py, item)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        _ => Err(crate::compat::unknown_variant("compat value")),
    }
}

/// A Python object as a dynamic value: none, bool, int, float, string,
/// bytes, list or tuple, or string-keyed dict.
fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<CompatValue> {
    if obj.is_none() {
        return Ok(CompatValue::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        // Bools extract as ints too, so they go first.
        return Ok(CompatValue::Bool(b));
    }
    if let Ok(v) = obj.extract::<i64>() {
        return Ok(CompatValue::Int(v));
    }
    if let Ok(v) = obj.extract::<f64>() {
        return Ok(CompatValue::Float(v));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(CompatValue::Str(s));
    }
    if obj.is_instance_of::<pyo3::types::PyBytes>() {
        let bytes: Vec<u8> = obj.extract()?;
        return Ok(CompatValue::Bytes(bytes));
    }
    if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
        let mut out = Vec::new();
        for item in obj.try_iter()? {
            out.push(py_to_value(&item?)?);
        }
        return Ok(CompatValue::List(out));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = HashMap::new();
        for (key, item) in dict.iter() {
            let name: String = key
                .extract()
                .map_err(|_| PyTypeError::new_err("dict keys must be strings"))?;
            map.insert(name, py_to_value(&item)?);
        }
        return Ok(CompatValue::Dict(map));
    }
    Err(PyTypeError::new_err(format!(
        "cannot convert {obj:?} to a dynamic value"
    )))
}

// ===== Binding part 1 end marker P1 =====

// ===== Lane L1: converters and owned classes =====

/// One word row: box, text, and block/line/word numbers.
fn word_row_to_py(py: Python<'_>, word: &TextWord) -> PyResult<PyObject> {
    let row = PyList::new(
        py,
        [
            word.rect.x0.into_pyobject(py)?.into_any().unbind(),
            word.rect.y0.into_pyobject(py)?.into_any().unbind(),
            word.rect.x1.into_pyobject(py)?.into_any().unbind(),
            word.rect.y1.into_pyobject(py)?.into_any().unbind(),
            word.text.clone().into_pyobject(py)?.into_any().unbind(),
            word.block_no.into_pyobject(py)?.into_any().unbind(),
            word.line_no.into_pyobject(py)?.into_any().unbind(),
            word.word_no.into_pyobject(py)?.into_any().unbind(),
        ],
    )?;
    Ok(row.into_any().unbind())
}

/// One text trace: the recorded nineteen keys.
fn trace_to_py(py: Python<'_>, trace: &TextTrace) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("ascender", trace.ascender)?;
    dict.set_item(
        "bbox",
        [trace.bbox.x0, trace.bbox.y0, trace.bbox.x1, trace.bbox.y1],
    )?;
    dict.set_item("bidi_dir", trace.bidi_dir)?;
    dict.set_item("bidi_lvl", trace.bidi_level)?;
    let mut chars = Vec::with_capacity(trace.chars.len());
    for ch in &trace.chars {
        chars.push(PyList::new(
            py,
            [
                ch.codepoint.into_pyobject(py)?.into_any().unbind(),
                ch.glyph.into_pyobject(py)?.into_any().unbind(),
                PyList::new(py, [ch.origin.x, ch.origin.y])?
                    .into_any()
                    .unbind(),
                PyList::new(py, [ch.bbox.x0, ch.bbox.y0, ch.bbox.x1, ch.bbox.y1])?
                    .into_any()
                    .unbind(),
            ],
        )?);
    }
    dict.set_item("chars", PyList::new(py, chars)?)?;
    dict.set_item("color", [trace.color[0], trace.color[1], trace.color[2]])?;
    dict.set_item("colorspace", trace.colorspace)?;
    dict.set_item("descender", trace.descender)?;
    dict.set_item("dir", [trace.dir.x, trace.dir.y])?;
    dict.set_item("flags", trace.flags)?;
    dict.set_item("font", trace.font.clone())?;
    dict.set_item("layer", trace.layer.clone())?;
    dict.set_item("linewidth", trace.linewidth)?;
    dict.set_item("opacity", trace.opacity)?;
    dict.set_item("seqno", trace.seqno)?;
    dict.set_item("size", trace.size)?;
    dict.set_item("spacewidth", trace.spacewidth)?;
    dict.set_item("type", trace.span_type)?;
    dict.set_item("wmode", trace.wmode)?;
    Ok(dict.into_any().unbind())
}

/// One dict block: text blocks carry lines, image blocks carry bytes.
fn dict_block_to_py(py: Python<'_>, block: &DictBlock, raw: bool) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    match block {
        DictBlock::Text {
            bbox,
            number,
            flags,
            lines,
        } => {
            dict.set_item("bbox", [bbox.x0, bbox.y0, bbox.x1, bbox.y1])?;
            dict.set_item("flags", *flags)?;
            let mut out = Vec::with_capacity(lines.len());
            for line in lines {
                out.push(dict_line_to_py(py, line, raw)?);
            }
            dict.set_item("lines", PyList::new(py, out)?)?;
            dict.set_item("number", *number)?;
            dict.set_item("type", 0)?;
        }
        DictBlock::Image {
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
        } => {
            dict.set_item("bbox", [bbox.x0, bbox.y0, bbox.x1, bbox.y1])?;
            dict.set_item("bpc", *bpc)?;
            dict.set_item("colorspace", *colorspace)?;
            dict.set_item("ext", ext.clone())?;
            dict.set_item("height", *height)?;
            dict.set_item("image", PyBytes::new(py, image_data))?;
            dict.set_item("mask", py.None())?;
            dict.set_item("number", *number)?;
            dict.set_item("size", image_data.len() as i32)?;
            dict.set_item(
                "transform",
                [
                    transform.a,
                    transform.b,
                    transform.c,
                    transform.d,
                    transform.e,
                    transform.f,
                ],
            )?;
            dict.set_item("type", 1)?;
            dict.set_item("width", *width)?;
            dict.set_item("xres", *xres)?;
            dict.set_item("yres", *yres)?;
        }
        _ => {
            unreachable!("future dict block variant");
        }
    }
    Ok(dict.into_any().unbind())
}

/// One dict line: box, direction, spans, writing mode.
fn dict_line_to_py(py: Python<'_>, line: &DictLine, raw: bool) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item(
        "bbox",
        [line.bbox.x0, line.bbox.y0, line.bbox.x1, line.bbox.y1],
    )?;
    dict.set_item("dir", [line.direction.x, line.direction.y])?;
    let mut out = Vec::with_capacity(line.spans.len());
    for span in &line.spans {
        out.push(dict_span_to_py(py, span, raw)?);
    }
    dict.set_item("spans", PyList::new(py, out)?)?;
    dict.set_item("wmode", line.wmode)?;
    Ok(dict.into_any().unbind())
}

/// One dict span: style, text (or characters in raw mode), geometry.
fn dict_span_to_py(py: Python<'_>, span: &DictSpan, raw: bool) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("alpha", span.alpha)?;
    dict.set_item("ascender", span.ascender)?;
    dict.set_item(
        "bbox",
        [span.bbox.x0, span.bbox.y0, span.bbox.x1, span.bbox.y1],
    )?;
    dict.set_item("bidi", span.bidi)?;
    dict.set_item("char_flags", span.char_flags)?;
    dict.set_item("color", span.color)?;
    dict.set_item("descender", span.descender)?;
    dict.set_item("flags", span.flags)?;
    dict.set_item("font", span.font.clone())?;
    dict.set_item("origin", [span.origin.x, span.origin.y])?;
    dict.set_item("size", span.size)?;
    if raw {
        let mut chars = Vec::with_capacity(span.chars.len());
        for ch in &span.chars {
            let entry = PyDict::new(py);
            entry.set_item("bbox", [ch.bbox.x0, ch.bbox.y0, ch.bbox.x1, ch.bbox.y1])?;
            entry.set_item("c", ch.ch.to_string())?;
            entry.set_item("origin", [ch.origin.x, ch.origin.y])?;
            entry.set_item("synthetic", ch.synthetic)?;
            chars.push(entry);
        }
        dict.set_item("chars", PyList::new(py, chars)?)?;
    } else {
        dict.set_item("text", span.text.clone())?;
    }
    Ok(dict.into_any().unbind())
}

/// One structured dict: size plus blocks.
fn text_dict_to_py(py: Python<'_>, dict: &TextDict, raw: bool) -> PyResult<PyObject> {
    let out = PyDict::new(py);
    let mut blocks = Vec::with_capacity(dict.blocks.len());
    for block in &dict.blocks {
        blocks.push(dict_block_to_py(py, block, raw)?);
    }
    out.set_item("blocks", PyList::new(py, blocks)?)?;
    out.set_item("height", dict.height)?;
    out.set_item("width", dict.width)?;
    Ok(out.into_any().unbind())
}

/// One extraction result: strings, row lists, or a structured dict.
fn output_to_py(py: Python<'_>, output: TextOutput, raw: bool) -> PyResult<PyObject> {
    match output {
        TextOutput::Text(text)
        | TextOutput::Json(text)
        | TextOutput::RawJson(text)
        | TextOutput::Html(text)
        | TextOutput::Xhtml(text)
        | TextOutput::Xml(text) => Ok(text.into_pyobject(py)?.into_any().unbind()),
        TextOutput::Words(words) => {
            let mut out = Vec::with_capacity(words.len());
            for word in &words {
                out.push(word_row_to_py(py, word)?);
            }
            Ok(PyList::new(py, out)?.into_any().unbind())
        }
        TextOutput::Blocks(blocks) => {
            let mut out = Vec::with_capacity(blocks.len());
            for block in &blocks {
                out.push(
                    PyList::new(
                        py,
                        [
                            block.rect.x0.into_pyobject(py)?.into_any().unbind(),
                            block.rect.y0.into_pyobject(py)?.into_any().unbind(),
                            block.rect.x1.into_pyobject(py)?.into_any().unbind(),
                            block.rect.y1.into_pyobject(py)?.into_any().unbind(),
                            block.text.clone().into_pyobject(py)?.into_any().unbind(),
                            block.number.into_pyobject(py)?.into_any().unbind(),
                            block.block_type.into_pyobject(py)?.into_any().unbind(),
                        ],
                    )?
                    .into_any()
                    .unbind(),
                );
            }
            Ok(PyList::new(py, out)?.into_any().unbind())
        }
        TextOutput::Dict(dict) => text_dict_to_py(py, &dict, raw),
        _ => Err(crate::compat::unknown_variant("text output")),
    }
}

/// One search result list: rectangles, or quads past a quarter turn.
fn search_hits_to_py(py: Python<'_>, hits: SearchHits) -> PyResult<PyObject> {
    let mut out = Vec::new();
    match hits {
        SearchHits::Rects(rects) => {
            for rect in rects {
                out.push(native_rect(py, rect)?);
            }
        }
        SearchHits::Quads(quads) => {
            for quad in quads {
                out.push(native_quad(py, quad)?);
            }
        }
        _ => return Err(crate::compat::unknown_variant("search hits")),
    }
    Ok(PyList::new(py, out)?.into_any().unbind())
}

/// Parse the text option word: present but not a string is an attribute
/// error, unknown after lowercasing an assertion error.
fn parse_format(option: Option<&Bound<'_, PyAny>>) -> PyResult<TextFormat> {
    let Some(obj) = option else {
        return Ok(TextFormat::Text);
    };
    if obj.is_none() {
        return Ok(TextFormat::Text);
    }
    let name: String = obj
        .extract()
        .map_err(|_| PyAttributeError::new_err("text option must be a string"))?;
    TextFormat::from_name(&name)
        .map_err(|_| PyAssertionError::new_err(format!("unknown text option {name:?}")))
}

/// An extracted text page: owned captures plus its clip and flags.
///
/// Unlike the borrowed Rust view this owns its content, so extending it
/// folds another page's capture in for good.
#[pyclass(name = "TextPage")]
#[derive(Debug, Clone)]
pub(crate) struct PyTextPage {
    capture: CapturedPage,
    clip: Option<CompatRect>,
    flags: i32,
    matrix: Option<CompatMatrix>,
    page_no: usize,
}

impl PyTextPage {
    /// Extraction window: our clip seen through our matrix. `rect` stays
    /// the raw clip.
    fn window(&self) -> Option<CompatRect> {
        pdfluent::compat::textpage_window(
            self.capture.width,
            self.capture.height,
            self.clip,
            self.matrix,
        )
    }
}

#[pymethods]
#[allow(non_snake_case)]
impl PyTextPage {
    /// A text page only comes from a page: direct construction raises.
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let _ = args;
        Err(crate::compat::errors::validation_error(
            crate::compat::errors::ValidationKind::Refused,
            "TextPage() cannot build a text page; use Page.get_textpage()",
        ))
    }

    /// The clip this page reads through, or the full page.
    #[getter]
    fn rect(&self, py: Python<'_>) -> PyResult<PyObject> {
        match self.clip {
            Some(clip) => native_rect(py, clip),
            None => native_rect(
                py,
                CompatRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: self.capture.width,
                    y1: self.capture.height,
                },
            ),
        }
    }

    /// Pool size: unsupported, and recorded as raising.
    fn poolsize(&self) -> PyResult<usize> {
        Err(PyAttributeError::new_err(
            "TextPage.poolsize is not supported",
        ))
    }

    /// Substring search over this page's lines.
    #[pyo3(signature = (needle, hit_max = 0, quads = None))]
    fn search(
        &self,
        py: Python<'_>,
        needle: &Bound<'_, PyAny>,
        hit_max: i32,
        quads: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let needle: String = needle
            .extract()
            .map_err(|_| PyTypeError::new_err("needle must be a string"))?;
        let quads = quads
            .as_ref()
            .map(|q| q.is_truthy())
            .transpose()?
            .unwrap_or(true);
        let hits = self
            .capture
            .render_search(&needle, self.window(), quads, hit_max);
        search_hits_to_py(py, hits)
    }

    /// Blocks of this page.
    fn extractBLOCKS(&self, py: Python<'_>) -> PyResult<PyObject> {
        output_to_py(
            py,
            TextOutput::Blocks(self.capture.render_blocks(self.window(), self.flags, false)),
            false,
        )
    }

    /// Structured dict of this page.
    #[pyo3(signature = (cb = None, sort = None))]
    fn extractDICT(
        &self,
        py: Python<'_>,
        cb: Option<PyObject>,
        sort: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let _ = cb;
        let sort = sort
            .as_ref()
            .map(|s| s.is_truthy())
            .transpose()?
            .unwrap_or(false);
        output_to_py(
            py,
            TextOutput::Dict(
                self.capture
                    .render_dict(self.window(), self.flags, sort, false),
            ),
            false,
        )
    }

    /// HTML fragment of this page.
    fn extractHTML(&self, py: Python<'_>) -> PyResult<PyObject> {
        output_to_py(
            py,
            TextOutput::Html(self.capture.render_html(
                self.page_no,
                self.window(),
                self.flags,
                false,
            )),
            false,
        )
    }

    /// Placed images of this page.
    #[pyo3(signature = (hashes = None))]
    fn extractIMGINFO(
        &self,
        py: Python<'_>,
        hashes: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let hashes = hashes
            .as_ref()
            .map(|h| h.is_truthy())
            .transpose()?
            .unwrap_or(false);
        let infos = pdfluent::compat::figures_to_imginfo(&self.capture.figures, hashes);
        let mut out = Vec::with_capacity(infos.len());
        for info in &infos {
            out.push(placed_image_to_py(py, info)?);
        }
        Ok(PyList::new(py, out)?.into_any().unbind())
    }

    /// JSON text of this page.
    #[pyo3(signature = (cb = None, sort = None))]
    fn extractJSON(
        &self,
        py: Python<'_>,
        cb: Option<PyObject>,
        sort: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let _ = cb;
        let sort = sort
            .as_ref()
            .map(|s| s.is_truthy())
            .transpose()?
            .unwrap_or(false);
        output_to_py(
            py,
            TextOutput::Json(
                self.capture
                    .render_json(self.window(), self.flags, sort, false),
            ),
            false,
        )
    }

    /// Raw dict of this page: per-character boxes.
    #[pyo3(signature = (cb = None, sort = None))]
    fn extractRAWDICT(
        &self,
        py: Python<'_>,
        cb: Option<PyObject>,
        sort: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let _ = cb;
        let sort = sort
            .as_ref()
            .map(|s| s.is_truthy())
            .transpose()?
            .unwrap_or(false);
        output_to_py(
            py,
            TextOutput::Dict(
                self.capture
                    .render_dict(self.window(), self.flags, sort, true),
            ),
            true,
        )
    }

    /// Raw JSON text of this page.
    #[pyo3(signature = (cb = None, sort = None))]
    fn extractRAWJSON(
        &self,
        py: Python<'_>,
        cb: Option<PyObject>,
        sort: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let _ = cb;
        let sort = sort
            .as_ref()
            .map(|s| s.is_truthy())
            .transpose()?
            .unwrap_or(false);
        output_to_py(
            py,
            TextOutput::RawJson(
                self.capture
                    .render_json(self.window(), self.flags, sort, true),
            ),
            true,
        )
    }

    /// Text between two points of this page.
    fn extractSelection(
        &self,
        pointa: &Bound<'_, PyAny>,
        pointb: &Bound<'_, PyAny>,
    ) -> PyResult<String> {
        let begin = parse_point(pointa)?;
        let end = parse_point(pointb)?;
        Ok(self.capture.render_selection(begin, end, self.window()))
    }

    /// Plain text of this page.
    #[pyo3(signature = (sort = None))]
    fn extractTEXT(&self, sort: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        let sort = sort
            .as_ref()
            .map(|s| s.is_truthy())
            .transpose()?
            .unwrap_or(false);
        Ok(self.capture.render_text(self.window(), self.flags, sort))
    }

    /// Plain text of this page, under its alias.
    #[pyo3(signature = (sort = None))]
    fn extractText(&self, sort: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        self.extractTEXT(sort)
    }

    /// Text inside a rectangle of this page.
    fn extractTextbox(&self, rect: &Bound<'_, PyAny>) -> PyResult<String> {
        let rect = parse_rect(rect)?;
        let clip = match self.clip {
            Some(own) => CompatRect {
                x0: own.x0.max(rect.x0),
                y0: own.y0.max(rect.y0),
                x1: own.x1.min(rect.x1),
                y1: own.y1.min(rect.y1),
            },
            None => rect,
        };
        let clip = pdfluent::compat::textpage_window(
            self.capture.width,
            self.capture.height,
            Some(clip),
            self.matrix,
        )
        .unwrap_or(clip);
        Ok(self.capture.render_textbox(clip, self.flags))
    }

    /// Words of this page.
    #[pyo3(signature = (delimiters = None))]
    fn extractWORDS(&self, py: Python<'_>, delimiters: Option<String>) -> PyResult<PyObject> {
        output_to_py(
            py,
            TextOutput::Words(self.capture.render_words(
                self.window(),
                self.flags,
                false,
                delimiters.as_deref(),
            )),
            false,
        )
    }

    /// XHTML fragment of this page.
    fn extractXHTML(&self, py: Python<'_>) -> PyResult<PyObject> {
        output_to_py(
            py,
            TextOutput::Xhtml(self.capture.render_xhtml(
                self.page_no,
                self.window(),
                self.flags,
                false,
            )),
            false,
        )
    }

    /// XML fragment of this page.
    fn extractXML(&self, py: Python<'_>) -> PyResult<PyObject> {
        output_to_py(
            py,
            TextOutput::Xml(self.capture.render_xml(
                self.page_no,
                self.window(),
                self.flags,
                false,
            )),
            false,
        )
    }

    /// Compare against another text page: identity only, the rest defers.
    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: pyo3::pyclass::CompareOp,
    ) -> PyResult<PyObject> {
        use pyo3::pyclass::CompareOp as Op;
        let py = slf.py();
        if slf.is(other) {
            let same = matches!(op, Op::Eq);
            if matches!(op, Op::Eq | Op::Ne) {
                return Ok(PyBool::new(py, same).to_owned().into_any().unbind());
            }
        }
        Ok(py.NotImplemented())
    }
}

/// One placed image: box, sizes, and the optional digest.
fn placed_image_to_py(py: Python<'_>, info: &pdfluent::compat::PlacedImage) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item(
        "bbox",
        [info.bbox.x0, info.bbox.y0, info.bbox.x1, info.bbox.y1],
    )?;
    dict.set_item("bits", info.bits)?;
    dict.set_item("colorspace_name", info.colorspace_name.clone())?;
    match &info.digest {
        Some(digest) => dict.set_item("digest", PyBytes::new(py, digest))?,
        None => dict.set_item("digest", py.None())?,
    }
    dict.set_item("has_mask", info.has_mask)?;
    dict.set_item("height", info.height)?;
    dict.set_item("number", info.number)?;
    dict.set_item("size", info.size)?;
    dict.set_item(
        "transform",
        [
            info.transform.a,
            info.transform.b,
            info.transform.c,
            info.transform.d,
            info.transform.e,
            info.transform.f,
        ],
    )?;
    dict.set_item("width", info.width)?;
    dict.set_item("xres", info.xres)?;
    dict.set_item("yres", info.yres)?;
    Ok(dict.into_any().unbind())
}

/// One found table: box, grid size, and cells.
#[pyclass(name = "Table")]
#[derive(Debug, Clone)]
pub(crate) struct PyTable {
    bbox: CompatRect,
    row_count: i32,
    col_count: i32,
    cells: Vec<PyTableCell>,
}

#[pymethods]
impl PyTable {
    /// Table box.
    #[getter]
    fn bbox(&self, py: Python<'_>) -> PyResult<PyObject> {
        native_rect(py, self.bbox)
    }

    /// Row count.
    #[getter]
    fn row_count(&self) -> i32 {
        self.row_count
    }

    /// Column count.
    #[getter]
    fn col_count(&self) -> i32 {
        self.col_count
    }

    /// Cells in row-major order.
    #[getter]
    fn cells(&self) -> Vec<PyTableCell> {
        self.cells.clone()
    }
}

/// One table cell: box and text.
#[pyclass(name = "TableCell")]
#[derive(Debug, Clone)]
pub(crate) struct PyTableCell {
    bbox: CompatRect,
    text: String,
}

#[pymethods]
impl PyTableCell {
    /// Cell box.
    #[getter]
    fn bbox(&self, py: Python<'_>) -> PyResult<PyObject> {
        native_rect(py, self.bbox)
    }

    /// Cell text.
    #[getter]
    fn text(&self) -> String {
        self.text.clone()
    }
}

/// Tables found on one page.
#[pyclass(name = "TableFinder")]
#[derive(Debug, Clone)]
pub(crate) struct PyTableFinder {
    tables: Vec<PyTable>,
}

#[pymethods]
impl PyTableFinder {
    /// Found tables.
    #[getter]
    fn tables(&self) -> Vec<PyTable> {
        self.tables.clone()
    }
}

impl PyTableFinder {
    /// Wrap a Lane result for Python.
    fn from_finder(finder: CompatTableFinder) -> Self {
        Self {
            tables: finder
                .tables
                .into_iter()
                .map(|table: CompatTable| PyTable {
                    bbox: table.bbox,
                    row_count: table.row_count,
                    col_count: table.col_count,
                    cells: table
                        .cells
                        .into_iter()
                        .map(|cell: CompatTableCell| PyTableCell {
                            bbox: cell.bbox,
                            text: cell.text,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

// ===== Binding part 2 end marker P2 =====

// ===== Lane L1: page and document methods =====

/// Intersect two optional clips.
fn intersect(a: Option<CompatRect>, b: Option<CompatRect>) -> Option<CompatRect> {
    match (a, b) {
        (None, clip) | (clip, None) => clip,
        (Some(x), Some(y)) => Some(CompatRect {
            x0: x.x0.max(y.x0),
            y0: x.y0.max(y.y0),
            x1: x.x1.min(y.x1),
            y1: x.y1.min(y.y1),
        }),
    }
}

/// Whether a format shows figures (only the dict family does).
fn needs_figures(format: TextFormat) -> bool {
    matches!(format, TextFormat::Dict | TextFormat::RawDict)
}

/// Truthiness with a default for `None`.
fn truthy_or(obj: &Option<Bound<'_, PyAny>>, default: bool) -> PyResult<bool> {
    match obj {
        None => Ok(default),
        Some(o) => o.is_truthy(),
    }
}

#[pymethods]
impl PyPage {
    /// Text of this page in the requested format.
    ///
    /// One positional argument at most (the option word); the keywords
    /// are `option`, `clip`, `flags`, `textpage`, `sort`, `delimiters`.
    /// Anything else is a type error.
    #[pyo3(signature = (*args, **kwargs))]
    fn get_text(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        if args.len() > 1 {
            return Err(PyTypeError::new_err(
                "get_text() takes at most 1 positional argument",
            ));
        }
        let positional = if args.is_empty() {
            None
        } else {
            Some(args.get_item(0)?)
        };
        let mut option = positional.clone();
        let mut clip: Option<CompatRect> = None;
        let mut flags: Option<i32> = None;
        let mut textpage: Option<PyTextPage> = None;
        let mut sort = true;
        let mut delimiters: Option<String> = None;
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                let name: String = key.extract()?;
                match name.as_str() {
                    "option" => {
                        if positional.is_none() {
                            option = Some(value);
                        }
                    }
                    "clip" => clip = parse_opt_rect(&value)?,
                    "flags" => {
                        flags = if value.is_none() {
                            None
                        } else {
                            Some(value.extract::<i32>().map_err(|_| {
                                PyTypeError::new_err("flags must be an int or None")
                            })?)
                        };
                    }
                    "textpage" => {
                        if !value.is_none() {
                            let tp = value
                                .downcast::<PyTextPage>()
                                .map_err(|_| {
                                    PyTypeError::new_err("textpage must be a TextPage or None")
                                })?
                                .borrow()
                                .clone();
                            textpage = Some(tp);
                        }
                    }
                    "sort" => sort = value.is_truthy()?,
                    "delimiters" => {
                        delimiters = if value.is_none() {
                            None
                        } else {
                            Some(value.extract::<String>().map_err(|_| {
                                PyTypeError::new_err("delimiters must be a string or None")
                            })?)
                        };
                    }
                    _ => {
                        return Err(PyTypeError::new_err(format!(
                            "get_text() got an unexpected keyword argument {name:?}"
                        )));
                    }
                }
            }
        }
        let format = parse_format(option.as_ref())?;
        let raw = format == TextFormat::RawDict;
        let (capture, page_no, mut clip, flags) = if let Some(tp) = textpage {
            (
                tp.capture,
                tp.page_no,
                intersect(tp.clip, clip),
                flags.unwrap_or(tp.flags),
            )
        } else {
            let capture = capture_page_off_gil(
                py,
                page_bytes(self),
                self.compat_index(),
                needs_figures(format),
            )?;
            (capture, self.compat_index(), clip, flags.unwrap_or(0))
        };
        let laid = self.compat_clip().map(|edges| CompatRect {
            x0: edges[0].min(edges[2]),
            y0: edges[1].min(edges[3]),
            x1: edges[0].max(edges[2]),
            y1: edges[1].max(edges[3]),
        });
        clip = intersect(clip, laid);
        let output = if format == TextFormat::Words {
            TextOutput::Words(capture.render_words(clip, flags, sort, delimiters.as_deref()))
        } else {
            render_format_owned(&capture, format, page_no, clip, flags, sort)?
        };
        output_to_py(py, output, raw)
    }

    /// Blocks of this page: keyword arguments only.
    #[pyo3(signature = (*args, **kwargs))]
    fn get_text_blocks(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        if !args.is_empty() {
            return Err(PyTypeError::new_err(
                "get_text_blocks() takes no positional arguments",
            ));
        }
        let (clip, flags, textpage, sort) = block_kwargs(kwargs)?;
        let (capture, clip, flags) =
            self.resolve_capture(py, textpage, clip, flags.unwrap_or(0), false)?;
        output_to_py(
            py,
            TextOutput::Blocks(capture.render_blocks(clip, flags, sort)),
            false,
        )
    }

    /// Words of this page: keyword arguments only.
    #[pyo3(signature = (*args, **kwargs))]
    fn get_text_words(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        if !args.is_empty() {
            return Err(PyTypeError::new_err(
                "get_text_words() takes no positional arguments",
            ));
        }
        let mut clip: Option<CompatRect> = None;
        let mut flags: Option<i32> = None;
        let mut textpage: Option<PyTextPage> = None;
        let mut sort = true;
        let mut delimiters: Option<String> = None;
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                let name: String = key.extract()?;
                match name.as_str() {
                    "clip" => clip = parse_opt_rect(&value)?,
                    "flags" => {
                        flags = if value.is_none() {
                            None
                        } else {
                            Some(value.extract::<i32>().map_err(|_| {
                                PyTypeError::new_err("flags must be an int or None")
                            })?)
                        };
                    }
                    "textpage" => {
                        if !value.is_none() {
                            let tp = value
                                .downcast::<PyTextPage>()
                                .map_err(|_| {
                                    PyTypeError::new_err("textpage must be a TextPage or None")
                                })?
                                .borrow()
                                .clone();
                            textpage = Some(tp);
                        }
                    }
                    "sort" => sort = value.is_truthy()?,
                    "delimiters" => {
                        delimiters = if value.is_none() {
                            None
                        } else {
                            Some(value.extract::<String>().map_err(|_| {
                                PyTypeError::new_err("delimiters must be a string or None")
                            })?)
                        };
                    }
                    _ => {
                        return Err(PyTypeError::new_err(format!(
                            "get_text_words() got an unexpected keyword argument {name:?}"
                        )));
                    }
                }
            }
        }
        let (capture, clip, flags) =
            self.resolve_capture(py, textpage, clip, flags.unwrap_or(0), false)?;
        output_to_py(
            py,
            TextOutput::Words(capture.render_words(clip, flags, sort, delimiters.as_deref())),
            false,
        )
    }

    /// Text between two points of this page.
    #[pyo3(signature = (begin, end, clip = None))]
    fn get_text_selection(
        &self,
        py: Python<'_>,
        begin: &Bound<'_, PyAny>,
        end: &Bound<'_, PyAny>,
        clip: Option<Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        let begin = parse_point(begin)?;
        let end = parse_point(end)?;
        let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
        let capture = capture_page_off_gil(py, page_bytes(self), self.compat_index(), false)?;
        Ok(capture.render_selection(begin, end, clip))
    }

    /// Text inside a rectangle of this page.
    #[pyo3(signature = (rect, textpage = None))]
    fn get_textbox(
        &self,
        py: Python<'_>,
        rect: &Bound<'_, PyAny>,
        textpage: Option<Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        let rect = parse_rect(rect)?;
        let textpage = textpage
            .as_ref()
            .filter(|t| !t.is_none())
            .map(|t| {
                t.downcast::<PyTextPage>()
                    .map_err(|_| PyTypeError::new_err("textpage must be a TextPage or None"))
                    .map(|tp| tp.borrow().clone())
            })
            .transpose()?;
        let (capture, clip, flags) = self.resolve_capture(py, textpage, Some(rect), 0, false)?;
        Ok(capture.render_textbox(clip.unwrap_or(rect), flags))
    }

    /// An extracted text page over this page.
    #[pyo3(signature = (clip = None, flags = None, matrix = None))]
    fn get_textpage(
        &self,
        py: Python<'_>,
        clip: Option<Bound<'_, PyAny>>,
        flags: Option<i32>,
        matrix: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyTextPage> {
        let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
        let matrix = matrix
            .as_ref()
            .filter(|m| !m.is_none())
            .map(parse_matrix)
            .transpose()?;
        let capture = capture_page_off_gil(py, page_bytes(self), self.compat_index(), true)?;
        Ok(PyTextPage {
            capture,
            clip,
            flags: flags.unwrap_or(0),
            matrix,
            page_no: self.compat_index(),
        })
    }

    /// An OCR-extracted text page: the digital text where the page has
    /// some (a scanned page with no text layer still needs a recogniser).
    #[pyo3(signature = (clip = None, flags = None, matrix = None, language = None, tessdata = None))]
    fn get_textpage_ocr(
        &self,
        py: Python<'_>,
        clip: Option<Bound<'_, PyAny>>,
        flags: Option<i32>,
        matrix: Option<Bound<'_, PyAny>>,
        language: Option<String>,
        tessdata: Option<String>,
    ) -> PyResult<PyTextPage> {
        let _ = (language, tessdata);
        self.get_textpage(py, clip, flags, matrix)
    }

    /// Traced spans of this page.
    fn get_texttrace(&self, py: Python<'_>) -> PyResult<PyObject> {
        let capture = capture_page_off_gil(py, page_bytes(self), self.compat_index(), false)?;
        let traces = capture.render_trace();
        let mut out = Vec::with_capacity(traces.len());
        for trace in &traces {
            out.push(trace_to_py(py, trace)?);
        }
        Ok(PyList::new(py, out)?.into_any().unbind())
    }

    /// Substring search over this page: rectangles, or quads on request.
    #[pyo3(signature = (text, *, clip = None, quads = None, flags = None, textpage = None))]
    #[allow(clippy::too_many_arguments)]
    fn search_for(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        clip: Option<Bound<'_, PyAny>>,
        quads: Option<Bound<'_, PyAny>>,
        flags: Option<i32>,
        textpage: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let needle: String = text
            .extract()
            .map_err(|_| PyTypeError::new_err("needle must be a string"))?;
        let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
        let quads = truthy_or(&quads, false)?;
        let textpage = textpage
            .as_ref()
            .filter(|t| !t.is_none())
            .map(|t| {
                t.downcast::<PyTextPage>()
                    .map_err(|_| PyTypeError::new_err("textpage must be a TextPage or None"))
                    .map(|tp| tp.borrow().clone())
            })
            .transpose()?;
        let (capture, clip, _) =
            self.resolve_capture(py, textpage, clip, flags.unwrap_or(0), false)?;
        let hits = capture.render_search(&needle, clip, quads, 0);
        search_hits_to_py(py, hits)
    }

    /// Tables found on this page; keywords are accepted and ignored.
    #[pyo3(signature = (**kwargs))]
    fn find_tables(
        &self,
        py: Python<'_>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyTableFinder> {
        let _ = kwargs;
        let owned = page_bytes(self).to_vec();
        let finder = py.allow_threads(|| {
            let engine = compat_engine(&owned)?;
            pdfluent::compat::find_page_tables(&engine, self.compat_index()).map_err(compat_err)
        })?;
        Ok(PyTableFinder::from_finder(finder))
    }

    /// Fonts used on this page: object number and base name each.
    #[pyo3(signature = (full = None))]
    fn get_fonts(&self, py: Python<'_>, full: Option<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let full = truthy_or(&full, false)?;
        let bytes = page_bytes(self).to_vec();
        let fonts = pdfluent::compat::page_fonts_from_bytes(&bytes, self.compat_index())
            .map_err(compat_err)?;
        font_rows_to_py(py, &fonts, full)
    }

    /// Fold this page's text into an extracted text page.
    #[pyo3(signature = (tpage, flags = None, matrix = None))]
    fn extend_textpage(
        &self,
        py: Python<'_>,
        tpage: &Bound<'_, PyAny>,
        flags: Option<i32>,
        matrix: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let _ = flags;
        if let Some(matrix) = matrix.as_ref().filter(|m| !m.is_none()) {
            let _ = parse_matrix(matrix)?;
        }
        let target = tpage
            .downcast::<PyTextPage>()
            .map_err(|_| PyTypeError::new_err("tpage must be a TextPage"))?;
        let capture = capture_page_off_gil(py, page_bytes(self), self.compat_index(), true)?;
        target.borrow_mut().capture.merge(&capture);
        Ok(py.None())
    }
}

/// Font records as rows: `[xref, ext, type, basefont, name, encoding]`, with
/// an embedded flag appended for the full read.
fn font_rows_to_py(
    py: Python<'_>,
    fonts: &[pdfluent::compat::FontInfo],
    full: bool,
) -> PyResult<PyObject> {
    let mut out = Vec::with_capacity(fonts.len());
    for font in fonts {
        let mut row = vec![
            font.xref.into_pyobject(py)?.into_any().unbind(),
            font.ext.clone().into_pyobject(py)?.into_any().unbind(),
            font.subtype.clone().into_pyobject(py)?.into_any().unbind(),
            font.name.clone().into_pyobject(py)?.into_any().unbind(),
            font.res_name.clone().into_pyobject(py)?.into_any().unbind(),
            font.encoding.clone().into_pyobject(py)?.into_any().unbind(),
        ];
        if full {
            let embedded = i32::from(font.ext != "n/a");
            row.push(embedded.into_pyobject(py)?.into_any().unbind());
        }
        out.push(PyList::new(py, row)?.into_any().unbind());
    }
    Ok(PyList::new(py, out)?.into_any().unbind())
}

/// Keywords shared by the block reads: clip, flags, textpage, sort.
type BlockKwargs = (Option<CompatRect>, Option<i32>, Option<PyTextPage>, bool);

fn block_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<BlockKwargs> {
    let mut clip: Option<CompatRect> = None;
    let mut flags: Option<i32> = None;
    let mut textpage: Option<PyTextPage> = None;
    let mut sort = true;
    if let Some(kwargs) = kwargs {
        for (key, value) in kwargs.iter() {
            let name: String = key.extract()?;
            match name.as_str() {
                "clip" => clip = parse_opt_rect(&value)?,
                "flags" => {
                    flags =
                        if value.is_none() {
                            None
                        } else {
                            Some(value.extract::<i32>().map_err(|_| {
                                PyTypeError::new_err("flags must be an int or None")
                            })?)
                        };
                }
                "textpage" => {
                    if !value.is_none() {
                        let tp = value
                            .downcast::<PyTextPage>()
                            .map_err(|_| {
                                PyTypeError::new_err("textpage must be a TextPage or None")
                            })?
                            .borrow()
                            .clone();
                        textpage = Some(tp);
                    }
                }
                "sort" => sort = value.is_truthy()?,
                _ => {
                    return Err(PyTypeError::new_err(format!(
                        "got an unexpected keyword argument {name:?}"
                    )));
                }
            }
        }
    }
    Ok((clip, flags, textpage, sort))
}

/// Render one capture in the requested format.
fn render_format_owned(
    capture: &CapturedPage,
    format: TextFormat,
    page: usize,
    clip: Option<CompatRect>,
    flags: i32,
    sort: bool,
) -> PyResult<TextOutput> {
    Ok(match format {
        TextFormat::Text => TextOutput::Text(capture.render_text(clip, flags, sort)),
        TextFormat::Words => TextOutput::Words(capture.render_words(clip, flags, sort, None)),
        TextFormat::Blocks => TextOutput::Blocks(capture.render_blocks(clip, flags, sort)),
        TextFormat::Dict => TextOutput::Dict(capture.render_dict(clip, flags, sort, false)),
        TextFormat::RawDict => TextOutput::Dict(capture.render_dict(clip, flags, sort, true)),
        TextFormat::Json => TextOutput::Json(capture.render_json(clip, flags, sort, false)),
        TextFormat::RawJson => TextOutput::RawJson(capture.render_json(clip, flags, sort, true)),
        TextFormat::Html => TextOutput::Html(capture.render_html(page, clip, flags, sort)),
        TextFormat::Xhtml => TextOutput::Xhtml(capture.render_xhtml(page, clip, flags, sort)),
        TextFormat::Xml => TextOutput::Xml(capture.render_xml(page, clip, flags, sort)),
        _ => return Err(crate::compat::unknown_variant("text format")),
    })
}

impl PyPage {
    /// The capture a call reads: a text page's own, or a fresh one.
    fn resolve_capture(
        &self,
        py: Python<'_>,
        textpage: Option<PyTextPage>,
        clip: Option<CompatRect>,
        flags: i32,
        with_figures: bool,
    ) -> PyResult<(CapturedPage, Option<CompatRect>, i32)> {
        let laid = self.compat_clip().map(|edges| CompatRect {
            x0: edges[0].min(edges[2]),
            y0: edges[1].min(edges[3]),
            x1: edges[0].max(edges[2]),
            y1: edges[1].max(edges[3]),
        });
        let clip = intersect(clip, laid);
        match textpage {
            Some(tp) => {
                let window = pdfluent::compat::textpage_window(
                    tp.capture.width,
                    tp.capture.height,
                    intersect(tp.clip, clip),
                    tp.matrix,
                );
                Ok((tp.capture, window, flags))
            }
            None => {
                let capture =
                    capture_page_off_gil(py, page_bytes(self), self.compat_index(), with_figures)?;
                Ok((capture, clip, flags))
            }
        }
    }
}

// ===== Binding part 3 end marker P3 =====

// ===== Lane L1: document methods =====

#[pymethods]
#[allow(non_snake_case)]
impl PyDocument {
    /// Text of one numbered page in the requested format.
    #[pyo3(signature = (pno, option = None, clip = None, flags = None, textpage = None, sort = None))]
    #[allow(clippy::too_many_arguments)]
    fn get_page_text(
        &self,
        py: Python<'_>,
        pno: i32,
        option: Option<Bound<'_, PyAny>>,
        clip: Option<Bound<'_, PyAny>>,
        flags: Option<i32>,
        textpage: Option<Bound<'_, PyAny>>,
        sort: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        if pno < 0 {
            return Err(PyIndexError::new_err(format!("page {pno} out of range")));
        }
        let format = parse_format(option.as_ref())?;
        let raw = format == TextFormat::RawDict;
        let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
        let sort = truthy_or(&sort, false)?;
        let textpage = textpage
            .as_ref()
            .filter(|t| !t.is_none())
            .map(|t| {
                t.downcast::<PyTextPage>()
                    .map_err(|_| PyTypeError::new_err("textpage must be a TextPage or None"))
                    .map(|tp| tp.borrow().clone())
            })
            .transpose()?;
        let (capture, page_no, clip, flags) = if let Some(tp) = textpage {
            (
                tp.capture,
                tp.page_no,
                intersect(tp.clip, clip),
                flags.unwrap_or(tp.flags),
            )
        } else {
            let bytes: &[u8] = self.compat_raw_bytes();
            let capture = capture_page_off_gil(py, bytes, pno as usize, needs_figures(format))?;
            (capture, pno as usize, clip, flags.unwrap_or(0))
        };
        output_to_py(
            py,
            render_format_owned(&capture, format, page_no, clip, flags, sort)?,
            raw,
        )
    }

    /// Substring search over one numbered page.
    #[pyo3(signature = (pno, text, quads = None, clip = None, flags = None, textpage = None))]
    #[allow(clippy::too_many_arguments)]
    fn search_page_for(
        &self,
        py: Python<'_>,
        pno: i32,
        text: &Bound<'_, PyAny>,
        quads: Option<Bound<'_, PyAny>>,
        clip: Option<Bound<'_, PyAny>>,
        flags: Option<i32>,
        textpage: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        if pno < 0 {
            return Err(PyIndexError::new_err(format!("page {pno} out of range")));
        }
        let needle: String = text
            .extract()
            .map_err(|_| PyTypeError::new_err("needle must be a string"))?;
        let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
        let quads = truthy_or(&quads, false)?;
        let textpage = textpage
            .as_ref()
            .filter(|t| !t.is_none())
            .map(|t| {
                t.downcast::<PyTextPage>()
                    .map_err(|_| PyTypeError::new_err("textpage must be a TextPage or None"))
                    .map(|tp| tp.borrow().clone())
            })
            .transpose()?;
        let (capture, clip) = if let Some(tp) = textpage {
            (tp.capture, intersect(tp.clip, clip))
        } else {
            let bytes: &[u8] = self.compat_raw_bytes();
            (capture_page_off_gil(py, bytes, pno as usize, false)?, clip)
        };
        let _ = flags;
        let hits = capture.render_search(&needle, clip, quads, 0);
        search_hits_to_py(py, hits)
    }

    /// Fonts used on one numbered page.
    #[pyo3(signature = (pno, full = None))]
    fn get_page_fonts(
        &self,
        py: Python<'_>,
        pno: i32,
        full: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        use crate::compat::errors::{validation_error, ValidationKind};
        let full = truthy_or(&full, false)?;
        if pno < 0 {
            return Err(validation_error(
                ValidationKind::BadXref,
                format!("page {pno} out of range"),
            ));
        }
        let fonts = pdfluent::compat::page_fonts_from_bytes(self.compat_raw_bytes(), pno as usize)
            .map_err(compat_err)?;
        font_rows_to_py(py, &fonts, full)
    }

    /// Font file of one object: name and bytes.
    #[pyo3(signature = (xref = 0, info_only = None, named = None))]
    fn extract_font(
        &self,
        py: Python<'_>,
        xref: i32,
        info_only: Option<Bound<'_, PyAny>>,
        named: Option<String>,
    ) -> PyResult<PyObject> {
        if let Some(named) = named {
            let found =
                pdfluent::compat::extract_named_font_from_bytes(self.compat_raw_bytes(), &named)
                    .map_err(compat_err)?;
            let dict = PyDict::new(py);
            match found {
                Some(data) => {
                    dict.set_item("content", PyBytes::new(py, &data.data))?;
                    dict.set_item("ext", data.ext)?;
                    dict.set_item("name", data.name)?;
                    dict.set_item("type", data.subtype)?;
                }
                None => {
                    dict.set_item("content", PyBytes::new(py, b""))?;
                    dict.set_item("ext", "")?;
                    dict.set_item("name", "")?;
                    dict.set_item("type", "")?;
                }
            }
            return Ok(dict.into_any().unbind());
        }
        let info_only = truthy_or(&info_only, false)?;
        let data =
            pdfluent::compat::extract_font_from_bytes(self.compat_raw_bytes(), xref, info_only)
                .map_err(compat_err)?;
        Ok(PyList::new(
            py,
            [
                data.name.into_pyobject(py)?.into_any().unbind(),
                data.ext.into_pyobject(py)?.into_any().unbind(),
                data.subtype.into_pyobject(py)?.into_any().unbind(),
                PyBytes::new(py, &data.data).into_any().unbind(),
            ],
        )?
        .into_any()
        .unbind())
    }

    /// Character widths of one font.
    #[pyo3(signature = (xref, limit = 256, idx = 0, fontdict = None))]
    fn get_char_widths(
        &self,
        py: Python<'_>,
        xref: i32,
        limit: i32,
        idx: i32,
        fontdict: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let fontdict = fontdict
            .as_ref()
            .filter(|f| !f.is_none())
            .map(|f| {
                f.downcast::<PyDict>()
                    .map_err(|_| PyTypeError::new_err("fontdict must be a dict or None"))?;
                py_to_value(f)
            })
            .transpose()?;
        let widths = pdfluent::compat::char_widths_from_bytes(
            self.compat_raw_bytes(),
            xref,
            limit,
            idx,
            fontdict.as_ref(),
        )
        .map_err(compat_err)?;
        Ok(PyList::new(py, widths)?.into_any().unbind())
    }

    /// Whether one object is a font dictionary.
    fn xref_is_font(&self, xref: i32) -> PyResult<bool> {
        pdfluent::compat::xref_is_font_in_bytes(self.compat_raw_bytes(), xref).map_err(compat_err)
    }

    /// Fonts of the form dictionary: resource names.
    #[getter]
    fn FormFonts(&self, py: Python<'_>) -> PyResult<PyObject> {
        let fonts =
            pdfluent::compat::form_fonts_from_bytes(self.compat_raw_bytes()).map_err(compat_err)?;
        Ok(
            PyList::new(py, fonts.iter().map(|font| font.res_name.clone()))?
                .into_any()
                .unbind(),
        )
    }
}

// ===== Binding part 4 end marker P4 =====

// ===== Lane L1: module functions =====

/// Require a dict.
fn as_dict<'a>(obj: &'a Bound<'_, PyAny>, what: &str) -> PyResult<Bound<'a, PyDict>> {
    obj.downcast::<PyDict>()
        .cloned()
        .map_err(|_| PyTypeError::new_err(format!("{what} must be a dict")))
}

/// Require a float list under a key.
fn dict_floats(dict: &Bound<'_, PyDict>, key: &str, what: &str) -> PyResult<Vec<f64>> {
    let value = dict
        .get_item(key)?
        .ok_or_else(|| PyKeyError::new_err(format!("{what} is missing {key:?}")))?;
    value
        .extract::<Vec<f64>>()
        .map_err(|_| PyTypeError::new_err(format!("{what} {key:?} must be a number list")))
}

/// A zeroed span for the quad helpers' unused span argument.
fn dummy_span() -> DictSpan {
    DictSpan {
        bbox: CompatRect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        },
        origin: CompatPoint { x: 0.0, y: 0.0 },
        font: String::new(),
        size: 0.0,
        color: 0,
        alpha: 255,
        flags: 0,
        bidi: 0,
        char_flags: 16,
        ascender: 0.0,
        descender: 0.0,
        text: String::new(),
        chars: Vec::new(),
    }
}

/// Parse a span dict leniently: present keys win, the rest default.
fn parse_span_lenient(obj: &Bound<'_, PyAny>) -> DictSpan {
    let mut span = dummy_span();
    let Ok(dict) = obj.downcast::<PyDict>() else {
        return span;
    };
    if let Ok(Some(bbox)) = dict.get_item("bbox") {
        if let Ok(v) = bbox.extract::<Vec<f64>>() {
            if v.len() == 4 {
                span.bbox = CompatRect {
                    x0: v[0],
                    y0: v[1],
                    x1: v[2],
                    y1: v[3],
                };
            }
        }
    }
    if let Ok(Some(origin)) = dict.get_item("origin") {
        if let Ok(v) = origin.extract::<Vec<f64>>() {
            if v.len() == 2 {
                span.origin = CompatPoint { x: v[0], y: v[1] };
            }
        }
    }
    if let Ok(Some(font)) = dict.get_item("font") {
        if let Ok(s) = font.extract::<String>() {
            span.font = s;
        }
    }
    if let Ok(Some(size)) = dict.get_item("size") {
        if let Ok(v) = size.extract::<f64>() {
            span.size = v;
        }
    }
    if let Ok(Some(text)) = dict.get_item("text") {
        if let Ok(s) = text.extract::<String>() {
            span.text = s;
        }
    }
    span
}

/// Parse a span dict with a required box.
fn parse_span_strict(obj: &Bound<'_, PyAny>) -> PyResult<DictSpan> {
    let dict = as_dict(obj, "span")?;
    let bbox = dict_floats(&dict, "bbox", "span")?;
    if bbox.len() != 4 {
        return Err(PyTypeError::new_err("span \"bbox\" must have 4 numbers"));
    }
    let mut span = parse_span_lenient(obj);
    span.bbox = CompatRect {
        x0: bbox[0],
        y0: bbox[1],
        x1: bbox[2],
        y1: bbox[3],
    };
    Ok(span)
}

/// Parse a character dict with a required box.
fn parse_char_strict(obj: &Bound<'_, PyAny>) -> PyResult<RawChar> {
    let dict = as_dict(obj, "char")?;
    let bbox = dict_floats(&dict, "bbox", "char")?;
    if bbox.len() != 4 {
        return Err(PyTypeError::new_err("char \"bbox\" must have 4 numbers"));
    }
    let ch = dict
        .get_item("c")?
        .and_then(|v| v.extract::<String>().ok())
        .and_then(|s| s.chars().next())
        .unwrap_or('?');
    let origin = dict
        .get_item("origin")?
        .and_then(|v| v.extract::<Vec<f64>>().ok())
        .filter(|v| v.len() == 2)
        .map(|v| CompatPoint { x: v[0], y: v[1] })
        .unwrap_or(CompatPoint { x: 0.0, y: 0.0 });
    let synthetic = dict
        .get_item("synthetic")?
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(false);
    Ok(RawChar {
        bbox: CompatRect {
            x0: bbox[0],
            y0: bbox[1],
            x1: bbox[2],
            y1: bbox[3],
        },
        ch,
        origin,
        synthetic,
    })
}

/// Parse a line dict with a required box; spans parse leniently.
fn parse_line_mixed(obj: &Bound<'_, PyAny>) -> PyResult<DictLine> {
    let dict = as_dict(obj, "line")?;
    let bbox = dict_floats(&dict, "bbox", "line")?;
    if bbox.len() != 4 {
        return Err(PyTypeError::new_err("line \"bbox\" must have 4 numbers"));
    }
    let dir = dict
        .get_item("dir")?
        .and_then(|v| v.extract::<Vec<f64>>().ok())
        .filter(|v| v.len() == 2)
        .map(|v| CompatPoint { x: v[0], y: v[1] })
        .unwrap_or(CompatPoint { x: 1.0, y: 0.0 });
    let spans = dict
        .get_item("spans")?
        .and_then(|v| {
            v.try_iter().ok().map(|it| {
                it.filter_map(|i| i.ok())
                    .map(|i| parse_span_lenient(&i))
                    .collect()
            })
        })
        .unwrap_or_default();
    Ok(DictLine {
        bbox: CompatRect {
            x0: bbox[0],
            y0: bbox[1],
            x1: bbox[2],
            y1: bbox[3],
        },
        wmode: 0,
        direction: dir,
        spans,
    })
}

/// Whole-document text in the requested string format.
#[pyfunction]
#[pyo3(
    name = "get_text",
    signature = (
        path,
        *,
        pages = None,
        method = None,
        concurrency = None,
        option = None,
        clip = None,
        flags = None,
        textpage = None,
        sort = None,
        delimiters = None,
        _stats = false,
    )
)]
#[allow(clippy::too_many_arguments)]
fn bulk_get_text(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    pages: Option<Bound<'_, PyAny>>,
    method: Option<String>,
    concurrency: Option<i32>,
    option: Option<Bound<'_, PyAny>>,
    clip: Option<Bound<'_, PyAny>>,
    flags: Option<i32>,
    textpage: Option<Bound<'_, PyAny>>,
    sort: Option<Bound<'_, PyAny>>,
    delimiters: Option<String>,
    _stats: bool,
) -> PyResult<Vec<String>> {
    let _ = (method, concurrency, textpage, delimiters);
    let path: String = path
        .extract()
        .map_err(|_| PyTypeError::new_err("path must be a string"))?;
    let bytes = std::fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PyFileNotFoundError::new_err(format!("no such file: {path:?}"))
        } else {
            PyRuntimeError::new_err(format!("cannot read {path:?}: {e}"))
        }
    })?;
    let format = parse_format(option.as_ref())?;
    if matches!(
        format,
        TextFormat::Blocks | TextFormat::Dict | TextFormat::RawDict
    ) {
        return Err(PyValueError::new_err(format!(
            "bulk text has no string form for option {format:?}"
        )));
    }
    let format = if matches!(format, TextFormat::Words) {
        TextFormat::Text
    } else {
        format
    };
    let pages = pages
        .as_ref()
        .filter(|p| !p.is_none())
        .map(parse_page_list)
        .transpose()?;
    let clip = clip.as_ref().map(parse_opt_rect).transpose()?.flatten();
    let flags = flags.unwrap_or(0);
    let sort = truthy_or(&sort, false)?;
    let owned = bytes;
    py.allow_threads(|| {
        let engine = compat_engine(&owned)?;
        let count = engine.page_count();
        let pages: Vec<usize> = match pages {
            Some(list) => {
                let mut out = Vec::with_capacity(list.len());
                for pno in list {
                    if pno < 0 || pno as usize >= count {
                        return Err(PyIndexError::new_err(format!(
                            "page {pno} out of range ({count} pages)"
                        )));
                    }
                    out.push(pno as usize);
                }
                out
            }
            None => (0..count).collect(),
        };
        let mut out = Vec::with_capacity(pages.len());
        for page in pages {
            let capture =
                pdfluent::compat::capture_page(&engine, page, false).map_err(compat_err)?;
            let mut text = String::new();
            match format {
                TextFormat::Text => text.push_str(&capture.render_text(clip, flags, sort)),
                TextFormat::Html => text.push_str(&capture.render_html(page, clip, flags, sort)),
                TextFormat::Xhtml => text.push_str(&capture.render_xhtml(page, clip, flags, sort)),
                TextFormat::Xml => text.push_str(&capture.render_xml(page, clip, flags, sort)),
                TextFormat::Json => text.push_str(&capture.render_json(clip, flags, sort, false)),
                TextFormat::RawJson => {
                    text.push_str(&capture.render_json(clip, flags, sort, true));
                }
                _ => {}
            }
            out.push(text);
        }
        Ok(out)
    })
}

/// Parse a page selection: a sequence of page numbers, nothing else.
fn parse_page_list(obj: &Bound<'_, PyAny>) -> PyResult<Vec<i32>> {
    if obj.extract::<String>().is_ok() {
        return Err(PyAssertionError::new_err(
            "pages must be a sequence of page numbers",
        ));
    }
    let mut out = Vec::new();
    for item in obj
        .try_iter()
        .map_err(|_| PyAssertionError::new_err("pages must be a sequence of page numbers"))?
    {
        out.push(
            item?
                .extract::<i32>()
                .map_err(|_| PyTypeError::new_err("page numbers must be integers"))?,
        );
    }
    Ok(out)
}

/// Typeset width of a string.
#[pyfunction]
#[pyo3(signature = (text, fontname = "helv", fontsize = 11.0, encoding = 0))]
fn get_text_length(text: String, fontname: &str, fontsize: f64, encoding: i32) -> PyResult<f64> {
    if !is_known_fontname(fontname) {
        return Err(PyValueError::new_err(format!("unknown font {fontname:?}")));
    }
    Ok(pdfluent::compat::get_text_length(
        &text, fontname, fontsize, encoding,
    ))
}

/// Whether a short or full font name resolves to a base-14 face.
fn is_known_fontname(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "helv"
            | "he"
            | "helvetica"
            | "hebo"
            | "helvetica-bold"
            | "helveticabold"
            | "heit"
            | "helvetica-oblique"
            | "helveticaoblique"
            | "hebi"
            | "helvetica-boldoblique"
            | "helvetica-bold-oblique"
            | "tiro"
            | "ti"
            | "times"
            | "times-roman"
            | "timesroman"
            | "tibo"
            | "times-bold"
            | "timesbold"
            | "tiit"
            | "times-italic"
            | "timesitalic"
            | "tibi"
            | "times-bolditalic"
            | "times-bold-italic"
            | "cour"
            | "co"
            | "courier"
            | "cobo"
            | "courier-bold"
            | "courierbold"
            | "coit"
            | "courier-oblique"
            | "courieroblique"
            | "cobi"
            | "courier-boldoblique"
            | "courier-bold-oblique"
            | "symb"
            | "symbol"
            | "zadb"
            | "zapfdingbats"
            | "zapf-dingbats"
    )
}

/// Canonical form of a character code.
#[pyfunction]
fn canon(code: &Bound<'_, PyAny>) -> PyResult<i64> {
    let code = code
        .extract::<i64>()
        .map_err(|_| PyAssertionError::new_err("canon() needs a character code"))?;
    match u32::try_from(code).ok().and_then(char::from_u32) {
        Some(ch) => Ok(pdfluent::compat::canon(ch) as i64),
        None => Ok(code),
    }
}

/// Canonical form of a string's first character: its UTF-8 length and code.
#[pyfunction]
fn chartocanon(text: &Bound<'_, PyAny>) -> PyResult<(i32, i32)> {
    let text: String = text
        .extract()
        .map_err(|_| PyAssertionError::new_err("chartocanon() needs a string"))?;
    let ch = text.chars().next().unwrap_or('\0');
    Ok((ch.len_utf8() as i32, pdfluent::compat::canon(ch) as i32))
}

/// Code point for a glyph name, or -1 when unknown.
#[pyfunction]
fn glyph_name_to_unicode(name: String) -> i32 {
    pdfluent::compat::glyph_name_to_unicode(&name)
}

/// Glyph name for a code point.
#[pyfunction]
fn unicode_to_glyph_name(code: i32) -> String {
    pdfluent::compat::unicode_to_glyph_name(code)
}

/// Empty grid cells over a rectangle: rows of cell boxes.
#[pyfunction]
#[pyo3(signature = (rect = None, cols = 1, rows = 1))]
fn make_table(
    py: Python<'_>,
    rect: Option<Bound<'_, PyAny>>,
    cols: i32,
    rows: i32,
) -> PyResult<Vec<Vec<PyObject>>> {
    if cols == 0 || rows == 0 {
        return Err(PyZeroDivisionError::new_err(
            "make_table() needs nonzero columns and rows",
        ));
    }
    let rect = match rect.as_ref().filter(|r| !r.is_none()) {
        None => CompatRect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        Some(obj) => parse_rect(obj)
            .map_err(|_| PyAssertionError::new_err("rect must be a Rect or a 4-number sequence"))?,
    };
    if rows < 0 {
        return Ok(Vec::new());
    }
    if cols < 0 {
        let mut out: Vec<Vec<PyObject>> = Vec::with_capacity(rows as usize);
        for _ in 0..rows {
            out.push(Vec::new());
        }
        return Ok(out);
    }
    let cells = pdfluent::compat::make_table(rect, cols, rows);
    let cols = cols as usize;
    let mut out = Vec::with_capacity(rows as usize);
    for row in cells.chunks(cols) {
        let mut line = Vec::with_capacity(cols);
        for cell in row {
            line.push(native_rect(py, cell.bbox)?);
        }
        out.push(line);
    }
    Ok(out)
}

/// Stylesheet for a font code.
#[pyfunction]
#[pyo3(signature = (fontcode, *, css = None, archive = None, name = None))]
fn css_for_font(
    fontcode: String,
    css: Option<String>,
    archive: Option<Bound<'_, PyAny>>,
    name: Option<String>,
) -> PyResult<String> {
    let _ = name;
    let archive = archive
        .as_ref()
        .filter(|a| !a.is_none())
        .map(py_to_value)
        .transpose()?;
    pdfluent::compat::css_for_font(&fontcode, css.as_deref(), archive, name.as_deref())
        .map_err(compat_err)
}

/// Tessdata directory: the argument, the environment, or empty.
#[pyfunction]
#[pyo3(signature = (tessdata = None))]
fn get_tessdata(tessdata: Option<String>) -> PyResult<String> {
    pdfluent::compat::get_tessdata(tessdata.as_deref()).map_err(compat_err)
}

/// Highlight fold over one character: refused without highlight state.
#[pyfunction]
fn on_highlight_char(
    hits: &Bound<'_, PyAny>,
    line: &Bound<'_, PyAny>,
    ch: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let _ = (hits, line, ch);
    Err(PyAssertionError::new_err(
        "on_highlight_char() needs highlight state this build does not keep",
    ))
}

/// Quad of a box in reading direction.
#[pyfunction]
fn recover_bbox_quad(
    py: Python<'_>,
    line_dir: &Bound<'_, PyAny>,
    span: &Bound<'_, PyAny>,
    bbox: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let dir = parse_direction(line_dir)?;
    let boxed = span
        .downcast::<PyDict>()
        .ok()
        .and_then(|dict| dict.get_item("bbox").ok().flatten())
        .is_some();
    if !boxed {
        return Err(PyKeyError::new_err("span is missing \"bbox\""));
    }
    let span = parse_span_lenient(span);
    let bbox = parse_rect(bbox)?;
    native_quad(py, pdfluent::compat::recover_bbox_quad(dir, &span, bbox))
}

/// Quad of a raw character in reading direction.
#[pyfunction]
fn recover_char_quad(
    py: Python<'_>,
    line_dir: &Bound<'_, PyAny>,
    span: &Bound<'_, PyAny>,
    char: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let dir = parse_direction(line_dir)?;
    let span = parse_span_lenient(span);
    let ch = parse_char_strict(char)?;
    native_quad(py, pdfluent::compat::recover_char_quad(dir, &span, &ch))
}

/// Quad covering a line.
#[pyfunction]
#[pyo3(signature = (line, spans = None))]
fn recover_line_quad(
    py: Python<'_>,
    line: &Bound<'_, PyAny>,
    spans: Option<Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let line = parse_line_mixed(line)?;
    let spans = spans
        .as_ref()
        .filter(|s| !s.is_none())
        .map(|s| {
            s.try_iter()
                .map_err(|_| PyTypeError::new_err("spans must be a sequence or None"))?
                .map(|item| item.and_then(|i| parse_span_strict(&i)))
                .collect::<PyResult<Vec<DictSpan>>>()
        })
        .transpose()?;
    native_quad(
        py,
        pdfluent::compat::recover_line_quad(&line, spans.as_deref()),
    )
}

/// Quad of a span box in reading direction.
#[pyfunction]
fn recover_quad(
    py: Python<'_>,
    line_dir: &Bound<'_, PyAny>,
    span: &Bound<'_, PyAny>,
) -> PyResult<PyObject> {
    let dir = parse_direction(line_dir)?;
    let span = parse_span_strict(span)?;
    native_quad(py, pdfluent::compat::recover_quad(dir, &span))
}

/// Quad covering span characters.
#[pyfunction]
#[pyo3(signature = (line_dir, span, chars = None))]
fn recover_span_quad(
    py: Python<'_>,
    line_dir: &Bound<'_, PyAny>,
    span: &Bound<'_, PyAny>,
    chars: Option<Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let dir = parse_direction(line_dir)?;
    let span = parse_span_strict(span)?;
    let chars = chars
        .as_ref()
        .filter(|c| !c.is_none())
        .map(|c| {
            c.try_iter()
                .map_err(|_| PyTypeError::new_err("chars must be a sequence or None"))?
                .map(|item| item.and_then(|i| parse_char_strict(&i)))
                .collect::<PyResult<Vec<RawChar>>>()
        })
        .transpose()?;
    native_quad(
        py,
        pdfluent::compat::recover_span_quad(dir, &span, chars.as_deref()),
    )
}

/// Probe a font on a page: name and object number, or nothing.
#[pyfunction]
#[pyo3(name = "CheckFont")]
fn check_font(page: &Bound<'_, PyAny>, fontname: String) -> PyResult<PyObject> {
    let page = page
        .downcast::<PyPage>()
        .map_err(|_| PyAttributeError::new_err("page must be a Page"))?;
    let page = page.borrow();
    let found =
        pdfluent::compat::check_font_in_bytes(page_bytes(&page), page.compat_index(), &fontname)
            .map_err(compat_err)?;
    Python::with_gil(|py| match found {
        CompatValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in &items {
                out.push(value_to_py(py, item)?);
            }
            Ok(pyo3::types::PyTuple::new(py, out)?.into_any().unbind())
        }
        CompatValue::Null => Ok(py.None()),
        other => value_to_py(py, &other),
    })
}

/// Probe one object's font identity: base name and subtype, or nothing.
#[pyfunction]
#[pyo3(name = "CheckFontInfo")]
fn check_font_info(py: Python<'_>, doc: &Bound<'_, PyAny>, xref: i32) -> PyResult<PyObject> {
    let doc = doc
        .downcast::<PyDocument>()
        .map_err(|_| PyAttributeError::new_err("doc must be a Document"))?;
    let info = pdfluent::compat::check_font_info_in_bytes(doc.borrow().compat_raw_bytes(), xref)
        .map_err(compat_err)?;
    value_to_py(py, &info)
}

/// Store font info on a document.
#[pyfunction]
#[pyo3(name = "UpdateFontInfo")]
fn update_font_info(doc: &Bound<'_, PyAny>, info: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    let held = doc
        .downcast::<PyDocument>()
        .map_err(|_| PyAttributeError::new_err("doc must be a Document"))?;
    if !info.is_instance_of::<PyList>() && !info.is_instance_of::<PyTuple>() {
        return Err(PyTypeError::new_err("info must be a sequence"));
    }
    if info.len()? == 0 {
        return Err(PyIndexError::new_err(
            "info must hold at least one font record",
        ));
    }
    let mut records = Vec::new();
    for row in info.try_iter()? {
        records.push(super::edit::value_from_py(&row?)?);
    }
    super::edit::mutate_doc(&mut held.borrow_mut(), |facade| {
        pdfluent::compat::update_font_info(facade, &records)
    })?;
    Ok(doc.py().None())
}

/// Repair a monospace font on a page.
#[pyfunction]
fn repair_mono_font(page: &Bound<'_, PyAny>, font: PyObject) -> PyResult<PyObject> {
    let held = page
        .downcast::<PyPage>()
        .map_err(|_| PyTypeError::new_err("page must be a Page"))?;
    let py = page.py();
    let font = font
        .bind(py)
        .downcast::<PyFont>()
        .map_err(|_| PyAttributeError::new_err("font must be a Font"))?;
    super::edit::repair_mono_font_on(&mut held.borrow_mut(), &font.borrow())?;
    Ok(py.None())
}

/// One text-marker quad: a Quad passes through, corners or a box convert.
fn coerce_text_marker_quad(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    if obj.hasattr("ul").unwrap_or(false)
        && obj.hasattr("ur").unwrap_or(false)
        && obj.hasattr("ll").unwrap_or(false)
        && obj.hasattr("lr").unwrap_or(false)
    {
        return Ok(obj.clone().unbind());
    }
    if let Ok(nums) = obj.extract::<Vec<f64>>() {
        if nums.len() == 8 {
            return native_quad(
                py,
                CompatQuad {
                    ul: CompatPoint {
                        x: nums[0],
                        y: nums[1],
                    },
                    ur: CompatPoint {
                        x: nums[2],
                        y: nums[3],
                    },
                    ll: CompatPoint {
                        x: nums[4],
                        y: nums[5],
                    },
                    lr: CompatPoint {
                        x: nums[6],
                        y: nums[7],
                    },
                },
            );
        }
    }
    if let Ok(rect) = parse_rect(obj) {
        return native_quad(
            py,
            CompatQuad {
                ul: CompatPoint {
                    x: rect.x0,
                    y: rect.y0,
                },
                ur: CompatPoint {
                    x: rect.x1,
                    y: rect.y0,
                },
                ll: CompatPoint {
                    x: rect.x0,
                    y: rect.y1,
                },
                lr: CompatPoint {
                    x: rect.x1,
                    y: rect.y1,
                },
            },
        );
    }
    Err(PyTypeError::new_err("quads must read as quads"))
}

/// Mark quads with a text annotation: the quad markups, else nothing.
#[pyfunction]
#[pyo3(name = "Page__add_text_marker")]
fn page_add_text_marker(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    quads: &Bound<'_, PyAny>,
    annot_type: i32,
) -> PyResult<PyObject> {
    this.downcast::<PyPage>()
        .map_err(|_| PyAttributeError::new_err("self must be a Page"))?;
    let factory = match annot_type {
        8 => "add_highlight_annot",
        9 => "add_underline_annot",
        10 => "add_squiggly_annot",
        11 => "add_strikeout_annot",
        _ => return Ok(py.None()),
    };
    let items: Vec<Bound<'_, PyAny>> = quads
        .try_iter()
        .map_err(|_| PyTypeError::new_err("quads must be a sequence"))?
        .collect::<PyResult<_>>()?;
    let mut converted = Vec::with_capacity(items.len());
    for item in &items {
        converted.push(coerce_text_marker_quad(py, item)?);
    }
    Ok(this.call_method1(factory, (converted,))?.unbind())
}

/// Stylesheet for a bundled font code: refused without an archive.
#[pyfunction]
fn css_for_pymupdf_font(fontcode: &Bound<'_, PyAny>) -> PyResult<String> {
    let _ = fontcode;
    Err(PyValueError::new_err(
        "not supported in this version: css_for_pymupdf_font; see KNOWN_DIFFERENCES",
    ))
}

/// Register the L1 lane's new module-level names.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTextPage>()?;
    m.add_class::<PyTableFinder>()?;
    m.add_class::<PyTable>()?;
    m.add_class::<PyTableCell>()?;
    m.add_function(pyo3::wrap_pyfunction!(bulk_get_text, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_text_length, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(canon, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(chartocanon, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(glyph_name_to_unicode, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(unicode_to_glyph_name, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(make_table, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(css_for_font, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(get_tessdata, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(on_highlight_char, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(recover_bbox_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(recover_char_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(recover_line_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(recover_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(recover_span_quad, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_font, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(check_font_info, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(update_font_info, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(repair_mono_font, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(page_add_text_marker, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(css_for_pymupdf_font, m)?)?;
    m.add("TEXT_DEHYPHENATE", TEXT_DEHYPHENATE)?;
    m.add("TEXT_INHIBIT_SPACES", TEXT_INHIBIT_SPACES)?;
    m.add("TEXTFLAGS_TEXT", TEXTFLAGS_TEXT)?;
    m.add("TEXTFLAGS_SEARCH", TEXTFLAGS_SEARCH)?;
    Ok(())
}

// ===== Binding part 5 end marker P5 =====
