/**
 * @xfa-engine/pdf-node — TypeScript demo
 *
 * Run from the crates/pdf-node directory after `npm run build`:
 *   npx ts-node examples/demo.ts [path/to/file.pdf]
 *
 * Or compile first:
 *   npx tsc examples/demo.ts --outDir examples/dist --esModuleInterop true
 *   node examples/dist/demo.js
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import {
  PdfDocument,
  ComplianceReportInfo,
  DocumentInfo,
  openPdf,
  mergePdfs,
  validatePdfa,
} from '../index';

// ── helpers ──────────────────────────────────────────────────────────────────

function banner(title: string): void {
  console.log('\n' + '─'.repeat(60));
  console.log('  ' + title);
  console.log('─'.repeat(60));
}

// ── main ─────────────────────────────────────────────────────────────────────

const inputPath: string =
  process.argv[2] ??
  path.join(__dirname, '..', '..', '..', 'fixtures', 'sample.pdf');

if (!fs.existsSync(inputPath)) {
  console.error(`File not found: ${inputPath}`);
  console.error('Usage: npx ts-node examples/demo.ts [path/to/file.pdf]');
  process.exit(1);
}

// 1. Open document via path helper ───────────────────────────────────────────
banner('1. openPdf(path) → PdfDocument');
const doc: PdfDocument = openPdf(inputPath);
console.log(`Opened: ${inputPath}`);

// 2. Page count ───────────────────────────────────────────────────────────────
banner('2. document.pageCount');
const count: number = doc.pageCount;
console.log(`Pages: ${count}`);

// 3. Metadata ─────────────────────────────────────────────────────────────────
banner('3. document.info()');
const info: DocumentInfo = doc.info();
console.log('Title:   ', info.title ?? '(none)');
console.log('Author:  ', info.author ?? '(none)');
console.log('Creator: ', info.creator ?? '(none)');

// 4. Extract text from first page ─────────────────────────────────────────────
banner('4. document.extractText(0)');
const text: string = doc.extractText(0);
const preview = text.trim().slice(0, 200).replace(/\n/g, ' ');
console.log(`Text (first 200 chars): ${preview || '(no text)'}`);

// 5. Page geometry ─────────────────────────────────────────────────────────────
banner('5. document.page(0) geometry');
const page = doc.page(0);
console.log(`Width: ${page.width.toFixed(1)} pt  Height: ${page.height.toFixed(1)} pt`);

// 6. Save to temp file ────────────────────────────────────────────────────────
banner('6. document.save(path)');
const tmpOut: string = path.join(os.tmpdir(), 'pdf-node-demo-ts-save.pdf');
doc.save(tmpOut);
const savedSize: number = fs.statSync(tmpOut).size;
console.log(`Saved to: ${tmpOut} (${savedSize} bytes)`);
fs.unlinkSync(tmpOut);

// 7. Merge two copies of the input ────────────────────────────────────────────
banner('7. mergePdfs([path, path], outputPath)');
const mergedOut: string = path.join(os.tmpdir(), 'pdf-node-demo-ts-merged.pdf');
mergePdfs([inputPath, inputPath], mergedOut);
const mergedDoc: PdfDocument = openPdf(mergedOut);
console.log(`Merged ${count} + ${count} pages → ${mergedDoc.pageCount} pages`);
console.log(`Written to: ${mergedOut}`);
fs.unlinkSync(mergedOut);

// 8. Validate PDF/A ────────────────────────────────────────────────────────────
banner('8. validatePdfA(path, "2b")');
const report: ComplianceReportInfo = validatePdfa(inputPath, '2b');
console.log(`Compliant: ${report.compliant}`);
console.log(`Errors:    ${report.errorCount}`);
console.log(`Warnings:  ${report.warningCount}`);
if (report.issues.length > 0) {
  console.log('First issue:', report.issues[0].message);
}

// 9. Open via Buffer (low-level) ───────────────────────────────────────────────
banner('9. PdfDocument.open(buffer)');
const buf: Buffer = fs.readFileSync(inputPath);
const docFromBuf: PdfDocument = PdfDocument.open(buf);
console.log(`Opened from Buffer — pageCount: ${docFromBuf.pageCount}`);

console.log('\n✓ All demos completed successfully.\n');
