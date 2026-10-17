// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Plain known-images on the shared construction fixture (#643).
 *
 * `PdfDocument#pageImages` plus `decodeImagePixels` over
 * fixtures/known-images.pdf, checked against
 * fixtures/known-images.expected.json: count, dimensions, colour space,
 * components, bpc, and the decoded-pixel sha256. `extractImageData`
 * rides along to pin the encoded-vs-pixels split (PNG bytes, not samples).
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/plain_known_images.test.js
 */

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const PDF = path.join(FIXTURES, 'known-images.pdf');
const EXPECTED = path.join(FIXTURES, 'known-images.expected.json');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — plain known-images not checked\n');
  describe.skip('plain known-images (native module not built)', () => {
    test('placeholder', () => {});
  });
}

function expected() {
  const images = JSON.parse(fs.readFileSync(EXPECTED, 'utf8')).images;
  if (images.length !== 2) throw new Error('fixture holds two images');
  return images.slice().sort((a, b) => a.width - b.width || a.height - b.height);
}

function norm(name) {
  return String(name).toLowerCase().replace(/device/g, '').replace(/grey/g, 'gray').replace(/[^a-z0-9]/g, '');
}

if (PdfDocument) {
  const open = () => PdfDocument.open(fs.readFileSync(PDF));

  test('pageImages lists the shared fixture', () => {
    const want = expected();
    const images = open().pageImages(0);

    expect(images).toHaveLength(2);
    const got = images.slice().sort((a, b) => a.width - b.width || a.height - b.height);
    for (let i = 0; i < want.length; i++) {
      expect(got[i].width).toBe(want[i].width);
      expect(got[i].height).toBe(want[i].height);
      expect(norm(got[i].colorSpace)).toBe(norm(want[i].colorspace));
      expect(got[i].components).toBe(want[i].components);
      expect(got[i].bitsPerComponent).toBe(want[i].bpc);
    }
  });

  test('decodeImagePixels matches the expected sha', () => {
    const want = expected();
    const bySize = new Map(want.map((w) => [`${w.width}x${w.height}`, w]));
    const doc = open();
    const images = doc.pageImages(0);

    expect(images).toHaveLength(2);
    for (const info of images) {
      const w = bySize.get(`${info.width}x${info.height}`);
      expect(w).toBeDefined();
      const decoded = doc.decodeImagePixels(info.id);
      expect(decoded.width).toBe(w.width);
      expect(decoded.height).toBe(w.height);
      expect(decoded.components).toBe(w.components);
      expect(norm(decoded.colorSpace)).toBe(norm(w.colorspace));
      const pixels = Buffer.from(decoded.pixels);
      expect(pixels.length).toBe(w.width * w.height * w.components);
      expect(crypto.createHash('sha256').update(pixels).digest('hex')).toBe(w.sha256);
    }
  });

  test('extractImageData is png not pixels', () => {
    const doc = open();
    const images = doc.pageImages(0);

    expect(images).toHaveLength(2);
    for (const info of images) {
      const extracted = doc.extractImageData(info.id);
      expect(extracted.encoding).toBe('png');
      expect([...Buffer.from(extracted.data).subarray(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
      const decoded = doc.decodeImagePixels(info.id);
      expect(Buffer.from(extracted.data).length).not.toBe(Buffer.from(decoded.pixels).length);
    }
  });
}
