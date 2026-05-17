# C ABI Stability Policy

## Scope

This document governs the binary-compatibility promises for
`crates/pdf-capi/include/pdfluent.h` and the compiled shared/static libraries
`libpdf_capi.{so,dylib,dll}` / `libpdf_capi.a`.

## Versioning scheme

The library follows [Semantic Versioning 2.0](https://semver.org/):

| Change type | Version bump | ABI compatible? |
|---|---|---|
| Incompatible — see §2 | **MAJOR** | No |
| Additive — see §3 | **MINOR** | Yes |
| Fix-only — see §4 | **PATCH** | Yes |

The current version is **1.0.0-beta.1** (pre-release; no compatibility promise
until 1.0.0 stable is tagged).

## §1 Opaque-struct convention

All handle types are forward declarations only:

```c
typedef struct PdfDocument      PdfDocument;
typedef struct PdfComplianceReport PdfComplianceReport;
```

Callers **must not** take `sizeof()`, `offsetof()`, or dereference these
types.  This guarantees that the internal layout can change freely across
minor versions without breaking compiled binaries.

Any struct whose fields are visible in the public header (e.g.
`PdfTextSpan`, `PdfTextSpanArray` inside `#ifdef PDFLUENT_TEXT_EDITING`) is
**not** opaque and **changes to its layout require a major version bump**.

## §2 Breaking changes → MAJOR bump

The following are ABI-incompatible and require a MAJOR version increment:

- Removing or renaming a `#no_mangle` exported symbol.
- Changing the parameter list or return type of any exported function.
- Changing the numeric value of any `PdfStatus` or `PdfALevel` enumerator.
- Adding a field to a non-opaque struct (e.g. `PdfTextSpan`).
- Removing a field from any struct.
- Changing the calling convention (always `cdecl` / system default).

## §3 Additive changes → MINOR bump

The following are ABI-compatible and require only a MINOR increment:

- Adding new exported functions.
- Adding new enumerator values to the **end** of an enum (consumers that
  switch on `PdfStatus` must have a `default:` case).
- Introducing new opt-in feature blocks (`#ifdef PDFLUENT_*`).
- Adding new opaque handle types with matching `_free` functions.

## §4 Fix-only changes → PATCH bump

- Documentation corrections.
- Improvements to error messages (the message format is not ABI).
- Performance improvements with no visible API change.
- Bug fixes that do not alter the function signature or observable return
  values on correct inputs.

## §5 Error catalogue (canonical source for all bindings)

This is the authoritative mapping.  Python (C2), .NET (C5), and Java (C6)
bindings must derive their exception hierarchies from this table.

| Code | C constant                       | Value | Meaning                                        | Recovery hint                                       | Rust variant           |
|------|----------------------------------|-------|------------------------------------------------|-----------------------------------------------------|------------------------|
|  0   | `PDF_STATUS_OK`                  |   0   | Success                                        | —                                                   | (none — success)       |
|  1   | `PDF_STATUS_ERROR_INVALID_ARG`   |   1   | Null pointer or invalid argument               | Check argument values before calling                | `ErrorInvalidArgument` |
|  2   | `PDF_STATUS_ERROR_FILE_NOT_FOUND`|   2   | File not found or unreadable                   | Verify path exists and process has read permission  | `ErrorFileNotFound`    |
|  3   | `PDF_STATUS_ERROR_INVALID_PASS`  |   3   | Incorrect PDF password                         | Supply the owner or user password                   | `ErrorInvalidPassword` |
|  4   | `PDF_STATUS_ERROR_CORRUPT_PDF`   |   4   | Corrupt, truncated, or non-PDF data            | Verify file integrity                               | `ErrorCorruptPdf`      |
|  5   | `PDF_STATUS_ERROR_PAGE_RANGE`    |   5   | Page index out of range                        | Clamp to `[0, page_count-1]`                        | `ErrorPageRange`       |
|  6   | `PDF_STATUS_ERROR_RENDER`        |   6   | Rendering pipeline failure                     | Retry at lower DPI; check last error                | `ErrorRender`          |
|  7   | `PDF_STATUS_ERROR_CONVERT`       |   7   | PDF/A conversion failure                       | Validate before converting; check last error        | `ErrorConvert`         |
|  8   | `PDF_STATUS_ERROR_REDACT`        |   8   | Redaction failure                              | Verify pattern is valid UTF-8                       | `ErrorRedact`          |
|  9   | `PDF_STATUS_ERROR_SIGN`          |   9   | Signing failure                                | Check .p12 file and password                        | `ErrorSign`            |
| 10   | `PDF_STATUS_ERROR_ANNOTATION`    |  10   | Annotation operation failed                    | Check page index and last error                     | `ErrorAnnotation`      |
| 11   | `PDF_STATUS_ERROR_MERGE`         |  11   | Document merge failure                         | Try merging fewer documents                         | `ErrorMerge`           |
| 12   | `PDF_STATUS_ERROR_EXTRACT`       |  12   | Image/content extraction failure               | Confirm index with `pdf_page_image_count`           | `ErrorExtract`         |
| 13   | `PDF_STATUS_ERROR_SPLIT`         |  13   | Page-range split failure                       | Verify `from_page ≤ to_page < page_count`           | `ErrorSplit`           |
| 14   | `PDF_STATUS_ERROR_WATERMARK`     |  14   | Watermark operation failure                    | Check last error                                    | `ErrorWatermark`       |
| 15   | `PDF_STATUS_ERROR_COMPRESS`      |  15   | Compression/optimisation failure               | Check last error                                    | `ErrorCompress`        |
| 99   | `PDF_STATUS_ERROR_UNKNOWN`       |  99   | Unclassified internal error                    | Always accompanied by `pdf_get_last_error` text     | `ErrorUnknown`         |

### Binding mapping rules

| Language | Exception type           | Mapping rule                                     |
|----------|--------------------------|--------------------------------------------------|
| Python   | `PdfluentError` subclass | One subclass per code; name = snake_case of code |
| .NET     | `PdfluentException`      | `StatusCode` property carries the integer value  |
| Java     | `PdfluentException`      | `getStatusCode()` returns the integer value      |

The `.NET` and Java bindings currently expose only codes 0–6 and 99.  They
must be updated to add codes 7–15 as part of the C8 cross-binding error
catalogue work.

## §6 Thread safety

- All stateless query functions (`pdf_document_page_count`, etc.) are safe to
  call concurrently on **different** document handles.
- A single `PdfDocument` handle must be used from **one thread at a time**.
- The error state (`pdf_get_last_error` / `pdf_clear_error`) is thread-local;
  no synchronisation is required.

## §7 Memory model

Every heap allocation performed by the library is released by the matching
`_free` function documented in `pdfluent.h`.  Mixing allocators (e.g.
calling `free()` directly on a pointer returned by the library) is undefined
behaviour.  The library uses Rust's global allocator internally.

## §8 Platform support

| Platform          | ABI        | Library name               |
|-------------------|------------|----------------------------|
| Linux x86-64      | System V   | `libpdf_capi.so` / `.a`    |
| macOS arm64/x86-64| Mach-O     | `libpdf_capi.dylib` / `.a` |
| Windows x86-64    | MSVC x64   | `pdf_capi.dll` / `.lib`    |

No other platforms are currently supported.  The `#ifdef __cplusplus` guards
ensure the header is usable from C++ as well.
