# PDFluent WASM Error Handling

> Typed error model for `@pdfluent/sdk-wasm`. Aligned with the C8 error
> catalogue (`docs/error_catalogue.md`) and the Python / C ABI bindings.

## Overview

Every fallible method on `PdfDoc`, `XfaEngine`, and the license API throws a
**`PdfluentError`** — a real JavaScript `class` extending `Error`. The error
instance carries machine-inspectable properties:

| Property      | Type     | Description                                                                                |
|---------------|----------|--------------------------------------------------------------------------------------------|
| `code`        | `string` | Stable C8 catalogue code in `E-<CATEGORY>-<SPECIFIC>` format.                              |
| `message`     | `string` | Human-readable description. Stable across versions.                                        |
| `operation`   | `string` | Short identifier of the API call that failed (e.g. `"PdfDoc.open"`).                       |
| `help`        | `string` | Optional actionable hint for developers. May be empty.                                     |
| `docsUrl`     | `string` | Deep-link to documentation: `https://pdfluent.com/errors/<code>`.                          |
| `legacyCode`  | `string` | Pre-1.0 SCREAMING_SNAKE_CASE identifier, preserved for backward compatibility.             |

## Recommended pattern

```ts
import init, { PdfDoc, isPdfluentError } from '@pdfluent/sdk-wasm';
await init();

try {
  using doc = PdfDoc.open(bytes);
  console.log(doc.pageCount);
} catch (e) {
  if (isPdfluentError(e)) {
    console.error(`[${e.code}] ${e.message} (during ${e.operation})`);
    if (e.code === 'E-PARSE-INVALID-PDF') {
      // show "not a valid PDF" UI feedback
    }
  } else {
    throw e;
  }
}
```

## `instanceof` checks

`error instanceof PdfluentError` works because the class is installed on
`globalThis.__PdfluentError` by the WASM module on first load. Bundlers
that strip duplicate class definitions are still safe — the WASM module
caches the constructor and reuses it for every thrown error.

For library code that may run before `await init()`, prefer the
`isPdfluentError` type guard, which falls back to structural inspection.

## Backward compatibility

The pre-1.0 WASM binding threw an `XfaWasmError` whose `.code` was a
SCREAMING_SNAKE_CASE identifier such as `"INVALID_PDF"`. From 1.0 onwards:

1. The thrown class is renamed to `PdfluentError`. A type alias
   `XfaWasmError = PdfluentError` is kept in `pkg-types/xfa_wasm.augment.d.ts`
   so existing imports continue to compile.
2. `.code` now carries the **C8 catalogue code** (`E-<CATEGORY>-<SPECIFIC>`).
3. The original SCREAMING_SNAKE_CASE identifier is preserved on
   `.legacyCode`. Existing user code that switches on the old identifier can
   be migrated incrementally by changing `.code` to `.legacyCode`:

   ```ts
   // Old (pre-1.0)
   if (err.code === 'INVALID_PDF') { ... }

   // New (1.0+) — choose one:
   if (err.code === 'E-PARSE-INVALID-PDF') { ... }      // canonical
   if (err.legacyCode === 'INVALID_PDF') { ... }        // backwards-compatible
   ```

4. `.message` is unchanged — text content is preserved so substring matching
   on the message body keeps working.

## Code mapping (legacy → C8)

| Legacy `code`           | New `code` (C8)                  | Notes                                  |
|-------------------------|----------------------------------|----------------------------------------|
| `INVALID_PDF`           | `E-PARSE-INVALID-PDF`            | Matches `pdfluent::Error::InvalidPdf`. |
| `INVALID_JSON`          | `E-WASM-INVALID-JSON`            | WASM-binding-only category.            |
| `INVALID_ARGUMENT`      | `E-WASM-INVALID-ARGUMENT`        | WASM-binding-only category.            |
| `PAGE_OUT_OF_RANGE`     | `E-WASM-PAGE-OUT-OF-RANGE`       | WASM-binding-only category.            |
| `FORMCALC_ERROR`        | `E-WASM-FORMCALC-FAILED`         | WASM-binding-only category.            |
| `SERIALIZE_ERROR`       | `E-WASM-INVALID-JSON`            | Folded under invalid-json.             |
| `RENDER_ERROR`          | `E-WASM-RENDER-FAILED`           | WASM-binding-only category.            |
| `RENDER_FALLBACK`       | `E-WASM-RENDER-FALLBACK`         | WASM-binding-only category.            |
| `TEXT_EXTRACT_FAILED`   | `E-WASM-TEXT-EXTRACT-FAILED`     | WASM-binding-only category.            |
| `XFA_FLATTEN_FAILED`    | `E-WASM-XFA-FAILED`              | WASM-binding-only category.            |
| `MERGE_FAILED`          | `E-WASM-MERGE-FAILED`            | WASM-binding-only category.            |
| `SAVE_FAILED`           | `E-WASM-SAVE-FAILED`             | WASM-binding-only category.            |
| `OPERATION_FAILED`      | `E-INTERNAL`                     | Generic safety-net.                    |
| `PDFA_CLEANUP_FAILED`   | `E-COMPLIANCE-PDFA-INVALID`      | Matches catalogue.                     |
| `COLORSPACE_ERROR`      | `E-COMPLIANCE-PDFA-INVALID`      | Matches catalogue.                     |
| `XMP_REPAIR_FAILED`     | `E-COMPLIANCE-PDFA-INVALID`      | Matches catalogue.                     |
| `LICENSE_ERROR`         | `E-LICENSE-INVALID`              | Matches catalogue.                     |
| `LICENSE_ALREADY_SET`   | `E-LICENSE-INVALID`              | Matches catalogue.                     |
| _(no legacy)_           | `E-LICENSE-FEATURE-NOT-IN-TIER`  | New: tier-gating failures.             |

## Catalogue gaps closed

This release closes the following gaps that were called out in
`docs/error_catalogue.md` (C8 survey):

- `E-PARSE-INVALID-PDF` — was `INVALID_PDF`, now C8.
- `E-COMPLIANCE-PDFA-INVALID` — was bucketed under `PDFA_CLEANUP_FAILED` /
  `COLORSPACE_ERROR` / `XMP_REPAIR_FAILED`; all now map to the catalogue code.
- `E-LICENSE-INVALID` and `E-LICENSE-FEATURE-NOT-IN-TIER` — license errors
  were previously thrown as raw strings; now typed.
- `E-ENV-UNSUPPORTED-ON-WASM` — reserved for future native-only calls.

Remaining catalogue gaps (e.g. `E-IO-GENERIC`, `E-SECURITY-INVALID-SIGNATURE`)
will be filled as the WASM surface gains corresponding methods.
