// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Plain embedded-image API on `PdfDocument`.
 *
 * `pageImages` lists a page's stored images, `extractImageData` returns
 * one image's encoded bytes with header facts, and `decodeImagePixels`
 * decodes to raw samples. The fixture is built in the suite: one page
 * holding a raw 2x2 grey image, so the known pixels below are the
 * samples the builder wrote.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/plain_images.test.js
 */

const GRAY_SAMPLES = Buffer.from([0, 128, 192, 255]);

function grayImagePdf() {
  const content = Buffer.from('q\nQ\n');
  const objects = [
    Buffer.from('<< /Type /Catalog /Pages 2 0 R >>'),
    Buffer.from('<< /Type /Pages /Kids [3 0 R] /Count 1 >>'),
    Buffer.from(
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources 5 0 R >>'
    ),
    Buffer.concat([
      Buffer.from(`<< /Length ${content.length} >>\nstream\n`),
      content,
      Buffer.from('\nendstream'),
    ]),
    Buffer.from('<< /XObject << /G0 6 0 R >> >>'),
    Buffer.concat([
      Buffer.from(
        `<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length ${GRAY_SAMPLES.length} >>\nstream\n`
      ),
      GRAY_SAMPLES,
      Buffer.from('\nendstream'),
    ]),
  ];
  const parts = [Buffer.from('%PDF-1.7\n')];
  const offsets = [];
  objects.forEach((body, i) => {
    offsets.push(Buffer.concat(parts).length);
    parts.push(Buffer.concat([Buffer.from(`${i + 1} 0 obj\n`), body, Buffer.from('\nendobj\n')]));
  });
  const xrefAt = Buffer.concat(parts).length;
  parts.push(Buffer.from(`xref\n0 ${objects.length + 1}\n`));
  parts.push(Buffer.from('0000000000 65535 f \n'));
  for (const offset of offsets) {
    parts.push(Buffer.from(`${String(offset).padStart(10, '0')} 00000 n \n`));
  }
  parts.push(
    Buffer.from(
      `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefAt}\n%%EOF\n`
    )
  );
  return Buffer.concat(parts);
}

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — image extraction not checked\n');
  describe.skip('image extraction (native module not built)', () => {
    test('placeholder', () => {});
  });
}

if (PdfDocument) {
  const open = () => PdfDocument.open(grayImagePdf());

  test('pageImages lists the stored image', () => {
    const images = open().pageImages(0);

    expect(images).toHaveLength(1);
    const image = images[0];
    expect(image.width).toBe(2);
    expect(image.height).toBe(2);
    expect(image.bitsPerComponent).toBe(8);
    expect(image.components).toBe(1);
    expect(image.colorSpace).toBe('DeviceGray');
    expect(image.name).toBe('G0');
    expect(image.hasMask).toBe(false);
    expect(typeof image.id).toBe('number');
  });

  test('pageImages past the last page carries the range code', () => {
    try {
      open().pageImages(999);
      throw new Error('expected pageImages to throw');
    } catch (e) {
      expect(e.code).toBe('E-PAGE-OUT-OF-RANGE');
    }
  });

  test('extractImageData returns png bytes with header', () => {
    const doc = open();
    const id = doc.pageImages(0)[0].id;
    const image = doc.extractImageData(id);

    expect(image.width).toBe(2);
    expect(image.height).toBe(2);
    expect(image.encoding).toBe('png');
    expect([...image.data.subarray(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
    expect(image.masked).toBe(false);
  });

  test('extractImageData rejects an unknown id', () => {
    expect(() => open().extractImageData(9999)).toThrow();
  });

  test('decodeImagePixels returns the fixture samples', () => {
    const doc = open();
    const id = doc.pageImages(0)[0].id;
    const image = doc.decodeImagePixels(id);

    expect(image.width).toBe(2);
    expect(image.height).toBe(2);
    expect(image.components).toBe(1);
    expect(image.colorSpace).toBe('DeviceGray');
    expect(Buffer.from(image.pixels).equals(GRAY_SAMPLES)).toBe(true);
  });

  test('decodeImagePixels rejects an unknown id', () => {
    expect(() => open().decodeImagePixels(9999)).toThrow();
  });
}
