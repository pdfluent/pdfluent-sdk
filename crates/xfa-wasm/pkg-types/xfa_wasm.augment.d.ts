/**
 * Hand-maintained TypeScript supplement for `@pdfluent/xfa-wasm`.
 *
 * Augments the generated `xfa_wasm.d.ts` with:
 * - `XfaWasmError` class declaration (errors thrown by all WASM methods)
 * - `free()` lifecycle documentation for `PdfDoc` and `XfaEngine`
 *
 * Usage: import this file alongside the main module in projects that need
 * strict error handling:
 *
 * ```ts
 * import init, { PdfDoc } from '@pdfluent/xfa-wasm';
 * import type { XfaWasmError } from '@pdfluent/xfa-wasm/pkg-types/xfa_wasm.augment';
 * ```
 *
 * Or add this file to your `tsconfig.json` `types` array for global augmentation.
 */

/* ─── Error class ────────────────────────────────────────────────────────── */

/**
 * Structured error thrown by all PDFluent WASM operations.
 *
 * Every fallible WASM method throws `XfaWasmError` — a standard `Error` with
 * three additional machine-inspectable properties.
 *
 * ## Example
 *
 * ```ts
 * import init, { PdfDoc } from '@pdfluent/xfa-wasm';
 * await init();
 *
 * try {
 *   using doc = PdfDoc.open(corruptBytes);
 * } catch (e) {
 *   if (e instanceof Error && 'code' in e) {
 *     const err = e as XfaWasmError;
 *     console.error(`[${err.code}] ${err.message}`);
 *     if (err.code === 'INVALID_PDF') {
 *       // show "not a valid PDF" UI feedback
 *     }
 *   }
 * }
 * ```
 *
 * ## Error codes (stable, SCREAMING_SNAKE_CASE)
 *
 * | Code | Thrown by |
 * |---|---|
 * | `INVALID_PDF` | `PdfDoc.open` — bytes are not a valid PDF |
 * | `PAGE_OUT_OF_RANGE` | Any method that takes `page_index` |
 * | `TEXT_EXTRACT_FAILED` | `getTextPositions` |
 * | `XFA_FLATTEN_FAILED` | `flattenXfa` |
 * | `INVALID_ARGUMENT` | Methods that validate string arguments (e.g. `validatePdfA`, `convertToPdfa`) |
 * | `INVALID_JSON` | `XfaEngine.fromFields`, `fromJson`, `importJson` |
 * | `FORMCALC_ERROR` | `XfaEngine.runCalculations` |
 * | `MERGE_FAILED` | `PdfDoc.merge` |
 * | `RENDER_ERROR` | `renderPage`, `renderPageToCanvas`, `renderPageToCanvasVector` |
 * | `RENDER_FALLBACK` | `renderPageToCanvasVector` — vector path hit unsupported feature |
 * | `OPERATION_FAILED` | Internal operations (annotation builder, PDF save) |
 * | `SERIALIZE_ERROR` | Internal JSON serialization failures (should not occur in practice) |
 * | `PDFA_CLEANUP_FAILED` | `convertToPdfa` — cleanup step |
 * | `COLORSPACE_ERROR` | `convertToPdfa` — colorspace normalisation |
 * | `XMP_REPAIR_FAILED` | `convertToPdfa` — XMP metadata repair |
 */
export declare class XfaWasmError extends Error {
  /** Stable error code (SCREAMING_SNAKE_CASE). Use this for programmatic dispatch. */
  readonly code: string;
  /** Human-readable actionable hint for developers. May be empty. */
  readonly help: string;
  /** Deep-link to the error documentation page. */
  readonly docsUrl: string;
}

/* ─── Type guard ──────────────────────────────────────────────────────────── */

/**
 * Type guard: returns `true` when `e` is an `XfaWasmError` thrown by the WASM module.
 *
 * ```ts
 * try {
 *   doc.flattenXfa();
 * } catch (e) {
 *   if (isXfaWasmError(e)) {
 *     console.error(e.code, e.help);
 *   }
 * }
 * ```
 */
export declare function isXfaWasmError(e: unknown): e is XfaWasmError;

/* ─── Lifecycle augmentation ─────────────────────────────────────────────── */

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
 * subsequent method call throws with code `"OPERATION_FAILED"`. Do not pass
 * freed objects to other functions.
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
