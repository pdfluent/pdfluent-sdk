/**
 * Office export contract for the Node binding.
 *
 * Until 23-08-2026 no binding could convert a PDF to Word, Excel or
 * PowerPoint, while the feature page sold it. The Rust crates existed and
 * stopped at the language boundary.
 *
 * These tests check the bytes are a package Office opens, not merely that a
 * call returned something. An OOXML file is a ZIP with a known entry; both
 * halves matter.
 *
 * Run:
 *   cd crates/pdf-node && npm run build && npx jest tests/office_export.test.js
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  PdfDocument = null;
}

// A skipped placeholder suite used to stand here while the real `describe`
// below ran regardless: without the addon these tests failed with
// "Cannot read properties of undefined" instead of skipping (#585).
if (!PdfDocument) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — office export not checked\n');
}
const d = PdfDocument ? describe : describe.skip;

function openSample() {
  return PdfDocument.open(fs.readFileSync(SAMPLE_PDF));
}

/** A ZIP whose central directory mentions `entry`. */
function assertOoxml(buf, entry, what) {
  expect(buf.length).toBeGreaterThan(0);
  expect(buf.slice(0, 2).toString('latin1')).toBe('PK');
  expect(buf.toString('latin1')).toContain(entry);
}

d('office export', () => {
  beforeAll(() => {
  });

  test('toDocx returns a package Word opens', () => {
    assertOoxml(openSample().toDocx(), 'word/document.xml', 'docx');
  });

  test('toXlsx returns a package Excel opens', () => {
    assertOoxml(openSample().toXlsx(), 'xl/workbook.xml', 'xlsx');
  });

  test('toPptx returns a package PowerPoint opens', () => {
    assertOoxml(openSample().toPptx(), 'ppt/presentation.xml', 'pptx');
  });
});
