//! Thread-local error state for the C API.

use pdf_engine::api_error::PdfError;
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Format a rich error message with code, help, and docs URL.
fn format_rich_error<E: PdfError>(e: &E) -> String {
    let code = e.code();
    let msg = e.to_string();
    let help = e.help().unwrap_or_default();
    let docs = e.docs_url();
    format!(
        "[{}] {} — Fix: {} — Docs: {}",
        code,
        msg.lines().next().unwrap_or(&msg),
        help.lines().next().unwrap_or(&help),
        docs
    )
}

/// Store an error message in the thread-local error buffer.
pub(crate) fn set_last_error<E: PdfError>(e: &E) {
    let msg = format_rich_error(e);
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(msg).ok();
    });
}

/// Store a plain error message (for errors that don't implement PdfError).
pub(crate) fn set_last_error_str(msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg).ok();
    });
}

/// Get the last error message as a null-terminated string.
///
/// Returns null if no error has been recorded. The pointer is valid until the
/// next API call on the same thread.
///
/// # Safety
/// The returned pointer must not be freed by the caller. It is only valid
/// until the next C API call on the same thread.
#[no_mangle]
pub extern "C" fn pdf_get_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        let borrow = e.borrow();
        match &*borrow {
            Some(cstr) => cstr.as_ptr(),
            None => std::ptr::null(),
        }
    })
}

/// Clear the last error.
#[no_mangle]
pub extern "C" fn pdf_clear_error() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}
