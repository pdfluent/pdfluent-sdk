// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Known images through the Node public image API (#643).
 *
 * Reads fixtures/known-images.expected.json; no fixture literals duplicated.
 * Dimensions ride the compat path (Document#getPageImages); pixels come from
 * the plain route (PdfDocument#decodeImagePixels), because the compat
 * Document#extractImage returns the encoded file, not samples.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/known_images.test.js
 */

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const PDF = path.join(FIXTURES, 'known-images.pdf');
const EXPECTED = path.join(FIXTURES, 'known-images.expected.json');

let Document;
let PdfDocument;
try {
  ({ Document, PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — known-images suite not checked\n');
  describe.skip('known-images (native module not built)', () => {
    test('placeholder', () => {});
  });
}

function expected() {
  return JSON.parse(fs.readFileSync(EXPECTED, 'utf8')).images;
}

function norm(name) {
  return String(name).toLowerCase().replace(/device/g, '').replace(/grey/g, 'gray').replace(/[^a-z0-9]/g, '');
}

function field(obj, names, index) {
  if (obj == null) throw new Error('missing field ' + names[0]);
  if (typeof obj === 'object' && !Array.isArray(obj)) {
    for (const k of names) {
      if (obj[k] !== undefined) return obj[k];
    }
    const lower = {};
    for (const k of Object.keys(obj)) lower[k.toLowerCase()] = obj[k];
    for (const k of names) {
      if (lower[k.toLowerCase()] !== undefined) return lower[k.toLowerCase()];
    }
  }
  if (Array.isArray(obj) && index >= 0 && index < obj.length) return obj[index];
  throw new Error('missing field ' + names[0]);
}

function refFacts(ref) {
  if (Array.isArray(ref) && ref.length >= 6) {
    const space = String(ref[5]);
    const n = norm(space);
    const comps = n === 'rgb' ? 3 : n === 'gray' ? 1 : n === 'cmyk' ? 4 : -1;
    return { xref: ref[0], width: ref[2], height: ref[3], space, comps, bpc: ref[4] };
  }
  const xref = field(ref, ['xref', 'Xref'], 0);
  const width = field(ref, ['width', 'Width'], 2);
  const height = field(ref, ['height', 'Height'], 3);
  const bpc = field(ref, ['bpc', 'bits', 'Bits'], 4);
  let space = '';
  try {
    space = String(field(ref, ['colorspace', 'colorspaceName', 'ColorspaceName', 'cs_name']));
  } catch (e) { /* leave empty */ }
  let comps = -1;
  try {
    comps = Number(field(ref, ['components', 'colorspace_code', 'n', 'N']));
  } catch (e) {
    const n = norm(space);
    comps = n === 'rgb' ? 3 : n === 'gray' ? 1 : n === 'cmyk' ? 4 : -1;
  }
  return { xref, width, height, space, comps, bpc };
}

function openCompat() {
  try {
    return new Document(PDF);
  } catch (e) {
    return new Document(null, fs.readFileSync(PDF));
  }
}

if (Document) {
  test('dimensions', () => {
    const want = expected().slice().sort((a, b) => a.width - b.width || a.height - b.height);
    const doc = openCompat();
    try {
      const refs = doc.getPageImages(0);
      expect(refs.length).toBe(want.length);
      expect(refs.length).toBe(2);
      const facts = refs.map(refFacts).sort((a, b) => a.width - b.width || a.height - b.height);
      for (let i = 0; i < want.length; i++) {
        expect(facts[i].width).toBe(want[i].width);
        expect(facts[i].height).toBe(want[i].height);
        expect(norm(facts[i].space)).toBe(norm(want[i].colorspace));
        expect(facts[i].comps).toBe(want[i].components);
        expect(facts[i].bpc).toBe(want[i].bpc);
      }
    } finally {
      if (typeof doc.close === 'function') doc.close();
    }
  });

  test('pixels', () => {
    const want = expected();
    const bySize = new Map(want.map((w) => [`${w.width}x${w.height}`, w]));
    const doc = PdfDocument.open(fs.readFileSync(PDF));
    const images = doc.pageImages(0);
    expect(images).toHaveLength(2);
    for (const info of images) {
      const w = bySize.get(`${info.width}x${info.height}`);
      expect(w).toBeDefined();
      const decoded = doc.decodeImagePixels(info.id);
      expect(decoded.width).toBe(w.width);
      expect(decoded.height).toBe(w.height);
      expect(decoded.components).toBe(w.components);
      const pixels = Buffer.from(decoded.pixels);
      expect(pixels.length).toBe(w.width * w.height * w.components);
      expect(crypto.createHash('sha256').update(pixels).digest('hex')).toBe(w.sha256);
    }
  });
}
