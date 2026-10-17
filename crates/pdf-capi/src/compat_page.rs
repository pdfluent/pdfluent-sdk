//! Compatible pages: text, images, and drawing.
//!
//! Generated from the member table; one `pdf_compat_*` wrapper per bound spec
//! member. See `compat.rs` for the conventions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::compat::{
    colorspace_from_code, fail, fail_arg, fail_page_range, guard, optional_bytes, optional_doubles,
    optional_ints, optional_matrix, optional_point, optional_rect, optional_str, out_handle,
    out_string, required_doc, required_doc_mut, required_matrix, required_point, required_quad,
    required_rect, required_str, PdfCompatDocument, PdfCompatLink, PdfCompatMatrix,
    PdfCompatPixmap, PdfCompatPoint, PdfCompatQuad, PdfCompatRect, PdfCompatSearchHits,
    PdfCompatTableFinder, PdfCompatTextOutput, PdfCompatWidget, PdfStatus,
};
use super::compat_handles::copy_ints;
use super::compat_lists::{
    PdfCompatAnnotEntryList, PdfCompatDrawingList, PdfCompatFontInfoList, PdfCompatImageRefList,
    PdfCompatLinkList, PdfCompatPlacedImageList, PdfCompatRectList, PdfCompatTextTraceList,
    PdfCompatWidgetList,
};
use super::compat_opts::{
    draw_options_from_json, insert_text_options_from_json, optional_json,
    text_box_options_from_json, value_from_json,
};
use pdfluent::compat::{Page, PageMut};
use std::ffi::CStr;
use std::os::raw::c_char;

/// Highlight text; returns the annotation xref.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_add_highlight_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    quads: *const PdfCompatQuad,
    quads_len: usize,
    start: *const PdfCompatPoint,
    stop: *const PdfCompatPoint,
    clip: *const PdfCompatRect,
    out: *mut i32,
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
    let quads = if quads.is_null() {
        None
    } else {
        let slice = unsafe { std::slice::from_raw_parts(quads, quads_len) };
        Some(
            slice
                .iter()
                .map(|q| pdfluent::compat::Quad::from(*q))
                .collect::<Vec<_>>(),
        )
    };
    let start = unsafe { optional_point(start) };
    let stop = unsafe { optional_point(stop) };
    let clip = unsafe { optional_rect(clip) };
    let mut hl_opts = pdfluent::compat::HighlightOptions::default();
    hl_opts.quads = quads;
    hl_opts.start = start;
    hl_opts.stop = stop;
    hl_opts.clip = clip;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.add_highlight_annot(&hl_opts).and_then(|a| a.xref())
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Add a rectangle annotation; returns its xref.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_add_rect_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    out: *mut i32,
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
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.add_rect_annot(rect).and_then(|a| a.xref())
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Mark a quadrilateral for redaction; returns its xref.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_add_redact_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    quad: *const PdfCompatQuad,
    text: *const c_char,
    fontname: *const c_char,
    fontsize: f64,
    align: i32,
    fill: *const f64,
    fill_len: usize,
    text_color: *const f64,
    text_color_len: usize,
    cross_out: i32,
    out: *mut i32,
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
    let quad = match required_quad(quad) {
        Ok(q) => q,
        Err(s) => return s,
    };
    let text = match optional_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let fontname = match optional_str(fontname) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let fill = match optional_doubles(fill, fill_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let text_color = match optional_doubles(text_color, text_color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let cross_out = cross_out != 0;
    let mut redact_opts = pdfluent::compat::RedactAnnotOptions::default();
    redact_opts.text = text.map(str::to_owned);
    redact_opts.fontname = fontname.map(str::to_owned);
    redact_opts.fontsize = fontsize;
    redact_opts.align = align;
    redact_opts.fill = fill;
    redact_opts.text_color = text_color;
    redact_opts.cross_out = cross_out;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.add_redact_annot(quad, &redact_opts)
            .and_then(|a| a.xref())
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Add a text annotation; returns its xref.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_add_text_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    at: *const PdfCompatPoint,
    text: *const c_char,
    icon: *const c_char,
    out: *mut i32,
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
    let at = match required_point(at) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let text = match required_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let icon = match required_str(icon) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.add_text_annot(at, text, icon).and_then(|a| a.xref())
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Add a form widget; returns its xref.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_add_widget(
    doc: *mut PdfCompatDocument,
    page: i32,
    widget: *const PdfCompatWidget,
    out: *mut i32,
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
    if widget.is_null() {
        return fail_arg("null pointer argument");
    }
    let widget = unsafe { &(*widget).0 };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.add_widget(widget).and_then(|a| a.xref())
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Annotations on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_annots(
    doc: *mut PdfCompatDocument,
    page: i32,
    types: *const i32,
    types_len: usize,
    out: *mut *mut PdfCompatAnnotEntryList,
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
    let types = match optional_ints(types, types_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    match guard(true, || {
        let pg = PageMut::new(doc, page as usize);
        pg.annots(types.as_deref())
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatAnnotEntryList(v)) },
        Err(status) => status,
    }
}

/// Burn pending redactions in.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_apply_redactions(
    doc: *mut PdfCompatDocument,
    page: i32,
    images: i32,
    graphics: i32,
    text: i32,
    out: *mut i32,
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
    let mut apply_opts = pdfluent::compat::RedactApplyOptions::default();
    apply_opts.images = images;
    apply_opts.graphics = graphics;
    apply_opts.text = text;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.apply_redactions(&apply_opts)
    }) {
        Ok(v) => {
            unsafe { *out = i32::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Tidy the content stream.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_clean_contents(
    doc: *mut PdfCompatDocument,
    page: i32,
    sanitize: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let sanitize = sanitize != 0;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.clean_contents(sanitize)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Delete an annotation.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_delete_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    xref: i32,
    type_code: i32,
    type_name: *const c_char,
    rect: *const PdfCompatRect,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let type_name = match required_str(type_name) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let rect = match required_rect(rect) {
        Ok(r) => r,
        Err(s) => return s,
    };
    let entry = pdfluent::compat::AnnotEntry {
        xref,
        annot_type: pdfluent::compat::AnnotType {
            code: type_code,
            name: type_name.to_owned(),
        },
        rect,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.delete_annot(&entry)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Stroke a Bezier curve.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_draw_bezier(
    doc: *mut PdfCompatDocument,
    page: i32,
    p1: *const PdfCompatPoint,
    p2: *const PdfCompatPoint,
    p3: *const PdfCompatPoint,
    p4: *const PdfCompatPoint,
    options: *const c_char,
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
    let p3 = match required_point(p3) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let p4 = match required_point(p4) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let draw_opts = match draw_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.draw_bezier(p1, p2, p3, p4, &draw_opts)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Stroke a circle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_draw_circle(
    doc: *mut PdfCompatDocument,
    page: i32,
    center: *const PdfCompatPoint,
    radius: f64,
    options: *const c_char,
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
    let center = match required_point(center) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let draw_opts = match draw_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.draw_circle(center, radius, &draw_opts)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Stroke a line.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_draw_line(
    doc: *mut PdfCompatDocument,
    page: i32,
    p1: *const PdfCompatPoint,
    p2: *const PdfCompatPoint,
    options: *const c_char,
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
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let draw_opts = match draw_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.draw_line(p1, p2, &draw_opts)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Stroke a rectangle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_draw_rect(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    options: *const c_char,
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
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let radius = if radius.is_nan() { None } else { Some(radius) };
    let draw_opts = match draw_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.draw_rect(rect, &draw_opts, radius)
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatPoint::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Detect tables on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_find_tables(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut *mut PdfCompatTableFinder,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let table_opts = pdfluent::compat::TableOptions::default();
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.find_tables(&table_opts)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatTableFinder(v)) },
        Err(status) => status,
    }
}

/// First annotation, if any.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_first_annot(
    doc: *mut PdfCompatDocument,
    page: i32,
    out: *mut *mut PdfCompatAnnotEntryList,
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
    match guard(true, || {
        let pg = PageMut::new(doc, page as usize);
        pg.first_annot().map(|o| o.into_iter().collect::<Vec<_>>())
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatAnnotEntryList(v)) },
        Err(status) => status,
    }
}

/// Content-stream object numbers.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_contents(
    doc: *const PdfCompatDocument,
    page: i32,
    out_data: *mut *mut i32,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_contents()
    }) {
        Ok(v) => unsafe { copy_ints(out_data, out_len, &v) },
        Err(status) => status,
    }
}

/// Vector drawings on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_drawings(
    doc: *mut PdfCompatDocument,
    page: i32,
    extended: i32,
    out: *mut *mut PdfCompatDrawingList,
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
    let extended = extended != 0;
    match guard(true, || {
        let pg = PageMut::new(doc, page as usize);
        pg.get_drawings(extended)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatDrawingList(v)) },
        Err(status) => status,
    }
}

/// Fonts used on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_fonts(
    doc: *const PdfCompatDocument,
    page: i32,
    full: i32,
    out: *mut *mut PdfCompatFontInfoList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let full = full != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_fonts(full)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatFontInfoList(v)) },
        Err(status) => status,
    }
}

/// Placed images with geometry.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_image_info(
    doc: *const PdfCompatDocument,
    page: i32,
    hashes: i32,
    xrefs: i32,
    out: *mut *mut PdfCompatPlacedImageList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let hashes = hashes != 0;
    let xrefs = xrefs != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_image_info(hashes, xrefs)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatPlacedImageList(v)) },
        Err(status) => status,
    }
}

/// Boxes where a named image appears.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_image_rects(
    doc: *const PdfCompatDocument,
    page: i32,
    name: *const c_char,
    transform: i32,
    out: *mut *mut PdfCompatRectList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let name = match required_str(name) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let transform = transform != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_image_rects(name, transform)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatRectList(v)) },
        Err(status) => status,
    }
}

/// Images on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_images(
    doc: *const PdfCompatDocument,
    page: i32,
    full: i32,
    out: *mut *mut PdfCompatImageRefList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let full = full != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_images(full)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatImageRefList(v)) },
        Err(status) => status,
    }
}

/// Links on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_links(
    doc: *mut PdfCompatDocument,
    page: i32,
    out: *mut *mut PdfCompatLinkList,
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
    match guard(true, || {
        let pg = PageMut::new(doc, page as usize);
        pg.get_links()
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatLinkList(v)) },
        Err(status) => status,
    }
}

/// Render the page to a pixel buffer.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_pixmap(
    doc: *const PdfCompatDocument,
    page: i32,
    matrix: *const PdfCompatMatrix,
    dpi: f64,
    colorspace: i32,
    clip: *const PdfCompatRect,
    alpha: i32,
    annots: i32,
    out: *mut *mut PdfCompatPixmap,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let matrix = match required_matrix(matrix) {
        Ok(m) => m,
        Err(s) => return s,
    };
    let dpi = if dpi.is_nan() { None } else { Some(dpi) };
    let clip = unsafe { optional_rect(clip) };
    let alpha = alpha != 0;
    let annots = annots != 0;
    let mut render_opts = pdfluent::compat::RenderOptions::default();
    render_opts.matrix = matrix;
    render_opts.dpi = dpi;
    render_opts.colorspace = if colorspace < 0 {
        None
    } else {
        match colorspace_from_code(colorspace) {
            Some(sp) => Some(sp),
            None => return fail_arg("unknown colorspace code"),
        }
    };
    render_opts.clip = clip;
    render_opts.alpha = alpha;
    render_opts.annots = annots;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_pixmap(&render_opts)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatPixmap(v)) },
        Err(status) => status,
    }
}

/// Render the page to SVG text.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_svg_image(
    doc: *const PdfCompatDocument,
    page: i32,
    matrix: *const PdfCompatMatrix,
    text_as_path: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let matrix = unsafe { optional_matrix(matrix) };
    let text_as_path = text_as_path != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_svg_image(matrix, text_as_path)
    }) {
        Ok(v) => unsafe { out_string(out, v) },
        Err(status) => status,
    }
}

/// Extract text in the named format.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_text(
    doc: *const PdfCompatDocument,
    page: i32,
    format: *const c_char,
    clip: *const PdfCompatRect,
    flags: i32,
    sort: i32,
    use_textpage: i32,
    out: *mut *mut PdfCompatTextOutput,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let format = match required_str(format) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let clip = unsafe { optional_rect(clip) };
    let flags = if flags < 0 { None } else { Some(flags) };
    let sort = sort != 0;
    let use_textpage = use_textpage != 0;
    let mut text_opts = pdfluent::compat::TextOptions::default();
    text_opts.format = match pdfluent::compat::TextFormat::from_name(format) {
        Ok(f) => f,
        Err(e) => return fail(&e, true),
    };
    text_opts.clip = clip;
    text_opts.flags = flags;
    text_opts.sort = sort;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        {
            if use_textpage {
                text_opts.textpage = Some(pdfluent::compat::TextPage::new(pg));
            }
            pg.get_text(&text_opts)
        }
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatTextOutput(v)) },
        Err(status) => status,
    }
}

/// Text inside a rectangle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_textbox(
    doc: *const PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    use_textpage: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
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
    let use_textpage = use_textpage != 0;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        {
            let tp = if use_textpage {
                Some(pdfluent::compat::TextPage::new(pg))
            } else {
                None
            };
            pg.get_textbox(rect, tp)
        }
    }) {
        Ok(v) => unsafe { out_string(out, v) },
        Err(status) => status,
    }
}

/// Typeset spans with glyph origins.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_get_texttrace(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut *mut PdfCompatTextTraceList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.get_texttrace()
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatTextTraceList(v)) },
        Err(status) => status,
    }
}

/// Register a font for the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_insert_font(
    doc: *mut PdfCompatDocument,
    page: i32,
    fontname: *const c_char,
    fontfile: *const c_char,
    fontbuffer: *const u8,
    fontbuffer_len: usize,
    set_simple: i32,
    wmode: i32,
    encoding: i32,
    out: *mut i32,
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
    let fontname = match required_str(fontname) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let fontfile = match optional_str(fontfile) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let fontbuffer = match optional_bytes(fontbuffer, fontbuffer_len) {
        Ok(Some(b)) => Some(b.to_vec()),
        Ok(None) => None,
        Err(s) => return s,
    };
    let set_simple = set_simple != 0;
    let mut font_opts = pdfluent::compat::InsertFontOptions::default();
    font_opts.fontname = fontname.to_owned();
    font_opts.fontfile = fontfile.map(str::to_owned);
    font_opts.fontbuffer = fontbuffer;
    font_opts.set_simple = set_simple;
    font_opts.wmode = wmode;
    font_opts.encoding = encoding;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.insert_font(&font_opts)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Place an image on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_insert_image(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    alpha: i32,
    filename: *const c_char,
    height: f64,
    keep_proportion: i32,
    mask: *const c_char,
    oc: i32,
    overlay: i32,
    pixmap: *const PdfCompatPixmap,
    rotate: i32,
    stream: *const u8,
    stream_len: usize,
    width: f64,
    xref: i32,
    out: *mut i32,
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
    let filename = match optional_str(filename) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let keep_proportion = keep_proportion != 0;
    let mask = match optional_json(mask) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let overlay = overlay != 0;
    let pixmap_cloned: Option<pdfluent::compat::Pixmap> = if pixmap.is_null() {
        None
    } else {
        Some(unsafe { (*pixmap).0.clone() })
    };
    let stream = match optional_bytes(stream, stream_len) {
        Ok(Some(b)) => Some(b.to_vec()),
        Ok(None) => None,
        Err(s) => return s,
    };
    let mut image_opts = pdfluent::compat::ImageInsertOptions::default();
    image_opts.alpha = alpha;
    image_opts.filename = filename.map(str::to_owned);
    image_opts.height = height;
    image_opts.keep_proportion = keep_proportion;
    if let Some(m) = mask {
        match value_from_json(&m) {
            Ok(v) => image_opts.mask = Some(v),
            Err(d) => return fail_arg(&format!("invalid mask argument: {d}")),
        }
    }
    image_opts.oc = oc;
    image_opts.overlay = overlay;
    image_opts.pixmap = pixmap_cloned;
    image_opts.rotate = rotate;
    image_opts.stream = stream;
    image_opts.width = width;
    image_opts.xref = xref;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.insert_image(rect, &image_opts)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Add a link to the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_insert_link(
    doc: *mut PdfCompatDocument,
    page: i32,
    link: *const PdfCompatLink,
    mark: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    if link.is_null() {
        return fail_arg("null pointer argument");
    }
    let link = unsafe { &(*link).0 };
    let mark = mark != 0;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.insert_link(link, mark)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Insert text at a point.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_insert_text(
    doc: *mut PdfCompatDocument,
    page: i32,
    at: *const PdfCompatPoint,
    text: *const c_char,
    lines: *const *const c_char,
    lines_len: usize,
    options: *const c_char,
    out: *mut i32,
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
    let at = match required_point(at) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let text_arg = {
        let mut lines_vec: Vec<String> = Vec::new();
        if !lines.is_null() && lines_len > 0 {
            let slice = unsafe { std::slice::from_raw_parts(lines, lines_len) };
            for ptr in slice {
                if ptr.is_null() {
                    return fail_arg("null line argument");
                }
                match unsafe { CStr::from_ptr(*ptr) }.to_str() {
                    Ok(s) => lines_vec.push(s.to_owned()),
                    Err(_) => return fail_arg("invalid UTF-8 in line argument"),
                }
            }
        }
        if !lines_vec.is_empty() {
            pdfluent::compat::TextArg::Many(lines_vec)
        } else {
            let one = match optional_str(text) {
                Ok(t) => t.unwrap_or("").to_owned(),
                Err(s) => return s,
            };
            pdfluent::compat::TextArg::One(one)
        }
    };
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let text_opts = match insert_text_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.insert_text(at, &text_arg, &text_opts)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Flow text into a rectangle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_insert_textbox(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    text: *const c_char,
    lines: *const *const c_char,
    lines_len: usize,
    options: *const c_char,
    out: *mut f64,
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
    let text_arg = {
        let mut lines_vec: Vec<String> = Vec::new();
        if !lines.is_null() && lines_len > 0 {
            let slice = unsafe { std::slice::from_raw_parts(lines, lines_len) };
            for ptr in slice {
                if ptr.is_null() {
                    return fail_arg("null line argument");
                }
                match unsafe { CStr::from_ptr(*ptr) }.to_str() {
                    Ok(s) => lines_vec.push(s.to_owned()),
                    Err(_) => return fail_arg("invalid UTF-8 in line argument"),
                }
            }
        }
        if !lines_vec.is_empty() {
            pdfluent::compat::TextArg::Many(lines_vec)
        } else {
            let one = match optional_str(text) {
                Ok(t) => t.unwrap_or("").to_owned(),
                Err(s) => return s,
            };
            pdfluent::compat::TextArg::One(one)
        }
    };
    let options = match optional_json(options) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let box_opts = match text_box_options_from_json(options.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.insert_textbox(rect, &text_arg, &box_opts)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Media box in points.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_mediabox(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.mediabox()
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatRect::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Open a drawing canvas on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_new_shape(
    doc: *mut PdfCompatDocument,
    page: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.new_shape().map(|_| ())
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Page box in points.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_rect(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.rect()
    }) {
        Ok(v) => {
            unsafe { *out = PdfCompatRect::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Page rotation in degrees.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_rotation(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.rotation()
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Search for text; rectangles or quadrilaterals.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_search_for(
    doc: *const PdfCompatDocument,
    page: i32,
    needle: *const c_char,
    clip: *const PdfCompatRect,
    quads: i32,
    flags: i32,
    limit: i32,
    use_textpage: i32,
    out: *mut *mut PdfCompatSearchHits,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    let needle = match required_str(needle) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let clip = unsafe { optional_rect(clip) };
    let quads = quads != 0;
    let flags = if flags < 0 { None } else { Some(flags) };
    let use_textpage = use_textpage != 0;
    let mut search_opts = pdfluent::compat::SearchOptions::default();
    search_opts.clip = clip;
    search_opts.quads = quads;
    search_opts.flags = flags;
    search_opts.limit = limit;
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        {
            if use_textpage {
                search_opts.textpage = Some(pdfluent::compat::TextPage::new(pg));
            }
            pg.search_for(needle, &search_opts)
        }
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatSearchHits(v)) },
        Err(status) => status,
    }
}

/// Set the crop box.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_set_cropbox(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
) -> PdfStatus {
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
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.set_cropbox(rect)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Set the rotation.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_set_rotation(
    doc: *mut PdfCompatDocument,
    page: i32,
    rotation: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.set_rotation(rotation)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Stamp a source page onto this one.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_show_pdf_page(
    doc: *mut PdfCompatDocument,
    page: i32,
    rect: *const PdfCompatRect,
    source: *const PdfCompatDocument,
    page_no: i32,
    keep_proportion: i32,
    overlay: i32,
    oc: i32,
    rotate: i32,
    clip: *const PdfCompatRect,
    out: *mut i32,
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
    let source = match required_doc(source) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let keep_proportion = keep_proportion != 0;
    let overlay = overlay != 0;
    let clip = unsafe { optional_rect(clip) };
    let mut show_opts = pdfluent::compat::ShowPageOptions::default();
    show_opts.page_no = page_no;
    show_opts.keep_proportion = keep_proportion;
    show_opts.overlay = overlay;
    show_opts.oc = oc;
    show_opts.rotate = rotate;
    show_opts.clip = clip;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        pg.show_pdf_page(rect, source, &show_opts)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Form widgets on the page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_widgets(
    doc: *mut PdfCompatDocument,
    page: i32,
    types: *const i32,
    types_len: usize,
    out: *mut *mut PdfCompatWidgetList,
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
    let types = match optional_ints(types, types_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    match guard(true, || {
        let pg = PageMut::new(doc, page as usize);
        pg.widgets(types.as_deref())
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatWidgetList(v)) },
        Err(status) => status,
    }
}

/// Page object number.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_page_xref(
    doc: *const PdfCompatDocument,
    page: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if page < 0 {
        return fail_page_range();
    }
    match guard(true, || {
        let pg = Page::new(doc, page as usize);
        pg.xref()
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}
