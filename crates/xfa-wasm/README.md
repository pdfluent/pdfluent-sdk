# PDFluent browser SDK (WASM)

WebAssembly distribution of the PDFluent PDF engine. Read, edit, annotate,
redact, sign, and validate PDFs (including XFA) entirely in the browser —
zero bytes go to a server.

Published as `@pdfluent/sdk-wasm` on npm. (The crate directory is named
`xfa-wasm` for historical XFA roots — the published npm package covers the
full SDK surface needed for an in-browser PDF editor. Previously known as
`@pdfluent/xfa-wasm`; renamed to `@pdfluent/sdk-wasm`.)

## Features

- **XFA Forms**: Parse, calculate, import/export XFA form data
- **PDF Analysis**: Metadata, signatures, PDF/A compliance validation
- **Page Rendering** (feature `render`): Render pages to RGBA pixels or Canvas2D
- **Page Manipulation**: delete, rotate, reorder, extract, split via re-extract, merge
- **Forms write-back**: set AcroForm field values, save modified PDF bytes
- **Annotations** (feature `annotate`): highlight, sticky note, free text
- **Text watermark**: diagonal text watermark with configurable opacity
- **Redaction**: by region (rectangle) or by search query (GDPR-safe permanent removal)
- **Stream compression**: re-deflate content streams for smaller file size
- **`PdfDocMut`** (Wave 3): stateful editing handle — open once, mutate in
  place, save once. 2.7× faster than the stateless `PdfDoc` chain for
  multi-step edits, identical output bytes.

## Building

```bash
# Install wasm-pack
cargo install wasm-pack

# Build the WASM package
wasm-pack build crates/xfa-wasm --target web

# Without rendering (smaller bundle)
wasm-pack build crates/xfa-wasm --target web -- --no-default-features
```

## Quick Start

### XFA Forms

```js
import init, { XfaEngine } from './pkg/xfa_wasm';

await init();

const engine = XfaEngine.fromFields(JSON.stringify([
  { name: "Name", value: "Alice" },
  { name: "Total", value: "", calculate: "100 + 21" },
]));

engine.runCalculations();
console.log(engine.getFieldValue("form1.Total")); // "121"

const json = engine.exportJson();
engine.importJson('{"fields": {"form1.Name": "Bob"}}');
```

### PDF Analysis

```js
import init, { PdfDoc } from './pkg/xfa_wasm';

await init();

const response = await fetch('document.pdf');
const data = new Uint8Array(await response.arrayBuffer());
const doc = PdfDoc.open(data);

console.log(`Pages: ${doc.pageCount()}`);

// Metadata
const meta = JSON.parse(doc.metadata());
console.log(`Title: ${meta.title}`);

// Signatures
if (doc.hasSignatures()) {
  const sigs = JSON.parse(doc.verifySignatures());
  for (const sig of sigs) {
    console.log(`${sig.signer}: integrity ${sig.structural_integrity}`);
  }
}

// PDF/A validation
const report = JSON.parse(doc.validatePdfA("pdfa2b"));
console.log(`Compliant: ${report.compliant}`);
```

### Page Rendering

```js
const raw = doc.renderPage(0, 1.5); // scale factor
const view = new DataView(raw.buffer);
const w = view.getUint32(0, true);  // little-endian
const h = view.getUint32(4, true);
const pixels = raw.slice(8);
const imageData = new ImageData(new Uint8ClampedArray(pixels), w, h);
ctx.putImageData(imageData, 0, 0);
```

### Annotations

```js
// Read existing annotations
const annots = JSON.parse(doc.getAnnotations(0));
for (const a of annots) {
  console.log(`${a.subtype} at (${a.rect?.x0}, ${a.rect?.y0})`);
}

// Add a highlight (returns new PDF bytes)
const newPdf = PdfDoc.addHighlight(pdfBytes, 0,
  100, 700, 400, 720,   // rect: x0, y0, x1, y1
  1.0, 1.0, 0.0);       // color: yellow RGB

// Add a sticky note
const withNote = PdfDoc.addStickyNote(pdfBytes, 0,
  50, 750, "Review this section");

// Add free text
const withText = PdfDoc.addFreeText(pdfBytes, 0,
  100, 600, 300, 620, "Important!", 12.0);
```

## TypeScript Support

Typed wrappers are provided in `ts/index.ts`:

```ts
import { XfaForms, PdfDocument, FieldDef } from './ts/index';

const fields: FieldDef[] = [
  { name: "Amount", value: "100" },
];
const forms = XfaForms.fromFields(fields);
forms.runCalculations();
const data = forms.exportJson();
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `render` | Yes | Page rendering via pdf-render |
| `annotate` | Yes | Annotation read/write via pdf-annot + lopdf |

Build without optional features for a smaller WASM binary:

```bash
wasm-pack build crates/xfa-wasm --target web -- --no-default-features
```

## API Reference

### XfaEngine

| Method | Description |
|--------|-------------|
| `XfaEngine.fromFields(json)` | Create from JSON field definitions |
| `XfaEngine.fromJson(json)` | Create from exported JSON |
| `runCalculations()` | Execute FormCalc calculate scripts |
| `exportJson()` | Export field values as JSON |
| `exportSchema()` | Export form schema as JSON |
| `importJson(json)` | Import field values from JSON |
| `getFieldValue(path)` | Get field value by SOM path |
| `setFieldValue(path, value)` | Set field value by SOM path |
| `nodeCount()` | Number of form nodes |
| `version()` | Engine version string |

### Licensing

Nothing to activate. There is no licence key, no tier and no output mark
(#226): the WASM build exposes the same capabilities to every caller. PDFluent
is published under the AGPLv3 with a commercial licence as the alternative, and
that alternative is a signed order form — see `docs/licensing.md`.

### PdfDoc

| Method | Description |
|--------|-------------|
| `PdfDoc.open(data)` | Open PDF from Uint8Array |
| `pageCount()` | Number of pages |
| `pageWidth(index)` | Page width in points |
| `pageHeight(index)` | Page height in points |
| `metadata()` | Document metadata as JSON |
| `signatures()` | Signature info as JSON array |
| `hasSignatures()` | Whether document has signatures |
| `verifySignatures()` | Verify signatures, returns JSON |
| `validatePdfA(level)` | PDF/A compliance check |
| `dssInfo()` | Document Security Store info |
| `renderPage(index, scale)` | Render page to RGBA (feature: render) |
| `renderThumbnail(index, maxDim)` | Render thumbnail (feature: render) |
| `renderPageToCanvas(canvas, index, scale)` | Render page directly to a Canvas2D (feature: render) |
| `getAnnotations(index)` | Read annotations as JSON (feature: annotate) |
| `getTextPositions(index)` | Per-glyph text positions as JSON |
| `merge(other)` | Append another PDF, returns merged bytes |
| `flattenXfa()` | Flatten XFA form fields into static PDF content |
| `convertToPdfa(level)` | Convert to PDF/A 1b/2b/3b, returns bytes |

### Page manipulation (Wave 2)

| Method | Description |
|--------|-------------|
| `deletePages(pages: Uint32Array)` | Remove the listed 0-based pages; returns new bytes |
| `rotatePage(pageIndex, degrees)` | Rotate one page by 90/180/270 (or negative); returns new bytes |
| `reorderPages(newOrder: Uint32Array)` | Permute pages; returns new bytes |
| `extractPages(pages: Uint32Array)` | Extract the listed pages into a new PDF (use for split) |

### Edit, annotate, redact, optimise (Wave 2)

| Method | Description |
|--------|-------------|
| `setFormField(path, value)` | Fill a single AcroForm **text** field; updates `/V`, `/AS`, and `/AP`; returns new bytes. For checkbox/radio/choice use `PdfDocMut`. |
| `setFormFields(jsonObject)` | Bulk-fill **text** fields from a JSON `{path: value}` map; returns new bytes |
| `addHighlight(pageIndex, x, y, w, h, colorHex?)` | Highlight annotation (feature: annotate) |
| `addStickyNote(pageIndex, x, y, contents)` | Sticky note annotation (feature: annotate) |
| `addFreeText(pageIndex, x, y, w, h, contents)` | Free-text annotation (feature: annotate) |
| `addTextWatermark(text, opacity)` | Diagonal text watermark on all pages |
| `redactRegion(pageIndex, x, y, w, h)` | Permanently remove content in a rectangle (GDPR-safe) |
| `redactSearch(query)` | Find all literal matches of `query` and redact each |
| `compress()` | Re-deflate content streams; returns optimised bytes |

### PdfDocMut (Wave 3) — **recommended for editor workflows**

`PdfDocMut` is a stateful editing handle. Open once, mutate in place,
save once. The `PdfDoc` class above is great for inspection and
single-shot transformations, but for an editor that applies multiple
operations to the same document, **`PdfDocMut` is dramatically faster**:
9-step sessions go from ~16 parse passes to **1 parse + 1 serialise**
(measured 2.7× wall-clock speedup; bigger on larger documents).

| Method | Description |
|--------|-------------|
| `PdfDocMut.open(bytes)` | Open for editing |
| `pageCount()` | Live page count after any mutations so far |
| `save()` | Serialise current state to `Uint8Array`. Non-consuming — keep editing after |
| `free()` | Release the WASM-side memory deterministically |
| `deletePages(pages)` | In place |
| `rotatePage(pageIndex, degrees)` | In place |
| `reorderPages(newOrder)` | In place |
| `extractPages(pages)` | Returns bytes for a NEW subdocument; current editor unchanged |
| `setFormField(path, value)` | In place — text, checkbox, radio, and choice; updates `/V`, `/AS`, and `/AP` |
| `setFormFields(jsonObject)` | In place, bulk — same full chain for each entry |
| `addHighlight(pageIndex, x, y, w, h, colorHex?)` | In place (feature: annotate) |
| `addStickyNote(pageIndex, x, y, contents)` | In place (feature: annotate) |
| `addFreeText(pageIndex, x, y, w, h, contents)` | In place (feature: annotate) |
| `addTextWatermark(text, opacity)` | In place |
| `redactRegion(pageIndex, x, y, w, h)` | In place (GDPR-safe permanent removal) |
| `redactSearch(query)` | In place |
| `compress()` | In place |

#### Editor flow with PdfDocMut

```js
import init, { PdfDocMut } from '@pdfluent/sdk-wasm';

await init();
const bytes  = new Uint8Array(await (await fetch('/document.pdf')).arrayBuffer());
const editor = PdfDocMut.open(bytes);

// All edits operate on the same internal lopdf::Document — no re-parse.
editor.reorderPages(new Uint32Array([1, 0, 2]));
editor.addTextWatermark('CONCEPT', 0.3);
editor.addHighlight(0, 100, 700, 200, 20, '#ffeb3b');
editor.redactSearch('John Doe');
editor.compress();

// Save once at the end.
const out = editor.save();
editor.free();
const blob = new Blob([out], { type: 'application/pdf' });
```

#### One-shot helpers on PdfDoc (kept for compatibility)

For a workflow with a single mutation, the stateless `PdfDoc` methods
are equally fine. They are kept for backward compatibility but the
recommended path for any multi-step edit is `PdfDocMut`.

```js
import init, { PdfDoc } from '@pdfluent/sdk-wasm';
await init();
const doc = PdfDoc.open(bytes);
const merged = doc.merge(otherBytes);   // single op, one-shot
```

## Bundle size

| Build | Tarball | Unpacked |
|-------|---------|----------|
| 1.0.0-beta.8 (pre-Wave 2) | 3.6 MB | 10.2 MB |
| **1.0.0-beta.9 (Wave 2)** | **3.8 MB** | **~11 MB** |

Wave 2 added 13 new methods for ~0.2 MB of binary growth. Well under the
15 MB hard limit and well under the 5 MB gzipped soft target.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **Pricing:** <https://pdfluent.com/pricing>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.
- Built by [Innovation Trigger BV](https://pdfluent.com)
