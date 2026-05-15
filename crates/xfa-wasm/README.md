# PDFluent browser SDK (WASM)

WebAssembly distribution of the PDFluent PDF engine. Read, edit, annotate,
redact, sign, and validate PDFs (including XFA) entirely in the browser —
zero bytes go to a server.

Published as `@pdfluent/xfa-wasm` on npm. (Crate name reflects historical
XFA roots; the package now covers the full SDK surface needed for an
in-browser PDF editor.)

## Features

- **XFA Forms**: Parse, calculate, import/export XFA form data
- **PDF Analysis**: Metadata, signatures, PDF/A compliance validation
- **Page Rendering** (feature `render`): Render pages to RGBA pixels or Canvas2D
- **Page Manipulation** (Wave 2): delete, rotate, reorder, extract, split via re-extract, merge
- **Forms write-back**: set AcroForm field values, save modified PDF bytes
- **Annotations** (feature `annotate`): highlight, sticky note, free text
- **Text watermark**: diagonal text watermark with configurable opacity
- **Redaction**: by region (rectangle) or by search query (GDPR-safe permanent removal)
- **Stream compression**: re-deflate content streams for smaller file size
- **License activation**: process-global tier activation via key string or file

## Building

```bash
# Install wasm-pack
cargo install wasm-pack

# Build the WASM package
wasm-pack build crates/@pdfluent/wasm --target web

# Without rendering (smaller bundle)
wasm-pack build crates/@pdfluent/wasm --target web -- --no-default-features
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
wasm-pack build crates/@pdfluent/wasm --target web -- --no-default-features
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

### License Activation

The WASM build runs in Trial mode by default. Output produced by the engine
is marked via `/Producer` metadata in Trial. Activate a license to remove
the mark and unlock paid capabilities.

```js
import init, { activateLicenseKey, licenseStatus } from '@pdfluent/xfa-wasm';

await init();
activateLicenseKey('tier:enterprise');

const s = licenseStatus();
console.log(s.tier);            // "Enterprise"
console.log(s.source);          // "Explicit" | "EnvVar" | "Default"
console.log(s.outputIsMarked);  // false
```

**Browser-specific caveats:**

- `activateLicenseFile` is intentionally **not** exposed. Browsers and
  Workers have no synchronous filesystem access. Fetch the key text
  yourself (`await fetch(...).then(r => r.text())`) and pass it to
  `activateLicenseKey`.
- The `PDFLUENT_LICENSE_KEY` environment variable is honoured only when a
  Node host provides it; browsers do not expose process env vars.
- The active tier is **process-global and set-once** within a single WASM
  instance. Activating a second time with a different tier throws; reload
  the page or re-initialise the WASM module to switch tiers.

Invalid keys throw `Error`. The key string is never logged.

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
| `setFormField(path, value)` | Set a single AcroForm text field; returns new bytes |
| `setFormFields(jsonObject)` | Bulk-set form fields from a JSON `{path: value}` map |
| `addHighlight(pageIndex, x, y, w, h, colorHex?)` | Highlight annotation (feature: annotate) |
| `addStickyNote(pageIndex, x, y, contents)` | Sticky note annotation (feature: annotate) |
| `addFreeText(pageIndex, x, y, w, h, contents)` | Free-text annotation (feature: annotate) |
| `addTextWatermark(text, opacity)` | Diagonal text watermark on all pages |
| `redactRegion(pageIndex, x, y, w, h)` | Permanently remove content in a rectangle (GDPR-safe) |
| `redactSearch(query)` | Find all literal matches of `query` and redact each |
| `compress()` | Re-deflate content streams; returns optimised bytes |

### Wave 2 example: editor flow

```js
import init, { PdfDoc } from '@pdfluent/xfa-wasm';

await init();
let bytes = await fetch('/document.pdf').then(r => r.arrayBuffer());
let doc   = PdfDoc.open(new Uint8Array(bytes));

// Reorder pages — page 2 first, then 1, then 3
bytes = doc.reorderPages(new Uint32Array([1, 0, 2]));
doc   = PdfDoc.open(bytes);

// Watermark
bytes = doc.addTextWatermark('CONCEPT', 0.3);
doc   = PdfDoc.open(bytes);

// Highlight on page 0
bytes = doc.addHighlight(0, 100, 700, 200, 20, '#ffeb3b');
doc   = PdfDoc.open(bytes);

// Redact by search
bytes = doc.redactSearch('John Doe');

// Save
const blob = new Blob([bytes], { type: 'application/pdf' });
```

## Bundle size

| Build | Tarball | Unpacked |
|-------|---------|----------|
| 1.0.0-beta.8 (pre-Wave 2) | 3.6 MB | 10.2 MB |
| **1.0.0-beta.9 (Wave 2)** | **3.8 MB** | **~11 MB** |

Wave 2 added 13 new methods for ~0.2 MB of binary growth. Well under the
15 MB hard limit and well under the 5 MB gzipped soft target.
