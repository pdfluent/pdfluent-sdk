# @xfa-engine/pdf-node

High-performance PDF engine for Node.js, built with Rust and [napi-rs](https://napi.rs).

## Features

- **Rendering** — PDF pages to RGBA pixel buffers at any DPI
- **Text extraction** — Full text and structured text blocks
- **Text search** — Parallel full-text search across all pages
- **Thumbnails** — Quick thumbnail generation
- **Metadata** — Title, author, subject, keywords, creator, producer
- **Bookmarks** — Document outline / table of contents
- **Page geometry** — Dimensions, rotation, boxes (MediaBox, CropBox, etc.)
- **Async API** — Promise-based, runs on worker threads (no event loop blocking)
- **Password support** — Open encrypted PDFs

## Installation

```bash
npm install @xfa-engine/pdf-node
```

Prebuilt binaries are provided for:
- macOS (x64, arm64)
- Linux (x64 glibc, x64 musl, arm64)
- Windows (x64)

## Quick Start

```javascript
const { PdfDocument } = require('@xfa-engine/pdf-node');
const fs = require('fs');

// Open a PDF
const data = fs.readFileSync('document.pdf');
const doc = PdfDocument.open(data);

console.log(`Pages: ${doc.pageCount}`);
console.log(`Title: ${doc.info().title}`);

// Render page 0 at 150 DPI
const result = doc.renderPage(0, { dpi: 150 });
console.log(`Rendered: ${result.width}x${result.height} (${result.data.length} bytes)`);

// Extract text
const text = doc.extractText(0);
console.log(text);
```

## Async API

All heavy operations have async variants that run on worker threads:

```javascript
const { PdfDocument } = require('@xfa-engine/pdf-node');
const fs = require('fs/promises');

async function main() {
  const data = await fs.readFile('document.pdf');
  const doc = await PdfDocument.openAsync(data);

  // Render without blocking the event loop
  const result = await doc.renderPageAsync(0, { dpi: 300 });

  // Extract text on worker thread
  const text = await doc.extractTextAsync(0);

  // Search across all pages (parallel)
  const pages = doc.searchText('invoice');
  console.log(`Found on pages: ${pages}`);
}

main();
```

## Page API

```javascript
const page = doc.page(0);

console.log(`Size: ${page.width} x ${page.height} points`);

const { width, height, rotation } = page.geometry();

// Render this specific page
const pixels = await page.renderAsync({ dpi: 200 });

// Generate thumbnail
const thumb = await page.thumbnail(128);
```

## Integration with Sharp

```javascript
const sharp = require('sharp');

const result = doc.renderPage(0, { dpi: 150 });
await sharp(result.data, {
  raw: { width: result.width, height: result.height, channels: 4 }
})
  .png()
  .toFile('page.png');
```

## Server-Side Rendering (Express)

```javascript
const express = require('express');
const sharp = require('sharp');
const { PdfDocument } = require('@xfa-engine/pdf-node');
const fs = require('fs');

const app = express();

app.get('/preview/:page', async (req, res) => {
  const data = fs.readFileSync('document.pdf');
  const doc = PdfDocument.open(data);
  const result = await doc.renderPageAsync(parseInt(req.params.page), { dpi: 150 });
  const png = await sharp(result.data, {
    raw: { width: result.width, height: result.height, channels: 4 }
  }).png().toBuffer();
  res.type('png').send(png);
});

app.listen(3000);
```

## Password-Protected PDFs

```javascript
const doc = PdfDocument.openWithPassword(data, 'secret');
```

## API Reference

See [index.d.ts](./index.d.ts) for the complete TypeScript API.

## License

MIT
