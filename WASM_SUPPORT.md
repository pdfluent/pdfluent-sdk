# PDFluent — WASM support matrix (1.0)

**Status:** Active for 1.0 GA. Part of the stability contract — see
[`STABILITY.md`](STABILITY.md) §10.
**Last updated:** 2026-04-21 (Slag 2 / Epic 4 / #1233).
**Target triple covered:** `wasm32-unknown-unknown`.

This document enumerates, **per public method on `PdfDocument`**, whether
the method compiles and runs on wasm32. It is derived from the actual
`cfg` gates in `crates/pdfluent/src/document.rs` at master
`696143b4e`, not from intent.

---

## 1. Legend

| Mark | Meaning |
|---|---|
| ✅ **both** | Compiles and runs on both native and wasm32 targets. No `cfg` gate. |
| ⚠️ **native + wasm stub** | Compiles on both; wasm32 path returns `Error::UnsupportedOnWasm { operation }` at runtime so callers get a typed error rather than a link failure. |
| ❌ **native only** | Does not compile on wasm32. Attempting `cargo check --target wasm32-unknown-unknown` will exclude the method entirely. |

A **wasm stub** is always preferred over **native only** when the
method can carry a graceful runtime error — that way downstream
wasm callers see a uniform `Result` contract.

---

## 2. Method matrix

### 2.1 Document lifecycle

| Method | Status | Notes |
|---|---|---|
| `PdfDocument::open` | ✅ both | Filesystem-backed; `wasm32-unknown-unknown` has no std::fs so this surface exists but any non-virtual path errors out cleanly via the normal `Error::Io` path. |
| `PdfDocument::open_with` | ✅ both | Same as `open`. |
| `PdfDocument::from_bytes` | ✅ both | Preferred entry point for wasm. |
| `PdfDocument::from_bytes_with` | ✅ both | Preferred entry point for wasm. |
| `PdfDocument::from_reader` | ✅ both | |
| `PdfDocument::create` | ✅ both | |
| `PdfDocument::save` / `save_with` | ✅ both | Writes via `std::fs`; on wasm32 the write will fail with `Error::Io` — use `to_bytes` / `write_to` for wasm. |
| `PdfDocument::to_bytes` | ✅ both | |
| `PdfDocument::write_to` | ✅ both | |

### 2.2 Inspection + text

| Method | Status | Notes |
|---|---|---|
| `page_count` | ✅ both | |
| `version` | ✅ both | |
| `text` | ✅ both | |
| `text_with_layout` | ✅ both | |
| `page(n)` / `pages()` | ✅ both | |
| `metadata` / `metadata_mut` | ✅ both | |

### 2.3 Forms

| Method | Status | Notes |
|---|---|---|
| `form_fields` | ✅ both | |
| `form_mut` + setters | ✅ both | set_text / set_checkbox / set_radio / set_dropdown are wasm-clean. |
| `flatten_forms` | ✅ both (panics) | §3.3 of STABILITY.md — currently panics via `unimplemented!()` on every target. GA-blocker to migrate to `Error::MissingDependency`. |

### 2.4 Decoration

| Method | Status | Notes |
|---|---|---|
| `add_decoration` / `add_watermark` | ✅ both | Both targets currently return `Error::MissingDependency` — watermark runtime tracked on #1223. |

### 2.5 Parity methods (Epic 3 #1224 / 3C-2)

| Method | Status | Notes |
|---|---|---|
| `to_docx(path)` | ⚠️ native + wasm stub | Native: routes to `pdf_docx::convert_pdf_bytes_to_docx`. Wasm: `Error::UnsupportedOnWasm { operation: "to_docx" }`. `pdf-docx` pulls in `zip` + `quick-xml` stacks that don't build on wasm32 today. |
| `to_images(pattern, opts)` | ⚠️ native + wasm stub | Native: routes to `pdf_engine::PdfDocument::render_page` + `png` / `image`. Wasm: `Error::UnsupportedOnWasm { operation: "to_images" }`. `pdf-render` builds on `vello_cpu` which isn't wasm32-ready in 1.0. |
| `compress(opts)` | ✅ both | Pure lopdf / pdf-manip transforms. |
| `linearize()` | ✅ both | Returns `Error::MissingDependency` on every target (deferred — see STABILITY.md §3.3). |
| `subset_fonts()` | ✅ both | Pure lopdf transform. |
| `embed_font(data, name)` | ✅ both | Returns `Error::MissingDependency` on every target (deferred). |
| `insert_image(img)` | ✅ both | lopdf-only path; pulls in `image` for PNG decode but the crate's default features are wasm-clean enough for this path. |

### 2.6 Page operations

| Method | Status | Notes |
|---|---|---|
| `rotate_page` | ✅ both | |
| `split_pages` | ✅ both | |
| `extract_pages` | ✅ both | |

### 2.7 Security

| Method | Status | Notes |
|---|---|---|
| `encrypt` / `decrypt` | ✅ both | `pdf-manip` encryption uses `aes` + `cbc` + `sha2` — all wasm-safe with the workspace's `getrandom` `js` feature setup. |
| `redact` / `redact_region` | ✅ both | |

### 2.8 Signing

| Method | Status | Notes |
|---|---|---|
| `sign` | ✅ both | `pdf-sign` depends on `rsa` + `sha2` + `getrandom` (`js`-feature on wasm) — all wasm-safe. |
| `signatures` | ✅ both | |
| `verify_signatures` | ✅ both | |

### 2.9 Merging

| Type | Status | Notes |
|---|---|---|
| `PdfMerger` + methods | ✅ both | |

### 2.10 License provisioning

| Method | Status | Notes |
|---|---|---|
| `license::set_license_key` | ✅ both | |
| `license::license_info` | ✅ both | |

---

## 3. `cfg`-gate audit

The authoritative list of `cfg(target_arch = "wasm32")` gates in the
`pdfluent` facade. Any future wasm-specific divergence must live
behind one of these gates so this matrix stays truthful.

| File | Line | Kind | Method |
|---|---|---|---|
| `document.rs` | 579 | `#[cfg(not(target_arch = "wasm32"))]` | `to_docx` (native impl) |
| `document.rs` | 593 | `#[cfg(target_arch = "wasm32")]` | `to_docx` (wasm stub) |
| `document.rs` | 611 | `#[cfg(not(target_arch = "wasm32"))]` | `to_images` (native impl) |
| `document.rs` | 656 | `#[cfg(target_arch = "wasm32")]` | `to_images` (wasm stub) |
| `document.rs` | 1177, 1216, 1241, 1270 | `#[cfg(not(target_arch = "wasm32"))]` | private encoder helpers (`build_image_path`, `encode_image`, `encode_png`, `encode_jpeg`) |

No wasm-specific code outside these gates. **If a future PR adds
native-only functionality without a wasm stub, `cargo check --target
wasm32-unknown-unknown -p pdfluent` will pass only because the code
is excluded. The author MUST either:**

1. Add a `#[cfg(target_arch = "wasm32")]` companion that returns
   `Error::UnsupportedOnWasm`, **or**
2. Accept that the method disappears from the wasm32 surface, and
   add a new row marked ❌ **native only** to §2 in the same PR.

No silent exclusions.

---

## 4. Cargo dependency split

Wasm-incompatible transitive deps are quarantined to
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` in
`crates/pdfluent/Cargo.toml`:

| Dep | Native-only reason |
|---|---|
| `pdf-docx` | `zip` + `quick-xml` stacks not wasm32-ready. |
| `pdf-render` | `vello_cpu` not wasm32-ready in 1.0. |
| `png` | Pulled in for `to_images` native encoding path. |
| `image` | `jpeg` feature set for `to_images`. |

Wasm-required workaround:

- `getrandom = { features = ["js"] }` under
  `[target.'cfg(all(target_arch = "wasm32", target_os = "unknown"))'.dependencies]`.
  `pdf-sign` / `pdf-redact` pull `getrandom` via `rand`; the `js`
  feature routes to the browser CSRNG.

---

## 5. Runtime probe

A `tests/wasm_surface.rs` integration test asserts, via compile-time
checks, that:

- The wasm stubs for `to_docx` and `to_images` compile on wasm32 and
  return `Error::UnsupportedOnWasm`.
- The native-only private helpers are gated so their absence on
  wasm32 doesn't trigger a dead-code or missing-symbol error.

Run locally:

```bash
cargo check --target wasm32-unknown-unknown -p pdfluent
cargo test -p pdfluent --test wasm_surface
```

---

## 6. What's explicitly NOT promised

- **PDF/A validation on wasm.** `pdf-compliance` uses filesystem
  fonts for profile checks. Works on wasm today because the fonts
  are bundled, but not performance-tuned.
- **OCR on wasm.** `pdf-ocr` needs Tesseract or PaddleOCR native
  libraries — not available on wasm32 at 1.0. OCR features are
  off by default (see `ocr-tesseract` / `ocr-paddle` Cargo
  features).
- **Bindings on wasm.** `pdf-capi` / `pdf-java` / `pdf-node` /
  `pdf-python` bindings are native-only by design. The
  `xfa-wasm` crate is the dedicated wasm binding surface.

---

## 7. Dropping wasm32 support

Dropping `wasm32-unknown-unknown` from the support matrix is a
**MAJOR** change per STABILITY.md §2.1 / §10. Reducing support
(e.g. changing ⚠️ → ❌ on more methods) is **MINOR** when the
`UnsupportedOnWasm` stub is kept — the runtime contract stays
consistent — and **MAJOR** when the method is removed from the
wasm32 surface entirely.

---

## 8. Memory lifecycle

`PdfDoc` and `XfaEngine` are Rust structs exposed via wasm-bindgen. Each instance
allocates memory on the WASM heap. The JavaScript garbage collector does **not** free
WASM heap memory automatically.

### 8.1 Options

**Option A — `using` keyword (TypeScript 5.2+ / ES2026)**

```ts
// tsconfig: "lib": ["ES2022", "ESNext.Disposable"]
using doc = PdfDoc.open(bytes);
// doc.free() is called automatically when the block exits
```

**Option B — explicit `free()`**

```ts
const doc = PdfDoc.open(bytes);
try {
  const count = doc.pageCount();
} finally {
  doc.free(); // must always be called
}
```

### 8.2 Contract

| Behaviour | Guarantee |
|---|---|
| After `free()`, all method calls throw | `OPERATION_FAILED` |
| Calling `free()` twice | Safe — no-op |
| `[Symbol.dispose]()` | Alias for `free()`; called by `using` |

### 8.3 Worker pattern

In a Worker, the WASM module is initialised per-thread. The lifecycle contract is
unchanged: call `free()` (or `using`) before the Worker exits.

---

## 9. Error model

All fallible WASM methods throw an `XfaWasmError` — a standard `Error` (name:
`"XfaWasmError"`) with three additional properties:

| Property | Type | Description |
|---|---|---|
| `message` | `string` | Human-readable description (inherited from `Error`) |
| `code` | `string` | Stable `SCREAMING_SNAKE_CASE` identifier — use for programmatic dispatch |
| `help` | `string` | Actionable hint for the developer (may be empty) |
| `docsUrl` | `string` | Deep-link to `https://docs.pdfluent.dev/errors/<slug>` |

### 9.1 Catching errors

```ts
try {
  using doc = PdfDoc.open(bytes);
} catch (e: unknown) {
  if (e instanceof Error && 'code' in e) {
    const err = e as XfaWasmError; // see pkg-types/xfa_wasm.augment.d.ts
    switch (err.code) {
      case 'INVALID_PDF':
        showUserMessage('File is not a valid PDF');
        break;
      case 'PAGE_OUT_OF_RANGE':
        showUserMessage('Page does not exist');
        break;
      default:
        console.error(`[${err.code}] ${err.message}\nHelp: ${err.help}`);
    }
  }
}
```

### 9.2 Error code catalogue (WASM binding layer)

| Code | Thrown by |
|---|---|
| `INVALID_PDF` | `PdfDoc.open` — bytes are not a valid PDF |
| `PAGE_OUT_OF_RANGE` | Any page-indexed method |
| `TEXT_EXTRACT_FAILED` | `getTextPositions` |
| `XFA_FLATTEN_FAILED` | `flattenXfa` |
| `INVALID_ARGUMENT` | `validatePdfA`, `convertToPdfa` |
| `INVALID_JSON` | `XfaEngine.fromFields`, `fromJson`, `importJson` |
| `FORMCALC_ERROR` | `XfaEngine.runCalculations` |
| `MERGE_FAILED` | `PdfDoc.merge` |
| `RENDER_ERROR` | `renderPage`, `renderPageToCanvas`, canvas API failures |
| `RENDER_FALLBACK` | `renderPageToCanvasVector` — unsupported PDF feature |
| `OPERATION_FAILED` | Internal operations (annotation builder, lopdf) |
| `SERIALIZE_ERROR` | Internal JSON serialisation failures |
| `PDFA_CLEANUP_FAILED` | `convertToPdfa` — cleanup step |
| `COLORSPACE_ERROR` | `convertToPdfa` — colorspace normalisation |
| `XMP_REPAIR_FAILED` | `convertToPdfa` — XMP metadata repair |

Engine-level errors (from `pdf-engine`) carry their `PdfError::code()` directly —
consult `https://docs.pdfluent.dev/errors/<code-in-kebab-case>` for full descriptions.

### 9.3 TypeScript declarations

The hand-maintained file `crates/xfa-wasm/pkg-types/xfa_wasm.augment.d.ts` declares
the `XfaWasmError` class and the `WasmLifecycle` interface. Projects that need strict
error types should include this file via `tsconfig.json`:

```json
{
  "compilerOptions": {
    "paths": {
      "@pdfluent/sdk-wasm/augment": ["node_modules/@pdfluent/sdk-wasm/pkg-types/xfa_wasm.augment"]
    }
  }
}
```

---

## Appendix A — History

| Date | Change |
|---|---|
| 2026-04-21 | Initial matrix at 1.0 GA prep (Epic 4 #1233). Matches master `696143b4e`. Follows Slag 1 merges (capability enforcement #1261, error system #1262, form mutation #1264, parity methods #1269, DX consolidation #1270). |
| 2026-05-16 | C3 DX audit: added §8 (memory lifecycle — `free()` / `using`) and §9 (error model — `XfaWasmError`, `code` property, full code catalogue). All fallible WASM methods now throw `XfaWasmError` with machine-inspectable `code`, `help`, `docsUrl`. Updated: `crates/xfa-wasm/src/lib.rs`, `pkg-types/xfa_wasm.augment.d.ts`. |
