//! Compatible annotations and shapes, addressed by document, page, and xref.
//!
//! Generated from the member table; one `pdf_compat_*` wrapper per bound spec
//! member. See `compat.rs` for the conventions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::compat::{
    fail_arg, fail_page_range, guard, optional_doubles, optional_str, required_doc_mut,
    required_point, required_rect, PdfCompatDocument, PdfCompatPoint, PdfCompatRect, PdfStatus,
};
use super::compat_opts::{optional_json, shape_finish_options_from_json};
use pdfluent::compat::{Annot, PageMut, Shape};
use std::os::raw::c_char;

/// Set stroke and fill colours.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_set_colors(
    doc: *mut PdfCompatDocument,
    page: i32,
    xref: i32,
    stroke: *const f64,
    stroke_len: usize,
    fill: *const f64,
    fill_len: usize,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let stroke = match optional_doubles(stroke, stroke_len) {
        Ok(Some(s)) => s.to_vec(),
        Ok(None) => return fail_arg("null pointer argument"),
        Err(s) => return s,
    };
    let fill = match optional_doubles(fill, fill_len) {
        Ok(Some(s)) => s.to_vec(),
        Ok(None) => return fail_arg("null pointer argument"),
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        let mut an = Annot::new(&mut pg, xref);
        an.set_colors(&pdfluent::compat::AnnotColors { stroke, fill })
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Push appearance changes.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_update(
    doc: *mut PdfCompatDocument,
    page: i32,
    xref: i32,
    blend_mode: *const c_char,
    opacity: f64,
    fontsize: f64,
    fontname: *const c_char,
    text_color: *const f64,
    text_color_len: usize,
    border_color: *const f64,
    border_color_len: usize,
    fill_color: *const f64,
    fill_color_len: usize,
    cross_out: i32,
    rotate: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let blend_mode = match optional_str(blend_mode) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let opacity = if opacity.is_nan() {
        None
    } else {
        Some(opacity)
    };
    let fontname = match optional_str(fontname) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let text_color = match optional_doubles(text_color, text_color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let border_color = match optional_doubles(border_color, border_color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let fill_color = match optional_doubles(fill_color, fill_color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let cross_out = cross_out != 0;
    let mut update_opts = pdfluent::compat::AnnotUpdateOptions::default();
    update_opts.blend_mode = blend_mode.map(str::to_owned);
    update_opts.opacity = opacity;
    update_opts.fontsize = fontsize;
    update_opts.fontname = fontname.map(str::to_owned);
    update_opts.text_color = text_color;
    update_opts.border_color = border_color;
    update_opts.fill_color = fill_color;
    update_opts.cross_out = cross_out;
    update_opts.rotate = rotate;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        let mut an = Annot::new(&mut pg, xref);
        an.update(&update_opts)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Commit pending marks.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_shape_commit(
    doc: *mut PdfCompatDocument,
    page: i32,
    overlay: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let overlay = overlay != 0;
    match guard(true, || {
        let mut sh = Shape::new(doc, page as usize);
        sh.commit(overlay)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Queue a line.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_shape_draw_line(
    doc: *mut PdfCompatDocument,
    page: i32,
    p1: *const PdfCompatPoint,
    p2: *const PdfCompatPoint,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let p1 = match required_point(p1) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let p2 = match required_point(p2) {
        Ok(p) => p,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut sh = Shape::new(doc, page as usize);
        sh.draw_line(p1, p2)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Queue a rectangle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_shape_draw_rect(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    radius: f64,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let rect = match required_rect(rect) {
        Ok(r) => r,
        Err(s) => return s,
    };
    let radius = if radius.is_nan() { None } else { Some(radius) };
    match guard(true, || {
        let mut sh = Shape::new(doc, page as usize);
        sh.draw_rect(rect, radius)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Stroke and fill the queued path.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_shape_finish(
    doc: *mut PdfCompatDocument,
    page: i32,
    options: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let finish_opts = match shape_finish_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut sh = Shape::new(doc, page as usize);
        sh.finish(&finish_opts)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}
