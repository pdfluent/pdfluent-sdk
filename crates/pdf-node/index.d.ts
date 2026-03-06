/**
 * @xfa-engine/pdf-node — High-performance PDF engine for Node.js.
 *
 * Built with Rust and napi-rs for near-native performance.
 */

/** Document metadata. */
export interface DocumentInfo {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string;
  creator?: string;
  producer?: string;
}

/** A bookmark / outline item. */
export interface BookmarkItem {
  title: string;
  page?: number;
  children: BookmarkItem[];
}

/** Render options. */
export interface RenderOpts {
  /** DPI (default: 72). */
  dpi?: number;
  /** Background color [r, g, b, a] each 0.0–1.0. */
  background?: number[];
  /** Force output width in pixels. */
  width?: number;
  /** Force output height in pixels. */
  height?: number;
}

/** Rendered page result. */
export interface RenderResult {
  /** RGBA pixel data. */
  data: Buffer;
  /** Width in pixels. */
  width: number;
  /** Height in pixels. */
  height: number;
}

/** Page geometry. */
export interface PageGeometry {
  /** Width in PDF points. */
  width: number;
  /** Height in PDF points. */
  height: number;
  /** Rotation in degrees. */
  rotation: number;
}

/** A handle to a single page in a PDF document. */
export class PdfPage {
  /** Page index (0-based). */
  get index(): number;
  /** Page width in PDF points. */
  get width(): number;
  /** Page height in PDF points. */
  get height(): number;
  /** Get page geometry. */
  geometry(): PageGeometry;
  /** Render this page to RGBA pixels (sync). */
  render(options?: RenderOpts): RenderResult;
  /** Render this page to RGBA pixels (async — worker thread). */
  renderAsync(options?: RenderOpts): Promise<RenderResult>;
  /** Generate a thumbnail (async). */
  thumbnail(maxDimension?: number): Promise<RenderResult>;
  /** Extract text from this page (sync). */
  text(): string;
  /** Extract text from this page (async). */
  textAsync(): Promise<string>;
}

/** A PDF document handle. */
export class PdfDocument {
  /** Open a PDF from a Buffer (sync). */
  static open(data: Buffer): PdfDocument;
  /** Open a PDF from a Buffer (async — worker thread). */
  static openAsync(data: Buffer): Promise<PdfDocument>;
  /** Open a password-protected PDF (sync). */
  static openWithPassword(data: Buffer, password: string): PdfDocument;
  /** Number of pages. */
  get pageCount(): number;
  /** Get document metadata. */
  info(): DocumentInfo;
  /** Get a page handle (0-based index). */
  page(index: number): PdfPage;
  /** Render a single page (sync). */
  renderPage(index: number, options?: RenderOpts): RenderResult;
  /** Render a single page (async — worker thread). */
  renderPageAsync(index: number, options?: RenderOpts): Promise<RenderResult>;
  /** Generate a thumbnail (async). */
  thumbnail(index: number, maxDimension?: number): Promise<RenderResult>;
  /** Extract text from a page (sync). */
  extractText(index: number): string;
  /** Extract text from a page (async). */
  extractTextAsync(index: number): Promise<string>;
  /** Search for text across all pages. Returns 0-based page indices. */
  searchText(query: string): number[];
  /** Get document bookmarks / outline. */
  bookmarks(): BookmarkItem[];
  /** Get page geometry. */
  pageGeometry(index: number): PageGeometry;
  /** Render all pages in parallel (async). */
  renderAll(options?: RenderOpts): Promise<RenderResult[]>;
}
