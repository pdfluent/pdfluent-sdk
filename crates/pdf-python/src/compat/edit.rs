//! Compatible page editing (lane L4): `Page`/`Document` mutation methods.
//!
//! Binds every L4 member of `specs/api/*.yaml` onto the existing
//! `Document`/`Page` classes (extra `#[pymethods]` blocks) plus the new
//! `Annot`, `Widget`, `Shape`, `Font` and `TextWriter` classes. Geometric
//! arguments are duck-typed so any object with the `Point`/`Rect`/`Quad`
//! attributes (or a plain sequence) works; the geometric classes below are
//! structural placeholders whose full surface belongs to the geometry lane.
//! `Pixmap`/`TextPage` are likewise placeholders for lanes L3/L1.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use lopdf::Document as LopdfDocument;
use pdf_engine::PdfDocument as EngineDoc;
use pdfluent::compat::constants::{
    PDF_ANNOT_INK, PDF_WIDGET_TYPE_CHECKBOX, PDF_WIDGET_TYPE_RADIOBUTTON,
};
use pdfluent::compat::document::{
    BakeOptions, DocMetadata, EmbFileOptions, EmbItem, InsertPageOptions, InsertPdfOptions,
    LayerOptions, Link as CompatLink, OcmdOptions, PageLabel, PageSelection, ScrubOptions, TocDest,
    TocEntry, TocItemOptions,
};
use pdfluent::compat::edit::{
    Annot as FacadeAnnot, AnnotColors, AnnotEntry, AnnotIdent, AnnotInfo, AnnotInfoOptions,
    AnnotType, AnnotUpdateOptions, AppendOptions, Border, BorderOptions, CdrawingsOptions,
    ClusterOptions, DrawOptions, Drawing, FileAnnotOptions, FillTextBoxOptions, FlushOptions,
    Font as FacadeFont, FontMeasureOptions, FontOptions, FreeTextOptions, HighlightOptions,
    HtmlBoxOptions, ImageInsertOptions, InsertFontOptions, InsertTextOptions, RedactAnnotOptions,
    RedactApplyOptions, ReplaceImageOptions, Shape as FacadeShape, ShapeFinishOptions,
    ShowPageOptions, TextBoxOptions, TextWriter as FacadeTextWriter, TextWriterOptions,
    Widget as FacadeWidget, WriteTextOptions as FacadeWriteTextOptions,
};
use pdfluent::compat::geometry::{
    Matrix as GMatrix, Point as GPoint, Quad as GQuad, Rect as GRect,
};
use pdfluent::compat::pixmap::{Pixmap as CompatPixmap, RenderOptions as CompatRenderOptions};
use pdfluent::compat::text::{
    TextArg, TextFormat, TextOptions, TextOutput, TextPage as CompatTextPage, TextWord,
};
use pdfluent::compat::{
    ErrorKind as CompatKind, Page as FacadePage, PageMut as FacadePageMut, Value as FacadeValue,
};
use pdfluent::document::PdfDocument as FacadeDoc;
use pyo3::exceptions::{PyAttributeError, PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pyclass::CompareOp as PyCompareOp;
use pyo3::types::{PyBool, PyBytes, PyDict};

use crate::compat::document::{PyCompatMatrix, PyLink};
use crate::compat::errors::{
    facade_error as compat_err_to_py, member_error, validation_error, FzErrorArgument, Member,
    ValidationKind,
};
use crate::{engine_err_to_py, PdfluentError, PyDocument, PyPage};

fn facade_opt_seq(value: &Bound<'_, PyAny>) -> PyResult<Option<Vec<f64>>> {
    if value.is_none() {
        return Ok(None);
    }
    let mut out = Vec::new();
    for item in value.try_iter()? {
        let item = item?;
        let number: f64 = item
            .extract()
            .map_err(|_| PyTypeError::new_err("colour components must be numbers".to_string()))?;
        out.push(number);
    }
    Ok(Some(out))
}

#[allow(dead_code)]
fn facade_seq(value: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    facade_opt_seq(value).map(|o| o.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Geometric placeholders (structural: constructors, attributes, equality)
// ---------------------------------------------------------------------------

/// A 2D point: `Point(x, y)`.
#[pyclass(name = "Point")]
#[derive(Debug, Clone, Copy)]
pub struct PyPoint {
    inner: GPoint,
}

#[pymethods]
impl PyPoint {
    #[new]
    #[pyo3(signature = (x, y=None))]
    fn new(x: PyObject, y: Option<f64>) -> PyResult<Self> {
        Python::with_gil(|py| {
            let bound = x.into_bound(py);
            if let Ok(pair) = bound.extract::<(f64, f64)>() {
                return Ok(Self {
                    inner: GPoint {
                        x: pair.0,
                        y: pair.1,
                    },
                });
            }
            let first: f64 = bound.extract().map_err(|_| {
                PyTypeError::new_err("point coordinates must be numbers".to_string())
            })?;
            let second =
                y.ok_or_else(|| PyTypeError::new_err("point needs x and y".to_string()))?;
            Ok(Self {
                inner: GPoint {
                    x: first,
                    y: second,
                },
            })
        })
    }

    #[getter]
    fn x(&self) -> f64 {
        self.inner.x
    }

    #[getter]
    fn y(&self) -> f64 {
        self.inner.y
    }

    fn __repr__(&self) -> String {
        format!("Point({}, {})", self.inner.x, self.inner.y)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let point = point_from_py(other)?;
        Ok(self.inner.x == point.x && self.inner.y == point.y)
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }
}

impl PyPoint {
    fn wrap(inner: GPoint) -> Self {
        Self { inner }
    }
}

/// An axis-aligned rectangle: `Rect(x0, y0, x1, y1)`.
#[pyclass(name = "Rect")]
#[derive(Debug, Clone, Copy)]
pub struct PyRect {
    inner: GRect,
}

#[pymethods]
impl PyRect {
    #[new]
    #[pyo3(signature = (x0, y0=None, x1=None, y1=None))]
    fn new(x0: PyObject, y0: Option<f64>, x1: Option<f64>, y1: Option<f64>) -> PyResult<Self> {
        Python::with_gil(|py| {
            let bound = x0.into_bound(py);
            if let Ok(quad) = bound.extract::<(f64, f64, f64, f64)>() {
                return Ok(Self {
                    inner: GRect {
                        x0: quad.0,
                        y0: quad.1,
                        x1: quad.2,
                        y1: quad.3,
                    },
                });
            }
            let first: f64 = bound.extract().map_err(|_| {
                PyTypeError::new_err("rect coordinates must be numbers".to_string())
            })?;
            let (y0, x1, y1) = match (y0, x1, y1) {
                (Some(y0), Some(x1), Some(y1)) => (y0, x1, y1),
                _ => {
                    return Err(PyTypeError::new_err(
                        "rect needs x0, y0, x1 and y1".to_string(),
                    ));
                }
            };
            Ok(Self {
                inner: GRect {
                    x0: first,
                    y0,
                    x1,
                    y1,
                },
            })
        })
    }

    #[getter]
    fn x0(&self) -> f64 {
        self.inner.x0
    }

    #[getter]
    fn y0(&self) -> f64 {
        self.inner.y0
    }

    #[getter]
    fn x1(&self) -> f64 {
        self.inner.x1
    }

    #[getter]
    fn y1(&self) -> f64 {
        self.inner.y1
    }

    fn __repr__(&self) -> String {
        format!(
            "Rect({}, {}, {}, {})",
            self.inner.x0, self.inner.y0, self.inner.x1, self.inner.y1
        )
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let rect = rect_from_py(other)?;
        Ok(self.inner.x0 == rect.x0
            && self.inner.y0 == rect.y0
            && self.inner.x1 == rect.x1
            && self.inner.y1 == rect.y1)
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }
}

impl PyRect {
    fn wrap(inner: GRect) -> Self {
        Self { inner }
    }
}

/// A quadrilateral: `Quad(ul, ur, ll, lr)` of points.
#[pyclass(name = "Quad")]
#[derive(Debug, Clone, Copy)]
pub struct PyQuad {
    inner: GQuad,
}

#[pymethods]
impl PyQuad {
    #[new]
    fn new(
        ul: &Bound<'_, PyAny>,
        ur: &Bound<'_, PyAny>,
        ll: &Bound<'_, PyAny>,
        lr: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: GQuad {
                ul: point_from_py(ul)?,
                ur: point_from_py(ur)?,
                ll: point_from_py(ll)?,
                lr: point_from_py(lr)?,
            },
        })
    }

    #[getter]
    fn ul(&self) -> PyPoint {
        PyPoint::wrap(self.inner.ul)
    }

    #[getter]
    fn ur(&self) -> PyPoint {
        PyPoint::wrap(self.inner.ur)
    }

    #[getter]
    fn ll(&self) -> PyPoint {
        PyPoint::wrap(self.inner.ll)
    }

    #[getter]
    fn lr(&self) -> PyPoint {
        PyPoint::wrap(self.inner.lr)
    }

    fn __repr__(&self) -> String {
        format!(
            "Quad({}, {}, {}, {})",
            self.ul().__repr__(),
            self.ur().__repr__(),
            self.ll().__repr__(),
            self.lr().__repr__()
        )
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let quad = quad_from_py(other)?;
        let same = |a: GPoint, b: GPoint| a.x == b.x && a.y == b.y;
        Ok(same(self.inner.ul, quad.ul)
            && same(self.inner.ur, quad.ur)
            && same(self.inner.ll, quad.ll)
            && same(self.inner.lr, quad.lr))
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }
}

impl PyQuad {
    #[allow(dead_code)]
    fn wrap(inner: GQuad) -> Self {
        Self { inner }
    }
}

/// An affine matrix: `Matrix(a, b, c, d, e, f)`.
#[pyclass(name = "Matrix")]
#[derive(Debug, Clone, Copy)]
pub struct PyMatrix {
    inner: GMatrix,
}

#[pymethods]
impl PyMatrix {
    #[new]
    #[pyo3(signature = (a=1.0, b=0.0, c=0.0, d=1.0, e=0.0, f=0.0))]
    fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self {
            inner: GMatrix { a, b, c, d, e, f },
        }
    }

    #[getter]
    fn a(&self) -> f64 {
        self.inner.a
    }

    #[getter]
    fn b(&self) -> f64 {
        self.inner.b
    }

    #[getter]
    fn c(&self) -> f64 {
        self.inner.c
    }

    #[getter]
    fn d(&self) -> f64 {
        self.inner.d
    }

    #[getter]
    fn e(&self) -> f64 {
        self.inner.e
    }

    #[getter]
    fn f(&self) -> f64 {
        self.inner.f
    }

    fn __repr__(&self) -> String {
        format!(
            "Matrix({}, {}, {}, {}, {}, {})",
            self.inner.a, self.inner.b, self.inner.c, self.inner.d, self.inner.e, self.inner.f
        )
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let matrix = matrix_from_py(other)?;
        Ok(self.inner.a == matrix.a
            && self.inner.b == matrix.b
            && self.inner.c == matrix.c
            && self.inner.d == matrix.d
            && self.inner.e == matrix.e
            && self.inner.f == matrix.f)
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }
}

impl PyMatrix {
    fn wrap(inner: GMatrix) -> Self {
        Self { inner }
    }
}

fn number_attr(value: &Bound<'_, PyAny>, name: &str) -> PyResult<f64> {
    value
        .getattr(name)
        .map_err(|_| PyTypeError::new_err(format!("point_like needs .{name}")))?
        .extract()
        .map_err(|_| PyTypeError::new_err(format!(".{name} must be a number")))
}

/// Any point-like object: attributes, a pair, or our own `Point`.
fn point_from_py(value: &Bound<'_, PyAny>) -> PyResult<GPoint> {
    if let Ok(point) = value.extract::<PyRef<'_, PyPoint>>() {
        return Ok(point.inner);
    }
    if let Ok(pair) = value.extract::<(f64, f64)>() {
        return Ok(GPoint {
            x: pair.0,
            y: pair.1,
        });
    }
    if value.hasattr("x")? && value.hasattr("y")? && !value.hasattr("x0")? {
        return Ok(GPoint {
            x: number_attr(value, "x")?,
            y: number_attr(value, "y")?,
        });
    }
    Err(PyTypeError::new_err(
        "point_like must be a Point or an (x, y) pair".to_string(),
    ))
}

/// Any rect-like object: attributes, a 4-tuple, or our own `Rect`.
fn rect_from_py(value: &Bound<'_, PyAny>) -> PyResult<GRect> {
    if let Ok(rect) = value.extract::<PyRef<'_, PyRect>>() {
        return Ok(rect.inner);
    }
    if let Ok(quad) = value.extract::<(f64, f64, f64, f64)>() {
        return Ok(GRect {
            x0: quad.0,
            y0: quad.1,
            x1: quad.2,
            y1: quad.3,
        });
    }
    for name in ["x0", "y0", "x1", "y1"] {
        if !value.hasattr(name)? {
            return Err(PyTypeError::new_err(
                "rect_like must be a Rect or an (x0, y0, x1, y1) tuple".to_string(),
            ));
        }
    }
    Ok(GRect {
        x0: number_attr(value, "x0")?,
        y0: number_attr(value, "y0")?,
        x1: number_attr(value, "x1")?,
        y1: number_attr(value, "y1")?,
    })
}

/// Any quad-like object: corner attributes or our own `Quad`.
fn quad_from_py(value: &Bound<'_, PyAny>) -> PyResult<GQuad> {
    if let Ok(quad) = value.extract::<PyRef<'_, PyQuad>>() {
        return Ok(quad.inner);
    }
    for name in ["ul", "ur", "ll", "lr"] {
        if !value.hasattr(name)? {
            return Err(PyTypeError::new_err("quad_like must be a Quad".to_string()));
        }
    }
    Ok(GQuad {
        ul: point_from_py(&value.getattr("ul")?)?,
        ur: point_from_py(&value.getattr("ur")?)?,
        ll: point_from_py(&value.getattr("ll")?)?,
        lr: point_from_py(&value.getattr("lr")?)?,
    })
}

/// Any matrix-like object: attributes, a 6-tuple, or our own `Matrix`.
fn matrix_from_py(value: &Bound<'_, PyAny>) -> PyResult<GMatrix> {
    if let Ok(matrix) = value.extract::<PyRef<'_, PyMatrix>>() {
        return Ok(matrix.inner);
    }
    if let Ok(cells) = value.extract::<(f64, f64, f64, f64, f64, f64)>() {
        return Ok(GMatrix {
            a: cells.0,
            b: cells.1,
            c: cells.2,
            d: cells.3,
            e: cells.4,
            f: cells.5,
        });
    }
    for name in ["a", "b", "c", "d", "e", "f"] {
        if !value.hasattr(name)? {
            return Err(PyTypeError::new_err(
                "matrix_like must be a Matrix or an (a, b, c, d, e, f) tuple".to_string(),
            ));
        }
    }
    Ok(GMatrix {
        a: number_attr(value, "a")?,
        b: number_attr(value, "b")?,
        c: number_attr(value, "c")?,
        d: number_attr(value, "d")?,
        e: number_attr(value, "e")?,
        f: number_attr(value, "f")?,
    })
}

#[allow(dead_code)]
fn opt_point(value: &Bound<'_, PyAny>) -> PyResult<Option<GPoint>> {
    if value.is_none() {
        return Ok(None);
    }
    point_from_py(value).map(Some)
}

fn opt_rect(value: &Bound<'_, PyAny>) -> PyResult<Option<GRect>> {
    if value.is_none() {
        return Ok(None);
    }
    rect_from_py(value).map(Some)
}

fn opt_lift<T>(
    value: Option<&Bound<'_, PyAny>>,
    convert: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<Option<T>>,
) -> PyResult<Option<T>> {
    match value {
        Some(found) => convert(found),
        None => Ok(None),
    }
}

fn opt_matrix(value: &Bound<'_, PyAny>) -> PyResult<Option<GMatrix>> {
    if value.is_none() {
        return Ok(None);
    }
    matrix_from_py(value).map(Some)
}

#[allow(dead_code)]
fn text_arg_from_py(value: &Bound<'_, PyAny>) -> PyResult<TextArg> {
    if let Ok(one) = value.extract::<String>() {
        return Ok(TextArg::One(one));
    }
    if let Ok(many) = value.extract::<Vec<String>>() {
        return Ok(TextArg::Many(many));
    }
    Err(PyTypeError::new_err(
        "text must be str or a list of str".to_string(),
    ))
}

fn bytes_from_py(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(raw) = value.extract::<Vec<u8>>() {
        return Ok(raw);
    }
    Err(PyTypeError::new_err("buffer must be bytes".to_string()))
}

/// Point in a draw call: the recordings pin `ValueError`, not `TypeError`.
fn draw_point(value: &Bound<'_, PyAny>, what: &str) -> PyResult<GPoint> {
    point_from_py(value).map_err(|_| validation_error(ValidationKind::DrawArg, what))
}

/// Rect in a draw call: the recordings pin `ValueError`, not `TypeError`.
fn draw_rect(value: &Bound<'_, PyAny>, what: &str) -> PyResult<GRect> {
    rect_from_py(value).map_err(|_| validation_error(ValidationKind::DrawArg, what))
}

/// Quad in a draw call: the recordings pin `ValueError`, not `TypeError`.
fn draw_quad(value: &Bound<'_, PyAny>, what: &str) -> PyResult<GQuad> {
    quad_from_py(value).map_err(|_| validation_error(ValidationKind::DrawArg, what))
}

/// Point list in a draw call: the recordings pin `ValueError`.
fn draw_points(value: &Bound<'_, PyAny>, what: &str) -> PyResult<Vec<GPoint>> {
    points_from_py(value).map_err(|_| validation_error(ValidationKind::DrawArg, what))
}

/// Box where a page or annotation box is set: `AssertionError` is recorded.
fn set_box(value: &Bound<'_, PyAny>, what: &str) -> PyResult<GRect> {
    rect_from_py(value).map_err(|_| validation_error(ValidationKind::SetBox, what))
}

/// Text where a string is required: `AttributeError` is recorded.
fn text_param(value: &Bound<'_, PyAny>, what: &str) -> PyResult<String> {
    value
        .extract::<String>()
        .map_err(|_| validation_error(ValidationKind::TextArg, what))
}

/// Flag where a boolean is required: `ValueError` is recorded for `bake`.
fn draw_flag(value: &Bound<'_, PyAny>, what: &str) -> PyResult<bool> {
    if let Ok(found) = value.extract::<bool>() {
        return Ok(found);
    }
    if let Ok(found) = value.extract::<i32>() {
        return Ok(found != 0);
    }
    Err(validation_error(ValidationKind::DrawArg, what))
}

/// Any pixmap: this file's placeholder or the pixmap lane's `Pixmap`.
fn pixmap_from_py(value: &Bound<'_, PyAny>) -> PyResult<CompatPixmap> {
    if let Ok(own) = value.extract::<PyRef<'_, PyPixmap>>() {
        return Ok(own.inner.clone());
    }
    crate::compat::pixmap::coerce_pixmap(value)
}

// ---------------------------------------------------------------------------
// Mutation channel: facade open/mutate/save over the shared slot
// ---------------------------------------------------------------------------

type Slot = Arc<Mutex<Option<LopdfDocument>>>;

fn open_facade(bytes: &[u8]) -> PyResult<FacadeDoc> {
    FacadeDoc::from_bytes(bytes).map_err(compat_err_to_py)
}

#[allow(dead_code)]
fn refresh_engine(bytes: &[u8]) -> PyResult<Arc<EngineDoc>> {
    let shared = Arc::new(bytes.to_vec());
    EngineDoc::open(shared)
        .map_err(engine_err_to_py)
        .map(Arc::new)
}

fn slot_bytes(slot: &Slot, fallback: &[u8]) -> PyResult<Vec<u8>> {
    if let Some(doc) = slot.lock().unwrap().as_mut() {
        let mut buf = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| PdfluentError::new_err(format!("serialise failed: {e}")))?;
        return Ok(buf);
    }
    Ok(fallback.to_vec())
}

fn store_slot(slot: &Slot, saved: &[u8]) -> PyResult<()> {
    let parsed = LopdfDocument::load_mem(saved)
        .map_err(|e| PdfluentError::new_err(format!("serialise failed: {e}")))?;
    *slot.lock().unwrap() = Some(parsed);
    Ok(())
}

/// Read through the document facade without mutating.
#[allow(dead_code)]
fn read_doc<T>(
    doc: &PyDocument,
    run: impl FnOnce(&FacadeDoc) -> Result<T, pdfluent::Error>,
) -> PyResult<T> {
    let bytes = doc.current_bytes()?;
    let facade = open_facade(&bytes)?;
    run(&facade).map_err(compat_err_to_py)
}

/// Facade errors with range failures translated to the recorded `ValueError`.
///
/// The recorded surface raises `ValueError` (not `IndexError`) for the
/// page-edit calls; other calls keep the shared mapping.
fn range_as_value(error: pdfluent::Error) -> PyErr {
    if matches!(error.compat_kind(), Some(CompatKind::IndexOutOfRange)) {
        return PyValueError::new_err(error.to_string());
    }
    compat_err_to_py(error)
}

/// Mutate through the document facade, then publish the bytes.
pub(crate) fn mutate_doc<T>(
    doc: &mut PyDocument,
    run: impl FnOnce(&mut FacadeDoc) -> Result<T, pdfluent::Error>,
) -> PyResult<T> {
    mutate_doc_with(doc, run, compat_err_to_py)
}

/// Mutate through the document facade with a custom error mapping, then
/// publish the bytes.
fn mutate_doc_with<T>(
    doc: &mut PyDocument,
    run: impl FnOnce(&mut FacadeDoc) -> Result<T, pdfluent::Error>,
    map_err: fn(pdfluent::Error) -> PyErr,
) -> PyResult<T> {
    let bytes = doc.current_bytes()?;
    let mut facade = open_facade(&bytes)?;
    let out = run(&mut facade).map_err(map_err)?;
    let saved = facade.to_bytes().map_err(map_err)?;
    publish_bytes(doc, saved)?;
    Ok(out)
}

/// Publish document bytes through the slots: the parsed lopdf copy, the
/// shared raw bytes, and a reopened engine.
fn publish_bytes(doc: &mut PyDocument, saved: Vec<u8>) -> PyResult<()> {
    store_slot(&doc.lopdf, &saved)?;
    let shared = Arc::new(saved);
    let engine = Arc::new(EngineDoc::open(Arc::clone(&shared)).map_err(engine_err_to_py)?);
    *doc.inner.lock().unwrap() = Some(engine);
    doc.raw_bytes = shared;
    doc.mark_dirty();
    Ok(())
}

#[allow(dead_code)]
fn page_bytes(page: &PyPage) -> PyResult<Vec<u8>> {
    slot_bytes(&page.lopdf, page.snapshot.as_ref())
}

/// Read through the page facade without mutating.
#[allow(dead_code)]
fn read_page<T>(
    page: &PyPage,
    run: impl FnOnce(&FacadeDoc) -> Result<T, pdfluent::Error>,
) -> PyResult<T> {
    let bytes = page_bytes(page)?;
    let facade = open_facade(&bytes)?;
    run(&facade).map_err(compat_err_to_py)
}

/// Mutate through the page facade, then publish the bytes.
#[allow(dead_code)]
fn mutate_page<T>(
    page: &mut PyPage,
    run: impl FnOnce(&mut FacadeDoc) -> Result<T, pdfluent::Error>,
) -> PyResult<T> {
    mutate_page_with(page, run, compat_err_to_py)
}

/// Mutate through the page facade with a custom error mapping, then
/// publish the bytes.
fn mutate_page_with<T>(
    page: &mut PyPage,
    run: impl FnOnce(&mut FacadeDoc) -> Result<T, pdfluent::Error>,
    map_err: fn(pdfluent::Error) -> PyErr,
) -> PyResult<T> {
    let bytes = page_bytes(page)?;
    let mut facade = open_facade(&bytes)?;
    let out = run(&mut facade).map_err(map_err)?;
    let saved = facade.to_bytes().map_err(map_err)?;
    store_slot(&page.lopdf, &saved)?;
    page.doc = refresh_engine(&saved)?;
    page.snapshot = Arc::new(saved);
    Ok(out)
}

fn facade_view<'a>(facade: &'a mut FacadeDoc, index: usize) -> FacadePageMut<'a> {
    FacadePageMut::new(facade, index)
}

/// Repair a monospace font on a page: module-function backing. A font
/// the page does not use has nothing to repair.
pub(crate) fn repair_mono_font_on(page: &mut PyPage, font: &PyFont) -> PyResult<()> {
    let index = page.index;
    mutate_page(page, |facade| {
        let wanted = font.inner.name()?;
        let fonts = FacadePage::new(facade, index).get_fonts(false)?;
        let Some(found) = fonts
            .iter()
            .find(|info| font_names_match(&info.name, &wanted))
        else {
            return Ok(());
        };
        let mut view = facade_view(facade, index);
        pdfluent::compat::repair_mono_font(&mut view, found)
    })
}

/// Whether a page font record answers to a font name: subset prefixes
/// aside, compared without case.
fn font_names_match(page: &str, wanted: &str) -> bool {
    let bare = match page.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => page,
    };
    bare.eq_ignore_ascii_case(wanted)
}

fn text_format_from_py(name: &str) -> PyResult<TextFormat> {
    match name {
        "text" => Ok(TextFormat::Text),
        "words" => Ok(TextFormat::Words),
        "blocks" => Ok(TextFormat::Blocks),
        "dict" => Ok(TextFormat::Dict),
        "rawdict" => Ok(TextFormat::RawDict),
        "json" => Ok(TextFormat::Json),
        "rawjson" => Ok(TextFormat::RawJson),
        "html" => Ok(TextFormat::Html),
        "xhtml" => Ok(TextFormat::Xhtml),
        "xml" => Ok(TextFormat::Xml),
        other => Err(PyValueError::new_err(format!(
            "unknown text format {other:?}"
        ))),
    }
}

type WordRow = (f64, f64, f64, f64, String, i32, i32, i32);
type CharRow = (f64, f64, f64, f64, String, f64, f64, bool);

fn text_output_to_py(py: Python<'_>, output: TextOutput) -> PyResult<PyObject> {
    match output {
        TextOutput::Text(text) => Ok(text.into_pyobject(py)?.into_any().unbind()),
        TextOutput::Json(text) | TextOutput::RawJson(text) => {
            Ok(text.into_pyobject(py)?.into_any().unbind())
        }
        TextOutput::Html(text) | TextOutput::Xhtml(text) | TextOutput::Xml(text) => {
            Ok(text.into_pyobject(py)?.into_any().unbind())
        }
        TextOutput::Words(words) => {
            let rows: Vec<WordRow> = words
                .into_iter()
                .map(|w: TextWord| {
                    (
                        w.rect.x0, w.rect.y0, w.rect.x1, w.rect.y1, w.text, w.block_no, w.line_no,
                        w.word_no,
                    )
                })
                .collect();
            Ok(rows.into_pyobject(py)?.into_any().unbind())
        }
        TextOutput::Blocks(blocks) => {
            let rows: Vec<(f64, f64, f64, f64, String, i32, i32)> = blocks
                .into_iter()
                .map(|b| {
                    (
                        b.rect.x0,
                        b.rect.y0,
                        b.rect.x1,
                        b.rect.y1,
                        b.text,
                        b.number,
                        b.block_type,
                    )
                })
                .collect();
            Ok(rows.into_pyobject(py)?.into_any().unbind())
        }
        TextOutput::Dict(dict) => {
            let py_dict = PyDict::new(py);
            py_dict.set_item("width", dict.width)?;
            py_dict.set_item("height", dict.height)?;
            let mut py_blocks = Vec::new();
            for block in dict.blocks {
                py_blocks.push(dict_block_to_py(py, block)?);
            }
            py_dict.set_item("blocks", py_blocks)?;
            Ok(py_dict.into_any().unbind())
        }
        _ => Err(crate::compat::unknown_variant("text output")),
    }
}

fn dict_block_to_py(
    py: Python<'_>,
    block: pdfluent::compat::text::DictBlock,
) -> PyResult<PyObject> {
    use pdfluent::compat::text::DictBlock as Block;
    let py_block = PyDict::new(py);
    match block {
        Block::Text {
            bbox,
            number,
            flags,
            lines,
        } => {
            py_block.set_item("type", 0)?;
            py_block.set_item("bbox", (bbox.x0, bbox.y0, bbox.x1, bbox.y1))?;
            py_block.set_item("number", number)?;
            py_block.set_item("flags", flags)?;
            let mut py_lines = Vec::new();
            for line in lines {
                let py_line = PyDict::new(py);
                py_line.set_item(
                    "bbox",
                    (line.bbox.x0, line.bbox.y0, line.bbox.x1, line.bbox.y1),
                )?;
                py_line.set_item("wmode", line.wmode)?;
                py_line.set_item("dir", (line.direction.x, line.direction.y))?;
                let mut py_spans = Vec::new();
                for span in line.spans {
                    let py_span = PyDict::new(py);
                    py_span.set_item(
                        "bbox",
                        (span.bbox.x0, span.bbox.y0, span.bbox.x1, span.bbox.y1),
                    )?;
                    py_span.set_item("origin", (span.origin.x, span.origin.y))?;
                    py_span.set_item("font", span.font)?;
                    py_span.set_item("size", span.size)?;
                    py_span.set_item("color", span.color)?;
                    py_span.set_item("alpha", span.alpha)?;
                    py_span.set_item("flags", span.flags)?;
                    py_span.set_item("bidi", span.bidi)?;
                    py_span.set_item("char_flags", span.char_flags)?;
                    py_span.set_item("ascender", span.ascender)?;
                    py_span.set_item("descender", span.descender)?;
                    py_span.set_item("text", span.text)?;
                    let chars: Vec<CharRow> = span
                        .chars
                        .into_iter()
                        .map(|c| {
                            (
                                c.bbox.x0,
                                c.bbox.y0,
                                c.bbox.x1,
                                c.bbox.y1,
                                c.ch.to_string(),
                                c.origin.x,
                                c.origin.y,
                                c.synthetic,
                            )
                        })
                        .collect();
                    py_span.set_item("chars", chars)?;
                    py_spans.push(py_span);
                }
                py_line.set_item("spans", py_spans)?;
                py_lines.push(py_line);
            }
            py_block.set_item("lines", py_lines)?;
        }
        Block::Image {
            bbox,
            number,
            width,
            height,
            ..
        } => {
            py_block.set_item("type", 1)?;
            py_block.set_item("bbox", (bbox.x0, bbox.y0, bbox.x1, bbox.y1))?;
            py_block.set_item("number", number)?;
            py_block.set_item("width", width)?;
            py_block.set_item("height", height)?;
        }
        _ => {}
    }
    Ok(py_block.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Pixmap and TextPage placeholders (lanes L3/L1 own the full classes)
// ---------------------------------------------------------------------------

/// Rendered pixels placeholder: lanes L3 owns `Pixmap`.
#[pyclass(name = "Pixmap")]
pub struct PyPixmap {
    inner: CompatPixmap,
}

#[pymethods]
impl PyPixmap {
    #[pyo3(signature = (output="png", jpg_quality=90))]
    fn tobytes(&self, py: Python<'_>, output: &str, jpg_quality: i32) -> PyResult<PyObject> {
        let bytes = self
            .inner
            .tobytes(output, jpg_quality)
            .map_err(compat_err_to_py)?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    #[getter]
    fn width(&self) -> i32 {
        self.inner.width()
    }

    #[getter]
    fn w(&self) -> i32 {
        self.inner.width()
    }

    #[getter]
    fn height(&self) -> i32 {
        self.inner.height()
    }

    #[getter]
    fn h(&self) -> i32 {
        self.inner.height()
    }

    #[getter]
    fn n(&self) -> i32 {
        self.inner.n()
    }

    #[getter]
    fn alpha(&self) -> bool {
        self.inner.alpha()
    }

    fn samples(&self, py: Python<'_>) -> PyObject {
        PyBytes::new(py, self.inner.samples()).into_any().unbind()
    }
}

impl PyPixmap {
    fn wrap(inner: CompatPixmap) -> Self {
        Self { inner }
    }

    /// Encoded bytes for image insertion, whatever lane owns the pixmap.
    #[allow(dead_code)]
    fn encoded_for_insert(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
        if let Ok(own) = value.extract::<PyRef<'_, Self>>() {
            return own.inner.tobytes("png", 90).map_err(compat_err_to_py);
        }
        let tobytes = value
            .getattr("tobytes")
            .map_err(|_| PyTypeError::new_err("pixmap must provide tobytes()".to_string()))?;
        let out = tobytes.call1(("png",))?;
        out.extract::<Vec<u8>>()
            .map_err(|_| PyTypeError::new_err("pixmap.tobytes() must return bytes".to_string()))
    }
}

/// Extracted text page placeholder: lane L1 owns `TextPage`.
#[pyclass(name = "TextPage")]
pub struct PyTextPage {
    slot: Slot,
    snapshot: Arc<Vec<u8>>,
    page_index: usize,
    clip: Option<GRect>,
    flags: i32,
}

#[pymethods]
impl PyTextPage {
    #[allow(non_snake_case)]
    fn extractTEXT(&self) -> PyResult<String> {
        let bytes = slot_bytes(&self.slot, self.snapshot.as_ref())?;
        let facade = open_facade(&bytes)?;
        let page = pdfluent::compat::Page::new(&facade, self.page_index);
        let textpage = page
            .get_textpage(self.clip, self.flags, None)
            .map_err(compat_err_to_py)?;
        textpage.extract_text(false).map_err(compat_err_to_py)
    }

    #[getter]
    fn rect(&self) -> PyResult<PyRect> {
        if let Some(clip) = self.clip {
            return Ok(PyRect::wrap(clip));
        }
        let bytes = slot_bytes(&self.slot, self.snapshot.as_ref())?;
        let facade = open_facade(&bytes)?;
        let page = pdfluent::compat::Page::new(&facade, self.page_index);
        let textpage = CompatTextPage::new(page);
        textpage.rect().map(PyRect::wrap).map_err(compat_err_to_py)
    }
}

impl PyTextPage {
    fn wrap(
        slot: Slot,
        snapshot: Arc<Vec<u8>>,
        page_index: usize,
        clip: Option<GRect>,
        flags: i32,
    ) -> Self {
        Self {
            slot,
            snapshot,
            page_index,
            clip,
            flags,
        }
    }
}

// ---------------------------------------------------------------------------
// Annot
// ---------------------------------------------------------------------------

/// A live annotation handle: object number plus the mutation channel.
#[pyclass(name = "Annot")]
pub struct PyAnnot {
    xref: i32,
    page_index: usize,
    slot: Slot,
    snapshot: Arc<Vec<u8>>,
    parent: Py<PyDocument>,
}

impl PyAnnot {
    fn wrap(
        xref: i32,
        page_index: usize,
        slot: Slot,
        snapshot: Arc<Vec<u8>>,
        parent: Py<PyDocument>,
    ) -> Self {
        Self {
            xref,
            page_index,
            slot,
            snapshot,
            parent,
        }
    }

    #[allow(dead_code)]
    fn from_page(page: &PyPage, xref: i32) -> Self {
        Self::wrap(
            xref,
            page.index,
            Arc::clone(&page.lopdf),
            Arc::clone(&page.snapshot),
            Python::with_gil(|py| page.parent.clone_ref(py)),
        )
    }

    fn bytes(&self) -> PyResult<Vec<u8>> {
        slot_bytes(&self.slot, self.snapshot.as_ref())
    }

    /// Ensure the widget is in the calculation order: module-function
    /// backing, published through the annotation slot like any write.
    pub(crate) fn ensure_widget_calc(&self) -> PyResult<()> {
        self.write(pdfluent::compat::util_ensure_widget_calc)
    }

    fn read<T>(
        &self,
        run: impl for<'x> FnOnce(FacadeAnnot<'x>) -> Result<T, pdfluent::Error>,
    ) -> PyResult<T> {
        let bytes = self.bytes()?;
        let mut facade = open_facade(&bytes)?;
        let mut view = facade_view(&mut facade, self.page_index);
        let annot = view
            .load_annot(&AnnotIdent::Xref(self.xref))
            .map_err(compat_err_to_py)?;
        run(annot).map_err(compat_err_to_py)
    }

    fn write<T>(
        &self,
        run: impl for<'x> FnOnce(FacadeAnnot<'x>) -> Result<T, pdfluent::Error>,
    ) -> PyResult<T> {
        let bytes = self.bytes()?;
        let mut facade = open_facade(&bytes)?;
        let out = {
            let mut view = facade_view(&mut facade, self.page_index);
            let annot = view
                .load_annot(&AnnotIdent::Xref(self.xref))
                .map_err(compat_err_to_py)?;
            run(annot).map_err(compat_err_to_py)?
        };
        let saved = facade.to_bytes().map_err(compat_err_to_py)?;
        store_slot(&self.slot, &saved)?;
        Ok(out)
    }
}

fn annot_info_to_py(py: Python<'_>, info: AnnotInfo) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("content", info.content)?;
    dict.set_item("title", info.title)?;
    dict.set_item("subject", info.subject)?;
    dict.set_item("creationDate", info.creation_date)?;
    dict.set_item("modDate", info.mod_date)?;
    dict.set_item("name", info.name)?;
    dict.set_item("id", info.id)?;
    Ok(dict.into_any().unbind())
}

fn annot_info_from_py(value: &Bound<'_, PyAny>) -> PyResult<AnnotInfo> {
    let get = |key: &str| -> PyResult<String> {
        match value.get_item(key).ok() {
            Some(item) if !item.is_none() => item
                .extract::<String>()
                .map_err(|_| PyTypeError::new_err(format!("info[{key:?}] must be str"))),
            _ => Ok(String::new()),
        }
    };
    Ok(AnnotInfo {
        content: get("content")?,
        title: get("title")?,
        subject: get("subject")?,
        creation_date: get("creationDate")?,
        mod_date: get("modDate")?,
        name: get("name")?,
        id: get("id")?,
    })
}

#[pymethods]
impl PyAnnot {
    #[new]
    #[pyo3(signature = (annot))]
    fn new(annot: &Bound<'_, PyAny>) -> PyResult<Self> {
        let _ = annot;
        Err(validation_error(
            ValidationKind::Precondition,
            "Annot() takes no live handle",
        ))
    }

    fn __bool__(&self) -> bool {
        true
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(peer) = other.extract::<PyRef<'_, Self>>() else {
            return Ok(false);
        };
        Ok(self.page_index == peer.page_index
            && self.xref == peer.xref
            && Arc::ptr_eq(&self.slot, &peer.slot))
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!self.__eq__(other)?)
    }

    fn __lt__(&self, _other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Err(PyTypeError::new_err(
            "annotation ordering is unsupported".to_string(),
        ))
    }

    fn __le__(&self, _other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Err(PyTypeError::new_err(
            "annotation ordering is unsupported".to_string(),
        ))
    }

    fn __gt__(&self, _other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Err(PyTypeError::new_err(
            "annotation ordering is unsupported".to_string(),
        ))
    }

    fn __ge__(&self, _other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Err(PyTypeError::new_err(
            "annotation ordering is unsupported".to_string(),
        ))
    }

    #[getter]
    fn apn_bbox(&self) -> PyResult<PyRect> {
        self.read(|annot| annot.apn_bbox()).map(PyRect::wrap)
    }

    #[getter]
    fn apn_matrix(&self) -> PyResult<PyMatrix> {
        self.read(|annot| annot.apn_matrix()).map(PyMatrix::wrap)
    }

    #[getter]
    fn blendmode(&self) -> PyResult<Option<String>> {
        let mode = self.read(|annot| annot.blendmode())?;
        Ok(if mode.is_empty() { None } else { Some(mode) })
    }

    #[getter]
    fn border(&self, py: Python<'_>) -> PyResult<PyObject> {
        let border = self.read(|annot| annot.border())?;
        let dict = PyDict::new(py);
        if border.clouds == -2 {
            return Ok(dict.into_any().unbind());
        }
        dict.set_item("width", num_to_py(py, border.width, border.width_is_int)?)?;
        dict.set_item(
            "style",
            border.style.map_or_else(
                || py.None(),
                |s| s.into_pyobject(py).unwrap().into_any().unbind(),
            ),
        )?;
        dict.set_item("dashes", border.dashes)?;
        dict.set_item("clouds", border.clouds)?;
        Ok(dict.into_any().unbind())
    }

    #[pyo3(signature = (sanitize=1))]
    fn clean_contents(&self, sanitize: i32) -> PyResult<()> {
        self.write(|mut annot| annot.clean_contents(sanitize != 0))
    }

    #[getter]
    fn colors(&self, py: Python<'_>) -> PyResult<PyObject> {
        let colors = self.read(|annot| annot.colors())?;
        let dict = PyDict::new(py);
        dict.set_item("stroke", colors.stroke)?;
        dict.set_item("fill", colors.fill)?;
        Ok(dict.into_any().unbind())
    }

    fn delete_responses(&self) -> PyResult<()> {
        self.write(|mut annot| annot.delete_responses())
    }

    #[getter]
    fn file_info(&self, py: Python<'_>) -> PyResult<PyObject> {
        let info = self.read(|annot| annot.file_info())?;
        let dict = PyDict::new(py);
        dict.set_item("filename", info.filename)?;
        dict.set_item("description", info.description)?;
        dict.set_item("size", info.size as i64)?;
        dict.set_item("length", info.size as i64)?;
        Ok(dict.into_any().unbind())
    }

    #[getter]
    fn flags(&self) -> PyResult<i32> {
        self.read(|annot| annot.flags())
    }

    fn get_file(&self, py: Python<'_>) -> PyResult<PyObject> {
        let bytes = self.read(|annot| annot.get_file())?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    fn get_oc(&self) -> PyResult<i32> {
        self.read(|annot| annot.get_oc())
    }

    fn get_parent(&self) -> PyResult<PyPage> {
        Python::with_gil(|py| {
            let parent = self.parent.bind(py).borrow();
            let engine = parent.engine()?;
            Ok(PyPage {
                doc: engine,
                index: self.page_index,
                parent: self.parent.clone_ref(py),
                lopdf: Arc::clone(&self.slot),
                snapshot: Arc::clone(&self.snapshot),
                refreshed: AtomicBool::new(false),
                clip: Mutex::new(None),
            })
        })
    }

    #[pyo3(signature = (matrix=None, dpi=None, colorspace=None, alpha=0))]
    fn get_pixmap(
        &self,
        matrix: Option<&Bound<'_, PyAny>>,
        dpi: Option<f64>,
        colorspace: Option<&Bound<'_, PyAny>>,
        alpha: i32,
    ) -> PyResult<PyPixmap> {
        let _ = colorspace;
        let mut options = CompatRenderOptions::default();
        options.matrix = opt_lift(matrix, opt_matrix)?.unwrap_or(GMatrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        });
        options.dpi = dpi;
        options.alpha = alpha != 0;
        options.annots = true;
        self.read(|annot| annot.get_pixmap(&options))
            .map(PyPixmap::wrap)
    }

    fn get_sound(&self, py: Python<'_>) -> PyResult<PyObject> {
        let bytes = self.read(|annot| annot.get_sound())?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn get_text(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let mut format = "text".to_string();
        let mut clip: Option<GRect> = None;
        let mut flags: Option<i32> = None;
        let mut sort = false;
        let positional = args.iter().collect::<Vec<_>>();
        if let Some(first) = positional.first() {
            format = first
                .extract::<String>()
                .map_err(|_| PyTypeError::new_err("format must be str".to_string()))?;
        }
        if let Some(second) = positional.get(1) {
            clip = opt_rect(second)?;
        }
        if let Some(third) = positional.get(2) {
            flags = third.extract::<i32>().ok();
        }
        if let Some(fourth) = positional.get(3) {
            sort = fourth.extract::<bool>().unwrap_or(false);
        }
        if let Some(kwargs) = kwargs {
            if let Some(item) = kwargs.get_item("format").ok().flatten() {
                if !item.is_none() {
                    format = item.extract::<String>()?;
                }
            }
            if let Some(item) = kwargs.get_item("clip").ok().flatten() {
                if !item.is_none() {
                    clip = Some(rect_from_py(&item)?);
                }
            }
            if let Some(item) = kwargs.get_item("flags").ok().flatten() {
                if !item.is_none() {
                    flags = item.extract::<i32>().ok();
                }
            }
            if let Some(item) = kwargs.get_item("sort").ok().flatten() {
                if !item.is_none() {
                    sort = item.extract::<bool>().unwrap_or(false);
                }
            }
        }
        let format = text_format_from_py(&format).map_err(|_| {
            validation_error(
                ValidationKind::Precondition,
                "get_text needs a known format",
            )
        })?;
        let mut options = TextOptions::default().with_format(format);
        options = options.with_sort(sort);
        if let Some(clip) = clip {
            options = options.with_clip(clip);
        }
        if let Some(flags) = flags {
            options = options.with_flags(flags);
        }
        let output = self.read(|annot| annot.get_text(&options))?;
        text_output_to_py(py, output)
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn get_textbox(
        &self,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        let positional = args.iter().collect::<Vec<_>>();
        let mut rect: Option<GRect> = None;
        if let Some(first) = positional.first() {
            rect = opt_rect(first)?;
        }
        if let Some(kwargs) = kwargs {
            if let Some(item) = kwargs.get_item("rect").ok().flatten() {
                if !item.is_none() {
                    rect = Some(rect_from_py(&item)?);
                }
            }
        }
        self.read(|annot| annot.get_textbox(rect))
    }

    #[pyo3(signature = (clip=None, flags=0))]
    fn get_textpage(&self, clip: Option<&Bound<'_, PyAny>>, flags: i32) -> PyResult<PyTextPage> {
        let clip = opt_lift(clip, opt_rect)?;
        let rect = self.read(|annot| annot.rect())?;
        Ok(PyTextPage::wrap(
            Arc::clone(&self.slot),
            Arc::clone(&self.snapshot),
            self.page_index,
            clip.or(Some(rect)),
            flags,
        ))
    }

    #[getter]
    fn has_popup(&self) -> PyResult<bool> {
        self.read(|annot| annot.has_popup())
    }

    #[getter]
    fn info(&self, py: Python<'_>) -> PyResult<PyObject> {
        let info = self.read(|annot| annot.info())?;
        annot_info_to_py(py, info)
    }

    #[getter]
    fn irt_xref(&self) -> PyResult<i32> {
        self.read(|annot| annot.irt_xref())
    }

    #[getter]
    fn is_open(&self) -> PyResult<i32> {
        Ok(i32::from(self.read(|annot| annot.is_open())?))
    }

    #[getter]
    fn language(&self) -> PyResult<String> {
        self.read(|annot| annot.language())
    }

    #[getter]
    fn line_ends(&self) -> PyResult<Option<(i32, i32)>> {
        self.read(|annot| annot.line_ends())
    }

    #[getter]
    fn next(&self) -> PyResult<Option<Self>> {
        let next = self.read(|mut annot| annot.next().map(|o| o.map(|a| a.xref().unwrap_or(0))))?;
        Ok(next.map(|xref| {
            Self::wrap(
                xref,
                self.page_index,
                Arc::clone(&self.slot),
                Arc::clone(&self.snapshot),
                Python::with_gil(|py| self.parent.clone_ref(py)),
            )
        }))
    }

    #[getter]
    fn opacity(&self, py: Python<'_>) -> PyResult<PyObject> {
        let (value, is_int) = self.read(|annot| {
            let value = annot.opacity()?;
            let is_int = annot.opacity_is_int()?;
            Ok((value, is_int))
        })?;
        num_to_py(py, value, is_int)
    }

    #[getter]
    fn popup_rect(&self) -> PyResult<PyRect> {
        match self.read(|annot| annot.popup_rect()) {
            Ok(rect) => Ok(PyRect::wrap(rect)),
            Err(_) => Ok(PyRect::wrap(GRect {
                x0: -2147483648.0,
                y0: -2147483648.0,
                x1: 2147483520.0,
                y1: 2147483520.0,
            })),
        }
    }

    #[getter]
    fn popup_xref(&self) -> PyResult<i32> {
        self.read(|annot| annot.popup_xref())
    }

    #[getter]
    fn rect(&self) -> PyResult<PyRect> {
        self.read(|annot| annot.rect()).map(PyRect::wrap)
    }

    #[getter]
    fn rect_delta(&self) -> PyResult<Option<(f64, f64, f64, f64)>> {
        match self.read(|annot| annot.rect_delta()) {
            Ok(rect) => Ok(Some((rect.x0, rect.y0, rect.x1, rect.y1))),
            Err(_) => Ok(None),
        }
    }

    #[getter]
    fn rotation(&self) -> PyResult<i32> {
        self.read(|annot| annot.rotation())
    }

    fn set_apn_bbox(&self, bbox: &Bound<'_, PyAny>) -> PyResult<()> {
        let bbox = rect_from_py(bbox)?;
        self.write(|mut annot| annot.set_apn_bbox(bbox))
    }

    fn set_apn_matrix(&self, matrix: &Bound<'_, PyAny>) -> PyResult<()> {
        let matrix = matrix_from_py(matrix)?;
        self.write(|mut annot| annot.set_apn_matrix(matrix))
    }

    fn set_blendmode(&self, blend_mode: &str) -> PyResult<()> {
        self.write(|mut annot| annot.set_blendmode(blend_mode))
    }

    #[pyo3(signature = (border=None, width=-1.0, style=None, dashes=None, clouds=-1))]
    fn set_border(
        &self,
        border: Option<&Bound<'_, PyAny>>,
        width: f64,
        style: Option<String>,
        dashes: Option<&Bound<'_, PyAny>>,
        clouds: i32,
    ) -> PyResult<()> {
        let mut options = BorderOptions::default();
        options.width = -1.0;
        options.clouds = -1;
        if let Some(border) = border {
            if !border.is_none() {
                if let Ok(current) = border.get_item("width") {
                    if !current.is_none() {
                        options.width = current.extract::<f64>()?;
                    }
                }
                if let Ok(current) = border.get_item("style") {
                    if !current.is_none() {
                        options.style = Some(current.extract::<String>()?);
                    }
                }
                if let Ok(current) = border.get_item("dashes") {
                    if !current.is_none() {
                        let raw: Vec<f64> = current.extract()?;
                        options.dashes = raw.iter().map(|d| d.round() as i32).collect();
                    }
                }
                if let Ok(current) = border.get_item("clouds") {
                    if !current.is_none() {
                        options.clouds = current.extract::<i32>()?;
                    }
                }
            }
        }
        if width >= 0.0 {
            options.width = width;
        }
        if style.is_some() {
            options.style = style;
        }
        if let Some(dashes) = dashes {
            if !dashes.is_none() {
                let raw: Vec<f64> = dashes.extract()?;
                options.dashes = raw.iter().map(|d| d.round() as i32).collect();
            }
        }
        if clouds >= 0 {
            options.clouds = clouds;
        }
        self.write(|mut annot| annot.set_border(&options))
    }

    #[pyo3(signature = (colors=None, stroke=None, fill=None))]
    fn set_colors(
        &self,
        colors: Option<&Bound<'_, PyAny>>,
        stroke: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let stroke_seq = opt_lift(stroke, facade_opt_seq)?;
        let colors_seq = opt_lift(colors, facade_opt_seq)?;
        let fill_seq = opt_lift(fill, facade_opt_seq)?;
        let resolved = AnnotColors {
            stroke: stroke_seq.or(colors_seq).unwrap_or_default(),
            fill: fill_seq.unwrap_or_default(),
        };
        self.write(|mut annot| annot.set_colors(&resolved))
    }

    fn set_flags(&self, flags: i32) -> PyResult<()> {
        self.write(|mut annot| annot.set_flags(flags))
    }

    #[pyo3(signature = (info=None, content=None, title=None, creationDate=None, modDate=None, subject=None))]
    #[allow(non_snake_case)]
    fn set_info(
        &self,
        info: Option<&Bound<'_, PyAny>>,
        content: Option<String>,
        title: Option<String>,
        creationDate: Option<String>,
        modDate: Option<String>,
        subject: Option<String>,
    ) -> PyResult<()> {
        let base = match info {
            Some(found) if !found.is_none() => Some(annot_info_from_py(found)?),
            _ => None,
        };
        let mut options = AnnotInfoOptions::default();
        options.info = base;
        options.content = content;
        options.title = title;
        options.creation_date = creationDate;
        options.mod_date = modDate;
        options.subject = subject;
        self.write(|mut annot| annot.set_info(&options))
    }

    fn set_irt_xref(&self, xref: i32) -> PyResult<()> {
        self.write(|mut annot| annot.set_irt_xref(xref))
    }

    #[pyo3(signature = (language=None))]
    fn set_language(&self, language: Option<String>) -> PyResult<()> {
        self.write(|mut annot| annot.set_language(language.as_deref()))
    }

    fn set_line_ends(&self, start: i32, end: i32) -> PyResult<()> {
        self.write(|mut annot| annot.set_line_ends(start, end))
    }

    fn set_name(&self, name: &str) -> PyResult<()> {
        self.write(|mut annot| annot.set_name(name))
    }

    #[pyo3(signature = (oc=0))]
    fn set_oc(&self, oc: i32) -> PyResult<()> {
        self.write(|mut annot| annot.set_oc(oc))
    }

    fn set_opacity(&self, opacity: f64) -> PyResult<()> {
        self.write(|mut annot| annot.set_opacity(opacity))
    }

    fn set_open(&self, is_open: &Bound<'_, PyAny>) -> PyResult<()> {
        let open = if let Ok(found) = is_open.extract::<bool>() {
            found
        } else if let Ok(found) = is_open.extract::<i32>() {
            found != 0
        } else {
            return Err(PyTypeError::new_err("set_open needs a flag".to_string()));
        };
        self.write(|mut annot| annot.set_open(open))
    }

    fn set_popup(&self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = rect_from_py(rect)?;
        self.write(|mut annot| annot.set_popup(rect))
    }

    fn set_rect(&self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = draw_rect(rect, "set_rect needs a rect")?;
        self.write(|mut annot| annot.set_rect(rect))
    }

    #[pyo3(signature = (rotate=0))]
    fn set_rotation(&self, rotate: i32) -> PyResult<()> {
        self.write(|mut annot| annot.set_rotation(rotate))
    }

    #[getter]
    fn r#type(&self) -> PyResult<(i32, String)> {
        let kind: AnnotType = self.read(|annot| annot.annot_type())?;
        Ok((kind.code, kind.name))
    }

    #[pyo3(signature = (blend_mode=None, opacity=None, fontsize=0.0, fontname=None, text_color=None, border_color=None, fill_color=None, cross_out=true, rotate=-1))]
    #[allow(clippy::too_many_arguments)]
    fn update(
        &self,
        blend_mode: Option<String>,
        opacity: Option<f64>,
        fontsize: f64,
        fontname: Option<String>,
        text_color: Option<&Bound<'_, PyAny>>,
        border_color: Option<&Bound<'_, PyAny>>,
        fill_color: Option<&Bound<'_, PyAny>>,
        cross_out: bool,
        rotate: i32,
    ) -> PyResult<()> {
        let mut options = AnnotUpdateOptions::default();
        options.blend_mode = blend_mode;
        options.opacity = opacity;
        options.fontsize = fontsize;
        options.fontname = fontname;
        options.text_color = opt_lift(text_color, facade_opt_seq)?;
        options.border_color = opt_lift(border_color, facade_opt_seq)?;
        options.fill_color = opt_lift(fill_color, facade_opt_seq)?;
        options.cross_out = cross_out;
        options.rotate = rotate;
        self.write(|mut annot| annot.update(&options))
    }

    #[pyo3(signature = (buffer_=None, filename=None, ufilename=None, desc=None))]
    fn update_file(
        &self,
        buffer_: Option<&Bound<'_, PyAny>>,
        filename: Option<String>,
        ufilename: Option<String>,
        desc: Option<String>,
    ) -> PyResult<()> {
        let data = match buffer_ {
            Some(found) if !found.is_none() => bytes_from_py(found).map_err(|_| {
                validation_error(ValidationKind::DrawArg, "update_file needs bytes")
            })?,
            _ => Vec::new(),
        };
        self.write(|mut annot| {
            annot.update_file(
                &data,
                filename.as_deref(),
                ufilename.as_deref(),
                desc.as_deref(),
            )
        })
    }

    fn update_timing_test(&self) -> PyResult<f64> {
        self.read(|annot| annot.update_timing_test())
    }

    #[getter]
    fn vertices(&self, py: Python<'_>) -> PyResult<PyObject> {
        enum Out {
            Flat(Vec<GPoint>),
            Strokes(Vec<Vec<GPoint>>),
        }
        let out = self.read(|annot| {
            if annot.annot_type()?.code == PDF_ANNOT_INK {
                Ok(Out::Strokes(annot.ink_vertices()?))
            } else {
                Ok(Out::Flat(annot.vertices()?))
            }
        })?;
        match out {
            Out::Flat(points) => {
                let rows: Vec<Vec<f64>> = points.iter().map(|p| vec![p.x, p.y]).collect();
                Ok(rows.into_pyobject(py)?.into_any().unbind())
            }
            Out::Strokes(strokes) => {
                let rows: Vec<Vec<Vec<f64>>> = strokes
                    .iter()
                    .map(|stroke| stroke.iter().map(|p| vec![p.x, p.y]).collect())
                    .collect();
                Ok(rows.into_pyobject(py)?.into_any().unbind())
            }
        }
    }

    #[getter]
    fn xref(&self) -> i32 {
        self.xref
    }
}

/// Register the lane's new module-level names.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAnnot>()?;
    m.add_class::<PyFont>()?;
    m.add_class::<PyShape>()?;
    m.add_class::<PyTextWriter>()?;
    m.add_class::<PyWidget>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Drawings
// ---------------------------------------------------------------------------

pub(crate) fn facade_value_to_py(py: Python<'_>, value: &FacadeValue) -> PyResult<PyObject> {
    match value {
        FacadeValue::Null => Ok(py.None()),
        FacadeValue::Bool(found) => Ok(pyo3::types::PyBool::new(py, *found)
            .to_owned()
            .into_any()
            .unbind()),
        FacadeValue::Int(found) => Ok(found.into_pyobject(py)?.into_any().unbind()),
        FacadeValue::Float(found) => Ok(found.into_pyobject(py)?.into_any().unbind()),
        FacadeValue::Str(found) => Ok(found.clone().into_pyobject(py)?.into_any().unbind()),
        FacadeValue::Bytes(found) => Ok(PyBytes::new(py, found).into_any().unbind()),
        FacadeValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(facade_value_to_py(py, item)?);
            }
            Ok(out.into_pyobject(py)?.into_any().unbind())
        }
        FacadeValue::Dict(entries) => {
            let dict = PyDict::new(py);
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort();
            for key in keys {
                dict.set_item(key, facade_value_to_py(py, &entries[key])?)?;
            }
            Ok(dict.into_any().unbind())
        }
        _ => Err(crate::compat::unknown_variant("compat value")),
    }
}

fn drawing_item_to_py(py: Python<'_>, op: &str, params: &FacadeValue) -> PyResult<PyObject> {
    let floats = match params {
        FacadeValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    FacadeValue::Float(found) => out.push(*found),
                    FacadeValue::Int(found) => out.push(*found as f64),
                    _ => return facade_value_to_py(py, params),
                }
            }
            out
        }
        _ => return facade_value_to_py(py, params),
    };
    if op == "re" && floats.len() == 5 {
        let rect = PyRect::wrap(GRect {
            x0: floats[0],
            y0: floats[1],
            x1: floats[2],
            y1: floats[3],
        });
        let flag = floats[4] as i32;
        return Ok((op, rect, flag).into_pyobject(py)?.into_any().unbind());
    }
    if op == "l" && floats.len() == 4 {
        let from = PyPoint::wrap(GPoint {
            x: floats[0],
            y: floats[1],
        });
        let to = PyPoint::wrap(GPoint {
            x: floats[2],
            y: floats[3],
        });
        return Ok((op, from, to).into_pyobject(py)?.into_any().unbind());
    }
    if op == "c" && floats.len() == 8 {
        let at = |i: usize| {
            PyPoint::wrap(GPoint {
                x: floats[i],
                y: floats[i + 1],
            })
        };
        return Ok((op, at(0), at(2), at(4), at(6))
            .into_pyobject(py)?
            .into_any()
            .unbind());
    }
    Ok((op, floats).into_pyobject(py)?.into_any().unbind())
}

/// A compat number that keeps its integer shape: int-typed PDF numbers read
/// back as Python ints, reals and computed values as floats.
fn num_to_py(py: Python<'_>, value: f64, is_int: bool) -> PyResult<PyObject> {
    if is_int {
        Ok((value as i32).into_pyobject(py)?.into_any().unbind())
    } else {
        Ok(value.into_pyobject(py)?.into_any().unbind())
    }
}

fn drawing_to_py(py: Python<'_>, drawing: &Drawing) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("rect", PyRect::wrap(drawing.rect))?;
    dict.set_item("seqno", drawing.seqno)?;
    dict.set_item("type", drawing.drawing_type.clone())?;
    dict.set_item("color", drawing.color.clone())?;
    dict.set_item("fill", drawing.fill.clone())?;
    dict.set_item("width", drawing.width)?;
    dict.set_item("lineCap", drawing.line_cap.clone())?;
    dict.set_item("lineJoin", drawing.line_join.map(|join| join as f64))?;
    dict.set_item("dashes", drawing.dashes.clone())?;
    dict.set_item("closePath", drawing.close_path)?;
    dict.set_item("even_odd", drawing.fill.as_ref().map(|_| drawing.even_odd))?;
    dict.set_item("fill_opacity", drawing.fill_opacity)?;
    dict.set_item("stroke_opacity", drawing.stroke_opacity)?;
    dict.set_item("layer", drawing.layer.clone())?;
    if let Some(level) = drawing.level {
        dict.set_item("level", level)?;
    }
    let mut items = Vec::with_capacity(drawing.items.len());
    for item in &drawing.items {
        items.push(drawing_item_to_py(py, &item.op, &item.params)?);
    }
    dict.set_item("items", items)?;
    Ok(dict.into_any().unbind())
}

/// Explicit drawing lists back to geometry: clustering only reads boxes,
// so the remaining fields stay at their defaults.
fn drawings_from_py(value: &Bound<'_, PyAny>) -> PyResult<Vec<Drawing>> {
    let mut out = Vec::new();
    for item in value.try_iter()? {
        let item = item?;
        let rect = item.getattr("rect").or_else(|_| item.get_item("rect"))?;
        let mut drawing = Drawing::default();
        drawing.rect = rect_from_py(&rect)?;
        out.push(drawing);
    }
    Ok(out)
}

fn morph_from_py(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<f64>>> {
    let found = match value {
        Some(bound) if !bound.is_none() => bound,
        _ => return Ok(None),
    };
    if let Ok(seq) = facade_opt_seq(found) {
        if seq.is_some() {
            return Ok(seq);
        }
    }
    let matrix = matrix_from_py(found)?;
    Ok(Some(vec![
        matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
    ]))
}

// ---------------------------------------------------------------------------
// Font
// ---------------------------------------------------------------------------

/// A font handle: metrics and names for text insertion.
#[pyclass(name = "Font")]
pub struct PyFont {
    inner: FacadeFont,
}

#[pymethods]
impl PyFont {
    #[new]
    #[pyo3(signature = (fontname=None, fontfile=None, fontbuffer=None, script=0, language=None, ordering=-1, is_bold=0, is_italic=0, is_serif=0, embed=1))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        fontname: Option<String>,
        fontfile: Option<String>,
        fontbuffer: Option<&Bound<'_, PyAny>>,
        script: i32,
        language: Option<String>,
        ordering: i32,
        is_bold: i32,
        is_italic: i32,
        is_serif: i32,
        embed: i32,
    ) -> PyResult<Self> {
        let mut options = FontOptions::default();
        options.fontname = fontname;
        options.fontfile = fontfile;
        options.fontbuffer = match fontbuffer {
            Some(found) if !found.is_none() => Some(bytes_from_py(found)?),
            _ => None,
        };
        options.script = script;
        options.language = language;
        options.ordering = ordering;
        options.is_bold = is_bold != 0;
        options.is_italic = is_italic != 0;
        options.is_serif = is_serif != 0;
        options.embed = embed != 0;
        FacadeFont::new(&options)
            .map(|inner| Self { inner })
            .map_err(compat_err_to_py)
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        self.inner.name().map_err(compat_err_to_py)
    }

    #[getter]
    fn flags(&self, py: Python<'_>) -> PyResult<PyObject> {
        let flags = self.inner.flags().map_err(compat_err_to_py)?;
        let dict = PyDict::new(py);
        let bit = |on: bool| i32::from(on);
        dict.set_item("bold", bit(flags.bold))?;
        dict.set_item("cjk", bit(flags.cjk))?;
        dict.set_item("cjk-lang", bit(flags.cjk_lang))?;
        dict.set_item("embed", bit(flags.embed))?;
        dict.set_item("fake-bold", bit(flags.fake_bold))?;
        dict.set_item("fake-italic", bit(flags.fake_italic))?;
        dict.set_item("invalid-bbox", bit(flags.invalid_bbox))?;
        dict.set_item("italic", bit(flags.italic))?;
        dict.set_item("mono", bit(flags.mono))?;
        dict.set_item("never-embed", bit(flags.never_embed))?;
        dict.set_item("opentype", bit(flags.opentype))?;
        dict.set_item("serif", bit(flags.serif))?;
        dict.set_item("stretch", bit(flags.stretch))?;
        dict.set_item("substitute", bit(flags.substitute))?;
        Ok(dict.into_any().unbind())
    }

    #[getter]
    fn ascender(&self) -> PyResult<f64> {
        self.inner.ascender().map_err(compat_err_to_py)
    }

    #[getter]
    fn descender(&self) -> PyResult<f64> {
        self.inner.descender().map_err(compat_err_to_py)
    }

    #[getter]
    fn bbox(&self) -> PyResult<PyRect> {
        self.inner
            .bbox()
            .map(PyRect::wrap)
            .map_err(compat_err_to_py)
    }

    #[getter]
    fn glyph_count(&self) -> PyResult<usize> {
        self.inner.glyph_count().map_err(compat_err_to_py)
    }

    #[getter]
    fn is_bold(&self) -> PyResult<i32> {
        Ok(i32::from(self.inner.is_bold().map_err(compat_err_to_py)?))
    }

    #[getter]
    fn is_italic(&self) -> PyResult<i32> {
        Ok(i32::from(self.inner.is_italic().map_err(compat_err_to_py)?))
    }

    #[getter]
    fn is_monospaced(&self) -> PyResult<i32> {
        Ok(i32::from(
            self.inner.is_monospaced().map_err(compat_err_to_py)?,
        ))
    }

    #[getter]
    fn is_serif(&self) -> PyResult<i32> {
        Ok(i32::from(self.inner.is_serif().map_err(compat_err_to_py)?))
    }

    #[getter]
    fn is_writable(&self) -> PyResult<bool> {
        self.inner.is_writable().map_err(compat_err_to_py)
    }

    #[getter]
    fn buffer(&self, py: Python<'_>) -> PyResult<PyObject> {
        let bytes = self.inner.buffer().map_err(compat_err_to_py)?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    #[pyo3(signature = (text, fontsize=11.0, language=None, script=0, wmode=0, small_caps=0))]
    fn text_length(
        &self,
        text: &str,
        fontsize: f64,
        language: Option<String>,
        script: i32,
        wmode: i32,
        small_caps: i32,
    ) -> PyResult<f64> {
        let mut options = FontMeasureOptions::default();
        options.fontsize = fontsize;
        options.language = language;
        options.script = script;
        options.wmode = wmode;
        options.small_caps = small_caps != 0;
        self.inner
            .text_length(text, &options)
            .map_err(compat_err_to_py)
    }

    #[pyo3(signature = (text, fontsize=11.0, language=None, script=0, wmode=0, small_caps=0))]
    fn char_lengths(
        &self,
        text: &str,
        fontsize: f64,
        language: Option<String>,
        script: i32,
        wmode: i32,
        small_caps: i32,
    ) -> PyResult<Vec<f64>> {
        let mut options = FontMeasureOptions::default();
        options.fontsize = fontsize;
        options.language = language;
        options.script = script;
        options.wmode = wmode;
        options.small_caps = small_caps != 0;
        self.inner
            .char_lengths(text, &options)
            .map_err(compat_err_to_py)
    }

    #[pyo3(signature = (r#chr, language=None, script=0, fallback=0, small_caps=0))]
    fn has_glyph(
        &self,
        r#chr: i32,
        language: Option<String>,
        script: i32,
        fallback: i32,
        small_caps: i32,
    ) -> PyResult<i32> {
        let mut options = FontMeasureOptions::default();
        options.language = language;
        options.script = script;
        options.fallback = fallback;
        options.small_caps = small_caps != 0;
        self.inner
            .has_glyph(r#chr, &options)
            .map_err(compat_err_to_py)
    }

    #[pyo3(signature = (chr_, language=None, script=0, wmode=0, small_caps=0))]
    fn glyph_advance(
        &self,
        chr_: i32,
        language: Option<String>,
        script: i32,
        wmode: i32,
        small_caps: i32,
    ) -> PyResult<f64> {
        let mut options = FontMeasureOptions::default();
        options.language = language;
        options.script = script;
        options.wmode = wmode;
        options.small_caps = small_caps != 0;
        self.inner
            .glyph_advance(chr_, &options)
            .map_err(compat_err_to_py)
    }

    #[pyo3(signature = (r#char, language=None, script=0, small_caps=0))]
    fn glyph_bbox(
        &self,
        r#char: i32,
        language: Option<String>,
        script: i32,
        small_caps: i32,
    ) -> PyResult<PyRect> {
        let mut options = FontMeasureOptions::default();
        options.language = language;
        options.script = script;
        options.small_caps = small_caps != 0;
        self.inner
            .glyph_bbox(r#char, &options)
            .map(PyRect::wrap)
            .map_err(compat_err_to_py)
    }

    fn glyph_name_to_unicode(&self, name: &str) -> PyResult<i32> {
        self.inner
            .glyph_name_to_unicode(name)
            .map_err(compat_err_to_py)
    }

    fn unicode_to_glyph_name(&self, ch: i32) -> PyResult<String> {
        self.inner
            .unicode_to_glyph_name(ch)
            .map_err(compat_err_to_py)
    }

    fn valid_codepoints(&self) -> PyResult<Vec<i32>> {
        self.inner.valid_codepoints().map_err(compat_err_to_py)
    }
}

// ---------------------------------------------------------------------------
// Page editing (T1)
// ---------------------------------------------------------------------------

#[pymethods]
impl PyPage {
    #[pyo3(signature = (quad, text=None, fontname=None, fontsize=11.0, align=0, fill=None, text_color=None, cross_out=true))]
    #[allow(clippy::too_many_arguments)]
    fn add_redact_annot(
        &mut self,
        quad: &Bound<'_, PyAny>,
        text: Option<String>,
        fontname: Option<String>,
        fontsize: f64,
        align: i32,
        fill: Option<&Bound<'_, PyAny>>,
        text_color: Option<&Bound<'_, PyAny>>,
        cross_out: bool,
    ) -> PyResult<PyAnnot> {
        // A rect spans the same corners as its quad; accept both shapes.
        let quad = quad_from_py(quad)
            .or_else(|_| rect_from_py(quad).map(|rect| rect.quad()))
            .map_err(|_| PyTypeError::new_err("quad_like must be a Quad or Rect".to_string()))?;
        let mut options = RedactAnnotOptions::default();
        options.text = text;
        options.fontname = fontname;
        options.fontsize = fontsize;
        options.align = align;
        options.fill = opt_lift(fill, facade_opt_seq)?;
        options.text_color = opt_lift(text_color, facade_opt_seq)?;
        options.cross_out = cross_out;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_redact_annot(quad, &options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (images=2, graphics=1, text=0))]
    fn apply_redactions(&mut self, images: i32, graphics: i32, text: i32) -> PyResult<bool> {
        let mut options = RedactApplyOptions::default();
        options.images = images;
        options.graphics = graphics;
        options.text = text;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).apply_redactions(&options)
        })
    }

    #[pyo3(signature = (p1, p2, color=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, overlay=true, morph=None, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_line(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        morph: Option<&Bound<'_, PyAny>>,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let p1 = point_from_py(p1)?;
        let p2 = point_from_py(p2)?;
        let mut options = DrawOptions::default();
        options.color = opt_lift(color, facade_opt_seq)?.unwrap_or(vec![0.0]);
        options.dashes = dashes;
        options.width = width;
        options.line_cap = lineCap;
        options.line_join = lineJoin;
        options.overlay = overlay;
        options.morph = morph_from_py(morph)?;
        options.stroke_opacity = stroke_opacity;
        options.fill_opacity = fill_opacity;
        options.oc = oc;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_line(p1, p2, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (rect, color=None, fill=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, morph=None, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0, radius=None))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_rect(
        &mut self,
        rect: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        morph: Option<&Bound<'_, PyAny>>,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
        radius: Option<f64>,
    ) -> PyResult<PyPoint> {
        let rect = rect_from_py(rect)?;
        let mut options = DrawOptions::default();
        options.color = opt_lift(color, facade_opt_seq)?.unwrap_or(vec![0.0]);
        options.fill = opt_lift(fill, facade_opt_seq)?;
        options.dashes = dashes;
        options.width = width;
        options.line_cap = lineCap;
        options.line_join = lineJoin;
        options.morph = morph_from_py(morph)?;
        options.overlay = overlay;
        options.stroke_opacity = stroke_opacity;
        options.fill_opacity = fill_opacity;
        options.oc = oc;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_rect(rect, &options, radius)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (extended=false))]
    fn get_drawings(&mut self, py: Python<'_>, extended: bool) -> PyResult<PyObject> {
        let index = self.index;
        let drawings = mutate_page(self, |facade| {
            let view = FacadePageMut::new(facade, index);
            view.get_drawings(extended)
        })?;
        let mut out = Vec::with_capacity(drawings.len());
        for drawing in &drawings {
            out.push(drawing_to_py(py, drawing)?);
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }

    #[pyo3(signature = (rect, *, alpha=-1, filename=None, height=0.0, keep_proportion=true, mask=None, oc=0, overlay=true, pixmap=None, rotate=0, stream=None, width=0.0, xref=0))]
    #[allow(clippy::too_many_arguments)]
    fn insert_image(
        &mut self,
        rect: &Bound<'_, PyAny>,
        alpha: i32,
        filename: Option<String>,
        height: f64,
        keep_proportion: bool,
        mask: Option<&Bound<'_, PyAny>>,
        oc: i32,
        overlay: bool,
        pixmap: Option<&Bound<'_, PyAny>>,
        rotate: i32,
        stream: Option<&Bound<'_, PyAny>>,
        width: f64,
        xref: i32,
    ) -> PyResult<i32> {
        let rect = rect_from_py(rect)?;
        let mut options = ImageInsertOptions::default();
        options.alpha = alpha;
        options.filename = filename;
        options.height = height;
        options.keep_proportion = keep_proportion;
        options.mask = match mask {
            Some(found) if !found.is_none() => {
                let xref: i32 = found.extract()?;
                Some(FacadeValue::Int(i64::from(xref)))
            }
            _ => None,
        };
        options.oc = oc;
        options.overlay = overlay;
        options.pixmap = pixmap
            .filter(|found| !found.is_none())
            .map(pixmap_from_py)
            .transpose()?;
        options.rotate = rotate;
        options.stream = match stream {
            Some(found) if !found.is_none() => Some(bytes_from_py(found)?),
            _ => None,
        };
        options.width = width;
        options.xref = xref;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_image(rect, &options)
        })
    }

    #[pyo3(signature = (point, text, *, fontsize=11.0, lineheight=None, fontname="helv", fontfile=None, set_simple=0, encoding=0, color=None, fill=None, border_width=0.05, miter_limit=1.0, render_mode=0, rotate=0, morph=None, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    fn insert_text(
        &mut self,
        point: &Bound<'_, PyAny>,
        text: &Bound<'_, PyAny>,
        fontsize: f64,
        lineheight: Option<f64>,
        fontname: &str,
        fontfile: Option<String>,
        set_simple: i32,
        encoding: i32,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        border_width: f64,
        miter_limit: f64,
        render_mode: i32,
        rotate: i32,
        morph: Option<&Bound<'_, PyAny>>,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<i32> {
        let point = point_from_py(point)?;
        let text = text_arg_from_py(text)?;
        let mut options = InsertTextOptions::default();
        options.fontsize = fontsize;
        options.lineheight = lineheight;
        options.fontname = fontname.to_string();
        options.fontfile = fontfile;
        options.set_simple = set_simple != 0;
        options.encoding = encoding;
        options.color = opt_lift(color, facade_opt_seq)?;
        options.fill = opt_lift(fill, facade_opt_seq)?;
        options.border_width = border_width;
        options.miter_limit = miter_limit;
        options.render_mode = render_mode;
        options.rotate = rotate;
        options.morph = morph_from_py(morph)?;
        options.overlay = overlay;
        options.stroke_opacity = stroke_opacity;
        options.fill_opacity = fill_opacity;
        options.oc = oc;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_text(point, &text, &options)
        })
    }

    #[pyo3(signature = (rect, buffer, *, align=0, border_width=0.05, color=None, encoding=0, expandtabs=1, fill_opacity=1.0, fill=None, fontfile=None, fontname="helv", fontsize=11.0, lineheight=None, miter_limit=1.0, morph=None, oc=0, overlay=true, render_mode=0, rotate=0, set_simple=0, stroke_opacity=1.0))]
    #[allow(clippy::too_many_arguments)]
    fn insert_textbox(
        &mut self,
        rect: &Bound<'_, PyAny>,
        buffer: &Bound<'_, PyAny>,
        align: i32,
        border_width: f64,
        color: Option<&Bound<'_, PyAny>>,
        encoding: i32,
        expandtabs: i32,
        fill_opacity: f64,
        fill: Option<&Bound<'_, PyAny>>,
        fontfile: Option<String>,
        fontname: &str,
        fontsize: f64,
        lineheight: Option<f64>,
        miter_limit: f64,
        morph: Option<&Bound<'_, PyAny>>,
        oc: i32,
        overlay: bool,
        render_mode: i32,
        rotate: i32,
        set_simple: i32,
        stroke_opacity: f64,
    ) -> PyResult<f64> {
        let rect = rect_from_py(rect)?;
        let buffer = text_arg_from_py(buffer)?;
        let mut text = InsertTextOptions::default();
        text.fontsize = fontsize;
        text.lineheight = lineheight;
        text.fontname = fontname.to_string();
        text.fontfile = fontfile;
        text.set_simple = set_simple != 0;
        text.encoding = encoding;
        text.color = opt_lift(color, facade_opt_seq)?;
        text.fill = opt_lift(fill, facade_opt_seq)?;
        text.border_width = border_width;
        text.miter_limit = miter_limit;
        text.render_mode = render_mode;
        text.rotate = rotate;
        text.morph = morph_from_py(morph)?;
        text.overlay = overlay;
        text.stroke_opacity = stroke_opacity;
        text.fill_opacity = fill_opacity;
        text.oc = oc;
        let mut options = TextBoxOptions::default();
        options.text = text;
        options.align = align;
        options.expandtabs = expandtabs;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_textbox(rect, &buffer, &options)
        })
    }

    #[pyo3(signature = (rect, docsrc, pno=0, keep_proportion=true, overlay=true, oc=0, rotate=0, clip=None))]
    #[allow(clippy::too_many_arguments)]
    fn show_pdf_page(
        &mut self,
        rect: &Bound<'_, PyAny>,
        docsrc: PyRef<'_, PyDocument>,
        pno: i32,
        keep_proportion: bool,
        overlay: bool,
        oc: i32,
        rotate: i32,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<i32> {
        let rect = rect_from_py(rect)?;
        let src_bytes = docsrc.current_bytes()?;
        let src = open_facade(&src_bytes)?;
        let mut options = ShowPageOptions::default();
        options.page_no = pno;
        options.keep_proportion = keep_proportion;
        options.overlay = overlay;
        options.oc = oc;
        options.rotate = rotate;
        options.clip = opt_lift(clip, opt_rect)?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).show_pdf_page(rect, &src, &options)
        })
    }
}

// ---------------------------------------------------------------------------
// Document editing (T1)
// ---------------------------------------------------------------------------

#[pymethods]
impl PyDocument {
    #[pyo3(signature = (docsrc, *, from_page=-1, to_page=-1, start_at=-1, rotate=-1, links=1, annots=1, widgets=1, join_duplicates=0, show_progress=0, **kwargs))]
    #[allow(clippy::too_many_arguments)]
    fn insert_pdf(
        &mut self,
        docsrc: PyRef<'_, PyDocument>,
        from_page: i32,
        to_page: i32,
        start_at: i32,
        rotate: i32,
        links: i32,
        annots: i32,
        widgets: i32,
        join_duplicates: i32,
        show_progress: i32,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let src_bytes = docsrc.current_bytes()?;
        let src = open_facade(&src_bytes)?;
        let mut options = InsertPdfOptions::default();
        options.from_page = from_page;
        options.to_page = to_page;
        options.start_at = start_at;
        options.rotate = rotate;
        options.links = links != 0;
        options.annots = annots != 0;
        options.widgets = widgets != 0;
        options.join_duplicates = join_duplicates != 0;
        options.show_progress = show_progress != 0;
        options.finish = match kwargs {
            Some(extra) => match extra.get_item("final").ok().flatten() {
                Some(found) if !found.is_none() => found.extract::<i32>()? != 0,
                _ => true,
            },
            None => true,
        };
        mutate_doc(self, |facade| facade.insert_pdf(&src, &options))
    }

    #[pyo3(signature = (pno=-1, width=595.0, height=842.0))]
    fn new_page(
        slf: Py<Self>,
        py: Python<'_>,
        pno: i32,
        width: f64,
        height: f64,
    ) -> PyResult<PyPage> {
        let mut borrowed = slf.bind(py).borrow_mut();
        borrowed.ensure_open()?;
        mutate_doc(&mut borrowed, |facade| {
            facade.new_page(pno, width, height).map(|_| ())
        })?;
        let engine = borrowed.engine()?;
        let count = engine.page_count();
        let index = if pno < 0 {
            count.saturating_sub(1)
        } else {
            (pno as usize).min(count.saturating_sub(1))
        };
        Ok(PyPage {
            doc: engine,
            index,
            refreshed: AtomicBool::new(false),
            clip: Mutex::new(None),
            parent: slf.clone_ref(py),
            lopdf: Arc::clone(&borrowed.lopdf),
            snapshot: Arc::clone(&borrowed.raw_bytes),
        })
    }

    #[pyo3(signature = (m=None))]
    fn set_metadata(&mut self, m: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        // The recorded key set: anything else raises ValueError.
        if let Some(dict) = m.and_then(|found| found.downcast::<PyDict>().ok()) {
            for key in dict.keys() {
                let name: String = key.extract()?;
                match name.as_str() {
                    "title" | "author" | "subject" | "keywords" | "creator" | "producer"
                    | "creationDate" | "modDate" | "trapped" => {}
                    _ => {
                        return Err(PyValueError::new_err(format!(
                            "unknown metadata key {name:?}"
                        )));
                    }
                }
            }
        }
        let get = |key: &str| -> PyResult<String> {
            let found = match m {
                Some(dict) if !dict.is_none() => dict,
                _ => return Ok(String::new()),
            };
            match found.get_item(key) {
                Ok(item) if !item.is_none() => item
                    .extract::<String>()
                    .map_err(|_| PyTypeError::new_err(format!("metadata[{key:?}] must be str"))),
                _ => Ok(String::new()),
            }
        };
        let metadata = DocMetadata {
            title: get("title")?,
            author: get("author")?,
            subject: get("subject")?,
            keywords: get("keywords")?,
            creator: get("creator")?,
            producer: get("producer")?,
            creation_date: get("creationDate")?,
            mod_date: get("modDate")?,
            format: String::new(),
            encryption: None,
            trapped: get("trapped")?,
        };
        mutate_doc(self, |facade| facade.set_metadata(&metadata))
    }
}

fn points_from_py(value: &Bound<'_, PyAny>) -> PyResult<Vec<GPoint>> {
    let mut out = Vec::new();
    for item in value.try_iter()? {
        out.push(point_from_py(&item?)?);
    }
    Ok(out)
}

fn quads_from_py(value: &Bound<'_, PyAny>) -> PyResult<Vec<GQuad>> {
    let mut out = Vec::new();
    for item in value.try_iter()? {
        out.push(quad_from_py(&item?)?);
    }
    Ok(out)
}

fn opt_quads_from_py(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<GQuad>>> {
    match value {
        Some(found) if !found.is_none() => Ok(Some(quads_from_py(found)?)),
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Page annotation factories
// ---------------------------------------------------------------------------

#[pymethods]
impl PyPage {
    fn add_caret_annot(&mut self, point: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let point = point_from_py(point)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_caret_annot(point)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_circle_annot(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let rect = rect_from_py(rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_circle_annot(rect)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (point, buffer_, filename, ufilename=None, desc=None, icon=None))]
    fn add_file_annot(
        &mut self,
        point: &Bound<'_, PyAny>,
        buffer_: &Bound<'_, PyAny>,
        filename: String,
        ufilename: Option<String>,
        desc: Option<String>,
        icon: Option<String>,
    ) -> PyResult<PyAnnot> {
        let point = point_from_py(point)?;
        let data = bytes_from_py(buffer_)?;
        let options = FileAnnotOptions::new(&filename)
            .with_display_name(ufilename.as_deref().unwrap_or(&filename))
            .with_description(desc.as_deref().unwrap_or(&filename));
        let options = match icon {
            Some(name) => options.with_icon(&name),
            None => options,
        };
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_file_annot(point, &data, &options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (rect, text, *, fontsize=11.0, fontname=None, text_color=None, fill_color=None, border_color=None, border_width=0.0, dashes=None, callout=None, line_end=4, opacity=1.0, align=0, rotate=0, richtext=false, style=None))]
    #[allow(clippy::too_many_arguments)]
    fn add_freetext_annot(
        &mut self,
        rect: &Bound<'_, PyAny>,
        text: &str,
        fontsize: f64,
        fontname: Option<String>,
        text_color: Option<&Bound<'_, PyAny>>,
        fill_color: Option<&Bound<'_, PyAny>>,
        border_color: Option<&Bound<'_, PyAny>>,
        border_width: f64,
        dashes: Option<&Bound<'_, PyAny>>,
        callout: Option<&Bound<'_, PyAny>>,
        line_end: i32,
        opacity: f64,
        align: i32,
        rotate: i32,
        richtext: bool,
        style: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAnnot> {
        let _ = style;
        let rect = rect_from_py(rect)?;
        let mut options = FreeTextOptions::default();
        options.fontsize = fontsize;
        options.fontname = fontname;
        options.text_color = opt_lift(text_color, facade_opt_seq)?;
        options.fill_color = opt_lift(fill_color, facade_opt_seq)?;
        options.border_color = opt_lift(border_color, facade_opt_seq)?;
        options.border_width = border_width;
        options.dashes = opt_lift(dashes, facade_opt_seq)?;
        options.callout = match callout {
            Some(found) if !found.is_none() => Some(points_from_py(found)?),
            _ => None,
        };
        options.line_end = line_end;
        options.opacity = opacity;
        options.align = align;
        options.rotate = rotate;
        options.richtext = richtext;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_freetext_annot(rect, text, &options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (quads=None, start=None, stop=None, clip=None))]
    fn add_highlight_annot(
        &mut self,
        quads: Option<&Bound<'_, PyAny>>,
        start: Option<&Bound<'_, PyAny>>,
        stop: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAnnot> {
        let mut options = HighlightOptions::default();
        options.quads = opt_quads_from_py(quads)?;
        options.start = opt_lift(start, opt_point)?;
        options.stop = opt_lift(stop, opt_point)?;
        options.clip = opt_lift(clip, opt_rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_highlight_annot(&options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_ink_annot(&mut self, handwriting: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let mut strokes = Vec::new();
        for stroke in handwriting.try_iter()? {
            strokes.push(points_from_py(&stroke?)?);
        }
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_ink_annot(&strokes)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_line_annot(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
    ) -> PyResult<PyAnnot> {
        let p1 = point_from_py(p1)?;
        let p2 = point_from_py(p2)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_line_annot(p1, p2)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_polygon_annot(&mut self, points: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let points = points_from_py(points)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_polygon_annot(&points)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_polyline_annot(&mut self, points: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let points = points_from_py(points)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_polyline_annot(&points)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn add_rect_annot(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let rect = rect_from_py(rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_rect_annot(rect)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (quads=None, start=None, stop=None, clip=None))]
    fn add_squiggly_annot(
        &mut self,
        quads: Option<&Bound<'_, PyAny>>,
        start: Option<&Bound<'_, PyAny>>,
        stop: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAnnot> {
        let mut options = HighlightOptions::default();
        options.quads = opt_quads_from_py(quads)?;
        options.start = opt_lift(start, opt_point)?;
        options.stop = opt_lift(stop, opt_point)?;
        options.clip = opt_lift(clip, opt_rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_squiggly_annot(&options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (rect, stamp=0))]
    fn add_stamp_annot(&mut self, rect: &Bound<'_, PyAny>, stamp: i32) -> PyResult<PyAnnot> {
        let rect = rect_from_py(rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_stamp_annot(rect, stamp)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (quads=None, start=None, stop=None, clip=None))]
    fn add_strikeout_annot(
        &mut self,
        quads: Option<&Bound<'_, PyAny>>,
        start: Option<&Bound<'_, PyAny>>,
        stop: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAnnot> {
        let mut options = HighlightOptions::default();
        options.quads = opt_quads_from_py(quads)?;
        options.start = opt_lift(start, opt_point)?;
        options.stop = opt_lift(stop, opt_point)?;
        options.clip = opt_lift(clip, opt_rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_strikeout_annot(&options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (point, text, icon="Note"))]
    fn add_text_annot(
        &mut self,
        point: &Bound<'_, PyAny>,
        text: &str,
        icon: &str,
    ) -> PyResult<PyAnnot> {
        let point = point_from_py(point)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_text_annot(point, text, icon)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    #[pyo3(signature = (quads=None, start=None, stop=None, clip=None))]
    fn add_underline_annot(
        &mut self,
        quads: Option<&Bound<'_, PyAny>>,
        start: Option<&Bound<'_, PyAny>>,
        stop: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAnnot> {
        let mut options = HighlightOptions::default();
        options.quads = opt_quads_from_py(quads)?;
        options.start = opt_lift(start, opt_point)?;
        options.stop = opt_lift(stop, opt_point)?;
        options.clip = opt_lift(clip, opt_rect)?;
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_underline_annot(&options)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn delete_annot(&mut self, annot: &Bound<'_, PyAny>) -> PyResult<()> {
        let xref = if let Ok(held) = annot.extract::<PyRef<'_, PyAnnot>>() {
            held.xref
        } else {
            annot.extract::<i32>()?
        };
        let entry = AnnotEntry {
            xref,
            annot_type: AnnotType {
                code: 0,
                name: String::new(),
            },
            rect: GRect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
        };
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).delete_annot(&entry)
        })
    }
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

/// One recorded shape operation: replayed on a fresh facade per call.
///
/// The facade shape borrows its document, so it cannot live in the Python
/// object; instead every method replays the log plus its own op and
/// publishes the bytes. Results are deterministic, so replay answers each
/// call exactly as a live shape would.
enum ShapeOp {
    Bezier(GPoint, GPoint, GPoint, GPoint),
    Circle(GPoint, f64),
    Curve(GPoint, GPoint, GPoint),
    Line(GPoint, GPoint),
    Oval(GRect),
    Polyline(Vec<GPoint>),
    Quad(GQuad),
    Rect(GRect, Option<f64>),
    Sector(GPoint, GPoint, f64, bool),
    Squiggle(GPoint, GPoint, f64),
    Zigzag(GPoint, GPoint, f64),
    Finish(ShapeFinishOptions),
    Text(GPoint, TextArg, InsertTextOptions),
    Textbox(GRect, TextArg, TextBoxOptions),
    UpdateRect(GRect),
    Commit(bool),
}

enum ShapeOut {
    Point(GPoint),
    Count(i32),
    Height(f64),
    Unit,
}

fn apply_shape_op(shape: &mut FacadeShape, op: &ShapeOp) -> Result<ShapeOut, pdfluent::Error> {
    match op {
        ShapeOp::Bezier(p1, p2, p3, p4) => {
            shape.draw_bezier(*p1, *p2, *p3, *p4).map(ShapeOut::Point)
        }
        ShapeOp::Circle(center, radius) => shape.draw_circle(*center, *radius).map(ShapeOut::Point),
        ShapeOp::Curve(p1, p2, p3) => shape.draw_curve(*p1, *p2, *p3).map(ShapeOut::Point),
        ShapeOp::Line(p1, p2) => shape.draw_line(*p1, *p2).map(ShapeOut::Point),
        ShapeOp::Oval(rect) => shape.draw_oval(*rect).map(ShapeOut::Point),
        ShapeOp::Polyline(points) => shape.draw_polyline(points).map(ShapeOut::Point),
        ShapeOp::Quad(quad) => shape.draw_quad(*quad).map(ShapeOut::Point),
        ShapeOp::Rect(rect, radius) => shape.draw_rect(*rect, *radius).map(ShapeOut::Point),
        ShapeOp::Sector(center, point, beta, full) => shape
            .draw_sector(*center, *point, *beta, *full)
            .map(ShapeOut::Point),
        ShapeOp::Squiggle(p1, p2, breadth) => {
            shape.draw_squiggle(*p1, *p2, *breadth).map(ShapeOut::Point)
        }
        ShapeOp::Zigzag(p1, p2, breadth) => {
            shape.draw_zigzag(*p1, *p2, *breadth).map(ShapeOut::Point)
        }
        ShapeOp::Finish(options) => shape.finish(options).map(|()| ShapeOut::Unit),
        ShapeOp::Text(point, text, options) => shape
            .insert_text(*point, text, options)
            .map(ShapeOut::Count),
        ShapeOp::Textbox(rect, text, options) => shape
            .insert_textbox(*rect, text, options)
            .map(ShapeOut::Height),
        ShapeOp::UpdateRect(rect) => shape.update_rect(*rect).map(|()| ShapeOut::Unit),
        ShapeOp::Commit(overlay) => shape.commit(*overlay).map(|()| ShapeOut::Unit),
    }
}

/// A drawing canvas on a page: staged paths plus the mutation channel.
#[pyclass(name = "Shape")]
pub struct PyShape {
    slot: Slot,
    snapshot: Arc<Vec<u8>>,
    page_index: usize,
    log: Vec<ShapeOp>,
}

impl PyShape {
    fn exec(&mut self, op: ShapeOp) -> PyResult<ShapeOut> {
        let bytes = slot_bytes(&self.slot, self.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let mut shape = FacadeShape::new(&mut facade, self.page_index);
        for played in &self.log {
            apply_shape_op(&mut shape, played).map_err(compat_err_to_py)?;
        }
        let out = apply_shape_op(&mut shape, &op).map_err(compat_err_to_py)?;
        drop(shape);
        let saved = facade.to_bytes().map_err(compat_err_to_py)?;
        store_slot(&self.slot, &saved)?;
        self.log.push(op);
        Ok(out)
    }

    fn exec_point(&mut self, op: ShapeOp) -> PyResult<PyPoint> {
        match self.exec(op)? {
            ShapeOut::Point(found) => Ok(PyPoint::wrap(found)),
            _ => unreachable!("draw ops answer points"),
        }
    }

    fn exec_unit(&mut self, op: ShapeOp) -> PyResult<()> {
        match self.exec(op)? {
            ShapeOut::Unit => Ok(()),
            _ => unreachable!("finish/commit/update answer nothing"),
        }
    }
}

#[pymethods]
impl PyShape {
    #[new]
    fn new(page: &Bound<'_, PyAny>) -> PyResult<Self> {
        let page = page
            .extract::<PyRef<'_, PyPage>>()
            .map_err(|_| validation_error(ValidationKind::TextArg, "Shape needs a page"))?;
        Ok(Self {
            slot: Arc::clone(&page.lopdf),
            snapshot: Arc::new(page_bytes(&page)?),
            page_index: page.index,
            log: Vec::new(),
        })
    }

    /// Union of everything drawn so far: nothing on a fresh canvas.
    #[getter]
    fn rect(&self) -> PyResult<Option<PyRect>> {
        let bytes = slot_bytes(&self.slot, self.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let mut shape = FacadeShape::new(&mut facade, self.page_index);
        for played in &self.log {
            apply_shape_op(&mut shape, played).map_err(compat_err_to_py)?;
        }
        Ok(shape.rect().map(PyRect::wrap))
    }

    #[pyo3(signature = (overlay=true))]
    fn commit(&mut self, overlay: bool) -> PyResult<()> {
        self.exec_unit(ShapeOp::Commit(overlay))
    }

    fn draw_bezier(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        p3: &Bound<'_, PyAny>,
        p4: &Bound<'_, PyAny>,
    ) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Bezier(
            draw_point(p1, "draw_bezier needs points")?,
            draw_point(p2, "draw_bezier needs points")?,
            draw_point(p3, "draw_bezier needs points")?,
            draw_point(p4, "draw_bezier needs points")?,
        ))
    }

    fn draw_circle(&mut self, center: &Bound<'_, PyAny>, radius: f64) -> PyResult<PyPoint> {
        if radius < 0.0 {
            return Err(validation_error(
                ValidationKind::DrawArg,
                "draw_circle needs a non-negative radius",
            ));
        }
        self.exec_point(ShapeOp::Circle(
            draw_point(center, "draw_circle needs a center point")?,
            radius,
        ))
    }

    fn draw_curve(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        p3: &Bound<'_, PyAny>,
    ) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Curve(
            draw_point(p1, "draw_curve needs points")?,
            draw_point(p2, "draw_curve needs points")?,
            draw_point(p3, "draw_curve needs points")?,
        ))
    }

    fn draw_line(&mut self, p1: &Bound<'_, PyAny>, p2: &Bound<'_, PyAny>) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Line(
            draw_point(p1, "draw_line needs points")?,
            draw_point(p2, "draw_line needs points")?,
        ))
    }

    fn draw_oval(&mut self, tetra: &Bound<'_, PyAny>) -> PyResult<PyPoint> {
        let rect = rect_from_py(tetra)
            .or_else(|_| {
                let quad = quad_from_py(tetra)?;
                let xs = [quad.ul.x, quad.ur.x, quad.ll.x, quad.lr.x];
                let ys = [quad.ul.y, quad.ur.y, quad.ll.y, quad.lr.y];
                Ok::<GRect, PyErr>(GRect {
                    x0: xs.iter().cloned().fold(f64::INFINITY, f64::min),
                    y0: ys.iter().cloned().fold(f64::INFINITY, f64::min),
                    x1: xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    y1: ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                })
            })
            .map_err(|_| validation_error(ValidationKind::DrawArg, "draw_oval needs a rect"))?;
        self.exec_point(ShapeOp::Oval(rect))
    }

    fn draw_polyline(&mut self, points: &Bound<'_, PyAny>) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Polyline(draw_points(
            points,
            "draw_polyline needs points",
        )?))
    }

    fn draw_quad(&mut self, quad: &Bound<'_, PyAny>) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Quad(draw_quad(quad, "draw_quad needs a quad")?))
    }

    #[pyo3(signature = (rect, *, radius=None))]
    fn draw_rect(&mut self, rect: &Bound<'_, PyAny>, radius: Option<f64>) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Rect(
            draw_rect(rect, "draw_rect needs a rect")?,
            radius,
        ))
    }

    #[pyo3(signature = (center, point, beta, fullSector=true))]
    #[allow(non_snake_case)]
    fn draw_sector(
        &mut self,
        center: &Bound<'_, PyAny>,
        point: &Bound<'_, PyAny>,
        beta: f64,
        fullSector: bool,
    ) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Sector(
            draw_point(center, "draw_sector needs a center point")?,
            draw_point(point, "draw_sector needs a point")?,
            beta,
            fullSector,
        ))
    }

    #[pyo3(signature = (p1, p2, breadth=2.0))]
    fn draw_squiggle(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        breadth: f64,
    ) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Squiggle(
            draw_point(p1, "draw_squiggle needs points")?,
            draw_point(p2, "draw_squiggle needs points")?,
            breadth,
        ))
    }

    #[pyo3(signature = (p1, p2, breadth=2.0))]
    fn draw_zigzag(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        breadth: f64,
    ) -> PyResult<PyPoint> {
        self.exec_point(ShapeOp::Zigzag(
            draw_point(p1, "draw_zigzag needs points")?,
            draw_point(p2, "draw_zigzag needs points")?,
            breadth,
        ))
    }

    #[pyo3(signature = (width=1.0, color=None, fill=None, lineCap=0, lineJoin=0, dashes=None, even_odd=false, morph=None, closePath=true, fill_opacity=1.0, stroke_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn finish(
        &mut self,
        width: f64,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        lineCap: i32,
        lineJoin: i32,
        dashes: Option<String>,
        even_odd: bool,
        morph: Option<&Bound<'_, PyAny>>,
        closePath: bool,
        fill_opacity: f64,
        stroke_opacity: f64,
        oc: i32,
    ) -> PyResult<()> {
        let mut options = ShapeFinishOptions::default();
        options.width = width;
        options.color = opt_lift(color, facade_opt_seq)?.unwrap_or(vec![0.0]);
        options.fill = opt_lift(fill, facade_opt_seq)?;
        options.line_cap = lineCap;
        options.line_join = lineJoin;
        options.dashes = dashes;
        options.even_odd = even_odd;
        options.morph = morph_from_py(morph)?;
        options.close_path = closePath;
        options.fill_opacity = fill_opacity;
        options.stroke_opacity = stroke_opacity;
        options.oc = oc;
        self.exec_unit(ShapeOp::Finish(options))
    }

    fn horizontal_angle(&self, c: &Bound<'_, PyAny>, p: &Bound<'_, PyAny>) -> PyResult<f64> {
        let bytes = slot_bytes(&self.slot, self.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let shape = FacadeShape::new(&mut facade, self.page_index);
        let center = draw_point(c, "horizontal_angle needs points")?;
        let point = draw_point(p, "horizontal_angle needs points")?;
        Ok(shape.horizontal_angle(center, point).to_radians())
    }

    #[pyo3(signature = (point, buffer, *, fontsize=11.0, lineheight=None, fontname="helv", fontfile=None, set_simple=0, encoding=0, color=None, fill=None, render_mode=0, border_width=0.05, miter_limit=1.0, rotate=0, morph=None, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    fn insert_text(
        &mut self,
        point: &Bound<'_, PyAny>,
        buffer: &Bound<'_, PyAny>,
        fontsize: f64,
        lineheight: Option<f64>,
        fontname: &str,
        fontfile: Option<String>,
        set_simple: i32,
        encoding: i32,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        render_mode: i32,
        border_width: f64,
        miter_limit: f64,
        rotate: i32,
        morph: Option<&Bound<'_, PyAny>>,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<i32> {
        let mut options = InsertTextOptions::default();
        options.fontsize = fontsize;
        options.lineheight = lineheight;
        options.fontname = fontname.to_string();
        options.fontfile = fontfile;
        options.set_simple = set_simple != 0;
        options.encoding = encoding;
        options.color = opt_lift(color, facade_opt_seq)?;
        options.fill = opt_lift(fill, facade_opt_seq)?;
        options.render_mode = render_mode;
        options.border_width = border_width;
        options.miter_limit = miter_limit;
        options.rotate = rotate;
        options.morph = morph_from_py(morph)?;
        options.stroke_opacity = stroke_opacity;
        options.fill_opacity = fill_opacity;
        options.oc = oc;
        options.overlay = true;
        let op = ShapeOp::Text(point_from_py(point)?, text_arg_from_py(buffer)?, options);
        match self.exec(op)? {
            ShapeOut::Count(found) => Ok(found),
            _ => unreachable!("shape text answers a count"),
        }
    }

    #[pyo3(signature = (rect, buffer, *, align=0, border_width=0.05, color=None, encoding=0, expandtabs=1, fill_opacity=1.0, fill=None, fontfile=None, fontname="helv", fontsize=11.0, lineheight=None, miter_limit=1.0, morph=None, oc=0, render_mode=0, rotate=0, set_simple=0, stroke_opacity=1.0))]
    #[allow(clippy::too_many_arguments)]
    fn insert_textbox(
        &mut self,
        rect: &Bound<'_, PyAny>,
        buffer: &Bound<'_, PyAny>,
        align: i32,
        border_width: f64,
        color: Option<&Bound<'_, PyAny>>,
        encoding: i32,
        expandtabs: i32,
        fill_opacity: f64,
        fill: Option<&Bound<'_, PyAny>>,
        fontfile: Option<String>,
        fontname: &str,
        fontsize: f64,
        lineheight: Option<f64>,
        miter_limit: f64,
        morph: Option<&Bound<'_, PyAny>>,
        oc: i32,
        render_mode: i32,
        rotate: i32,
        set_simple: i32,
        stroke_opacity: f64,
    ) -> PyResult<f64> {
        let mut text = InsertTextOptions::default();
        text.fontsize = fontsize;
        text.lineheight = lineheight;
        text.fontname = fontname.to_string();
        text.fontfile = fontfile;
        text.set_simple = set_simple != 0;
        text.encoding = encoding;
        text.color = opt_lift(color, facade_opt_seq)?;
        text.fill = opt_lift(fill, facade_opt_seq)?;
        text.border_width = border_width;
        text.miter_limit = miter_limit;
        text.render_mode = render_mode;
        text.rotate = rotate;
        text.morph = morph_from_py(morph)?;
        text.stroke_opacity = stroke_opacity;
        text.fill_opacity = fill_opacity;
        text.oc = oc;
        text.overlay = true;
        let mut options = TextBoxOptions::default();
        options.text = text;
        options.align = align;
        options.expandtabs = expandtabs;
        let op = ShapeOp::Textbox(
            set_box(rect, "insert_textbox needs a rect")?,
            text_arg_from_py(buffer)?,
            options,
        );
        match self.exec(op)? {
            ShapeOut::Height(found) => Ok(found),
            _ => unreachable!("shape textbox answers a height"),
        }
    }

    #[allow(non_snake_case)]
    fn updateRect(&mut self, x: &Bound<'_, PyAny>) -> PyResult<()> {
        self.exec_unit(ShapeOp::UpdateRect(set_box(x, "updateRect needs a rect")?))
    }
}

#[pymethods]
impl PyPage {
    fn new_shape(&self) -> PyResult<PyShape> {
        Ok(PyShape {
            slot: Arc::clone(&self.lopdf),
            snapshot: Arc::new(page_bytes(self)?),
            page_index: self.index,
            log: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// TextWriter
// ---------------------------------------------------------------------------

/// A staged text run: appended lines flushed onto a page on demand.
#[pyclass(name = "TextWriter")]
pub struct PyTextWriter {
    inner: FacadeTextWriter,
}

#[pymethods]
impl PyTextWriter {
    #[new]
    #[pyo3(signature = (page_rect, opacity=1.0, color=None))]
    fn new(
        page_rect: &Bound<'_, PyAny>,
        opacity: f64,
        color: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut options = TextWriterOptions::default();
        options.opacity = opacity;
        options.color = opt_lift(color, facade_opt_seq)?;
        Ok(Self {
            inner: FacadeTextWriter::new(rect_from_py(page_rect)?, &options),
        })
    }

    #[pyo3(signature = (pos, text, font=None, fontsize=11.0, language=None, right_to_left=0, small_caps=0))]
    #[allow(clippy::too_many_arguments)]
    fn append(
        &mut self,
        pos: &Bound<'_, PyAny>,
        text: &str,
        font: Option<PyRef<'_, PyFont>>,
        fontsize: f64,
        language: Option<String>,
        right_to_left: i32,
        small_caps: i32,
    ) -> PyResult<(PyRect, PyPoint)> {
        let mut options = AppendOptions::default();
        options.font = font.as_ref().map(|held| &held.inner);
        options.fontsize = fontsize;
        options.language = language;
        options.right_to_left = right_to_left != 0;
        options.small_caps = small_caps != 0;
        let (rect, end) = self
            .inner
            .append(point_from_py(pos)?, text, &options)
            .map_err(compat_err_to_py)?;
        Ok((PyRect::wrap(rect), PyPoint::wrap(end)))
    }

    #[pyo3(signature = (pos, text, font=None, fontsize=11.0, language=None, small_caps=false))]
    #[allow(clippy::too_many_arguments)]
    fn appendv(
        &mut self,
        pos: &Bound<'_, PyAny>,
        text: &str,
        font: Option<PyRef<'_, PyFont>>,
        fontsize: f64,
        language: Option<String>,
        small_caps: bool,
    ) -> PyResult<(PyRect, PyPoint)> {
        let mut options = AppendOptions::default();
        options.font = font.as_ref().map(|held| &held.inner);
        options.fontsize = fontsize;
        options.language = language;
        options.small_caps = small_caps;
        let (rect, end) = self
            .inner
            .appendv(draw_point(pos, "appendv needs a point")?, text, &options)
            .map_err(compat_err_to_py)?;
        Ok((PyRect::wrap(rect), PyPoint::wrap(end)))
    }

    fn clean_rtl(&self, text: &Bound<'_, PyAny>) -> PyResult<String> {
        let text = text_param(text, "clean_rtl needs text")?;
        Ok(self.inner.clean_rtl(&text))
    }

    #[pyo3(signature = (rect, text, pos=None, font=None, fontsize=11.0, lineheight=None, align=0, warn=None, right_to_left=false, small_caps=false))]
    #[allow(clippy::too_many_arguments)]
    fn fill_textbox(
        &mut self,
        rect: &Bound<'_, PyAny>,
        text: &Bound<'_, PyAny>,
        pos: Option<&Bound<'_, PyAny>>,
        font: Option<PyRef<'_, PyFont>>,
        fontsize: f64,
        lineheight: Option<f64>,
        align: i32,
        warn: Option<bool>,
        right_to_left: bool,
        small_caps: bool,
    ) -> PyResult<Vec<(String, f64)>> {
        let mut options = FillTextBoxOptions::default();
        options.pos = opt_lift(pos, opt_point)?;
        options.font = font.as_ref().map(|held| &held.inner);
        options.fontsize = fontsize;
        options.lineheight = lineheight;
        options.align = align;
        options.warn = warn;
        options.right_to_left = right_to_left;
        options.small_caps = small_caps;
        self.inner
            .fill_textbox(rect_from_py(rect)?, &text_arg_from_py(text)?, &options)
            .map_err(compat_err_to_py)
    }

    #[pyo3(signature = (page, color=None, opacity=-1.0, overlay=1, morph=None, matrix=None, render_mode=0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    fn write_text(
        &self,
        page: PyRefMut<'_, PyPage>,
        color: Option<&Bound<'_, PyAny>>,
        opacity: f64,
        overlay: i32,
        morph: Option<&Bound<'_, PyAny>>,
        matrix: Option<&Bound<'_, PyAny>>,
        render_mode: i32,
        oc: i32,
    ) -> PyResult<()> {
        let mut options = FlushOptions::default();
        options.color = opt_lift(color, facade_opt_seq)?;
        options.opacity = if opacity < 0.0 { None } else { Some(opacity) };
        options.overlay = overlay != 0;
        options.morph = morph_from_py(morph)?;
        options.matrix = opt_lift(matrix, opt_matrix)?;
        options.render_mode = render_mode;
        options.oc = oc;
        let index = page.index;
        let mut page = page;
        mutate_page(&mut page, |facade| {
            let mut view = facade_view(facade, index);
            self.inner.write_text(&mut view, &options)
        })
    }
}

// ---------------------------------------------------------------------------
// Page annotation listings
// ---------------------------------------------------------------------------

#[pymethods]
impl PyPage {
    #[pyo3(signature = (types=None))]
    fn annots(&mut self, types: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<PyAnnot>> {
        let filter: Option<Vec<i32>> = match types {
            Some(found) if !found.is_none() => {
                let mut out = Vec::new();
                for item in found.try_iter()? {
                    out.push(item?.extract::<i32>()?);
                }
                Some(out)
            }
            _ => None,
        };
        let index = self.index;
        let entries = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.annots(filter.as_deref())
        })?;
        Ok(entries
            .into_iter()
            .map(|entry| PyAnnot::from_page(self, entry.xref))
            .collect())
    }

    #[getter]
    fn first_annot(&mut self) -> PyResult<Option<PyAnnot>> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.first_annot()
        })?;
        Ok(found.map(|entry| PyAnnot::from_page(self, entry.xref)))
    }

    fn load_annot(&mut self, ident: &Bound<'_, PyAny>) -> PyResult<PyAnnot> {
        let found = if let Ok(number) = ident.extract::<i32>() {
            AnnotIdent::Xref(number)
        } else {
            AnnotIdent::Name(ident.extract::<String>().map_err(|_| {
                PyTypeError::new_err("annot identifier must be an xref or a name".to_string())
            })?)
        };
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            view.load_annot(&found).and_then(|a| a.xref())
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

/// Live back-channel for a widget read from a page: the mutation slot plus
/// the page it belongs to. Blank-built widgets carry none.
struct WidgetLive {
    slot: Slot,
    snapshot: Arc<Vec<u8>>,
    page_index: usize,
    parent: Py<PyDocument>,
}

/// Whether a field type carries button states.
fn widget_is_button(field_type: i32) -> bool {
    field_type == PDF_WIDGET_TYPE_CHECKBOX || field_type == PDF_WIDGET_TYPE_RADIOBUTTON
}

/// Flag value as a button state name: off, or the default on-state.
fn button_value(is_button: bool, on: bool) -> String {
    if !is_button {
        return if on {
            "True".to_string()
        } else {
            "False".to_string()
        };
    }
    if on {
        "Yes".to_string()
    } else {
        "Off".to_string()
    }
}

/// A form field: blank-built, filled in, then added to a page.
#[pyclass(name = "Widget")]
pub struct PyWidget {
    inner: FacadeWidget,
    live: Option<WidgetLive>,
}

impl PyWidget {
    fn live_from_page(page: &PyPage, inner: FacadeWidget) -> Self {
        Self {
            inner,
            live: Some(WidgetLive {
                slot: Arc::clone(&page.lopdf),
                snapshot: Arc::clone(&page.snapshot),
                page_index: page.index,
                parent: Python::with_gil(|py| page.parent.clone_ref(py)),
            }),
        }
    }

    fn mutate<T>(
        &self,
        run: impl for<'a, 'b> FnOnce(
            &'a mut FacadePageMut<'b>,
            &'a FacadeWidget,
        ) -> Result<T, pdfluent::Error>,
    ) -> PyResult<T> {
        let live = self.live.as_ref().expect("widget has no live page");
        let bytes = slot_bytes(&live.slot, live.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let out = {
            let mut view = facade_view(&mut facade, live.page_index);
            run(&mut view, &self.inner).map_err(compat_err_to_py)?
        };
        let saved = facade.to_bytes().map_err(compat_err_to_py)?;
        store_slot(&live.slot, &saved)?;
        Ok(out)
    }

    /// Button state names: live appearance states, else the field value.
    fn button_state_names(&self) -> PyResult<Vec<String>> {
        let Some(live) = &self.live else {
            return self.inner.button_states().map_err(compat_err_to_py);
        };
        let bytes = slot_bytes(&live.slot, live.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let view = facade_view(&mut facade, live.page_index);
        view.widget_states(self.inner.xref)
            .map(|(states, _)| states)
            .map_err(compat_err_to_py)
    }
}

#[pymethods]
impl PyWidget {
    #[new]
    fn new() -> Self {
        Self {
            inner: FacadeWidget::new(),
            live: None,
        }
    }

    /// The backing annotation of a placed widget.
    #[getter]
    fn _annot(&self) -> PyResult<PyAnnot> {
        let live = self
            .live
            .as_ref()
            .ok_or_else(|| PyAttributeError::new_err("widget is not placed"))?;
        Ok(PyAnnot::wrap(
            self.inner.xref,
            live.page_index,
            Arc::clone(&live.slot),
            Arc::clone(&live.snapshot),
            Python::with_gil(|py| live.parent.clone_ref(py)),
        ))
    }

    #[getter]
    fn field_name(&self) -> String {
        self.inner.field_name.clone()
    }

    #[setter]
    fn set_field_name(&mut self, value: String) {
        self.inner.field_name = value;
    }

    #[getter]
    fn field_type(&self) -> i32 {
        self.inner.field_type
    }

    #[setter]
    fn set_field_type(&mut self, value: i32) {
        self.inner.field_type = value;
    }

    #[getter]
    fn field_value(&self) -> String {
        self.inner.field_value.clone()
    }

    #[setter]
    fn set_field_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(found) = value.extract::<String>() {
            self.inner.field_value = found;
            return Ok(());
        }
        let is_button = widget_is_button(self.inner.field_type);
        if let Ok(found) = value.extract::<bool>() {
            self.inner.field_value = button_value(is_button, found);
            return Ok(());
        }
        if let Ok(found) = value.extract::<i32>() {
            if is_button {
                self.inner.field_value = button_value(true, found != 0);
            } else {
                self.inner.field_value = found.to_string();
            }
            return Ok(());
        }
        Err(PyTypeError::new_err(
            "field_value must be text or a flag".to_string(),
        ))
    }

    #[getter]
    fn rect(&self) -> PyRect {
        PyRect::wrap(self.inner.rect)
    }

    #[setter]
    fn set_rect(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.rect = rect_from_py(value)?;
        Ok(())
    }

    #[getter]
    fn xref(&self) -> i32 {
        self.inner.xref
    }

    #[setter]
    fn set_xref(&mut self, value: i32) {
        self.inner.xref = value;
    }

    #[getter]
    fn field_flags(&self) -> i32 {
        self.inner.field_flags
    }

    #[setter]
    fn set_field_flags(&mut self, value: i32) {
        self.inner.field_flags = value;
    }

    #[getter]
    fn field_display(&self) -> i32 {
        self.inner.field_display
    }

    #[setter]
    fn set_field_display(&mut self, value: i32) {
        self.inner.field_display = value;
    }

    #[getter]
    fn choice_values(&self) -> Vec<String> {
        self.inner.choices.clone()
    }

    #[setter]
    fn set_choice_values(&mut self, value: Vec<String>) {
        self.inner.choices = value;
    }

    #[getter]
    fn text_font(&self) -> String {
        self.inner.text_font.clone()
    }

    #[setter]
    fn set_text_font(&mut self, value: String) {
        self.inner.text_font = value;
    }

    #[getter]
    fn text_fontsize(&self, py: Python<'_>) -> PyResult<PyObject> {
        num_to_py(
            py,
            self.inner.text_fontsize,
            self.inner.text_fontsize_is_int,
        )
    }

    #[setter]
    fn set_text_fontsize(&mut self, value: f64) {
        self.inner.text_fontsize = value;
    }

    #[getter]
    fn text_maxlen(&self) -> i32 {
        self.inner.text_maxlen
    }

    #[setter]
    fn set_text_maxlen(&mut self, value: i32) {
        self.inner.text_maxlen = value;
    }

    #[getter]
    fn border_width(&self, py: Python<'_>) -> PyResult<PyObject> {
        num_to_py(py, self.inner.border_width, self.inner.border_width_is_int)
    }

    #[setter]
    fn set_border_width(&mut self, value: f64) {
        self.inner.border_width = value;
    }

    #[getter]
    fn border_style(&self) -> String {
        self.inner.border_style.clone()
    }

    #[setter]
    fn set_border_style(&mut self, value: String) {
        self.inner.border_style = value;
    }

    #[getter]
    fn field_type_string(&self) -> String {
        self.inner.field_type_string()
    }

    fn button_states(&self, py: Python<'_>) -> PyResult<PyObject> {
        if !widget_is_button(self.inner.field_type) {
            return Ok(py.None());
        }
        let states = self.button_state_names()?;
        let out = PyDict::new(py);
        out.set_item("normal", states)?;
        out.set_item("down", py.None())?;
        Ok(out.into_any().unbind())
    }

    #[getter]
    fn next(&self) -> PyResult<Option<Self>> {
        let Some(live) = &self.live else {
            return Ok(None);
        };
        let bytes = slot_bytes(&live.slot, live.snapshot.as_ref())?;
        let mut facade = open_facade(&bytes)?;
        let widgets = {
            let view = facade_view(&mut facade, live.page_index);
            view.widgets(None).map_err(compat_err_to_py)?
        };
        let at = widgets.iter().position(|w| w.xref == self.inner.xref);
        let Some(next) = at.and_then(|i| widgets.get(i + 1)) else {
            return Ok(None);
        };
        Ok(Some(Self {
            inner: next.clone(),
            live: Some(WidgetLive {
                slot: Arc::clone(&live.slot),
                snapshot: Arc::clone(&live.snapshot),
                page_index: live.page_index,
                parent: Python::with_gil(|py| live.parent.clone_ref(py)),
            }),
        }))
    }

    fn on_state(&self, py: Python<'_>) -> PyResult<PyObject> {
        if !widget_is_button(self.inner.field_type) {
            return Ok(py.None());
        }
        let on = self
            .button_state_names()?
            .into_iter()
            .find(|state| state != "Off");
        match on {
            Some(name) => Ok(name.into_pyobject(py)?.into_any().unbind()),
            None => Ok(py.None()),
        }
    }

    /// Identity comparison: equality is identity, ordering is refused.
    fn __richcmp__(
        slf: PyRef<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: PyCompareOp,
        py: Python<'_>,
    ) -> PyResult<Py<PyAny>> {
        let same = other
            .extract::<PyRef<'_, Self>>()
            .map(|o| std::ptr::eq(slf.as_ptr(), o.as_ptr()))
            .unwrap_or(false);
        match op {
            PyCompareOp::Eq => Ok(PyBool::new(py, same).to_owned().into_any().unbind()),
            PyCompareOp::Ne => Ok(PyBool::new(py, !same).to_owned().into_any().unbind()),
            _ => Ok(py.NotImplemented()),
        }
    }

    fn reset(&mut self) -> PyResult<()> {
        if self.live.is_some() {
            return self.mutate(|view, widget| view.reset_widget(widget.xref));
        }
        self.inner.reset().map_err(compat_err_to_py)
    }

    #[pyo3(signature = (sync_flags=false))]
    fn update(&self, sync_flags: bool) -> PyResult<()> {
        let _ = sync_flags;
        if self.live.is_some() {
            return self.mutate(|view, widget| view.update_widget(widget));
        }
        self.inner.update().map_err(compat_err_to_py)
    }
}

// ---------------------------------------------------------------------------
// Page widget helpers
// ---------------------------------------------------------------------------

#[pymethods]
impl PyPage {
    fn add_widget(&mut self, widget: PyRef<'_, PyWidget>) -> PyResult<PyAnnot> {
        let index = self.index;
        let xref = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            let annot = view.add_widget(&widget.inner)?;
            annot.xref()
        })?;
        Ok(PyAnnot::from_page(self, xref))
    }

    fn delete_widget(&mut self, widget: &Bound<'_, PyAny>) -> PyResult<PyWidget> {
        let widget = widget.extract::<PyRef<'_, PyWidget>>().map_err(|_| {
            validation_error(ValidationKind::DrawArg, "delete_widget needs a widget")
        })?;
        let index = self.index;
        let removed = mutate_page(self, |facade| {
            let mut view = facade_view(facade, index);
            view.delete_widget(&widget.inner)
        })?;
        Ok(PyWidget {
            inner: removed,
            live: None,
        })
    }

    #[pyo3(signature = (types=None))]
    fn widgets(&mut self, types: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<PyWidget>> {
        let filter: Option<Vec<i32>> = match types {
            Some(found) if !found.is_none() => {
                let mut out = Vec::new();
                for item in found.try_iter()? {
                    out.push(item?.extract::<i32>()?);
                }
                Some(out)
            }
            _ => None,
        };
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.widgets(filter.as_deref())
        })?;
        Ok(found
            .into_iter()
            .map(|inner| PyWidget::live_from_page(self, inner))
            .collect())
    }

    #[getter]
    fn first_widget(&mut self) -> PyResult<Option<PyWidget>> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.first_widget()
        })?;
        Ok(found.map(|inner| PyWidget::live_from_page(self, inner)))
    }

    fn load_widget(&mut self, xref: i32) -> PyResult<PyWidget> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.load_widget(xref)
        })?;
        Ok(PyWidget::live_from_page(self, found))
    }
}

// ---------------------------------------------------------------------------
// Page queries, fonts, and the remaining draw family
// ---------------------------------------------------------------------------

#[pymethods]
impl PyPage {
    fn annot_names(&mut self) -> PyResult<Vec<String>> {
        let index = self.index;
        mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.annot_names()
        })
    }

    fn annot_xrefs(&mut self) -> PyResult<Vec<(i32, i32, String)>> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.annot_xrefs()
        })?;
        Ok(found
            .into_iter()
            .map(|entry| (entry.xref, entry.kind_code, entry.name))
            .collect())
    }

    #[pyo3(signature = (fontname="helv", fontfile=None, fontbuffer=None, set_simple=false, wmode=0, encoding=0))]
    fn insert_font(
        &mut self,
        fontname: &str,
        fontfile: Option<String>,
        fontbuffer: Option<&Bound<'_, PyAny>>,
        set_simple: bool,
        wmode: i32,
        encoding: i32,
    ) -> PyResult<i32> {
        let mut options = InsertFontOptions::default();
        options.fontname = fontname.to_string();
        options.fontfile = fontfile;
        options.fontbuffer = match fontbuffer {
            Some(found) if !found.is_none() => Some(bytes_from_py(found)?),
            _ => None,
        };
        options.set_simple = set_simple;
        options.wmode = wmode;
        options.encoding = encoding;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_font(&options)
        })
    }

    #[pyo3(signature = (p1, p2, p3, p4, color=None, fill=None, dashes=None, width=1.0, morph=None, closePath=false, lineCap=0, lineJoin=0, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_bezier(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        p3: &Bound<'_, PyAny>,
        p4: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        morph: Option<&Bound<'_, PyAny>>,
        closePath: bool,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            closePath,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let (p1, p2, p3, p4) = (
            draw_point(p1, "draw_bezier needs points")?,
            draw_point(p2, "draw_bezier needs points")?,
            draw_point(p3, "draw_bezier needs points")?,
            draw_point(p4, "draw_bezier needs points")?,
        );
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_bezier(p1, p2, p3, p4, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (center, radius, color=None, fill=None, morph=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_circle(
        &mut self,
        center: &Bound<'_, PyAny>,
        radius: f64,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        morph: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            false,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        if radius < 0.0 {
            return Err(validation_error(
                ValidationKind::DrawArg,
                "draw_circle needs a non-negative radius",
            ));
        }
        let center = draw_point(center, "draw_circle needs a center point")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_circle(center, radius, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (p1, p2, p3, color=None, fill=None, dashes=None, width=1.0, morph=None, closePath=false, lineCap=0, lineJoin=0, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_curve(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        p3: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        morph: Option<&Bound<'_, PyAny>>,
        closePath: bool,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            closePath,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let (p1, p2, p3) = (
            draw_point(p1, "draw_curve needs points")?,
            draw_point(p2, "draw_curve needs points")?,
            draw_point(p3, "draw_curve needs points")?,
        );
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_curve(p1, p2, p3, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (rect, color=None, fill=None, dashes=None, morph=None, width=1.0, lineCap=0, lineJoin=0, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_oval(
        &mut self,
        rect: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        morph: Option<&Bound<'_, PyAny>>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            false,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let rect = oval_rect_from_py(rect)?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_oval(rect, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (points, color=None, fill=None, dashes=None, width=1.0, morph=None, lineCap=0, lineJoin=0, overlay=true, closePath=false, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_polyline(
        &mut self,
        points: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        morph: Option<&Bound<'_, PyAny>>,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        closePath: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            closePath,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let points = draw_points(points, "draw_polyline needs points")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_polyline(&points, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (quad, color=None, fill=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, morph=None, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_quad(
        &mut self,
        quad: &Bound<'_, PyAny>,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        morph: Option<&Bound<'_, PyAny>>,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            false,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let quad = draw_quad(quad, "draw_quad needs a quad")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_quad(quad, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (center, point, beta, color=None, fill=None, dashes=None, fullSector=true, morph=None, width=1.0, closePath=false, lineCap=0, lineJoin=0, overlay=true, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_sector(
        &mut self,
        center: &Bound<'_, PyAny>,
        point: &Bound<'_, PyAny>,
        beta: f64,
        color: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        fullSector: bool,
        morph: Option<&Bound<'_, PyAny>>,
        width: f64,
        closePath: bool,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            fill,
            dashes,
            width,
            morph,
            closePath,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let center = draw_point(center, "draw_sector needs a center point")?;
        let point = draw_point(point, "draw_sector needs a point")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_sector(center, point, beta, fullSector, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (p1, p2, breadth=2.0, color=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, overlay=true, morph=None, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_squiggle(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        breadth: f64,
        color: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        morph: Option<&Bound<'_, PyAny>>,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            None,
            dashes,
            width,
            morph,
            false,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let p1 = draw_point(p1, "draw_squiggle needs points")?;
        let p2 = draw_point(p2, "draw_squiggle needs points")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_squiggle(p1, p2, breadth, &options)
        })
        .map(PyPoint::wrap)
    }

    #[pyo3(signature = (p1, p2, breadth=2.0, color=None, dashes=None, width=1.0, lineCap=0, lineJoin=0, overlay=true, morph=None, stroke_opacity=1.0, fill_opacity=1.0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    #[allow(non_snake_case)]
    fn draw_zigzag(
        &mut self,
        p1: &Bound<'_, PyAny>,
        p2: &Bound<'_, PyAny>,
        breadth: f64,
        color: Option<&Bound<'_, PyAny>>,
        dashes: Option<String>,
        width: f64,
        lineCap: i32,
        lineJoin: i32,
        overlay: bool,
        morph: Option<&Bound<'_, PyAny>>,
        stroke_opacity: f64,
        fill_opacity: f64,
        oc: i32,
    ) -> PyResult<PyPoint> {
        let options = draw_options_from_py(
            color,
            None,
            dashes,
            width,
            morph,
            false,
            lineCap,
            lineJoin,
            overlay,
            stroke_opacity,
            fill_opacity,
            oc,
        )?;
        let p1 = draw_point(p1, "draw_zigzag needs points")?;
        let p2 = draw_point(p2, "draw_zigzag needs points")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).draw_zigzag(p1, p2, breadth, &options)
        })
        .map(PyPoint::wrap)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_options_from_py(
    color: Option<&Bound<'_, PyAny>>,
    fill: Option<&Bound<'_, PyAny>>,
    dashes: Option<String>,
    width: f64,
    morph: Option<&Bound<'_, PyAny>>,
    close_path: bool,
    line_cap: i32,
    line_join: i32,
    overlay: bool,
    stroke_opacity: f64,
    fill_opacity: f64,
    oc: i32,
) -> PyResult<DrawOptions> {
    let mut options = DrawOptions::default();
    options.color = opt_lift(color, facade_opt_seq)?.unwrap_or(vec![0.0]);
    options.fill = opt_lift(fill, facade_opt_seq)?;
    options.dashes = dashes;
    options.width = width;
    options.morph = morph_from_py(morph)?;
    options.close_path = close_path;
    options.line_cap = line_cap;
    options.line_join = line_join;
    options.overlay = overlay;
    options.stroke_opacity = stroke_opacity;
    options.fill_opacity = fill_opacity;
    options.oc = oc;
    Ok(options)
}

fn oval_rect_from_py(value: &Bound<'_, PyAny>) -> PyResult<GRect> {
    if let Ok(rect) = rect_from_py(value) {
        return Ok(rect);
    }
    let quad = quad_from_py(value)
        .map_err(|_| validation_error(ValidationKind::DrawArg, "draw_oval needs a rect"))?;
    let xs = [quad.ul.x, quad.ur.x, quad.ll.x, quad.lr.x];
    let ys = [quad.ul.y, quad.ur.y, quad.ll.y, quad.lr.y];
    Ok(GRect {
        x0: xs.iter().cloned().fold(f64::INFINITY, f64::min),
        y0: ys.iter().cloned().fold(f64::INFINITY, f64::min),
        x1: xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        y1: ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    })
}

// ---------------------------------------------------------------------------
// Links, outlines, and document maintenance
// ---------------------------------------------------------------------------

fn link_to_py(py: Python<'_>, link: &CompatLink) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("from", PyRect::wrap(link.rect))?;
    dict.set_item("kind", link.kind)?;
    dict.set_item("xref", link.xref)?;
    dict.set_item("id", link.id.clone())?;
    if !link.uri.is_empty() {
        dict.set_item("uri", link.uri.clone())?;
    }
    if let Some(dest) = &link.dest {
        dict.set_item("page", dest.page)?;
        dict.set_item(
            "to",
            PyPoint::wrap(GPoint {
                x: dest.target.x,
                y: dest.target.y,
            }),
        )?;
        dict.set_item("zoom", dest.zoom)?;
    }
    Ok(dict.into_any().unbind())
}

fn link_from_py(value: &Bound<'_, PyAny>) -> PyResult<CompatLink> {
    let get = |key: &str| -> PyResult<Option<Bound<'_, PyAny>>> { Ok(value.get_item(key).ok()) };
    let kind: i32 = match get("kind")? {
        Some(found) => found.extract()?,
        None => {
            return Err(validation_error(
                ValidationKind::MissingKey,
                "link dict needs 'kind'",
            ));
        }
    };
    if !(0..=5).contains(&kind) {
        return Err(validation_error(
            ValidationKind::DrawArg,
            format!("unknown link kind {kind}"),
        ));
    }
    let rect = match get("from")? {
        Some(found) => rect_from_py(&found)?,
        None => {
            return Err(validation_error(
                ValidationKind::MissingKey,
                "link dict needs 'from'",
            ));
        }
    };
    let uri: String = match get("uri")? {
        Some(found) if !found.is_none() => found.extract()?,
        _ => String::new(),
    };
    let xref: i32 = match get("xref")? {
        Some(found) if !found.is_none() => found.extract().unwrap_or(0),
        _ => 0,
    };
    let id: String = match get("id")? {
        Some(found) if !found.is_none() => found.extract().unwrap_or_default(),
        _ => String::new(),
    };
    let dest = match (get("page")?, get("to")?, get("zoom")?) {
        (None, None, None) => None,
        (page, to, zoom) => {
            let page: i32 = match page {
                Some(found) if !found.is_none() => found
                    .extract()
                    .map_err(|_| PyTypeError::new_err("link 'page' must be an int".to_string()))?,
                _ => {
                    return Err(validation_error(
                        ValidationKind::DrawArg,
                        "goto link needs 'page'",
                    ));
                }
            };
            let target = match to {
                Some(found) if !found.is_none() => point_from_py(&found)?,
                _ => pdfluent::compat::geometry::Point { x: 0.0, y: 0.0 },
            };
            let zoom: f64 = match zoom {
                Some(found) if !found.is_none() => found.extract().map_err(|_| {
                    PyTypeError::new_err("link 'zoom' must be a number".to_string())
                })?,
                _ => 0.0,
            };
            Some(pdfluent::compat::document::LinkDest::new(
                page, target.x, target.y, zoom,
            ))
        }
    };
    Ok(CompatLink {
        rect,
        kind,
        uri,
        xref,
        id,
        border: Border {
            width: -1.0,
            width_is_int: true,
            style: None,
            dashes: Vec::new(),
            clouds: -1,
        },
        colors: AnnotColors::default(),
        flags: 0,
        dest,
        next: None,
    })
}

fn toc_entry_from_py(value: &Bound<'_, PyAny>) -> PyResult<TocEntry> {
    let row: Vec<Bound<'_, PyAny>> = value.try_iter()?.collect::<PyResult<_>>()?;
    if row.len() < 3 {
        return Err(PyIndexError::new_err(
            "toc rows are [level, title, page]".to_string(),
        ));
    }
    let dest = if row.len() > 3 && !row[3].is_none() {
        let info = &row[3];
        let need = |key: &str| -> PyResult<Bound<'_, PyAny>> {
            info.get_item(key)
                .map_err(|_| PyTypeError::new_err(format!("toc dest needs {key:?}")))
        };
        let target = need("to").and_then(|to| point_from_py(&to))?;
        Some(TocDest {
            kind: need("kind")?.extract()?,
            page: need("page")?.extract()?,
            target,
            zoom: need("zoom")?.extract()?,
        })
    } else {
        None
    };
    Ok(TocEntry {
        level: row[0].extract()?,
        title: row[1].extract()?,
        page: row[2].extract()?,
        dest,
    })
}

#[pymethods]
impl PyPage {
    fn get_links(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.get_links()
        })?;
        let mut out = Vec::with_capacity(found.len());
        for link in &found {
            out.push(link_to_py(py, link)?);
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }

    #[pyo3(signature = (kinds=None))]
    fn links(&mut self, py: Python<'_>, kinds: Option<&Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let filter: Option<Vec<i32>> = match kinds {
            Some(found) if !found.is_none() => {
                let mut out = Vec::new();
                for item in found.try_iter()? {
                    out.push(item?.extract::<i32>()?);
                }
                Some(out)
            }
            _ => None,
        };
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.links(filter.as_deref())
        })?;
        let mut out = Vec::with_capacity(found.len());
        for link in &found {
            out.push(link_to_py(py, link)?);
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }

    fn load_links(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.load_links()
        })?;
        let Some(first) = found.first() else {
            return Ok(py.None());
        };
        Ok(PyLink::from_compat(first)
            .into_pyobject(py)?
            .into_any()
            .unbind())
    }

    /// First link on this page, with the rest chained behind it.
    #[getter]
    fn first_link(&self) -> PyResult<Option<PyLink>> {
        let bytes = page_bytes(self)?;
        let mut facade = open_facade(&bytes)?;
        let links = facade_view(&mut facade, self.compat_index())
            .get_links()
            .map_err(compat_err_to_py)?;
        let mut head: Option<PyLink> = None;
        for link in links.iter().rev() {
            let mut wrapped = PyLink::from_facade(link);
            wrapped.set_next(head.take());
            head = Some(wrapped);
        }
        Ok(head)
    }

    #[pyo3(signature = (lnk, mark=true))]
    fn insert_link(&mut self, lnk: &Bound<'_, PyAny>, mark: bool) -> PyResult<()> {
        let link = link_from_py(lnk)?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_link(&link, mark)
        })
    }

    fn set_cropbox(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = set_box(rect, "set_cropbox needs a rect")?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_cropbox(rect))
    }
}

#[pymethods]
impl PyDocument {
    #[pyo3(signature = (*, annots=None, widgets=None))]
    fn bake(
        &mut self,
        annots: Option<&Bound<'_, PyAny>>,
        widgets: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let mut options = BakeOptions::default();
        options.annots = match annots {
            Some(found) => draw_flag(found, "bake needs boolean flags")?,
            None => true,
        };
        options.widgets = match widgets {
            Some(found) => draw_flag(found, "bake needs boolean flags")?,
            None => true,
        };
        mutate_doc(self, |facade| facade.bake(&options))
    }

    #[pyo3(signature = (toc, collapse=1))]
    fn set_toc(&mut self, toc: &Bound<'_, PyAny>, collapse: i32) -> PyResult<i32> {
        let mut entries = Vec::new();
        for row in toc.try_iter()? {
            entries.push(toc_entry_from_py(&row?)?);
        }
        mutate_doc(self, |facade| facade.set_toc(&entries, collapse))
    }

    #[pyo3(signature = (xref, text, page=None))]
    fn update_object(&mut self, xref: i32, text: &str, page: Option<i32>) -> PyResult<()> {
        mutate_doc_with(
            self,
            |facade| facade.update_object(xref, text, page),
            |e| member_error(Member::UpdateObject, e),
        )
    }

    #[pyo3(signature = (xref=0, stream=None, new=1, compress=1))]
    fn update_stream(
        &mut self,
        xref: i32,
        stream: Option<&Bound<'_, PyAny>>,
        new: i32,
        compress: i32,
    ) -> PyResult<()> {
        let data = match stream {
            Some(found) if !found.is_none() => Some(bytes_from_py(found)?),
            _ => None,
        };
        mutate_doc_with(
            self,
            |facade| facade.update_stream(xref, data.as_deref(), new != 0, compress != 0),
            |e| member_error(Member::UpdateStream, e),
        )
    }

    fn xref_set_key(&mut self, xref: i32, key: &str, value: &str) -> PyResult<()> {
        mutate_doc_with(
            self,
            |facade| facade.xref_set_key(xref, key, value),
            |e| member_error(Member::XrefSetKey, e),
        )
    }
}

// ---------------------------------------------------------------------------
// Page boxes, images, clusters, and document page maintenance
// ---------------------------------------------------------------------------

pub(crate) fn value_from_py(value: &Bound<'_, PyAny>) -> PyResult<FacadeValue> {
    if value.is_none() {
        return Ok(FacadeValue::Null);
    }
    if let Ok(found) = value.extract::<bool>() {
        return Ok(FacadeValue::Bool(found));
    }
    if let Ok(found) = value.extract::<i64>() {
        return Ok(FacadeValue::Int(found));
    }
    if let Ok(found) = value.extract::<f64>() {
        return Ok(FacadeValue::Float(found));
    }
    if let Ok(found) = value.extract::<String>() {
        return Ok(FacadeValue::Str(found));
    }
    if let Ok(raw) = value.extract::<Vec<u8>>() {
        return Ok(FacadeValue::Bytes(raw));
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let mut out = std::collections::HashMap::new();
        for (key, item) in dict.iter() {
            out.insert(key.extract::<String>()?, value_from_py(&item)?);
        }
        return Ok(FacadeValue::Dict(out));
    }
    let mut out = Vec::new();
    for item in value.try_iter()? {
        out.push(value_from_py(&item?)?);
    }
    Ok(FacadeValue::List(out))
}

#[pymethods]
impl PyPage {
    fn set_artbox(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = set_box(rect, "set_artbox needs a rect")?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_artbox(rect))
    }

    fn set_bleedbox(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = set_box(rect, "set_bleedbox needs a rect")?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_bleedbox(rect))
    }

    fn set_mediabox(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = rect_from_py(rect)?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_mediabox(rect))
    }

    /// Adopt the document language on this page.
    fn set_language(&mut self) -> PyResult<()> {
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_language(None))
    }

    /// Rewrite the content stream, dropping redundant operations.
    #[pyo3(signature = (sanitize=None))]
    fn clean_contents(&mut self, sanitize: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let sanitize = match sanitize {
            None => true,
            Some(bound) if bound.is_none() => true,
            Some(bound) => {
                if let Ok(found) = bound.extract::<bool>() {
                    found
                } else if let Ok(found) = bound.extract::<i32>() {
                    found != 0
                } else if let Ok(found) = bound.extract::<String>() {
                    !found.is_empty()
                } else {
                    return Err(validation_error(
                        ValidationKind::WrongType,
                        "clean_contents needs a flag",
                    ));
                }
            }
        };
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).clean_contents(sanitize)
        })
    }

    fn set_trimbox(&mut self, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let rect = set_box(rect, "set_trimbox needs a rect")?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).set_trimbox(rect))
    }

    fn set_contents(&mut self, xref: i32) -> PyResult<()> {
        let index = self.index;
        mutate_page_with(
            self,
            |facade| facade_view(facade, index).set_contents(xref),
            |e| member_error(Member::SetContents, e),
        )
    }

    fn set_rotation(&mut self, rotate: i32) -> PyResult<()> {
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).set_rotation(rotate)
        })
    }

    fn remove_rotation(&mut self) -> PyResult<PyCompatMatrix> {
        let index = self.index;
        let found = mutate_page(self, |facade| facade_view(facade, index).remove_rotation())?;
        Ok(PyCompatMatrix {
            a: found.a,
            b: found.b,
            c: found.c,
            d: found.d,
            e: found.e,
            f: found.f,
        })
    }

    fn delete_image(&mut self, xref: i32) -> PyResult<()> {
        let index = self.index;
        mutate_page_with(
            self,
            |facade| facade_view(facade, index).delete_image(xref),
            |e| member_error(Member::DeleteImage, e),
        )
    }

    #[pyo3(signature = (xref, *, filename=None, pixmap=None, stream=None))]
    fn replace_image(
        &mut self,
        xref: i32,
        filename: Option<String>,
        pixmap: Option<&Bound<'_, PyAny>>,
        stream: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let mut options = ReplaceImageOptions::default();
        options.filename = filename;
        options.pixmap = pixmap
            .filter(|found| !found.is_none())
            .map(pixmap_from_py)
            .transpose()?;
        options.stream = match stream {
            Some(found) if !found.is_none() => Some(bytes_from_py(found)?),
            _ => None,
        };
        let index = self.index;
        mutate_page_with(
            self,
            |facade| facade_view(facade, index).replace_image(xref, &options),
            |e| member_error(Member::ReplaceImage, e),
        )
    }

    fn delete_link(&mut self, linkdict: &Bound<'_, PyAny>) -> PyResult<()> {
        let identified = ["xref", "id"].iter().any(|key| {
            linkdict
                .get_item(key)
                .ok()
                .is_some_and(|found| !found.is_none())
        });
        if !identified {
            return Err(PyKeyError::new_err("link dict needs 'xref'"));
        }
        let link = link_from_py(linkdict)?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).delete_link(&link))
    }

    fn update_link(&mut self, lnk: &Bound<'_, PyAny>) -> PyResult<()> {
        let link = link_from_py(lnk)?;
        let index = self.index;
        mutate_page(self, |facade| facade_view(facade, index).update_link(&link))
    }

    /// Flush staged writers onto the page inside a box.
    #[pyo3(signature = (rect=None, writers=None, overlay=true, color=None, opacity=None, keep_proportion=true, rotate=0, oc=0))]
    #[allow(clippy::too_many_arguments)]
    fn write_text(
        &mut self,
        rect: Option<&Bound<'_, PyAny>>,
        writers: Option<&Bound<'_, PyAny>>,
        overlay: bool,
        color: Option<&Bound<'_, PyAny>>,
        opacity: Option<f64>,
        keep_proportion: bool,
        rotate: i32,
        oc: i32,
    ) -> PyResult<()> {
        let missing = || validation_error(ValidationKind::DrawArg, "write_text needs writers");
        let rect = match rect {
            Some(found) if !found.is_none() => rect_from_py(found)?,
            _ => return Err(missing()),
        };
        let writers = match writers {
            Some(found) if !found.is_none() => found,
            _ => return Err(missing()),
        };
        let mut staged = Vec::new();
        for item in writers.try_iter()? {
            let held = item?.extract::<PyRef<'_, PyTextWriter>>()?;
            staged.push(held.inner.staged_value());
        }
        let mut options = FacadeWriteTextOptions::default();
        options.rect = Some(rect);
        options.writers = Some(FacadeValue::List(staged));
        options.overlay = overlay;
        options.color = match color {
            Some(found) if !found.is_none() => facade_opt_seq(found)?,
            _ => None,
        };
        options.opacity = opacity;
        options.keep_proportion = keep_proportion;
        options.rotate = rotate;
        options.oc = oc;
        let index = self.index;
        mutate_page_with(
            self,
            |facade| facade_view(facade, index).write_text(&options),
            |error| member_error(Member::WriteText, error),
        )
    }

    #[pyo3(signature = (rect, text, *, css=None, scale_low=0.0, archive=None, rotate=0, oc=0, opacity=1.0, overlay=true))]
    #[allow(clippy::too_many_arguments)]
    fn insert_htmlbox(
        &mut self,
        rect: &Bound<'_, PyAny>,
        text: &str,
        css: Option<String>,
        scale_low: f64,
        archive: Option<&Bound<'_, PyAny>>,
        rotate: i32,
        oc: i32,
        opacity: f64,
        overlay: bool,
    ) -> PyResult<(f64, f64)> {
        let mut options = HtmlBoxOptions::default();
        options.css = css;
        options.scale_low = scale_low;
        options.archive = match archive {
            Some(found) if !found.is_none() => Some(value_from_py(found)?),
            _ => None,
        };
        options.rotate = rotate;
        options.oc = oc;
        options.opacity = opacity;
        options.overlay = overlay;
        let rect = set_box(rect, "insert_htmlbox needs a rect")?;
        let index = self.index;
        mutate_page(self, |facade| {
            facade_view(facade, index).insert_htmlbox(rect, text, &options)
        })
    }

    #[pyo3(signature = (clip=None, drawings=None, x_tolerance=3.0, y_tolerance=3.0, final_filter=true))]
    fn cluster_drawings(
        &mut self,
        py: Python<'_>,
        clip: Option<&Bound<'_, PyAny>>,
        drawings: Option<&Bound<'_, PyAny>>,
        x_tolerance: f64,
        y_tolerance: f64,
        final_filter: bool,
    ) -> PyResult<PyObject> {
        let mut options = ClusterOptions::default();
        options.clip = opt_lift(clip, opt_rect)?;
        options.drawings = match drawings {
            Some(found) if !found.is_none() => Some(drawings_from_py(found)?),
            _ => None,
        };
        options.x_tolerance = x_tolerance;
        options.y_tolerance = y_tolerance;
        options.final_filter = final_filter;
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.cluster_drawings(&options)
        })?;
        let mut out = Vec::with_capacity(found.len());
        for cluster in &found {
            out.push(PyRect::wrap(cluster.bbox));
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }

    #[pyo3(signature = (extended=None, callback=None, method=None))]
    fn get_cdrawings(
        &mut self,
        py: Python<'_>,
        extended: Option<bool>,
        callback: Option<&Bound<'_, PyAny>>,
        method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        if let Some(found) = callback {
            if !found.is_none() {
                return Err(FzErrorArgument::new_err(
                    "drawing callbacks are not supported".to_string(),
                ));
            }
        }
        if let Some(name) = method {
            if !name.is_none() {
                return Ok(py.None());
            }
        }
        let mut options = CdrawingsOptions::default();
        options.extended = extended.unwrap_or(false);
        options.callback = None;
        let index = self.index;
        let found = mutate_page(self, |facade| {
            let view = facade_view(facade, index);
            view.get_cdrawings(&options)
        })?;
        let mut out = Vec::with_capacity(found.len());
        for drawing in &found {
            out.push(drawing_to_py(py, drawing)?);
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }
}

#[pymethods]
impl PyDocument {
    #[pyo3(signature = (pno, to=-1))]
    fn copy_page(&mut self, pno: i32, to: i32) -> PyResult<()> {
        mutate_doc(self, |facade| facade.copy_page(pno, to))
    }

    #[pyo3(signature = (pno, to=-1))]
    fn fullcopy_page(&mut self, pno: i32, to: i32) -> PyResult<()> {
        mutate_doc(self, |facade| facade.fullcopy_page(pno, to))
    }

    #[pyo3(signature = (pno, to=-1))]
    fn move_page(&mut self, pno: i32, to: i32) -> PyResult<()> {
        mutate_doc_with(self, |facade| facade.move_page(pno, to), range_as_value)
    }

    #[pyo3(signature = (pno=-1))]
    fn delete_page(&mut self, pno: i32) -> PyResult<()> {
        mutate_doc_with(self, |facade| facade.delete_page(pno), range_as_value)
    }

    #[pyo3(signature = (*args))]
    fn delete_pages(&mut self, args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<()> {
        let items: Vec<Bound<'_, PyAny>> = args.iter().collect();
        let selection = match items.as_slice() {
            [single] => {
                if single.is_instance_of::<pyo3::types::PyString>() {
                    return Err(PyValueError::new_err(
                        "delete_pages takes page numbers, not text".to_string(),
                    ));
                }
                if let Ok(number) = single.extract::<i32>() {
                    PageSelection::List(vec![number])
                } else {
                    let mut out = Vec::new();
                    for item in single.try_iter()? {
                        out.push(item?.extract::<i32>()?);
                    }
                    PageSelection::List(out)
                }
            }
            [first, second] => PageSelection::Range(first.extract()?, second.extract()?),
            _ => {
                return Err(PyTypeError::new_err(
                    "delete_pages takes a list or a from/to pair".to_string(),
                ));
            }
        };
        mutate_doc(self, |facade| facade.delete_pages(&selection))
    }

    fn has_annots(&mut self) -> PyResult<bool> {
        mutate_doc(self, |facade| facade.has_annots())
    }

    fn has_links(&mut self) -> PyResult<bool> {
        mutate_doc(self, |facade| facade.has_links())
    }

    #[pyo3(signature = (pno, text=None, fontsize=11.0, width=595.0, height=842.0, fontname="helv", fontfile=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn insert_page(
        &mut self,
        pno: i32,
        text: Option<&Bound<'_, PyAny>>,
        fontsize: f64,
        width: f64,
        height: f64,
        fontname: &str,
        fontfile: Option<String>,
        color: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<i32> {
        let mut options = InsertPageOptions::default();
        options.text = match text {
            Some(found) if !found.is_none() => {
                if let Ok(line) = found.extract::<String>() {
                    vec![line]
                } else {
                    let mut out = Vec::new();
                    for item in found.try_iter()? {
                        out.push(item?.extract::<String>()?);
                    }
                    out
                }
            }
            _ => Vec::new(),
        };
        options.fontsize = fontsize;
        options.width = width;
        options.height = height;
        options.fontname = fontname.to_string();
        options.fontfile = fontfile;
        options.color = opt_lift(color, facade_opt_seq)?.unwrap_or(vec![0.0]);
        if pno >= 0 && pno as usize > self.page_count()? {
            return Err(validation_error(
                ValidationKind::OutOfRange,
                "insert_page needs a page number in range",
            ));
        }
        mutate_doc(self, |facade| facade.insert_page(pno, &options))
    }

    fn page_annot_xrefs(&mut self, py: Python<'_>, n: i32) -> PyResult<PyObject> {
        let rows = mutate_doc_with(
            self,
            |facade| facade.page_annot_xrefs(n),
            |e| member_error(Member::PageAnnotXrefs, e),
        )?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push((row.xref, row.kind, row.name.clone()));
        }
        Ok(out.into_pyobject(py)?.into_any().unbind())
    }

    fn repair(&mut self) -> PyResult<()> {
        mutate_doc(self, |facade| facade.repair())
    }

    fn set_markinfo(&mut self, markinfo: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(dict) = markinfo.downcast::<pyo3::types::PyDict>() else {
            return Ok(false);
        };
        if dict.is_empty() {
            mutate_doc(self, |facade| facade.set_markinfo(&FacadeValue::Null))?;
            return Ok(false);
        }
        let info = value_from_py(markinfo)?;
        mutate_doc(self, |facade| facade.set_markinfo(&info))
    }

    fn set_pagelayout(&mut self, pagelayout: &str) -> PyResult<bool> {
        mutate_doc_with(
            self,
            |facade| facade.set_pagelayout(pagelayout),
            |e| member_error(Member::SetPagelayout, e),
        )?;
        Ok(true)
    }

    fn set_pagemode(&mut self, pagemode: &str) -> PyResult<bool> {
        mutate_doc_with(
            self,
            |facade| facade.set_pagemode(pagemode),
            |e| member_error(Member::SetPagemode, e),
        )?;
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Layers, journals, embedded files, and remaining document maintenance
// ---------------------------------------------------------------------------

#[pymethods]
impl PyDocument {
    #[pyo3(signature = (name, creator=None, on=None))]
    fn add_layer(&mut self, name: &str, creator: Option<String>, on: Option<bool>) -> PyResult<()> {
        mutate_doc(self, |facade| {
            facade.add_layer(name, creator.as_deref(), on)
        })
    }

    #[pyo3(signature = (name, config=-1, on=1, intent=None, usage=None))]
    fn add_ocg(
        &mut self,
        name: &str,
        config: i32,
        on: i32,
        intent: Option<String>,
        usage: Option<String>,
    ) -> PyResult<i32> {
        mutate_doc(self, |facade| {
            facade.add_ocg(name, config, on != 0, intent.as_deref(), usage.as_deref())
        })
    }

    #[pyo3(signature = (css, append=true))]
    fn apply_css(&mut self, css: &str, append: bool) -> PyResult<()> {
        mutate_doc(self, |facade| facade.apply_css(css, append))
    }

    fn del_toc_item(&mut self, idx: i32) -> PyResult<()> {
        mutate_doc(self, |facade| facade.del_toc_item(idx))
    }

    fn del_xml_metadata(&mut self) -> PyResult<()> {
        mutate_doc(self, |facade| facade.del_xml_metadata())
    }

    #[pyo3(signature = (name, buffer_, filename=None, ufilename=None, desc=None))]
    fn embfile_add(
        &mut self,
        name: &str,
        buffer_: &Bound<'_, PyAny>,
        filename: Option<String>,
        ufilename: Option<String>,
        desc: Option<String>,
    ) -> PyResult<i32> {
        let data = bytes_from_py(buffer_)?;
        let mut options = EmbFileOptions::default();
        options.filename = filename;
        options.display_name = ufilename;
        options.description = desc;
        mutate_doc(self, |facade| facade.embfile_add(name, &data, &options))
    }

    fn embfile_del(&mut self, item: &Bound<'_, PyAny>) -> PyResult<()> {
        let item = emb_item_from_py(item)?;
        mutate_doc_with(
            self,
            |facade| facade.embfile_del(&item),
            |e| member_error(Member::EmbfileDel, e),
        )
    }

    #[pyo3(signature = (item, buffer_=None, filename=None, ufilename=None, desc=None))]
    fn embfile_upd(
        &mut self,
        item: &Bound<'_, PyAny>,
        buffer_: Option<&Bound<'_, PyAny>>,
        filename: Option<String>,
        ufilename: Option<String>,
        desc: Option<String>,
    ) -> PyResult<i32> {
        let item = emb_item_from_py(item)?;
        if let Some(found) = buffer_ {
            if !found.is_none() {
                return Err(validation_error(
                    ValidationKind::TextArg,
                    "embfile_upd takes no buffer",
                ));
            }
        }
        let data: Option<Vec<u8>> = None;
        let mut options = EmbFileOptions::default();
        options.filename = filename;
        options.display_name = ufilename;
        options.description = desc;
        mutate_doc_with(
            self,
            |facade| facade.embfile_upd(&item, data.as_deref(), &options),
            |e| member_error(Member::EmbfileUpd, e),
        )
    }

    #[pyo3(signature = (infile, from_page=-1, to_page=-1, start_at=-1, rotate=-1, links=true, annots=true, show_progress=0, **kwargs))]
    #[allow(clippy::too_many_arguments)]
    fn insert_file(
        &mut self,
        infile: &str,
        from_page: i32,
        to_page: i32,
        start_at: i32,
        rotate: i32,
        links: bool,
        annots: bool,
        show_progress: i32,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let _ = kwargs;
        let mut options = InsertPdfOptions::default();
        options.from_page = from_page;
        options.to_page = to_page;
        options.start_at = start_at;
        options.rotate = rotate;
        options.links = links;
        options.annots = annots;
        options.show_progress = show_progress != 0;
        mutate_doc_with(
            self,
            |facade| facade.insert_file(infile, &options),
            |e| member_error(Member::InsertFile, e),
        )
    }

    fn journal_can_do(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let (undo, redo) = self.journal.can_do();
        let out = PyDict::new(py);
        out.set_item("undo", undo)?;
        out.set_item("redo", redo)?;
        Ok(out.into_any().unbind())
    }

    fn journal_enable(&mut self) -> PyResult<()> {
        self.journal.enable();
        Ok(())
    }

    fn journal_is_enabled(&mut self) -> PyResult<Option<bool>> {
        Ok(self.journal.is_enabled())
    }

    fn journal_load(&mut self, filename: &str) -> PyResult<()> {
        self.journal.load(filename).map_err(compat_err_to_py)
    }

    fn journal_op_name(&mut self, step: i32) -> PyResult<Option<String>> {
        Ok(self.journal.op_name(step).map(str::to_string))
    }

    fn journal_position(&mut self) -> PyResult<(i32, i32)> {
        let (pos, count) = self.journal.position();
        Ok((pos as i32, count as i32))
    }

    fn journal_redo(&mut self) -> PyResult<bool> {
        let bytes = self.journal.redo().map_err(compat_err_to_py)?;
        publish_bytes(self, bytes)?;
        Ok(true)
    }

    fn journal_save(&mut self, filename: &str) -> PyResult<()> {
        self.journal.save(filename).map_err(compat_err_to_py)
    }

    #[pyo3(signature = (name=None))]
    fn journal_start_op(&mut self, name: Option<String>) -> PyResult<()> {
        let before = self.current_bytes()?;
        self.journal.start_op(name.as_deref(), before);
        Ok(())
    }

    fn journal_stop_op(&mut self) -> PyResult<()> {
        let after = self.current_bytes()?;
        self.journal.stop_op(after);
        Ok(())
    }

    fn journal_undo(&mut self) -> PyResult<bool> {
        let bytes = self.journal.undo().map_err(compat_err_to_py)?;
        publish_bytes(self, bytes)?;
        Ok(true)
    }

    /// Subset embedded fonts to the glyphs in use.
    ///
    /// Answers the subsetted count when `verbose` asks for it, else
    /// nothing; `fallback` keeps the unsubsetted document when subsetting
    /// fails instead of raising.
    #[pyo3(signature = (verbose=false, fallback=false))]
    fn subset_fonts(&mut self, verbose: bool, fallback: bool) -> PyResult<Option<i32>> {
        let subsetted = match mutate_doc(self, |facade| facade.subset_fonts()) {
            Ok(report) => report.fonts_subsetted,
            Err(_) if fallback => 0,
            Err(failed) => return Err(failed),
        };
        Ok(verbose.then_some(subsetted as i32))
    }

    #[pyo3(signature = (uri=None, chapters=0))]
    fn resolve_link(
        &mut self,
        py: Python<'_>,
        uri: Option<String>,
        chapters: i32,
    ) -> PyResult<PyObject> {
        let found = mutate_doc(self, |facade| facade.resolve_link(uri.as_deref(), chapters))?;
        facade_value_to_py(py, &found)
    }

    #[pyo3(signature = (*, attached_files=true, clean_pages=true, embedded_files=true, hidden_text=true, javascript=true, metadata=true, redactions=true, redact_images=0, remove_links=true, reset_fields=true, reset_responses=true, thumbnails=true, xml_metadata=true))]
    #[allow(clippy::too_many_arguments)]
    fn scrub(
        &mut self,
        attached_files: bool,
        clean_pages: bool,
        embedded_files: bool,
        hidden_text: bool,
        javascript: bool,
        metadata: bool,
        redactions: bool,
        redact_images: i32,
        remove_links: bool,
        reset_fields: bool,
        reset_responses: bool,
        thumbnails: bool,
        xml_metadata: bool,
    ) -> PyResult<()> {
        let mut options = ScrubOptions::default();
        options.attached_files = attached_files;
        options.clean_pages = clean_pages;
        options.embedded_files = embedded_files;
        options.hidden_text = hidden_text;
        options.javascript = javascript;
        options.metadata = metadata;
        options.redactions = redactions;
        options.redact_images = redact_images;
        options.remove_links = remove_links;
        options.reset_fields = reset_fields;
        options.reset_responses = reset_responses;
        options.thumbnails = thumbnails;
        options.xml_metadata = xml_metadata;
        mutate_doc(self, |facade| facade.scrub(&options))
    }

    fn select(&mut self, pyliste: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut pages = Vec::new();
        for item in pyliste.try_iter()? {
            pages.push(item?.extract::<i32>()?);
        }
        if pages.is_empty() {
            return Err(PyValueError::new_err("select needs at least one page"));
        }
        mutate_doc_with(self, |facade| facade.select(&pages), range_as_value)
    }

    #[pyo3(signature = (config, basestate=None, on=None, off=None, rbgroups=None, locked=None))]
    fn set_layer(
        &mut self,
        config: i32,
        basestate: Option<String>,
        on: Option<&Bound<'_, PyAny>>,
        off: Option<&Bound<'_, PyAny>>,
        rbgroups: Option<&Bound<'_, PyAny>>,
        locked: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let _ = (rbgroups, locked);
        let mut options = LayerOptions::default();
        options.base_state = basestate;
        options.on = int_list_from_py(on)?;
        options.off = int_list_from_py(off)?;
        mutate_doc_with(
            self,
            |facade| facade.set_layer(config, &options),
            |e| member_error(Member::SetLayer, e),
        )
    }

    #[pyo3(signature = (number, action=0))]
    fn set_layer_ui_config(&mut self, number: i32, action: i32) -> PyResult<()> {
        mutate_doc(self, |facade| facade.set_layer_ui_config(number, action))
    }

    /// Tag the document with a language identifier, or clear it.
    #[pyo3(signature = (language=None))]
    fn set_language(&mut self, language: Option<&Bound<'_, PyAny>>) -> PyResult<bool> {
        let language = match language {
            Some(found) if !found.is_none() => Some(
                found
                    .extract::<String>()
                    .map_err(|_| PyTypeError::new_err("set_language needs text".to_string()))?,
            ),
            _ => None,
        };
        mutate_doc(self, |facade| facade.set_language(language.as_deref()))
    }

    /// Bind an object to an optional-content group or membership dict.
    fn set_oc(&mut self, xref: i32, oc: i32) -> PyResult<()> {
        mutate_doc_with(
            self,
            |facade| facade.set_oc(xref, oc),
            |e| member_error(Member::SetOc, e),
        )
    }

    #[pyo3(signature = (xref=0, ocgs=None, policy=None, ve=None))]
    fn set_ocmd(
        &mut self,
        xref: i32,
        ocgs: Option<&Bound<'_, PyAny>>,
        policy: Option<String>,
        ve: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<i32> {
        let _ = ve;
        let mut options = OcmdOptions::default();
        options.ocgs = int_list_from_py(ocgs)?;
        options.policy = policy;
        mutate_doc_with(
            self,
            |facade| facade.set_ocmd(xref, &options),
            |e| member_error(Member::SetOcmd, e),
        )
    }

    fn set_page_labels(&mut self, labels: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut out = Vec::new();
        for row in labels.try_iter()? {
            out.push(page_label_from_py(&row?)?);
        }
        mutate_doc(self, |facade| facade.set_page_labels(&out))
    }

    #[pyo3(signature = (idx, dest_dict=None, kind=None, pno=None, uri=None, title=None, to=None, filename=None, zoom=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn set_toc_item(
        &mut self,
        idx: i32,
        dest_dict: Option<&Bound<'_, PyAny>>,
        kind: Option<i32>,
        pno: Option<i32>,
        uri: Option<String>,
        title: Option<String>,
        to: Option<&Bound<'_, PyAny>>,
        filename: Option<String>,
        zoom: f64,
    ) -> PyResult<()> {
        let mut options = TocItemOptions::default();
        options.kind = kind;
        options.page = pno;
        options.uri = uri;
        options.title = title;
        options.target = opt_lift(to, opt_point)?;
        options.filename = filename;
        options.zoom = zoom;
        if let Some(info) = dest_dict {
            if !info.is_none() {
                if options.kind.is_none() {
                    options.kind = info.get_item("kind").ok().and_then(|v| v.extract().ok());
                }
                if options.page.is_none() {
                    options.page = info.get_item("page").ok().and_then(|v| v.extract().ok());
                }
                if options.target.is_none() {
                    options.target = info
                        .get_item("to")
                        .ok()
                        .and_then(|v| point_from_py(&v).ok());
                }
                if options.uri.is_none() {
                    options.uri = info.get_item("uri").ok().and_then(|v| v.extract().ok());
                }
            }
        }
        mutate_doc(self, |facade| facade.set_toc_item(idx, &options))
    }

    fn set_xml_metadata(&mut self, metadata: &Bound<'_, PyAny>) -> PyResult<()> {
        let metadata = text_param(metadata, "set_xml_metadata needs text")?;
        mutate_doc(self, |facade| facade.set_xml_metadata(&metadata))
    }

    #[pyo3(signature = (source, target, *, keep=None))]
    fn xref_copy(
        &mut self,
        source: i32,
        target: i32,
        keep: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let keep: Option<Vec<String>> = match keep {
            Some(found) if !found.is_none() => {
                let mut out = Vec::new();
                for item in found.try_iter()? {
                    out.push(item?.extract::<String>()?);
                }
                Some(out)
            }
            _ => None,
        };
        mutate_doc(self, |facade| {
            facade.xref_copy(source, target, keep.as_deref())
        })
    }
}

fn emb_item_from_py(value: &Bound<'_, PyAny>) -> PyResult<EmbItem> {
    if let Ok(index) = value.extract::<i32>() {
        return Ok(EmbItem::Index(index));
    }
    Ok(EmbItem::Name(value.extract::<String>()?))
}

fn int_list_from_py(value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<i32>> {
    match value {
        Some(found) if !found.is_none() => {
            let mut out = Vec::new();
            for item in found.try_iter()? {
                out.push(item?.extract::<i32>()?);
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

fn page_label_from_py(value: &Bound<'_, PyAny>) -> PyResult<PageLabel> {
    let get = |keys: &[&str]| -> Option<Bound<'_, PyAny>> {
        keys.iter().find_map(|key| value.get_item(key).ok())
    };
    let start_page: i32 = get(&["startpage", "start_page"])
        .and_then(|v| v.extract().ok())
        .unwrap_or(0);
    let prefix: String = get(&["prefix"])
        .and_then(|v| v.extract().ok())
        .unwrap_or_default();
    let first_number: i32 = get(&["firstpagenum", "first_number"])
        .and_then(|v| v.extract().ok())
        .unwrap_or(1);
    let style: String = get(&["style"])
        .and_then(|v| v.extract().ok())
        .unwrap_or_default();
    Ok(PageLabel {
        start_page,
        prefix,
        first_number,
        style,
    })
}
