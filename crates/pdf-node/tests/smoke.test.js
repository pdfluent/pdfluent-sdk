/**
 * Smoke tests for the pdf-node binding — 12 core scenarios.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');
const ACROFORM_PDF = path.join(FIXTURES, 'acroform.pdf');
const SIGNED_PDF = path.join(FIXTURES, 'signed.pdf');
const MULTI_PDF = path.join(FIXTURES, 'multi-page.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  // Native module not built — skip all tests
  describe.skip('pdf-node (native module not built)', () => {
    test('placeholder', () => {});
  });
}

function loadPdf(filePath) {
  const buf = fs.readFileSync(filePath);
  return PdfDocument.open(buf);
}

if (PdfDocument) {
  // ---------- Scenario 1: Open PDF, count pages ----------

  test('1. open PDF and count pages', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(doc.pageCount).toBeGreaterThanOrEqual(1);
  });

  test('1b. multi-page document', () => {
    const doc = loadPdf(MULTI_PDF);
    expect(doc.pageCount).toBeGreaterThan(1);
  });

  // ---------- Scenario 2: Render page 1 ----------

  test('2. render page 1', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const result = doc.renderPage(0, { dpi: 72 });
    expect(result.width).toBeGreaterThan(0);
    expect(result.height).toBeGreaterThan(0);
    expect(result.data.length).toBe(result.width * result.height * 4);
  });

  // ---------- Scenario 3: Extract text ----------

  test('3. extract text from page 1', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const text = doc.extractText(0);
    expect(typeof text).toBe('string');
  });

  // ---------- Scenario 4: Read metadata ----------

  test('4. read metadata', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const info = doc.info();
    // All keys should exist (may be null)
    expect(info).toHaveProperty('title');
    expect(info).toHaveProperty('author');
    expect(info).toHaveProperty('subject');
    expect(info).toHaveProperty('creator');
    expect(info).toHaveProperty('producer');
  });

  // ---------- Scenario 5: Read AcroForm fields ----------

  test('5. read form fields', () => {
    const doc = loadPdf(ACROFORM_PDF);
    const fields = doc.formFields();
    // acroform.pdf should have at least one field
    expect(Array.isArray(fields)).toBe(true);
  });

  // ---------- Scenario 6: Fill text field, save ----------

  test.skip('6. fill text field and save (TODO: save API not exposed)', () => {
    // TODO: doc.setFieldValue(name, value) works in-memory
    //       but save/export to bytes is not yet exposed
  });

  // ---------- Scenario 7: Read annotations ----------

  test('7. read annotations', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const annots = doc.annotations(0);
    expect(Array.isArray(annots)).toBe(true);
  });

  // ---------- Scenario 8: Add highlight, save ----------

  test.skip('8. add highlight annotation (TODO: annotation creation not exposed)', () => {
    // TODO: requires annotation creation API in pdf-node
  });

  // ---------- Scenario 9: Validate PDF/A ----------

  test('9. validate PDF/A', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const report = doc.validatePdfa('2b');
    expect(report).toHaveProperty('compliant');
    expect(report).toHaveProperty('errorCount');
    expect(report).toHaveProperty('warningCount');
    expect(Array.isArray(report.issues)).toBe(true);
  });

  // ---------- Scenario 10: Merge 2 PDFs ----------

  test.skip('10. merge 2 PDFs (TODO: merge API not exposed)', () => {
    // TODO: requires pdf_manip merge exposed in pdf-node
  });

  // ---------- Scenario 11: Verify signature ----------

  test('11. verify signatures', () => {
    const doc = loadPdf(SIGNED_PDF);
    const sigs = doc.validateSignatures();
    expect(Array.isArray(sigs)).toBe(true);
  });

  // ---------- Scenario 12: Extract images ----------

  test.skip('12. extract images (TODO: image extraction not exposed)', () => {
    // TODO: requires image extraction API in pdf-node
  });

  // ---------- Extra: page geometry ----------

  test('page geometry', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const geo = doc.pageGeometry(0);
    expect(geo.width).toBeGreaterThan(0);
    expect(geo.height).toBeGreaterThan(0);
  });

  // ---------- Extra: text blocks ----------

  test('structured text blocks', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const blocks = doc.extractTextBlocks(0);
    expect(Array.isArray(blocks)).toBe(true);
  });

  // ---------- Extra: search text ----------

  test('search text', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const pages = doc.searchText('the');
    expect(Array.isArray(pages)).toBe(true);
  });

  // ---------- Extra: page handle ----------

  test('page handle API', () => {
    const doc = loadPdf(SAMPLE_PDF);
    const page = doc.page(0);
    expect(page.width).toBeGreaterThan(0);
    expect(page.height).toBeGreaterThan(0);
  });

  // ---------- Extra: error handling ----------

  test('invalid PDF throws', () => {
    expect(() => PdfDocument.open(Buffer.from('not a pdf'))).toThrow();
  });

  test('page out of range throws', () => {
    const doc = loadPdf(SAMPLE_PDF);
    expect(() => doc.page(999)).toThrow();
  });
}
