//! Compatible API surface (lane L5): the C projection of `pdfluent::compat`.
//!
//! One `pdf_compat_*` function per bound spec member, following
//! `docs/c_abi_stability.md`: opaque handles, `PdfStatus` returns, and
//! caller-frees-own transfer. Pages, annotations, and shapes are addressed by
//! id (`doc`, `page`, `xref`) rather than by handle because the facade views
//! (`Page`, `PageMut`, `Annot`, `Shape`, `TextPage`) borrow the document; each
//! call rebuilds its view, so no borrowed state crosses the boundary.
//!
//! The implementation lanes are still filling the facade, so every facade
//! call runs inside `catch_unwind`: a stub that panics today becomes
//! `ErrorUnknown` with the panic text in the thread-local error slot instead
//! of aborting the caller. Once a lane lands, the same wrapper returns its
//! real result; the wrappers themselves do not change.
//!
//! Error mapping: `pdfluent::Error::compat_kind` selects the status code and
//! the compat-kind slot read through [`pdf_compat_last_error_kind`]; errors
//! without a kind fall back to the facade variant (`FileNotFound`,
//! `InvalidPdf`, `DecryptionFailed`). Bindings raise their recorded exception
//! classes from the (status, kind) pair.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::ErrorKind;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::error;
pub(crate) use crate::PdfStatus;

// ---- Opaque handles --------------------------------------------------------
// Each handle owns its facade value outright. Document-borrowing views never
// become handles; see the module docs.

/// Owned compatible document: the facade type under its C name.
pub struct PdfCompatDocument(pub(crate) pdfluent::PdfDocument);

/// Owned pixel buffer.
pub struct PdfCompatPixmap(pub(crate) pdfluent::compat::Pixmap);

/// Owned font.
pub struct PdfCompatFont(pub(crate) pdfluent::compat::Font);

/// Owned staged-text writer.
pub struct PdfCompatTextWriter(pub(crate) pdfluent::compat::TextWriter);

/// Owned form widget.
pub struct PdfCompatWidget(pub(crate) pdfluent::compat::Widget);

/// Owned link.
pub struct PdfCompatLink(pub(crate) pdfluent::compat::Link);

/// Owned table finder over one page.
pub struct PdfCompatTableFinder(pub(crate) pdfluent::compat::TableFinder);

/// Owned option-dependent text result.
pub struct PdfCompatTextOutput(pub(crate) pdfluent::compat::TextOutput);

/// Owned text-search hits: rectangles or quadrilaterals.
pub struct PdfCompatSearchHits(pub(crate) pdfluent::compat::SearchHits);

// ---- Geometry values -------------------------------------------------------
// Plain `repr(C)` mirrors of the facade geometry structs. Passed by value or
// by pointer; the layout is fixed, so these are the ABI's only non-opaque
// structs beside `PdfTextSpan`.

/// Compatible rectangle: four edges in points.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfCompatRect {
    /// Left edge, in points.
    pub x0: f64,
    /// Top edge, in points.
    pub y0: f64,
    /// Right edge, in points.
    pub x1: f64,
    /// Bottom edge, in points.
    pub y1: f64,
}

/// Compatible integer rectangle: four edges in pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfCompatIRect {
    /// Left edge, in pixels.
    pub x0: i32,
    /// Top edge, in pixels.
    pub y0: i32,
    /// Right edge, in pixels.
    pub x1: i32,
    /// Bottom edge, in pixels.
    pub y1: i32,
}

/// Compatible point: an x/y pair in points.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfCompatPoint {
    /// Horizontal position, in points.
    pub x: f64,
    /// Vertical position, in points.
    pub y: f64,
}

/// Compatible 2D affine matrix: six coefficients in `a b c d e f` order.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfCompatMatrix {
    /// Horizontal scale factor.
    pub a: f64,
    /// Vertical shear factor.
    pub b: f64,
    /// Horizontal shear factor.
    pub c: f64,
    /// Vertical scale factor.
    pub d: f64,
    /// Horizontal translation, in points.
    pub e: f64,
    /// Vertical translation, in points.
    pub f: f64,
}

/// Compatible quadrilateral: four corners.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfCompatQuad {
    /// Upper-left corner.
    pub ul: PdfCompatPoint,
    /// Upper-right corner.
    pub ur: PdfCompatPoint,
    /// Lower-left corner.
    pub ll: PdfCompatPoint,
    /// Lower-right corner.
    pub lr: PdfCompatPoint,
}

impl From<pdfluent::compat::Rect> for PdfCompatRect {
    fn from(r: pdfluent::compat::Rect) -> Self {
        Self {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

impl From<PdfCompatRect> for pdfluent::compat::Rect {
    fn from(r: PdfCompatRect) -> Self {
        Self {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

impl From<pdfluent::compat::IRect> for PdfCompatIRect {
    fn from(r: pdfluent::compat::IRect) -> Self {
        Self {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

impl From<PdfCompatIRect> for pdfluent::compat::IRect {
    fn from(r: PdfCompatIRect) -> Self {
        Self {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

impl From<pdfluent::compat::Point> for PdfCompatPoint {
    fn from(p: pdfluent::compat::Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl From<PdfCompatPoint> for pdfluent::compat::Point {
    fn from(p: PdfCompatPoint) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl From<pdfluent::compat::Matrix> for PdfCompatMatrix {
    fn from(m: pdfluent::compat::Matrix) -> Self {
        Self {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            e: m.e,
            f: m.f,
        }
    }
}

impl From<PdfCompatMatrix> for pdfluent::compat::Matrix {
    fn from(m: PdfCompatMatrix) -> Self {
        Self {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            e: m.e,
            f: m.f,
        }
    }
}

impl From<pdfluent::compat::Quad> for PdfCompatQuad {
    fn from(q: pdfluent::compat::Quad) -> Self {
        Self {
            ul: q.ul.into(),
            ur: q.ur.into(),
            ll: q.ll.into(),
            lr: q.lr.into(),
        }
    }
}

impl From<PdfCompatQuad> for pdfluent::compat::Quad {
    fn from(q: PdfCompatQuad) -> Self {
        Self {
            ul: q.ul.into(),
            ur: q.ur.into(),
            ll: q.ll.into(),
            lr: q.lr.into(),
        }
    }
}

/// Colour-space codes: the channel count, matching `Colorspace::n`.
pub(crate) fn colorspace_from_code(code: i32) -> Option<pdfluent::compat::Colorspace> {
    use pdfluent::compat::Colorspace;
    match code {
        1 => Some(Colorspace::Gray),
        3 => Some(Colorspace::Rgb),
        4 => Some(Colorspace::Cmyk),
        _ => None,
    }
}

/// Colour-space codes: the channel count, matching `Colorspace::n`.
///
/// Fails on a colour space this binding has no code for (see
/// [`unknown_variant`]).
pub(crate) fn colorspace_to_code(
    space: pdfluent::compat::Colorspace,
) -> Result<i32, pdfluent::Error> {
    use pdfluent::compat::Colorspace;
    match space {
        Colorspace::Gray => Ok(1),
        Colorspace::Rgb => Ok(3),
        Colorspace::Cmyk => Ok(4),
        _ => Err(unknown_variant("colour space")),
    }
}

/// The error for a value of a `#[non_exhaustive]` facade enum that this
/// binding does not know yet.
///
/// The facade's 1.x enums are `#[non_exhaustive]`, so a later minor release
/// may add a variant this build was not compiled against. Reporting it as a
/// neighbouring code would misstate the value; failing says what happened.
pub(crate) fn unknown_variant(what: &str) -> pdfluent::Error {
    pdfluent::Error::Unsupported(format!(
        "{what}: this value is not supported by this version of the C binding"
    ))
}

/// Record an [`unknown_variant`] failure and return its status.
pub(crate) fn fail_unknown_variant(what: &str) -> PdfStatus {
    fail(&unknown_variant(what), false)
}

// ---- Compat-kind slot ------------------------------------------------------

thread_local! {
    static LAST_KIND: RefCell<i32> = const { RefCell::new(0) };
}

/// Compat-kind codes for [`pdf_compat_last_error_kind`].
pub(crate) mod kind {
    /// No kind: success, or a facade error without a compat kind.
    pub(crate) const NONE: i32 = 0;
    /// Wrong argument type.
    pub(crate) const WRONG_TYPE: i32 = 1;
    /// Right type, unacceptable value.
    pub(crate) const BAD_VALUE: i32 = 2;
    /// Unknown key.
    pub(crate) const UNKNOWN_KEY: i32 = 3;
    /// Index outside the valid range.
    pub(crate) const INDEX_OUT_OF_RANGE: i32 = 4;
    /// File not found.
    pub(crate) const MISSING_FILE: i32 = 5;
    /// File bytes do not parse.
    pub(crate) const UNREADABLE_FILE: i32 = 6;
    /// File has no bytes.
    pub(crate) const EMPTY_FILE: i32 = 7;
    /// Engine failure.
    pub(crate) const ENGINE_FAULT: i32 = 8;
    /// Optional component not installed.
    pub(crate) const MISSING_COMPONENT: i32 = 9;
    /// Retired spelling was used.
    pub(crate) const DEPRECATION: i32 = 10;
}

/// The compat failure kind behind the last error on this thread.
///
/// Returns 0 when the last call succeeded or failed without a compat kind;
/// otherwise one of the `PDF_COMPAT_KIND_*` codes. Like
/// [`pdf_get_last_error`](crate::pdf_get_last_error), the slot is per-thread
/// and valid until the next API call on the same thread.
#[no_mangle]
pub extern "C" fn pdf_compat_last_error_kind() -> i32 {
    LAST_KIND.with(|cell| *cell.borrow())
}

pub(crate) fn set_kind(code: i32) {
    LAST_KIND.with(|cell| {
        *cell.borrow_mut() = code;
    });
}

pub(crate) fn kind_of(kind: &ErrorKind) -> i32 {
    match kind {
        ErrorKind::WrongType => kind::WRONG_TYPE,
        ErrorKind::BadValue => kind::BAD_VALUE,
        ErrorKind::UnknownKey => kind::UNKNOWN_KEY,
        ErrorKind::IndexOutOfRange => kind::INDEX_OUT_OF_RANGE,
        ErrorKind::MissingFile => kind::MISSING_FILE,
        ErrorKind::UnreadableFile => kind::UNREADABLE_FILE,
        ErrorKind::EmptyFile => kind::EMPTY_FILE,
        ErrorKind::EngineFault => kind::ENGINE_FAULT,
        ErrorKind::MissingComponent => kind::MISSING_COMPONENT,
        ErrorKind::Deprecation => kind::DEPRECATION,
        _ => kind::ENGINE_FAULT,
    }
}

// ---- Error mapping ---------------------------------------------------------

/// Map a facade error onto a status code.
///
/// `page_indexed` tells whether an out-of-range index means a page index
/// (status 5) rather than a generic bad index (status 1): page calls pass
/// `true`, everything else `false`.
pub(crate) fn status_for(err: &pdfluent::Error, page_indexed: bool) -> PdfStatus {
    if let Some(kind) = err.compat_kind() {
        return match kind {
            ErrorKind::WrongType
            | ErrorKind::BadValue
            | ErrorKind::UnknownKey
            | ErrorKind::Deprecation => PdfStatus::ErrorInvalidArgument,
            ErrorKind::IndexOutOfRange if page_indexed => PdfStatus::ErrorPageRange,
            ErrorKind::IndexOutOfRange => PdfStatus::ErrorInvalidArgument,
            ErrorKind::MissingFile => PdfStatus::ErrorFileNotFound,
            ErrorKind::UnreadableFile | ErrorKind::EmptyFile => PdfStatus::ErrorCorruptPdf,
            ErrorKind::EngineFault | ErrorKind::MissingComponent => PdfStatus::ErrorUnknown,
            _ => PdfStatus::ErrorUnknown,
        };
    }
    match err {
        pdfluent::Error::FileNotFound { .. } => PdfStatus::ErrorFileNotFound,
        pdfluent::Error::InvalidPdf { .. } | pdfluent::Error::UnsupportedPdfVersion { .. } => {
            PdfStatus::ErrorCorruptPdf
        }
        pdfluent::Error::DecryptionFailed { .. } => PdfStatus::ErrorInvalidPassword,
        _ => PdfStatus::ErrorUnknown,
    }
}

/// Record a facade error: status text plus the compat-kind slot.
pub(crate) fn fail(err: &pdfluent::Error, page_indexed: bool) -> PdfStatus {
    let code = err.compat_kind().map(kind_of).unwrap_or_else(|| match err {
        pdfluent::Error::FileNotFound { .. } => kind::MISSING_FILE,
        pdfluent::Error::InvalidPdf { .. } => kind::UNREADABLE_FILE,
        _ => kind::NONE,
    });
    set_kind(code);
    error::set_last_error_str(&err.to_string());
    status_for(err, page_indexed)
}

/// Record a stub panic: unknown status, engine-fault kind, panic text kept.
pub(crate) fn fail_panic(payload: Box<dyn std::any::Any + Send>) -> PdfStatus {
    set_kind(kind::ENGINE_FAULT);
    if let Some(s) = payload.downcast_ref::<String>() {
        error::set_last_error_str(s);
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        error::set_last_error_str(s);
    } else {
        error::set_last_error_str("unimplemented compatible operation");
    }
    PdfStatus::ErrorUnknown
}

/// Record a plain argument failure: invalid-argument status, no kind.
pub(crate) fn fail_arg(msg: &str) -> PdfStatus {
    set_kind(kind::NONE);
    error::set_last_error_str(msg);
    PdfStatus::ErrorInvalidArgument
}

/// Record a negative page index: page-range status, index kind.
pub(crate) fn fail_page_range() -> PdfStatus {
    set_kind(kind::INDEX_OUT_OF_RANGE);
    error::set_last_error_str("page index out of range");
    PdfStatus::ErrorPageRange
}

/// Clear the error state on success.
pub(crate) fn clear_compat_error() {
    set_kind(kind::NONE);
    error::set_last_error_str("");
}

/// Run a facade call under `catch_unwind`, mapping the outcome.
pub(crate) fn guard<T>(
    page_indexed: bool,
    f: impl FnOnce() -> Result<T, pdfluent::Error>,
) -> Result<T, PdfStatus> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => {
            clear_compat_error();
            Ok(value)
        }
        Ok(Err(err)) => Err(fail(&err, page_indexed)),
        Err(payload) => Err(fail_panic(payload)),
    }
}

// ---- Argument helpers ------------------------------------------------------

/// Read a required null-terminated UTF-8 string.
pub(crate) unsafe fn required_str<'a>(ptr: *const c_char) -> Result<&'a str, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a valid null-terminated string.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| fail_arg("invalid UTF-8 in string argument"))
}

/// Read an optional null-terminated UTF-8 string; null becomes `None`.
pub(crate) unsafe fn optional_str<'a>(ptr: *const c_char) -> Result<Option<&'a str>, PdfStatus> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller guarantees a valid null-terminated string.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(Some)
        .map_err(|_| fail_arg("invalid UTF-8 in string argument"))
}

/// Read a required byte buffer.
pub(crate) unsafe fn required_bytes<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Read an optional byte buffer; null becomes `None`.
pub(crate) unsafe fn optional_bytes<'a>(
    ptr: *const u8,
    len: usize,
) -> Result<Option<&'a [u8]>, PdfStatus> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`.
    Ok(Some(unsafe { std::slice::from_raw_parts(ptr, len) }))
}

/// Read an optional doubles array; null becomes `None`.
pub(crate) unsafe fn optional_doubles<'a>(
    ptr: *const f64,
    len: usize,
) -> Result<Option<&'a [f64]>, PdfStatus> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller guarantees `len` readable doubles at `ptr`.
    Ok(Some(unsafe { std::slice::from_raw_parts(ptr, len) }))
}

/// Read an optional int array; null becomes `None`.
pub(crate) unsafe fn optional_ints<'a>(
    ptr: *const i32,
    len: usize,
) -> Result<Option<&'a [i32]>, PdfStatus> {
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: the caller guarantees `len` readable ints at `ptr`.
    Ok(Some(unsafe { std::slice::from_raw_parts(ptr, len) }))
}

/// Read an optional geometry value; null becomes `None`.
pub(crate) unsafe fn optional_rect(ptr: *const PdfCompatRect) -> Option<pdfluent::compat::Rect> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a readable rect at `ptr`.
    Some(unsafe { *ptr }.into())
}

/// Read an optional geometry value; null becomes `None`.
pub(crate) unsafe fn optional_irect(ptr: *const PdfCompatIRect) -> Option<pdfluent::compat::IRect> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a readable irect at `ptr`.
    Some(unsafe { *ptr }.into())
}

/// Read an optional geometry value; null becomes `None`.
pub(crate) unsafe fn optional_point(ptr: *const PdfCompatPoint) -> Option<pdfluent::compat::Point> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a readable point at `ptr`.
    Some(unsafe { *ptr }.into())
}

/// Read an optional geometry value; null becomes `None`.
pub(crate) unsafe fn optional_matrix(
    ptr: *const PdfCompatMatrix,
) -> Option<pdfluent::compat::Matrix> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a readable matrix at `ptr`.
    Some(unsafe { *ptr }.into())
}

/// Read a required geometry value.
pub(crate) unsafe fn required_rect(
    ptr: *const PdfCompatRect,
) -> Result<pdfluent::compat::Rect, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a readable rect at `ptr`.
    Ok(unsafe { *ptr }.into())
}

/// Read a required geometry value.
pub(crate) unsafe fn required_irect(
    ptr: *const PdfCompatIRect,
) -> Result<pdfluent::compat::IRect, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a readable irect at `ptr`.
    Ok(unsafe { *ptr }.into())
}

/// Read a required geometry value.
pub(crate) unsafe fn required_point(
    ptr: *const PdfCompatPoint,
) -> Result<pdfluent::compat::Point, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a readable point at `ptr`.
    Ok(unsafe { *ptr }.into())
}

/// Read a required geometry value.
pub(crate) unsafe fn required_matrix(
    ptr: *const PdfCompatMatrix,
) -> Result<pdfluent::compat::Matrix, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a readable matrix at `ptr`.
    Ok(unsafe { *ptr }.into())
}

/// Read a required geometry value.
pub(crate) unsafe fn required_quad(
    ptr: *const PdfCompatQuad,
) -> Result<pdfluent::compat::Quad, PdfStatus> {
    if ptr.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a readable quad at `ptr`.
    Ok(unsafe { *ptr }.into())
}

/// Read a required document handle as shared.
pub(crate) unsafe fn required_doc(
    doc: *const PdfCompatDocument,
) -> Result<&'static pdfluent::PdfDocument, PdfStatus> {
    if doc.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a live document handle, used from one
    // thread at a time; the borrow never outlives the call.
    Ok(unsafe { &(*doc).0 })
}

/// Read a required document handle as exclusive.
pub(crate) unsafe fn required_doc_mut(
    doc: *mut PdfCompatDocument,
) -> Result<&'static mut pdfluent::PdfDocument, PdfStatus> {
    if doc.is_null() {
        return Err(fail_arg("null pointer argument"));
    }
    // SAFETY: the caller guarantees a live document handle, used from one
    // thread at a time; the borrow never outlives the call.
    Ok(unsafe { &mut (*doc).0 })
}

// ---- Out-param helpers -----------------------------------------------------

/// Transfer a string to the caller; freed with `pdf_string_free`.
pub(crate) unsafe fn out_string(out: *mut *mut c_char, value: String) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    match CString::new(value) {
        Ok(owned) => {
            // SAFETY: the caller guarantees a writable out-param.
            unsafe { *out = owned.into_raw() };
            PdfStatus::Ok
        }
        Err(_) => fail_arg("result is not a C string"),
    }
}

/// Transfer bytes to the caller; freed with `pdf_bytes_free` and its length.
pub(crate) unsafe fn out_bytes(
    out_data: *mut *mut u8,
    out_len: *mut usize,
    value: Vec<u8>,
) -> PdfStatus {
    if out_data.is_null() || out_len.is_null() {
        return fail_arg("null pointer argument");
    }
    let mut value = value.into_boxed_slice();
    let ptr = value.as_mut_ptr();
    let len = value.len();
    std::mem::forget(value);
    // SAFETY: the caller guarantees writable out-params.
    unsafe {
        *out_data = ptr;
        *out_len = len;
    }
    PdfStatus::Ok
}

/// Transfer a boxed value to the caller as an opaque handle.
pub(crate) unsafe fn out_handle<T>(out: *mut *mut T, value: T) -> PdfStatus {
    if out.is_null() {
        return fail_arg("null pointer argument");
    }
    // SAFETY: the caller guarantees a writable out-param.
    unsafe { *out = Box::into_raw(Box::new(value)) };
    PdfStatus::Ok
}
