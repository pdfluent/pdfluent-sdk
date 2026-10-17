//! Value types for the compatible API (lane P0: points, rectangles, matrices).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Plain data: every struct here is `Copy` field storage with no document
//! behind it, so methods take and return values rather than borrowing. The
//! mutation methods (`transform`, `normalize`, `intersect`, the `pre*` family)
//! update in place and return the new value, matching the recorded call
//! shapes; ordering (`lt`/`le`/`gt`/`ge`) is one `compare` method per type
//! because the recordings fail every ordering attempt.

use std::ops::{Add, BitAnd, BitOr, Div, Index, IndexMut, Mul, Neg, Sub};

use super::ErrorKind;
use crate::error::Result;

/// Smallest edge of the infinite rectangle, shared by the float and integer types.
const INFINITE_MIN: f64 = -2_147_483_648.0;
/// Largest edge of the infinite rectangle; the span is 2^32 - 128 points.
const INFINITE_MAX: f64 = 2_147_483_520.0;
/// Integer rectangles built from float input clamp to this magnitude.
const IRECT_LIMIT: f64 = 16_777_216.0;

/// Outward float-to-integer conversion: floors the top-left corner, ceils the
/// bottom-right corner, then clamps to [`IRECT_LIMIT`] so the result fits.
fn outward_to_irect(x0: f64, y0: f64, x1: f64, y1: f64) -> IRect {
    fn edge(value: f64, low: bool) -> i32 {
        let rounded = if low { value.floor() } else { value.ceil() };
        rounded.clamp(-IRECT_LIMIT, IRECT_LIMIT) as i32
    }
    IRect {
        x0: edge(x0, true),
        y0: edge(y0, true),
        x1: edge(x1, false),
        y1: edge(y1, false),
    }
}

/// Whether the point lies on the segment `a`-`b`, endpoints included.
fn point_on_segment(p: Point, a: Point, b: Point) -> bool {
    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    if cross != 0.0 {
        return false;
    }
    let (min_x, max_x) = if a.x < b.x { (a.x, b.x) } else { (b.x, a.x) };
    let (min_y, max_y) = if a.y < b.y { (a.y, b.y) } else { (b.y, a.y) };
    p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y
}

/// A rectangle in points: two diagonally opposite corners.
///
/// Fields are public so bindings build values directly; the `P0` lane keeps
/// every method's recorded shape (in-place plus returned value).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    /// Left edge, in points.
    pub x0: f64,
    /// Top edge, in points.
    pub y0: f64,
    /// Right edge, in points.
    pub x1: f64,
    /// Bottom edge, in points.
    pub y1: f64,
}

/// An integer rectangle: the same corners as [`Rect`], rounded to pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IRect {
    /// Left edge, in pixels.
    pub x0: i32,
    /// Top edge, in pixels.
    pub y0: i32,
    /// Right edge, in pixels.
    pub x1: i32,
    /// Bottom edge, in pixels.
    pub y1: i32,
}

/// A point in points: an x/y pair with vector arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Horizontal position, in points.
    pub x: f64,
    /// Vertical position, in points.
    pub y: f64,
}

/// A 2D affine matrix: six coefficients in `a b c d e f` order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    /// Horizontal scale factor.
    pub a: f64,
    /// Vertical shear factor.
    pub b: f64,
    /// Horizontal shear factor.
    pub c: f64,
    /// Vertical scale factor.
    pub d: f64,
    /// Horizontal translation, in points.
    pub e: f64,
    /// Vertical translation, in points.
    pub f: f64,
}

/// A quadrilateral: four corners in upper-left, upper-right, lower-left,
/// lower-right order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    /// Upper-left corner.
    pub ul: Point,
    /// Upper-right corner.
    pub ur: Point,
    /// Lower-left corner.
    pub ll: Point,
    /// Lower-right corner.
    pub lr: Point,
}

/// Area units accepted by [`Rect::get_area_in`] and [`IRect::get_area_in`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AreaUnit {
    /// Square display pixels, the default.
    Pixels,
    /// Square inches.
    Inches,
    /// Square millimetres.
    Millimetres,
    /// Square centimetres.
    Centimetres,
}

/// Ordering operators that have no Rust trait, carried as data.
///
/// Every ordering attempt in the recordings fails, so each type exposes one
/// `compare` method taking this instead of four `lt`/`le`/`gt`/`ge` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompareOp {
    /// Less than.
    Less,
    /// Less than or equal.
    LessOrEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterOrEqual,
}

/// Area of a width-by-height span, clamped at zero for empty spans.
fn clamped_area(width: f64, height: f64) -> f64 {
    if width <= 0.0 || height <= 0.0 {
        0.0
    } else {
        width * height
    }
}

/// Area in the given units; 72 points make one inch.
fn area_in(area: f64, unit: AreaUnit) -> f64 {
    match unit {
        AreaUnit::Pixels => area,
        AreaUnit::Inches => area / 5184.0,
        AreaUnit::Millimetres => area / 5184.0 * 25.4 * 25.4,
        AreaUnit::Centimetres => area / 5184.0 * 2.54 * 2.54,
    }
}

impl Rect {
    /// Build a rectangle from four edges.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Area of the rectangle.
    pub fn abs(&self) -> f64 {
        clamped_area(self.x1 - self.x0, self.y1 - self.y0)
    }

    /// Whether any edge is nonzero.
    pub fn to_bool(&self) -> bool {
        self.x0 != 0.0 || self.y0 != 0.0 || self.x1 != 0.0 || self.y1 != 0.0
    }

    /// Point containment code: 1 when the point lies inside or on the edge, else 0.
    pub fn contains_point(&self, point: Point) -> i32 {
        i32::from(
            point.x >= self.x0 && point.x <= self.x1 && point.y >= self.y0 && point.y <= self.y1,
        )
    }

    /// Edge count, always four.
    pub fn len(&self) -> usize {
        4
    }

    /// Copy of the rectangle.
    pub fn pos(&self) -> Self {
        *self
    }

    /// Bottom-left corner.
    pub fn bl(&self) -> Point {
        Point {
            x: self.x0,
            y: self.y1,
        }
    }

    /// Bottom-left corner.
    pub fn bottom_left(&self) -> Point {
        self.bl()
    }

    /// Bottom-right corner.
    pub fn bottom_right(&self) -> Point {
        Point {
            x: self.x1,
            y: self.y1,
        }
    }

    /// Bottom-right corner.
    pub fn br(&self) -> Point {
        self.bottom_right()
    }

    /// Whether this covers another rectangle.
    pub fn contains_rect(&self, other: Rect) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }

    /// Area in square pixels.
    pub fn get_area(&self) -> f64 {
        self.abs()
    }

    /// Height in points; negative while the edges run backwards.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// Grow to cover a point.
    pub fn include_point(&mut self, point: Point) -> Rect {
        self.x0 = self.x0.min(point.x);
        self.y0 = self.y0.min(point.y);
        self.x1 = self.x1.max(point.x);
        self.y1 = self.y1.max(point.y);
        *self
    }

    /// Grow to cover another rectangle.
    pub fn include_rect(&mut self, other: Rect) -> Rect {
        self.x0 = self.x0.min(other.x0);
        self.y0 = self.y0.min(other.y0);
        self.x1 = self.x1.max(other.x1);
        self.y1 = self.y1.max(other.y1);
        *self
    }

    /// Shrink to the overlap with another rectangle.
    pub fn intersect(&mut self, other: Rect) -> Rect {
        self.x0 = self.x0.max(other.x0);
        self.y0 = self.y0.max(other.y0);
        self.x1 = self.x1.min(other.x1);
        self.y1 = self.y1.min(other.y1);
        *self
    }

    /// Whether this overlaps another rectangle.
    pub fn intersects(&self, other: Rect) -> bool {
        self.x0.max(other.x0) < self.x1.min(other.x1)
            && self.y0.max(other.y0) < self.y1.min(other.y1)
    }

    /// Integer copy of the rectangle.
    pub fn irect(&self) -> IRect {
        outward_to_irect(self.x0, self.y0, self.x1, self.y1)
    }

    /// Whether the rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// Whether the rectangle is infinite.
    pub fn is_infinite(&self) -> bool {
        self.x0 == INFINITE_MIN
            && self.y0 == INFINITE_MIN
            && self.x1 == INFINITE_MAX
            && self.y1 == INFINITE_MAX
    }

    /// Whether the rectangle is valid.
    pub fn is_valid(&self) -> bool {
        self.x0 <= self.x1 && self.y0 <= self.y1
    }

    /// Morphed quadrilateral around a fixed point.
    pub fn morph(&self, point: Point, matrix: Matrix) -> Quad {
        self.quad().morph(point, matrix)
    }

    /// Euclidean norm of the edges.
    pub fn norm(&self) -> f64 {
        (self.x0 * self.x0 + self.y0 * self.y0 + self.x1 * self.x1 + self.y1 * self.y1).sqrt()
    }

    /// Order the edges and return the result.
    pub fn normalize(&mut self) -> Rect {
        if self.x0 > self.x1 {
            std::mem::swap(&mut self.x0, &mut self.x1);
        }
        if self.y0 > self.y1 {
            std::mem::swap(&mut self.y0, &mut self.y1);
        }
        *self
    }

    /// Quadrilateral over the four corners.
    pub fn quad(&self) -> Quad {
        Quad {
            ul: self.top_left(),
            ur: self.top_right(),
            ll: self.bottom_left(),
            lr: self.bottom_right(),
        }
    }

    /// Integer copy with rounded edges.
    pub fn round(&self) -> IRect {
        self.irect()
    }

    /// Top-left corner.
    pub fn tl(&self) -> Point {
        Point {
            x: self.x0,
            y: self.y0,
        }
    }

    /// Top-left corner.
    pub fn top_left(&self) -> Point {
        self.tl()
    }

    /// Top-right corner.
    pub fn top_right(&self) -> Point {
        Point {
            x: self.x1,
            y: self.y0,
        }
    }

    /// Matrix mapping this onto another rectangle.
    pub fn torect(&self, target: Rect) -> Matrix {
        let sx = (target.x1 - target.x0) / (self.x1 - self.x0);
        let sy = (target.y1 - target.y0) / (self.y1 - self.y0);
        Matrix {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: target.x0 - self.x0 * sx,
            f: target.y0 - self.y0 * sy,
        }
    }

    /// Top-right corner.
    pub fn tr(&self) -> Point {
        self.top_right()
    }

    /// Transform in place and return the result.
    pub fn transform(&mut self, matrix: Matrix) -> Rect {
        *self = *self * matrix;
        *self
    }

    /// Width in points; negative while the edges run backwards.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Whether this covers a quadrilateral.
    pub fn contains_quad(&self, quad: Quad) -> bool {
        self.contains_rect(quad.rect())
    }

    /// Area in the given units.
    pub fn get_area_in(&self, unit: AreaUnit) -> f64 {
        area_in(self.abs(), unit)
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("rectangles have no ordering"))
    }
}

impl IRect {
    /// Build an integer rectangle from four edges.
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Point containment code: 1 when the point lies inside or on the edge, else 0.
    pub fn contains_point(&self, point: Point) -> i32 {
        let x0 = f64::from(self.x0);
        let y0 = f64::from(self.y0);
        let x1 = f64::from(self.x1);
        let y1 = f64::from(self.y1);
        i32::from(point.x >= x0 && point.x <= x1 && point.y >= y0 && point.y <= y1)
    }

    /// Edge count, always four.
    pub fn len(&self) -> usize {
        4
    }

    /// Copy of the rectangle.
    pub fn pos(&self) -> Self {
        *self
    }

    /// Bottom-left corner.
    pub fn bl(&self) -> Point {
        Point {
            x: f64::from(self.x0),
            y: f64::from(self.y1),
        }
    }

    /// Bottom-left corner.
    pub fn bottom_left(&self) -> Point {
        self.bl()
    }

    /// Bottom-right corner.
    pub fn bottom_right(&self) -> Point {
        Point {
            x: f64::from(self.x1),
            y: f64::from(self.y1),
        }
    }

    /// Bottom-right corner.
    pub fn br(&self) -> Point {
        self.bottom_right()
    }

    /// Whether this covers another integer rectangle.
    pub fn contains_rect(&self, other: IRect) -> bool {
        other.x0 >= self.x0 && other.y0 >= self.y0 && other.x1 <= self.x1 && other.y1 <= self.y1
    }

    /// Area in square pixels.
    pub fn get_area(&self) -> f64 {
        let width = (i64::from(self.x1) - i64::from(self.x0)).max(0) as f64;
        let height = (i64::from(self.y1) - i64::from(self.y0)).max(0) as f64;
        width * height
    }

    /// Height in pixels, clamped at zero for inverted edges.
    pub fn height(&self) -> i32 {
        self.y1.saturating_sub(self.y0).max(0)
    }

    /// Grow to cover a point.
    pub fn include_point(&mut self, point: Point) -> IRect {
        let grown = outward_to_irect(point.x, point.y, point.x, point.y);
        self.x0 = self.x0.min(grown.x0);
        self.y0 = self.y0.min(grown.y0);
        self.x1 = self.x1.max(grown.x1);
        self.y1 = self.y1.max(grown.y1);
        *self
    }

    /// Grow to cover another integer rectangle.
    pub fn include_rect(&mut self, other: IRect) -> IRect {
        self.x0 = self.x0.min(other.x0);
        self.y0 = self.y0.min(other.y0);
        self.x1 = self.x1.max(other.x1);
        self.y1 = self.y1.max(other.y1);
        *self
    }

    /// Shrink to the overlap with another integer rectangle.
    pub fn intersect(&mut self, other: IRect) -> IRect {
        self.x0 = self.x0.max(other.x0);
        self.y0 = self.y0.max(other.y0);
        self.x1 = self.x1.min(other.x1);
        self.y1 = self.y1.min(other.y1);
        *self
    }

    /// Whether this overlaps another integer rectangle.
    pub fn intersects(&self, other: IRect) -> bool {
        self.x0.max(other.x0) < self.x1.min(other.x1)
            && self.y0.max(other.y0) < self.y1.min(other.y1)
    }

    /// Whether the rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// Whether the rectangle is infinite.
    pub fn is_infinite(&self) -> bool {
        self.x0 == i32::MIN
            && self.y0 == i32::MIN
            && self.x1 == 2_147_483_520
            && self.y1 == 2_147_483_520
    }

    /// Whether the rectangle is valid.
    pub fn is_valid(&self) -> bool {
        self.x0 <= self.x1 && self.y0 <= self.y1
    }

    /// Morphed quadrilateral around a fixed point.
    pub fn morph(&self, point: Point, matrix: Matrix) -> Quad {
        self.quad().morph(point, matrix)
    }

    /// Euclidean norm of the edges.
    pub fn norm(&self) -> f64 {
        let edge = |v: i32| {
            let v = f64::from(v);
            v * v
        };
        (edge(self.x0) + edge(self.y0) + edge(self.x1) + edge(self.y1)).sqrt()
    }

    /// Order the edges and return the result.
    pub fn normalize(&mut self) -> IRect {
        if self.x0 > self.x1 {
            std::mem::swap(&mut self.x0, &mut self.x1);
        }
        if self.y0 > self.y1 {
            std::mem::swap(&mut self.y0, &mut self.y1);
        }
        *self
    }

    /// Quadrilateral over the four corners.
    pub fn quad(&self) -> Quad {
        Quad {
            ul: self.top_left(),
            ur: self.top_right(),
            ll: self.bottom_left(),
            lr: self.bottom_right(),
        }
    }

    /// Float copy of the rectangle.
    pub fn rect(&self) -> Rect {
        Rect {
            x0: f64::from(self.x0),
            y0: f64::from(self.y0),
            x1: f64::from(self.x1),
            y1: f64::from(self.y1),
        }
    }

    /// Top-left corner.
    pub fn tl(&self) -> Point {
        Point {
            x: f64::from(self.x0),
            y: f64::from(self.y0),
        }
    }

    /// Top-left corner.
    pub fn top_left(&self) -> Point {
        self.tl()
    }

    /// Top-right corner.
    pub fn top_right(&self) -> Point {
        Point {
            x: f64::from(self.x1),
            y: f64::from(self.y0),
        }
    }

    /// Matrix mapping this onto another integer rectangle.
    pub fn torect(&self, target: IRect) -> Matrix {
        let sx = (f64::from(target.x1) - f64::from(target.x0))
            / (f64::from(self.x1) - f64::from(self.x0));
        let sy = (f64::from(target.y1) - f64::from(target.y0))
            / (f64::from(self.y1) - f64::from(self.y0));
        Matrix {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: f64::from(target.x0) - f64::from(self.x0) * sx,
            f: f64::from(target.y0) - f64::from(self.y0) * sy,
        }
    }

    /// Top-right corner.
    pub fn tr(&self) -> Point {
        self.top_right()
    }

    /// Transform in place and return the result.
    pub fn transform(&mut self, matrix: Matrix) -> IRect {
        *self = *self * matrix;
        *self
    }

    /// Width in pixels, clamped at zero for inverted edges.
    pub fn width(&self) -> i32 {
        self.x1.saturating_sub(self.x0).max(0)
    }

    /// Area in the given units.
    pub fn get_area_in(&self, unit: AreaUnit) -> f64 {
        area_in(self.get_area(), unit)
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("integer rectangles have no ordering"))
    }
}

impl Point {
    /// Build a point from two coordinates.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Point moved by a matrix, returned as a new value.
    fn transformed(self, matrix: Matrix) -> Point {
        Point {
            x: self.x * matrix.a + self.y * matrix.c + matrix.e,
            y: self.x * matrix.b + self.y * matrix.d + matrix.f,
        }
    }

    /// Length of the point.
    pub fn abs(&self) -> f64 {
        self.x.hypot(self.y)
    }

    /// Whether any coordinate is nonzero.
    pub fn to_bool(&self) -> bool {
        self.x != 0.0 || self.y != 0.0
    }

    /// Length without emptiness: the spec has no empty check here.
    #[allow(clippy::len_without_is_empty)]
    /// Coordinate count, always two.
    pub fn len(&self) -> usize {
        2
    }

    /// Copy of the point.
    pub fn pos(&self) -> Self {
        *self
    }

    /// Unit vector of the absolute point.
    pub fn abs_unit(&self) -> Point {
        Point {
            x: self.x.abs(),
            y: self.y.abs(),
        }
        .unit()
    }

    /// Distance to another point.
    pub fn distance_to(&self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    /// Length of the point.
    pub fn norm(&self) -> f64 {
        self.abs()
    }

    /// Transform in place and return the result.
    pub fn transform(&mut self, matrix: Matrix) -> Point {
        *self = self.transformed(matrix);
        *self
    }

    /// Unit vector in the same direction.
    pub fn unit(&self) -> Point {
        let length = self.abs();
        if length == 0.0 {
            *self
        } else {
            Point {
                x: self.x / length,
                y: self.y / length,
            }
        }
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("points have no ordering"))
    }
}

impl Matrix {
    /// Build a matrix from six coefficients.
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// Determinant of the linear part; zero means the matrix has no inverse.
    fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Product of two matrices, `one` applied before `two`.
    fn concat_of(one: Matrix, two: Matrix) -> Matrix {
        Matrix {
            a: one.a * two.a + one.b * two.c,
            b: one.a * two.b + one.b * two.d,
            c: one.c * two.a + one.d * two.c,
            d: one.c * two.b + one.d * two.d,
            e: one.e * two.a + one.f * two.c + two.e,
            f: one.e * two.b + one.f * two.d + two.f,
        }
    }

    /// Norm of the matrix.
    pub fn abs(&self) -> f64 {
        self.norm()
    }

    /// Whether any coefficient is nonzero.
    pub fn to_bool(&self) -> bool {
        self.a != 0.0
            || self.b != 0.0
            || self.c != 0.0
            || self.d != 0.0
            || self.e != 0.0
            || self.f != 0.0
    }

    /// Length without emptiness: the spec has no empty check here.
    #[allow(clippy::len_without_is_empty)]
    /// Coefficient count, always six.
    pub fn len(&self) -> usize {
        6
    }

    /// Copy of the matrix.
    pub fn pos(&self) -> Self {
        *self
    }

    /// Set this to the product of two matrices and return it.
    pub fn concat(&mut self, one: Matrix, two: Matrix) -> Matrix {
        *self = Self::concat_of(one, two);
        *self
    }

    /// Inverse of this matrix, or of the given one.
    ///
    /// A singular input has no inverse and comes back unchanged.
    pub fn invert(&self, src: Option<Matrix>) -> Matrix {
        let m = src.unwrap_or(*self);
        let det = m.determinant();
        if det == 0.0 {
            return m;
        }
        Matrix {
            a: m.d / det,
            b: -m.b / det,
            c: -m.c / det,
            d: m.a / det,
            e: (m.c * m.f - m.e * m.d) / det,
            f: (m.e * m.b - m.a * m.f) / det,
        }
    }

    /// Whether the matrix keeps axes aligned.
    pub fn is_rectilinear(&self) -> bool {
        (self.b == 0.0 && self.c == 0.0) || (self.a == 0.0 && self.d == 0.0)
    }

    /// Norm of the matrix.
    pub fn norm(&self) -> f64 {
        (self.a * self.a
            + self.b * self.b
            + self.c * self.c
            + self.d * self.d
            + self.e * self.e
            + self.f * self.f)
            .sqrt()
    }

    /// Rotate in place and return the result.
    pub fn prerotate(&mut self, theta: f64) -> Matrix {
        let angle = theta.to_radians();
        let rotation = Matrix {
            a: angle.cos(),
            b: angle.sin(),
            c: -angle.sin(),
            d: angle.cos(),
            e: 0.0,
            f: 0.0,
        };
        *self = Self::concat_of(rotation, *self);
        *self
    }

    /// Scale in place and return the result.
    pub fn prescale(&mut self, sx: f64, sy: f64) -> Matrix {
        let scale = Matrix {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        };
        *self = Self::concat_of(scale, *self);
        *self
    }

    /// Shear in place and return the result.
    pub fn preshear(&mut self, h: f64, v: f64) -> Matrix {
        let shear = Matrix {
            a: 1.0,
            b: v,
            c: h,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        };
        *self = Self::concat_of(shear, *self);
        *self
    }

    /// Translate in place and return the result.
    pub fn pretranslate(&mut self, tx: f64, ty: f64) -> Matrix {
        let shift = Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        };
        *self = Self::concat_of(shift, *self);
        *self
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("matrices have no ordering"))
    }
}

impl Quad {
    /// Build a quadrilateral from four corners.
    pub fn new(ul: Point, ur: Point, ll: Point, lr: Point) -> Self {
        Self { ul, ur, ll, lr }
    }

    /// Corners in boundary order: upper-left, upper-right, lower-right, lower-left.
    fn ring(&self) -> [Point; 4] {
        [self.ul, self.ur, self.lr, self.ll]
    }

    /// Area of the quadrilateral.
    pub fn abs(&self) -> f64 {
        let ring = self.ring();
        let mut sum = 0.0;
        for i in 0..4 {
            let p = ring[i];
            let q = ring[(i + 1) % 4];
            sum += p.x * q.y - q.x * p.y;
        }
        0.5 * sum.abs()
    }

    /// Whether any corner is nonzero.
    pub fn to_bool(&self) -> bool {
        self.ul.to_bool() || self.ur.to_bool() || self.ll.to_bool() || self.lr.to_bool()
    }

    /// Whether this covers a point.
    pub fn contains_point(&self, point: Point) -> bool {
        let ring = self.ring();
        for i in 0..4 {
            if point_on_segment(point, ring[i], ring[(i + 1) % 4]) {
                return true;
            }
        }
        let mut inside = false;
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            if (a.y > point.y) != (b.y > point.y) {
                let cross_x = a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if point.x < cross_x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Corner count, always four.
    pub fn len(&self) -> usize {
        4
    }

    /// Copy of the quadrilateral.
    pub fn pos(&self) -> Self {
        *self
    }

    /// Height in points.
    pub fn height(&self) -> f64 {
        (self.ll.x - self.ul.x).hypot(self.ll.y - self.ul.y)
    }

    /// Whether the quadrilateral is convex.
    pub fn is_convex(&self) -> bool {
        let ring = self.ring();
        let mut positive = false;
        let mut negative = false;
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            let c = ring[(i + 2) % 4];
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if cross > 0.0 {
                positive = true;
            } else if cross < 0.0 {
                negative = true;
            }
        }
        !(positive && negative)
    }

    /// Whether the quadrilateral is empty.
    pub fn is_empty(&self) -> bool {
        self.rect().is_empty()
    }

    /// Whether the quadrilateral is infinite.
    pub fn is_infinite(&self) -> bool {
        self.rect().is_infinite()
    }

    /// Whether the quadrilateral is a rectangle.
    pub fn is_rectangular(&self) -> bool {
        !self.is_empty()
            && self.ul.x == self.ll.x
            && self.ur.x == self.lr.x
            && self.ul.y == self.ur.y
            && self.ll.y == self.lr.y
    }

    /// Morphed quadrilateral around a fixed point.
    pub fn morph(&self, point: Point, matrix: Matrix) -> Quad {
        let morph_corner = |corner: Point| {
            let shifted = Point {
                x: corner.x - point.x,
                y: corner.y - point.y,
            }
            .transformed(matrix);
            Point {
                x: point.x + shifted.x,
                y: point.y + shifted.y,
            }
        };
        Quad {
            ul: morph_corner(self.ul),
            ur: morph_corner(self.ur),
            ll: morph_corner(self.ll),
            lr: morph_corner(self.lr),
        }
    }

    /// Smallest covering rectangle.
    pub fn rect(&self) -> Rect {
        let xs = [self.ul.x, self.ur.x, self.ll.x, self.lr.x];
        let ys = [self.ul.y, self.ur.y, self.ll.y, self.lr.y];
        Rect {
            x0: xs.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            y0: ys.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            x1: xs.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
            y1: ys.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
        }
    }

    /// Transform in place and return the result.
    pub fn transform(&mut self, matrix: Matrix) -> Quad {
        self.ul = self.ul.transformed(matrix);
        self.ur = self.ur.transformed(matrix);
        self.ll = self.ll.transformed(matrix);
        self.lr = self.lr.transformed(matrix);
        *self
    }

    /// Width in points.
    pub fn width(&self) -> f64 {
        (self.ur.x - self.ul.x).hypot(self.ur.y - self.ul.y)
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("quadrilaterals have no ordering"))
    }
}

impl Add<IRect> for IRect {
    type Output = Self;
    fn add(self, rhs: IRect) -> Self {
        Self {
            x0: self.x0.wrapping_add(rhs.x0),
            y0: self.y0.wrapping_add(rhs.y0),
            x1: self.x1.wrapping_add(rhs.x1),
            y1: self.y1.wrapping_add(rhs.y1),
        }
    }
}

impl Add<i32> for IRect {
    type Output = Self;
    fn add(self, rhs: i32) -> Self {
        Self {
            x0: self.x0.wrapping_add(rhs),
            y0: self.y0.wrapping_add(rhs),
            x1: self.x1.wrapping_add(rhs),
            y1: self.y1.wrapping_add(rhs),
        }
    }
}

impl Sub<IRect> for IRect {
    type Output = Self;
    fn sub(self, rhs: IRect) -> Self {
        Self {
            x0: self.x0.wrapping_sub(rhs.x0),
            y0: self.y0.wrapping_sub(rhs.y0),
            x1: self.x1.wrapping_sub(rhs.x1),
            y1: self.y1.wrapping_sub(rhs.y1),
        }
    }
}

impl Sub<i32> for IRect {
    type Output = Self;
    fn sub(self, rhs: i32) -> Self {
        Self {
            x0: self.x0.wrapping_sub(rhs),
            y0: self.y0.wrapping_sub(rhs),
            x1: self.x1.wrapping_sub(rhs),
            y1: self.y1.wrapping_sub(rhs),
        }
    }
}

impl Mul<Matrix> for IRect {
    type Output = Self;
    fn mul(self, rhs: Matrix) -> Self {
        let moved = self.rect() * rhs;
        outward_to_irect(moved.x0, moved.y0, moved.x1, moved.y1)
    }
}

impl Mul<i32> for IRect {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self {
            x0: self.x0.wrapping_mul(rhs),
            y0: self.y0.wrapping_mul(rhs),
            x1: self.x1.wrapping_mul(rhs),
            y1: self.y1.wrapping_mul(rhs),
        }
    }
}

impl Div<Matrix> for IRect {
    type Output = Self;
    fn div(self, rhs: Matrix) -> Self {
        if rhs.determinant() == 0.0 {
            self
        } else {
            self * rhs.invert(None)
        }
    }
}

impl Div<i32> for IRect {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        assert!(rhs != 0, "integer rectangle division by zero");
        let divisor = f64::from(rhs);
        let edge = |value: i32, low: bool| {
            let quotient = f64::from(value) / divisor;
            (if low {
                quotient.floor()
            } else {
                quotient.ceil()
            }) as i32
        };
        Self {
            x0: edge(self.x0, true),
            y0: edge(self.y0, true),
            x1: edge(self.x1, false),
            y1: edge(self.y1, false),
        }
    }
}

impl Neg for IRect {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x0: self.x0.wrapping_neg(),
            y0: self.y0.wrapping_neg(),
            x1: self.x1.wrapping_neg(),
            y1: self.y1.wrapping_neg(),
        }
    }
}

impl BitAnd<IRect> for IRect {
    type Output = Self;
    fn bitand(self, rhs: IRect) -> Self {
        Self {
            x0: self.x0.max(rhs.x0),
            y0: self.y0.max(rhs.y0),
            x1: self.x1.min(rhs.x1),
            y1: self.y1.min(rhs.y1),
        }
    }
}

impl BitOr<IRect> for IRect {
    type Output = Self;
    fn bitor(self, rhs: IRect) -> Self {
        Self {
            x0: self.x0.min(rhs.x0),
            y0: self.y0.min(rhs.y0),
            x1: self.x1.max(rhs.x1),
            y1: self.y1.max(rhs.y1),
        }
    }
}

impl Index<usize> for IRect {
    type Output = i32;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x0,
            1 => &self.y0,
            2 => &self.x1,
            3 => &self.y1,
            _ => panic!("integer rectangle index out of range: {index}"),
        }
    }
}

impl IndexMut<usize> for IRect {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x0,
            1 => &mut self.y0,
            2 => &mut self.x1,
            3 => &mut self.y1,
            _ => panic!("integer rectangle index out of range: {index}"),
        }
    }
}

impl Add<Matrix> for Matrix {
    type Output = Self;
    fn add(self, rhs: Matrix) -> Self {
        Self {
            a: self.a + rhs.a,
            b: self.b + rhs.b,
            c: self.c + rhs.c,
            d: self.d + rhs.d,
            e: self.e + rhs.e,
            f: self.f + rhs.f,
        }
    }
}

impl Add<f64> for Matrix {
    type Output = Self;
    fn add(self, rhs: f64) -> Self {
        Self {
            a: self.a + rhs,
            b: self.b + rhs,
            c: self.c + rhs,
            d: self.d + rhs,
            e: self.e + rhs,
            f: self.f + rhs,
        }
    }
}

impl Sub<Matrix> for Matrix {
    type Output = Self;
    fn sub(self, rhs: Matrix) -> Self {
        Self {
            a: self.a - rhs.a,
            b: self.b - rhs.b,
            c: self.c - rhs.c,
            d: self.d - rhs.d,
            e: self.e - rhs.e,
            f: self.f - rhs.f,
        }
    }
}

impl Sub<f64> for Matrix {
    type Output = Self;
    fn sub(self, rhs: f64) -> Self {
        Self {
            a: self.a - rhs,
            b: self.b - rhs,
            c: self.c - rhs,
            d: self.d - rhs,
            e: self.e - rhs,
            f: self.f - rhs,
        }
    }
}

impl Mul<Matrix> for Matrix {
    type Output = Self;
    fn mul(self, rhs: Matrix) -> Self {
        Self::concat_of(self, rhs)
    }
}

impl Mul<f64> for Matrix {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            a: self.a * rhs,
            b: self.b * rhs,
            c: self.c * rhs,
            d: self.d * rhs,
            e: self.e * rhs,
            f: self.f * rhs,
        }
    }
}

impl Div<Matrix> for Matrix {
    type Output = Self;
    fn div(self, rhs: Matrix) -> Self {
        if rhs.determinant() == 0.0 {
            self
        } else {
            self * rhs.invert(None)
        }
    }
}

impl Div<f64> for Matrix {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self {
            a: self.a / rhs,
            b: self.b / rhs,
            c: self.c / rhs,
            d: self.d / rhs,
            e: self.e / rhs,
            f: self.f / rhs,
        }
    }
}

impl Neg for Matrix {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            a: -self.a,
            b: -self.b,
            c: -self.c,
            d: -self.d,
            e: -self.e,
            f: -self.f,
        }
    }
}

impl Index<usize> for Matrix {
    type Output = f64;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.a,
            1 => &self.b,
            2 => &self.c,
            3 => &self.d,
            4 => &self.e,
            5 => &self.f,
            _ => panic!("matrix index out of range: {index}"),
        }
    }
}

impl IndexMut<usize> for Matrix {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.a,
            1 => &mut self.b,
            2 => &mut self.c,
            3 => &mut self.d,
            4 => &mut self.e,
            5 => &mut self.f,
            _ => panic!("matrix index out of range: {index}"),
        }
    }
}

impl Add<Point> for Point {
    type Output = Self;
    fn add(self, rhs: Point) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Add<f64> for Point {
    type Output = Self;
    fn add(self, rhs: f64) -> Self {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

impl Sub<Point> for Point {
    type Output = Self;
    fn sub(self, rhs: Point) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Sub<f64> for Point {
    type Output = Self;
    fn sub(self, rhs: f64) -> Self {
        Self {
            x: self.x - rhs,
            y: self.y - rhs,
        }
    }
}

impl Mul<Point> for Point {
    type Output = f64;
    fn mul(self, rhs: Point) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }
}

impl Mul<Matrix> for Point {
    type Output = Self;
    fn mul(self, rhs: Matrix) -> Self {
        self.transformed(rhs)
    }
}

impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Div<Matrix> for Point {
    type Output = Self;
    fn div(self, rhs: Matrix) -> Self {
        if rhs.determinant() == 0.0 {
            self
        } else {
            self * rhs.invert(None)
        }
    }
}

impl Div<f64> for Point {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl Neg for Point {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl Index<usize> for Point {
    type Output = f64;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("point index out of range: {index}"),
        }
    }
}

impl IndexMut<usize> for Point {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("point index out of range: {index}"),
        }
    }
}

impl Add<Quad> for Quad {
    type Output = Self;
    fn add(self, rhs: Quad) -> Self {
        Self {
            ul: self.ul + rhs.ul,
            ur: self.ur + rhs.ur,
            ll: self.ll + rhs.ll,
            lr: self.lr + rhs.lr,
        }
    }
}

impl Add<f64> for Quad {
    type Output = Self;
    fn add(self, rhs: f64) -> Self {
        Self {
            ul: self.ul + rhs,
            ur: self.ur + rhs,
            ll: self.ll + rhs,
            lr: self.lr + rhs,
        }
    }
}

impl Sub<Quad> for Quad {
    type Output = Self;
    fn sub(self, rhs: Quad) -> Self {
        Self {
            ul: self.ul - rhs.ul,
            ur: self.ur - rhs.ur,
            ll: self.ll - rhs.ll,
            lr: self.lr - rhs.lr,
        }
    }
}

impl Sub<f64> for Quad {
    type Output = Self;
    fn sub(self, rhs: f64) -> Self {
        Self {
            ul: self.ul - rhs,
            ur: self.ur - rhs,
            ll: self.ll - rhs,
            lr: self.lr - rhs,
        }
    }
}

impl Mul<Matrix> for Quad {
    type Output = Self;
    fn mul(self, rhs: Matrix) -> Self {
        Self {
            ul: self.ul.transformed(rhs),
            ur: self.ur.transformed(rhs),
            ll: self.ll.transformed(rhs),
            lr: self.lr.transformed(rhs),
        }
    }
}

impl Mul<f64> for Quad {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            ul: self.ul * rhs,
            ur: self.ur * rhs,
            ll: self.ll * rhs,
            lr: self.lr * rhs,
        }
    }
}

impl Div<Matrix> for Quad {
    type Output = Self;
    fn div(self, rhs: Matrix) -> Self {
        if rhs.determinant() == 0.0 {
            self
        } else {
            self * rhs.invert(None)
        }
    }
}

impl Div<f64> for Quad {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self {
            ul: self.ul / rhs,
            ur: self.ur / rhs,
            ll: self.ll / rhs,
            lr: self.lr / rhs,
        }
    }
}

impl Neg for Quad {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            ul: -self.ul,
            ur: -self.ur,
            ll: -self.ll,
            lr: -self.lr,
        }
    }
}

impl Index<usize> for Quad {
    type Output = Point;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.ul,
            1 => &self.ur,
            2 => &self.ll,
            3 => &self.lr,
            _ => panic!("quadrilateral index out of range: {index}"),
        }
    }
}

impl IndexMut<usize> for Quad {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.ul,
            1 => &mut self.ur,
            2 => &mut self.ll,
            3 => &mut self.lr,
            _ => panic!("quadrilateral index out of range: {index}"),
        }
    }
}

impl Add<Rect> for Rect {
    type Output = Self;
    fn add(self, rhs: Rect) -> Self {
        Self {
            x0: self.x0 + rhs.x0,
            y0: self.y0 + rhs.y0,
            x1: self.x1 + rhs.x1,
            y1: self.y1 + rhs.y1,
        }
    }
}

impl Add<f64> for Rect {
    type Output = Self;
    fn add(self, rhs: f64) -> Self {
        Self {
            x0: self.x0 + rhs,
            y0: self.y0 + rhs,
            x1: self.x1 + rhs,
            y1: self.y1 + rhs,
        }
    }
}

impl Sub<Rect> for Rect {
    type Output = Self;
    fn sub(self, rhs: Rect) -> Self {
        Self {
            x0: self.x0 - rhs.x0,
            y0: self.y0 - rhs.y0,
            x1: self.x1 - rhs.x1,
            y1: self.y1 - rhs.y1,
        }
    }
}

impl Sub<f64> for Rect {
    type Output = Self;
    fn sub(self, rhs: f64) -> Self {
        Self {
            x0: self.x0 - rhs,
            y0: self.y0 - rhs,
            x1: self.x1 - rhs,
            y1: self.y1 - rhs,
        }
    }
}

impl Mul<Matrix> for Rect {
    type Output = Self;
    fn mul(self, rhs: Matrix) -> Self {
        let at = |x: f64, y: f64| Point::new(x, y).transformed(rhs);
        let p0 = at(self.x0, self.y0);
        let p1 = at(self.x1, self.y0);
        let p2 = at(self.x0, self.y1);
        let p3 = at(self.x1, self.y1);
        Self {
            x0: p0.x.min(p1.x).min(p2.x).min(p3.x),
            y0: p0.y.min(p1.y).min(p2.y).min(p3.y),
            x1: p0.x.max(p1.x).max(p2.x).max(p3.x),
            y1: p0.y.max(p1.y).max(p2.y).max(p3.y),
        }
    }
}

impl Mul<f64> for Rect {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x0: self.x0 * rhs,
            y0: self.y0 * rhs,
            x1: self.x1 * rhs,
            y1: self.y1 * rhs,
        }
    }
}

impl Div<Matrix> for Rect {
    type Output = Self;
    fn div(self, rhs: Matrix) -> Self {
        if rhs.determinant() == 0.0 {
            self
        } else {
            self * rhs.invert(None)
        }
    }
}

impl Div<f64> for Rect {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self {
            x0: self.x0 / rhs,
            y0: self.y0 / rhs,
            x1: self.x1 / rhs,
            y1: self.y1 / rhs,
        }
    }
}

impl Neg for Rect {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x0: -self.x0,
            y0: -self.y0,
            x1: -self.x1,
            y1: -self.y1,
        }
    }
}

impl BitAnd<Rect> for Rect {
    type Output = Self;
    fn bitand(self, rhs: Rect) -> Self {
        Self {
            x0: self.x0.max(rhs.x0),
            y0: self.y0.max(rhs.y0),
            x1: self.x1.min(rhs.x1),
            y1: self.y1.min(rhs.y1),
        }
    }
}

impl BitOr<Rect> for Rect {
    type Output = Self;
    fn bitor(self, rhs: Rect) -> Self {
        Self {
            x0: self.x0.min(rhs.x0),
            y0: self.y0.min(rhs.y0),
            x1: self.x1.max(rhs.x1),
            y1: self.y1.max(rhs.y1),
        }
    }
}

impl Index<usize> for Rect {
    type Output = f64;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x0,
            1 => &self.y0,
            2 => &self.x1,
            3 => &self.y1,
            _ => panic!("rectangle index out of range: {index}"),
        }
    }
}

impl IndexMut<usize> for Rect {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x0,
            1 => &mut self.y0,
            2 => &mut self.x1,
            3 => &mut self.y1,
            _ => panic!("rectangle index out of range: {index}"),
        }
    }
}
