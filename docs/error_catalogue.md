# PDFluent Error Catalogue

> **Append-only policy.** Error codes are frozen once assigned. You may add
> new codes; you must never rename, remove, or reassign an existing code.
> Any change to the Rust `code()` match arms must be reflected here before
> the release gate passes. The sync gate lives at
> `scripts/release/error_catalogue_sync.sh`.
>
> **Version scope:** PDFluent 1.x (GA hardening branch). Generated from
> `crates/pdfluent/src/error.rs`.
>
> **Per-binding columns:** status as of C8 survey. `TBD` = binding terminal
> (C8-3 through C8-8) has not yet confirmed the mapping. `gap` annotations
> indicate that the binding does not yet surface a dedicated type/code for
> this error; the binding terminal for that language should fill in the gap.

**Total error variants:** 13

| Code | Rust Variant | Description | How to Fix | Python | WASM/TS | Node | .NET | Java | C ABI |
|------|-------------|-------------|------------|--------|---------|------|------|------|-------|
| `E-IO-GENERIC` | `Io` | Underlying I/O operation failed. | Check that the file path is accessible and the process has read/write permissions. Inspect `source` for the underlying OS error. | PdfluentIoError | OPERATION_FAILED (no dedicated code — gap) | PdfluentIoError | PdfluentIoException | PdfluentIoException | PDF_STATUS_ERROR_FILE_NOT_FOUND (partial — gap: generic I/O has no own code) |
| `E-IO-FILE-NOT-FOUND` | `FileNotFound` | File not found at the given path. | Verify the path exists before calling. Use `Path::exists()` or handle this variant to prompt the user for the correct path. | PdfluentIoError | OPERATION_FAILED (no dedicated code — gap) | PdfluentIoError | PdfluentIoException (via PDF_STATUS_ERROR_FILE_NOT_FOUND) | PdfluentIoException | PDF_STATUS_ERROR_FILE_NOT_FOUND |
| `E-PARSE-INVALID-PDF` | `InvalidPdf` | PDF is structurally invalid. | Ensure the bytes are a complete, undamaged PDF. Check `byte_offset` for the failure site. Re-download or re-export the file if corrupt. | PdfluentParseError | INVALID_PDF | PdfluentParseError | PdfluentParseException (via PDF_STATUS_ERROR_CORRUPT_PDF) | PdfluentParseException | PDF_STATUS_ERROR_CORRUPT_PDF |
| `E-PARSE-UNSUPPORTED-VERSION` | `UnsupportedPdfVersion` | PDF version is newer than the supported maximum. | The PDF version header exceeds what this build supports. Upgrade to a newer PDFluent release, or pre-process the file with a downgrader. | PdfluentParseError (no dedicated subtype — gap) | INVALID_PDF (no dedicated code — gap) | PdfluentParseError (no dedicated subtype — gap) | PdfluentParseException (no dedicated subtype — gap) | PdfluentParseException (no dedicated subtype — gap) | PDF_STATUS_ERROR_CORRUPT_PDF (no dedicated code — gap) |
| `E-COMPLIANCE-PDFA-INVALID` | `PdfaValidationFailed` | PDF/A validation failed against the requested profile. | Inspect `violations` for specific rule identifiers. Use `OpenOptions::convert_to_pdfa()` to auto-repair, or fix the source document before validation. | PdfluentValidationError | INVALID_ARGUMENT (no dedicated code — gap) | TBD (no dedicated subtype) | PdfluentValidationException | PdfluentValidationException | PDF_STATUS_ERROR_CONVERT (partial) |
| `E-SECURITY-DECRYPTION-FAILED` | `DecryptionFailed` | Decryption failed — wrong password or unsupported algorithm. | Supply the correct password via `OpenOptions::password()`. Check `reason` to distinguish wrong-password from unsupported-algorithm cases. | PdfluentEncryptedError | OPERATION_FAILED (no dedicated code — gap) | PdfluentPasswordError | PdfluentPermissionException (via PDF_STATUS_ERROR_INVALID_PASS) | PdfluentEncryptedDocumentException | PDF_STATUS_ERROR_INVALID_PASS |
| `E-SECURITY-INVALID-SIGNATURE` | `InvalidSignature` | A digital signature is invalid. | The signature in `field` failed verification. Check `reason` for details. Do not trust the document content if integrity is required. | TBD (no dedicated subtype — gap) | OPERATION_FAILED (no dedicated code — gap) | TBD (no dedicated subtype — gap) | TBD (no dedicated subtype — gap) | TBD (no dedicated subtype — gap) | PDF_STATUS_ERROR_SIGN (partial — sign covers both signing failure and invalid sig) |
| `E-ENV-UNSUPPORTED-ON-WASM` | `UnsupportedOnWasm` | Operation is not supported in WebAssembly builds. | This operation (`operation`) cannot run in a WASM32 environment. Use the server-side API or guard with `#[cfg(not(target_arch = "wasm32"))]`. | N/A (Python binding is not WASM) | OPERATION_FAILED (code exists but not specific — gap) | N/A (Node binding is not WASM) | N/A (.NET binding is not WASM) | N/A (Java binding is not WASM) | N/A (C ABI is not WASM) |
| `E-EDIT-TEXT-FAILED` | `TextEditFailed` | A text-edit transaction failed (see [`crate::text_edit`] for the typed per-edit errors this message summarizes). | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| `E-ENV-MISSING-DEPENDENCY` | `MissingDependency` | A native dependency is required but not installed or discoverable. | Install the missing native library (`dep`) following `install_hint`. Ensure the library is on `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`. | TBD (no dedicated subtype — gap) | N/A (WASM has no native dependencies) | TBD (no dedicated subtype — gap) | TBD (no dedicated subtype — gap) | TBD (no dedicated subtype — gap) | PDF_STATUS_ERROR_UNKNOWN (gap) |
| `E-BUDGET-MEMORY-EXCEEDED` | `MemoryBudgetExceeded` | Memory budget set via [`crate::OpenOptions::strict_memory_limit`] exceeded. | Raise the memory limit via `OpenOptions::strict_memory_limit()`, or process the document in smaller chunks. | PdfluentLimitError | OPERATION_FAILED (no dedicated code — gap) | TBD (no dedicated subtype — gap) | PdfluentLimitException | PdfluentLimitException | PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code) |
| `E-BUDGET-RESOURCE-LIMIT` | `ResourceLimitExceeded` | A configured [`ProcessingLimits`](pdf_engine::ProcessingLimits) resource cap was exceeded while loading or processing the document. Returned when the caller has set a limits object via [`crate::OpenOptions::with_processing_limits`] and the input breaches one of those caps. The `kind` field discriminates which cap fired so callers can tell a "file too large" rejection from e.g. an "image too large" rejection without parsing the message. | Inspect `kind` to identify which cap fired, then raise the corresponding `ProcessingLimits` field. For untrusted input, keep limits tight and reject oversized files at the ingestion layer. | PdfluentLimitError | OPERATION_FAILED (no dedicated code — gap) | TBD (no dedicated subtype — gap) | PdfluentLimitException | PdfluentLimitException | PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code) |
| `E-INTERNAL` | `Internal` | Internal safety-net. Should never fire under normal operation. | This should never occur under normal operation. File a bug report at https://pdfluent.com/support including `message` and `crate_version`. | PdfluentError (base, no dedicated subtype) | OPERATION_FAILED | PdfluentOperationError | PdfluentException (base) | PdfluentException (base) | PDF_STATUS_ERROR_UNKNOWN |

---

## Per-Binding Gap Summary

The following gaps were identified during the C8 survey. Each binding terminal (C8-3 through C8-8) is responsible for closing the gaps in its language.

### Python

- `E-PARSE-UNSUPPORTED-VERSION`
- `E-SECURITY-INVALID-SIGNATURE`
- `E-EDIT-TEXT-FAILED`
- `E-ENV-MISSING-DEPENDENCY`

### WASM/TS

- `E-IO-GENERIC`
- `E-IO-FILE-NOT-FOUND`
- `E-PARSE-UNSUPPORTED-VERSION`
- `E-COMPLIANCE-PDFA-INVALID`
- `E-SECURITY-DECRYPTION-FAILED`
- `E-SECURITY-INVALID-SIGNATURE`
- `E-ENV-UNSUPPORTED-ON-WASM`
- `E-EDIT-TEXT-FAILED`
- `E-BUDGET-MEMORY-EXCEEDED`
- `E-BUDGET-RESOURCE-LIMIT`

### Node

- `E-PARSE-UNSUPPORTED-VERSION`
- `E-SECURITY-INVALID-SIGNATURE`
- `E-EDIT-TEXT-FAILED`
- `E-ENV-MISSING-DEPENDENCY`
- `E-BUDGET-MEMORY-EXCEEDED`
- `E-BUDGET-RESOURCE-LIMIT`

### .NET

- `E-PARSE-UNSUPPORTED-VERSION`
- `E-SECURITY-INVALID-SIGNATURE`
- `E-EDIT-TEXT-FAILED`
- `E-ENV-MISSING-DEPENDENCY`

### Java

- `E-PARSE-UNSUPPORTED-VERSION`
- `E-SECURITY-INVALID-SIGNATURE`
- `E-EDIT-TEXT-FAILED`
- `E-ENV-MISSING-DEPENDENCY`

### C ABI

- `E-IO-GENERIC`
- `E-PARSE-UNSUPPORTED-VERSION`
- `E-EDIT-TEXT-FAILED`
- `E-ENV-MISSING-DEPENDENCY`
- `E-BUDGET-MEMORY-EXCEEDED`
- `E-BUDGET-RESOURCE-LIMIT`

---

*This file is generated by `scripts/docs/generate_error_catalogue.py`. Do not edit manually — regenerate after updating `error.rs`.*
