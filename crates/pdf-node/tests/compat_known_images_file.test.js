// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Compat known-images as encoded files, decoded independently (#636).
 *
 * `Document#extractImage` over fixtures/known-images.pdf returns the image
 * FILE (header facts plus encoded bytes), checked against
 * fixtures/known-images.expected.json: `ext` matches the payload's magic
 * bytes, and the payload decoded with pngjs (third party, not PDFluent)
 * yields the expected dimensions, components, and decoded-pixel sha256. A
 * lossy (JPEG) payload cannot match a pixel sha by construction, so it pins
 * only magic bytes, dimensions, and components.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/compat_known_images_file.test.js
 */

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const PDF = path.join(FIXTURES, 'known-images.pdf');
const EXPECTED = path.join(FIXTURES, 'known-images.expected.json');

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const JPEG_MAGIC = Buffer.from([0xff, 0xd8, 0xff]);

let Document;
try {
  ({ Document } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — compat known-images file not checked\n');
  describe.skip('compat known-images file (native module not built)', () => {
    test('placeholder', () => {});
  });
}

let PNG;
try {
  ({ PNG } = require('pngjs'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): pngjs devDependency not installed — compat known-images file not checked\n');
  describe.skip('compat known-images file (pngjs not installed)', () => {
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

function openCompat() {
  try {
    return new Document(PDF);
  } catch (e) {
    return new Document(null, fs.readFileSync(PDF));
  }
}

// Declared components from the IHDR color type (file offset 25): 0 is
// grey (1), 2 is truecolour (3). pngjs always expands samples to RGBA,
// so the count comes from the header byte, the samples from the decode.
function pngComponents(file) {
  const colorType = file[25];
  if (colorType === 0) return 1;
  if (colorType === 2) return 3;
  throw new Error(`unexpected PNG color type for this fixture: ${colorType}`);
}

function pngSamples(png, components) {
  const pixels = png.width * png.height;
  const out = Buffer.alloc(pixels * components);
  for (let i = 0; i < pixels; i++) {
    if (components === 1) {
      out[i] = png.data[i * 4];
    } else {
      out[i * 3] = png.data[i * 4];
      out[i * 3 + 1] = png.data[i * 4 + 1];
      out[i * 3 + 2] = png.data[i * 4 + 2];
    }
  }
  return out;
}

if (Document && PNG) {
  test('extractImage returns the expected encoded file', () => {
    const want = expected();
    const bySize = new Map(want.map((w) => [`${w.width}x${w.height}`, w]));
    const doc = openCompat();
    try {
      const refs = doc.getPageImages(0);
      expect(refs.length).toBe(2);
      for (const ref of refs) {
        const [xref, , width, height] = ref;
        const w = bySize.get(`${width}x${height}`);
        expect(w).toBeDefined();
        const payload = doc.extractImage(xref);
        expect(['png', 'jpeg']).toContain(payload.ext);
        expect(payload.width).toBe(w.width);
        expect(payload.height).toBe(w.height);
        expect(payload.colorspace).toBe(w.components);
        expect(norm(payload['cs-name'])).toBe(norm(w.colorspace));
        expect(payload.bpc).toBe(w.bpc);
        const file = Buffer.from(payload.image);
        if (payload.ext === 'png') {
          expect(file.subarray(0, 8).equals(PNG_MAGIC)).toBe(true);
          const png = PNG.sync.read(file);
          expect(png.width).toBe(w.width);
          expect(png.height).toBe(w.height);
          const components = pngComponents(file);
          expect(components).toBe(w.components);
          const samples = pngSamples(png, components);
          expect(samples.length).toBe(w.width * w.height * w.components);
          expect(crypto.createHash('sha256').update(samples).digest('hex')).toBe(w.sha256);
        } else {
          expect(file.subarray(0, 3).equals(JPEG_MAGIC)).toBe(true);
        }
      }
    } finally {
      if (typeof doc.close === 'function') doc.close();
    }
  });
}
