//! C-compatible API for the PDF engine.
//!
//! Provides a stable C ABI for embedding the PDF engine in non-Rust applications.
//! Mirrors PDFium-style patterns: opaque handles, status codes, free functions.

mod error;
mod types;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

pub use error::*;
pub use types::*;

// ---- Library lifecycle ---------------------------------------------------

/// Initialize the PDF library. Currently a no-op.
#[no_mangle]
pub extern "C" fn pdf_init() -> PdfStatus {
    PdfStatus::Ok
}

/// Shut down the PDF library. Currently a no-op.
#[no_mangle]
pub extern "C" fn pdf_destroy() {}

/// Return the library version as a static null-terminated string.
#[no_mangle]
pub extern "C" fn pdf_version() -> *const c_char {
    c"0.1.0".as_ptr()
}

// ---- Document lifecycle --------------------------------------------------

/// Open a PDF from a byte buffer. Caller must free with `pdf_document_free`.
///
/// # Safety
/// `data` must point to `len` readable bytes. `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_open_from_bytes(
    data: *const u8,
    len: usize,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if data.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) }.to_vec();
    match pdf_engine::PdfDocument::open(bytes) {
        Ok(doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorCorruptPdf
        }
    }
}

/// Open a PDF from a file path. `password` may be null.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 string. `out` must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_open(
    path: *const c_char,
    password: *const c_char,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if path.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid UTF-8 in path");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let bytes = match std::fs::read(path_str) {
        Ok(b) => b,
        Err(e) => {
            error::set_last_error(&format!("failed to read file: {e}"));
            return PdfStatus::ErrorFileNotFound;
        }
    };
    if password.is_null() {
        match pdf_engine::PdfDocument::open(bytes) {
            Ok(doc) => {
                unsafe { *out = Box::into_raw(Box::new(PdfDocument(doc))) };
                PdfStatus::Ok
            }
            Err(e) => {
                error::set_last_error(&e.to_string());
                PdfStatus::ErrorCorruptPdf
            }
        }
    } else {
        let pw = match unsafe { CStr::from_ptr(password) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid UTF-8 in password");
                return PdfStatus::ErrorInvalidArgument;
            }
        };
        match pdf_engine::PdfDocument::open_with_password(bytes, pw) {
            Ok(doc) => {
                unsafe { *out = Box::into_raw(Box::new(PdfDocument(doc))) };
                PdfStatus::Ok
            }
            Err(e) => {
                error::set_last_error(&e.to_string());
                PdfStatus::ErrorInvalidPassword
            }
        }
    }
}

/// Free a document. Null is safe (no-op).
///
/// # Safety
/// `doc` must have been returned by `pdf_document_open*` and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_free(doc: *mut PdfDocument) {
    if !doc.is_null() {
        drop(unsafe { Box::from_raw(doc) });
    }
}

// ---- Document queries ----------------------------------------------------

/// Page count, or -1 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_page_count(doc: *const PdfDocument) -> i32 {
    if doc.is_null() {
        error::set_last_error("null document pointer");
        return -1;
    }
    unsafe { &*doc }.0.page_count() as i32
}

/// Page width in points, or 0.0 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_width(doc: *const PdfDocument, page_index: i32) -> f64 {
    if doc.is_null() || page_index < 0 {
        return 0.0;
    }
    match unsafe { &*doc }.0.page_geometry(page_index as usize) {
        Ok(g) => g.effective_dimensions().0,
        Err(_) => 0.0,
    }
}

/// Page height in points, or 0.0 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_height(doc: *const PdfDocument, page_index: i32) -> f64 {
    if doc.is_null() || page_index < 0 {
        return 0.0;
    }
    match unsafe { &*doc }.0.page_geometry(page_index as usize) {
        Ok(g) => g.effective_dimensions().1,
        Err(_) => 0.0,
    }
}

/// Page rotation in degrees (0/90/180/270).
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_rotation(doc: *const PdfDocument, page_index: i32) -> i32 {
    if doc.is_null() || page_index < 0 {
        return 0;
    }
    match unsafe { &*doc }.0.page_geometry(page_index as usize) {
        Ok(g) => g.rotation.degrees() as i32,
        Err(_) => 0,
    }
}

// ---- Rendering -----------------------------------------------------------

/// Render page to RGBA at given DPI. Free pixels with `pdf_pixels_free(w*h*4)`.
///
/// # Safety
/// `doc` must be valid. `out_width`, `out_height`, `out_pixels` must be non-null writable pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_render(
    doc: *const PdfDocument,
    page_index: i32,
    dpi: f64,
    out_width: *mut u32,
    out_height: *mut u32,
    out_pixels: *mut *mut u8,
) -> PdfStatus {
    if doc.is_null() || out_width.is_null() || out_height.is_null() || out_pixels.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error("negative page index");
        return PdfStatus::ErrorPageRange;
    }
    let opts = pdf_engine::RenderOptions {
        dpi,
        ..Default::default()
    };
    match unsafe { &*doc }.0.render_page(page_index as usize, &opts) {
        Ok(r) => {
            unsafe { *out_width = r.width };
            unsafe { *out_height = r.height };
            let mut px = r.pixels.into_boxed_slice();
            unsafe { *out_pixels = px.as_mut_ptr() };
            std::mem::forget(px);
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorRender
        }
    }
}

/// Render a thumbnail (fits longest side in `max_dimension`).
///
/// # Safety
/// `doc` must be valid. `out_width`, `out_height`, `out_pixels` must be non-null writable pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_render_thumbnail(
    doc: *const PdfDocument,
    page_index: i32,
    max_dimension: u32,
    out_width: *mut u32,
    out_height: *mut u32,
    out_pixels: *mut *mut u8,
) -> PdfStatus {
    if doc.is_null() || out_width.is_null() || out_height.is_null() || out_pixels.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error("negative page index");
        return PdfStatus::ErrorPageRange;
    }
    let opts = pdf_engine::ThumbnailOptions { max_dimension };
    match unsafe { &*doc }.0.thumbnail(page_index as usize, &opts) {
        Ok(r) => {
            unsafe { *out_width = r.width };
            unsafe { *out_height = r.height };
            let mut px = r.pixels.into_boxed_slice();
            unsafe { *out_pixels = px.as_mut_ptr() };
            std::mem::forget(px);
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorRender
        }
    }
}

/// Free pixel buffer. `len` = `width * height * 4`. Null is safe.
///
/// # Safety
/// `pixels` must have been returned by `pdf_page_render*` with matching `len`, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_pixels_free(pixels: *mut u8, len: usize) {
    if !pixels.is_null() && len > 0 {
        drop(unsafe { Vec::from_raw_parts(pixels, len, len) });
    }
}

// ---- Text extraction -----------------------------------------------------

/// Extract page text as null-terminated UTF-8. Free with `pdf_string_free`.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_extract_text(
    doc: *const PdfDocument,
    page_index: i32,
) -> *mut c_char {
    if doc.is_null() || page_index < 0 {
        error::set_last_error("invalid argument");
        return ptr::null_mut();
    }
    match unsafe { &*doc }.0.extract_text(page_index as usize) {
        Ok(text) => match std::ffi::CString::new(text) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                error::set_last_error("text contains interior null byte");
                ptr::null_mut()
            }
        },
        Err(e) => {
            error::set_last_error(&e.to_string());
            ptr::null_mut()
        }
    }
}

/// Free a string returned by text/metadata functions. Null is safe.
///
/// # Safety
/// `s` must have been returned by a `pdf_page_extract_text` or `pdf_document_get_meta` call, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(s) });
    }
}

// ---- Metadata ------------------------------------------------------------

/// Get metadata by key ("Title"/"Author"/"Subject"/"Keywords"/"Creator"/"Producer").
/// Returns null if absent. Free with `pdf_string_free`.
///
/// # Safety
/// `doc` and `key` must be valid pointers or null. `key` must be null-terminated.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_get_meta(
    doc: *const PdfDocument,
    key: *const c_char,
) -> *mut c_char {
    if doc.is_null() || key.is_null() {
        return ptr::null_mut();
    }
    let key_str = match unsafe { CStr::from_ptr(key) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };
    let info = unsafe { &*doc }.0.info();
    let value = match key_str {
        "Title" => info.title.as_deref(),
        "Author" => info.author.as_deref(),
        "Subject" => info.subject.as_deref(),
        "Keywords" => info.keywords.as_deref(),
        "Creator" => info.creator.as_deref(),
        "Producer" => info.producer.as_deref(),
        _ => None,
    };
    match value {
        Some(v) => match std::ffi::CString::new(v) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        None => ptr::null_mut(),
    }
}

// ---- Bookmarks -----------------------------------------------------------

/// Number of top-level bookmarks, or 0 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_bookmark_count(doc: *const PdfDocument) -> i32 {
    if doc.is_null() {
        return 0;
    }
    unsafe { &*doc }.0.bookmarks().len() as i32
}

// ---- Page geometry boxes -------------------------------------------------

/// Get media box of a page.
///
/// # Safety
/// `doc` must be valid. All output pointers must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_media_box(
    doc: *const PdfDocument,
    page_index: i32,
    out_x0: *mut f64,
    out_y0: *mut f64,
    out_x1: *mut f64,
    out_y1: *mut f64,
) -> PdfStatus {
    unsafe {
        get_box(
            doc,
            page_index,
            |g| &g.media_box,
            out_x0,
            out_y0,
            out_x1,
            out_y1,
        )
    }
}

/// Get crop box of a page.
///
/// # Safety
/// `doc` must be valid. All output pointers must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_crop_box(
    doc: *const PdfDocument,
    page_index: i32,
    out_x0: *mut f64,
    out_y0: *mut f64,
    out_x1: *mut f64,
    out_y1: *mut f64,
) -> PdfStatus {
    unsafe {
        get_box(
            doc,
            page_index,
            |g| &g.crop_box,
            out_x0,
            out_y0,
            out_x1,
            out_y1,
        )
    }
}

unsafe fn get_box(
    doc: *const PdfDocument,
    page_index: i32,
    extract: fn(&pdf_engine::PageGeometry) -> &pdf_engine::PageBox,
    out_x0: *mut f64,
    out_y0: *mut f64,
    out_x1: *mut f64,
    out_y1: *mut f64,
) -> PdfStatus {
    if doc.is_null() || out_x0.is_null() || out_y0.is_null() || out_x1.is_null() || out_y1.is_null()
    {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error("negative page index");
        return PdfStatus::ErrorPageRange;
    }
    match unsafe { &*doc }.0.page_geometry(page_index as usize) {
        Ok(g) => {
            let b = extract(&g);
            unsafe {
                *out_x0 = b.x0;
                *out_y0 = b.y0;
                *out_x1 = b.x1;
                *out_y1 = b.y1;
            }
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorPageRange
        }
    }
}

// ---- Tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_not_null() {
        let v = pdf_version();
        assert!(!v.is_null());
        assert_eq!(unsafe { CStr::from_ptr(v) }.to_str().unwrap(), "0.1.0");
    }

    #[test]
    fn init_destroy_idempotent() {
        assert_eq!(pdf_init(), PdfStatus::Ok);
        pdf_destroy();
    }

    #[test]
    fn null_doc_returns_error() {
        assert_eq!(unsafe { pdf_document_page_count(ptr::null()) }, -1);
        assert_eq!(unsafe { pdf_page_width(ptr::null(), 0) }, 0.0);
        assert_eq!(unsafe { pdf_page_height(ptr::null(), 0) }, 0.0);
    }

    #[test]
    fn open_null_rejects() {
        let mut out: *mut PdfDocument = ptr::null_mut();
        let s = unsafe { pdf_document_open_from_bytes(ptr::null(), 0, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn open_invalid_pdf() {
        let data = b"not a pdf";
        let mut out: *mut PdfDocument = ptr::null_mut();
        let s = unsafe { pdf_document_open_from_bytes(data.as_ptr(), data.len(), &mut out) };
        assert_eq!(s, PdfStatus::ErrorCorruptPdf);
        assert!(out.is_null());
    }

    #[test]
    fn free_null_safe() {
        unsafe { pdf_document_free(ptr::null_mut()) };
        unsafe { pdf_string_free(ptr::null_mut()) };
        unsafe { pdf_pixels_free(ptr::null_mut(), 0) };
    }

    #[test]
    fn error_state() {
        pdf_clear_error();
        assert!(pdf_get_last_error().is_null());

        let mut out: *mut PdfDocument = ptr::null_mut();
        let _ = unsafe { pdf_document_open_from_bytes(ptr::null(), 0, &mut out) };
        let err = pdf_get_last_error();
        assert!(!err.is_null());
        let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
        assert!(msg.contains("null"));

        pdf_clear_error();
        assert!(pdf_get_last_error().is_null());
    }

    #[test]
    fn status_repr() {
        assert_eq!(PdfStatus::Ok as i32, 0);
        assert_eq!(PdfStatus::ErrorInvalidArgument as i32, 1);
        assert_eq!(PdfStatus::ErrorUnknown as i32, 99);
    }

    #[test]
    fn bookmark_null() {
        assert_eq!(unsafe { pdf_bookmark_count(ptr::null()) }, 0);
    }

    #[test]
    fn meta_null() {
        let key = b"Title\0".as_ptr().cast::<c_char>();
        assert!(unsafe { pdf_document_get_meta(ptr::null(), key) }.is_null());
    }
}
