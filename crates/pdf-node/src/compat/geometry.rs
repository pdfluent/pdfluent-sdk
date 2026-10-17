//! Compatible value types: geometry classes shared by every lane.
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi(object)]
pub struct PointLike {
    pub x: f64,
    pub y: f64,
}

impl PointLike {
    pub(crate) fn to_compat(&self) -> fc::Point {
        fc::Point {
            x: self.x,
            y: self.y,
        }
    }
}

#[napi(object)]
pub struct RectLike {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl RectLike {
    pub(crate) fn to_compat(&self) -> fc::Rect {
        fc::Rect {
            x0: self.x0,
            y0: self.y0,
            x1: self.x1,
            y1: self.y1,
        }
    }
}

#[napi(object)]
pub struct IRectLike {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRectLike {
    pub(crate) fn to_compat(&self) -> fc::IRect {
        fc::IRect {
            x0: self.x0,
            y0: self.y0,
            x1: self.x1,
            y1: self.y1,
        }
    }
}

#[napi(object)]
pub struct MatrixLike {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl MatrixLike {
    pub(crate) fn to_compat(&self) -> fc::Matrix {
        fc::Matrix {
            a: self.a,
            b: self.b,
            c: self.c,
            d: self.d,
            e: self.e,
            f: self.f,
        }
    }
}

#[napi(object)]
pub struct QuadLike {
    pub ul: PointLike,
    pub ur: PointLike,
    pub ll: PointLike,
    pub lr: PointLike,
}

impl QuadLike {
    pub(crate) fn to_compat(&self) -> fc::Quad {
        fc::Quad {
            ul: self.ul.to_compat(),
            ur: self.ur.to_compat(),
            ll: self.ll.to_compat(),
            lr: self.lr.to_compat(),
        }
    }
}

#[napi]
pub struct Point {
    pub(crate) inner: fc::Point,
}

#[napi]
pub struct Rect {
    pub(crate) inner: fc::Rect,
}

#[napi]
pub struct IRect {
    pub(crate) inner: fc::IRect,
}

#[napi]
pub struct Matrix {
    pub(crate) inner: fc::Matrix,
}

#[napi]
pub struct Quad {
    pub(crate) inner: fc::Quad,
}

#[napi]
impl Point {
    /// Read the x component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x")]
    pub fn x(&self) -> f64 {
        self.inner.x
    }

    /// Write the x component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "x")]
    pub fn set_x(&mut self, value: f64) {
        self.inner.x = value;
    }

    /// Read the y component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y")]
    pub fn y(&self) -> f64 {
        self.inner.y
    }

    /// Write the y component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "y")]
    pub fn set_y(&mut self, value: f64) {
        self.inner.y = value;
    }
}

#[napi]
impl Rect {
    /// Read the x0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x0")]
    pub fn x0(&self) -> f64 {
        self.inner.x0
    }

    /// Write the x0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "x0")]
    pub fn set_x0(&mut self, value: f64) {
        self.inner.x0 = value;
    }

    /// Read the y0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y0")]
    pub fn y0(&self) -> f64 {
        self.inner.y0
    }

    /// Write the y0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "y0")]
    pub fn set_y0(&mut self, value: f64) {
        self.inner.y0 = value;
    }

    /// Read the x1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x1")]
    pub fn x1(&self) -> f64 {
        self.inner.x1
    }

    /// Write the x1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "x1")]
    pub fn set_x1(&mut self, value: f64) {
        self.inner.x1 = value;
    }

    /// Read the y1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y1")]
    pub fn y1(&self) -> f64 {
        self.inner.y1
    }

    /// Write the y1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "y1")]
    pub fn set_y1(&mut self, value: f64) {
        self.inner.y1 = value;
    }
}

#[napi]
impl IRect {
    /// Read the x0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x0")]
    pub fn x0(&self) -> i32 {
        self.inner.x0
    }

    /// Write the x0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "x0")]
    pub fn set_x0(&mut self, value: i32) {
        self.inner.x0 = value;
    }

    /// Read the y0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y0")]
    pub fn y0(&self) -> i32 {
        self.inner.y0
    }

    /// Write the y0 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "y0")]
    pub fn set_y0(&mut self, value: i32) {
        self.inner.y0 = value;
    }

    /// Read the x1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x1")]
    pub fn x1(&self) -> i32 {
        self.inner.x1
    }

    /// Write the x1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "x1")]
    pub fn set_x1(&mut self, value: i32) {
        self.inner.x1 = value;
    }

    /// Read the y1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y1")]
    pub fn y1(&self) -> i32 {
        self.inner.y1
    }

    /// Write the y1 component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "y1")]
    pub fn set_y1(&mut self, value: i32) {
        self.inner.y1 = value;
    }
}

#[napi]
impl Matrix {
    /// Read the a component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "a")]
    pub fn a(&self) -> f64 {
        self.inner.a
    }

    /// Write the a component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "a")]
    pub fn set_a(&mut self, value: f64) {
        self.inner.a = value;
    }

    /// Read the b component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "b")]
    pub fn b(&self) -> f64 {
        self.inner.b
    }

    /// Write the b component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "b")]
    pub fn set_b(&mut self, value: f64) {
        self.inner.b = value;
    }

    /// Read the c component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "c")]
    pub fn c(&self) -> f64 {
        self.inner.c
    }

    /// Write the c component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "c")]
    pub fn set_c(&mut self, value: f64) {
        self.inner.c = value;
    }

    /// Read the d component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "d")]
    pub fn d(&self) -> f64 {
        self.inner.d
    }

    /// Write the d component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "d")]
    pub fn set_d(&mut self, value: f64) {
        self.inner.d = value;
    }

    /// Read the e component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "e")]
    pub fn e(&self) -> f64 {
        self.inner.e
    }

    /// Write the e component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "e")]
    pub fn set_e(&mut self, value: f64) {
        self.inner.e = value;
    }

    /// Read the f component.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "f")]
    pub fn f(&self) -> f64 {
        self.inner.f
    }

    /// Write the f component.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "f")]
    pub fn set_f(&mut self, value: f64) {
        self.inner.f = value;
    }
}

#[napi]
impl Quad {
    /// Read the ul corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "ul")]
    pub fn ul(&self) -> Point {
        Point {
            inner: self.inner.ul,
        }
    }

    /// Write the ul corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "ul")]
    pub fn set_ul(&mut self, value: PointLike) {
        self.inner.ul = value.to_compat();
    }

    /// Read the ur corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "ur")]
    pub fn ur(&self) -> Point {
        Point {
            inner: self.inner.ur,
        }
    }

    /// Write the ur corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "ur")]
    pub fn set_ur(&mut self, value: PointLike) {
        self.inner.ur = value.to_compat();
    }

    /// Read the ll corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "ll")]
    pub fn ll(&self) -> Point {
        Point {
            inner: self.inner.ll,
        }
    }

    /// Write the ll corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "ll")]
    pub fn set_ll(&mut self, value: PointLike) {
        self.inner.ll = value.to_compat();
    }

    /// Read the lr corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "lr")]
    pub fn lr(&self) -> Point {
        Point {
            inner: self.inner.lr,
        }
    }

    /// Write the lr corner.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "lr")]
    pub fn set_lr(&mut self, value: PointLike) {
        self.inner.lr = value.to_compat();
    }
}

#[napi]
impl IRect {
    /// IRect.__add__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "add")]
    pub fn add(&self, other: IRectLike) -> IRect {
        IRect {
            inner: self.inner + other.to_compat(),
        }
    }

    /// IRect.__contains__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "containsOp")]
    pub fn contains_point(&self, point: PointLike) -> napi::Result<i32> {
        let out = self.inner.contains_point(point.to_compat());
        Ok(out)
    }

    /// IRect.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: IRectLike) -> bool {
        self.inner == other.to_compat()
    }

    /// IRect.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: IRectLike) -> napi::Result<bool> {
        let op = "ge";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// IRect.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn getitem(&self, i: u32) -> napi::Result<i32> {
        let op = "getitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        Ok(self.inner[i as usize])
    }

    /// IRect.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: IRectLike) -> napi::Result<bool> {
        let op = "gt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// IRect.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(x0: Option<i32>, y0: Option<i32>, x1: Option<i32>, y1: Option<i32>) -> Self {
        Self {
            inner: fc::IRect::new(
                x0.unwrap_or(0),
                y0.unwrap_or(0),
                x1.unwrap_or(0),
                y1.unwrap_or(0),
            ),
        }
    }

    /// IRect.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: IRectLike) -> napi::Result<bool> {
        let op = "le";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// IRect.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// IRect.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: IRectLike) -> napi::Result<bool> {
        let op = "lt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// IRect.__mul__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "mul")]
    pub fn mul(&self, matrix: MatrixLike) -> IRect {
        IRect {
            inner: self.inner * matrix.to_compat(),
        }
    }

    /// IRect.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: IRectLike) -> bool {
        self.inner != other.to_compat()
    }

    /// IRect.__neg__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "neg")]
    pub fn neg(&self) -> IRect {
        IRect { inner: -self.inner }
    }

    /// IRect.__pos__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pos")]
    pub fn pos(&self) -> napi::Result<IRect> {
        let out = self.inner.pos();
        Ok(IRect { inner: out })
    }

    /// IRect.__setitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setitem")]
    pub fn setitem(&mut self, i: u32, value: i32) -> napi::Result<()> {
        let op = "setitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        self.inner[i as usize] = value;
        Ok(())
    }

    /// IRect.__sub__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "sub")]
    pub fn sub(&self, other: IRectLike) -> IRect {
        IRect {
            inner: self.inner - other.to_compat(),
        }
    }

    /// IRect.__truediv__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "truediv")]
    pub fn div(&self, matrix: MatrixLike) -> IRect {
        IRect {
            inner: self.inner / matrix.to_compat(),
        }
    }

    /// IRect.bl from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bl")]
    pub fn bl(&self) -> napi::Result<Point> {
        let out = self.inner.bl();
        Ok(Point { inner: out })
    }

    /// IRect.bottom_left from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bottomLeft")]
    pub fn bottom_left(&self) -> napi::Result<Point> {
        let out = self.inner.bottom_left();
        Ok(Point { inner: out })
    }

    /// IRect.bottom_right from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bottomRight")]
    pub fn bottom_right(&self) -> napi::Result<Point> {
        let out = self.inner.bottom_right();
        Ok(Point { inner: out })
    }

    /// IRect.br from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "br")]
    pub fn br(&self) -> napi::Result<Point> {
        let out = self.inner.br();
        Ok(Point { inner: out })
    }

    /// IRect.contains from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "contains")]
    pub fn contains_rect(&self, other: IRectLike) -> napi::Result<bool> {
        let out = self.inner.contains_rect(other.to_compat());
        Ok(out)
    }

    /// IRect.get_area from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getArea")]
    pub fn get_area(&self) -> napi::Result<f64> {
        let out = self.inner.get_area();
        Ok(out)
    }

    /// IRect.height from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "height")]
    pub fn height(&self) -> napi::Result<i32> {
        let out = self.inner.height();
        Ok(out)
    }

    /// IRect.include_point from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "includePoint")]
    pub fn include_point(&mut self, point: PointLike) -> napi::Result<IRect> {
        let out = self.inner.include_point(point.to_compat());
        Ok(IRect { inner: out })
    }

    /// IRect.include_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "includeRect")]
    pub fn include_rect(&mut self, other: IRectLike) -> napi::Result<IRect> {
        let out = self.inner.include_rect(other.to_compat());
        Ok(IRect { inner: out })
    }

    /// IRect.intersect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "intersect")]
    pub fn intersect(&mut self, other: IRectLike) -> napi::Result<IRect> {
        let out = self.inner.intersect(other.to_compat());
        Ok(IRect { inner: out })
    }

    /// IRect.intersects from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "intersects")]
    pub fn intersects(&self, other: IRectLike) -> napi::Result<bool> {
        let out = self.inner.intersects(other.to_compat());
        Ok(out)
    }

    /// IRect.is_empty from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isEmpty")]
    pub fn is_empty(&self) -> napi::Result<bool> {
        let out = self.inner.is_empty();
        Ok(out)
    }

    /// IRect.is_infinite from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isInfinite")]
    pub fn is_infinite(&self) -> napi::Result<bool> {
        let out = self.inner.is_infinite();
        Ok(out)
    }

    /// IRect.is_valid from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isValid")]
    pub fn is_valid(&self) -> napi::Result<bool> {
        let out = self.inner.is_valid();
        Ok(out)
    }

    /// IRect.morph from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "morph")]
    pub fn morph(&self, point: PointLike, matrix: MatrixLike) -> napi::Result<Quad> {
        let out = self.inner.morph(point.to_compat(), matrix.to_compat());
        Ok(Quad { inner: out })
    }

    /// IRect.norm from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "norm")]
    pub fn norm(&self) -> napi::Result<f64> {
        let out = self.inner.norm();
        Ok(out)
    }

    /// IRect.normalize from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "normalize")]
    pub fn normalize(&mut self) -> napi::Result<IRect> {
        let out = self.inner.normalize();
        Ok(IRect { inner: out })
    }

    /// IRect.quad from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "quad")]
    pub fn quad(&self) -> napi::Result<Quad> {
        let out = self.inner.quad();
        Ok(Quad { inner: out })
    }

    /// IRect.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let out = self.inner.rect();
        Ok(Rect { inner: out })
    }

    /// IRect.tl from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "tl")]
    pub fn tl(&self) -> napi::Result<Point> {
        let out = self.inner.tl();
        Ok(Point { inner: out })
    }

    /// IRect.top_left from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "topLeft")]
    pub fn top_left(&self) -> napi::Result<Point> {
        let out = self.inner.top_left();
        Ok(Point { inner: out })
    }

    /// IRect.top_right from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "topRight")]
    pub fn top_right(&self) -> napi::Result<Point> {
        let out = self.inner.top_right();
        Ok(Point { inner: out })
    }

    /// IRect.torect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "torect")]
    pub fn torect(&self, target: IRectLike) -> napi::Result<Matrix> {
        let out = self.inner.torect(target.to_compat());
        Ok(Matrix { inner: out })
    }

    /// IRect.tr from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "tr")]
    pub fn tr(&self) -> napi::Result<Point> {
        let out = self.inner.tr();
        Ok(Point { inner: out })
    }

    /// IRect.transform from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "transform")]
    pub fn transform(&mut self, matrix: MatrixLike) -> napi::Result<IRect> {
        let out = self.inner.transform(matrix.to_compat());
        Ok(IRect { inner: out })
    }

    /// IRect.width from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "width")]
    pub fn width(&self) -> napi::Result<i32> {
        let out = self.inner.width();
        Ok(out)
    }
}

#[napi]
impl Matrix {
    /// Matrix.__abs__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "abs")]
    pub fn abs(&self) -> napi::Result<f64> {
        let out = self.inner.abs();
        Ok(out)
    }

    /// Matrix.__add__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "add")]
    pub fn add(&self, other: MatrixLike) -> Matrix {
        Matrix {
            inner: self.inner + other.to_compat(),
        }
    }

    /// Matrix.__bool__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bool")]
    pub fn to_bool(&self) -> napi::Result<bool> {
        let out = self.inner.to_bool();
        Ok(out)
    }

    /// Matrix.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: MatrixLike) -> bool {
        self.inner == other.to_compat()
    }

    /// Matrix.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: MatrixLike) -> napi::Result<bool> {
        let op = "ge";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Matrix.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn getitem(&self, i: u32) -> napi::Result<f64> {
        let op = "getitem";
        if 6 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        Ok(self.inner[i as usize])
    }

    /// Matrix.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: MatrixLike) -> napi::Result<bool> {
        let op = "gt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Matrix.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(
        a: Option<f64>,
        b: Option<f64>,
        c: Option<f64>,
        d: Option<f64>,
        e: Option<f64>,
        f: Option<f64>,
    ) -> Self {
        Self {
            inner: fc::Matrix::new(
                a.unwrap_or(0.0),
                b.unwrap_or(0.0),
                c.unwrap_or(0.0),
                d.unwrap_or(0.0),
                e.unwrap_or(0.0),
                f.unwrap_or(0.0),
            ),
        }
    }

    /// Matrix.__invert__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "invertOp")]
    pub fn invert(&self, src: Option<MatrixLike>) -> napi::Result<Matrix> {
        let src_v = src.as_ref().map(MatrixLike::to_compat);
        let out = self.inner.invert(src_v);
        Ok(Matrix { inner: out })
    }

    /// Matrix.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: MatrixLike) -> napi::Result<bool> {
        let op = "le";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Matrix.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// Matrix.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: MatrixLike) -> napi::Result<bool> {
        let op = "lt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Matrix.__mul__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "mul")]
    pub fn mul(&self, matrix: MatrixLike) -> Matrix {
        Matrix {
            inner: self.inner * matrix.to_compat(),
        }
    }

    /// Matrix.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: MatrixLike) -> bool {
        self.inner != other.to_compat()
    }

    /// Matrix.__neg__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "neg")]
    pub fn neg(&self) -> Matrix {
        Matrix { inner: -self.inner }
    }

    /// Matrix.__pos__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pos")]
    pub fn pos(&self) -> napi::Result<Matrix> {
        let out = self.inner.pos();
        Ok(Matrix { inner: out })
    }

    /// Matrix.__setitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setitem")]
    pub fn setitem(&mut self, i: u32, value: f64) -> napi::Result<()> {
        let op = "setitem";
        if 6 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        self.inner[i as usize] = value;
        Ok(())
    }

    /// Matrix.__sub__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "sub")]
    pub fn sub(&self, other: MatrixLike) -> Matrix {
        Matrix {
            inner: self.inner - other.to_compat(),
        }
    }

    /// Matrix.__truediv__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "truediv")]
    pub fn div(&self, matrix: MatrixLike) -> Matrix {
        Matrix {
            inner: self.inner / matrix.to_compat(),
        }
    }

    /// Matrix.concat from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "concat")]
    pub fn concat(&mut self, one: MatrixLike, two: MatrixLike) -> napi::Result<Matrix> {
        let out = self.inner.concat(one.to_compat(), two.to_compat());
        Ok(Matrix { inner: out })
    }

    /// Matrix.invert from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "invert")]
    pub fn invert_2(&self, src: Option<MatrixLike>) -> napi::Result<Matrix> {
        let src_v = src.as_ref().map(MatrixLike::to_compat);
        let out = self.inner.invert(src_v);
        Ok(Matrix { inner: out })
    }

    /// Matrix.is_rectilinear from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isRectilinear")]
    pub fn is_rectilinear(&self) -> napi::Result<bool> {
        let out = self.inner.is_rectilinear();
        Ok(out)
    }

    /// Matrix.norm from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "norm")]
    pub fn norm(&self) -> napi::Result<f64> {
        let out = self.inner.norm();
        Ok(out)
    }

    /// Matrix.prerotate from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "prerotate")]
    pub fn prerotate(&mut self, theta: f64) -> napi::Result<Matrix> {
        let out = self.inner.prerotate(theta);
        Ok(Matrix { inner: out })
    }

    /// Matrix.prescale from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "prescale")]
    pub fn prescale(&mut self, sx: f64, sy: f64) -> napi::Result<Matrix> {
        let out = self.inner.prescale(sx, sy);
        Ok(Matrix { inner: out })
    }

    /// Matrix.preshear from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "preshear")]
    pub fn preshear(&mut self, h: f64, v: f64) -> napi::Result<Matrix> {
        let out = self.inner.preshear(h, v);
        Ok(Matrix { inner: out })
    }

    /// Matrix.pretranslate from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pretranslate")]
    pub fn pretranslate(&mut self, tx: f64, ty: f64) -> napi::Result<Matrix> {
        let out = self.inner.pretranslate(tx, ty);
        Ok(Matrix { inner: out })
    }
}

#[napi]
impl Point {
    /// Point.__abs__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "abs")]
    pub fn abs(&self) -> napi::Result<f64> {
        let out = self.inner.abs();
        Ok(out)
    }

    /// Point.__add__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "add")]
    pub fn add(&self, other: PointLike) -> Point {
        Point {
            inner: self.inner + other.to_compat(),
        }
    }

    /// Point.__bool__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bool")]
    pub fn to_bool(&self) -> napi::Result<bool> {
        let out = self.inner.to_bool();
        Ok(out)
    }

    /// Point.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: PointLike) -> bool {
        self.inner == other.to_compat()
    }

    /// Point.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: PointLike) -> napi::Result<bool> {
        let op = "ge";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Point.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn getitem(&self, i: u32) -> napi::Result<f64> {
        let op = "getitem";
        if 2 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        Ok(self.inner[i as usize])
    }

    /// Point.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: PointLike) -> napi::Result<bool> {
        let op = "gt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Point.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(x: Option<f64>, y: Option<f64>) -> Self {
        Self {
            inner: fc::Point::new(x.unwrap_or(0.0), y.unwrap_or(0.0)),
        }
    }

    /// Point.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: PointLike) -> napi::Result<bool> {
        let op = "le";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Point.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// Point.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: PointLike) -> napi::Result<bool> {
        let op = "lt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Point.__mul__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "mul")]
    pub fn mul(&self, matrix: MatrixLike) -> Point {
        Point {
            inner: self.inner * matrix.to_compat(),
        }
    }

    /// Point.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: PointLike) -> bool {
        self.inner != other.to_compat()
    }

    /// Point.__neg__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "neg")]
    pub fn neg(&self) -> Point {
        Point { inner: -self.inner }
    }

    /// Point.__pos__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pos")]
    pub fn pos(&self) -> napi::Result<Point> {
        let out = self.inner.pos();
        Ok(Point { inner: out })
    }

    /// Point.__setitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setitem")]
    pub fn setitem(&mut self, i: u32, value: f64) -> napi::Result<()> {
        let op = "setitem";
        if 2 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        self.inner[i as usize] = value;
        Ok(())
    }

    /// Point.__sub__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "sub")]
    pub fn sub(&self, other: PointLike) -> Point {
        Point {
            inner: self.inner - other.to_compat(),
        }
    }

    /// Point.__truediv__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "truediv")]
    pub fn div(&self, matrix: MatrixLike) -> Point {
        Point {
            inner: self.inner / matrix.to_compat(),
        }
    }

    /// Point.abs_unit from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "absUnit")]
    pub fn abs_unit(&self) -> napi::Result<Point> {
        let out = self.inner.abs_unit();
        Ok(Point { inner: out })
    }

    /// Point.distance_to from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "distanceTo")]
    pub fn distance_to(&self, other: PointLike) -> napi::Result<f64> {
        let out = self.inner.distance_to(other.to_compat());
        Ok(out)
    }

    /// Point.norm from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "norm")]
    pub fn norm(&self) -> napi::Result<f64> {
        let out = self.inner.norm();
        Ok(out)
    }

    /// Point.transform from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "transform")]
    pub fn transform(&mut self, matrix: MatrixLike) -> napi::Result<Point> {
        let out = self.inner.transform(matrix.to_compat());
        Ok(Point { inner: out })
    }

    /// Point.unit from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "unit")]
    pub fn unit(&self) -> napi::Result<Point> {
        let out = self.inner.unit();
        Ok(Point { inner: out })
    }
}

#[napi]
impl Quad {
    /// Quad.__abs__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "abs")]
    pub fn abs(&self) -> napi::Result<f64> {
        let out = self.inner.abs();
        Ok(out)
    }

    /// Quad.__add__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "add")]
    pub fn add(&self, other: QuadLike) -> Quad {
        Quad {
            inner: self.inner + other.to_compat(),
        }
    }

    /// Quad.__bool__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bool")]
    pub fn to_bool(&self) -> napi::Result<bool> {
        let out = self.inner.to_bool();
        Ok(out)
    }

    /// Quad.__contains__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "contains")]
    pub fn contains_point(&self, point: PointLike) -> napi::Result<bool> {
        let out = self.inner.contains_point(point.to_compat());
        Ok(out)
    }

    /// Quad.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: QuadLike) -> bool {
        self.inner == other.to_compat()
    }

    /// Quad.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: QuadLike) -> napi::Result<bool> {
        let op = "ge";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Quad.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn getitem(&self, i: u32) -> napi::Result<Point> {
        let op = "getitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        Ok(Point {
            inner: self.inner[i as usize],
        })
    }

    /// Quad.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: QuadLike) -> napi::Result<bool> {
        let op = "gt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Quad.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(
        ul: Option<PointLike>,
        ur: Option<PointLike>,
        ll: Option<PointLike>,
        lr: Option<PointLike>,
    ) -> Self {
        Self {
            inner: fc::Quad::new(
                ul.as_ref()
                    .map(PointLike::to_compat)
                    .unwrap_or(fc::Point { x: 0.0, y: 0.0 }),
                ur.as_ref()
                    .map(PointLike::to_compat)
                    .unwrap_or(fc::Point { x: 0.0, y: 0.0 }),
                ll.as_ref()
                    .map(PointLike::to_compat)
                    .unwrap_or(fc::Point { x: 0.0, y: 0.0 }),
                lr.as_ref()
                    .map(PointLike::to_compat)
                    .unwrap_or(fc::Point { x: 0.0, y: 0.0 }),
            ),
        }
    }

    /// Quad.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: QuadLike) -> napi::Result<bool> {
        let op = "le";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Quad.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// Quad.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: QuadLike) -> napi::Result<bool> {
        let op = "lt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Quad.__mul__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "mul")]
    pub fn mul(&self, matrix: MatrixLike) -> Quad {
        Quad {
            inner: self.inner * matrix.to_compat(),
        }
    }

    /// Quad.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: QuadLike) -> bool {
        self.inner != other.to_compat()
    }

    /// Quad.__neg__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "neg")]
    pub fn neg(&self) -> Quad {
        Quad { inner: -self.inner }
    }

    /// Quad.__pos__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pos")]
    pub fn pos(&self) -> napi::Result<Quad> {
        let out = self.inner.pos();
        Ok(Quad { inner: out })
    }

    /// Quad.__setitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setitem")]
    pub fn setitem(&mut self, i: u32, value: PointLike) -> napi::Result<()> {
        let op = "setitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        self.inner[i as usize] = value.to_compat();
        Ok(())
    }

    /// Quad.__sub__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "sub")]
    pub fn sub(&self, other: QuadLike) -> Quad {
        Quad {
            inner: self.inner - other.to_compat(),
        }
    }

    /// Quad.__truediv__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "truediv")]
    pub fn div(&self, matrix: MatrixLike) -> Quad {
        Quad {
            inner: self.inner / matrix.to_compat(),
        }
    }

    /// Quad.height from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "height")]
    pub fn height(&self) -> napi::Result<f64> {
        let out = self.inner.height();
        Ok(out)
    }

    /// Quad.is_convex from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isConvex")]
    pub fn is_convex(&self) -> napi::Result<bool> {
        let out = self.inner.is_convex();
        Ok(out)
    }

    /// Quad.is_empty from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isEmpty")]
    pub fn is_empty(&self) -> napi::Result<bool> {
        let out = self.inner.is_empty();
        Ok(out)
    }

    /// Quad.is_infinite from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isInfinite")]
    pub fn is_infinite(&self) -> napi::Result<bool> {
        let out = self.inner.is_infinite();
        Ok(out)
    }

    /// Quad.is_rectangular from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isRectangular")]
    pub fn is_rectangular(&self) -> napi::Result<bool> {
        let out = self.inner.is_rectangular();
        Ok(out)
    }

    /// Quad.morph from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "morph")]
    pub fn morph(&self, point: PointLike, matrix: MatrixLike) -> napi::Result<Quad> {
        let out = self.inner.morph(point.to_compat(), matrix.to_compat());
        Ok(Quad { inner: out })
    }

    /// Quad.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let out = self.inner.rect();
        Ok(Rect { inner: out })
    }

    /// Quad.transform from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "transform")]
    pub fn transform(&mut self, matrix: MatrixLike) -> napi::Result<Quad> {
        let out = self.inner.transform(matrix.to_compat());
        Ok(Quad { inner: out })
    }

    /// Quad.width from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "width")]
    pub fn width(&self) -> napi::Result<f64> {
        let out = self.inner.width();
        Ok(out)
    }
}

#[napi]
impl Rect {
    /// Rect.__abs__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "abs")]
    pub fn abs(&self) -> napi::Result<f64> {
        let out = self.inner.abs();
        Ok(out)
    }

    /// Rect.__add__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "add")]
    pub fn add(&self, other: RectLike) -> Rect {
        Rect {
            inner: self.inner + other.to_compat(),
        }
    }

    /// Rect.__bool__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bool")]
    pub fn to_bool(&self) -> napi::Result<bool> {
        let out = self.inner.to_bool();
        Ok(out)
    }

    /// Rect.__contains__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "containsOp")]
    pub fn contains_point(&self, point: PointLike) -> napi::Result<i32> {
        let out = self.inner.contains_point(point.to_compat());
        Ok(out)
    }

    /// Rect.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: RectLike) -> bool {
        self.inner == other.to_compat()
    }

    /// Rect.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: RectLike) -> napi::Result<bool> {
        let op = "ge";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Rect.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn getitem(&self, i: u32) -> napi::Result<f64> {
        let op = "getitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        Ok(self.inner[i as usize])
    }

    /// Rect.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: RectLike) -> napi::Result<bool> {
        let op = "gt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Rect.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(x0: Option<f64>, y0: Option<f64>, x1: Option<f64>, y1: Option<f64>) -> Self {
        Self {
            inner: fc::Rect::new(
                x0.unwrap_or(0.0),
                y0.unwrap_or(0.0),
                x1.unwrap_or(0.0),
                y1.unwrap_or(0.0),
            ),
        }
    }

    /// Rect.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: RectLike) -> napi::Result<bool> {
        let op = "le";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Rect.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// Rect.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: RectLike) -> napi::Result<bool> {
        let op = "lt";
        let o = other.to_compat();
        self.inner
            .compare(&o, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Rect.__mul__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "mul")]
    pub fn mul(&self, matrix: MatrixLike) -> Rect {
        Rect {
            inner: self.inner * matrix.to_compat(),
        }
    }

    /// Rect.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: RectLike) -> bool {
        self.inner != other.to_compat()
    }

    /// Rect.__neg__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "neg")]
    pub fn neg(&self) -> Rect {
        Rect { inner: -self.inner }
    }

    /// Rect.__pos__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pos")]
    pub fn pos(&self) -> napi::Result<Rect> {
        let out = self.inner.pos();
        Ok(Rect { inner: out })
    }

    /// Rect.__setitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setitem")]
    pub fn setitem(&mut self, i: u32, value: f64) -> napi::Result<()> {
        let op = "setitem";
        if 4 <= i as usize {
            return Err(compat_error(
                op,
                fc::ErrorKind::IndexOutOfRange,
                format!("{op}: index {i} out of range"),
            ));
        }
        self.inner[i as usize] = value;
        Ok(())
    }

    /// Rect.__sub__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "sub")]
    pub fn sub(&self, other: RectLike) -> Rect {
        Rect {
            inner: self.inner - other.to_compat(),
        }
    }

    /// Rect.__truediv__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "truediv")]
    pub fn div(&self, matrix: MatrixLike) -> Rect {
        Rect {
            inner: self.inner / matrix.to_compat(),
        }
    }

    /// Rect.bl from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bl")]
    pub fn bl(&self) -> napi::Result<Point> {
        let out = self.inner.bl();
        Ok(Point { inner: out })
    }

    /// Rect.bottom_left from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bottomLeft")]
    pub fn bottom_left(&self) -> napi::Result<Point> {
        let out = self.inner.bottom_left();
        Ok(Point { inner: out })
    }

    /// Rect.bottom_right from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bottomRight")]
    pub fn bottom_right(&self) -> napi::Result<Point> {
        let out = self.inner.bottom_right();
        Ok(Point { inner: out })
    }

    /// Rect.br from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "br")]
    pub fn br(&self) -> napi::Result<Point> {
        let out = self.inner.br();
        Ok(Point { inner: out })
    }

    /// Rect.contains from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "contains")]
    pub fn contains_rect(&self, other: RectLike) -> napi::Result<bool> {
        let out = self.inner.contains_rect(other.to_compat());
        Ok(out)
    }

    /// Rect.get_area from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getArea")]
    pub fn get_area(&self) -> napi::Result<f64> {
        let out = self.inner.get_area();
        Ok(out)
    }

    /// Rect.height from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "height")]
    pub fn height(&self) -> napi::Result<f64> {
        let out = self.inner.height();
        Ok(out)
    }

    /// Rect.include_point from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "includePoint")]
    pub fn include_point(&mut self, point: PointLike) -> napi::Result<Rect> {
        let out = self.inner.include_point(point.to_compat());
        Ok(Rect { inner: out })
    }

    /// Rect.include_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "includeRect")]
    pub fn include_rect(&mut self, other: RectLike) -> napi::Result<Rect> {
        let out = self.inner.include_rect(other.to_compat());
        Ok(Rect { inner: out })
    }

    /// Rect.intersect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "intersect")]
    pub fn intersect(&mut self, other: RectLike) -> napi::Result<Rect> {
        let out = self.inner.intersect(other.to_compat());
        Ok(Rect { inner: out })
    }

    /// Rect.intersects from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "intersects")]
    pub fn intersects(&self, other: RectLike) -> napi::Result<bool> {
        let out = self.inner.intersects(other.to_compat());
        Ok(out)
    }

    /// Rect.irect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "irect")]
    pub fn irect(&self) -> napi::Result<IRect> {
        let out = self.inner.irect();
        Ok(IRect { inner: out })
    }

    /// Rect.is_empty from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isEmpty")]
    pub fn is_empty(&self) -> napi::Result<bool> {
        let out = self.inner.is_empty();
        Ok(out)
    }

    /// Rect.is_infinite from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isInfinite")]
    pub fn is_infinite(&self) -> napi::Result<bool> {
        let out = self.inner.is_infinite();
        Ok(out)
    }

    /// Rect.is_valid from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isValid")]
    pub fn is_valid(&self) -> napi::Result<bool> {
        let out = self.inner.is_valid();
        Ok(out)
    }

    /// Rect.morph from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "morph")]
    pub fn morph(&self, point: PointLike, matrix: MatrixLike) -> napi::Result<Quad> {
        let out = self.inner.morph(point.to_compat(), matrix.to_compat());
        Ok(Quad { inner: out })
    }

    /// Rect.norm from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "norm")]
    pub fn norm(&self) -> napi::Result<f64> {
        let out = self.inner.norm();
        Ok(out)
    }

    /// Rect.normalize from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "normalize")]
    pub fn normalize(&mut self) -> napi::Result<Rect> {
        let out = self.inner.normalize();
        Ok(Rect { inner: out })
    }

    /// Rect.quad from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "quad")]
    pub fn quad(&self) -> napi::Result<Quad> {
        let out = self.inner.quad();
        Ok(Quad { inner: out })
    }

    /// Rect.round from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "round")]
    pub fn round(&self) -> napi::Result<IRect> {
        let out = self.inner.round();
        Ok(IRect { inner: out })
    }

    /// Rect.tl from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "tl")]
    pub fn tl(&self) -> napi::Result<Point> {
        let out = self.inner.tl();
        Ok(Point { inner: out })
    }

    /// Rect.top_left from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "topLeft")]
    pub fn top_left(&self) -> napi::Result<Point> {
        let out = self.inner.top_left();
        Ok(Point { inner: out })
    }

    /// Rect.top_right from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "topRight")]
    pub fn top_right(&self) -> napi::Result<Point> {
        let out = self.inner.top_right();
        Ok(Point { inner: out })
    }

    /// Rect.torect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "torect")]
    pub fn torect(&self, target: RectLike) -> napi::Result<Matrix> {
        let out = self.inner.torect(target.to_compat());
        Ok(Matrix { inner: out })
    }

    /// Rect.tr from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "tr")]
    pub fn tr(&self) -> napi::Result<Point> {
        let out = self.inner.tr();
        Ok(Point { inner: out })
    }

    /// Rect.transform from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "transform")]
    pub fn transform(&mut self, matrix: MatrixLike) -> napi::Result<Rect> {
        let out = self.inner.transform(matrix.to_compat());
        Ok(Rect { inner: out })
    }

    /// Rect.width from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "width")]
    pub fn width(&self) -> napi::Result<f64> {
        let out = self.inner.width();
        Ok(out)
    }
}
