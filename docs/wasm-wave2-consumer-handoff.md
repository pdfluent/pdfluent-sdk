# Wave 2 consumer-side handoff (HISTORICAL — superseded)

> **Status:** Historical handoff document for Wave 2 (`1.0.0-beta.9`).
> The current canonical wasm package is **`@pdfluent/sdk-wasm @
> 1.0.0-beta.11`** (renamed from `@pdfluent/xfa-wasm`). References below
> to `@pdfluent/xfa-wasm` and `1.0.0-beta.8`/`1.0.0-beta.9` are preserved
> for historical context.

The SDK side of Wave 2 was complete: 13 new methods on `PdfDoc`,
`@pdfluent/xfa-wasm@1.0.0-beta.9` ready (in `pkg/` — not yet published to
npm). This document is what the PDFluent editor app + website needed to do
to consume the new capabilities.

---

## 1. Editor app (separate repo, probably `pdfluent-editor`)

### 1a. `WasmCapabilityRegistry`

Add new fields (or flip existing ones to `true`):

```ts
export interface WasmCapabilityRegistry {
  // ... existing fields ...
  supportsForms: true;        // setFormField, setFormFields
  supportsAnnotations: true;  // addHighlight, addStickyNote, addFreeText
  supportsPageOps: true;      // deletePages, rotatePage, reorderPages, extractPages
  supportsWatermark: true;    // addTextWatermark
  supportsRedaction: true;    // redactRegion, redactSearch
  supportsCompress: true;     // compress
  supportsMerge: true;        // (already true; unchanged)
  // existing imageWatermark, ocr, sign, encrypt remain false
}
```

### 1b. `src/viewer/components/ModeToolbar.tsx`

The `getWiredTools(isTauri)` function decides which toolbar buttons are
active in the browser. Add the new tool IDs to the `base` set (i.e. the
set that's available even when `isTauri === false`):

```ts
const base = new Set<ToolId>([
  // ... existing tools ...
  'delete-pages',
  'rotate-page',
  'reorder-pages',
  'split-pages',
  'fill-form',
  'add-highlight',
  'add-sticky-note',
  'add-free-text',
  'add-watermark',
  'redact-region',
  'redact-search',
  'compress',
]);
```

Per-tool button handlers call the matching `PdfDoc` method. Example for
`add-highlight`:

```ts
async function onAddHighlight(rect: PageRect) {
  const next = doc.addHighlight(
    rect.pageIndex, rect.x, rect.y, rect.width, rect.height,
    /* colorHex */ '#ffeb3b',
  );
  setDocBytes(next);
  setDoc(PdfDoc.open(next));
}
```

### 1c. Per-tool implementation snippets

```ts
// Pages
doc.deletePages(new Uint32Array(selectedPageIndices));
doc.rotatePage(pageIndex, 90);
doc.reorderPages(new Uint32Array(newOrder));
doc.extractPages(new Uint32Array(rangePages));   // → save as new file

// Forms
doc.setFormField('form.name', 'Alice');
doc.setFormFields(JSON.stringify({
  'form.name': 'Alice',
  'form.email': 'alice@example.com',
}));

// Annotations (default color yellow if colorHex omitted)
doc.addHighlight(0, 100, 700, 200, 20, '#ffeb3b');
doc.addStickyNote(0, 50, 750, 'Review this paragraph');
doc.addFreeText(0, 100, 600, 200, 80, 'Annotated by PDFluent');

// Watermark
doc.addTextWatermark('CONCEPT', 0.3);

// Redaction
doc.redactRegion(0, 50, 50, 100, 20);
doc.redactSearch('John Doe');

// Compress
doc.compress();
```

All methods return `Uint8Array` — the new PDF bytes. The caller is
expected to `PdfDoc.open()` the result if further chained edits are
needed (the methods are intentionally non-mutating to keep the WASM
boundary simple).

---

## 2. Website (`~/Documents/pdfluent-website`)

### 2a. `/docs/wasm` or `/docs/browser-sdk` page

Update capability list to match `docs/wasm-capability-matrix.md` from
the SDK repo. Bold the 13 Wave 2 rows.

### 2b. `/download` page

Bump the version chip for `@pdfluent/xfa-wasm` from `1.0.0-beta.8` to
`1.0.0-beta.9` (after the operator publishes).

### 2c. `/llms.txt` and `/llms-full.txt`

Append to the SDK section:

```
PDFluent browser SDK Wave 2 (1.0.0-beta.9):
- Pages: deletePages, rotatePage, reorderPages, extractPages
- Forms: setFormField, setFormFields (AcroForm text fields; Wave 3 `PdfDocMut.setFormField` extends this to all types: text, checkbox, radio, choice)
- Annotations: addHighlight, addStickyNote, addFreeText
- Watermark: addTextWatermark
- Redaction: redactRegion, redactSearch (GDPR-safe permanent removal)
- Compress: stream-level deflate optimisation
- All methods are instance methods on PdfDoc; return Uint8Array of new PDF bytes
- Bundle: 3.8 MB tarball, ~11 MB unpacked
```

### 2d. `public/llms/docs-*.md`

The per-section LLM index files mirror the docs pages. Update any
that describe WASM capabilities or bundle size.

---

## 3. Publish flow (separate wave)

This handoff does NOT publish to npm. The published-ready artefact lives
at:

```
crates/xfa-wasm/pkg/pdfluent-xfa-wasm-1.0.0-beta.9.tgz   (3.8 MB)
```

When the operator wants to ship:

```bash
cd ~/Documents/XFA/crates/xfa-wasm/pkg
npm publish pdfluent-xfa-wasm-1.0.0-beta.9.tgz --access public
```

After publish:
- editor's `package.json` bumps `@pdfluent/xfa-wasm: 1.0.0-beta.9`
- website `/download` chip bumps to `1.0.0-beta.9`
- `scripts/deploy/manual-cloudflare-deploy.sh` to refresh the live docs

---

## 4. Naming note (out of scope for this wave)

The npm package name `@pdfluent/xfa-wasm` is now narrower than the
package's actual surface (27 methods on `PdfDoc`, only a handful are
XFA-specific). A follow-up wave should re-publish under
`@pdfluent/sdk-wasm` or `@pdfluent/wasm` and deprecate the old name.
That work involves coordinated changes across SDK, editor, and website —
intentionally not started here.
