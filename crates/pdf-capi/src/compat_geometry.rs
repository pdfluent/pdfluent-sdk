//! Compatible geometry values (lane P0): points, rectangles, matrices.
//!
//! Thin wrappers over the facade geometry types. Every function returns a
//! status code like the rest of the compatible surface, so a stub that still
//! panics surfaces as `ErrorUnknown` instead of aborting the caller.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::compat::{
    fail_arg, guard, PdfCompatIRect, PdfCompatMatrix, PdfCompatPoint, PdfCompatRect, PdfStatus,
};

/// Build a point from its coordinates.
///
/// # Safety
/// `out` must be a writable point.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_point_new(
    x: f64,
    y: f64,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || Ok(pdfluent::compat::Point::new(x, y))) {
        Ok(point) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = point.into() };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Build a rectangle from its four edges.
///
/// # Safety
/// `out` must be a writable rect.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_new(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || Ok(pdfluent::compat::Rect::new(x0, y0, x1, y1))) {
        Ok(rect) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = rect.into() };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Build an integer rectangle from its four edges.
///
/// # Safety
/// `out` must be a writable irect.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_irect_new(
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    out: *mut PdfCompatIRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || Ok(pdfluent::compat::IRect::new(x0, y0, x1, y1))) {
        Ok(rect) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = rect.into() };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Build a matrix from its six coefficients.
///
/// # Safety
/// `out` must be a writable matrix.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_matrix_new(
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
    out: *mut PdfCompatMatrix,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || {
        Ok(pdfluent::compat::Matrix::new(a, b, c, d, e, f))
    }) {
        Ok(matrix) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = matrix.into() };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether a rectangle contains another: 1 yes, 0 no.
///
/// # Safety
/// `rect`, `other`, and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_contains(
    rect: *const PdfCompatRect,
    other: *const PdfCompatRect,
    out: *mut i32,
) -> PdfStatus {
    if rect.is_null() || other.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees readable rects and a writable out-param.
    let (rect, other) = unsafe { (*rect, *other) };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.contains_rect(other.into()))
    }) {
        Ok(hit) => {
            unsafe { *out = i32::from(hit) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Rectangle area, in square points.
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_get_area(
    rect: *const PdfCompatRect,
    out: *mut f64,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let rect = unsafe { *rect };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.get_area())
    }) {
        Ok(area) => {
            unsafe { *out = area };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Rectangle height, in points (`y1 - y0`).
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_get_height(
    rect: *const PdfCompatRect,
    out: *mut f64,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let rect = unsafe { *rect };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.height())
    }) {
        Ok(height) => {
            unsafe { *out = height };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Rectangle width, in points (`x1 - x0`).
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_get_width(
    rect: *const PdfCompatRect,
    out: *mut f64,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let rect = unsafe { *rect };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.width())
    }) {
        Ok(width) => {
            unsafe { *out = width };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Grow a rectangle in place to include another; writes the new value back.
///
/// # Safety
/// `rect`, `other`, and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_include_rect(
    rect: *mut PdfCompatRect,
    other: *const PdfCompatRect,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if rect.is_null() || other.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees readable rects and a writable out-param.
    let (current, other) = unsafe { (*rect, *other) };
    match guard(false, || {
        let mut rect: pdfluent::compat::Rect = current.into();
        Ok(rect.include_rect(other.into()))
    }) {
        Ok(grown) => {
            let grown = PdfCompatRect::from(grown);
            unsafe {
                *rect = grown;
                *out = grown;
            }
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Intersect a rectangle in place with another; writes the new value back.
///
/// # Safety
/// `rect`, `other`, and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_intersect(
    rect: *mut PdfCompatRect,
    other: *const PdfCompatRect,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if rect.is_null() || other.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees readable rects and a writable out-param.
    let (current, other) = unsafe { (*rect, *other) };
    match guard(false, || {
        let mut rect: pdfluent::compat::Rect = current.into();
        Ok(rect.intersect(other.into()))
    }) {
        Ok(hit) => {
            let hit = PdfCompatRect::from(hit);
            unsafe {
                *rect = hit;
                *out = hit;
            }
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether two rectangles overlap: 1 yes, 0 no.
///
/// # Safety
/// `rect`, `other`, and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_intersects(
    rect: *const PdfCompatRect,
    other: *const PdfCompatRect,
    out: *mut i32,
) -> PdfStatus {
    if rect.is_null() || other.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees readable rects and a writable out-param.
    let (rect, other) = unsafe { (*rect, *other) };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.intersects(other.into()))
    }) {
        Ok(hit) => {
            unsafe { *out = i32::from(hit) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether a rectangle is empty: 1 yes, 0 no.
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_is_empty(
    rect: *const PdfCompatRect,
    out: *mut i32,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let rect = unsafe { *rect };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.is_empty())
    }) {
        Ok(empty) => {
            unsafe { *out = i32::from(empty) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether a rectangle is infinite: 1 yes, 0 no.
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_is_infinite(
    rect: *const PdfCompatRect,
    out: *mut i32,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let rect = unsafe { *rect };
    match guard(false, || {
        let rect: pdfluent::compat::Rect = rect.into();
        Ok(rect.is_infinite())
    }) {
        Ok(infinite) => {
            unsafe { *out = i32::from(infinite) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Normalize a rectangle in place; writes the new value back.
///
/// # Safety
/// `rect` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_normalize(
    rect: *mut PdfCompatRect,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if rect.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect and a writable out-param.
    let current = unsafe { *rect };
    match guard(false, || {
        let mut rect: pdfluent::compat::Rect = current.into();
        Ok(rect.normalize())
    }) {
        Ok(normal) => {
            let normal = PdfCompatRect::from(normal);
            unsafe {
                *rect = normal;
                *out = normal;
            }
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Rotate a matrix in place by a quarter-turn count; writes the new value.
///
/// # Safety
/// `matrix` and `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_matrix_prerotate(
    matrix: *mut PdfCompatMatrix,
    theta: f64,
    out: *mut PdfCompatMatrix,
) -> PdfStatus {
    if matrix.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable matrix and a writable out-param.
    let current = unsafe { *matrix };
    match guard(false, || {
        let mut matrix: pdfluent::compat::Matrix = current.into();
        Ok(matrix.prerotate(theta))
    }) {
        Ok(rotated) => {
            let rotated = PdfCompatMatrix::from(rotated);
            unsafe {
                *matrix = rotated;
                *out = rotated;
            }
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}
