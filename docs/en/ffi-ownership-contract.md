# PDFluent — FFI Ownership & Memory Contract (C ABI)

The contract for safe use of the C ABI (`pdf-capi`) and, by extension, the
language bindings layered on it. This is the QR-9 ownership documentation;
the dynamic sanitizer pass is a CI step (see status note).

## Rules

1. **Null is always safe.** Every `pdf_*` function accepts `NULL` handles
   and returns a typed `PdfStatus` / sentinel (e.g. `-1`) instead of
   dereferencing. Proven by the null-handling tests in `crates/pdf-capi`
   (86 null guards across the API; `*_null` test cases).
2. **The SDK owns what it returns; the caller frees it with the matching
   `pdf_*_free`.** Returned buffers/strings/handles are heap-owned by the
   SDK allocator and must be released through the SDK's free function — never
   `free()` directly, never twice.
3. **Caller owns what it passes in.** Input byte buffers passed to
   `pdf_document_open_from_bytes` are borrowed for the duration of the call;
   the SDK copies what it needs and does not retain the caller's pointer.
4. **Double-free / use-after-free are caller errors.** Freeing a handle twice
   or using it after free is undefined; the contract is single-owner,
   single-free. Bindings encapsulate this so end users never touch raw
   pointers.
5. **Strings are UTF-8, length-checked.** Returned strings are NUL-terminated
   UTF-8; embedded-NUL inputs are rejected with a typed error.
6. **Errors never panic across the boundary.** A Rust panic is caught at the
   FFI edge and converted to a `PdfStatus` error code (no unwinding into C).

## Verification status

- **Static / deterministic (done):** null-handling (86 guards), typed status
  codes, strict C build (`-Wall -Wextra -Werror`) — `cargo test -p pdf-capi`
  green (43 tests across suites).
- **Dynamic sanitizers (release-CI recheck):** ASAN / LSAN over the C-ABI
  harness and Miri over the `pdf-capi` unsafe blocks require a Linux
  sanitizer toolchain (not available on this Apple-silicon/stable host).
  These run as a dedicated CI job before GA. Until then QR-9 is
  `green_but_needs_release_recheck` with this exact remaining proof.
