/**
 * @xfa-engine/pdf-node — JavaScript demo
 *
 * Run from the crates/pdf-node directory after `npm run build`:
 *   node examples/demo.js <path/to/file.pdf>
 */

'use strict';

const fs = require('fs');
const path = require('path');
const os = require('os');

const { PdfDocument, openPdf, mergePdfs, validatePdfa } = require('../index');

// ── helpers ──────────────────────────────────────────────────────────────────

function banner(title) {
  console.log('\n' + '─'.repeat(60));
  console.log('  ' + title);
  console.log('─'.repeat(60));
}

// ── main ─────────────────────────────────────────────────────────────────────

const inputPath = process.argv[2] || path.join(__dirname, '..', '..', '..', 'fixtures', 'sample.pdf');

if (!fs.existsSync(inputPath)) {
  console.error(`File not found: ${inputPath}`);
  console.error('Usage: node examples/demo.js [path/to/file.pdf]');
  process.exit(1);
}

// 1. Open document via path helper ───────────────────────────────────────────
banner('1. openPdf(path) → PdfDocument');
const doc = openPdf(inputPath);
console.log(`Opened: ${inputPath}`);

// 2. Page count ───────────────────────────────────────────────────────────────
banner('2. document.pageCount');
console.log(`Pages: ${doc.pageCount}`);

// 3. Metadata ─────────────────────────────────────────────────────────────────
banner('3. document.info()');
const info = doc.info();
console.log('Title:   ', info.title ?? '(none)');
console.log('Author:  ', info.author ?? '(none)');
console.log('Creator: ', info.creator ?? '(none)');

// 4. Extract text from first page ─────────────────────────────────────────────
banner('4. document.extractText(0)');
const text = doc.extractText(0);
const preview = text.trim().slice(0, 200).replace(/\n/g, ' ');
console.log(`Text (first 200 chars): ${preview || '(no text)'}`);

// 5. Page geometry ─────────────────────────────────────────────────────────────
banner('5. document.page(0) geometry');
const page = doc.page(0);
console.log(`Width: ${page.width.toFixed(1)} pt  Height: ${page.height.toFixed(1)} pt`);

// 6. Save to temp file ────────────────────────────────────────────────────────
banner('6. document.save(path)');
const tmpOut = path.join(os.tmpdir(), 'pdf-node-demo-save.pdf');
doc.save(tmpOut);
const savedSize = fs.statSync(tmpOut).size;
console.log(`Saved to: ${tmpOut} (${savedSize} bytes)`);
fs.unlinkSync(tmpOut);

// 7. Merge two copies of the input ────────────────────────────────────────────
banner('7. mergePdfs([path, path], outputPath)');
const mergedOut = path.join(os.tmpdir(), 'pdf-node-demo-merged.pdf');
mergePdfs([inputPath, inputPath], mergedOut);
const mergedDoc = openPdf(mergedOut);
console.log(`Merged ${doc.pageCount} + ${doc.pageCount} pages → ${mergedDoc.pageCount} pages`);
console.log(`Written to: ${mergedOut}`);
fs.unlinkSync(mergedOut);

// 8. Validate PDF/A ────────────────────────────────────────────────────────────
banner('8. validatePdfA(path, "2b")');
const report = validatePdfa(inputPath, '2b');
console.log(`Compliant: ${report.compliant}`);
console.log(`Errors:    ${report.errorCount}`);
console.log(`Warnings:  ${report.warningCount}`);
if (report.issues.length > 0) {
  console.log('First issue:', report.issues[0].message);
}

// 9. Open via Buffer (low-level) ───────────────────────────────────────────────
banner('9. PdfDocument.open(buffer)');
const buf = fs.readFileSync(inputPath);
const docFromBuf = PdfDocument.open(buf);
console.log(`Opened from Buffer — pageCount: ${docFromBuf.pageCount}`);

console.log('\n✓ All demos completed successfully.\n');
