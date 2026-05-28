/**
 * Hand-maintained TypeScript supplement for `@pdfluent/sdk-wasm`.
 *
 * Augments the generated `xfa_wasm.d.ts` with:
 * - `PdfluentError` class declaration (errors thrown by all WASM methods)
 * - `XfaWasmError` legacy alias (kept for older consumer code that imported
 *   the pre-1.0 type name)
 * - `free()` lifecycle documentation for `PdfDoc` and `XfaEngine`
 *
 * Usage: import this file alongside the main module in projects that need
 * strict error handling:
 *
 * ```ts
 * import init, { PdfDoc } from '@pdfluent/sdk-wasm';
 * import type { PdfluentError } from '@pdfluent/sdk-wasm/pkg-types/xfa_wasm.augment';
 * ```
 *
 * Or add this file to your `tsconfig.json` `types` array for global augmentation.
 */

/* --- Typed error class ----------------------------------------------------- */

/**
 * Structured error thrown by every fallible PDFluent WASM operation.
 *
 * `PdfluentError` is a real `class` (extends `Error`) registered on
 * `globalThis.__PdfluentError` the first time the WASM module loads. The
 * thrown instance is a true subclass of `Error`, so
 * `error instanceof PdfluentError` works in user code.
 *
 * ## Example
 *
 * ```ts
 * import init, { PdfDoc, PdfluentError } from '@pdfluent/sdk-wasm';
 * await init();
 *
 * try {
 *   using doc = PdfDoc.open(corruptBytes);
 * } catch (e) {
 *   if (e instanceof PdfluentError) {
 *     console.error(`[${e.code}] ${e.message} (in ${e.operation})`);
 *     if (e.code === 'E-PARSE-INVALID-PDF') {
 *       // show "not a valid PDF" UI feedback
 *     }
 *   }
 * }
 * ```
 *
 * ## Properties
 *
 * | Property | Type | Description |
 * |----------|------|-------------|
 * | `code` | `string` | Stable C8 catalogue code, format `E-<CATEGORY>-<SPECIFIC>`. |
 * | `message` | `string` | Human-readable description. |
 * | `operation` | `string` | API call that failed (e.g. `"PdfDoc.open"`). |
 * | `help` | `string` | Optional actionable hint (may be empty). |
 * | `docsUrl` | `string` | Deep-link to documentation at `https://pdfluent.com/errors/<code>`. |
 * | `legacyCode` | `string` | Pre-1.0 SCREAMING_SNAKE_CASE identifier, preserved for backward compatibility. |
 *
 * ## C8 Error codes (canonical, from `docs/error_catalogue.md`)
 *
 * | Code | Thrown by |
 * |------|-----------|
 * | `E-PARSE-INVALID-PDF` | `PdfDoc.open` — bytes are not a valid PDF |
 * | `E-IO-GENERIC` | I/O failure |
 * | `E-PARSE-UNSUPPORTED-VERSION` | `PdfDoc.open` — PDF version too new |
 * | `E-COMPLIANCE-PDFA-INVALID` | `convertToPdfa`, `validatePdfA` |
 * | `E-LICENSE-INVALID` | `activateLicenseKey` — bad key |
 * | `E-LICENSE-FEATURE-NOT-IN-TIER` | Restricted method on Trial/lower tier |
 * | `E-ENV-UNSUPPORTED-ON-WASM` | Native-only operation invoked from WASM |
 * | `E-INTERNAL` | Internal safety-net |
 * | `E-WASM-INVALID-ARGUMENT` | Argument validation (e.g. `validatePdfA("xyz")`) |
 * | `E-WASM-PAGE-OUT-OF-RANGE` | Page index outside `0..page_count` |
 * | `E-WASM-RENDER-FAILED` | `renderPage`, `renderPageToCanvas` |
 * | `E-WASM-RENDER-FALLBACK` | Vector renderer hit unsupported feature |
 * | `E-WASM-TEXT-EXTRACT-FAILED` | `getTextPositions` |
 * | `E-WASM-XFA-FAILED` | `flattenXfa` |
 * | `E-WASM-MERGE-FAILED` | `merge` |
 * | `E-WASM-SAVE-FAILED` | Internal save / serialize |
 * | `E-WASM-INVALID-JSON` | `fromFields`, `fromJson`, `importJson`, schema export |
 * | `E-WASM-FORMCALC-FAILED` | `runCalculations` |
 *
 * ## Backward compatibility
 *
 * Before 1.0 the WASM binding threw `XfaWasmError` with a SCREAMING_SNAKE_CASE
 * `.code` property. That identifier is preserved on the new `.legacyCode`
 * property and on the `XfaWasmError` type alias (see below) so existing
 * consumer code keeps working. Newly-written code should use `.code` with the
 * `E-<CATEGORY>-<SPECIFIC>` strings.
 */
export declare class PdfluentError extends Error {
  /** Stable C8 catalogue code in `E-<CATEGORY>-<SPECIFIC>` format. */
  readonly code: string;
  /** Short identifier of the API call that failed (e.g. `"PdfDoc.open"`). */
  readonly operation: string;
  /** Optional actionable hint for developers. May be empty string. */
  readonly help: string;
  /** Deep-link into the error documentation. */
  readonly docsUrl: string;
  /** Pre-1.0 SCREAMING_SNAKE_CASE identifier, kept for backward compatibility. */
  readonly legacyCode: string;
}

/**
 * Legacy alias for `PdfluentError`. Existing code that does
 * `import { XfaWasmError }` keeps compiling; new code should use
 * `PdfluentError`.
 *
 * @deprecated Use {@link PdfluentError}.
 */
export type XfaWasmError = PdfluentError;

/* --- Type guards ----------------------------------------------------------- */

/**
 * Type guard: returns `true` when `e` is a `PdfluentError` thrown by the WASM module.
 *
 * ```ts
 * try {
 *   doc.flattenXfa();
 * } catch (e) {
 *   if (isPdfluentError(e)) {
 *     console.error(e.code, e.help);
 *   }
 * }
 * ```
 */
export declare function isPdfluentError(e: unknown): e is PdfluentError;

/**
 * Legacy alias for {@link isPdfluentError}.
 * @deprecated Use {@link isPdfluentError}.
 */
export declare function isXfaWasmError(e: unknown): e is PdfluentError;

/* --- Lifecycle augmentation ----------------------------------------------- */

/**
 * Lifecycle contract for WASM objects that hold native memory.
 *
 * Both `PdfDoc` and `XfaEngine` allocate memory in the WASM heap. The JavaScript
 * garbage collector does **not** free WASM heap memory — you must call `free()`
 * explicitly, or use the `using` keyword (TypeScript 5.2+, requires `--target ES2022`
 * or newer and `lib: ["ESNext.Disposable"]`).
 *
 * ### Option 1 — `using` (recommended, TypeScript 5.2+)
 *
 * ```ts
 * {
 *   using doc = PdfDoc.open(bytes);
 *   const count = doc.pageCount();
 * } // doc.free() called automatically here
 * ```
 *
 * ### Option 2 — explicit `free()`
 *
 * ```ts
 * const doc = PdfDoc.open(bytes);
 * try {
 *   const count = doc.pageCount();
 * } finally {
 *   doc.free();
 * }
 * ```
 *
 * ### After `free()`
 *
 * Once `free()` is called, the object's native pointer is invalidated. Any
 * subsequent method call throws a `PdfluentError` with code `"E-INTERNAL"`.
 */
export interface WasmLifecycle {
  /**
   * Release all native WASM heap memory owned by this object.
   *
   * - Must be called when you are done with the object, unless you used `using`.
   * - After calling `free()`, all methods throw.
   * - Calling `free()` twice is safe (no-op).
   * - Equivalent to `[Symbol.dispose]()`.
   */
  free(): void;

  /**
   * Alias for `free()`, called automatically by the `using` statement.
   *
   * See {@link WasmLifecycle.free} for full lifecycle documentation.
   */
  [Symbol.dispose](): void;
}
