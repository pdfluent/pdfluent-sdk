/**
 * Office export works with no licence, and the environment cannot change it.
 *
 * This file used to assert the opposite: that the exports refused below
 * Business tier and pointed at a free key. There are no tiers and no keys any
 * more (#199, #226), so what is worth guarding is that an unlicensed caller
 * gets the package and that PDFLUENT_LICENSE_KEY is inert.
 *
 * A separate file on purpose, as the refusal test was: it is the only place
 * that touches the environment, so no other suite's setup can decide the
 * answer.
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  describe.skip('office export without a licence (native module not built)', () => {
    test('placeholder', () => {});
  });
}

describe('office export without a licence', () => {
  test('produces a package, with and without a key in the environment', () => {
    const bytes = fs.readFileSync(SAMPLE_PDF);

    delete process.env.PDFLUENT_LICENSE_KEY;
    const unlicensed = PdfDocument.open(bytes).toDocx();
    expect(unlicensed.length).toBeGreaterThan(0);
    expect(unlicensed.slice(0, 2).toString('latin1')).toBe('PK');

    process.env.PDFLUENT_LICENSE_KEY = 'tier:enterprise';
    const withAKey = PdfDocument.open(bytes).toDocx();
    delete process.env.PDFLUENT_LICENSE_KEY;

    expect(withAKey.length).toBe(unlicensed.length);
  });
});
