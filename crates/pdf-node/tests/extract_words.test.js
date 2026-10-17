// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Words through the Node binding.
 *
 * `fixtures/structured.pdf` is the document every binding suite reads: a
 * heading and a bulleted list on page 0, a ruled table on page 1. The engine
 * reads 41 words out of it
 * (`crates/pdf-engine/tests/words_come_from_the_blocks.rs` pins the boxing);
 * this suite pins the crossing, so a fault here stands next to a Python suite
 * that still works.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/extract_words.test.js
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const STRUCTURED_PDF = path.join(FIXTURES, 'structured.pdf');

// What the engine reads out of the fixture: 41 words in 16 blocks.
const ENGINE_WORD_COUNT = 41;

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — word extraction not checked\n');
  describe.skip('word extraction (native module not built)', () => {
    test('placeholder', () => {});
  });
}

if (PdfDocument) {
  const open = () => PdfDocument.open(fs.readFileSync(STRUCTURED_PDF));

  test('extractWords matches the engine count', () => {
    const words = open().extractWords();

    expect(words).toHaveLength(ENGINE_WORD_COUNT);
    expect(words.slice(0, 2).map((w) => w.text)).toEqual(['Safety', 'rules']);
    expect(words[words.length - 1].text).toBe('11.07');
  });

  test('a word carries its place and its box', () => {
    const doc = open();
    const words = doc.extractWords();
    const blocks = doc.extractBlocks();
    const first = words[0];

    expect(first.page).toBe(0);
    expect(first.block).toBe(0);
    expect(first.line).toBe(0);
    expect(first.bbox).toHaveLength(4);
    expect(first.bbox[2]).toBeGreaterThan(first.bbox[0]);
    expect(first.bbox[3]).toBeGreaterThan(first.bbox[1]);
    expect(blocks[first.block].text).toContain(first.text);
    expect(first.fontSize).toBeGreaterThan(0);
    expect(first.bold).toBe(true);
    expect(first.italic).toBe(false);
  });

  test('a page list returns only those pages words', () => {
    const words = open().extractWords({ pages: [1] });

    expect(words.length).toBeGreaterThan(0);
    expect(words.every((w) => w.page === 1)).toBe(true);
    expect(words.some((w) => w.text === 'Safety')).toBe(false);
  });
}
