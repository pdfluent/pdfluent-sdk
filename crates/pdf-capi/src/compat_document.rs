//! Compatible documents: open, save, outline, and object access.
//!
//! Generated from the member table; one `pdf_compat_*` wrapper per bound spec
//! member. See `compat.rs` for the conventions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::compat::{
    fail_arg, fail_page_range, guard, optional_bytes, optional_rect, optional_str, out_bytes,
    out_handle, out_string, required_doc, required_doc_mut, required_str, PdfCompatDocument,
    PdfCompatRect, PdfStatus,
};
use super::compat_handles::PdfCompatTocBuilder;
use super::compat_lists::{
    PdfCompatFontData, PdfCompatFontInfoList, PdfCompatImageData, PdfCompatImageRefList,
    PdfCompatTocList,
};
use super::compat_opts::{optional_json, save_settings_from_json, value_from_json};
use std::ffi::CString;
use std::os::raw::c_char;

/// Open a file, bytes, or a fresh shell.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_open(
    filename: *const c_char,
    data: *const u8,
    data_len: usize,
    filetype: *const c_char,
    rect: *const PdfCompatRect,
    width: f64,
    height: f64,
    fontsize: f64,
    archive: *const c_char,
    out: *mut *mut PdfCompatDocument,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let filename = match optional_str(filename) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let data = match optional_bytes(data, data_len) {
        Ok(Some(b)) => Some(b.to_vec()),
        Ok(None) => None,
        Err(s) => return s,
    };
    let filetype = match optional_str(filetype) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let rect = unsafe { optional_rect(rect) };
    let archive = match optional_json(archive) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let mut opts = pdfluent::compat::DocumentOpenOptions::default();
    opts.filename = filename.map(str::to_owned);
    opts.data = data;
    opts.filetype = filetype.map(str::to_owned);
    opts.rect = rect;
    opts.width = width;
    opts.height = height;
    opts.fontsize = fontsize;
    if let Some(a) = archive {
        match value_from_json(&a) {
            Ok(v) => opts.archive = Some(v),
            Err(d) => return fail_arg(&format!("invalid archive argument: {d}")),
        }
    }
    match guard(false, || pdfluent::compat::Document::open_compat(&opts)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatDocument(v)) },
        Err(status) => status,
    }
}

/// Unlock with a password.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_authenticate(
    doc: *const PdfCompatDocument,
    password: *const c_char,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let password = match required_str(password) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.authenticate(password)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Flatten annotations and widgets into content.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_bake(
    doc: *mut PdfCompatDocument,
    annots: i32,
    widgets: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let annots = annots != 0;
    let widgets = widgets != 0;
    let mut options = pdfluent::compat::BakeOptions::default();
    options.annots = annots;
    options.widgets = widgets;
    match guard(false, || doc.bake(&options)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Close the document and free its handle.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_close(doc: *mut PdfCompatDocument) -> PdfStatus {
    if doc.is_null() {
        return fail_arg("null pointer argument");
    }
    match guard(false, || {
        let owned = unsafe { *Box::from_raw(doc) };
        owned.0.close();
        Ok(())
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Convert a page span to PDF content.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_convert_to_pdf(
    doc: *mut PdfCompatDocument,
    from_page: i32,
    to_page: i32,
    rotate: i32,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if out_data.is_null() {
        return fail_arg("null pointer argument");
    }
    if out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.convert_to_pdf(from_page, to_page, rotate)) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}

/// Font bytes and facts.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_extract_font(
    doc: *const PdfCompatDocument,
    xref: i32,
    info_only: i32,
    name: *const c_char,
    out: *mut *mut PdfCompatFontData,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let info_only = info_only != 0;
    let name = match optional_str(name) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.extract_font(xref, info_only, name)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatFontData(v)) },
        Err(status) => status,
    }
}

/// Image bytes and facts for an object number.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_extract_image(
    doc: *const PdfCompatDocument,
    xref: i32,
    out: *mut *mut PdfCompatImageData,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.extract_image(xref)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatImageData(v)) },
        Err(status) => status,
    }
}

/// Reserve a fresh object number.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_get_new_xref(
    doc: *mut PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.get_new_xref()) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Fonts referenced by a page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_get_page_fonts(
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
    match guard(true, || doc.get_page_fonts(page, full)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatFontInfoList(v)) },
        Err(status) => status,
    }
}

/// Images referenced by a page.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_get_page_images(
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
    match guard(true, || doc.get_page_images(page, full)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatImageRefList(v)) },
        Err(status) => status,
    }
}

/// Outline entries.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_get_toc(
    doc: *const PdfCompatDocument,
    simple: i32,
    out: *mut *mut PdfCompatTocList,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let simple = simple != 0;
    match guard(false, || doc.get_toc(simple)) {
        Ok(v) => unsafe { out_handle(out, PdfCompatTocList(v)) },
        Err(status) => status,
    }
}

/// Insert pages from another document.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_insert_pdf(
    doc: *mut PdfCompatDocument,
    source: *const PdfCompatDocument,
    from_page: i32,
    to_page: i32,
    start_at: i32,
    rotate: i32,
    links: i32,
    annots: i32,
    widgets: i32,
    join_duplicates: i32,
    show_progress: i32,
    finish: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let source = match required_doc(source) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let links = links != 0;
    let annots = annots != 0;
    let widgets = widgets != 0;
    let join_duplicates = join_duplicates != 0;
    let show_progress = show_progress != 0;
    let finish = finish != 0;
    let mut options = pdfluent::compat::InsertPdfOptions::default();
    options.from_page = from_page;
    options.to_page = to_page;
    options.start_at = start_at;
    options.rotate = rotate;
    options.links = links;
    options.annots = annots;
    options.widgets = widgets;
    options.join_duplicates = join_duplicates;
    options.show_progress = show_progress;
    options.finish = finish;
    match guard(false, || doc.insert_pdf(source, &options)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Whether this is a form PDF.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_is_form_pdf(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.is_form_pdf()) {
        Ok(v) => {
            unsafe { *out = i32::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether this is a PDF document.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_is_pdf(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.is_pdf()) {
        Ok(v) => {
            unsafe { *out = i32::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Undo/redo availability of the compatible journal.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_can_do(
    doc: *const PdfCompatDocument,
    can_undo: *mut i32,
    can_redo: *mut i32,
) -> PdfStatus {
    if can_undo.is_null() || can_redo.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_can_do()) {
        Ok((undo, redo)) => {
            unsafe {
                *can_undo = i32::from(undo);
                *can_redo = i32::from(redo);
            }
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Enable the compatible journal.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_enable(
    doc: *mut PdfCompatDocument,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_enable()) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Whether the compatible journal is enabled.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_is_enabled(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_is_enabled()) {
        Ok(v) => {
            unsafe { *out = i32::from(v) };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Load a journal file over a never-enabled journal.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_load(
    doc: *mut PdfCompatDocument,
    path: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let path = match required_str(path) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_load(path)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Journal step name. CALLER FREES with `pdf_string_free`.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_op_name(
    doc: *const PdfCompatDocument,
    step: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_op_name(step)) {
        Ok(v) => unsafe { out_string(out, v) },
        Err(status) => status,
    }
}

/// Journal position: the applied prefix length.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_position(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_position()) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Redo a journal step.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_redo(
    doc: *mut PdfCompatDocument,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_redo()) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Save the journal file.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_save(
    doc: *mut PdfCompatDocument,
    path: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let path = match required_str(path) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_save(path)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Start a journal step; the name may be NULL for an anonymous step.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_start_op(
    doc: *mut PdfCompatDocument,
    name: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let name = match optional_str(name) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_start_op(name)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Stop the journal step.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_stop_op(
    doc: *mut PdfCompatDocument,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_stop_op()) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Undo a journal step.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_journal_undo(
    doc: *mut PdfCompatDocument,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.journal_undo()) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Validate a page number; returns it.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_load_page(
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
    match guard(true, || doc.load_page(page).map(|p| p.index() as i32)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Whether the document needs a password.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_needs_pass(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.needs_pass()) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Insert a fresh page; returns its number.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_new_page(
    doc: *mut PdfCompatDocument,
    at: i32,
    width: f64,
    height: f64,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || {
        doc.new_page(at, width, height).map(|p| p.index() as i32)
    }) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Number of pages. Binds PdfDocument::len; the spec names no page_count (reported).
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_page_count(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || Ok(doc.len() as i32)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// The document catalog.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_pdf_catalog(
    doc: *const PdfCompatDocument,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.pdf_catalog()) {
        Ok(v) => unsafe { out_string(out, v) },
        Err(status) => status,
    }
}

/// Save to a file.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_save(
    doc: *const PdfCompatDocument,
    path: *const c_char,
    settings: *const c_char,
) -> PdfStatus {
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let path = match required_str(path) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let settings = match optional_json(settings) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let settings = match save_settings_from_json(settings.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(false, || doc.save_full(path, &settings)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Save incrementally.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_save_incr(doc: *const PdfCompatDocument) -> PdfStatus {
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.save_incr()) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Replace the document metadata.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_set_metadata(
    doc: *mut PdfCompatDocument,
    title: *const c_char,
    author: *const c_char,
    subject: *const c_char,
    keywords: *const c_char,
    creator: *const c_char,
    producer: *const c_char,
    creation_date: *const c_char,
    mod_date: *const c_char,
    format: *const c_char,
    encryption: *const c_char,
    trapped: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let title = match optional_str(title) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let author = match optional_str(author) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let subject = match optional_str(subject) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let keywords = match optional_str(keywords) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let creator = match optional_str(creator) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let producer = match optional_str(producer) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let creation_date = match optional_str(creation_date) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let mod_date = match optional_str(mod_date) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let format = match optional_str(format) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let encryption = match optional_str(encryption) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let trapped = match optional_str(trapped) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let metadata = pdfluent::compat::DocMetadata {
        title: title.unwrap_or("").to_owned(),
        author: author.unwrap_or("").to_owned(),
        subject: subject.unwrap_or("").to_owned(),
        keywords: keywords.unwrap_or("").to_owned(),
        creator: creator.unwrap_or("").to_owned(),
        producer: producer.unwrap_or("").to_owned(),
        creation_date: creation_date.unwrap_or("").to_owned(),
        mod_date: mod_date.unwrap_or("").to_owned(),
        format: format.unwrap_or("").to_owned(),
        encryption: encryption.map(str::to_owned),
        trapped: trapped.unwrap_or("").to_owned(),
    };
    match guard(false, || doc.set_metadata(&metadata)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Replace the outline; returns its size.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_set_toc(
    doc: *mut PdfCompatDocument,
    toc: *const PdfCompatTocBuilder,
    collapse: i32,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    if toc.is_null() {
        return fail_arg("null pointer argument");
    }
    let toc = unsafe { &*toc };
    match guard(false, || doc.set_toc(&toc.0, collapse)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// Serialise the document to bytes.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_tobytes(
    doc: *const PdfCompatDocument,
    settings: *const c_char,
    out_data: *mut *mut u8,
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
    let settings = match optional_json(settings) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let settings = match save_settings_from_json(settings.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(false, || doc.tobytes(&settings)) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}

/// Replace one raw object.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_update_object(
    doc: *mut PdfCompatDocument,
    xref: i32,
    text: *const c_char,
    page: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let text = match required_str(text) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let page = if page < 0 { None } else { Some(page) };
    match guard(false, || doc.update_object(xref, text, page)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Replace one stream payload.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_update_stream(
    doc: *mut PdfCompatDocument,
    xref: i32,
    data: *const u8,
    data_len: usize,
    is_new: i32,
    compress: i32,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let data = match optional_bytes(data, data_len) {
        Ok(Some(b)) => Some(b.to_vec()),
        Ok(None) => None,
        Err(s) => return s,
    };
    let is_new = is_new != 0;
    let compress = compress != 0;
    match guard(false, || {
        doc.update_stream(xref, data.as_deref(), is_new, compress)
    }) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// Write the document to bytes.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_write(
    doc: *const PdfCompatDocument,
    settings: *const c_char,
    out_data: *mut *mut u8,
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
    let settings = match optional_json(settings) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let settings = match save_settings_from_json(settings.as_ref()) {
        Ok(o) => o,
        Err(s) => return s,
    };
    match guard(false, || doc.write(&settings)) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}

/// One object key: value and type.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_xref_get_key(
    doc: *const PdfCompatDocument,
    xref: i32,
    key: *const c_char,
    out: *mut *mut c_char,
    out2: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    if out2.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let key = match required_str(key) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.xref_get_key(xref, key)) {
        Ok(v) => match (CString::new(v.0), CString::new(v.1)) {
            (Ok(a), Ok(b)) => {
                unsafe {
                    *out = a.into_raw();
                    *out2 = b.into_raw();
                }
                PdfStatus::Ok
            }
            _ => fail_arg("result is not a C string"),
        },
        Err(status) => status,
    }
}

/// Object count.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_xref_length(
    doc: *const PdfCompatDocument,
    out: *mut i32,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    match guard(false, || doc.xref_length().map(|n| n as i32)) {
        Ok(v) => {
            unsafe { *out = v };
            PdfStatus::Ok
        }
        Err(status) => status,
    }
}

/// One raw object.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_xref_object(
    doc: *const PdfCompatDocument,
    xref: i32,
    compressed: i32,
    ascii: i32,
    out: *mut *mut c_char,
) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    let doc = match required_doc(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let compressed = compressed != 0;
    let ascii = ascii != 0;
    match guard(false, || doc.xref_object(xref, compressed, ascii)) {
        Ok(v) => unsafe { out_string(out, v) },
        Err(status) => status,
    }
}

/// Set one object key.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_xref_set_key(
    doc: *mut PdfCompatDocument,
    xref: i32,
    key: *const c_char,
    value: *const c_char,
) -> PdfStatus {
    let doc = match required_doc_mut(doc) {
        Ok(d) => d,
        Err(s) => return s,
    };
    let key = match required_str(key) {
        Ok(s) => s,
        Err(s) => return s,
    };
    let value = match required_str(value) {
        Ok(s) => s,
        Err(s) => return s,
    };
    match guard(false, || doc.xref_set_key(xref, key, value)) {
        Ok(()) => PdfStatus::Ok,
        Err(status) => status,
    }
}

/// One stream payload.
///
/// # Safety
/// All pointers must be valid for their use; see the header docs.
#[no_mangle]
pub unsafe extern "C" fn pdf_compat_document_xref_stream(
    doc: *const PdfCompatDocument,
    xref: i32,
    out_data: *mut *mut u8,
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
    match guard(false, || doc.xref_stream(xref)) {
        Ok(v) => unsafe { out_bytes(out_data, out_len, v) },
        Err(status) => status,
    }
}
