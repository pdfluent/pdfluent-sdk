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

// ---- PDF/A compliance ----------------------------------------------------

/// Validate a document against a PDF/A conformance level.
///
/// On success writes a `PdfComplianceReport` pointer to `*out`. The caller
/// owns the report and must free it with `pdf_compliance_report_free`.
/// The report is written even when the document is non-compliant — a
/// `PDF_STATUS_OK` return only means the validation ran, not that it passed.
///
/// # Safety
/// `doc` must be valid. `out` must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_validate_pdfa(
    doc: *const PdfDocument,
    level: PdfALevel,
    out: *mut *mut PdfComplianceReport,
) -> PdfStatus {
    if doc.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let pdf = unsafe { &*doc }.0.pdf();
    let report = pdf_compliance::validate_pdfa(pdf, level.to_compliance_level());
    unsafe { *out = Box::into_raw(Box::new(PdfComplianceReport(report))) };
    PdfStatus::Ok
}

/// Returns 1 if the compliance report indicates full conformance, 0 otherwise.
/// Returns 0 when `report` is null.
///
/// # Safety
/// `report` must have been returned by `pdf_document_validate_pdfa`, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compliance_report_is_compliant(
    report: *const PdfComplianceReport,
) -> i32 {
    if report.is_null() {
        return 0;
    }
    unsafe { &*report }.0.is_compliant() as i32
}

/// Number of conformance errors in the report, or -1 if `report` is null.
///
/// # Safety
/// `report` must have been returned by `pdf_document_validate_pdfa`, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compliance_report_error_count(
    report: *const PdfComplianceReport,
) -> i32 {
    if report.is_null() {
        return -1;
    }
    unsafe { &*report }.0.error_count() as i32
}

/// Free a compliance report. Null is safe (no-op).
///
/// # Safety
/// `report` must have been returned by `pdf_document_validate_pdfa` and not yet freed, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_compliance_report_free(report: *mut PdfComplianceReport) {
    if !report.is_null() {
        drop(unsafe { Box::from_raw(report) });
    }
}

// ---- PDF/A conversion ----------------------------------------------------

/// Convert a document to PDF/A and return the result as a new document.
///
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc` and `out` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_convert_pdfa(
    doc: *const PdfDocument,
    level: PdfALevel,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw_bytes) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error(&format!("lopdf load: {e}"));
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    // cleanup_for_pdfa applies PDF/A-incompatible element removal; is_pdfa1
    // enables stricter PDF/A-1 rules (e.g. no transparency at all).
    if let Err(e) =
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut lopdf_doc, level.is_part1())
    {
        error::set_last_error(&e.to_string());
        return PdfStatus::ErrorConvert;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error(&format!("save: {e}"));
        return PdfStatus::ErrorConvert;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorConvert
        }
    }
}

// ---- Redaction -----------------------------------------------------------

/// Redact all text occurrences of `pattern` in the document.
///
/// Returns a new document with the redactions applied. The caller owns the
/// returned document and must free it with `pdf_document_free`.
/// If no text matches `pattern`, the document is returned unchanged (no error).
///
/// # Safety
/// `doc`, `pattern`, and `out` must be valid non-null pointers.
/// `pattern` must be a null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_redact(
    doc: *const PdfDocument,
    pattern: *const c_char,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || pattern.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let pat = match unsafe { CStr::from_ptr(pattern) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid UTF-8 in pattern");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw_bytes) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error(&format!("lopdf load: {e}"));
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    let opts = pdf_redact::RedactSearchOptions::default();
    if let Err(e) = pdf_redact::search_and_redact(&mut lopdf_doc, pat, &opts) {
        error::set_last_error(&e.to_string());
        return PdfStatus::ErrorRedact;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error(&format!("save: {e}"));
        return PdfStatus::ErrorRedact;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorRedact
        }
    }
}

// ---- Signing -------------------------------------------------------------

/// Sign a document using a PKCS#12 identity and return the signed document.
///
/// `pkcs12_path` is the filesystem path to the .p12 / .pfx bundle.
/// `pkcs12_password` unlocks the bundle; pass `NULL` or an empty string for
/// password-less bundles.
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc`, `pkcs12_path`, and `out` must be valid non-null pointers.
/// Both C strings must be null-terminated UTF-8. `pkcs12_password` may be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_sign(
    doc: *const PdfDocument,
    pkcs12_path: *const c_char,
    pkcs12_password: *const c_char,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || pkcs12_path.is_null() || out.is_null() {
        error::set_last_error("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let path_str = match unsafe { CStr::from_ptr(pkcs12_path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid UTF-8 in pkcs12_path");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let password = if pkcs12_password.is_null() {
        ""
    } else {
        match unsafe { CStr::from_ptr(pkcs12_password) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error("invalid UTF-8 in pkcs12_password");
                return PdfStatus::ErrorInvalidArgument;
            }
        }
    };
    let pkcs12_bytes = match std::fs::read(path_str) {
        Ok(b) => b,
        Err(e) => {
            error::set_last_error(&format!("failed to read PKCS#12: {e}"));
            return PdfStatus::ErrorFileNotFound;
        }
    };
    let signer = match pdf_sign::Pkcs12Signer::from_pkcs12(&pkcs12_bytes, password) {
        Ok(s) => s,
        Err(e) => {
            error::set_last_error(&e.to_string());
            return PdfStatus::ErrorSign;
        }
    };
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref();
    let signed_bytes =
        match pdf_sign::sign_pdf(raw_bytes, &signer, &pdf_sign::SignOptions::default()) {
            Ok(b) => b,
            Err(e) => {
                error::set_last_error(&e.to_string());
                return PdfStatus::ErrorSign;
            }
        };
    match pdf_engine::PdfDocument::open(signed_bytes) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e.to_string());
            PdfStatus::ErrorSign
        }
    }
}

// ---- Form fields ---------------------------------------------------------

/// Number of terminal (leaf-widget) AcroForm fields. Returns -1 if `doc` is
/// null; returns 0 if the document has no AcroForm or no terminal fields.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_form_field_count(doc: *const PdfDocument) -> i32 {
    if doc.is_null() {
        error::set_last_error("null document pointer");
        return -1;
    }
    let pdf = unsafe { &*doc }.0.pdf();
    match pdf_forms::parse_acroform(pdf) {
        Some(tree) => tree.terminal_fields().len() as i32,
        None => 0,
    }
}

/// Return the fully qualified name of the AcroForm field at zero-based `index`.
/// Returns null if `doc` is null, `index` is negative or out of range, or the
/// document has no AcroForm. Free the returned string with `pdf_string_free`.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_form_field_name(
    doc: *const PdfDocument,
    index: i32,
) -> *mut c_char {
    if doc.is_null() || index < 0 {
        return ptr::null_mut();
    }
    let pdf = unsafe { &*doc }.0.pdf();
    let tree = match pdf_forms::parse_acroform(pdf) {
        Some(t) => t,
        None => return ptr::null_mut(),
    };
    let terminals = tree.terminal_fields();
    let idx = index as usize;
    if idx >= terminals.len() {
        return ptr::null_mut();
    }
    let name = tree.fully_qualified_name(terminals[idx]);
    match std::ffi::CString::new(name) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => ptr::null_mut(),
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
