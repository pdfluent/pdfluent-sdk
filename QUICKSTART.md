# PDFluent — Quick Start

Get productive in 5 minutes. Pick your language:

- [Rust](#rust)
- [Python](#python)
- [Node.js](#nodejs)
- [Browser (WebAssembly)](#browser-webassembly)

---

## Rust

**Requirements:** Rust 1.80+

```bash
cargo new my-pdf-app
cd my-pdf-app
cargo add pdfluent
```

```rust
// src/main.rs
use pdfluent::prelude::*;

fn main() -> Result<()> {
    // Open a PDF
    let doc = PdfDocument::open("input.pdf")?;

    // Read page count + extract text
    println!("{} pages", doc.page_count());
    println!("{}", doc.extract_text()?);

    // Fill an AcroForm field
    let mut doc = PdfDocument::open("form.pdf")?;
    doc.form_mut()
        .set_text("Name", "Jane Doe")?;
    doc.save("form-filled.pdf")?;

    // Validate PDF/A compliance
    let report = doc.validate_pdfa(PdfAProfile::A2b)?;
    if report.is_compliant() {
        println!("PDF/A-2B ✓");
    }

    // Render pages to images (PNG/JPEG)
    let report = doc.to_images("page-{n}.png", Default::default())?;
    println!("Saved {} page image(s)", report.paths.len());

    // Merge two PDFs
    let merged = PdfMerger::new()
        .add(PdfDocument::open("a.pdf")?)
        .add(PdfDocument::open("b.pdf")?)
        .build()?;
    merged.save("merged.pdf")?;

    Ok(())
}
```

```bash
cargo run
```

**License note:** Without a license key the SDK runs in evaluation mode — all
features are available; output PDFs carry an evaluation marker in `/Producer`
metadata. For production use obtain a license at <https://pdfluent.com/trial>.

Set the key before opening any document:

```rust
pdfluent::set_license_key("YOUR-KEY-HERE")?;
```

Or via environment variable:

```bash
PDFLUENT_LICENSE_KEY=YOUR-KEY-HERE ./my-pdf-app
```

**More:** <https://pdfluent.com/docs> · [examples/](crates/pdfluent/tests/) · [CHANGELOG](CHANGELOG.md)

---

## Python

**Requirements:** Python ≥ 3.8

```bash
pip install pdfluent
```

```python
from pdfluent import Document, merge_pdfs, validate_pdfa

# Open and inspect
with Document("invoice.pdf") as doc:
    print(f"{doc.page_count} pages — {doc.metadata.title}")

# Extract text (page-by-page)
doc = Document("report.pdf")
for page in doc:
    print(page.extract_text())

# Render to image (requires Pillow: pip install pdfluent[pillow])
img = doc[0].render(dpi=150)
img.save("page-0.png")

# Fill an AcroForm field
doc = Document("form.pdf")
doc.set_form_field("Name", "Jane Doe")
doc.save("form-filled.pdf")

# Redact sensitive text
doc = Document("contract.pdf")
doc.redact_text("Confidential")
doc.save("contract-redacted.pdf")

# PDF/A validation
report = validate_pdfa("archive.pdf")
if report.is_compliant:
    print(f"✓ {report.pdfa_level}")
else:
    for issue in report.issues:
        print(f"[{issue.severity}] {issue.rule}: {issue.message}")

# Merge multiple PDFs
merge_pdfs(["a.pdf", "b.pdf"], "merged.pdf")
```

**Optional extras:**

```bash
pip install pdfluent[pillow]   # PIL Image support → page.render().to_pil()
pip install pdfluent[numpy]    # NumPy array support → page.render().to_numpy()
```

**Platforms:** Linux x86_64/aarch64, macOS x86_64/arm64, Windows x86_64.
Pre-built wheels — no Rust toolchain required.

**Build from source:**

```bash
pip install maturin
git clone https://github.com/pdfluent/examples
cd examples/rust
maturin develop --release
```

---

## Node.js

**Requirements:** Node.js ≥ 18

```bash
npm install @pdfluent/node
```

```js
const fs = require('fs');
const { openPdf, mergePdfs } = require('@pdfluent/node');

// Open from file path
const doc = openPdf('input.pdf');
console.log(`${doc.pageCount} pages`);

// Extract text
console.log(doc.extractText(0));  // page 0

// Fill form field
doc.setFieldValue('Name', 'Jane Doe');
doc.save('form-filled.pdf');

// Render page — returns { data: Buffer (RGBA), width, height }
const render = doc.renderPage(0, { dpi: 150 });
// encode with sharp: await sharp(render.data, { raw: { width: render.width, height: render.height, channels: 4 } }).png().toFile('page-0.png')

// Open from Buffer (e.g. HTTP response or fs.readFileSync)
const { PdfDocument } = require('@pdfluent/node');
const data = fs.readFileSync('other.pdf');
const doc2 = PdfDocument.open(data);

// Merge two PDFs into a new file
mergePdfs(['a.pdf', 'b.pdf'], 'merged.pdf');
```

**TypeScript:**

```ts
import { openPdf } from '@pdfluent/node';

const doc = openPdf('input.pdf');
const text: string = doc.extractText(0);
```

**Platforms:** Linux x86_64/x64-musl/arm64, macOS x86_64/arm64, Windows x86_64.
Native `.node` binaries — no Rust toolchain required.

---

## Browser (WebAssembly)

**Requirements:** A module-capable browser (Chrome 89+, Firefox 89+, Safari 15+)

### Via CDN / download

```html
<!DOCTYPE html>
<html>
<head><title>PDFluent WASM Demo</title></head>
<body>
  <input type="file" id="file" accept=".pdf">
  <canvas id="canvas"></canvas>

  <script type="module">
    import init, { PdfDoc } from './xfa_wasm.js';

    await init();  // load the .wasm module

    document.getElementById('file').addEventListener('change', async (e) => {
      const bytes = new Uint8Array(await e.target.files[0].arrayBuffer());
      const doc = PdfDoc.open(bytes);

      console.log(`${doc.pageCount()} pages`);
      console.log('Text:', doc.text(0));

      // Render page 0 to canvas at 1.5x scale (108 DPI)
      const canvas = document.getElementById('canvas');
      doc.renderPageToCanvas(canvas, 0, 1.5);

      doc.free();
    });
  </script>
</body>
</html>
```

### Via npm (bundler — Vite / webpack)

```bash
npm install @pdfluent/wasm
```

```js
import init, { PdfDoc } from '@pdfluent/wasm';

await init();

const response = await fetch('/your-document.pdf');
const bytes = new Uint8Array(await response.arrayBuffer());
const doc = PdfDoc.open(bytes);

console.log(`${doc.pageCount()} pages`);
const text = doc.text(0);         // extract text from page 0
const metadata = JSON.parse(doc.metadata());

doc.free();
```

**Available WASM APIs** (see `xfa_wasm.d.ts` for full TypeScript types):

| Method | Description |
|---|---|
| `PdfDoc.open(bytes)` | Parse PDF from `Uint8Array` |
| `doc.pageCount()` | Number of pages |
| `doc.text(page)` | Extract plain text from a page |
| `doc.renderPage(page, scale)` | Render to RGBA pixel buffer |
| `doc.renderPageToCanvas(canvas, page, scale)` | Render directly to `<canvas>` |
| `doc.convertToPdfa(level)` | Convert to PDF/A (returns `Uint8Array`) |
| `doc.validatePdfA(level)` | Validate compliance (returns JSON report) |
| `doc.flattenXfa()` | Flatten XFA form to static PDF |
| `doc.merge(otherBytes)` | Merge another PDF |
| `doc.metadata()` | Document metadata as JSON |
| `doc.signatures()` | Digital signature info as JSON |
| `doc.free()` | Release WASM memory (use `using doc = ...` in TS 5.2+) |

**Build WASM from source:**

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build crates/xfa-wasm --target web --release
# Output in crates/xfa-wasm/pkg/
```

---

## Platform Support Matrix

| Platform | Rust SDK | Python | Node.js | WASM |
|---|---|---|---|---|
| Linux x86_64 | ✓ | ✓ wheel | ✓ native | ✓ |
| Linux aarch64 | ✓ | ✓ wheel | ✓ native | ✓ |
| macOS x86_64 (Intel) | ✓ | ✓ wheel | ✓ native | ✓ |
| macOS arm64 (Apple Silicon) | ✓ | ✓ wheel | ✓ native | ✓ |
| Windows x86_64 | ✓ | ✓ wheel | ✓ native | ✓ |
| Browser (WASM) | — | — | — | ✓ |

**Minimum versions:** Rust 1.80 · Python 3.8 · Node.js 18

---

## Common Errors

**`Error: evaluation mode — output marked`**
→ SDK running without a license key. Output is fully functional but `/Producer`
metadata includes an evaluation marker. Get a license at <https://pdfluent.com/trial>.

**`InvalidPdf` / `ParseError`**
→ File is not a valid PDF, is password-protected, or is corrupted. Try
`PdfDocument::open_with(path, OpenOptions::default().password("pw"))`.

**`CapabilityError: feature requires license tier X`**
→ Your license tier does not include this feature. See
[pdfluent.com/pricing](https://pdfluent.com/pricing) for tier details.

**WASM: `RuntimeError: memory access out of bounds`**
→ Calling a WASM method after `doc.free()`. Ensure all method calls complete
before `free()`. Use `using doc = PdfDoc.open(bytes)` (TypeScript 5.2+) for
automatic cleanup.

---

## Links

- **Docs:** <https://pdfluent.com/docs>
- **Trial license:** <https://pdfluent.com/trial>
- **Pricing:** <https://pdfluent.com/pricing>
- **Issues:** <https://github.com/pdfluent/examples/issues>
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)
