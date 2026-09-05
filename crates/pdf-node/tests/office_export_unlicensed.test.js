/**
 * Office export must refuse below Business tier, and say something useful.
 *
 * A separate file on purpose: the licence is process-wide state, and the
 * happy-path suite sets a Business key. A refusal test living beside it would
 * pass whether or not the check exists — that exact hole was found in the C ABI
 * suite on 23-08 and is the reason this file is separate.
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  describe.skip('office export refusal (native module not built)', () => {
    test('placeholder', () => {});
  });
}

describe('office export without a licence', () => {
  test('refuses, and points at the free key before the price list', () => {
    if (process.env.PDFLUENT_LICENSE_KEY) {
      console.error(
        'SKIPPED (not a pass): PDFLUENT_LICENSE_KEY is set, so a refusal cannot be observed'
      );
      return;
    }
    const doc = PdfDocument.open(fs.readFileSync(SAMPLE_PDF));
    let boodschap = null;
    try {
      doc.toDocx();
    } catch (e) {
      boodschap = String(e.message || e);
    }
    expect(boodschap).not.toBeNull();
    expect(boodschap).toContain('pdfluent.com/sdk');
    expect(boodschap).toContain('30-day');
    expect(boodschap.indexOf('pdfluent.com/sdk')).toBeLessThan(
      boodschap.indexOf('pdfluent.com/pricing')
    );
  });
});
