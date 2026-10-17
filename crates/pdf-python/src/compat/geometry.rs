//! Compatible value types (lane P0): geometry classes shared by every lane.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Five mutable value classes over the core geometry in `pdfluent::compat`:
//! [`Rect`], [`IRect`], [`Point`], [`Matrix`] and [`Quad`]. The core owns the
//! arithmetic; this file owns the calling conventions the recordings pin
//! down: which shapes each parameter accepts, which failure each shape
//! raises, and the exact int-vs-float-vs-bool of every return.

use pdfluent::compat::{
    AreaUnit, IRect as CoreIRect, Matrix as CoreMatrix, Point as CorePoint, Quad as CoreQuad,
    Rect as CoreRect,
};
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{
    PyAssertionError, PyAttributeError, PyException, PyIndexError, PyKeyError, PyTypeError,
    PyValueError, PyZeroDivisionError,
};
use pyo3::prelude::*;
use pyo3::types::{PyFloat, PyInt, PyList, PySlice, PyTuple};

/// A Python number (int, float or bool) read as a float, if it is one.
fn num_opt(value: &Bound<'_, PyAny>) -> Option<f64> {
    value.extract::<f64>().ok()
}

/// A Python int (or bool, never a float) read as an int, if it is one.
fn int_opt(value: &Bound<'_, PyAny>) -> Option<i32> {
    value.extract::<i32>().ok()
}

/// Items of a Python list or tuple, if `value` is one.
fn seq_opt<'a>(value: &'a Bound<'a, PyAny>) -> Option<Vec<Bound<'a, PyAny>>> {
    if let Ok(tuple) = value.downcast::<PyTuple>() {
        Some(tuple.iter().collect())
    } else if let Ok(list) = value.downcast::<PyList>() {
        Some(list.iter().collect())
    } else {
        None
    }
}

/// The inner value of our own class, if `value` is one. A failed borrow reads
/// as absent rather than panicking, so an object passed to its own mutating
/// method answers the context's fallthrough error instead of crashing.
fn as_rect(value: &Bound<'_, PyAny>) -> Option<CoreRect> {
    value
        .downcast::<Rect>()
        .ok()
        .and_then(|r| r.try_borrow().ok().map(|b| b.inner))
}

/// The inner value of our own class, if `value` is one.
fn as_irect(value: &Bound<'_, PyAny>) -> Option<CoreIRect> {
    value
        .downcast::<IRect>()
        .ok()
        .and_then(|r| r.try_borrow().ok().map(|b| b.inner))
}

/// The inner value of our own class, if `value` is one.
fn as_point(value: &Bound<'_, PyAny>) -> Option<CorePoint> {
    value
        .downcast::<Point>()
        .ok()
        .and_then(|p| p.try_borrow().ok().map(|b| b.inner))
}

/// The inner value of our own class, if `value` is one.
fn as_matrix(value: &Bound<'_, PyAny>) -> Option<CoreMatrix> {
    value
        .downcast::<Matrix>()
        .ok()
        .and_then(|m| m.try_borrow().ok().map(|b| b.inner))
}

/// The inner value of our own class, if `value` is one.
fn as_quad(value: &Bound<'_, PyAny>) -> Option<CoreQuad> {
    value
        .downcast::<Quad>()
        .ok()
        .and_then(|q| q.try_borrow().ok().map(|b| b.inner))
}

/// Exactly `n` numbers from a list or tuple, if `value` is one.
fn numbers_opt(value: &Bound<'_, PyAny>, n: usize) -> Option<Vec<f64>> {
    let items = seq_opt(value)?;
    if items.len() != n {
        return None;
    }
    items.iter().map(num_opt).collect()
}

/// A point from a `Point` or a two-number list/tuple, if `value` is one.
fn point_opt(value: &Bound<'_, PyAny>) -> Option<CorePoint> {
    if let Some(point) = as_point(value) {
        return Some(point);
    }
    numbers_opt(value, 2).map(|pair| CorePoint::new(pair[0], pair[1]))
}

/// A float rectangle from a `Rect`, an `IRect`, a four-number list/tuple or a
/// two-point list/tuple, if `value` is one.
fn rect_opt(value: &Bound<'_, PyAny>) -> Option<CoreRect> {
    if let Some(rect) = as_rect(value) {
        return Some(rect);
    }
    if let Some(rect) = as_irect(value) {
        return Some(rect.rect());
    }
    let items = seq_opt(value)?;
    if items.len() == 4 {
        let mut edges = [0.0; 4];
        for (i, item) in items.iter().enumerate() {
            edges[i] = num_opt(item)?;
        }
        return Some(CoreRect::new(edges[0], edges[1], edges[2], edges[3]));
    }
    if items.len() == 2 {
        let p0 = point_opt(&items[0])?;
        let p1 = point_opt(&items[1])?;
        return Some(CoreRect::new(p0.x, p0.y, p1.x, p1.y));
    }
    None
}

/// One integer edge from a float: floors the top-left, ceils the
/// bottom-right, then clamps to the range the core conversion keeps.
fn float_edge(value: f64, low: bool) -> i32 {
    let rounded = if low { value.floor() } else { value.ceil() };
    rounded.clamp(-16_777_216.0, 16_777_216.0) as i32
}

/// One integer edge from a number: exact for ints, outward for floats.
fn irect_number(value: &Bound<'_, PyAny>, low: bool) -> Option<i32> {
    if let Some(exact) = int_opt(value) {
        return Some(exact);
    }
    num_opt(value).map(|v| float_edge(v, low))
}

/// An integer rectangle from an `IRect`, a `Rect`, a four-number list/tuple or
/// a two-point list/tuple, if `value` is one.
fn irect_opt(value: &Bound<'_, PyAny>) -> Option<CoreIRect> {
    if let Some(rect) = as_irect(value) {
        return Some(rect);
    }
    if let Some(rect) = as_rect(value) {
        return Some(rect.irect());
    }
    let items = seq_opt(value)?;
    if items.len() == 4 {
        return Some(CoreIRect::new(
            irect_number(&items[0], true)?,
            irect_number(&items[1], true)?,
            irect_number(&items[2], false)?,
            irect_number(&items[3], false)?,
        ));
    }
    if items.len() == 2 {
        let p0 = point_opt(&items[0])?;
        let p1 = point_opt(&items[1])?;
        return Some(CoreIRect::new(
            float_edge(p0.x, true),
            float_edge(p0.y, true),
            float_edge(p1.x, false),
            float_edge(p1.y, false),
        ));
    }
    None
}

/// A matrix from a `Matrix` or a six-number list/tuple, if `value` is one.
fn matrix_opt(value: &Bound<'_, PyAny>) -> Option<CoreMatrix> {
    if let Some(matrix) = as_matrix(value) {
        return Some(matrix);
    }
    numbers_opt(value, 6).map(|c| CoreMatrix::new(c[0], c[1], c[2], c[3], c[4], c[5]))
}

/// A quad from a `Quad` or a four-point list/tuple, if `value` is one.
fn quad_opt(value: &Bound<'_, PyAny>) -> Option<CoreQuad> {
    if let Some(quad) = as_quad(value) {
        return Some(quad);
    }
    let items = seq_opt(value)?;
    if items.len() != 4 {
        return None;
    }
    let mut corners = [CorePoint::new(0.0, 0.0); 4];
    for (i, item) in items.iter().enumerate() {
        corners[i] = point_opt(item)?;
    }
    Some(CoreQuad::new(
        corners[0], corners[1], corners[2], corners[3],
    ))
}

/// Whether the matrix has an inverse.
fn invertible(matrix: CoreMatrix) -> bool {
    matrix.a * matrix.d - matrix.b * matrix.c != 0.0
}

/// Rotation matrix for `theta` degrees.
fn rotation(theta: f64) -> CoreMatrix {
    let angle = theta.to_radians();
    CoreMatrix::new(
        angle.cos(),
        angle.sin(),
        -angle.sin(),
        angle.cos(),
        0.0,
        0.0,
    )
}

/// A matrix argument that must be a `Matrix` or a six-number list/tuple.
/// Numbers answer the type error the recordings show; other shapes the value
/// error, since they parse as sequences of the wrong length.
fn strict_matrix(value: &Bound<'_, PyAny>) -> PyResult<CoreMatrix> {
    if let Some(matrix) = matrix_opt(value) {
        return Ok(matrix);
    }
    if num_opt(value).is_some() || value.is_none() {
        return Err(PyTypeError::new_err("expected a matrix"));
    }
    Err(PyValueError::new_err("expected a matrix of six numbers"))
}

/// An int as an untyped Python object.
fn any_int(py: Python<'_>, value: i32) -> Py<PyAny> {
    PyInt::new(py, value).into_any().unbind()
}

/// A float as an untyped Python object.
fn any_float(py: Python<'_>, value: f64) -> Py<PyAny> {
    PyFloat::new(py, value).into_any().unbind()
}

/// A bool as an untyped Python object.
fn any_bool(py: Python<'_>, value: bool) -> PyResult<Py<PyAny>> {
    Ok(value.into_bound_py_any(py)?.unbind())
}

/// A list-style index over `len` items: negatives wrap, the rest must fit.
fn get_index(what: &str, index: isize, len: usize) -> PyResult<usize> {
    let len = len as isize;
    let at = if index < 0 { index + len } else { index };
    if at < 0 || at >= len {
        return Err(PyIndexError::new_err(format!("{what} index out of range")));
    }
    Ok(at as usize)
}

/// An assignment index over `len` items: no wrapping, the recordings fail
/// every negative one.
fn set_index(what: &str, index: isize, len: usize) -> PyResult<usize> {
    if index < 0 || index >= len as isize {
        return Err(PyIndexError::new_err(format!("{what} index out of range")));
    }
    Ok(index as usize)
}

/// Indices of a slice over `len` items, in iteration order.
fn slice_range(slice: &Bound<'_, PySlice>, len: usize) -> PyResult<Vec<usize>> {
    let at = slice.indices(len as isize)?;
    let mut out = Vec::new();
    if at.step > 0 {
        let mut i = at.start;
        while i < at.stop {
            out.push(i as usize);
            i += at.step;
        }
    } else if at.step < 0 {
        let mut i = at.start;
        while i > at.stop {
            out.push(i as usize);
            i += at.step;
        }
    }
    Ok(out)
}

/// The area unit named by the first `get_area` argument, if any. A missing
/// argument reads as pixels; extra arguments are ignored.
fn area_unit(arg: Option<&Bound<'_, PyAny>>) -> PyResult<AreaUnit> {
    let Some(arg) = arg else {
        return Ok(AreaUnit::Pixels);
    };
    if let Ok(name) = arg.extract::<String>() {
        return match name.as_str() {
            "px" => Ok(AreaUnit::Pixels),
            "in" => Ok(AreaUnit::Inches),
            "mm" => Ok(AreaUnit::Millimetres),
            "cm" => Ok(AreaUnit::Centimetres),
            _ => Err(PyKeyError::new_err(format!("unknown area unit: {name}"))),
        };
    }
    // Mirror a dict lookup: hashable misses raise KeyError, unhashable keys
    // raise the TypeError their own hash does.
    match arg.hash() {
        Ok(_) => Err(PyKeyError::new_err("unknown area unit")),
        Err(error) => Err(error),
    }
}

/// A keyword counts only when given a non-None value.
fn given<'a>(value: Option<&'a Bound<'a, PyAny>>) -> Option<&'a Bound<'a, PyAny>> {
    match value {
        Some(bound) if bound.is_none() => None,
        other => other,
    }
}

/// A rectangle: two diagonally opposite corners in points, mutable.
#[pyclass]
pub struct Rect {
    inner: CoreRect,
}

#[pymethods]
impl Rect {
    #[new]
    #[pyo3(signature = (*args, p0=None, p1=None, x0=None, y0=None, x1=None, y1=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        args: &Bound<'_, PyTuple>,
        p0: Option<&Bound<'_, PyAny>>,
        p1: Option<&Bound<'_, PyAny>>,
        x0: Option<&Bound<'_, PyAny>>,
        y0: Option<&Bound<'_, PyAny>>,
        x1: Option<&Bound<'_, PyAny>>,
        y1: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (p0, p1, x0, y0, x1, y1) = (
            given(p0),
            given(p1),
            given(x0),
            given(y0),
            given(x1),
            given(y1),
        );
        if p0.is_some()
            || p1.is_some()
            || x0.is_some()
            || y0.is_some()
            || x1.is_some()
            || y1.is_some()
        {
            if args.len() != 0 {
                return Err(PyTypeError::new_err(
                    "cannot mix positional and keyword edges",
                ));
            }
            let mut edges = [0.0; 4];
            for (i, keyword) in [x0, y0, x1, y1].iter().enumerate() {
                if let Some(value) = keyword {
                    let Some(number) = num_opt(value) else {
                        return Err(PyTypeError::new_err(
                            "rectangle keyword edges must be numbers",
                        ));
                    };
                    edges[i] = number;
                }
            }
            if let Some(value) = p0 {
                let Some(point) = point_opt(value) else {
                    return Err(PyTypeError::new_err(
                        "rectangle corner keywords must be points",
                    ));
                };
                edges[0] = point.x;
                edges[1] = point.y;
            }
            if let Some(value) = p1 {
                let Some(point) = point_opt(value) else {
                    return Err(PyTypeError::new_err(
                        "rectangle corner keywords must be points",
                    ));
                };
                edges[2] = point.x;
                edges[3] = point.y;
            }
            return Ok(Self {
                inner: CoreRect::new(edges[0], edges[1], edges[2], edges[3]),
            });
        }
        match args.len() {
            0 => Ok(Self {
                inner: CoreRect::new(0.0, 0.0, 0.0, 0.0),
            }),
            1 => {
                let one = args.get_item(0)?;
                if let Some(rect) = rect_opt(&one) {
                    return Ok(Self { inner: rect });
                }
                Err(PyAssertionError::new_err(
                    "cannot build a rectangle from this",
                ))
            }
            2 => {
                let first = point_opt(&args.get_item(0)?);
                let second = point_opt(&args.get_item(1)?);
                match (first, second) {
                    (Some(a), Some(b)) => Ok(Self {
                        inner: CoreRect::new(a.x, a.y, b.x, b.y),
                    }),
                    _ => Err(PyTypeError::new_err(
                        "two-argument rectangles need two points",
                    )),
                }
            }
            4 => {
                let mut edges = [0.0; 4];
                for (i, edge) in edges.iter_mut().enumerate() {
                    let Some(number) = num_opt(&args.get_item(i)?) else {
                        return Err(PyTypeError::new_err(
                            "four-argument rectangles need four numbers",
                        ));
                    };
                    *edge = number;
                }
                Ok(Self {
                    inner: CoreRect::new(edges[0], edges[1], edges[2], edges[3]),
                })
            }
            n => Err(PyException::new_err(format!(
                "cannot build a rectangle from {n} arguments"
            ))),
        }
    }

    /// Left edge, in points.
    #[getter]
    fn x0(&self) -> f64 {
        self.inner.x0
    }

    #[setter]
    fn set_x0(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("rectangle edges must be numbers"));
        };
        self.inner.x0 = number;
        Ok(())
    }

    /// Top edge, in points.
    #[getter]
    fn y0(&self) -> f64 {
        self.inner.y0
    }

    #[setter]
    fn set_y0(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("rectangle edges must be numbers"));
        };
        self.inner.y0 = number;
        Ok(())
    }

    /// Right edge, in points.
    #[getter]
    fn x1(&self) -> f64 {
        self.inner.x1
    }

    #[setter]
    fn set_x1(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("rectangle edges must be numbers"));
        };
        self.inner.x1 = number;
        Ok(())
    }

    /// Bottom edge, in points.
    #[getter]
    fn y1(&self) -> f64 {
        self.inner.y1
    }

    #[setter]
    fn set_y1(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("rectangle edges must be numbers"));
        };
        self.inner.y1 = number;
        Ok(())
    }

    /// Area of the rectangle.
    fn __abs__(&self) -> f64 {
        self.inner.abs()
    }

    /// Shift every edge by a number, or add two rectangles edge by edge.
    fn __add__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(p) {
            return Ok(Self {
                inner: self.inner + number,
            });
        }
        if let Some(other) = rect_edges_opt(p) {
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        Err(PyValueError::new_err("cannot add a rectangle and this"))
    }

    /// Whether any edge is nonzero.
    fn __bool__(&self) -> bool {
        self.inner.to_bool()
    }

    /// Whether a point, rectangle or quad lies inside this rectangle.
    fn __contains__(&self, x: &Bound<'_, PyAny>) -> PyResult<bool> {
        if let Some(point) = point_opt(x) {
            return Ok(self.inner.contains_point(point) != 0);
        }
        if let Some(quad) = quad_opt(x) {
            return Ok(self.inner.contains_quad(quad));
        }
        if let Some(rect) = rect_opt(x) {
            return Ok(self.inner.contains_rect(rect));
        }
        Ok(false)
    }

    /// Edge-by-edge equality against rectangles and four-number sequences.
    fn __eq__(&self, rect: &Bound<'_, PyAny>) -> PyResult<bool> {
        if rect.is_instance_of::<Quad>() {
            return Err(PyTypeError::new_err(
                "cannot compare a rectangle with a quadrilateral",
            ));
        }
        Ok(rect_opt(rect).is_some_and(|r| r == self.inner))
    }

    /// Rectangles have no ordering.
    fn __ge__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("rectangles have no ordering"))
    }

    /// One edge by index, or a list of edges for a slice.
    fn __getitem__(&self, py: Python<'_>, i: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let edges = [self.inner.x0, self.inner.y0, self.inner.x1, self.inner.y1];
        if let Ok(index) = i.extract::<isize>() {
            let at = get_index("rectangle", index, 4)?;
            return Ok(any_float(py, edges[at]));
        }
        if let Ok(slice) = i.downcast::<PySlice>() {
            let items: Vec<f64> = slice_range(slice, 4)?.iter().map(|&at| edges[at]).collect();
            return Ok(PyList::new(py, items)?.into_any().unbind());
        }
        Err(PyTypeError::new_err(
            "rectangle indices must be integers or slices",
        ))
    }

    /// Rectangles have no ordering.
    fn __gt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("rectangles have no ordering"))
    }

    /// Rectangles have no ordering.
    fn __le__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("rectangles have no ordering"))
    }

    /// Edge count, always four.
    fn __len__(&self) -> usize {
        4
    }

    /// Rectangles have no ordering.
    fn __lt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("rectangles have no ordering"))
    }

    /// Scale every edge by a number, or move the rectangle by a matrix.
    fn __mul__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            return Ok(Self {
                inner: self.inner * number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner * matrix,
            });
        }
        if as_rect(m).is_some() || as_irect(m).is_some() {
            return Err(PyException::new_err("cannot multiply two rectangles"));
        }
        Err(PyValueError::new_err("cannot multiply a rectangle by this"))
    }

    /// Edge-by-edge inequality against rectangles and four-number sequences.
    fn __ne__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        if value.is_instance_of::<Quad>() {
            return Err(PyTypeError::new_err(
                "cannot compare a rectangle with a quadrilateral",
            ));
        }
        Ok(!rect_opt(value).is_some_and(|r| r == self.inner))
    }

    /// Negated copy of the rectangle.
    fn __neg__(&self) -> Self {
        Self { inner: -self.inner }
    }

    /// Copy of the rectangle.
    fn __pos__(&self) -> Self {
        Self { inner: self.inner }
    }

    /// Set one edge by index.
    fn __setitem__(&mut self, i: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        if i.downcast::<PySlice>().is_ok() {
            return Err(PyTypeError::new_err(
                "rectangle edges cannot be assigned by slice",
            ));
        }
        let Ok(index) = i.extract::<isize>() else {
            return Err(PyTypeError::new_err("rectangle indices must be integers"));
        };
        let at = set_index("rectangle", index, 4)?;
        let Some(number) = num_opt(v) else {
            return Err(PyValueError::new_err("rectangle edges must be numbers"));
        };
        match at {
            0 => self.inner.x0 = number,
            1 => self.inner.y0 = number,
            2 => self.inner.x1 = number,
            _ => self.inner.y1 = number,
        }
        Ok(())
    }

    /// Shift every edge by a number, or subtract two rectangles edge by edge.
    fn __sub__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(p) {
            return Ok(Self {
                inner: self.inner - number,
            });
        }
        if let Some(other) = rect_edges_opt(p) {
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        Err(PyValueError::new_err(
            "cannot subtract this from a rectangle",
        ))
    }

    /// Divide every edge by a number, or move the rectangle by the inverse of
    /// a matrix. Anything else answers a copy of the rectangle.
    fn __truediv__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            if number == 0.0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            return Ok(Self {
                inner: self.inner / number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            if !invertible(matrix) {
                return Err(PyZeroDivisionError::new_err(
                    "division by a singular matrix",
                ));
            }
            return Ok(Self {
                inner: self.inner / matrix,
            });
        }
        Ok(Self { inner: self.inner })
    }

    /// Overlap of two rectangles, as a new rectangle.
    fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(rect) = rect_edges_opt(other) {
            return Ok(Self {
                inner: self.inner & rect,
            });
        }
        Err(PyValueError::new_err(
            "cannot intersect a rectangle with this",
        ))
    }

    /// Cover of two rectangles, as a new rectangle.
    fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(rect) = rect_edges_opt(other) {
            return Ok(Self {
                inner: self.inner | rect,
            });
        }
        Err(PyValueError::new_err("cannot join a rectangle with this"))
    }

    /// Rectangles are mutable and unhashable.
    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'Rect'"))
    }

    /// Bottom-left corner.
    #[getter]
    fn bl(&self) -> Point {
        Point {
            inner: self.inner.bl(),
        }
    }

    /// Bottom-left corner.
    #[getter]
    fn bottom_left(&self) -> Point {
        self.bl()
    }

    /// Bottom-right corner.
    #[getter]
    fn bottom_right(&self) -> Point {
        Point {
            inner: self.inner.bottom_right(),
        }
    }

    /// Bottom-right corner.
    #[getter]
    fn br(&self) -> Point {
        self.bottom_right()
    }

    /// Top-left corner.
    #[getter]
    fn tl(&self) -> Point {
        Point {
            inner: self.inner.tl(),
        }
    }

    /// Top-left corner.
    #[getter]
    fn top_left(&self) -> Point {
        self.tl()
    }

    /// Top-right corner.
    #[getter]
    fn top_right(&self) -> Point {
        Point {
            inner: self.inner.top_right(),
        }
    }

    /// Top-right corner.
    #[getter]
    fn tr(&self) -> Point {
        self.top_right()
    }

    /// Whether this covers a point (1 or 0), a rectangle or a quad (bool).
    #[pyo3(signature = (x))]
    fn contains(&self, py: Python<'_>, x: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if let Some(point) = point_opt(x) {
            return Ok(any_int(py, self.inner.contains_point(point)));
        }
        if let Some(quad) = quad_opt(x) {
            return any_bool(py, self.inner.contains_quad(quad));
        }
        if let Some(rect) = rect_opt(x) {
            return any_bool(py, self.inner.contains_rect(rect));
        }
        any_bool(py, false)
    }

    /// Area in square pixels, inches, millimetres or centimetres.
    #[pyo3(signature = (*args))]
    fn get_area(&self, args: &Bound<'_, PyTuple>) -> PyResult<f64> {
        let first = args.get_item(0).ok();
        Ok(self.inner.get_area_in(area_unit(first.as_ref())?))
    }

    /// Height in points.
    #[getter]
    fn height(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.inner.y1 <= self.inner.y0 {
            Ok(any_int(py, 0))
        } else {
            Ok(any_float(py, self.inner.height()))
        }
    }

    /// Width in points.
    #[getter]
    fn width(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.inner.x1 <= self.inner.x0 {
            Ok(any_int(py, 0))
        } else {
            Ok(any_float(py, self.inner.width()))
        }
    }

    /// Grow in place to cover a point, and return the grown rectangle.
    #[pyo3(signature = (p))]
    fn include_point(&mut self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        let Some(point) = point_opt(p) else {
            return Err(PyValueError::new_err("include_point needs a point"));
        };
        self.inner.include_point(point);
        Ok(Self { inner: self.inner })
    }

    /// Grow in place to cover another rectangle, and return it.
    #[pyo3(signature = (r))]
    fn include_rect(&mut self, r: &Bound<'_, PyAny>) -> PyResult<Self> {
        let Some(other) = rect_opt(r) else {
            return Err(PyValueError::new_err("include_rect needs a rectangle"));
        };
        self.inner.include_rect(other);
        Ok(Self { inner: self.inner })
    }

    /// Shrink in place to the overlap with another rectangle, and return it.
    #[pyo3(signature = (r))]
    fn intersect(&mut self, r: &Bound<'_, PyAny>) -> PyResult<Self> {
        let Some(other) = rect_opt(r) else {
            return Err(PyValueError::new_err("intersect needs a rectangle"));
        };
        self.inner.intersect(other);
        Ok(Self { inner: self.inner })
    }

    /// Whether this overlaps another rectangle.
    #[pyo3(signature = (x))]
    fn intersects(&self, x: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Some(other) = rect_opt(x) else {
            return Err(PyAssertionError::new_err("intersects needs a rectangle"));
        };
        Ok(self.inner.intersects(other))
    }

    /// Integer copy of the rectangle.
    #[getter]
    fn irect(&self) -> IRect {
        IRect {
            inner: self.inner.irect(),
        }
    }

    /// Whether the rectangle is empty.
    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Whether the rectangle is infinite.
    #[getter]
    fn is_infinite(&self) -> bool {
        self.inner.is_infinite()
    }

    /// Whether the rectangle is valid.
    #[getter]
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    /// Quadrilateral of the corners moved by a matrix around a fixed point.
    #[pyo3(signature = (p, m))]
    fn morph(&self, p: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> PyResult<Quad> {
        let Some(point) = as_point(p) else {
            return Err(PyAttributeError::new_err(
                "morph needs a point with x and y",
            ));
        };
        let matrix = strict_matrix(m)?;
        Ok(Quad {
            inner: self.inner.morph(point, matrix),
        })
    }

    /// Euclidean norm of the edges.
    fn norm(&self) -> f64 {
        self.inner.norm()
    }

    /// Order the edges in place, and return the ordered rectangle.
    fn normalize(&mut self) -> Self {
        self.inner.normalize();
        Self { inner: self.inner }
    }

    /// Quadrilateral over the four corners.
    #[getter]
    fn quad(&self) -> Quad {
        Quad {
            inner: self.inner.quad(),
        }
    }

    /// Integer copy with rounded edges.
    fn round(&self) -> IRect {
        IRect {
            inner: self.inner.round(),
        }
    }

    /// Matrix mapping this rectangle onto another one.
    #[pyo3(signature = (r))]
    fn torect(&self, r: &Bound<'_, PyAny>) -> PyResult<Matrix> {
        let Some(target) = rect_opt(r) else {
            return Err(PyAssertionError::new_err("torect needs a rectangle"));
        };
        Ok(Matrix {
            inner: self.inner.torect(target),
        })
    }

    /// Move in place by a matrix, and return the moved rectangle.
    #[pyo3(signature = (m))]
    fn transform(&mut self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        let matrix = strict_matrix(m)?;
        self.inner.transform(matrix);
        Ok(Self { inner: self.inner })
    }
}

/// Four edges from a rectangle or a four-number list/tuple, for the
/// elementwise operators. Two-point shapes stay out: the recordings fail
/// every short sequence here.
fn rect_edges_opt(value: &Bound<'_, PyAny>) -> Option<CoreRect> {
    if let Some(rect) = as_rect(value) {
        return Some(rect);
    }
    if let Some(rect) = as_irect(value) {
        return Some(rect.rect());
    }
    numbers_opt(value, 4).map(|e| CoreRect::new(e[0], e[1], e[2], e[3]))
}

/// An integer rectangle: the same corners as [`Rect`], rounded outward to
/// pixels. It has no `intersect` and no `transform`: the recordings answer
/// both with `AttributeError`, so both stay undefined here.
#[pyclass]
pub struct IRect {
    inner: CoreIRect,
}

#[pymethods]
impl IRect {
    #[new]
    #[pyo3(signature = (*args, p0=None, p1=None, x0=None, y0=None, x1=None, y1=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        args: &Bound<'_, PyTuple>,
        p0: Option<&Bound<'_, PyAny>>,
        p1: Option<&Bound<'_, PyAny>>,
        x0: Option<&Bound<'_, PyAny>>,
        y0: Option<&Bound<'_, PyAny>>,
        x1: Option<&Bound<'_, PyAny>>,
        y1: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (p0, p1, x0, y0, x1, y1) = (
            given(p0),
            given(p1),
            given(x0),
            given(y0),
            given(x1),
            given(y1),
        );
        if p0.is_some()
            || p1.is_some()
            || x0.is_some()
            || y0.is_some()
            || x1.is_some()
            || y1.is_some()
        {
            if args.len() != 0 {
                return Err(PyTypeError::new_err(
                    "cannot mix positional and keyword edges",
                ));
            }
            let mut edges = [0; 4];
            for (i, keyword) in [x0, y0, x1, y1].iter().enumerate() {
                if let Some(value) = keyword {
                    let Some(number) = irect_number(value, i < 2) else {
                        return Err(PyTypeError::new_err(
                            "rectangle keyword edges must be numbers",
                        ));
                    };
                    edges[i] = number;
                }
            }
            if let Some(value) = p0 {
                let Some(point) = point_opt(value) else {
                    return Err(PyTypeError::new_err(
                        "rectangle corner keywords must be points",
                    ));
                };
                edges[0] = float_edge(point.x, true);
                edges[1] = float_edge(point.y, true);
            }
            if let Some(value) = p1 {
                let Some(point) = point_opt(value) else {
                    return Err(PyTypeError::new_err(
                        "rectangle corner keywords must be points",
                    ));
                };
                edges[2] = float_edge(point.x, false);
                edges[3] = float_edge(point.y, false);
            }
            return Ok(Self {
                inner: CoreIRect::new(edges[0], edges[1], edges[2], edges[3]),
            });
        }
        match args.len() {
            0 => Ok(Self {
                inner: CoreIRect::new(0, 0, 0, 0),
            }),
            1 => {
                let one = args.get_item(0)?;
                if let Some(rect) = irect_opt(&one) {
                    return Ok(Self { inner: rect });
                }
                Err(PyAssertionError::new_err(
                    "cannot build a rectangle from this",
                ))
            }
            2 => {
                let first = point_opt(&args.get_item(0)?);
                let second = point_opt(&args.get_item(1)?);
                match (first, second) {
                    (Some(a), Some(b)) => Ok(Self {
                        inner: CoreIRect::new(
                            float_edge(a.x, true),
                            float_edge(a.y, true),
                            float_edge(b.x, false),
                            float_edge(b.y, false),
                        ),
                    }),
                    _ => Err(PyTypeError::new_err(
                        "two-argument rectangles need two points",
                    )),
                }
            }
            4 => {
                let mut edges = [0; 4];
                for (i, edge) in edges.iter_mut().enumerate() {
                    let Some(number) = irect_number(&args.get_item(i)?, i < 2) else {
                        return Err(PyTypeError::new_err(
                            "four-argument rectangles need four numbers",
                        ));
                    };
                    *edge = number;
                }
                Ok(Self {
                    inner: CoreIRect::new(edges[0], edges[1], edges[2], edges[3]),
                })
            }
            n => Err(PyException::new_err(format!(
                "cannot build a rectangle from {n} arguments"
            ))),
        }
    }

    /// Left edge, in pixels.
    #[getter]
    fn x0(&self) -> i32 {
        self.inner.x0
    }

    #[setter]
    fn set_x0(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.x0 = irect_assigned(value)?;
        Ok(())
    }

    /// Top edge, in pixels.
    #[getter]
    fn y0(&self) -> i32 {
        self.inner.y0
    }

    #[setter]
    fn set_y0(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.y0 = irect_assigned(value)?;
        Ok(())
    }

    /// Right edge, in pixels.
    #[getter]
    fn x1(&self) -> i32 {
        self.inner.x1
    }

    #[setter]
    fn set_x1(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.x1 = irect_assigned(value)?;
        Ok(())
    }

    /// Bottom edge, in pixels.
    #[getter]
    fn y1(&self) -> i32 {
        self.inner.y1
    }

    #[setter]
    fn set_y1(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.y1 = irect_assigned(value)?;
        Ok(())
    }

    /// Shift every edge by a number, or add two rectangles edge by edge.
    fn __add__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = int_opt(p) {
            return Ok(Self {
                inner: self.inner + number,
            });
        }
        if let Some(scalar) = num_opt(p) {
            let sum = self.inner.rect() + scalar;
            return Ok(Self { inner: sum.irect() });
        }
        if let Some(other) = as_irect(p) {
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        if let Some(other) = as_rect(p) {
            let sum = self.inner.rect() + other;
            return Ok(Self { inner: sum.irect() });
        }
        if let Some(edges) = numbers_opt(p, 4) {
            let sum = self.inner.rect() + CoreRect::new(edges[0], edges[1], edges[2], edges[3]);
            return Ok(Self { inner: sum.irect() });
        }
        Err(PyValueError::new_err(
            "cannot add an integer rectangle and this",
        ))
    }

    /// Whether a point, rectangle or quad lies inside this rectangle.
    fn __contains__(&self, x: &Bound<'_, PyAny>) -> PyResult<bool> {
        if let Some(point) = point_opt(x) {
            return Ok(self.inner.contains_point(point) != 0);
        }
        if let Some(quad) = quad_opt(x) {
            return Ok(self.inner.rect().contains_quad(quad));
        }
        if let Some(rect) = rect_opt(x) {
            return Ok(self.inner.rect().contains_rect(rect));
        }
        Ok(false)
    }

    /// Edge-by-edge equality against rectangles and four-number sequences.
    fn __eq__(&self, r: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(rect_opt(r).is_some_and(|r| r == self.inner.rect()))
    }

    /// Integer rectangles have no ordering.
    fn __ge__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("integer rectangles have no ordering"))
    }

    /// One edge by index, or a list of edges for a slice.
    fn __getitem__(&self, py: Python<'_>, i: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let edges = [self.inner.x0, self.inner.y0, self.inner.x1, self.inner.y1];
        if let Ok(index) = i.extract::<isize>() {
            let at = get_index("integer rectangle", index, 4)?;
            return Ok(any_int(py, edges[at]));
        }
        if let Ok(slice) = i.downcast::<PySlice>() {
            let items: Vec<i32> = slice_range(slice, 4)?.iter().map(|&at| edges[at]).collect();
            return Ok(PyList::new(py, items)?.into_any().unbind());
        }
        Err(PyTypeError::new_err(
            "rectangle indices must be integers or slices",
        ))
    }

    /// Integer rectangles have no ordering.
    fn __gt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("integer rectangles have no ordering"))
    }

    /// Integer rectangles have no ordering.
    fn __le__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("integer rectangles have no ordering"))
    }

    /// Edge count, always four.
    fn __len__(&self) -> usize {
        4
    }

    /// Integer rectangles have no ordering.
    fn __lt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("integer rectangles have no ordering"))
    }

    /// Scale every edge by a number, or move the rectangle by a matrix.
    fn __mul__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = int_opt(m) {
            return Ok(Self {
                inner: self.inner * number,
            });
        }
        if let Some(scalar) = num_opt(m) {
            let moved = self.inner.rect() * scalar;
            return Ok(Self {
                inner: moved.irect(),
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner * matrix,
            });
        }
        if as_rect(m).is_some() || as_irect(m).is_some() {
            return Err(PyException::new_err("cannot multiply two rectangles"));
        }
        Err(PyValueError::new_err(
            "cannot multiply an integer rectangle by this",
        ))
    }

    /// Edge-by-edge inequality against rectangles and four-number sequences.
    fn __ne__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!rect_opt(value).is_some_and(|r| r == self.inner.rect()))
    }

    /// Negated copy of the rectangle.
    fn __neg__(&self) -> Self {
        Self { inner: -self.inner }
    }

    /// Copy of the rectangle.
    fn __pos__(&self) -> Self {
        Self { inner: self.inner }
    }

    /// Set one edge by index.
    fn __setitem__(&mut self, i: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        if i.downcast::<PySlice>().is_ok() {
            return Err(PyTypeError::new_err(
                "rectangle edges cannot be assigned by slice",
            ));
        }
        let Ok(index) = i.extract::<isize>() else {
            return Err(PyTypeError::new_err("rectangle indices must be integers"));
        };
        let at = set_index("integer rectangle", index, 4)?;
        let number = irect_assigned(v)?;
        match at {
            0 => self.inner.x0 = number,
            1 => self.inner.y0 = number,
            2 => self.inner.x1 = number,
            _ => self.inner.y1 = number,
        }
        Ok(())
    }

    /// Shift every edge by a number, or subtract two rectangles edge by edge.
    fn __sub__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = int_opt(p) {
            return Ok(Self {
                inner: self.inner - number,
            });
        }
        if let Some(scalar) = num_opt(p) {
            let diff = self.inner.rect() - scalar;
            return Ok(Self {
                inner: diff.irect(),
            });
        }
        if let Some(other) = as_irect(p) {
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        if let Some(other) = as_rect(p) {
            let diff = self.inner.rect() - other;
            return Ok(Self {
                inner: diff.irect(),
            });
        }
        if let Some(edges) = numbers_opt(p, 4) {
            let diff = self.inner.rect() - CoreRect::new(edges[0], edges[1], edges[2], edges[3]);
            return Ok(Self {
                inner: diff.irect(),
            });
        }
        Err(PyValueError::new_err(
            "cannot subtract this from an integer rectangle",
        ))
    }

    /// Divide every edge by a number, or move the rectangle by the inverse of
    /// a matrix. Anything else answers a copy of the rectangle.
    fn __truediv__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = int_opt(m) {
            if number == 0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            return Ok(Self {
                inner: self.inner / number,
            });
        }
        if let Some(number) = num_opt(m) {
            if number == 0.0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            let moved = self.inner.rect() / number;
            return Ok(Self {
                inner: moved.irect(),
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            if !invertible(matrix) {
                return Err(PyZeroDivisionError::new_err(
                    "division by a singular matrix",
                ));
            }
            return Ok(Self {
                inner: self.inner / matrix,
            });
        }
        Ok(Self { inner: self.inner })
    }

    /// Overlap of two rectangles, as a new integer rectangle.
    fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(rect) = as_irect(other) {
            return Ok(Self {
                inner: self.inner & rect,
            });
        }
        if let Some(rect) = as_rect(other) {
            let hit = self.inner.rect() & rect;
            return Ok(Self { inner: hit.irect() });
        }
        if let Some(edges) = numbers_opt(other, 4) {
            let hit = self.inner.rect() & CoreRect::new(edges[0], edges[1], edges[2], edges[3]);
            return Ok(Self { inner: hit.irect() });
        }
        Err(PyValueError::new_err(
            "cannot intersect an integer rectangle with this",
        ))
    }

    /// Cover of two rectangles, as a new integer rectangle.
    fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(rect) = as_irect(other) {
            return Ok(Self {
                inner: self.inner | rect,
            });
        }
        if let Some(rect) = as_rect(other) {
            let cover = self.inner.rect() | rect;
            return Ok(Self {
                inner: cover.irect(),
            });
        }
        if let Some(edges) = numbers_opt(other, 4) {
            let cover = self.inner.rect() | CoreRect::new(edges[0], edges[1], edges[2], edges[3]);
            return Ok(Self {
                inner: cover.irect(),
            });
        }
        Err(PyValueError::new_err(
            "cannot join an integer rectangle with this",
        ))
    }

    /// Integer rectangles are mutable and unhashable.
    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'IRect'"))
    }

    /// Bottom-left corner.
    #[getter]
    fn bl(&self) -> Point {
        Point {
            inner: self.inner.bl(),
        }
    }

    /// Bottom-left corner.
    #[getter]
    fn bottom_left(&self) -> Point {
        self.bl()
    }

    /// Bottom-right corner.
    #[getter]
    fn bottom_right(&self) -> Point {
        Point {
            inner: self.inner.bottom_right(),
        }
    }

    /// Bottom-right corner.
    #[getter]
    fn br(&self) -> Point {
        self.bottom_right()
    }

    /// Top-left corner.
    #[getter]
    fn tl(&self) -> Point {
        Point {
            inner: self.inner.tl(),
        }
    }

    /// Top-left corner.
    #[getter]
    fn top_left(&self) -> Point {
        self.tl()
    }

    /// Top-right corner.
    #[getter]
    fn top_right(&self) -> Point {
        Point {
            inner: self.inner.top_right(),
        }
    }

    /// Top-right corner.
    #[getter]
    fn tr(&self) -> Point {
        self.top_right()
    }

    /// Whether this covers a point (1 or 0), a rectangle or a quad (bool).
    #[pyo3(signature = (x))]
    fn contains(&self, py: Python<'_>, x: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if let Some(point) = point_opt(x) {
            return Ok(any_int(py, self.inner.contains_point(point)));
        }
        if let Some(quad) = quad_opt(x) {
            return any_bool(py, self.inner.rect().contains_quad(quad));
        }
        if let Some(rect) = rect_opt(x) {
            return any_bool(py, self.inner.rect().contains_rect(rect));
        }
        any_bool(py, false)
    }

    /// Area in square pixels, inches, millimetres or centimetres.
    #[pyo3(signature = (*args))]
    fn get_area(&self, args: &Bound<'_, PyTuple>) -> PyResult<f64> {
        let first = args.get_item(0).ok();
        Ok(self.inner.get_area_in(area_unit(first.as_ref())?))
    }

    /// Height in pixels.
    #[getter]
    fn height(&self) -> i32 {
        self.inner.height()
    }

    /// Width in pixels.
    #[getter]
    fn width(&self) -> i32 {
        self.inner.width()
    }

    /// Grown copy covering a point. Unlike the float rectangle, this leaves
    /// the original alone; the recordings pin the grown copy only.
    #[pyo3(signature = (p))]
    fn include_point(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        let Some(point) = point_opt(p) else {
            return Err(PyValueError::new_err("include_point needs a point"));
        };
        let mut grown = self.inner;
        grown.include_point(point);
        Ok(Self { inner: grown })
    }

    /// Grown copy covering another rectangle, leaving the original alone.
    #[pyo3(signature = (r))]
    fn include_rect(&self, r: &Bound<'_, PyAny>) -> PyResult<Self> {
        let Some(other) = irect_opt(r) else {
            return Err(PyValueError::new_err("include_rect needs a rectangle"));
        };
        let mut grown = self.inner;
        grown.include_rect(other);
        Ok(Self { inner: grown })
    }

    /// Whether this overlaps another rectangle.
    #[pyo3(signature = (x))]
    fn intersects(&self, x: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Some(other) = rect_opt(x) else {
            return Err(PyAssertionError::new_err("intersects needs a rectangle"));
        };
        Ok(self.inner.rect().intersects(other))
    }

    /// Whether the rectangle is empty.
    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Whether the rectangle is infinite.
    #[getter]
    fn is_infinite(&self) -> bool {
        self.inner.is_infinite()
    }

    /// Whether the rectangle is valid.
    #[getter]
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    /// Quadrilateral of the corners moved by a matrix around a fixed point.
    #[pyo3(signature = (p, m))]
    fn morph(&self, p: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> PyResult<Quad> {
        let Some(point) = as_point(p) else {
            return Err(PyAttributeError::new_err(
                "morph needs a point with x and y",
            ));
        };
        let matrix = strict_matrix(m)?;
        Ok(Quad {
            inner: self.inner.morph(point, matrix),
        })
    }

    /// Euclidean norm of the edges.
    fn norm(&self) -> f64 {
        self.inner.norm()
    }

    /// Order the edges in place, and return the ordered rectangle.
    fn normalize(&mut self) -> Self {
        self.inner.normalize();
        Self { inner: self.inner }
    }

    /// Quadrilateral over the four corners.
    #[getter]
    fn quad(&self) -> Quad {
        Quad {
            inner: self.inner.quad(),
        }
    }

    /// Float copy of the rectangle.
    #[getter]
    fn rect(&self) -> Rect {
        Rect {
            inner: self.inner.rect(),
        }
    }

    /// Matrix mapping this rectangle onto another one.
    #[pyo3(signature = (r))]
    fn torect(&self, r: &Bound<'_, PyAny>) -> PyResult<Matrix> {
        let Some(target) = irect_opt(r) else {
            return Err(PyAssertionError::new_err("torect needs a rectangle"));
        };
        Ok(Matrix {
            inner: self.inner.torect(target),
        })
    }
}

/// One assigned integer edge: exact for ints, truncated for floats.
fn irect_assigned(value: &Bound<'_, PyAny>) -> PyResult<i32> {
    if let Some(exact) = int_opt(value) {
        return Ok(exact);
    }
    if let Some(float) = num_opt(value) {
        return Ok(float as i32);
    }
    Err(PyValueError::new_err(
        "integer rectangle edges must be numbers",
    ))
}

/// A point: an x/y pair in points with vector arithmetic, mutable.
#[pyclass]
pub struct Point {
    inner: CorePoint,
}

#[pymethods]
impl Point {
    #[new]
    #[pyo3(signature = (*args, x=None, y=None))]
    fn new(
        args: &Bound<'_, PyTuple>,
        x: Option<&Bound<'_, PyAny>>,
        y: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (x, y) = (given(x), given(y));
        if x.is_some() || y.is_some() {
            if args.len() != 0 {
                return Err(PyTypeError::new_err(
                    "cannot mix positional and keyword coordinates",
                ));
            }
            let mut pair = [0.0; 2];
            for (i, keyword) in [x, y].iter().enumerate() {
                if let Some(value) = keyword {
                    let Some(number) = num_opt(value) else {
                        return Err(PyTypeError::new_err("point keywords must be numbers"));
                    };
                    pair[i] = number;
                }
            }
            return Ok(Self {
                inner: CorePoint::new(pair[0], pair[1]),
            });
        }
        match args.len() {
            0 => Ok(Self {
                inner: CorePoint::new(0.0, 0.0),
            }),
            1 => {
                let one = args.get_item(0)?;
                if let Some(point) = point_opt(&one) {
                    return Ok(Self { inner: point });
                }
                Err(PyValueError::new_err("cannot build a point from this"))
            }
            2 => {
                let first = num_opt(&args.get_item(0)?);
                let second = num_opt(&args.get_item(1)?);
                match (first, second) {
                    (Some(a), Some(b)) => Ok(Self {
                        inner: CorePoint::new(a, b),
                    }),
                    _ => Err(PyValueError::new_err(
                        "two-argument points need two numbers",
                    )),
                }
            }
            _ => Err(PyValueError::new_err(
                "cannot build a point from these arguments",
            )),
        }
    }

    /// Horizontal position, in points.
    #[getter]
    fn x(&self) -> f64 {
        self.inner.x
    }

    #[setter]
    fn set_x(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("point coordinates must be numbers"));
        };
        self.inner.x = number;
        Ok(())
    }

    /// Vertical position, in points.
    #[getter]
    fn y(&self) -> f64 {
        self.inner.y
    }

    #[setter]
    fn set_y(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("point coordinates must be numbers"));
        };
        self.inner.y = number;
        Ok(())
    }

    /// Length of the point.
    fn __abs__(&self) -> f64 {
        self.inner.abs()
    }

    /// Shift both coordinates by a number, or add two points coordinate by
    /// coordinate.
    fn __add__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(p) {
            return Ok(Self {
                inner: self.inner + number,
            });
        }
        if let Some(other) = point_opt(p) {
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        Err(PyValueError::new_err("cannot add a point and this"))
    }

    /// Whether any coordinate is nonzero.
    fn __bool__(&self) -> bool {
        self.inner.to_bool()
    }

    /// Coordinate-by-coordinate equality against points and pairs.
    fn __eq__(&self, p: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(point_opt(p).is_some_and(|q| q == self.inner))
    }

    /// Points have no ordering.
    fn __ge__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("points have no ordering"))
    }

    /// One coordinate by index, or a list of coordinates for a slice.
    fn __getitem__(&self, py: Python<'_>, i: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let pair = [self.inner.x, self.inner.y];
        if let Ok(index) = i.extract::<isize>() {
            let at = get_index("point", index, 2)?;
            return Ok(any_float(py, pair[at]));
        }
        if let Ok(slice) = i.downcast::<PySlice>() {
            let items: Vec<f64> = slice_range(slice, 2)?.iter().map(|&at| pair[at]).collect();
            return Ok(PyList::new(py, items)?.into_any().unbind());
        }
        Err(PyTypeError::new_err(
            "point indices must be integers or slices",
        ))
    }

    /// Points have no ordering.
    fn __gt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("points have no ordering"))
    }

    /// Points have no ordering.
    fn __le__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("points have no ordering"))
    }

    /// Coordinate count, always two.
    fn __len__(&self) -> usize {
        2
    }

    /// Points have no ordering.
    fn __lt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("points have no ordering"))
    }

    /// Scale by a number, move by a matrix, or take the dot product with a
    /// point or a pair.
    fn __mul__(&self, py: Python<'_>, m: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if let Some(number) = num_opt(m) {
            let scaled = Py::new(
                py,
                Self {
                    inner: self.inner * number,
                },
            )?;
            return Ok(scaled.into_any());
        }
        if let Some(matrix) = matrix_opt(m) {
            let moved = Py::new(
                py,
                Self {
                    inner: self.inner * matrix,
                },
            )?;
            return Ok(moved.into_any());
        }
        if let Some(other) = point_opt(m) {
            return Ok(any_float(py, self.inner * other));
        }
        Err(PyValueError::new_err("cannot multiply a point by this"))
    }

    /// Coordinate-by-coordinate inequality against points and pairs.
    fn __ne__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!point_opt(value).is_some_and(|q| q == self.inner))
    }

    /// Negated copy of the point.
    fn __neg__(&self) -> Self {
        Self { inner: -self.inner }
    }

    /// Copy of the point.
    fn __pos__(&self) -> Self {
        Self { inner: self.inner }
    }

    /// Set one coordinate by index.
    fn __setitem__(&mut self, i: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        if i.downcast::<PySlice>().is_ok() {
            return Err(PyTypeError::new_err(
                "point coordinates cannot be assigned by slice",
            ));
        }
        let Ok(index) = i.extract::<isize>() else {
            return Err(PyTypeError::new_err("point indices must be integers"));
        };
        let at = set_index("point", index, 2)?;
        let Some(number) = num_opt(v) else {
            return Err(PyValueError::new_err("point coordinates must be numbers"));
        };
        if at == 0 {
            self.inner.x = number;
        } else {
            self.inner.y = number;
        }
        Ok(())
    }

    /// Shift both coordinates by a number, or subtract two points coordinate
    /// by coordinate.
    fn __sub__(&self, p: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(p) {
            return Ok(Self {
                inner: self.inner - number,
            });
        }
        if let Some(other) = point_opt(p) {
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        Err(PyValueError::new_err("cannot subtract this from a point"))
    }

    /// Divide by a number, or move by the inverse of a matrix. Anything else
    /// answers a copy of the point.
    fn __truediv__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            if number == 0.0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            return Ok(Self {
                inner: self.inner / number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            if !invertible(matrix) {
                return Err(PyZeroDivisionError::new_err(
                    "division by a singular matrix",
                ));
            }
            return Ok(Self {
                inner: self.inner / matrix,
            });
        }
        Ok(Self { inner: self.inner })
    }

    /// Points are mutable and unhashable.
    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'Point'"))
    }

    /// Unit vector of the absolute point.
    #[getter]
    fn abs_unit(&self) -> Point {
        Point {
            inner: self.inner.abs_unit(),
        }
    }

    /// Unit vector in the same direction.
    #[getter]
    fn unit(&self) -> Point {
        Point {
            inner: self.inner.unit(),
        }
    }

    /// Distance to a point, a pair, or the top-left of a rectangle.
    #[pyo3(signature = (*args))]
    fn distance_to(&self, args: &Bound<'_, PyTuple>) -> PyResult<f64> {
        if args.len() != 1 {
            return Err(PyValueError::new_err("distance_to takes exactly one point"));
        }
        let arg = args.get_item(0)?;
        if let Some(point) = as_point(&arg) {
            return Ok(self.inner.distance_to(point));
        }
        if let Some(rect) = as_rect(&arg) {
            return Ok(self.inner.distance_to(CorePoint::new(rect.x0, rect.y0)));
        }
        if let Some(rect) = as_irect(&arg) {
            return Ok(self
                .inner
                .distance_to(CorePoint::new(f64::from(rect.x0), f64::from(rect.y0))));
        }
        if num_opt(&arg).is_some() || arg.is_none() {
            return Err(PyTypeError::new_err("distance_to needs a point"));
        }
        if let Some(items) = seq_opt(&arg) {
            if items.len() == 2 || items.len() == 4 {
                if let (Some(x), Some(y)) = (num_opt(&items[0]), num_opt(&items[1])) {
                    return Ok(self.inner.distance_to(CorePoint::new(x, y)));
                }
            }
        }
        Err(PyValueError::new_err(
            "distance_to needs a point or a rectangle",
        ))
    }

    /// Length of the point.
    fn norm(&self) -> f64 {
        self.inner.norm()
    }

    /// Move in place by a matrix, and return the moved point.
    #[pyo3(signature = (m))]
    fn transform(&mut self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        let matrix = strict_matrix(m)?;
        self.inner.transform(matrix);
        Ok(Self { inner: self.inner })
    }
}

/// A 2D affine matrix: six coefficients in `a b c d e f` order, mutable.
#[pyclass]
pub struct Matrix {
    inner: CoreMatrix,
}

#[pymethods]
impl Matrix {
    #[new]
    #[pyo3(signature = (*args, a=None, b=None, c=None, d=None, e=None, f=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        args: &Bound<'_, PyTuple>,
        a: Option<&Bound<'_, PyAny>>,
        b: Option<&Bound<'_, PyAny>>,
        c: Option<&Bound<'_, PyAny>>,
        d: Option<&Bound<'_, PyAny>>,
        e: Option<&Bound<'_, PyAny>>,
        f: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (a, b, c, d, e, f) = (given(a), given(b), given(c), given(d), given(e), given(f));
        if a.is_some() || b.is_some() || c.is_some() || d.is_some() || e.is_some() || f.is_some() {
            if args.len() != 0 {
                return Err(PyTypeError::new_err(
                    "cannot mix positional and keyword coefficients",
                ));
            }
            let mut coeffs = [0.0; 6];
            for (i, keyword) in [a, b, c, d, e, f].iter().enumerate() {
                if let Some(value) = keyword {
                    let Some(number) = num_opt(value) else {
                        return Err(PyTypeError::new_err(
                            "matrix keyword coefficients must be numbers",
                        ));
                    };
                    coeffs[i] = number;
                }
            }
            return Ok(Self {
                inner: CoreMatrix::new(
                    coeffs[0], coeffs[1], coeffs[2], coeffs[3], coeffs[4], coeffs[5],
                ),
            });
        }
        match args.len() {
            0 => Ok(Self {
                inner: CoreMatrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            }),
            1 => {
                let one = args.get_item(0)?;
                if let Some(theta) = num_opt(&one) {
                    return Ok(Self {
                        inner: rotation(theta),
                    });
                }
                if let Some(matrix) = matrix_opt(&one) {
                    return Ok(Self { inner: matrix });
                }
                Err(PyValueError::new_err("cannot build a matrix from this"))
            }
            2 => {
                let first = num_opt(&args.get_item(0)?);
                let second = num_opt(&args.get_item(1)?);
                match (first, second) {
                    (Some(sx), Some(sy)) => Ok(Self {
                        inner: CoreMatrix::new(sx, 0.0, 0.0, sy, 0.0, 0.0),
                    }),
                    _ => Err(PyValueError::new_err(
                        "two-argument matrices need two scales",
                    )),
                }
            }
            6 => {
                let mut coeffs = [0.0; 6];
                for (i, coeff) in coeffs.iter_mut().enumerate() {
                    let Some(number) = num_opt(&args.get_item(i)?) else {
                        return Err(PyValueError::new_err(
                            "six-argument matrices need six numbers",
                        ));
                    };
                    *coeff = number;
                }
                Ok(Self {
                    inner: CoreMatrix::new(
                        coeffs[0], coeffs[1], coeffs[2], coeffs[3], coeffs[4], coeffs[5],
                    ),
                })
            }
            _ => Err(PyValueError::new_err(
                "cannot build a matrix from these arguments",
            )),
        }
    }

    /// Horizontal scale factor.
    #[getter]
    fn a(&self) -> f64 {
        self.inner.a
    }

    #[setter]
    fn set_a(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.a = number;
        Ok(())
    }

    /// Vertical shear factor.
    #[getter]
    fn b(&self) -> f64 {
        self.inner.b
    }

    #[setter]
    fn set_b(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.b = number;
        Ok(())
    }

    /// Horizontal shear factor.
    #[getter]
    fn c(&self) -> f64 {
        self.inner.c
    }

    #[setter]
    fn set_c(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.c = number;
        Ok(())
    }

    /// Vertical scale factor.
    #[getter]
    fn d(&self) -> f64 {
        self.inner.d
    }

    #[setter]
    fn set_d(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.d = number;
        Ok(())
    }

    /// Horizontal translation, in points.
    #[getter]
    fn e(&self) -> f64 {
        self.inner.e
    }

    #[setter]
    fn set_e(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.e = number;
        Ok(())
    }

    /// Vertical translation, in points.
    #[getter]
    fn f(&self) -> f64 {
        self.inner.f
    }

    #[setter]
    fn set_f(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(number) = num_opt(value) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        self.inner.f = number;
        Ok(())
    }

    /// Norm of the matrix.
    fn __abs__(&self) -> f64 {
        self.inner.abs()
    }

    /// Shift every coefficient by a number, or add two matrices coefficient
    /// by coefficient.
    fn __add__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            return Ok(Self {
                inner: self.inner + number,
            });
        }
        if let Some(other) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        Err(PyValueError::new_err("cannot add a matrix and this"))
    }

    /// Whether any coefficient is nonzero.
    fn __bool__(&self) -> bool {
        self.inner.to_bool()
    }

    /// Coefficient-by-coefficient equality against matrices and sequences.
    fn __eq__(&self, mat: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(matrix_opt(mat).is_some_and(|m| m == self.inner))
    }

    /// Matrices have no ordering.
    fn __ge__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("matrices have no ordering"))
    }

    /// One coefficient by index, or a list of coefficients for a slice.
    fn __getitem__(&self, py: Python<'_>, i: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let coeffs = [
            self.inner.a,
            self.inner.b,
            self.inner.c,
            self.inner.d,
            self.inner.e,
            self.inner.f,
        ];
        if let Ok(index) = i.extract::<isize>() {
            let at = get_index("matrix", index, 6)?;
            return Ok(any_float(py, coeffs[at]));
        }
        if let Ok(slice) = i.downcast::<PySlice>() {
            let items: Vec<f64> = slice_range(slice, 6)?
                .iter()
                .map(|&at| coeffs[at])
                .collect();
            return Ok(PyList::new(py, items)?.into_any().unbind());
        }
        Err(PyTypeError::new_err(
            "matrix indices must be integers or slices",
        ))
    }

    /// Matrices have no ordering.
    fn __gt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("matrices have no ordering"))
    }

    /// Inverted copy of the matrix; a singular matrix answers itself.
    fn __invert__(&self) -> Self {
        Self {
            inner: self.inner.invert(None),
        }
    }

    /// Matrices have no ordering.
    fn __le__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("matrices have no ordering"))
    }

    /// Coefficient count, always six.
    fn __len__(&self) -> usize {
        6
    }

    /// Matrices have no ordering.
    fn __lt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("matrices have no ordering"))
    }

    /// Scale every coefficient by a number, or concatenate two matrices.
    fn __mul__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            return Ok(Self {
                inner: self.inner * number,
            });
        }
        if let Some(other) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner * other,
            });
        }
        Err(PyValueError::new_err("cannot multiply a matrix by this"))
    }

    /// Coefficient-by-coefficient inequality against matrices and sequences.
    fn __ne__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!matrix_opt(value).is_some_and(|m| m == self.inner))
    }

    /// Negated copy of the matrix.
    fn __neg__(&self) -> Self {
        Self { inner: -self.inner }
    }

    /// Copy of the matrix.
    fn __pos__(&self) -> Self {
        Self { inner: self.inner }
    }

    /// Set one coefficient by index.
    fn __setitem__(&mut self, i: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        if i.downcast::<PySlice>().is_ok() {
            return Err(PyTypeError::new_err(
                "matrix coefficients cannot be assigned by slice",
            ));
        }
        let Ok(index) = i.extract::<isize>() else {
            return Err(PyTypeError::new_err("matrix indices must be integers"));
        };
        let at = set_index("matrix", index, 6)?;
        let Some(number) = num_opt(v) else {
            return Err(PyValueError::new_err("matrix coefficients must be numbers"));
        };
        match at {
            0 => self.inner.a = number,
            1 => self.inner.b = number,
            2 => self.inner.c = number,
            3 => self.inner.d = number,
            4 => self.inner.e = number,
            _ => self.inner.f = number,
        }
        Ok(())
    }

    /// Shift every coefficient by a number, or subtract two matrices
    /// coefficient by coefficient.
    fn __sub__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            return Ok(Self {
                inner: self.inner - number,
            });
        }
        if let Some(other) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        Err(PyValueError::new_err("cannot subtract this from a matrix"))
    }

    /// Divide every coefficient by a number, or concatenate with the inverse
    /// of a matrix. Anything else answers a copy of the matrix.
    fn __truediv__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            if number == 0.0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            return Ok(Self {
                inner: self.inner / number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            if !invertible(matrix) {
                return Err(PyZeroDivisionError::new_err(
                    "division by a singular matrix",
                ));
            }
            return Ok(Self {
                inner: self.inner / matrix,
            });
        }
        Ok(Self { inner: self.inner })
    }

    /// Matrices are mutable and unhashable.
    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'Matrix'"))
    }

    /// Set this to the product of two matrices, and return it.
    #[pyo3(signature = (one, two))]
    fn concat(&mut self, one: &Bound<'_, PyAny>, two: &Bound<'_, PyAny>) -> PyResult<Self> {
        let one = strict_matrix(one)?;
        let two = strict_matrix(two)?;
        self.inner.concat(one, two);
        Ok(Self { inner: self.inner })
    }

    /// Replace this with the inverse of itself, or of the given matrix, and
    /// answer 0. A singular matrix answers 1 and stays unchanged.
    #[pyo3(signature = (src=None))]
    fn invert(&mut self, src: Option<&Bound<'_, PyAny>>) -> PyResult<i32> {
        let target = match src {
            None => self.inner,
            Some(value) if value.is_none() => self.inner,
            Some(value) => {
                matrix_opt(value).unwrap_or(CoreMatrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0))
            }
        };
        if !invertible(target) {
            return Ok(1);
        }
        self.inner = target.invert(None);
        Ok(0)
    }

    /// Whether the matrix keeps axes aligned.
    #[getter]
    fn is_rectilinear(&self) -> bool {
        self.inner.is_rectilinear()
    }

    /// Norm of the matrix.
    fn norm(&self) -> f64 {
        self.inner.norm()
    }

    /// Rotate in place, and return the rotated matrix.
    #[pyo3(signature = (theta))]
    fn prerotate(&mut self, theta: f64) -> Self {
        self.inner.prerotate(theta);
        Self { inner: self.inner }
    }

    /// Scale in place, and return the scaled matrix.
    #[pyo3(signature = (sx, sy))]
    fn prescale(&mut self, sx: f64, sy: f64) -> Self {
        self.inner.prescale(sx, sy);
        Self { inner: self.inner }
    }

    /// Shear in place, and return the sheared matrix.
    #[pyo3(signature = (h, v))]
    fn preshear(&mut self, h: f64, v: f64) -> Self {
        self.inner.preshear(h, v);
        Self { inner: self.inner }
    }

    /// Translate in place, and return the translated matrix.
    #[pyo3(signature = (tx, ty))]
    fn pretranslate(&mut self, tx: f64, ty: f64) -> Self {
        self.inner.pretranslate(tx, ty);
        Self { inner: self.inner }
    }
}

/// A quadrilateral: four corners in upper-left, upper-right, lower-left,
/// lower-right order, mutable.
#[pyclass]
pub struct Quad {
    inner: CoreQuad,
}

#[pymethods]
impl Quad {
    #[new]
    #[pyo3(signature = (*args, ul=None, ur=None, ll=None, lr=None))]
    fn new(
        args: &Bound<'_, PyTuple>,
        ul: Option<&Bound<'_, PyAny>>,
        ur: Option<&Bound<'_, PyAny>>,
        ll: Option<&Bound<'_, PyAny>>,
        lr: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let (ul, ur, ll, lr) = (given(ul), given(ur), given(ll), given(lr));
        if ul.is_some() || ur.is_some() || ll.is_some() || lr.is_some() {
            if args.len() != 0 {
                return Err(PyTypeError::new_err(
                    "cannot mix positional and keyword corners",
                ));
            }
            let mut corners = [CorePoint::new(0.0, 0.0); 4];
            for (i, keyword) in [ul, ur, ll, lr].iter().enumerate() {
                if let Some(value) = keyword {
                    let Some(point) = point_opt(value) else {
                        return Err(PyTypeError::new_err("quad corner keywords must be points"));
                    };
                    corners[i] = point;
                }
            }
            return Ok(Self {
                inner: CoreQuad::new(corners[0], corners[1], corners[2], corners[3]),
            });
        }
        match args.len() {
            0 => Ok(Self {
                inner: CoreQuad::new(
                    CorePoint::new(0.0, 0.0),
                    CorePoint::new(0.0, 0.0),
                    CorePoint::new(0.0, 0.0),
                    CorePoint::new(0.0, 0.0),
                ),
            }),
            1 => {
                let one = args.get_item(0)?;
                if let Some(quad) = quad_opt(&one) {
                    return Ok(Self { inner: quad });
                }
                Err(PyValueError::new_err(
                    "cannot build a quadrilateral from this",
                ))
            }
            4 => {
                let mut corners = [CorePoint::new(0.0, 0.0); 4];
                for (i, corner) in corners.iter_mut().enumerate() {
                    let Some(point) = point_opt(&args.get_item(i)?) else {
                        return Err(PyValueError::new_err(
                            "four-argument quads need four points",
                        ));
                    };
                    *corner = point;
                }
                Ok(Self {
                    inner: CoreQuad::new(corners[0], corners[1], corners[2], corners[3]),
                })
            }
            _ => Err(PyValueError::new_err(
                "cannot build a quadrilateral from these arguments",
            )),
        }
    }

    /// Upper-left corner.
    #[getter]
    fn ul(&self) -> Point {
        Point {
            inner: self.inner.ul,
        }
    }

    #[setter]
    fn set_ul(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(point) = point_opt(value) else {
            return Err(PyValueError::new_err(
                "quadrilateral corners must be points",
            ));
        };
        self.inner.ul = point;
        Ok(())
    }

    /// Upper-right corner.
    #[getter]
    fn ur(&self) -> Point {
        Point {
            inner: self.inner.ur,
        }
    }

    #[setter]
    fn set_ur(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(point) = point_opt(value) else {
            return Err(PyValueError::new_err(
                "quadrilateral corners must be points",
            ));
        };
        self.inner.ur = point;
        Ok(())
    }

    /// Lower-left corner.
    #[getter]
    fn ll(&self) -> Point {
        Point {
            inner: self.inner.ll,
        }
    }

    #[setter]
    fn set_ll(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(point) = point_opt(value) else {
            return Err(PyValueError::new_err(
                "quadrilateral corners must be points",
            ));
        };
        self.inner.ll = point;
        Ok(())
    }

    /// Lower-right corner.
    #[getter]
    fn lr(&self) -> Point {
        Point {
            inner: self.inner.lr,
        }
    }

    #[setter]
    fn set_lr(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let Some(point) = point_opt(value) else {
            return Err(PyValueError::new_err(
                "quadrilateral corners must be points",
            ));
        };
        self.inner.lr = point;
        Ok(())
    }

    /// Area of the quadrilateral.
    fn __abs__(&self) -> f64 {
        self.inner.abs()
    }

    /// Shift every corner by a number or per-corner scalars, or add two
    /// quadrilaterals corner by corner.
    fn __add__(&self, q: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(q) {
            return Ok(Self {
                inner: self.inner + number,
            });
        }
        if let Some(other) = as_quad(q) {
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        if let Some(scalars) = numbers_opt(q, 4) {
            return Ok(Self {
                inner: self.inner + corner_scalars(&scalars),
            });
        }
        if let Some(other) = quad_opt(q) {
            // A four-point sequence reads as a quadrilateral here.
            return Ok(Self {
                inner: self.inner + other,
            });
        }
        Err(PyValueError::new_err("cannot add a quadrilateral and this"))
    }

    /// Whether any corner is nonzero.
    fn __bool__(&self) -> bool {
        self.inner.to_bool()
    }

    /// Whether a point lies inside this quadrilateral.
    fn __contains__(&self, x: &Bound<'_, PyAny>) -> PyResult<bool> {
        if let Some(point) = point_opt(x) {
            return Ok(self.inner.contains_point(point));
        }
        Ok(false)
    }

    /// Corner-by-corner equality against quads and four-point sequences.
    fn __eq__(&self, quad: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(quad_opt(quad).is_some_and(|q| q == self.inner))
    }

    /// Quadrilaterals have no ordering.
    fn __ge__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("quadrilaterals have no ordering"))
    }

    /// One corner by index, or a list of corners for a slice.
    fn __getitem__(&self, py: Python<'_>, i: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let corners = [self.inner.ul, self.inner.ur, self.inner.ll, self.inner.lr];
        if let Ok(index) = i.extract::<isize>() {
            let at = get_index("quadrilateral", index, 4)?;
            let corner = Py::new(py, Point { inner: corners[at] })?;
            return Ok(corner.into_any());
        }
        if let Ok(slice) = i.downcast::<PySlice>() {
            let items: Vec<Point> = slice_range(slice, 4)?
                .iter()
                .map(|&at| Point { inner: corners[at] })
                .collect();
            return Ok(PyList::new(py, items)?.into_any().unbind());
        }
        Err(PyTypeError::new_err(
            "quad indices must be integers or slices",
        ))
    }

    /// Quadrilaterals have no ordering.
    fn __gt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("quadrilaterals have no ordering"))
    }

    /// Quadrilaterals have no ordering.
    fn __le__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("quadrilaterals have no ordering"))
    }

    /// Corner count, always four.
    fn __len__(&self) -> usize {
        4
    }

    /// Quadrilaterals have no ordering.
    fn __lt__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let _ = value;
        Err(PyTypeError::new_err("quadrilaterals have no ordering"))
    }

    /// Scale by a number, or move every corner by a matrix.
    fn __mul__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            return Ok(Self {
                inner: self.inner * number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            return Ok(Self {
                inner: self.inner * matrix,
            });
        }
        Err(PyValueError::new_err(
            "cannot multiply a quadrilateral by this",
        ))
    }

    /// Corner-by-corner inequality against quads and four-point sequences.
    fn __ne__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(!quad_opt(value).is_some_and(|q| q == self.inner))
    }

    /// Negated copy of the quadrilateral.
    fn __neg__(&self) -> Self {
        Self { inner: -self.inner }
    }

    /// Copy of the quadrilateral.
    fn __pos__(&self) -> Self {
        Self { inner: self.inner }
    }

    /// Set one corner by index.
    fn __setitem__(&mut self, i: &Bound<'_, PyAny>, v: &Bound<'_, PyAny>) -> PyResult<()> {
        if i.downcast::<PySlice>().is_ok() {
            return Err(PyTypeError::new_err(
                "quad corners cannot be assigned by slice",
            ));
        }
        let Ok(index) = i.extract::<isize>() else {
            return Err(PyTypeError::new_err("quad indices must be integers"));
        };
        let at = set_index("quadrilateral", index, 4)?;
        let Some(point) = point_opt(v) else {
            return Err(PyValueError::new_err(
                "quadrilateral corners must be points",
            ));
        };
        match at {
            0 => self.inner.ul = point,
            1 => self.inner.ur = point,
            2 => self.inner.ll = point,
            _ => self.inner.lr = point,
        }
        Ok(())
    }

    /// Shift every corner by a number or per-corner scalars, or subtract two
    /// quadrilaterals corner by corner.
    fn __sub__(&self, q: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(q) {
            return Ok(Self {
                inner: self.inner - number,
            });
        }
        if let Some(other) = as_quad(q) {
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        if let Some(scalars) = numbers_opt(q, 4) {
            return Ok(Self {
                inner: self.inner - corner_scalars(&scalars),
            });
        }
        if let Some(other) = quad_opt(q) {
            // A four-point sequence reads as a quadrilateral here.
            return Ok(Self {
                inner: self.inner - other,
            });
        }
        Err(PyValueError::new_err(
            "cannot subtract this from a quadrilateral",
        ))
    }

    /// Divide by a number, or move by the inverse of a matrix. Anything else
    /// answers a copy of the quadrilateral.
    fn __truediv__(&self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(number) = num_opt(m) {
            if number == 0.0 {
                return Err(PyZeroDivisionError::new_err("division by zero"));
            }
            return Ok(Self {
                inner: self.inner / number,
            });
        }
        if let Some(matrix) = matrix_opt(m) {
            if !invertible(matrix) {
                return Err(PyZeroDivisionError::new_err(
                    "division by a singular matrix",
                ));
            }
            return Ok(Self {
                inner: self.inner / matrix,
            });
        }
        Ok(Self { inner: self.inner })
    }

    /// Quadrilaterals are mutable and unhashable.
    fn __hash__(&self) -> PyResult<isize> {
        Err(PyTypeError::new_err("unhashable type: 'Quad'"))
    }

    /// Height in points.
    #[getter]
    fn height(&self) -> f64 {
        self.inner.height()
    }

    /// Width in points.
    #[getter]
    fn width(&self) -> f64 {
        self.inner.width()
    }

    /// Whether the quadrilateral is convex.
    #[getter]
    fn is_convex(&self) -> bool {
        self.inner.is_convex()
    }

    /// Whether the quadrilateral is empty.
    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Whether the quadrilateral is infinite.
    #[getter]
    fn is_infinite(&self) -> bool {
        self.inner.is_infinite()
    }

    /// Whether the quadrilateral is a rectangle.
    #[getter]
    fn is_rectangular(&self) -> bool {
        self.inner.is_rectangular()
    }

    /// Quadrilateral of the corners moved by a matrix around a fixed point.
    #[pyo3(signature = (p, m))]
    fn morph(&self, p: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> PyResult<Quad> {
        let Some(point) = as_point(p) else {
            return Err(PyAttributeError::new_err(
                "morph needs a point with x and y",
            ));
        };
        let matrix = strict_matrix(m)?;
        Ok(Quad {
            inner: self.inner.morph(point, matrix),
        })
    }

    /// Smallest covering rectangle.
    #[getter]
    fn rect(&self) -> Rect {
        Rect {
            inner: self.inner.rect(),
        }
    }

    /// Move in place by a matrix, and return the moved quadrilateral.
    #[pyo3(signature = (m))]
    fn transform(&mut self, m: &Bound<'_, PyAny>) -> PyResult<Self> {
        let matrix = strict_matrix(m)?;
        self.inner.transform(matrix);
        Ok(Self { inner: self.inner })
    }
}

/// Four scalars as one per-corner shift, added to both coordinates of each
/// corner in upper-left, upper-right, lower-left, lower-right order.
fn corner_scalars(scalars: &[f64]) -> CoreQuad {
    CoreQuad::new(
        CorePoint::new(scalars[0], scalars[0]),
        CorePoint::new(scalars[1], scalars[1]),
        CorePoint::new(scalars[2], scalars[2]),
        CorePoint::new(scalars[3], scalars[3]),
    )
}

/// Register the P0 lane's new module-level names.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Rect>()?;
    m.add_class::<IRect>()?;
    m.add_class::<Point>()?;
    m.add_class::<Matrix>()?;
    m.add_class::<Quad>()?;
    m.add(
        "Identity",
        Matrix {
            inner: pdfluent::compat::constants::Identity,
        },
    )?;
    Ok(())
}
