# XFA WASM SDK

WebAssembly bindings for XFA form processing and PDF analysis in the browser.

## Features

- **XFA Forms**: Parse, calculate, import/export XFA form data
- **PDF Analysis**: Metadata, signatures, PDF/A compliance validation
- **Page Rendering** (feature `render`): Render pages to RGBA pixels for Canvas
- **Annotations** (feature `annotate`): Read and create highlights, sticky notes, free text

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
| `getAnnotations(index)` | Read annotations as JSON (feature: annotate) |
| `PdfDoc.addHighlight(...)` | Add highlight annotation (feature: annotate) |
| `PdfDoc.addStickyNote(...)` | Add sticky note (feature: annotate) |
| `PdfDoc.addFreeText(...)` | Add free text annotation (feature: annotate) |
