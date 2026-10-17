//! Owned compatible handles with field access: widgets, links, text results.
//!
//! These handles need per-field getters, setters, or tree navigation that the
//! regular snapshot lists do not, so they are written by hand instead of
//! generated: `Widget` and `Link` (owned values the bindings both read and
//! build), `TextOutput` (the option-dependent `get_text` result),
//! `SearchHits`, `TableFinder`, and the `set_toc` outline builder.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::os::raw::c_char;

use super::compat::{
    fail, fail_arg, fail_unknown_variant, optional_doubles, optional_ints, optional_str,
    out_handle, out_string, required_str, PdfCompatLink, PdfCompatPoint, PdfCompatQuad,
    PdfCompatRect, PdfCompatSearchHits, PdfCompatTableFinder, PdfCompatTextOutput, PdfCompatWidget,
    PdfStatus,
};
use super::compat_opts::value_to_json;

// ---- Outline builder -------------------------------------------------------

/// A `set_toc` outline under construction.
pub struct PdfCompatTocBuilder(pub(crate) Vec<pdfluent::compat::TocEntry>);

/// Start an empty outline builder.
///
/// # Safety
/// `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_builder_new(
    out: *mut *mut PdfCompatTocBuilder,
) -> PdfStatus {
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_handle(out, PdfCompatTocBuilder(Vec::new())) }
}

/// Push one outline entry: level, title, 1-based page, and destination.
///
/// A null `title` becomes the empty string. When `has_dest` is 0 the
/// remaining destination arguments are ignored.
///
/// # Safety
/// `builder` must be live; strings must be valid or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_builder_push(
    builder: *mut PdfCompatTocBuilder,
    level: i32,
    title: *const c_char,
    page: i32,
    has_dest: i32,
    dest_kind: i32,
    dest_page: i32,
    dest_x: f64,
    dest_y: f64,
    dest_zoom: f64,
) -> PdfStatus {
    if builder.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live builder and valid strings.
    let title = match unsafe { optional_str(title) } {
        Ok(title) => title.unwrap_or("").to_owned(),
        Err(status) => return status,
    };
    let dest = if has_dest == 0 {
        None
    } else {
        Some(pdfluent::compat::TocDest {
            kind: dest_kind,
            page: dest_page,
            target: pdfluent::compat::Point {
                x: dest_x,
                y: dest_y,
            },
            zoom: dest_zoom,
        })
    };
    unsafe {
        (*builder).0.push(pdfluent::compat::TocEntry {
            level,
            title,
            page,
            dest,
        });
    }
    PdfStatus::Ok
}

/// Free an outline builder. Null is safe.
///
/// # Safety
/// `builder` must have been returned by `pdf_compat_toc_builder_new`.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_builder_free(builder: *mut PdfCompatTocBuilder) {
    if !builder.is_null() {
        // SAFETY: the caller guarantees ownership of the builder.
        unsafe { drop(Box::from_raw(builder)) };
    }
}

// ---- Widget fields --------------------------------------------------------

macro_rules! widget_ref {
    ($widget:expr) => {{
        if $widget.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live widget handle.
        unsafe { &(*$widget).0 }
    }};
}

macro_rules! widget_mut {
    ($widget:expr) => {{
        if $widget.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live widget handle.
        unsafe { &mut (*$widget).0 }
    }};
}

/// Widget field name.
///
/// # Safety
/// `widget` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_field_name(
    widget: *const PdfCompatWidget,
    out: *mut *mut c_char,
) -> PdfStatus {
    let widget = widget_ref!(widget);
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, widget.field_name.clone()) }
}

/// Set the widget field name.
///
/// # Safety
/// `widget` must be live; `value` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_set_field_name(
    widget: *mut PdfCompatWidget,
    value: *const c_char,
) -> PdfStatus {
    let widget = widget_mut!(widget);
    // SAFETY: the caller guarantees a valid string.
    let value = match unsafe { required_str(value) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    widget.field_name = value.to_owned();
    PdfStatus::Ok
}

/// Widget field-type code.
///
/// # Safety
/// `widget` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_field_type(
    widget: *const PdfCompatWidget,
    out: *mut i32,
) -> PdfStatus {
    let widget = widget_ref!(widget);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = widget.field_type };
    PdfStatus::Ok
}

/// Set the widget field-type code.
///
/// # Safety
/// `widget` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_set_field_type(
    widget: *mut PdfCompatWidget,
    value: i32,
) -> PdfStatus {
    let widget = widget_mut!(widget);
    widget.field_type = value;
    PdfStatus::Ok
}

/// Widget field value text.
///
/// # Safety
/// `widget` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_field_value(
    widget: *const PdfCompatWidget,
    out: *mut *mut c_char,
) -> PdfStatus {
    let widget = widget_ref!(widget);
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, widget.field_value.clone()) }
}

/// Set the widget field value text.
///
/// # Safety
/// `widget` must be live; `value` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_set_field_value(
    widget: *mut PdfCompatWidget,
    value: *const c_char,
) -> PdfStatus {
    let widget = widget_mut!(widget);
    // SAFETY: the caller guarantees a valid string.
    let value = match unsafe { required_str(value) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    widget.field_value = value.to_owned();
    PdfStatus::Ok
}

/// Widget box in points.
///
/// # Safety
/// `widget` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_rect(
    widget: *const PdfCompatWidget,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let widget = widget_ref!(widget);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(widget.rect) };
    PdfStatus::Ok
}

/// Set the widget box.
///
/// # Safety
/// `widget` must be live; `value` must be readable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_set_rect(
    widget: *mut PdfCompatWidget,
    value: *const PdfCompatRect,
) -> PdfStatus {
    let widget = widget_mut!(widget);
    if value.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect.
    widget.rect = unsafe { *value }.into();
    PdfStatus::Ok
}

// ---- Link ------------------------------------------------------------------

macro_rules! link_ref {
    ($link:expr) => {{
        if $link.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live link handle.
        unsafe { &(*$link).0 }
    }};
}

macro_rules! link_mut {
    ($link:expr) => {{
        if $link.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live link handle.
        unsafe { &mut (*$link).0 }
    }};
}

/// Build a blank link.
///
/// # Safety
/// `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_new(out: *mut *mut PdfCompatLink) -> PdfStatus {
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_handle(out, PdfCompatLink(pdfluent::compat::Link::default())) }
}

/// Link box in points.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_rect(
    link: *const PdfCompatLink,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(link.rect) };
    PdfStatus::Ok
}

/// Set the link box.
///
/// # Safety
/// `link` must be live; `value` must be readable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_rect(
    link: *mut PdfCompatLink,
    value: *const PdfCompatRect,
) -> PdfStatus {
    let link = link_mut!(link);
    if value.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a readable rect.
    link.rect = unsafe { *value }.into();
    PdfStatus::Ok
}

/// Link kind code.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_kind(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.kind };
    PdfStatus::Ok
}

/// Set the link kind code.
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_kind(
    link: *mut PdfCompatLink,
    value: i32,
) -> PdfStatus {
    let link = link_mut!(link);
    link.kind = value;
    PdfStatus::Ok
}

/// Link target URI.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_uri(
    link: *const PdfCompatLink,
    out: *mut *mut c_char,
) -> PdfStatus {
    let link = link_ref!(link);
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, link.uri.clone()) }
}

/// Set the link target URI.
///
/// # Safety
/// `link` must be live; `value` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_uri(
    link: *mut PdfCompatLink,
    value: *const c_char,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees a valid string.
    let value = match unsafe { required_str(value) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    link.uri = value.to_owned();
    PdfStatus::Ok
}

/// Link object number.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_xref(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.xref };
    PdfStatus::Ok
}

/// Set the link object number.
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_xref(
    link: *mut PdfCompatLink,
    value: i32,
) -> PdfStatus {
    let link = link_mut!(link);
    link.xref = value;
    PdfStatus::Ok
}

/// Link identifier.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_id(
    link: *const PdfCompatLink,
    out: *mut *mut c_char,
) -> PdfStatus {
    let link = link_ref!(link);
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, link.id.clone()) }
}

/// Set the link identifier.
///
/// # Safety
/// `link` must be live; `value` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_id(
    link: *mut PdfCompatLink,
    value: *const c_char,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees a valid string.
    let value = match unsafe { required_str(value) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    link.id = value.to_owned();
    PdfStatus::Ok
}

/// Link border width in points.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_border_width(
    link: *const PdfCompatLink,
    out: *mut f64,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.border.width };
    PdfStatus::Ok
}

/// Set the link border width.
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_border_width(
    link: *mut PdfCompatLink,
    value: f64,
) -> PdfStatus {
    let link = link_mut!(link);
    link.border.width = value;
    PdfStatus::Ok
}

/// Link border style, if any: `has` says whether one exists.
///
/// # Safety
/// `link` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_border_style(
    link: *const PdfCompatLink,
    out_has: *mut i32,
    out_style: *mut *mut c_char,
) -> PdfStatus {
    let link = link_ref!(link);
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    let style = link.border.style.clone().unwrap_or_default();
    // SAFETY: the caller guarantees writable out-params.
    unsafe {
        *out_has = i32::from(link.border.style.is_some());
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out_style, style) }
}

/// Set the link border style; null clears it.
///
/// # Safety
/// `link` must be live; `value` must be valid or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_border_style(
    link: *mut PdfCompatLink,
    value: *const c_char,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees a valid string or null.
    let value = match unsafe { optional_str(value) } {
        Ok(value) => value,
        Err(status) => return status,
    };
    link.border.style = value.map(str::to_owned);
    PdfStatus::Ok
}

/// Link border dashes: element count plus heap ints, freed with `pdf_bytes_free`.
///
/// # Safety
/// `link` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_border_dashes(
    link: *const PdfCompatLink,
    out_data: *mut *mut i32,
    out_len: *mut usize,
) -> PdfStatus {
    let link = link_ref!(link);
    copy_ints(out_data, out_len, &link.border.dashes)
}

/// Set the link border dashes.
///
/// # Safety
/// `link` must be live; `values` must hold `len` readable ints.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_border_dashes(
    link: *mut PdfCompatLink,
    values: *const i32,
    len: usize,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees `len` readable ints.
    let values = match unsafe { optional_ints(values, len) } {
        Ok(values) => values,
        Err(status) => return status,
    };
    link.border.dashes = values.unwrap_or(&[]).to_vec();
    PdfStatus::Ok
}

/// Link border cloud effect level.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_border_clouds(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.border.clouds };
    PdfStatus::Ok
}

/// Set the link border cloud effect level.
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_border_clouds(
    link: *mut PdfCompatLink,
    value: i32,
) -> PdfStatus {
    let link = link_mut!(link);
    link.border.clouds = value;
    PdfStatus::Ok
}

/// Link stroke colour: element count plus heap doubles, freed with `pdf_bytes_free`.
///
/// # Safety
/// `link` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_stroke(
    link: *const PdfCompatLink,
    out_data: *mut *mut f64,
    out_len: *mut usize,
) -> PdfStatus {
    let link = link_ref!(link);
    copy_doubles(out_data, out_len, &link.colors.stroke)
}

/// Set the link stroke colour.
///
/// # Safety
/// `link` must be live; `values` must hold `len` readable doubles.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_stroke(
    link: *mut PdfCompatLink,
    values: *const f64,
    len: usize,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees `len` readable doubles.
    let values = match unsafe { optional_doubles(values, len) } {
        Ok(values) => values,
        Err(status) => return status,
    };
    link.colors.stroke = values.unwrap_or(&[]).to_vec();
    PdfStatus::Ok
}

/// Link fill colour: element count plus heap doubles, freed with `pdf_bytes_free`.
///
/// # Safety
/// `link` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_fill(
    link: *const PdfCompatLink,
    out_data: *mut *mut f64,
    out_len: *mut usize,
) -> PdfStatus {
    let link = link_ref!(link);
    copy_doubles(out_data, out_len, &link.colors.fill)
}

/// Set the link fill colour.
///
/// # Safety
/// `link` must be live; `values` must hold `len` readable doubles.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_fill(
    link: *mut PdfCompatLink,
    values: *const f64,
    len: usize,
) -> PdfStatus {
    let link = link_mut!(link);
    // SAFETY: the caller guarantees `len` readable doubles.
    let values = match unsafe { optional_doubles(values, len) } {
        Ok(values) => values,
        Err(status) => return status,
    };
    link.colors.fill = values.unwrap_or(&[]).to_vec();
    PdfStatus::Ok
}

/// Link flags.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_flags(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.flags };
    PdfStatus::Ok
}

/// Set the link flags.
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_flags(
    link: *mut PdfCompatLink,
    value: i32,
) -> PdfStatus {
    let link = link_mut!(link);
    link.flags = value;
    PdfStatus::Ok
}

/// Whether the link carries a follow-on destination: 1 yes, 0 no.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_has_dest(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = i32::from(link.dest.is_some()) };
    PdfStatus::Ok
}

/// Follow-on destination page, or -1 when there is none.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_dest_page(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.dest.as_ref().map(|d| d.page).unwrap_or(-1) };
    PdfStatus::Ok
}

/// Follow-on destination point, or the origin when there is none.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_dest_target(
    link: *const PdfCompatLink,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let target = link
        .dest
        .as_ref()
        .map(|d| d.target)
        .unwrap_or(pdfluent::compat::Point { x: 0.0, y: 0.0 });
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatPoint::from(target) };
    PdfStatus::Ok
}

/// Follow-on destination zoom, or 0 when there is none.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_dest_zoom(
    link: *const PdfCompatLink,
    out: *mut f64,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = link.dest.as_ref().map(|d| d.zoom).unwrap_or(0.0) };
    PdfStatus::Ok
}

/// Clear the follow-on destination.
///
/// A matching setter does not exist: `LinkDest` is non-exhaustive without a
/// default, so no caller outside the facade can build one (reported as a
/// signature change request).
///
/// # Safety
/// `link` must be live.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_clear_dest(link: *mut PdfCompatLink) -> PdfStatus {
    let link = link_mut!(link);
    link.dest = None;
    PdfStatus::Ok
}

/// Whether the link chains to another: 1 yes, 0 no.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_has_next(
    link: *const PdfCompatLink,
    out: *mut i32,
) -> PdfStatus {
    let link = link_ref!(link);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = i32::from(link.next.is_some()) };
    PdfStatus::Ok
}

/// Clone the chained link into a fresh handle, freed with `pdf_compat_link_free`.
///
/// # Safety
/// `link` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_next(
    link: *const PdfCompatLink,
    out: *mut *mut PdfCompatLink,
) -> PdfStatus {
    let link = link_ref!(link);
    match link.next.as_deref() {
        Some(next) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { out_handle(out, PdfCompatLink((*next).clone())) }
        }
        None => fail_arg("link has no chained link"),
    }
}

/// Chain another link; null clears the chain.
///
/// # Safety
/// `link` must be live; `next` must be live or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_set_next(
    link: *mut PdfCompatLink,
    next: *const PdfCompatLink,
) -> PdfStatus {
    let link = link_mut!(link);
    if next.is_null() {
        link.next = None;
        return PdfStatus::Ok;
    }
    // SAFETY: the caller guarantees a live link handle.
    let next = unsafe { &(*next).0 };
    link.next = Some(Box::new(next.clone()));
    PdfStatus::Ok
}

// ---- Array copies ----------------------------------------------------------

/// Copy ints to a heap array; freed with `pdf_bytes_free` and `len * 4`.
///
/// # Safety
/// Out-params must be writable.
pub(crate) unsafe fn copy_ints(
    out_data: *mut *mut i32,
    out_len: *mut usize,
    values: &[i32],
) -> PdfStatus {
    if out_data.is_null() || out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    let mut owned = values.to_vec().into_boxed_slice();
    let ptr = owned.as_mut_ptr();
    let len = owned.len();
    std::mem::forget(owned);
    // SAFETY: the caller guarantees writable out-params.
    unsafe {
        *out_data = ptr;
        *out_len = len;
    }
    PdfStatus::Ok
}

/// Copy doubles to a heap array; freed with `pdf_bytes_free` and `len * 8`.
///
/// # Safety
/// Out-params must be writable.
pub(crate) unsafe fn copy_doubles(
    out_data: *mut *mut f64,
    out_len: *mut usize,
    values: &[f64],
) -> PdfStatus {
    if out_data.is_null() || out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    let mut owned = values.to_vec().into_boxed_slice();
    let ptr = owned.as_mut_ptr();
    let len = owned.len();
    std::mem::forget(owned);
    // SAFETY: the caller guarantees writable out-params.
    unsafe {
        *out_data = ptr;
        *out_len = len;
    }
    PdfStatus::Ok
}

// ---- Text output -----------------------------------------------------------
// Navigation over the `get_text` result. Every getter fails with
// invalid-argument when the result holds another format, another block tag,
// or the index is out of range.

macro_rules! text_ref {
    ($handle:expr) => {{
        if $handle.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live text-output handle.
        unsafe { &(*$handle).0 }
    }};
}

fn text_words(
    output: &pdfluent::compat::TextOutput,
) -> Result<Vec<pdfluent::compat::TextWord>, PdfStatus> {
    match output {
        pdfluent::compat::TextOutput::Words(words) => Ok(words.clone()),
        _ => Err(fail_arg("text output is not a word list")),
    }
}

fn text_blocks(
    output: &pdfluent::compat::TextOutput,
) -> Result<Vec<pdfluent::compat::TextBlock>, PdfStatus> {
    match output {
        pdfluent::compat::TextOutput::Blocks(blocks) => Ok(blocks.clone()),
        _ => Err(fail_arg("text output is not a block list")),
    }
}

/// Result format tag: 0 text, 1 words, 2 blocks, 3 dict, 4 json, 5 raw json,
/// 6 html, 7 xhtml, 8 xml.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_format(
    handle: *const PdfCompatTextOutput,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let tag = match output {
        pdfluent::compat::TextOutput::Text(_) => 0,
        pdfluent::compat::TextOutput::Words(_) => 1,
        pdfluent::compat::TextOutput::Blocks(_) => 2,
        pdfluent::compat::TextOutput::Dict(_) => 3,
        pdfluent::compat::TextOutput::Json(_) => 4,
        pdfluent::compat::TextOutput::RawJson(_) => 5,
        pdfluent::compat::TextOutput::Html(_) => 6,
        pdfluent::compat::TextOutput::Xhtml(_) => 7,
        pdfluent::compat::TextOutput::Xml(_) => 8,
        _ => return fail_unknown_variant("text output"),
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = tag };
    PdfStatus::Ok
}

/// Result text for the string formats (text, json, raw json, html, xhtml, xml).
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_text(
    handle: *const PdfCompatTextOutput,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let text = match output {
        pdfluent::compat::TextOutput::Text(text)
        | pdfluent::compat::TextOutput::Json(text)
        | pdfluent::compat::TextOutput::RawJson(text)
        | pdfluent::compat::TextOutput::Html(text)
        | pdfluent::compat::TextOutput::Xhtml(text)
        | pdfluent::compat::TextOutput::Xml(text) => text.clone(),
        _ => return fail_arg("text output holds no string for this format"),
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, text) }
}

/// Word count for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_count(
    handle: *const PdfCompatTextOutput,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let words = match text_words(output) {
        Ok(words) => words,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = words.len() as i32 };
    PdfStatus::Ok
}

/// Word box for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_rect(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let words = match text_words(output) {
        Ok(words) => words,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match words.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(word) => unsafe { *out = PdfCompatRect::from(word.rect) },
        None => return fail_arg("word index out of range"),
    }
    PdfStatus::Ok
}

/// Word text for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_text(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let words = match text_words(output) {
        Ok(words) => words,
        Err(status) => return status,
    };
    match words.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(word) => unsafe { out_string(out, word.text.clone()) },
        None => fail_arg("word index out of range"),
    }
}

/// Word block number for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_block(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    text_output_word_field(handle, index, out, |word| word.block_no)
}

/// Word line number for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_line(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    text_output_word_field(handle, index, out, |word| word.line_no)
}

/// Word number for the words format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_word_number(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    text_output_word_field(handle, index, out, |word| word.word_no)
}

fn text_output_word_field(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
    field: impl FnOnce(&pdfluent::compat::TextWord) -> i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let words = match text_words(output) {
        Ok(words) => words,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match words.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(word) => unsafe { *out = field(word) },
        None => return fail_arg("word index out of range"),
    }
    PdfStatus::Ok
}

/// Block count for the blocks format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_block_count(
    handle: *const PdfCompatTextOutput,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let blocks = match text_blocks(output) {
        Ok(blocks) => blocks,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = blocks.len() as i32 };
    PdfStatus::Ok
}

/// Block box for the blocks format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_block_rect(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let blocks = match text_blocks(output) {
        Ok(blocks) => blocks,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match blocks.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(block) => unsafe { *out = PdfCompatRect::from(block.rect) },
        None => return fail_arg("block index out of range"),
    }
    PdfStatus::Ok
}

/// Block text for the blocks format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_block_text(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let blocks = match text_blocks(output) {
        Ok(blocks) => blocks,
        Err(status) => return status,
    };
    match blocks.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(block) => unsafe { out_string(out, block.text.clone()) },
        None => fail_arg("block index out of range"),
    }
}

/// Block number for the blocks format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_block_number(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    text_output_block_field(handle, index, out, |block| block.number)
}

/// Block type for the blocks format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_block_type(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    text_output_block_field(handle, index, out, |block| block.block_type)
}

fn text_output_block_field(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
    field: impl FnOnce(&pdfluent::compat::TextBlock) -> i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let blocks = match text_blocks(output) {
        Ok(blocks) => blocks,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match blocks.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(block) => unsafe { *out = field(block) },
        None => return fail_arg("block index out of range"),
    }
    PdfStatus::Ok
}

fn text_dict(
    output: &pdfluent::compat::TextOutput,
) -> Result<pdfluent::compat::TextDict, PdfStatus> {
    match output {
        pdfluent::compat::TextOutput::Dict(dict) => Ok(dict.clone()),
        _ => Err(fail_arg("text output is not a dict")),
    }
}

/// Page width for the dict format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_width(
    handle: *const PdfCompatTextOutput,
    out: *mut f64,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = dict.width };
    PdfStatus::Ok
}

/// Page height for the dict format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_height(
    handle: *const PdfCompatTextOutput,
    out: *mut f64,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = dict.height };
    PdfStatus::Ok
}

/// Dict block count for the dict format.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_block_count(
    handle: *const PdfCompatTextOutput,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = dict.blocks.len() as i32 };
    PdfStatus::Ok
}

/// Dict block tag: 0 text, 1 image.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_block_tag(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match dict.blocks.get(index as usize) {
        Some(pdfluent::compat::DictBlock::Text { .. }) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = 0 };
        }
        Some(pdfluent::compat::DictBlock::Image { .. }) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = 1 };
        }
        Some(_) => return fail_arg("dict block index out of range"),
        None => return fail_arg("dict block index out of range"),
    }
    PdfStatus::Ok
}

fn dict_text_lines(
    dict: &pdfluent::compat::TextDict,
    index: i32,
) -> Result<Vec<pdfluent::compat::DictLine>, PdfStatus> {
    match dict.blocks.get(index as usize) {
        Some(pdfluent::compat::DictBlock::Text { lines, .. }) => Ok(lines.clone()),
        Some(pdfluent::compat::DictBlock::Image { .. }) => {
            Err(fail_arg("dict block is not a text block"))
        }
        Some(_) => Err(fail_arg("dict block is not a text block")),
        None => Err(fail_arg("dict block index out of range")),
    }
}

fn dict_text_triple(
    dict: &pdfluent::compat::TextDict,
    index: i32,
) -> Result<(pdfluent::compat::Rect, i32, i32), PdfStatus> {
    match dict.blocks.get(index as usize) {
        Some(pdfluent::compat::DictBlock::Text {
            bbox,
            number,
            flags,
            ..
        }) => Ok((*bbox, *number, *flags)),
        Some(pdfluent::compat::DictBlock::Image { .. }) => {
            Err(fail_arg("dict block is not a text block"))
        }
        Some(_) => Err(fail_arg("dict block is not a text block")),
        None => Err(fail_arg("dict block index out of range")),
    }
}

/// Text dict block box.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_text_bbox(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let triple = match dict_text_triple(&dict, index) {
        Ok(triple) => triple,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(triple.0) };
    PdfStatus::Ok
}

/// Text dict block number.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_text_number(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let triple = match dict_text_triple(&dict, index) {
        Ok(triple) => triple,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = triple.1 };
    PdfStatus::Ok
}

/// Text dict block flags.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_text_flags(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let triple = match dict_text_triple(&dict, index) {
        Ok(triple) => triple,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = triple.2 };
    PdfStatus::Ok
}

/// Text dict block line count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_line_count(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let lines = match dict_text_lines(&dict, index) {
        Ok(lines) => lines,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = lines.len() as i32 };
    PdfStatus::Ok
}

fn dict_line(
    dict: &pdfluent::compat::TextDict,
    index: i32,
    line: i32,
) -> Result<pdfluent::compat::DictLine, PdfStatus> {
    let lines = dict_text_lines(dict, index)?;
    lines
        .get(line as usize)
        .cloned()
        .ok_or_else(|| fail_arg("dict line index out of range"))
}

/// Dict line box.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_line_bbox(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let line = match dict_line(&dict, index, line) {
        Ok(line) => line,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(line.bbox) };
    PdfStatus::Ok
}

/// Dict line writing mode.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_line_wmode(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let line = match dict_line(&dict, index, line) {
        Ok(line) => line,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = line.wmode };
    PdfStatus::Ok
}

/// Dict line direction.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_line_direction(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let line = match dict_line(&dict, index, line) {
        Ok(line) => line,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatPoint::from(line.direction) };
    PdfStatus::Ok
}

/// Dict line span count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_count(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let line = match dict_line(&dict, index, line) {
        Ok(line) => line,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = line.spans.len() as i32 };
    PdfStatus::Ok
}

fn dict_span(
    dict: &pdfluent::compat::TextDict,
    index: i32,
    line: i32,
    span: i32,
) -> Result<pdfluent::compat::DictSpan, PdfStatus> {
    let line = dict_line(dict, index, line)?;
    line.spans
        .get(span as usize)
        .cloned()
        .ok_or_else(|| fail_arg("dict span index out of range"))
}

/// Dict span box.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_bbox(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(span.bbox) };
    PdfStatus::Ok
}

/// Dict span origin.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_origin(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatPoint::from(span.origin) };
    PdfStatus::Ok
}

/// Dict span font name.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_font(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, span.font.clone()) }
}

/// Dict span size in points.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_size(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut f64,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.size };
    PdfStatus::Ok
}

/// Dict span colour.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_color(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.color };
    PdfStatus::Ok
}

/// Dict span alpha.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_alpha(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.alpha };
    PdfStatus::Ok
}

/// Dict span flags.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_flags(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.flags };
    PdfStatus::Ok
}

/// Dict span bidi level.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_bidi(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.bidi };
    PdfStatus::Ok
}

/// Dict span character flags.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_char_flags(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.char_flags };
    PdfStatus::Ok
}

/// Dict span ascender.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_ascender(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut f64,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.ascender };
    PdfStatus::Ok
}

/// Dict span descender.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_descender(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut f64,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.descender };
    PdfStatus::Ok
}

/// Dict span text.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_text(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, span.text.clone()) }
}

/// Dict span character count (raw dicts only).
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_span_char_count(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let span = match dict_span(&dict, index, line, span) {
        Ok(span) => span,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = span.chars.len() as i32 };
    PdfStatus::Ok
}

fn dict_char(
    dict: &pdfluent::compat::TextDict,
    index: i32,
    line: i32,
    span: i32,
    ch: i32,
) -> Result<pdfluent::compat::RawChar, PdfStatus> {
    let span = dict_span(dict, index, line, span)?;
    span.chars
        .get(ch as usize)
        .cloned()
        .ok_or_else(|| fail_arg("dict char index out of range"))
}

/// Dict character box.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_char_bbox(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    ch: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let ch = match dict_char(&dict, index, line, span, ch) {
        Ok(ch) => ch,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(ch.bbox) };
    PdfStatus::Ok
}

/// Dict character text (a single character).
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_char_text(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    ch: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let ch = match dict_char(&dict, index, line, span, ch) {
        Ok(ch) => ch,
        Err(status) => return status,
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { out_string(out, ch.ch.to_string()) }
}

/// Dict character origin.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_char_origin(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    ch: i32,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let ch = match dict_char(&dict, index, line, span, ch) {
        Ok(ch) => ch,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatPoint::from(ch.origin) };
    PdfStatus::Ok
}

/// Whether the dict character is synthetic: 1 yes, 0 no.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_char_synthetic(
    handle: *const PdfCompatTextOutput,
    index: i32,
    line: i32,
    span: i32,
    ch: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let ch = match dict_char(&dict, index, line, span, ch) {
        Ok(ch) => ch,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = i32::from(ch.synthetic) };
    PdfStatus::Ok
}

fn dict_image_quad(
    dict: &pdfluent::compat::TextDict,
    index: i32,
) -> Result<(pdfluent::compat::Rect, i32, i32, i32), PdfStatus> {
    match dict.blocks.get(index as usize) {
        Some(pdfluent::compat::DictBlock::Image {
            bbox,
            number,
            width,
            height,
            ..
        }) => Ok((*bbox, *number, *width, *height)),
        Some(pdfluent::compat::DictBlock::Text { .. }) => {
            Err(fail_arg("dict block is not an image block"))
        }
        Some(_) => Err(fail_arg("dict block is not an image block")),
        None => Err(fail_arg("dict block index out of range")),
    }
}

/// Image dict block box.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_image_bbox(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let quad = match dict_image_quad(&dict, index) {
        Ok(quad) => quad,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(quad.0) };
    PdfStatus::Ok
}

/// Image dict block number.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_image_number(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let quad = match dict_image_quad(&dict, index) {
        Ok(quad) => quad,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = quad.1 };
    PdfStatus::Ok
}

/// Image dict block width in pixels.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_image_width(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let quad = match dict_image_quad(&dict, index) {
        Ok(quad) => quad,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = quad.2 };
    PdfStatus::Ok
}

/// Image dict block height in pixels.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_dict_image_height(
    handle: *const PdfCompatTextOutput,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let output = text_ref!(handle);
    let dict = match text_dict(output) {
        Ok(dict) => dict,
        Err(status) => return status,
    };
    let quad = match dict_image_quad(&dict, index) {
        Ok(quad) => quad,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = quad.3 };
    PdfStatus::Ok
}

// ---- Search hits -----------------------------------------------------------

macro_rules! hits_ref {
    ($handle:expr) => {{
        if $handle.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live search-hits handle.
        unsafe { &(*$handle).0 }
    }};
}

/// Whether hits are quadrilaterals (1) or rectangles (0).
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_search_hits_is_quads(
    handle: *const PdfCompatSearchHits,
    out: *mut i32,
) -> PdfStatus {
    let hits = hits_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let quads = match hits {
        pdfluent::compat::SearchHits::Rects(_) => false,
        pdfluent::compat::SearchHits::Quads(_) => true,
        _ => return fail_unknown_variant("search hits"),
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = i32::from(quads) };
    PdfStatus::Ok
}

/// Hit count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_search_hits_count(
    handle: *const PdfCompatSearchHits,
    out: *mut i32,
) -> PdfStatus {
    let hits = hits_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let count = match hits {
        pdfluent::compat::SearchHits::Rects(hits) => hits.len(),
        pdfluent::compat::SearchHits::Quads(hits) => hits.len(),
        _ => return fail_unknown_variant("search hits"),
    };
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = count as i32 };
    PdfStatus::Ok
}

/// Hit rectangle; fails when the hits are quadrilaterals.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_search_hits_rect_at(
    handle: *const PdfCompatSearchHits,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let hits = hits_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match hits {
        pdfluent::compat::SearchHits::Rects(hits) => match hits.get(index as usize) {
            // SAFETY: the caller guarantees a writable out-param.
            Some(rect) => unsafe { *out = PdfCompatRect::from(*rect) },
            None => return fail_arg("hit index out of range"),
        },
        pdfluent::compat::SearchHits::Quads(_) => {
            return fail_arg("search hits are quadrilaterals");
        }
        _ => return fail_unknown_variant("search hits"),
    }
    PdfStatus::Ok
}

/// Hit quadrilateral; fails when the hits are rectangles.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_search_hits_quad_at(
    handle: *const PdfCompatSearchHits,
    index: i32,
    out: *mut PdfCompatQuad,
) -> PdfStatus {
    let hits = hits_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match hits {
        pdfluent::compat::SearchHits::Quads(hits) => match hits.get(index as usize) {
            // SAFETY: the caller guarantees a writable out-param.
            Some(quad) => unsafe { *out = PdfCompatQuad::from(*quad) },
            None => return fail_arg("hit index out of range"),
        },
        pdfluent::compat::SearchHits::Rects(_) => {
            return fail_arg("search hits are rectangles");
        }
        _ => return fail_unknown_variant("search hits"),
    }
    PdfStatus::Ok
}

// ---- Table finder ----------------------------------------------------------

macro_rules! tables_ref {
    ($handle:expr) => {{
        if $handle.is_null() {
            return fail_arg("null pointer argument");
        }
        // SAFETY: the caller guarantees a live table-finder handle.
        unsafe { &(*$handle).0 }
    }};
}

fn table_at(
    finder: &pdfluent::compat::TableFinder,
    index: i32,
) -> Result<pdfluent::compat::Table, PdfStatus> {
    finder
        .tables
        .get(index as usize)
        .cloned()
        .ok_or_else(|| fail_arg("table index out of range"))
}

/// Table count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_count(
    handle: *const PdfCompatTableFinder,
    out: *mut i32,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = finder.tables.len() as i32 };
    PdfStatus::Ok
}

/// Table box in points.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_bbox(
    handle: *const PdfCompatTableFinder,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = PdfCompatRect::from(table.bbox) };
    PdfStatus::Ok
}

/// Table row count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_row_count(
    handle: *const PdfCompatTableFinder,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = table.row_count };
    PdfStatus::Ok
}

/// Table column count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_col_count(
    handle: *const PdfCompatTableFinder,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = table.col_count };
    PdfStatus::Ok
}

/// Table cell count.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_cell_count(
    handle: *const PdfCompatTableFinder,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = table.cells.len() as i32 };
    PdfStatus::Ok
}

/// Table cell box in points.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_cell_bbox(
    handle: *const PdfCompatTableFinder,
    index: i32,
    cell: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match table.cells.get(cell as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(entry) => unsafe { *out = PdfCompatRect::from(entry.bbox) },
        None => return fail_arg("cell index out of range"),
    }
    PdfStatus::Ok
}

/// Table cell text.
///
/// # Safety
/// `handle` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_table_cell_text(
    handle: *const PdfCompatTableFinder,
    index: i32,
    cell: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    let finder = tables_ref!(handle);
    let table = match table_at(finder, index) {
        Ok(table) => table,
        Err(status) => return status,
    };
    match table.cells.get(cell as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(entry) => unsafe { out_string(out, entry.text.clone()) },
        None => fail_arg("cell index out of range"),
    }
}

// ---- Drawing-item parameters -----------------------------------------------

/// Serialize one drawing path-item's parameters to JSON text.
///
/// Only used by the generated drawing-list getters. Fails, with the error
/// recorded, on a parameter value this binding cannot spell.
pub(crate) fn drawing_item_params_json(
    item: &pdfluent::compat::DrawingItem,
) -> Result<String, PdfStatus> {
    value_to_json(&item.params)
        .map(|json| json.to_string())
        .map_err(|err| fail(&err, false))
}

/// Read drawing bytes or ints len helpers for the generated getters.
pub(crate) fn drawing_item_op(item: &pdfluent::compat::DrawingItem) -> String {
    item.op.clone()
}
