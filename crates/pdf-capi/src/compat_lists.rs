//! Compatible snapshot lists: typed read-only views over result vectors.
//!
//! Generated from the member table; one `pdf_compat_*` wrapper per bound spec
//! member. See `compat.rs` for the conventions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::os::raw::c_char;

use super::compat::{
    fail_arg, out_bytes, out_handle, out_string, PdfCompatFont, PdfCompatLink, PdfCompatMatrix,
    PdfCompatPixmap, PdfCompatPoint, PdfCompatRect, PdfCompatSearchHits, PdfCompatTableFinder,
    PdfCompatTextOutput, PdfCompatTextWriter, PdfCompatWidget, PdfStatus,
};
use super::compat_handles::{copy_doubles, copy_ints, drawing_item_op, drawing_item_params_json};

/// Snapshot toc values.
pub struct PdfCompatTocList(pub(crate) Vec<pdfluent::compat::TocEntry>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_count(
    list: *const PdfCompatTocList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Nesting level, from 1.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_level(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.level };
    PdfStatus::Ok
}

/// Entry title.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_title(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.title.clone()) }
}

/// 1-based destination page.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_page(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.page };
    PdfStatus::Ok
}

/// Whether a follow-on destination exists.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_has_dest(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = i32::from(item.dest.is_some()) };
    PdfStatus::Ok
}

/// Destination kind, or -1.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_dest_kind(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.dest.as_ref().map(|d| d.kind).unwrap_or(-1) };
    PdfStatus::Ok
}

/// Destination page, or -1.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_dest_page(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.dest.as_ref().map(|d| d.page).unwrap_or(-1) };
    PdfStatus::Ok
}

/// Destination point, or the origin.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_dest_target(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe {
        *out = PdfCompatPoint::from(
            item.dest
                .as_ref()
                .map(|d| d.target)
                .unwrap_or(pdfluent::compat::Point { x: 0.0, y: 0.0 }),
        )
    };
    PdfStatus::Ok
}

/// Destination zoom, or 0.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_dest_zoom(
    list: *const PdfCompatTocList,
    index: i32,
    out: *mut f64,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.dest.as_ref().map(|d| d.zoom).unwrap_or(0.0) };
    PdfStatus::Ok
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_toc_list_free(list: *mut PdfCompatTocList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// A single imagedata snapshot.
pub struct PdfCompatImageData(pub(crate) pdfluent::compat::ImageData);

/// Width in pixels.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_width(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.width };
    PdfStatus::Ok
}

/// Height in pixels.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_height(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.height };
    PdfStatus::Ok
}

/// Bits per component.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_bits(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.bits };
    PdfStatus::Ok
}

/// Colour-space code.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_colorspace(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.colorspace };
    PdfStatus::Ok
}

/// Colour-space name.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_colorspace_name(
    handle: *const PdfCompatImageData,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { out_string(out, item.colorspace_name.clone()) }
}

/// File extension.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_ext(
    handle: *const PdfCompatImageData,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { out_string(out, item.ext.clone()) }
}

/// Payload size in bytes.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_size(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.size };
    PdfStatus::Ok
}

/// Whether masked.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_masked(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = i32::from(item.masked) };
    PdfStatus::Ok
}

/// Horizontal resolution.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_xres(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.xres };
    PdfStatus::Ok
}

/// Vertical resolution.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_yres(
    handle: *const PdfCompatImageData,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { *out = item.yres };
    PdfStatus::Ok
}

/// Raw payload bytes.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_data(
    handle: *const PdfCompatImageData,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { out_bytes(out_data, out_len, item.data.clone()) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_data_free(list: *mut PdfCompatImageData) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// A single fontdata snapshot.
pub struct PdfCompatFontData(pub(crate) pdfluent::compat::FontData);

/// Font name.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_data_name(
    handle: *const PdfCompatFontData,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { out_string(out, item.name.clone()) }
}

/// Font bytes.
///
/// # Safety
/// `handle` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_data_data(
    handle: *const PdfCompatFontData,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if handle.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live handle.
    let item = unsafe { &(*handle).0 };
    unsafe { out_bytes(out_data, out_len, item.data.clone()) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_data_free(list: *mut PdfCompatFontData) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot fontinfo values.
pub struct PdfCompatFontInfoList(pub(crate) Vec<pdfluent::compat::FontInfo>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_info_list_count(
    list: *const PdfCompatFontInfoList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Font object number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_info_list_xref(
    list: *const PdfCompatFontInfoList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.xref };
    PdfStatus::Ok
}

/// Font name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_info_list_name(
    list: *const PdfCompatFontInfoList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.name.clone()) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_info_list_free(list: *mut PdfCompatFontInfoList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot imageref values.
pub struct PdfCompatImageRefList(pub(crate) Vec<pdfluent::compat::ImageRef>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_count(
    list: *const PdfCompatImageRefList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Image object number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_xref(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.xref };
    PdfStatus::Ok
}

/// Mask object number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_mask(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.mask };
    PdfStatus::Ok
}

/// Width in pixels.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_width(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.width };
    PdfStatus::Ok
}

/// Height in pixels.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_height(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.height };
    PdfStatus::Ok
}

/// Bits per component.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_bits(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.bits };
    PdfStatus::Ok
}

/// Colour-space name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_colorspace_name(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.colorspace_name.clone()) }
}

/// Alternate name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_alt(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.alt.clone()) }
}

/// Resource name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_name(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.name.clone()) }
}

/// Filter name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_filter(
    list: *const PdfCompatImageRefList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.filter.clone()) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_image_ref_list_free(list: *mut PdfCompatImageRefList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot placedimage values.
pub struct PdfCompatPlacedImageList(pub(crate) Vec<pdfluent::compat::PlacedImage>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_count(
    list: *const PdfCompatPlacedImageList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Placed box in points.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_bbox(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(item.bbox) };
    PdfStatus::Ok
}

/// Image-to-page matrix.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_transform(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut PdfCompatMatrix,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatMatrix::from(item.transform) };
    PdfStatus::Ok
}

/// Width in pixels.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_width(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.width };
    PdfStatus::Ok
}

/// Height in pixels.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_height(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.height };
    PdfStatus::Ok
}

/// Bits per component.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_bits(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.bits };
    PdfStatus::Ok
}

/// Payload size in bytes.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_size(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.size };
    PdfStatus::Ok
}

/// Sequence number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_number(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.number };
    PdfStatus::Ok
}

/// Colour-space name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_colorspace_name(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.colorspace_name.clone()) }
}

/// Whether masked.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_has_mask(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = i32::from(item.has_mask) };
    PdfStatus::Ok
}

/// Horizontal resolution.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_xres(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.xres };
    PdfStatus::Ok
}

/// Vertical resolution.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_yres(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.yres };
    PdfStatus::Ok
}

/// Whether a digest is present.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_has_digest(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = i32::from(item.digest.is_some()) };
    PdfStatus::Ok
}

/// Digest bytes, or empty.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_digest(
    list: *const PdfCompatPlacedImageList,
    index: i32,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_bytes(out_data, out_len, item.digest.clone().unwrap_or_default()) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_placed_image_list_free(list: *mut PdfCompatPlacedImageList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot rect values.
pub struct PdfCompatRectList(pub(crate) Vec<pdfluent::compat::Rect>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_list_count(
    list: *const PdfCompatRectList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Rectangle at an index.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_list_at(
    list: *const PdfCompatRectList,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(*item) };
    PdfStatus::Ok
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_rect_list_free(list: *mut PdfCompatRectList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot drawing values.
pub struct PdfCompatDrawingList(pub(crate) Vec<pdfluent::compat::Drawing>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_count(
    list: *const PdfCompatDrawingList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Drawing box in points.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_rect(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(item.rect) };
    PdfStatus::Ok
}

/// Sequence number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_seqno(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.seqno };
    PdfStatus::Ok
}

/// Drawing operator name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_drawing_type(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.drawing_type.clone()) }
}

/// Stroke colour, or empty.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_color(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out_data: *mut *mut f64,
    out_len: *mut usize,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.color.clone() {
        Some(values) => {
            unsafe { *out_has = 1 };
            unsafe { copy_doubles(out_data, out_len, &values) }
        }
        None => {
            unsafe { *out_has = 0 };
            unsafe { copy_doubles(out_data, out_len, &[] as &[f64]) }
        }
    }
}

/// Fill colour, or empty.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_fill(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out_data: *mut *mut f64,
    out_len: *mut usize,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.fill.clone() {
        Some(values) => {
            unsafe { *out_has = 1 };
            unsafe { copy_doubles(out_data, out_len, &values) }
        }
        None => {
            unsafe { *out_has = 0 };
            unsafe { copy_doubles(out_data, out_len, &[] as &[f64]) }
        }
    }
}

/// Stroke width, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_width(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut f64,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.width {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = value
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0.0
            };
            PdfStatus::Ok
        }
    }
}

/// Line-cap values, or empty.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_line_cap(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out_data: *mut *mut i32,
    out_len: *mut usize,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.line_cap.clone() {
        Some(values) => {
            unsafe { *out_has = 1 };
            unsafe { copy_ints(out_data, out_len, &values) }
        }
        None => {
            unsafe { *out_has = 0 };
            unsafe { copy_ints(out_data, out_len, &[] as &[i32]) }
        }
    }
}

/// Line-join value, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_line_join(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut i32,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.line_join {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = value
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0
            };
            PdfStatus::Ok
        }
    }
}

/// Dash pattern, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_dashes(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.dashes.clone() {
        Some(value) => {
            unsafe { *out_has = 1 };
            unsafe { out_string(out, value) }
        }
        None => {
            unsafe { *out_has = 0 };
            unsafe { out_string(out, String::new()) }
        }
    }
}

/// Close-path flag, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_close_path(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut i32,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.close_path {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = i32::from(value)
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0
            };
            PdfStatus::Ok
        }
    }
}

/// Even-odd fill rule.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_even_odd(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = i32::from(item.even_odd) };
    PdfStatus::Ok
}

/// Fill opacity, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_fill_opacity(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut f64,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.fill_opacity {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = value
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0.0
            };
            PdfStatus::Ok
        }
    }
}

/// Stroke opacity, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_stroke_opacity(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut f64,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.stroke_opacity {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = value
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0.0
            };
            PdfStatus::Ok
        }
    }
}

/// Layer name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_layer(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.layer.clone()) }
}

/// Nesting level, if any.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_level(
    list: *const PdfCompatDrawingList,
    index: i32,
    out_has: *mut i32,
    out: *mut i32,
) -> PdfStatus {
    if out_has.is_null() {
        return fail_arg("null pointer argument");
    }
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    match item.level {
        Some(value) => {
            unsafe {
                *out_has = 1;
                *out = value
            };
            PdfStatus::Ok
        }
        None => {
            unsafe {
                *out_has = 0;
                *out = 0
            };
            PdfStatus::Ok
        }
    }
}

/// Path-item count.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_item_count(
    list: *const PdfCompatDrawingList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.items.len() as i32 };
    PdfStatus::Ok
}

/// Path-item operator.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_item_op(
    list: *const PdfCompatDrawingList,
    index: i32,
    sub: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.items).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, drawing_item_op(entry)) }
}

/// Path-item parameters as JSON.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_item_params(
    list: *const PdfCompatDrawingList,
    index: i32,
    sub: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.items).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    let params = match drawing_item_params_json(entry) {
        Ok(params) => params,
        Err(status) => return status,
    };
    unsafe { out_string(out, params) }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_drawing_list_free(list: *mut PdfCompatDrawingList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot link values.
pub struct PdfCompatLinkList(pub(crate) Vec<pdfluent::compat::Link>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_list_count(
    list: *const PdfCompatLinkList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Clone one entry into a fresh handle.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_list_at(
    list: *const PdfCompatLinkList,
    index: i32,
    out: *mut *mut PdfCompatLink,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    match items.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(entry) => unsafe { out_handle(out, PdfCompatLink(entry.clone())) },
        None => fail_arg("index out of range"),
    }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_list_free(list: *mut PdfCompatLinkList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot widget values.
pub struct PdfCompatWidgetList(pub(crate) Vec<pdfluent::compat::Widget>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_list_count(
    list: *const PdfCompatWidgetList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Clone one entry into a fresh handle.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_list_at(
    list: *const PdfCompatWidgetList,
    index: i32,
    out: *mut *mut PdfCompatWidget,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    match items.get(index as usize) {
        // SAFETY: the caller guarantees a writable out-param.
        Some(entry) => unsafe { out_handle(out, PdfCompatWidget(entry.clone())) },
        None => fail_arg("index out of range"),
    }
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_list_free(list: *mut PdfCompatWidgetList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot annotentry values.
pub struct PdfCompatAnnotEntryList(pub(crate) Vec<pdfluent::compat::AnnotEntry>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_count(
    list: *const PdfCompatAnnotEntryList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Annotation object number.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_xref(
    list: *const PdfCompatAnnotEntryList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.xref };
    PdfStatus::Ok
}

/// Annotation type code.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_type_code(
    list: *const PdfCompatAnnotEntryList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.annot_type.code };
    PdfStatus::Ok
}

/// Annotation type name.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_type_name(
    list: *const PdfCompatAnnotEntryList,
    index: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { out_string(out, item.annot_type.name.clone()) }
}

/// Annotation box in points.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_rect(
    list: *const PdfCompatAnnotEntryList,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(item.rect) };
    PdfStatus::Ok
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_annot_entry_list_free(list: *mut PdfCompatAnnotEntryList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Snapshot texttrace values.
pub struct PdfCompatTextTraceList(pub(crate) Vec<pdfluent::compat::TextTrace>);

/// Snapshot count.
///
/// # Safety
/// `list` must be live; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_count(
    list: *const PdfCompatTextTraceList,
    out: *mut i32,
) -> PdfStatus {
    if list.is_null() || out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list and a writable out-param.
    unsafe { *out = (*list).0.len() as i32 };
    PdfStatus::Ok
}

/// Span box in points.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_bbox(
    list: *const PdfCompatTextTraceList,
    index: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(item.bbox) };
    PdfStatus::Ok
}

/// Ascender height.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_ascender(
    list: *const PdfCompatTextTraceList,
    index: i32,
    out: *mut f64,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.ascender };
    PdfStatus::Ok
}

/// Writing direction.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_bidi_dir(
    list: *const PdfCompatTextTraceList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.bidi_dir };
    PdfStatus::Ok
}

/// Bidi level.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_bidi_level(
    list: *const PdfCompatTextTraceList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.bidi_level };
    PdfStatus::Ok
}

/// Character count.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_char_count(
    list: *const PdfCompatTextTraceList,
    index: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = item.chars.len() as i32 };
    PdfStatus::Ok
}

/// Character codepoint.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_char_codepoint(
    list: *const PdfCompatTextTraceList,
    index: i32,
    sub: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.chars).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = entry.codepoint };
    PdfStatus::Ok
}

/// Glyph id.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_char_glyph(
    list: *const PdfCompatTextTraceList,
    index: i32,
    sub: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.chars).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = entry.glyph };
    PdfStatus::Ok
}

/// Glyph origin.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_char_origin(
    list: *const PdfCompatTextTraceList,
    index: i32,
    sub: i32,
    out: *mut PdfCompatPoint,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.chars).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatPoint::from(entry.origin) };
    PdfStatus::Ok
}

/// Glyph box.
///
/// # Safety
/// `list` must be live; out-params must be writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_char_bbox(
    list: *const PdfCompatTextTraceList,
    index: i32,
    sub: i32,
    out: *mut PdfCompatRect,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if list.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a live list.
    let items = unsafe { &(*list).0 };
    let item = match items.get(index as usize) {
        Some(it) => it,
        None => return fail_arg("index out of range"),
    };
    let entry = match (item.chars).get(sub as usize) {
        Some(e) => e,
        None => return fail_arg("index out of range"),
    };
    unsafe { *out = PdfCompatRect::from(entry.bbox) };
    PdfStatus::Ok
}

/// Free a snapshot list. Null is safe.
///
/// # Safety
/// `list` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_trace_list_free(list: *mut PdfCompatTextTraceList) {
    if !list.is_null() {
        // SAFETY: the caller guarantees ownership of the list.
        unsafe { drop(Box::from_raw(list)) };
    }
}

/// Free a pixel buffer. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_pixmap_free(handle: *mut PdfCompatPixmap) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a font. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_font_free(handle: *mut PdfCompatFont) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a staged-text writer. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_writer_free(handle: *mut PdfCompatTextWriter) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a widget. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_widget_free(handle: *mut PdfCompatWidget) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a link. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_link_free(handle: *mut PdfCompatLink) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a table finder. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_table_finder_free(handle: *mut PdfCompatTableFinder) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free a text result. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_text_output_free(handle: *mut PdfCompatTextOutput) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}

/// Free search hits. Null is safe.
///
/// # Safety
/// `handle` must have been returned by this API and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_search_hits_free(handle: *mut PdfCompatSearchHits) {
    if !handle.is_null() {
        // SAFETY: the caller guarantees ownership of the handle.
        unsafe { drop(Box::from_raw(handle)) };
    }
}
