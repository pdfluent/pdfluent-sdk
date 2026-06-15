/**
 * @pdfluent/node — TypeScript demo
 *
 * Run from the crates/pdf-node directory after `npm run build`:
 *   npx ts-node examples/demo.ts [path/to/file.pdf]
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import {
  PdfDocument,
  ComplianceReportInfo,
  DocumentInfo,
  FormFieldInfo,
  RedactionResult,
  openPdf,
  mergePdfs,
  validatePdfa,
} from '../index';

function banner(title: string): void {
  console.log('\n' + '─'.repeat(60));
  console.log('  ' + title);
  console.log('─'.repeat(60));
}

const inputPath: string =
  process.argv[2] ??
  path.join(__dirname, '..', '..', '..', 'fixtures', 'sample.pdf');
const acroformPath: string =
  process.argv[3] ??
  path.join(__dirname, '..', '..', '..', 'fixtures', 'acroform.pdf');

if (!fs.existsSync(inputPath)) {
  console.error(`File not found: ${inputPath}`);
  process.exit(1);
}

// ── 1. Open ────────────────────────────────────────────────────────────────
banner('1. openPdf(path) → PdfDocument');
const doc: PdfDocument = openPdf(inputPath);
console.log(`Opened: ${inputPath}`);

// ── 2. Page count ──────────────────────────────────────────────────────────
banner('2. document.pageCount');
const count: number = doc.pageCount;
console.log(`Pages: ${count}`);

// ── 3. Metadata ────────────────────────────────────────────────────────────
banner('3. document.info()');
const info: DocumentInfo = doc.info();
console.log('Title:   ', info.title ?? '(none)');
console.log('Author:  ', info.author ?? '(none)');

// ── 4. Extract text ────────────────────────────────────────────────────────
banner('4. document.extractText(0)');
const text: string = doc.extractText(0);
const preview = text.trim().slice(0, 200).replace(/\n/g, ' ');
console.log(`Text: ${preview || '(no text)'}`);

// ── 5. Form fields (read) ──────────────────────────────────────────────────
banner('5. getFormFields() + setFormField()');
if (fs.existsSync(acroformPath)) {
  const formDoc: PdfDocument = openPdf(acroformPath);
  const fields: FormFieldInfo[] = formDoc.getFormFields();
  console.log(`Found ${fields.length} form fields`);
  fields.slice(0, 3).forEach(f =>
    console.log(`  ${f.fieldType.padEnd(9)} ${f.name} = ${f.value ?? '(empty)'}`)
  );
  const textField = fields.find(f => f.fieldType === 'text');
  if (textField) {
    formDoc.setFormField(textField.name, 'filled by pdf-node TS');
    console.log(`Updated '${textField.name}'`);
    const savedForm = path.join(os.tmpdir(), 'pdf-node-ts-demo-form.pdf');
    formDoc.save(savedForm);
    console.log(`Saved: ${savedForm}`);
    fs.unlinkSync(savedForm);
  }
}

// ── 6. Add annotations ────────────────────────────────────────────────────
banner('6. document.addAnnotation(page, type, rect, content)');
doc.addAnnotation(0, 'highlight', [72, 700, 540, 720], 'highlighted');
doc.addAnnotation(0, 'freetext', [72, 650, 400, 680], 'a note');
doc.addAnnotation(0, 'note', [520, 740, 540, 760], 'sticky');
console.log('Added highlight, freetext, note.');

// ── 7. Read annotations ───────────────────────────────────────────────────
banner('7. document.annotations(page)');
const annots = doc.annotations(0);
console.log(`${annots.length} annotations on page 0`);

// ── 8. Redact text ────────────────────────────────────────────────────────
banner('8. document.redactText(term)');
const redactDoc: PdfDocument = openPdf(inputPath);
const rr: RedactionResult = redactDoc.redactText('Test');
console.log(`matches=${rr.matchesFound}  areas=${rr.areasRedacted}  pages=${rr.pagesAffected}`);
const redactOut = path.join(os.tmpdir(), 'pdf-node-ts-redacted.pdf');
redactDoc.save(redactOut);
console.log(`Saved: ${redactOut}`);
fs.unlinkSync(redactOut);

// ── 9. Save with annotations ──────────────────────────────────────────────
banner('9. document.save(path)');
const saveOut = path.join(os.tmpdir(), 'pdf-node-ts-save.pdf');
doc.save(saveOut);
console.log(`Saved: ${saveOut} (${fs.statSync(saveOut).size} bytes)`);
fs.unlinkSync(saveOut);

// ── 10. Merge ─────────────────────────────────────────────────────────────
banner('10. mergePdfs([path, path], outputPath)');
const mergedOut = path.join(os.tmpdir(), 'pdf-node-ts-merged.pdf');
mergePdfs([inputPath, inputPath], mergedOut);
const mergedDoc: PdfDocument = openPdf(mergedOut);
console.log(`Merged → ${mergedDoc.pageCount} pages`);
fs.unlinkSync(mergedOut);

// ── 11. Encrypt ───────────────────────────────────────────────────────────
banner('11. encrypt(path, pw)');
const encOut = path.join(os.tmpdir(), 'pdf-node-ts-encrypted.pdf');
const plainDoc: PdfDocument = openPdf(inputPath);
plainDoc.encrypt(encOut, 'ts-demo-pw');
console.log(`Encrypted: ${encOut} (${fs.statSync(encOut).size} bytes)`);
fs.unlinkSync(encOut);
// decrypt() is available on PdfDocument.openWithPassword(buf, pw) documents.

// ── 12. validatePdfA standalone ───────────────────────────────────────────
banner('12. validatePdfa(path, level)');
const pdfaReport: ComplianceReportInfo = validatePdfa(inputPath, '2b');
console.log(`compliant=${pdfaReport.compliant}  errors=${pdfaReport.errorCount}`);

console.log('\n✓ All demos completed successfully.\n');
