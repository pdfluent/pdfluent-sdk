/**
 * XFA-WASM TypeScript bindings.
 *
 * Provides typed wrappers around the WASM-exported classes.
 */

// Re-export WASM init and raw classes.
// The pkg/ directory is produced by `wasm-pack build --target web --no-default-features --features wasm`.
// At development time, resolve via tsconfig.json paths or the published @pdfluent/sdk-wasm package.
export { default as init, XfaEngine as RawXfaEngine, PdfDoc as RawPdfDoc } from '../pkg/xfa_wasm';
export type { InitOutput, InitInput, SyncInitInput } from '../pkg/xfa_wasm';

// --- Typed error class ---
// PdfluentError is registered on globalThis by the WASM module on first load,
// so it is always available at runtime once `await init()` has resolved. The
// type declaration lives in pkg-types/xfa_wasm.augment.d.ts; here we provide a
// runtime accessor + the public type re-export.

import type { PdfluentError as _PdfluentErrorType } from '../pkg-types/xfa_wasm.augment';

/** Re-exported typed error class. See `pkg-types/xfa_wasm.augment.d.ts`. */
export type PdfluentError = _PdfluentErrorType;

/**
 * Runtime accessor for the `PdfluentError` class.
 *
 * Returns the constructor used to create errors thrown from the WASM module.
 * The class is installed on `globalThis.__PdfluentError` on first error
 * throw / first access via this helper.
 */
export function getPdfluentError(): {
  new (message: string): PdfluentError;
  prototype: PdfluentError;
} {
  const g = globalThis as unknown as {
    __PdfluentError?: { new (message: string): PdfluentError; prototype: PdfluentError };
  };
  if (g.__PdfluentError) return g.__PdfluentError;
  // Fallback shim if WASM hasn't been initialised yet.
  class PdfluentErrorShim extends Error {
    public readonly code: string = 'E-INTERNAL';
    public readonly operation: string = '';
    public readonly help: string = '';
    public readonly docsUrl: string = 'https://pdfluent.com/errors/E-INTERNAL';
    public readonly legacyCode: string = 'OPERATION_FAILED';
    public constructor(message: string) {
      super(message);
      this.name = 'PdfluentError';
    }
  }
  return PdfluentErrorShim as unknown as {
    new (message: string): PdfluentError;
    prototype: PdfluentError;
  };
}

/**
 * Type guard: `true` when `e` is a `PdfluentError` thrown from the WASM module.
 *
 * ```ts
 * try { using doc = PdfDoc.open(bytes); }
 * catch (e) {
 *   if (isPdfluentError(e)) {
 *     console.error(e.code, e.message, e.operation);
 *   }
 * }
 * ```
 */
export function isPdfluentError(e: unknown): e is PdfluentError {
  if (!(e instanceof Error)) return false;
  // Tolerate the case where init() hasn't run yet: name + code field is enough.
  const hasShape =
    typeof (e as { code?: unknown }).code === 'string' &&
    typeof (e as { operation?: unknown }).operation === 'string';
  if (e.name === 'PdfluentError' && hasShape) return true;
  const g = globalThis as unknown as { __PdfluentError?: Function };
  return g.__PdfluentError != null && e instanceof (g.__PdfluentError as Function);
}

/** Legacy alias for {@link isPdfluentError}. @deprecated */
export const isXfaWasmError = isPdfluentError;


// --- Type definitions ---

export interface FieldDef {
  name: string;
  value?: string;
  calculate?: string;
  validate?: string;
}

export interface PdfMetadata {
  title: string | null;
  author: string | null;
  subject: string | null;
  keywords: string | null;
  creator: string | null;
  producer: string | null;
}

export interface SignatureInfo {
  field_name: string;
  signer: string | null;
  reason: string | null;
  location: string | null;
  signing_time: string | null;
  sub_filter: string | null;
}

export interface ComplianceIssue {
  rule: string;
  severity: string;
  message: string;
}

export interface ComplianceReport {
  compliant: boolean;
  errors: number;
  warnings: number;
  issues: ComplianceIssue[];
}

export interface DssInfo {
  has_ltv: boolean;
  certificates: number;
  ocsp_responses: number;
  crls: number;
  vri_entries: number;
}

/**
 * Source of `width` and `charBounds` coordinates.
 *
 * - `"Metric"`: derived from real font advance metrics (FreeType/HarfBuzz
 *   reference). Widths are physically accurate.
 * - `"Estimate"`: derived from PDF content-stream displacement only.
 *   Width may differ from the rendered glyph advance for variable-width
 *   fonts; treat as approximate.
 */
export type TextRunWidthSource = 'Metric' | 'Estimate';

/**
 * One glyph's bounding box `[x0, y0, x1, y1]` in CSS-pixel space
 * (y=0 at top-left, y increasing downward).
 */
export type GlyphBox = [number, number, number, number];

/**
 * One run of contiguous text on a page. Schema is additive — older
 * consumers can ignore unknown keys; new fields default to safe values.
 */
export interface TextRun {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  fontSize: number;

  /** PostScript font name (subset prefix stripped). Omitted when unknown. */
  fontName?: string;
  /** Inferred bold style. Always present (defaults to `false`). */
  isBold: boolean;
  /** Inferred italic style. Always present (defaults to `false`). */
  isItalic: boolean;
  /** Fill color as `[r, g, b, a]` 0–255. Omitted for patterns/shadings. */
  color?: [number, number, number, number];

  /** `"Metric"` when widths come from real font advance data; `"Estimate"` otherwise. */
  widthSource: TextRunWidthSource;
  /**
   * Per-glyph bounding boxes, one entry per source glyph.
   * Coordinates share the same `x`/`y` CSS-pixel space.
   * Omitted when no glyph metrics are available.
   */
  charBounds?: GlyphBox[];
}

/**
 * Strategy used to isolate a formatting mutation from neighbouring runs.
 *
 * - `"NoIsolation"`: no `q`/`Q` group injected (size-only change OR run is
 *   already inside a single-run group).
 * - `"AddQGroup"`: a new `q … Q` block was added around the target run to
 *   confine fill-color state.
 * - `"ReuseExistingQGroup"`: the run sits alone inside a pre-existing
 *   `q … Q`; no new group was added.
 */
export type FormatIsolationStrategy =
  | 'NoIsolation'
  | 'AddQGroup'
  | 'ReuseExistingQGroup';

/**
 * Result of `PdfDocMut.formatTextSpan`.
 *
 * Returned both for successful formatting and for no-op invocations
 * (`fontSize=undefined && color=undefined`). When `formatted` is `false`,
 * the document was not modified.
 */
export interface FormatTextSpanResult {
  /** `true` if any operators were injected into the content stream. */
  formatted: boolean;
  /** Bytes added to the content stream by injected operators. */
  bytesChanged: number;
  /** State-isolation strategy chosen for this run. */
  isolationStrategy: FormatIsolationStrategy;
  /** Pre-format font size (points) when a preceding `Tf` was detected. */
  originalSize?: number;
  /** Pre-format fill color `[r, g, b]` in `0.0..=1.0` when resolvable. */
  originalColor?: [number, number, number];
}

/**
 * Options accepted by `PdfDocMut.formatTextSpan`.
 *
 * At least one of `fontSize` / `colorHex` should be set; otherwise the
 * call is a no-op and `formatted=false` is returned.
 */
export interface FormatTextSpanOptions {
  /** New font size in points. */
  fontSize?: number;
  /** New fill color as a `"#RRGGBB"` hex string. */
  colorHex?: string;
}

// --- XFA Forms wrapper ---

import type { XfaEngine as RawEngine } from '../pkg/xfa_wasm';
import { XfaEngine as _XfaEngineImpl } from '../pkg/xfa_wasm';

export class XfaForms {
  private engine: RawEngine;

  private constructor(engine: RawEngine) {
    this.engine = engine;
  }

  static fromFields(fields: FieldDef[]): XfaForms {
    const raw = _XfaEngineImpl.fromFields(JSON.stringify(fields));
    return new XfaForms(raw);
  }

  static fromJson(json: string): XfaForms {
    const raw = _XfaEngineImpl.fromJson(json);
    return new XfaForms(raw);
  }

  runCalculations(): void {
    this.engine.runCalculations();
  }

  exportJson(): Record<string, unknown> {
    return JSON.parse(this.engine.exportJson()) as Record<string, unknown>;
  }

  exportSchema(): Record<string, unknown> {
    return JSON.parse(this.engine.exportSchema()) as Record<string, unknown>;
  }

  importJson(data: Record<string, unknown> | string): void {
    const json = typeof data === 'string' ? data : JSON.stringify(data);
    this.engine.importJson(json);
  }

  getFieldValue(path: string): string | undefined {
    return this.engine.getFieldValue(path) ?? undefined;
  }

  setFieldValue(path: string, value: string): boolean {
    return this.engine.setFieldValue(path, value);
  }

  get nodeCount(): number {
    return this.engine.nodeCount();
  }

  static version(): string {
    return _XfaEngineImpl.version();
  }

  free(): void {
    this.engine.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }
}

// --- PDF Document wrapper ---

import type { PdfDoc as RawPdf } from '../pkg/xfa_wasm';
import { PdfDoc as _PdfDocImpl } from '../pkg/xfa_wasm';

export interface RenderedPage {
  /** RGBA pixels as a flat Uint8Array (4 bytes per pixel, row-major). */
  data: Uint8Array;
  width: number;
  height: number;
}

export class PdfDocument {
  private doc: RawPdf;

  private constructor(doc: RawPdf) {
    this.doc = doc;
  }

  static open(data: Uint8Array): PdfDocument {
    const raw = _PdfDocImpl.open(data);
    return new PdfDocument(raw);
  }

  /**
   * Release native WASM heap memory. Call when done, or use the `using` keyword.
   * After `free()`, all methods throw.
   */
  free(): void {
    this.doc.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }

  get pageCount(): number {
    return this.doc.pageCount();
  }

  pageWidth(index: number): number {
    return this.doc.pageWidth(index);
  }

  pageHeight(index: number): number {
    return this.doc.pageHeight(index);
  }

  get metadata(): PdfMetadata {
    return JSON.parse(this.doc.metadata()) as PdfMetadata;
  }

  get signatures(): SignatureInfo[] {
    return JSON.parse(this.doc.signatures()) as SignatureInfo[];
  }

  get hasSignatures(): boolean {
    return this.doc.hasSignatures();
  }

  validatePdfA(level: string = '2b'): ComplianceReport {
    return JSON.parse(this.doc.validatePdfA(level)) as ComplianceReport;
  }

  get dssInfo(): DssInfo | null {
    const json = this.doc.dssInfo();
    return json != null ? (JSON.parse(json) as DssInfo) : null;
  }

  text(pageIndex: number): string {
    return this.doc.text(pageIndex);
  }

  /**
   * Parse text-run positions for a page.
   *
   * Returns typed `TextRun` objects instead of the raw JSON string from the WASM layer.
   */
  getTextPositions(pageIndex: number): TextRun[] {
    return JSON.parse(this.doc.getTextPositions(pageIndex)) as TextRun[];
  }

  /**
   * Render a page and return decoded pixel data.
   *
   * `renderPage` returns a binary buffer with a 4-byte little-endian width,
   * a 4-byte little-endian height, then RGBA pixels. This helper decodes that
   * format into a plain `RenderedPage` object.
   *
   * ```ts
   * const { data, width, height } = doc.renderPageDecoded(0, 1.5);
   * const imageData = new ImageData(new Uint8ClampedArray(data), width, height);
   * ctx.putImageData(imageData, 0, 0);
   * ```
   */
  renderPageDecoded(pageIndex: number, scale: number): RenderedPage {
    const raw = this.doc.renderPage(pageIndex, scale);
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    const width = view.getUint32(0, /* littleEndian */ true);
    const height = view.getUint32(4, true);
    return { data: raw.slice(8), width, height };
  }

  /**
   * Render a page directly onto an `HTMLCanvasElement`.
   *
   * Resizes the canvas to the rendered pixel dimensions and calls
   * `putImageData`. Only available in a browser context (wasm32 target).
   */
  renderPageToCanvas(canvas: HTMLCanvasElement, pageIndex: number, scale: number): void {
    this.doc.renderPageToCanvas(canvas, pageIndex, scale);
  }

  /**
   * Create an `ImageData` object for a rendered page.
   *
   * Convenience wrapper around `renderPageDecoded` for use with the Canvas 2D
   * API without needing to own the canvas element.
   *
   * ```ts
   * const imageData = doc.renderPageToImageData(0, 1.5);
   * canvas.width = imageData.width;
   * canvas.height = imageData.height;
   * ctx.putImageData(imageData, 0, 0);
   * ```
   */
  renderPageToImageData(pageIndex: number, scale: number): ImageData {
    const { data, width, height } = this.renderPageDecoded(pageIndex, scale);
    return new ImageData(new Uint8ClampedArray(data), width, height);
  }

  merge(other: Uint8Array): Uint8Array {
    return this.doc.merge(other);
  }

  convertToPdfa(level: string): Uint8Array {
    return this.doc.convertToPdfa(level);
  }

  flattenXfa(): Uint8Array {
    return this.doc.flattenXfa();
  }
}
