# Fase I — C-Compatible API: Design Decisions

## Overview
Fase I implements a C-compatible API (`pdf-capi` crate) exposing the PDF engine
to non-Rust consumers via a stable C ABI.

## Design Decisions

### 1. PDFium-style API pattern
Adopted PDFium's conventions: opaque handles, status-code returns, and explicit
free functions. This makes the API immediately familiar to developers who have
used PDFium, FreeType, or similar C libraries.

### 2. Opaque pointer pattern
`PdfDocument` is an opaque struct wrapping `pdf_engine::PdfDocument`. Ownership
transfers across the FFI boundary via `Box::into_raw` (creation) and
`Box::from_raw` (destruction). This keeps the Rust internals completely hidden.

### 3. Thread-local error state
Per-thread error messages via `thread_local!` + `RefCell<Option<CString>>`.
This avoids global locks while giving callers detailed error messages beyond
the status code. Modeled after PDFium's `FPDF_GetLastError`.

### 4. Hand-written C header (not cbindgen)
Attempted cbindgen first, but it produced empty output due to build-system
conflicts. Switched to a hand-written `include/pdf_engine.h` which gives full
control over naming, layout, and documentation. The header is small enough
(~120 lines) that maintenance cost is negligible.

### 5. Crate types: cdylib + staticlib + lib
- `cdylib` — shared library (.dylib/.so/.dll) for dynamic linking
- `staticlib` — static archive (.a/.lib) for static linking
- `lib` — standard Rust lib target, required for `cargo test` to find tests

### 6. Pixel buffer ownership transfer
Render functions return pixel data via `into_boxed_slice` + `mem::forget`,
with a corresponding `pdf_pixels_free(ptr, len)` for deallocation. The caller
must pass the exact byte count (`width * height * 4`) to free correctly.

### 7. String ownership transfer
Functions returning strings (text extraction, metadata) use
`CString::into_raw` with a corresponding `pdf_string_free`. Null is safe
for all free functions.

### 8. Status codes
Seven specific error codes plus a catch-all `ErrorUnknown = 99`. The gap
leaves room for future error codes without breaking ABI compatibility.

### 9. Null-safety
All functions that accept pointers check for null before dereferencing.
Null document pointers return sentinel values (0, -1, null) rather than
crashing. All `_free` functions accept null as a no-op.

## Files
- `crates/pdf-capi/src/lib.rs` — All FFI functions + 10 unit tests
- `crates/pdf-capi/src/error.rs` — Thread-local error state
- `crates/pdf-capi/src/types.rs` — `PdfDocument` handle + `PdfStatus` enum
- `crates/pdf-capi/include/pdf_engine.h` — C header
- `crates/pdf-capi/Cargo.toml` — Crate manifest

## Issues
- #184 — C API design (document lifecycle, rendering, text, metadata)
- #185 — Thread safety, error codes, header generation, library build
