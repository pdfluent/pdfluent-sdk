//! Layout-aware text replacement over the C ABI.
//!
//! The editor handle owns a [`pdfluent::PdfDocument`] rather than a
//! `pdf_engine::PdfDocument`, so licensing and the Trial-tier notice apply the
//! same way they do in the Rust SDK and the other bindings. This is a separate
//! handle from [`crate::types::PdfDocument`] on purpose: that one is the
//! read/render handle and is not licensed.
//!
//! Structured values cross the boundary as JSON strings — the same shapes the
//! WASM and Node bindings use — because a C ABI cannot carry the nested match
//! and report structures without inventing a struct zoo that would then need
//! versioning. Every returned string is owned by the caller and must be freed
//! with [`crate::pdf_string_free`].
//!
//! Match ids are opaque strings: find them now, hand them to a translation
//! service, apply them later.

use std::ffi::{c_char, CStr, CString};

use pdfluent::text_edit::{
    CommitPolicy, FontFallback, MatchId, RegionRelation, ReplaceOptions, SignaturePolicy, TextQuery,
};

use crate::error;
use crate::types::PdfStatus;

/// Opaque handle to a licensed text-editing session over one document.
pub struct PdfTextEditor(pdfluent::PdfDocument);

/// Read a C string argument, or record an error and return `None`.
unsafe fn cstr_arg(ptr: *const c_char, what: &str) -> Option<String> {
    if ptr.is_null() {
        error::set_last_error_str(&format!("null pointer argument: {what}"));
        return None;
    }
    match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            error::set_last_error_str(&format!("invalid UTF-8 in {what}"));
            None
        }
    }
}

/// Hand a Rust string to the caller as an owned C string.
fn out_string(value: String, out: *mut *mut c_char) -> PdfStatus {
    match CString::new(value) {
        Ok(c) => {
            unsafe { *out = c.into_raw() };
            PdfStatus::Ok
        }
        Err(_) => {
            error::set_last_error_str("result contained an interior NUL byte");
            PdfStatus::ErrorTextEdit
        }
    }
}

/// Open a document for text editing, from raw bytes.
///
/// # Safety
/// `data` must point to `len` readable bytes. `out` must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_open_from_bytes(
    data: *const u8,
    len: usize,
    out: *mut *mut PdfTextEditor,
) -> PdfStatus {
    if data.is_null() || out.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    match pdfluent::PdfDocument::from_bytes(bytes) {
        Ok(doc) => {
            unsafe { *out = Box::into_raw(Box::new(PdfTextEditor(doc))) };
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            PdfStatus::ErrorCorruptPdf
        }
    }
}

/// Free a text-editor handle.
///
/// # Safety
/// `editor` must come from `pdf_text_editor_open_from_bytes` and must not be
/// used afterwards. Passing null is allowed and does nothing.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_free(editor: *mut PdfTextEditor) {
    if !editor.is_null() {
        drop(unsafe { Box::from_raw(editor) });
    }
}

/// Number of pages, or 0 when `editor` is null.
///
/// # Safety
/// `editor` must be a live handle or null.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_page_count(editor: *const PdfTextEditor) -> usize {
    match unsafe { editor.as_ref() } {
        Some(e) => e.0.page_count(),
        None => 0,
    }
}

/// Find text occurrences. `query_json` uses the shape documented on the WASM
/// and Node bindings. On success `*out_json` receives a JSON array of matches,
/// owned by the caller — free it with `pdf_string_free`.
///
/// # Safety
/// All pointers must be valid; `query_json` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_find(
    editor: *mut PdfTextEditor,
    query_json: *const c_char,
    out_json: *mut *mut c_char,
) -> PdfStatus {
    if editor.is_null() || out_json.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let Some(json) = (unsafe { cstr_arg(query_json, "query_json") }) else {
        return PdfStatus::ErrorInvalidArgument;
    };
    let query = match parse_query(&json) {
        Ok(q) => q,
        Err(e) => {
            error::set_last_error_str(&e);
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let doc = unsafe { &mut (*editor).0 };
    match doc.find_text(query) {
        Ok(matches) => match serde_json::to_string(&matches) {
            Ok(s) => out_string(s, out_json),
            Err(e) => {
                error::set_last_error_str(&e.to_string());
                PdfStatus::ErrorTextEdit
            }
        },
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            PdfStatus::ErrorTextEdit
        }
    }
}

/// Find and replace in one call. `*out_json` receives the JSON report, owned
/// by the caller — free it with `pdf_string_free`.
///
/// # Safety
/// All pointers must be valid; the string arguments must be null-terminated
/// UTF-8. `options_json` may be `"{}"`.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_replace(
    editor: *mut PdfTextEditor,
    query_json: *const c_char,
    replacement: *const c_char,
    options_json: *const c_char,
    out_json: *mut *mut c_char,
) -> PdfStatus {
    if editor.is_null() || out_json.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let (Some(qjson), Some(repl), Some(ojson)) = (unsafe {
        (
            cstr_arg(query_json, "query_json"),
            cstr_arg(replacement, "replacement"),
            cstr_arg(options_json, "options_json"),
        )
    }) else {
        return PdfStatus::ErrorInvalidArgument;
    };
    let (query, options) = match (parse_query(&qjson), parse_options(&ojson)) {
        (Ok(q), Ok(o)) => (q, o),
        (Err(e), _) | (_, Err(e)) => {
            error::set_last_error_str(&e);
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let doc = unsafe { &mut (*editor).0 };
    match doc.replace_text(query, &repl, options) {
        Ok(report) => match serde_json::to_string(&report) {
            Ok(s) => out_string(s, out_json),
            Err(e) => {
                error::set_last_error_str(&e.to_string());
                PdfStatus::ErrorTextEdit
            }
        },
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            PdfStatus::ErrorTextEdit
        }
    }
}

/// Apply replacements to matches found earlier. `edits_json` is a JSON array
/// of `{"id": "<match id>", "text": "<new text>"}`. They commit as one
/// transaction. `*out_json` receives the JSON report, owned by the caller.
///
/// # Safety
/// All pointers must be valid; the string arguments must be null-terminated
/// UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_replace_matches(
    editor: *mut PdfTextEditor,
    edits_json: *const c_char,
    options_json: *const c_char,
    out_json: *mut *mut c_char,
) -> PdfStatus {
    #[derive(serde::Deserialize)]
    struct EditIn {
        id: String,
        text: String,
    }

    if editor.is_null() || out_json.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let (Some(ejson), Some(ojson)) = (unsafe {
        (
            cstr_arg(edits_json, "edits_json"),
            cstr_arg(options_json, "options_json"),
        )
    }) else {
        return PdfStatus::ErrorInvalidArgument;
    };
    let edits: Vec<EditIn> = match serde_json::from_str(&ejson) {
        Ok(v) => v,
        Err(e) => {
            error::set_last_error_str(&format!("expected [{{id, text}}, …]: {e}"));
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let options = match parse_options(&ojson) {
        Ok(o) => o,
        Err(e) => {
            error::set_last_error_str(&e);
            return PdfStatus::ErrorInvalidArgument;
        }
    };
    let edits: Vec<(MatchId, String)> = edits
        .into_iter()
        .map(|e| (MatchId::from_token(e.id), e.text))
        .collect();
    let doc = unsafe { &mut (*editor).0 };
    match doc.replace_text_matches(&edits, options) {
        Ok(report) => match serde_json::to_string(&report) {
            Ok(s) => out_string(s, out_json),
            Err(e) => {
                error::set_last_error_str(&e.to_string());
                PdfStatus::ErrorTextEdit
            }
        },
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            PdfStatus::ErrorTextEdit
        }
    }
}

/// Serialize the (possibly edited) document. On success `*out_data` receives a
/// buffer of `*out_len` bytes, owned by the caller — free it with
/// `pdf_text_editor_bytes_free`.
///
/// # Safety
/// All pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_to_bytes(
    editor: *const PdfTextEditor,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> PdfStatus {
    if editor.is_null() || out_data.is_null() || out_len.is_null() {
        error::set_last_error_str("null pointer argument");
        return PdfStatus::ErrorInvalidArgument;
    }
    let doc = unsafe { &(*editor).0 };
    match doc.to_bytes() {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            unsafe {
                *out_len = boxed.len();
                *out_data = boxed.as_mut_ptr();
            }
            std::mem::forget(boxed);
            PdfStatus::Ok
        }
        Err(e) => {
            error::set_last_error_str(&e.to_string());
            PdfStatus::ErrorTextEdit
        }
    }
}

/// Free a buffer returned by `pdf_text_editor_to_bytes`.
///
/// # Safety
/// `data`/`len` must be exactly what `pdf_text_editor_to_bytes` produced.
#[no_mangle]
pub unsafe extern "C" fn pdf_text_editor_bytes_free(data: *mut u8, len: usize) {
    if !data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) });
    }
}

// ---------------------------------------------------------------------------
// JSON input parsing (same shapes as the WASM and Node bindings)
// ---------------------------------------------------------------------------

fn parse_query(json: &str) -> Result<TextQuery, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RegionIn {
        page: u32,
        rect: [f64; 4],
        #[serde(default)]
        relation: Option<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct QueryIn {
        text: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        pages: Option<[u32; 2]>,
        #[serde(default)]
        region: Option<RegionIn>,
        #[serde(default)]
        limit: Option<u32>,
    }

    let q: QueryIn = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut query = TextQuery::exact(q.text).case_insensitive(q.case_insensitive);
    if let Some([lo, hi]) = q.pages {
        query = query.pages(lo..=hi);
    }
    if let Some(region) = q.region {
        let relation = match region.relation.as_deref() {
            None | Some("intersects") => RegionRelation::Intersects,
            Some("contained") => RegionRelation::Contained,
            Some(other) => return Err(format!("unknown region relation {other:?}")),
        };
        query = query.region_with(region.page, region.rect, relation);
    }
    if let Some(n) = q.limit {
        query = query.limit(n as usize);
    }
    Ok(query)
}

fn parse_options(json: &str) -> Result<ReplaceOptions, String> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum FallbackIn {
        Named(String),
        Explicit { explicit: String },
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct OptionsIn {
        #[serde(default)]
        font_fallback: Option<FallbackIn>,
        #[serde(default)]
        signature_policy: Option<String>,
        #[serde(default)]
        commit_policy: Option<String>,
    }

    let trimmed = json.trim();
    let o: OptionsIn = if trimmed.is_empty() {
        OptionsIn::default()
    } else {
        serde_json::from_str(trimmed).map_err(|e| e.to_string())?
    };

    let mut options = ReplaceOptions::default();
    match o.font_fallback {
        None => {}
        Some(FallbackIn::Named(name)) => match name.as_str() {
            "deny" => options.font_fallback = FontFallback::Deny,
            "injectStandard" => options.font_fallback = FontFallback::InjectStandard,
            other => return Err(format!("unknown fontFallback {other:?}")),
        },
        Some(FallbackIn::Explicit { explicit }) => {
            options.font_fallback = FontFallback::Explicit(explicit);
        }
    }
    match o.signature_policy.as_deref() {
        None | Some("reject") => {}
        Some("allowPostSignatureChange") => {
            options.signature_policy = SignaturePolicy::AllowPostSignatureChange;
        }
        Some(other) => return Err(format!("unknown signaturePolicy {other:?}")),
    }
    match o.commit_policy.as_deref() {
        None | Some("allOrNothing") => {}
        Some("bestEffort") => options.commit_policy = CommitPolicy::BestEffort,
        Some(other) => return Err(format!("unknown commitPolicy {other:?}")),
    }
    Ok(options)
}
