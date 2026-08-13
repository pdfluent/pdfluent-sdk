//! C-compatible API for the PDF engine.
//!
//! Provides a stable C ABI for embedding the PDF engine in non-Rust applications.
//! Mirrors PDFium-style patterns: opaque handles, status codes, free functions.

mod error;
mod license;
mod text_edit;
mod types;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

pub use error::*;
pub use license::*;
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) }.to_vec();
    match pdf_engine::PdfDocument::open(bytes) {
        Ok(doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error_str("invalid UTF-8 in path");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let bytes = match std::fs::read(path_str) {
        Ok(b) => b,
        Err(e) => {
            error::set_last_error_str(&format!("failed to read file: {e}"));
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
                error::set_last_error(&e);
                PdfStatus::ErrorCorruptPdf
            }
        }
    } else {
        let pw = match unsafe { CStr::from_ptr(password) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error_str("invalid UTF-8 in password");
                return PdfStatus::ErrorInvalidArgument;
            }
        };
        match pdf_engine::PdfDocument::open_with_password(bytes, pw) {
            Ok(doc) => {
                unsafe { *out = Box::into_raw(Box::new(PdfDocument(doc))) };
                PdfStatus::Ok
            }
            Err(e) => {
                error::set_last_error(&e);
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
        error::set_last_error_str("null document pointer");
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error_str("negative page index");
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
            error::set_last_error_str(&e.to_string());
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error_str("negative page index");
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
            error::set_last_error_str(&e.to_string());
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
        error::set_last_error_str("invalid argument");
        return ptr::null_mut();
    }
    match unsafe { &*doc }.0.extract_text(page_index as usize) {
        Ok(text) => match std::ffi::CString::new(text) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                error::set_last_error_str("text contains interior null byte");
                ptr::null_mut()
            }
        },
        Err(e) => {
            error::set_last_error_str(&e.to_string());
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error_str("negative page index");
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
            error::set_last_error_str(&e.to_string());
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
        error::set_last_error_str("null pointer argument");
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: level.to_convert_conformance(),
        ..Default::default()
    };
    let buf = match pdf_manip::pdfa::convert_bytes(&raw_bytes, &opts) {
        Ok(b) => b,
        Err(e @ pdf_manip::pdfa::PdfAConvertError::LoadFailed) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorConvert;
        }
    };
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let pat = match unsafe { CStr::from_ptr(pattern) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error_str("invalid UTF-8 in pattern");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw_bytes) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&format!("lopdf load: {e}"));
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    let opts = pdf_redact::RedactSearchOptions::default();
    if let Err(e) = pdf_redact::search_and_redact(&mut lopdf_doc, pat, &opts) {
        error::set_last_error_str(&e.to_string());
        return PdfStatus::ErrorRedact;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorRedact;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
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
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let path_str = match unsafe { CStr::from_ptr(pkcs12_path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error_str("invalid UTF-8 in pkcs12_path");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let password = if pkcs12_password.is_null() {
        ""
    } else {
        match unsafe { CStr::from_ptr(pkcs12_password) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                error::set_last_error_str("invalid UTF-8 in pkcs12_password");
                return PdfStatus::ErrorInvalidArgument;
            }
        }
    };
    let pkcs12_bytes = match std::fs::read(path_str) {
        Ok(b) => b,
        Err(e) => {
            error::set_last_error_str(&format!("failed to read PKCS#12: {e}"));
            return PdfStatus::ErrorFileNotFound;
        }
    };
    let signer = match pdf_sign::Pkcs12Signer::from_pkcs12(&pkcs12_bytes, password) {
        Ok(s) => s,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorSign;
        }
    };
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref();
    let signed_bytes =
        match pdf_sign::sign_pdf(raw_bytes, &signer, &pdf_sign::SignOptions::default()) {
            Ok(b) => b,
            Err(e) => {
                error::set_last_error_str(&e.to_string());
                return PdfStatus::ErrorSign;
            }
        };
    match pdf_engine::PdfDocument::open(signed_bytes) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
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
        error::set_last_error_str("null document pointer");
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
pub unsafe extern "C" fn pdf_form_field_name(doc: *const PdfDocument, index: i32) -> *mut c_char {
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

// ---- Annotations ---------------------------------------------------------

/// Number of annotations on a page, or -1 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_annotation_count(doc: *const PdfDocument, page_index: i32) -> i32 {
    if doc.is_null() || page_index < 0 {
        error::set_last_error_str("invalid argument");
        return -1;
    }
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref();
    let lopdf_doc = match lopdf::Document::load_mem(raw_bytes) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return -1;
        }
    };
    page_annot_objects(&lopdf_doc, page_index as u32)
        .map(|v| v.len() as i32)
        .unwrap_or(0)
}

/// Annotation subtype string at the given index on a page, or null on error.
///
/// Free the returned string with `pdf_string_free`.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_annotation_type(
    doc: *const PdfDocument,
    page_index: i32,
    annot_index: i32,
) -> *mut c_char {
    if doc.is_null() || page_index < 0 || annot_index < 0 {
        return ptr::null_mut();
    }
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref();
    let lopdf_doc = match lopdf::Document::load_mem(raw_bytes) {
        Ok(d) => d,
        Err(_) => return ptr::null_mut(),
    };
    let annots = match page_annot_objects(&lopdf_doc, page_index as u32) {
        Some(a) => a,
        None => return ptr::null_mut(),
    };
    let idx = annot_index as usize;
    if idx >= annots.len() {
        return ptr::null_mut();
    }
    let subtype = annot_subtype(&lopdf_doc, &annots[idx]).unwrap_or_else(|| "Unknown".to_string());
    match std::ffi::CString::new(subtype) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Add a yellow highlight annotation to a page. Returns a new document.
///
/// `x`, `y`, `w`, `h` are in PDF user-space units (points), where `y`
/// increases upward from the bottom of the page.
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc` and `out` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_annotation_add_highlight(
    doc: *const PdfDocument,
    page_index: i32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || out.is_null() || page_index < 0 {
        error::set_last_error_str("invalid argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw_bytes = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw_bytes) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    if let Err(msg) = add_highlight(&mut lopdf_doc, page_index as u32, x, y, w, h) {
        error::set_last_error_str(&msg);
        return PdfStatus::ErrorAnnotation;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorAnnotation;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
            PdfStatus::ErrorAnnotation
        }
    }
}

// ---- Document merge ------------------------------------------------------

/// Merge `count` documents into one. `docs` is an array of `count` document pointers.
///
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `docs` must point to `count` valid non-null `PdfDocument` pointers.
/// `out` must be a valid non-null writable pointer.
#[no_mangle]
pub unsafe extern "C" fn pdf_documents_merge(
    docs: *const *const PdfDocument,
    count: i32,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if docs.is_null() || out.is_null() || count <= 0 {
        error::set_last_error_str("invalid argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let count = count as usize;
    let docs_slice = unsafe { slice::from_raw_parts(docs, count) };
    let mut lopdf_docs = Vec::with_capacity(count);
    for &doc_ptr in docs_slice {
        if doc_ptr.is_null() {
            error::set_last_error_str("null document pointer in array");
            return PdfStatus::ErrorInvalidArgument;
        }
        let raw = unsafe { &*doc_ptr }.0.pdf().data().as_ref().to_vec();
        match lopdf::Document::load_mem(&raw) {
            Ok(d) => lopdf_docs.push(d),
            Err(e) => {
                error::set_last_error_str(&e.to_string());
                return PdfStatus::ErrorCorruptPdf;
            }
        }
    }
    let mut merged = match pdf_manip::pages::merge_documents(&lopdf_docs) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorMerge;
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = merged.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorMerge;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
            PdfStatus::ErrorMerge
        }
    }
}

// ---- Annotation helpers --------------------------------------------------

/// Collect the annotation objects for a page (0-based index).
/// Returns None if the page doesn't exist; empty Vec if it has no annotations.
fn page_annot_objects(doc: &lopdf::Document, page_index: u32) -> Option<Vec<lopdf::Object>> {
    let pages = doc.get_pages();
    let page_id = *pages.get(&(page_index + 1))?;
    let page_dict = doc.get_dictionary(page_id).ok()?;
    let annots_obj = page_dict.get(b"Annots").ok()?;
    let (_, annots_resolved) = doc.dereference(annots_obj).ok()?;
    let arr = annots_resolved.as_array().ok()?;
    Some(arr.clone())
}

/// Resolve an annotation object to its /Subtype name.
fn annot_subtype(doc: &lopdf::Document, annot_obj: &lopdf::Object) -> Option<String> {
    let (_, resolved) = doc.dereference(annot_obj).ok()?;
    let annot_dict = resolved.as_dict().ok()?;
    let subtype_obj = annot_dict.get(b"Subtype").ok()?;
    let (_, subtype_resolved) = doc.dereference(subtype_obj).ok()?;
    let name_bytes = subtype_resolved.as_name().ok()?;
    Some(String::from_utf8_lossy(name_bytes).into_owned())
}

/// Add a /Highlight annotation to the given page (0-based) of a lopdf document.
fn add_highlight(
    doc: &mut lopdf::Document,
    page_index: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    use lopdf::{Dictionary, Object};

    let pages = doc.get_pages();
    let page_id = *pages
        .get(&(page_index + 1))
        .ok_or_else(|| format!("page {} out of range", page_index))?;

    let x2 = x + w;
    let y2 = y + h;

    let mut annot = Dictionary::new();
    annot.set("Type", Object::Name(b"Annot".to_vec()));
    annot.set("Subtype", Object::Name(b"Highlight".to_vec()));
    annot.set(
        "Rect",
        Object::Array(vec![
            Object::Real(x as f32),
            Object::Real(y as f32),
            Object::Real(x2 as f32),
            Object::Real(y2 as f32),
        ]),
    );
    // QuadPoints: top-left, top-right, bottom-left, bottom-right
    annot.set(
        "QuadPoints",
        Object::Array(vec![
            Object::Real(x as f32),
            Object::Real(y2 as f32),
            Object::Real(x2 as f32),
            Object::Real(y2 as f32),
            Object::Real(x as f32),
            Object::Real(y as f32),
            Object::Real(x2 as f32),
            Object::Real(y as f32),
        ]),
    );
    // Yellow color (R=1, G=1, B=0)
    annot.set(
        "C",
        Object::Array(vec![
            Object::Real(1.0),
            Object::Real(1.0),
            Object::Real(0.0),
        ]),
    );
    annot.set("F", Object::Integer(4)); // Print flag

    let annot_id = doc.add_object(Object::Dictionary(annot));

    // Check whether /Annots on the page is a direct array or an indirect reference,
    // then append accordingly (two separate borrows to satisfy the borrow checker).
    let annots_ref_id: Option<lopdf::ObjectId> = {
        let page_dict = doc.get_dictionary(page_id).map_err(|e| e.to_string())?;
        page_dict
            .get(b"Annots")
            .ok()
            .and_then(|o| o.as_reference().ok())
    };

    if let Some(arr_id) = annots_ref_id {
        // /Annots is an indirect reference — mutate the referenced array.
        let arr_obj = doc.get_object_mut(arr_id).map_err(|e| e.to_string())?;
        if let Object::Array(ref mut a) = arr_obj {
            a.push(Object::Reference(annot_id));
        }
    } else {
        // /Annots is a direct array or absent.
        let page_dict = doc.get_dictionary_mut(page_id).map_err(|e| e.to_string())?;
        if page_dict.has(b"Annots") {
            let existing = page_dict.get(b"Annots").map_err(|e| e.to_string())?.clone();
            if let Object::Array(mut arr) = existing {
                arr.push(Object::Reference(annot_id));
                page_dict.set("Annots", Object::Array(arr));
            }
        } else {
            page_dict.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
        }
    }

    Ok(())
}

// ---- Signature verification ----------------------------------------------

/// Number of signature fields in the document, or -1 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_signature_count(doc: *const PdfDocument) -> i32 {
    if doc.is_null() {
        error::set_last_error_str("null document pointer");
        return -1;
    }
    let pdf = unsafe { &*doc }.0.pdf();
    pdf_sign::validate_signatures(pdf).len() as i32
}

/// Validate the signature at zero-based `index`.
///
/// Returns 1 if the signature is cryptographically valid, 0 if invalid,
/// -1 if the status is unknown or on error (null doc, out-of-range index).
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_signature_is_valid(doc: *const PdfDocument, index: i32) -> i32 {
    if doc.is_null() || index < 0 {
        error::set_last_error_str("invalid argument");
        return -1;
    }
    let pdf = unsafe { &*doc }.0.pdf();
    let results = pdf_sign::validate_signatures(pdf);
    let idx = index as usize;
    if idx >= results.len() {
        error::set_last_error_str("signature index out of range");
        return -1;
    }
    match results[idx].status {
        pdf_sign::ValidationStatus::Valid => 1,
        pdf_sign::ValidationStatus::Invalid(_) => 0,
        pdf_sign::ValidationStatus::Unknown(_) => -1,
    }
}

// ---- Image extraction ----------------------------------------------------

/// Number of images on a page, or -1 on error.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*`, or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_image_count(doc: *const PdfDocument, page_index: i32) -> i32 {
    if doc.is_null() || page_index < 0 {
        error::set_last_error_str("invalid argument");
        return -1;
    }
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return -1;
        }
    };
    match pdf_extract::images::extract_page_images(&lopdf_doc, page_index as u32 + 1) {
        Ok(imgs) => imgs.len() as i32,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            -1
        }
    }
}

/// Extract raw image bytes for the image at `image_index` on `page_index`.
///
/// On success writes the image dimensions to `out_width`/`out_height` and
/// a heap-allocated byte buffer to `*out_data` with its length in `*out_len`.
/// Free the buffer with `pdf_bytes_free(*out_data, *out_len)`.
///
/// # Safety
/// `doc` must be valid. All output pointers must be non-null and writable.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_extract_image(
    doc: *const PdfDocument,
    page_index: i32,
    image_index: i32,
    out_width: *mut u32,
    out_height: *mut u32,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if doc.is_null()
        || page_index < 0
        || image_index < 0
        || out_width.is_null()
        || out_height.is_null()
        || out_data.is_null()
        || out_len.is_null()
    {
        error::set_last_error_str("null pointer or invalid argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    let imgs = match pdf_extract::images::extract_page_images(&lopdf_doc, page_index as u32 + 1) {
        Ok(v) => v,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorExtract;
        }
    };
    let idx = image_index as usize;
    if idx >= imgs.len() {
        error::set_last_error_str("image index out of range");
        return PdfStatus::ErrorInvalidArgument;
    }
    let img = &imgs[idx];
    unsafe {
        *out_width = img.width;
        *out_height = img.height;
    }
    let mut data = img.data.clone().into_boxed_slice();
    let len = data.len();
    unsafe {
        *out_data = data.as_mut_ptr();
        *out_len = len;
    }
    std::mem::forget(data);
    PdfStatus::Ok
}

/// Free a byte buffer returned by `pdf_page_extract_image`. Null is safe (no-op).
///
/// # Safety
/// `data` must have been returned by `pdf_page_extract_image` with matching `len`, or be null.
#[no_mangle]
pub unsafe extern "C" fn pdf_bytes_free(data: *mut u8, len: usize) {
    if !data.is_null() && len > 0 {
        drop(unsafe { Vec::from_raw_parts(data, len, len) });
    }
}

// ---- Text search ---------------------------------------------------------

/// Count total occurrences of `query` across all pages. Returns -1 on error.
///
/// # Safety
/// `doc` and `query` must be valid pointers. `query` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_search_count(
    doc: *const PdfDocument,
    query: *const c_char,
) -> i32 {
    if doc.is_null() || query.is_null() {
        error::set_last_error_str("null pointer argument");
        return -1;
    }
    let q = match unsafe { CStr::from_ptr(query) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error_str("invalid UTF-8 in query");
            return -1;
        }
    };
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return -1;
        }
    };
    let opts = pdf_extract::search::SearchOptions::default();
    let results = pdf_extract::search::search_text(&lopdf_doc, q, &opts);
    results.len() as i32
}

// ---- Document split ------------------------------------------------------

/// Extract pages `from_page..=to_page` (0-based, inclusive) into a new document.
///
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc` and `out` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_split_range(
    doc: *const PdfDocument,
    from_page: i32,
    to_page: i32,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || out.is_null() || from_page < 0 || to_page < from_page {
        error::set_last_error_str("invalid argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    // extract_pages takes 1-based page numbers.
    let pages: Vec<u32> = (from_page as u32..=to_page as u32).map(|p| p + 1).collect();
    let mut extracted = match pdf_manip::pages::extract_pages(&lopdf_doc, &pages) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorSplit;
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = extracted.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorSplit;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
            PdfStatus::ErrorSplit
        }
    }
}

// ---- Watermark -----------------------------------------------------------

/// Apply a diagonal text watermark to all pages and return a new document.
///
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc`, `text`, and `out` must be valid non-null pointers.
/// `text` must be a null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_add_watermark(
    doc: *const PdfDocument,
    text: *const c_char,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || text.is_null() || out.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let text_str = match unsafe { CStr::from_ptr(text) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error_str("invalid UTF-8 in text");
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    let watermark = pdf_manip::watermark::TextWatermark {
        text: text_str.to_string(),
        font_size: 60.0,
        rotation: 45.0,
        opacity: 0.25,
        color: pdf_manip::watermark::Color::Rgb(0.5, 0.5, 0.5),
        position: pdf_manip::watermark::Position::Center,
        layer: pdf_manip::watermark::Layer::Foreground,
    };
    if let Err(e) = pdf_manip::watermark::apply_text_watermark(
        &mut lopdf_doc,
        &watermark,
        &pdf_manip::watermark::PageSelection::All,
    ) {
        error::set_last_error_str(&e.to_string());
        return PdfStatus::ErrorWatermark;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorWatermark;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
            PdfStatus::ErrorWatermark
        }
    }
}

// ---- Compression ---------------------------------------------------------

/// Compress stream objects in the document and return a new (smaller) document.
///
/// The caller owns the returned document and must free it with `pdf_document_free`.
///
/// # Safety
/// `doc` and `out` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_document_compress(
    doc: *const PdfDocument,
    out: *mut *mut PdfDocument,
) -> PdfStatus {
    if doc.is_null() || out.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let raw = unsafe { &*doc }.0.pdf().data().as_ref().to_vec();
    let mut lopdf_doc = match lopdf::Document::load_mem(&raw) {
        Ok(d) => d,
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            return PdfStatus::ErrorCorruptPdf;
        }
    };
    if let Err(e) = pdf_manip::optimize::compress_streams(&mut lopdf_doc) {
        error::set_last_error_str(&e.to_string());
        return PdfStatus::ErrorCompress;
    }
    let mut buf = Vec::new();
    if let Err(e) = lopdf_doc.save_to(&mut buf) {
        error::set_last_error_str(&format!("save: {e}"));
        return PdfStatus::ErrorCompress;
    }
    match pdf_engine::PdfDocument::open(buf) {
        Ok(new_doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfDocument(new_doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error(&e);
            PdfStatus::ErrorCompress
        }
    }
}

// ---- Structured text-block extraction -----------------------------------

/// A single text block with bounding-box coordinates in PDF user space.
///
/// Memory ownership: every `PdfTextBlock*` returned by
/// `pdf_page_extract_text_blocks` is part of one heap allocation that the
/// caller MUST release via `pdf_text_blocks_free`. The embedded
/// `const char* text` pointers point into a parallel Rust-owned
/// `Vec<CString>` and are freed together with the block array. Do NOT
/// `free()` individual `text` pointers; do NOT mix allocators.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PdfTextBlock {
    /// PDF user-space X of the block's bottom-left corner (1/72 inch).
    pub x: f64,
    /// PDF user-space Y of the block's bottom-left corner (1/72 inch).
    pub y: f64,
    /// Block width in PDF points. Always non-negative; 0 for empty blocks.
    pub width: f64,
    /// Block height in PDF points. Always non-negative; 0 for empty blocks.
    pub height: f64,
    /// UTF-8, null-terminated text content. Lifetime is tied to the
    /// containing array; do not free individually.
    pub text: *const c_char,
}

// SAFETY: The struct is plain old data plus an immutable C-string
// pointer. The free function below is the only thing that mutates the
// pointer state, and that happens after the C consumer has handed
// ownership back to Rust.
unsafe impl Send for PdfTextBlock {}
unsafe impl Sync for PdfTextBlock {}

/// Backing storage for a `PdfTextBlock*` array. Kept private so the C
/// side never sees it; `Box::into_raw`+`Box::from_raw` round-trip the
/// pointer when freeing.
struct TextBlockBacking {
    blocks: Box<[PdfTextBlock]>,
    // The CStrings own the byte buffers the `text` ptrs point into.
    // Dropping this Vec frees them; we keep it alive for the full life
    // of `blocks`.
    _texts: Vec<std::ffi::CString>,
}

/// Extract structured text blocks from a single page.
///
/// On success: `*out_blocks` is set to a heap-allocated array of
/// `PdfTextBlock` (or `NULL` if the page has zero blocks), `*out_count`
/// holds the number of blocks. The caller MUST release the array via
/// `pdf_text_blocks_free(*out_blocks, *out_count)` when finished.
///
/// On failure: `*out_blocks` is set to `NULL`, `*out_count` to `0`, and
/// the function returns a non-`Ok` status. The error message is
/// available via `pdf_get_last_error()`.
///
/// # Safety
/// `doc` must be a valid pointer returned by `pdf_document_open*` or
/// null. `out_blocks` and `out_count` must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_page_extract_text_blocks(
    doc: *const PdfDocument,
    page_index: i32,
    out_blocks: *mut *mut PdfTextBlock,
    out_count: *mut usize,
) -> PdfStatus {
    // Defensive: even if we early-return, callers benefit from a
    // consistent (NULL, 0) post-state.
    if !out_blocks.is_null() {
        unsafe { *out_blocks = ptr::null_mut() };
    }
    if !out_count.is_null() {
        unsafe { *out_count = 0 };
    }
    if doc.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    if out_blocks.is_null() || out_count.is_null() {
        error::set_last_error_str("null out_blocks or out_count");
        return PdfStatus::ErrorInvalidArgument;
    }
    if page_index < 0 {
        error::set_last_error_str("page_index must be >= 0");
        return PdfStatus::ErrorInvalidArgument;
    }

    let document = unsafe { &(*doc).0 };
    let page_count = document.page_count();
    if (page_index as usize) >= page_count {
        error::set_last_error_str(&format!(
            "page index {page_index} out of range (0..{page_count})"
        ));
        return PdfStatus::ErrorPageRange;
    }

    let engine_blocks = match document.extract_text_blocks(page_index as usize) {
        Ok(b) => b,
        Err(e) => {
            error::set_last_error_str(&format!("text-block extraction failed: {e}"));
            return PdfStatus::ErrorExtract;
        }
    };

    // Aggregate every block's bbox and turn its text into a CString.
    let mut blocks: Vec<PdfTextBlock> = Vec::with_capacity(engine_blocks.len());
    let mut texts: Vec<std::ffi::CString> = Vec::with_capacity(engine_blocks.len());
    for eb in engine_blocks {
        let (mut x_min, mut y_min) = (f64::INFINITY, f64::INFINITY);
        let (mut x_max, mut y_max) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for span in &eb.spans {
            if span.x < x_min {
                x_min = span.x;
            }
            if span.y < y_min {
                y_min = span.y;
            }
            let right = span.x + span.width;
            let top = span.y + span.height;
            if right > x_max {
                x_max = right;
            }
            if top > y_max {
                y_max = top;
            }
        }
        if !x_min.is_finite() {
            x_min = 0.0;
            y_min = 0.0;
            x_max = 0.0;
            y_max = 0.0;
        }
        let cstr = match std::ffi::CString::new(eb.text()) {
            Ok(c) => c,
            Err(_) => {
                error::set_last_error_str("text contains interior null byte");
                // We have not yet allocated any backing for the C consumer
                // (out_blocks is still NULL), so the partial vecs simply
                // drop here.
                return PdfStatus::ErrorExtract;
            }
        };
        let text_ptr = cstr.as_ptr();
        texts.push(cstr);
        blocks.push(PdfTextBlock {
            x: x_min,
            y: y_min,
            width: (x_max - x_min).max(0.0),
            height: (y_max - y_min).max(0.0),
            text: text_ptr,
        });
    }

    if blocks.is_empty() {
        // Empty page → leave (NULL, 0) which we already wrote above.
        return PdfStatus::Ok;
    }

    let count = blocks.len();
    let backing = Box::new(TextBlockBacking {
        blocks: blocks.into_boxed_slice(),
        _texts: texts,
    });

    // Hand the array pointer to the C caller; keep the backing alive
    // via Box::into_raw so it survives until pdf_text_blocks_free.
    let backing_ptr = Box::into_raw(backing);
    // SAFETY: TextBlockBacking { blocks: Box<[..]>, _texts: .. } — the
    // `blocks` field's first element address equals .as_ptr().
    let array_ptr = unsafe { (*backing_ptr).blocks.as_ptr() } as *mut PdfTextBlock;
    // Stash backing_ptr so free can recover it. We encode the backing
    // pointer in a side-table keyed by array_ptr.
    register_backing(array_ptr, backing_ptr);
    unsafe {
        *out_blocks = array_ptr;
        *out_count = count;
    }
    PdfStatus::Ok
}

/// Free an array of `PdfTextBlock`s returned by
/// `pdf_page_extract_text_blocks`.
///
/// `pdf_text_blocks_free(NULL, 0)` is a no-op. The `count` argument MUST
/// match the value `pdf_page_extract_text_blocks` wrote into
/// `*out_count` — passing a different value is undefined behaviour.
///
/// # Safety
/// `blocks` must be either null or a pointer previously returned by
/// `pdf_page_extract_text_blocks` that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_blocks_free(blocks: *mut PdfTextBlock, _count: usize) {
    if blocks.is_null() {
        return;
    }
    if let Some(backing_ptr) = take_backing(blocks) {
        // SAFETY: backing_ptr came from Box::into_raw and we only ever
        // call take_backing exactly once per registration.
        drop(unsafe { Box::from_raw(backing_ptr) });
    }
    // If take_backing returned None the pointer wasn't ours — silently
    // ignore (safer than aborting on a double-free).
}

// ---------------------------------------------------------------------------
// Side-table to recover the backing TextBlockBacking from the array
// pointer the C consumer holds. Locked with a Mutex; entries are
// inserted on extraction and removed on free.
// ---------------------------------------------------------------------------

use std::sync::Mutex;
use std::sync::OnceLock;

fn backing_table() -> &'static Mutex<std::collections::HashMap<usize, usize>> {
    static TABLE: OnceLock<Mutex<std::collections::HashMap<usize, usize>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn register_backing(array_ptr: *mut PdfTextBlock, backing_ptr: *mut TextBlockBacking) {
    if let Ok(mut g) = backing_table().lock() {
        g.insert(array_ptr as usize, backing_ptr as usize);
    }
}

fn take_backing(array_ptr: *mut PdfTextBlock) -> Option<*mut TextBlockBacking> {
    let mut g = backing_table().lock().ok()?;
    g.remove(&(array_ptr as usize))
        .map(|v| v as *mut TextBlockBacking)
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
        assert_eq!(PdfStatus::ErrorAnnotation as i32, 10);
        assert_eq!(PdfStatus::ErrorMerge as i32, 11);
        assert_eq!(PdfStatus::ErrorUnknown as i32, 99);
    }

    #[test]
    fn annotation_null_doc() {
        assert_eq!(unsafe { pdf_annotation_count(ptr::null(), 0) }, -1);
        assert!(unsafe { pdf_annotation_type(ptr::null(), 0, 0) }.is_null());
    }

    #[test]
    fn merge_invalid_args() {
        let mut out: *mut PdfDocument = ptr::null_mut();
        // null docs pointer
        let s = unsafe { pdf_documents_merge(ptr::null(), 2, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
        // zero count
        let docs: [*const PdfDocument; 0] = [];
        let s = unsafe { pdf_documents_merge(docs.as_ptr(), 0, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn bookmark_null() {
        assert_eq!(unsafe { pdf_bookmark_count(ptr::null()) }, 0);
    }

    #[test]
    fn meta_null() {
        let key = c"Title".as_ptr();
        assert!(unsafe { pdf_document_get_meta(ptr::null(), key) }.is_null());
    }

    #[test]
    fn signature_count_null() {
        assert_eq!(unsafe { pdf_signature_count(ptr::null()) }, -1);
    }

    #[test]
    fn signature_is_valid_null() {
        assert_eq!(unsafe { pdf_signature_is_valid(ptr::null(), 0) }, -1);
        // negative index
        let mut out: *mut PdfDocument = ptr::null_mut();
        let data = b"not a pdf";
        let _ = unsafe { pdf_document_open_from_bytes(data.as_ptr(), data.len(), &mut out) };
        // out is still null because the open failed
        assert_eq!(unsafe { pdf_signature_is_valid(ptr::null(), -1) }, -1);
    }

    #[test]
    fn image_count_null() {
        assert_eq!(unsafe { pdf_page_image_count(ptr::null(), 0) }, -1);
        assert_eq!(unsafe { pdf_page_image_count(ptr::null(), -1) }, -1);
    }

    #[test]
    fn search_count_null() {
        let q = c"test".as_ptr();
        assert_eq!(unsafe { pdf_document_search_count(ptr::null(), q) }, -1);
        assert_eq!(
            unsafe { pdf_document_search_count(ptr::null(), ptr::null()) },
            -1
        );
    }

    #[test]
    fn split_range_null() {
        let mut out: *mut PdfDocument = ptr::null_mut();
        let s = unsafe { pdf_document_split_range(ptr::null(), 0, 0, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn split_range_invalid_pages() {
        let mut out: *mut PdfDocument = ptr::null_mut();
        // to_page < from_page
        let s = unsafe { pdf_document_split_range(ptr::null(), 5, 2, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
        // negative from_page
        let s = unsafe { pdf_document_split_range(ptr::null(), -1, 0, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn watermark_null() {
        let text = c"DRAFT".as_ptr();
        let mut out: *mut PdfDocument = ptr::null_mut();
        let s = unsafe { pdf_document_add_watermark(ptr::null(), text, &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn compress_null() {
        let mut out: *mut PdfDocument = ptr::null_mut();
        let s = unsafe { pdf_document_compress(ptr::null(), &mut out) };
        assert_eq!(s, PdfStatus::ErrorInvalidArgument);
    }

    #[test]
    fn bytes_free_null_safe() {
        unsafe { pdf_bytes_free(ptr::null_mut(), 0) };
        unsafe { pdf_bytes_free(ptr::null_mut(), 42) };
    }

    #[test]
    fn new_status_codes() {
        assert_eq!(PdfStatus::ErrorExtract as i32, 12);
        assert_eq!(PdfStatus::ErrorSplit as i32, 13);
        assert_eq!(PdfStatus::ErrorWatermark as i32, 14);
        assert_eq!(PdfStatus::ErrorCompress as i32, 15);
    }
}
