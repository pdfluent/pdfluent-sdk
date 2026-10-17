// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Word boxes: `word` and `dir` on the word type (#663).
 *
 * Every word `extractWords` hands back carries its index within its line and
 * the baseline angle it was drawn at, beside the page, block, line, text and
 * box the type already had.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/word_boxes.test.js
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — word-box suite not checked\n');
  describe.skip('word boxes (native module not built)', () => {
    test('placeholder', () => {});
  });
}

if (PdfDocument) {
  function wordsOfSample() {
    const doc = PdfDocument.open(fs.readFileSync(SAMPLE_PDF));
    return doc.extractWords();
  }

  test('every word carries its index within the line', () => {
    const words = wordsOfSample();
    expect(words.length).toBeGreaterThan(0);
    for (const word of words) {
      expect(typeof word.word).toBe('number');
      expect(Number.isInteger(word.word)).toBe(true);
      expect(word.word).toBeGreaterThanOrEqual(0);
    }
    // The index restarts at each line.
    const lines = new Map();
    for (const word of words) {
      const key = `${word.block}:${word.line}`;
      if (!lines.has(key)) lines.set(key, []);
      lines.get(key).push(word.word);
    }
    for (const indices of lines.values()) {
      expect(indices).toEqual([...indices.keys()]);
    }
  });

  test('every word carries its baseline angle', () => {
    const words = wordsOfSample();
    expect(words.length).toBeGreaterThan(0);
    for (const word of words) {
      expect(typeof word.dir).toBe('number');
    }
  });
}
