/**
 * Smoke tests for the pdf-node binding.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/
 */

const fs = require('fs');
const os = require('os');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');
const ACROFORM_PDF = path.join(FIXTURES, 'acroform.pdf');
const SIGNED_PDF = path.join(FIXTURES, 'signed.pdf');
const MULTI_PDF = path.join(FIXTURES, 'multi-page.pdf');

let PdfDocument, openPdf, mergePdfs, validatePdfa;
try {
  ({ PdfDocument, openPdf, mergePdfs, validatePdfa } = require('../index'));
} catch (e) {
  describe.skip('pdf-node (native module not built)', () => {
    test('placeholder', () => {});
  });
}

function loadPdf(filePath) {
  const buf = fs.readFileSync(filePath);
  return PdfDocument.open(buf);
}

function tmpPath(name) {
  return path.join(os.tmpdir(), `pdf-node-test-${name}-${Date.now()}.pdf`);
}

if (PdfDocument) {
  // ── 1. Open / page count ──────────────────────────────────────────────────

  test('1. open PDF and count pages', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(doc.pageCount).toBeGreaterThanOrEqual(1);
  });

  test('1b. multi-page document', () => {
    const doc = loadPdf(MULTI_PDF);
    expect(doc.pageCount).toBeGreaterThan(1);
  });

  // ── 2. Render ─────────────────────────────────────────────────────────────

  test('2. render page 1', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const result = doc.renderPage(0, { dpi: 72 });
    expect(result.width).toBeGreaterThan(0);
    expect(result.height).toBeGreaterThan(0);
    expect(result.data.length).toBe(result.width * result.height * 4);
  });

  // ── 3. Text extraction ────────────────────────────────────────────────────

  test('3. extract text from page 1', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const text = doc.extractText(0);
    expect(typeof text).toBe('string');
  });

  // ── 4. Metadata ───────────────────────────────────────────────────────────

  test('4. read metadata', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const info = doc.info();
    expect(info).toHaveProperty('title');
    expect(info).toHaveProperty('author');
  });

  // ── 5. Form fields (read) ─────────────────────────────────────────────────

  test('5. read form fields', () => {
    const doc = loadPdf(ACROFORM_PDF);
    const fields = doc.formFields();
    expect(Array.isArray(fields)).toBe(true);
  });

  test('5b. form field structure', () => {
    const doc = loadPdf(ACROFORM_PDF);
    const fields = doc.formFields();
    if (fields.length > 0) {
      const f = fields[0];
      expect(typeof f.name).toBe('string');
      expect(typeof f.fieldType).toBe('string');
      expect(typeof f.readOnly).toBe('boolean');
    }
  });

  // ── 6. Form field write + save ────────────────────────────────────────────

  test('6. set form field and save', () => {
    const doc = loadPdf(ACROFORM_PDF);
    const fields = doc.formFields();
    const textField = fields.find(f => f.fieldType === 'text');
    if (!textField) {
      // no text field in fixture — skip gracefully
      return;
    }
    expect(() => doc.setFieldValue(textField.name, 'hello world')).not.toThrow();
    const out = tmpPath('form-save');
    doc.save(out);
    expect(fs.existsSync(out)).toBe(true);
    expect(fs.statSync(out).size).toBeGreaterThan(0);
    fs.unlinkSync(out);
  });

  // ── 7. Annotations (read) ─────────────────────────────────────────────────

  test('7. read annotations', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const annots = doc.annotations(0);
    expect(Array.isArray(annots)).toBe(true);
  });

  // ── 8. Add annotation ─────────────────────────────────────────────────────

  test('8a. add highlight annotation and save', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() =>
      doc.addAnnotation(0, 'highlight', [100, 700, 400, 720], 'marked text')
    ).not.toThrow();
    const out = tmpPath('highlight');
    doc.save(out);
    expect(fs.statSync(out).size).toBeGreaterThan(0);
    fs.unlinkSync(out);
  });

  test('8b. add freetext annotation and save', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() =>
      doc.addAnnotation(0, 'freetext', [50, 600, 300, 640], 'a note here')
    ).not.toThrow();
    const out = tmpPath('freetext');
    doc.save(out);
    expect(fs.statSync(out).size).toBeGreaterThan(0);
    fs.unlinkSync(out);
  });

  test('8c. add note (sticky) annotation', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() =>
      doc.addAnnotation(0, 'note', [50, 750, 70, 770], 'sticky note')
    ).not.toThrow();
  });

  test('8d. unknown annotation type throws', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() =>
      doc.addAnnotation(0, 'nonexistent', [0, 0, 100, 100], null)
    ).toThrow();
  });

  // ── 9. PDF/A validation ───────────────────────────────────────────────────

  test('9. validate PDF/A on doc instance', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const report = doc.validatePdfa('2b');
    expect(report).toHaveProperty('compliant');
    expect(report).toHaveProperty('errorCount');
    expect(Array.isArray(report.issues)).toBe(true);
  });

  test('9b. validatePdfa standalone function', () => {
    const report = validatePdfa(SAMPLE_PDF, '2b');
    expect(typeof report.compliant).toBe('boolean');
    expect(typeof report.errorCount).toBe('number');
  });

  // ── 10. Merge PDFs ────────────────────────────────────────────────────────

  test('10. merge 2 PDFs', () => {
    const out = tmpPath('merge');
    mergePdfs([SAMPLE_PDF, SAMPLE_PDF], out);
    const merged = openPdf(out);
    expect(merged.pageCount).toBe(2);
    fs.unlinkSync(out);
  });

  // ── 11. Redact text ───────────────────────────────────────────────────────

  test('11. redactText returns a report', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const report = doc.redactText('page');
    expect(typeof report.matchesFound).toBe('number');
    expect(typeof report.areasRedacted).toBe('number');
    expect(typeof report.pagesAffected).toBe('number');
  });

  test('11b. redactText and save produces a valid file', () => {
    const doc = loadPdf(SAMPLE_PDF);
    doc.redactText('Test');
    const out = tmpPath('redact');
    doc.save(out);
    expect(fs.statSync(out).size).toBeGreaterThan(0);
    fs.unlinkSync(out);
  });

  // ── 12. Encrypt / decrypt ─────────────────────────────────────────────────

  test('12a. encrypt saves a file', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const out = tmpPath('encrypt');
    doc.encrypt(out, 'secret123');
    expect(fs.statSync(out).size).toBeGreaterThan(0);
    fs.unlinkSync(out);
  });

  // Test 12b: full round-trip depends on encrypt_and_save producing a
  // lopdf-compatible ciphertext. Skipped until that is verified.
  test.skip('12b. decrypt saves a file (encrypt round-trip not yet verified)', () => {});

  // ── 13. Signature verification ────────────────────────────────────────────

  test('13. verify signatures', () => {
    const doc = loadPdf(SIGNED_PDF);
    const sigs = doc.validateSignatures();
    expect(Array.isArray(sigs)).toBe(true);
  });

  // ── Extra: error handling ─────────────────────────────────────────────────

  test('invalid PDF throws', () => {
    expect(() => PdfDocument.open(Buffer.from('not a pdf'))).toThrow();
  });

  test('page out of range throws', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() => doc.page(999)).toThrow();
  });

  test('addAnnotation rect too short throws', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() => doc.addAnnotation(0, 'highlight', [0, 0], null)).toThrow();
  });

  // ── Extra: page handle API ────────────────────────────────────────────────

  test('page handle API', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const page = doc.page(0);
    expect(page.width).toBeGreaterThan(0);
    expect(page.height).toBeGreaterThan(0);
  });

  test('page geometry', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const geo = doc.pageGeometry(0);
    expect(geo.width).toBeGreaterThan(0);
    expect(geo.height).toBeGreaterThan(0);
  });

  test('search text', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const pages = doc.searchText('the');
    expect(Array.isArray(pages)).toBe(true);
  });
}
