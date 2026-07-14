# @pdfluent/node

Enterprise PDF SDK for Node.js, powered by a native Rust engine via [napi-rs](https://napi.rs).
Supports rendering, text extraction, forms, annotations, redaction, encryption, digital signatures,
and PDF/A compliance validation.

## Installation

```sh
npm install @pdfluent/node
```

Pre-built binaries are provided for:

| Platform | Architecture |
|---|---|
| macOS | x64, arm64 |
| Linux (glibc) | x64, arm64 |
| Linux (musl) | x64 |
| Windows | x64 |

## Quick start

```js
const { PdfDocument } = require('@pdfluent/node');
const fs = require('fs');

const data = fs.readFileSync('document.pdf');
const doc = PdfDocument.open(data);

console.log(`Pages: ${doc.pageCount}`);
console.log(`Title: ${doc.info().title}`);

// Extract text from page 0
const text = doc.extractText(0);
console.log(text);

// Save after mutations
doc.save('output.pdf');
```

### Async (recommended for servers)

```js
const data = await fs.promises.readFile('document.pdf');
const doc = await PdfDocument.openAsync(data);

const text = await doc.extractTextAsync(0);
const render = await doc.renderPageAsync(0, { dpi: 150 });
// render.data — Buffer with RGBA pixels; render.width / render.height
```

### Open from file path

```js
const { openPdf } = require('@pdfluent/node');
const doc = openPdf('/path/to/document.pdf');
```

## Features

- **Rendering** — rasterise pages to RGBA pixels at any DPI, generate thumbnails
- **Text extraction** — plain text per page or structured `TextBlockInfo[]` with coordinates
- **Text search** — returns page indices containing the query string
- **Forms (AcroForm)** — enumerate, read, and fill fields (text/checkbox/radio/choice); hierarchical names; appearance-stream regeneration
- **Annotations** — read existing annotations, add highlight / freetext
- **Redaction** — permanently remove text by search term
- **Encryption** — encrypt with AES-256, decrypt password-protected PDFs
- **Digital signatures** — validate existing signatures
- **Bookmarks** — read the document outline
- **PDF/A compliance** — validate against levels 1a/1b through 3a/3b/3u
- **Async API** — Promise-based worker-thread execution; no event-loop blocking
- **TypeScript** — full `.d.ts` declarations with JSDoc included

## API reference

### `PdfDocument`

#### Factory methods

| Method | Description |
|---|---|
| `PdfDocument.open(data: Buffer)` | Open synchronously from a Buffer |
| `PdfDocument.openAsync(data: Buffer)` | Open on a worker thread |
| `PdfDocument.openWithPassword(data, password)` | Open a password-protected PDF |
| `openPdf(path: string)` | Convenience wrapper — open from file path |

#### Metadata & structure

```js
const info = doc.info(); // { title, author, subject, keywords, creator, producer }
const outline = doc.bookmarks(); // BookmarkItem[]
const geo = doc.pageGeometry(0); // { width, height, rotation }
```

#### Rendering

```js
// Synchronous
const result = doc.renderPage(0, { dpi: 150 });
// result.data — Buffer (RGBA), result.width, result.height

// Async (worker thread)
const result = await doc.renderPageAsync(0, { dpi: 300 });

// All pages in parallel
const pages = await doc.renderAll({ dpi: 72 });

// Thumbnail
const thumb = await doc.thumbnail(0, 256); // max 256 px on longest side
```

`RenderOpts`: `{ dpi?, background?, width?, height? }`

#### Text extraction

```js
const text = doc.extractText(0); // plain string
const blocks = doc.extractTextBlocks(0); // TextBlockInfo[] with x/y/fontSize
const hits = doc.searchText('invoice'); // number[] of page indices
```

#### Forms (AcroForm)

```js
const fields = doc.getFormFields(); // FormFieldInfo[]
// fields[0] → { name, fieldType, value, readOnly }

const value = doc.getFieldValue('Address.Street');

// setFormField supports text, checkbox, radio, and choice fields.
// Hierarchical names ("parent.child") are resolved through /Kids recursion.
// The call updates /V (UTF-16BE+BOM for non-ASCII), per-widget /AS, and
// regenerates the /AP appearance stream so the fill is visible in all viewers
// without /NeedAppearances processing.
// Read-only fields are rejected (throws).
doc.setFormField('Address.Street', '123 Main St');
doc.setFormField('Agree', 'On');        // checkbox — pass the on-state name
doc.setFormField('Country', 'NL');      // radio group — pass the export value
doc.setFormField('Category', 'Option2'); // combo/list — pass the option value

// Multi-select list box: pass an array of option values. Writes /V as an
// array and rebuilds the sorted /I index cache (Acrobat-faithful).
doc.setMultiSelect('Languages', ['EN', 'NL']);
doc.save('filled.pdf');
```

> The pre-`1.0.0-beta.14` names `formFields()` and `setFieldValue()` still work as
> deprecated aliases for `getFormFields()` and `setFormField()`, but will be
> removed in `1.0.0`. Use the canonical names above.

#### Annotations

```js
const annots = doc.annotations(0); // AnnotationInfo[]

doc.addAnnotation(
  0,           // page index
  'highlight', // type: highlight | freetext | note | underline | strikeout | squiggly
  [100, 200, 300, 220], // rect [x0, y0, x1, y1] in PDF user-space points
  'Important'  // optional content
);
doc.save('annotated.pdf');
```

#### Redaction

```js
const report = doc.redactText('John Doe'); // all pages
// report → { matchesFound, areasRedacted, pagesAffected }
doc.save('redacted.pdf');
```

#### Encryption

```js
// Encrypt — saved copy is AES-256 protected; in-memory doc is unchanged
doc.encrypt('protected.pdf', 'secret');

// Decrypt — works on a doc opened with openWithPassword
doc.decrypt('plain.pdf');
```

#### Digital signatures

```js
const sigs = doc.validateSignatures();
// sigs[0] → { status, fieldName, signer, timestamp, reason }
```

#### PDF/A compliance

```js
// Per-document
const report = doc.validatePdfa('2b');
// report → { compliant, errorCount, warningCount, issues[] }

// Or as a module-level function (opens the file internally)
const { validatePdfa } = require('@pdfluent/node');
const report = validatePdfa('document.pdf', '2b');
```

Valid levels: `"1a"`, `"1b"`, `"2a"`, `"2b"`, `"2u"`, `"3a"`, `"3b"`, `"3u"`.

### `PdfPage`

Obtain a page handle with `doc.page(index)`. Provides the same render/text methods scoped to a single page.

```js
const page = doc.page(0);
console.log(`${page.width} x ${page.height} pts`);

const pixels = await page.renderAsync({ dpi: 150 });
const text = page.text();
const blocks = page.textBlocks();
const annots = page.annotations();
```

### Module-level functions

```js
const { openPdf, mergePdfs, validatePdfa } = require('@pdfluent/node');

// Open from path
const doc = openPdf('/path/to/file.pdf');

// Merge multiple PDFs
mergePdfs(['a.pdf', 'b.pdf', 'c.pdf'], 'merged.pdf');

// Validate compliance
const report = validatePdfa('document.pdf', '2b');
console.log(report.compliant, report.errorCount);
```

### Integration with Sharp

```js
const sharp = require('sharp');
const result = doc.renderPage(0, { dpi: 150 });
await sharp(result.data, {
  raw: { width: result.width, height: result.height, channels: 4 }
}).png().toFile('page.png');
```

## License

**PDFluent Commercial License** — free for evaluation; a valid license is required for production use.

- **30-day trial:** <https://pdfluent.com/trial>
- **Pricing:** <https://pdfluent.com/pricing>
- **Commercial terms:** <https://pdfluent.com/terms>

Without a license key the SDK is fully functional; output PDFs carry an evaluation marker in `/Producer` metadata.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **Issues:** <https://pdfluent.com/support>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.
- Built by [Innovation Trigger BV](https://pdfluent.com)
