//! Thread-local error state for the C API.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Store an error message in the thread-local error buffer.
pub(crate) fn set_last_error(msg: &str) {
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
