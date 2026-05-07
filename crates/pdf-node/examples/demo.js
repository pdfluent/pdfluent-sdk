/**
 * @pdfluent/node — JavaScript demo
 *
 * Run from the crates/pdf-node directory after `npm run build`:
 *   node examples/demo.js <path/to/file.pdf>
 */

'use strict';

const fs = require('fs');
const path = require('path');
const os = require('os');

const { PdfDocument, openPdf, mergePdfs, validatePdfa } = require('../index');

function banner(title) {
  console.log('\n' + '─'.repeat(60));
  console.log('  ' + title);
  console.log('─'.repeat(60));
}

const inputPath = process.argv[2] ||
  path.join(__dirname, '..', '..', '..', 'fixtures', 'sample.pdf');
const acroformPath = process.argv[3] ||
  path.join(__dirname, '..', '..', '..', 'fixtures', 'acroform.pdf');

if (!fs.existsSync(inputPath)) {
  console.error(`File not found: ${inputPath}`);
  process.exit(1);
}

// ── 1. Open document via path helper ─────────────────────────────────────────
banner('1. openPdf(path) → PdfDocument');
const doc = openPdf(inputPath);
console.log(`Opened: ${inputPath}`);

// ── 2. Page count ─────────────────────────────────────────────────────────────
banner('2. document.pageCount');
console.log(`Pages: ${doc.pageCount}`);

// ── 3. Metadata ───────────────────────────────────────────────────────────────
banner('3. document.info()');
const info = doc.info();
console.log('Title:   ', info.title ?? '(none)');
console.log('Author:  ', info.author ?? '(none)');

// ── 4. Extract text ───────────────────────────────────────────────────────────
banner('4. document.extractText(0)');
const text = doc.extractText(0);
const preview = text.trim().slice(0, 200).replace(/\n/g, ' ');
console.log(`Text (first 200 chars): ${preview || '(no text)'}`);

// ── 5. Page geometry ──────────────────────────────────────────────────────────
banner('5. document.page(0) geometry');
const page = doc.page(0);
console.log(`Width: ${page.width.toFixed(1)} pt  Height: ${page.height.toFixed(1)} pt`);

// ── 6. Form fields (read) ─────────────────────────────────────────────────────
banner('6. document.formFields()');
if (fs.existsSync(acroformPath)) {
  const formDoc = openPdf(acroformPath);
  const fields = formDoc.formFields();
  console.log(`Found ${fields.length} form fields`);
  fields.slice(0, 3).forEach(f =>
    console.log(`  ${f.fieldType.padEnd(9)} ${f.name} = ${f.value ?? '(empty)'}`)
  );

  // ── 7. Set form field value ────────────────────────────────────────────────
  banner('7. document.setFieldValue(name, value)');
  const textField = fields.find(f => f.fieldType === 'text');
  if (textField) {
    formDoc.setFieldValue(textField.name, 'filled by pdf-node');
    const updated = formDoc.getFieldValue(textField.name);
    console.log(`Set '${textField.name}' → '${updated}'`);
    const savedForm = path.join(os.tmpdir(), 'pdf-node-demo-form.pdf');
    formDoc.save(savedForm);
    console.log(`Saved with updated field: ${savedForm} (${fs.statSync(savedForm).size} bytes)`);
    fs.unlinkSync(savedForm);
  } else {
    console.log('No text field found in acroform fixture — skipping set.');
  }
}

// ── 8. Add annotation ─────────────────────────────────────────────────────────
banner('8. document.addAnnotation(page, type, rect, content)');
doc.addAnnotation(0, 'highlight', [72, 700, 540, 720], 'highlighted by pdf-node');
doc.addAnnotation(0, 'freetext', [72, 650, 400, 680], 'free text note');
doc.addAnnotation(0, 'note', [520, 740, 540, 760], 'sticky note');
console.log('Added highlight, freetext, and note annotations to page 0.');

// ── 9. Get annotations ────────────────────────────────────────────────────────
banner('9. document.annotations(page) — read-only via pdf-syntax');
const annots = doc.annotations(0);
console.log(`Found ${annots.length} annotations on page 0`);
annots.slice(0, 3).forEach(a =>
  console.log(`  ${a.annotationType}  contents=${a.contents ?? '(none)'}`)
);

// ── 10. Redact text ───────────────────────────────────────────────────────────
banner('10. document.redactText(term, page?)');
const redactDoc = openPdf(inputPath);
const report = redactDoc.redactText('Test');
console.log(`Redaction: ${report.matchesFound} matches, ${report.areasRedacted} areas, ${report.pagesAffected} pages`);
const redactOut = path.join(os.tmpdir(), 'pdf-node-demo-redacted.pdf');
redactDoc.save(redactOut);
console.log(`Redacted PDF saved: ${redactOut} (${fs.statSync(redactOut).size} bytes)`);
fs.unlinkSync(redactOut);

// ── 11. Save ──────────────────────────────────────────────────────────────────
banner('11. document.save(path) — includes added annotations');
const saveOut = path.join(os.tmpdir(), 'pdf-node-demo-save.pdf');
doc.save(saveOut);
console.log(`Saved: ${saveOut} (${fs.statSync(saveOut).size} bytes)`);
fs.unlinkSync(saveOut);

// ── 12. Merge ─────────────────────────────────────────────────────────────────
banner('12. mergePdfs([path, path], outputPath)');
const mergedOut = path.join(os.tmpdir(), 'pdf-node-demo-merged.pdf');
mergePdfs([inputPath, inputPath], mergedOut);
const mergedDoc = openPdf(mergedOut);
console.log(`Merged ${doc.pageCount} + ${doc.pageCount} → ${mergedDoc.pageCount} pages`);
fs.unlinkSync(mergedOut);

// ── 13. Encrypt ───────────────────────────────────────────────────────────────
banner('13. document.encrypt(path, password)');
const encOut = path.join(os.tmpdir(), 'pdf-node-demo-encrypted.pdf');

const plainDoc = openPdf(inputPath);
plainDoc.encrypt(encOut, 'demo-password');
console.log(`Encrypted: ${encOut} (${fs.statSync(encOut).size} bytes)`);
fs.unlinkSync(encOut);

// document.decrypt(path) saves a decrypted copy of a password-opened document.
console.log('decrypt() is available on PdfDocument.openWithPassword(buf, pw).');

// ── 14. validatePdfa standalone ───────────────────────────────────────────────
banner('14. validatePdfa(path, level)');
const pdfaReport = validatePdfa(inputPath, '2b');
console.log(`PDF/A-2b compliant: ${pdfaReport.compliant}  errors: ${pdfaReport.errorCount}`);

console.log('\n✓ All demos completed successfully.\n');
