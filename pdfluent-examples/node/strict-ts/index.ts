/**
 * Strict TypeScript example for @pdfluent/node.
 *
 * Validates that the published index.d.ts is complete and correct under
 * `tsc --strict --noEmit`.
 *
 * Run:
 *   npx tsc --strict --noEmit -p pdfluent-examples/node/strict-ts/tsconfig.json
 */

import {
  PdfDocument,
  PdfPage,
  openPdf,
  mergePdfs,
  validatePdfa,
  PdfluentError,
  PdfluentIoError,
  PdfluentParseError,
  PdfluentPasswordError,
  PdfluentPageError,
  PdfluentFormError,
  PdfluentOperationError,
  type AnnotationInfo,
  type BookmarkItem,
  type ComplianceIssueInfo,
  type ComplianceReportInfo,
  type DocumentInfo,
  type FormFieldInfo,
  type PageGeometry,
  type RedactionResult,
  type RenderOpts,
  type RenderResult,
  type SignatureResult,
  type TextBlockInfo,
  type TextSpanInfo,
} from '@pdfluent/node'
import * as fs from 'fs'

// ── Open from Buffer ──────────────────────────────────────────────────────────

function openDocument(pdfPath: string): PdfDocument {
  const buf: Buffer = fs.readFileSync(pdfPath)
  return PdfDocument.open(buf)
}

// ── Page count ────────────────────────────────────────────────────────────────

function countPages(doc: PdfDocument): number {
  return doc.pageCount
}

// ── Render ────────────────────────────────────────────────────────────────────

function renderFirst(doc: PdfDocument): RenderResult {
  const opts: RenderOpts = { dpi: 150 }
  return doc.renderPage(0, opts)
}

async function renderAsync(doc: PdfDocument): Promise<RenderResult> {
  return doc.renderPageAsync(0, { dpi: 72 })
}

async function renderAll(doc: PdfDocument): Promise<RenderResult[]> {
  return doc.renderAll({ dpi: 72 })
}

// ── Text extraction ───────────────────────────────────────────────────────────

function extractText(doc: PdfDocument, page: number): string {
  return doc.extractText(page)
}

async function extractTextAsync(doc: PdfDocument, page: number): Promise<string> {
  return doc.extractTextAsync(page)
}

function extractBlocks(doc: PdfDocument, page: number): TextBlockInfo[] {
  return doc.extractTextBlocks(page)
}

function useSpan(span: TextSpanInfo): string {
  return `${span.text} at (${span.x}, ${span.y}) size ${span.fontSize}`
}

// ── Metadata ──────────────────────────────────────────────────────────────────

function readMetadata(doc: PdfDocument): DocumentInfo {
  return doc.info()
}

// ── Page handle ───────────────────────────────────────────────────────────────

function getPage(doc: PdfDocument, index: number): PdfPage {
  return doc.page(index)
}

function pageSize(page: PdfPage): { w: number; h: number } {
  return { w: page.width, h: page.height }
}

function pageGeo(doc: PdfDocument, index: number): PageGeometry {
  return doc.pageGeometry(index)
}

// ── Bookmarks ─────────────────────────────────────────────────────────────────

function printBookmarks(items: BookmarkItem[]): void {
  for (const item of items) {
    console.log(item.title, item.page)
    printBookmarks(item.children)
  }
}

// ── Search ────────────────────────────────────────────────────────────────────

function search(doc: PdfDocument, query: string): number[] {
  return doc.searchText(query)
}

// ── Forms ─────────────────────────────────────────────────────────────────────

function readFields(doc: PdfDocument): FormFieldInfo[] {
  return doc.formFields()
}

function getField(doc: PdfDocument, name: string): string | null {
  return doc.getFieldValue(name)
}

function setField(doc: PdfDocument, name: string, value: string): void {
  doc.setFieldValue(name, value)
}

// ── Annotations ───────────────────────────────────────────────────────────────

function readAnnotations(doc: PdfDocument, page: number): AnnotationInfo[] {
  return doc.annotations(page)
}

function addAnnotation(doc: PdfDocument): void {
  doc.addAnnotation(0, 'highlight', [100, 700, 400, 720], 'marked')
}

// ── Signatures ────────────────────────────────────────────────────────────────

function validateSigs(doc: PdfDocument): SignatureResult[] {
  return doc.validateSignatures()
}

// ── Redaction ─────────────────────────────────────────────────────────────────

function redact(doc: PdfDocument): RedactionResult {
  return doc.redactText('confidential', 0)
}

// ── Encryption ────────────────────────────────────────────────────────────────

function encryptDoc(doc: PdfDocument, out: string): void {
  doc.encrypt(out, 'secret')
}

function decryptDoc(doc: PdfDocument, out: string): void {
  doc.decrypt(out)
}

// ── PDF/A ─────────────────────────────────────────────────────────────────────

function checkCompliance(doc: PdfDocument): ComplianceReportInfo {
  return doc.validatePdfa('2b')
}

function useIssue(issue: ComplianceIssueInfo): string {
  return `[${issue.severity}] ${issue.rule}: ${issue.message}`
}

// ── Module-level functions ────────────────────────────────────────────────────

function openByPath(p: string): PdfDocument {
  return openPdf(p)
}

function merge(paths: string[], out: string): void {
  mergePdfs(paths, out)
}

function validateFile(p: string): ComplianceReportInfo {
  return validatePdfa(p, '2b')
}

// ── Save ──────────────────────────────────────────────────────────────────────

function saveDoc(doc: PdfDocument, out: string): void {
  doc.save(out)
}

// ── Async open ────────────────────────────────────────────────────────────────

async function openAsync(buf: Buffer): Promise<PdfDocument> {
  return PdfDocument.openAsync(buf)
}

async function openWithPassword(buf: Buffer, pw: string): Promise<PdfDocument> {
  return PdfDocument.openWithPassword(buf, pw)
}

// ── Thumbnail ─────────────────────────────────────────────────────────────────

async function thumb(doc: PdfDocument): Promise<RenderResult> {
  return doc.thumbnail(0, 256)
}

async function pageThumb(page: PdfPage): Promise<RenderResult> {
  return page.thumbnail(128)
}

// ── Error classes ─────────────────────────────────────────────────────────────

function handleError(err: unknown): void {
  if (err instanceof PdfluentPasswordError) {
    console.error('Need a password:', err.message)
  } else if (err instanceof PdfluentParseError) {
    console.error('Corrupt PDF:', err.message)
  } else if (err instanceof PdfluentIoError) {
    console.error('I/O failure:', err.message)
  } else if (err instanceof PdfluentPageError) {
    console.error('Page out of range:', err.message)
  } else if (err instanceof PdfluentFormError) {
    console.error('Form error:', err.message)
  } else if (err instanceof PdfluentOperationError) {
    console.error('Operation failed:', err.message)
  } else if (err instanceof PdfluentError) {
    console.error('PDF error:', err.message)
  } else {
    throw err
  }
}

// ── Suppress unused-variable warnings without calling side-effects ────────────

void (openDocument as unknown)
void (countPages as unknown)
void (renderFirst as unknown)
void (renderAsync as unknown)
void (renderAll as unknown)
void (extractText as unknown)
void (extractTextAsync as unknown)
void (extractBlocks as unknown)
void (useSpan as unknown)
void (readMetadata as unknown)
void (getPage as unknown)
void (pageSize as unknown)
void (pageGeo as unknown)
void (printBookmarks as unknown)
void (search as unknown)
void (readFields as unknown)
void (getField as unknown)
void (setField as unknown)
void (readAnnotations as unknown)
void (addAnnotation as unknown)
void (validateSigs as unknown)
void (redact as unknown)
void (encryptDoc as unknown)
void (decryptDoc as unknown)
void (checkCompliance as unknown)
void (useIssue as unknown)
void (openByPath as unknown)
void (merge as unknown)
void (validateFile as unknown)
void (saveDoc as unknown)
void (openAsync as unknown)
void (openWithPassword as unknown)
void (thumb as unknown)
void (pageThumb as unknown)
void (handleError as unknown)
