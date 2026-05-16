/**
 * XFA-WASM TypeScript bindings.
 *
 * Provides typed wrappers around the WASM-exported classes.
 */

// Re-export WASM init and raw classes.
// The pkg/ directory is produced by `wasm-pack build --target web --no-default-features --features wasm`.
// At development time, resolve via tsconfig.json paths or the published @pdfluent/xfa-wasm package.
export { default as init, XfaEngine as RawXfaEngine, PdfDoc as RawPdfDoc } from '../pkg/xfa_wasm';
export type { InitOutput, InitInput, SyncInitInput } from '../pkg/xfa_wasm';

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

export interface TextRun {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  fontSize: number;
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
