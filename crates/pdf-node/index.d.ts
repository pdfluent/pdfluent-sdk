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

/** A text span at a specific position. */
export interface TextSpanInfo {
  /** The extracted text. */
  text: string;
  /** X position in user space. */
  x: number;
  /** Y position in user space. */
  y: number;
  /** Approximate font size. */
  fontSize: number;
}

/** A block of text (grouped by vertical proximity). */
export interface TextBlockInfo {
  /** Concatenated text of the block. */
  text: string;
  /** Individual spans within this block. */
  spans: TextSpanInfo[];
}

/** Information about a single form field. */
export interface FormFieldInfo {
  /** Fully qualified field name. */
  name: string;
  /** Field type: "text", "button", "choice", or "signature". */
  fieldType: string;
  /** Current value, if any. */
  value?: string;
  /** Whether the field is read-only. */
  readOnly: boolean;
}

/** Information about a single annotation. */
export interface AnnotationInfo {
  /** Annotation subtype (e.g., "Text", "Link", "Widget"). */
  annotationType: string;
  /** Annotation rectangle [x0, y0, x1, y1] in page coordinates. */
  rect?: number[];
  /** Text contents, if any. */
  contents?: string;
  /** Author, if any. */
  author?: string;
  /** Whether the annotation is hidden. */
  hidden: boolean;
  /** Whether the annotation should print. */
  printable: boolean;
}

/** Signature validation result. */
export interface SignatureResult {
  /** Validation status: "valid", "invalid", or "unknown". */
  status: string;
  /** Reason for invalid/unknown status. */
  reason?: string;
  /** Fully qualified field name. */
  fieldName: string;
  /** Signer common name, if available. */
  signer?: string;
  /** Signing timestamp, if available. */
  timestamp?: string;
}

/** A PDF/A compliance issue. */
export interface ComplianceIssueInfo {
  /** Rule identifier. */
  rule: string;
  /** Severity: "error", "warning", or "info". */
  severity: string;
  /** Description. */
  message: string;
}

/** PDF/A compliance report. */
export interface ComplianceReportInfo {
  /** Whether the document is compliant. */
  compliant: boolean;
  /** Number of errors. */
  errorCount: number;
  /** Number of warnings. */
  warningCount: number;
  /** All issues found. */
  issues: ComplianceIssueInfo[];
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
  /** Extract structured text blocks from this page. */
  textBlocks(): TextBlockInfo[];
  /** Get annotations on this page. */
  annotations(): AnnotationInfo[];
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
  /** Extract structured text blocks from a page. */
  extractTextBlocks(index: number): TextBlockInfo[];
  /** Get all form fields in the document. */
  formFields(): FormFieldInfo[];
  /** Get the value of a form field by name. */
  getFieldValue(name: string): string | undefined;
  /** Set the value of a form field by name. */
  setFieldValue(name: string, value: string): void;
  /** Get annotations on a specific page (0-based). */
  annotations(pageIndex: number): AnnotationInfo[];
  /** Validate all digital signatures. */
  validateSignatures(): SignatureResult[];
  /** Validate against a PDF/A conformance level (e.g., "1b", "2a", "3b"). */
  validatePdfa(level: string): ComplianceReportInfo;
}
