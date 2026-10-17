//! Compatible owned values: pixmaps, fonts, writers, colours, and module functions.
//!
//! Generated from the member table; one `pdf_compat_*` wrapper per bound spec
//! member. See `compat.rs` for the conventions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::compat::{
    colorspace_from_code, colorspace_to_code, fail_arg, fail_page_range, guard, optional_bytes,
    optional_doubles, optional_irect, optional_matrix, optional_str, out_bytes, out_handle,
    required_bytes, required_doc_mut, required_irect, required_point, required_rect, required_str,
    PdfCompatDocument, PdfCompatFont, PdfCompatIRect, PdfCompatMatrix, PdfCompatPixmap,
    PdfCompatPoint, PdfCompatRect, PdfCompatTextWriter, PdfCompatWidget, PdfStatus,
};
use pdfluent::compat::PageMut;
use std::os::raw::c_char;

/// Channel count, excluding alpha.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_colorspace_n(colorspace: i32, out: *mut i32) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let space = match colorspace_from_code(colorspace) {
        Some(sp) => sp,
        None => return fail_arg("unknown colorspace code"),
    };
    match guard(false, || Ok(space.n())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Build a font from a name, file, or buffer.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_new(
    fontname: *const c_char,
    fontfile: *const c_char,
    fontbuffer: *const u8,
    fontbuffer_len: usize,
    script: i32,
    language: *const c_char,
    ordering: i32,
    is_bold: i32,
    is_italic: i32,
    is_serif: i32,
    embed: i32,
    out: *mut *mut PdfCompatFont,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let fontname = match optional_str(fontname) {
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
    let language = match optional_str(language) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let is_bold = is_bold != 0;
    let is_italic = is_italic != 0;
    let is_serif = is_serif != 0;
    let embed = embed != 0;
    let mut font_opts = pdfluent::compat::FontOptions::default();
    font_opts.fontname = fontname.map(str::to_owned);
    font_opts.fontfile = fontfile.map(str::to_owned);
    font_opts.fontbuffer = fontbuffer;
    font_opts.script = script;
    font_opts.language = language.map(str::to_owned);
    font_opts.ordering = ordering;
    font_opts.is_bold = is_bold;
    font_opts.is_italic = is_italic;
    font_opts.is_serif = is_serif;
    font_opts.embed = embed;
    match guard(false, || pdfluent::compat::Font::new(&font_opts)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatFont(v)) },
        Err(status) => status,
    }
}

/// Typeset width of a string.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_text_length(
    font: *const PdfCompatFont,
    text: *const c_char,
    fontsize: f64,
    language: *const c_char,
    script: i32,
    wmode: i32,
    small_caps: i32,
    fallback: i32,
    out: *mut f64,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if font.is_null() {
        return fail_arg("null pointer argument");
    }
    let f = unsafe { &(*font).0 };
    let text = match required_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let language = match optional_str(language) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let small_caps = small_caps != 0;
    let mut measure_opts = pdfluent::compat::FontMeasureOptions::default();
    measure_opts.fontsize = fontsize;
    measure_opts.language = language.map(str::to_owned);
    measure_opts.script = script;
    measure_opts.wmode = wmode;
    measure_opts.small_caps = small_caps;
    measure_opts.fallback = fallback;
    match guard(false, || f.text_length(text, &measure_opts)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Allocate a pixel buffer.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_new(
    colorspace: i32,
    bounds: *const PdfCompatIRect,
    alpha: i32,
    out: *mut *mut PdfCompatPixmap,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let bounds = match required_irect(bounds) {
        Ok(r) => r,
        Err(s) => return s,
    };
    let alpha = alpha != 0;
    let space = match colorspace_from_code(colorspace) {
        Some(sp) => sp,
        None => return fail_arg("unknown colorspace code"),
    };
    match guard(false, || {
        pdfluent::compat::Pixmap::new(space, bounds, alpha)
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatPixmap(v)) },
        Err(status) => status,
    }
}

/// Whether an alpha channel is present.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_alpha(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.alpha())) {
        Ok(v) => {
            unsafe { *out = i32::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Clear to a value, optionally clipped.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_clear_with(
    pixmap: *mut PdfCompatPixmap,
    value: i32,
    bbox: *const PdfCompatIRect,
) -> PdfStatus {
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &mut (*pixmap).0 };
    let value = if value < 0 { None } else { Some(value as u8) };
    let bbox = unsafe { optional_irect(bbox) };
    match guard(false, || px.clear_with(value, bbox)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Colour-space code: 1 grey, 3 RGB, 4 CMYK.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_colorspace(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || colorspace_to_code(px.colorspace())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Height in pixels.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_h(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.h())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Height in pixels.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_height(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.height())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Pixel bounds.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_irect(
    pixmap: *const PdfCompatPixmap,
    out: *mut PdfCompatIRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.irect())) {
        Ok(v) => {
            unsafe { *out = PdfCompatIRect::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Channel count, excluding alpha.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_n(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.n())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// A copy of the raw samples.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_samples(
    pixmap: *const PdfCompatPixmap,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.samples().to_vec())) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}

/// Write the pixmap to a file.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_save(
    pixmap: *const PdfCompatPixmap,
    path: *const c_char,
    output: *const c_char,
    jpg_quality: i32,
) -> PdfStatus {
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    let path = match required_str(path) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let output = match optional_str(output) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || px.save(path, output, jpg_quality)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Fill a box with a colour.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_set_rect(
    pixmap: *mut PdfCompatPixmap,
    bbox: *const PdfCompatIRect,
    color: *const u8,
    color_len: usize,
) -> PdfStatus {
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &mut (*pixmap).0 };
    let bbox = match required_irect(bbox) {
        Ok(r) => r,
        Err(s) => return s,
    };
    let color = match required_bytes(color, color_len) {
        Ok(b) => b.to_vec(),
        Err(s) => return s,
    };
    match guard(false, || px.set_rect(bbox, &color)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Row stride in bytes.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_stride(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.stride() as i32)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Encode the pixmap.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_tobytes(
    pixmap: *const PdfCompatPixmap,
    output: *const c_char,
    jpg_quality: i32,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    let output = match required_str(output) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || px.tobytes(output, jpg_quality)) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}

/// Width in pixels.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_w(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.w())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Width in pixels.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_width(
    pixmap: *const PdfCompatPixmap,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if pixmap.is_null() {
        return fail_arg("null pointer argument");
    }
    let px = unsafe { &(*pixmap).0 };
    match guard(false, || Ok(px.width())) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Stage text for a page rectangle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_writer_new(
    rect: *const PdfCompatRect,
    opacity: f64,
    color: *const f64,
    color_len: usize,
    out: *mut *mut PdfCompatTextWriter,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let rect = match required_rect(rect) {
        Ok(r) => r,
        Err(s) => return s,
    };
    let color = match optional_doubles(color, color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let mut writer_opts = pdfluent::compat::TextWriterOptions::default();
    writer_opts.opacity = opacity;
    writer_opts.color = color;
    match guard(false, || {
        Ok(pdfluent::compat::TextWriter::new(rect, &writer_opts))
    }) {
        Ok(v) => unsafe { out_handle(out, PdfCompatTextWriter(v)) },
        Err(status) => status,
    }
}

/// Append text; returns the used box and end point.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_writer_append(
    writer: *mut PdfCompatTextWriter,
    pos: *const PdfCompatPoint,
    text: *const c_char,
    font: *const PdfCompatFont,
    fontsize: f64,
    language: *const c_char,
    right_to_left: i32,
    small_caps: i32,
    out: *mut PdfCompatRect,
    out2: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if out2.is_null() {
        return fail_arg("null pointer argument");
    }
    if writer.is_null() {
        return fail_arg("null pointer argument");
    }
    let w = unsafe { &mut (*writer).0 };
    let pos = match required_point(pos) {
        Ok(p) => p,
        Err(s) => return s,
    };
    let text = match required_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let font_ref: Option<&pdfluent::compat::Font> = if font.is_null() {
        None
    } else {
        Some(unsafe { &(*font).0 })
    };
    let language = match optional_str(language) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let right_to_left = right_to_left != 0;
    let small_caps = small_caps != 0;
    let mut append_opts = pdfluent::compat::AppendOptions::default();
    append_opts.font = font_ref;
    append_opts.fontsize = fontsize;
    append_opts.language = language.map(str::to_owned);
    append_opts.right_to_left = right_to_left;
    append_opts.small_caps = small_caps;
    match guard(false, || w.append(pos, text, &append_opts)) {
        Ok(v) => {
            unsafe { *out = PdfCompatRect::from(v.0) };
            unsafe { *out2 = PdfCompatPoint::from(v.1) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Flush staged text onto a page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_writer_write_text(
    writer: *const PdfCompatTextWriter,
    doc: *mut PdfCompatDocument,
    page: i32,
    color: *const f64,
    color_len: usize,
    opacity: f64,
    overlay: i32,
    morph: *const f64,
    morph_len: usize,
    matrix: *const PdfCompatMatrix,
    render_mode: i32,
    oc: i32,
) -> PdfStatus {
    if writer.is_null() {
        return fail_arg("null pointer argument");
    }
    let w = unsafe { &(*writer).0 };
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let color = match optional_doubles(color, color_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let opacity = if opacity.is_nan() {
        None
    } else {
        Some(opacity)
    };
    let overlay = overlay != 0;
    let morph = match optional_doubles(morph, morph_len) {
        Ok(s) => s.map(|s| s.to_vec()),
        Err(s) => return s,
    };
    let matrix = unsafe { optional_matrix(matrix) };
    if page < 0 {
        return fail_page_range();
    }
    let mut flush_opts = pdfluent::compat::FlushOptions::default();
    flush_opts.color = color;
    flush_opts.opacity = opacity;
    flush_opts.overlay = overlay;
    flush_opts.morph = morph;
    flush_opts.matrix = matrix;
    flush_opts.render_mode = render_mode;
    flush_opts.oc = oc;
    match guard(true, || {
        let mut pg = PageMut::new(doc, page as usize);
        w.write_text(&mut pg, &flush_opts)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Build a blank widget.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_new(out: *mut *mut PdfCompatWidget) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || Ok(pdfluent::compat::Widget::new())) {
        Ok(v) => unsafe { out_handle(out, PdfCompatWidget(v)) },
        Err(status) => status,
    }
}

/// Typeset width of a string in a base font.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_get_text_length(
    text: *const c_char,
    fontname: *const c_char,
    fontsize: f64,
    encoding: i32,
    out: *mut f64,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let text = match required_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let fontname = match required_str(fontname) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || {
        Ok(pdfluent::compat::get_text_length(
            text, fontname, fontsize, encoding,
        ))
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Named paper size in points.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_paper_size(
    name: *const c_char,
    out: *mut f64,
    out2: *mut f64,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if out2.is_null() {
        return fail_arg("null pointer argument");
    }
    let name = match required_str(name) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || pdfluent::compat::paper_size(name)) {
        Ok(v) => {
            unsafe { *out = v.0 };
            unsafe { *out2 = v.1 };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}
