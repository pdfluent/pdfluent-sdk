# PDFluent 1.0 — deferred items (truth-gaps)

> This is a shared snippet the website should include verbatim (or
> as a pull-quote) on every how-to page that covers a method in
> the table below. It prevents users from building expectations
> the 1.0 SDK deliberately doesn't meet.

## What "deferred" means

A **deferred** item is on the public 1.0 API surface but its
runtime is not implemented yet. Calling it returns a typed
`Error::MissingDependency` — **not** a panic, **not** a silent
no-op. Your code must check the result.

When the runtime lands in a future 1.x release, callers who
already `?`-propagate see a transparent upgrade — the method
starts succeeding. No breaking change.

## Deferred items at 1.0 GA

| Method | Current 1.0 behaviour | Planned |
|---|---|---|
| `PdfDocument::linearize()` | `Err(Error::MissingDependency)` | 1.1 follow-up (#1224). A true linearizer is multi-day work on `pdf-manip`. |
| `PdfDocument::embed_font(font_data, name)` | `Err(Error::MissingDependency)` | 1.1 follow-up (#1224). General-purpose Type0/CIDFont writer. |
| `PdfDocument::add_decoration(variant)` | `Err(Error::MissingDependency)` for every variant | #1223 watermark runtime. All decoration families (watermark, header/footer, page numbers, stamp) route through this entry point. |
| `PdfDocument::add_watermark(text, opts)` | Same — delegates to `add_decoration`. | #1223. |
| `PdfDocument::flatten_forms()` | `Err(Error::MissingDependency)` | #1223. *(Previously this method panicked via `unimplemented!()` in alpha/beta; GA-blocker fixed in PR #1280.)* |

## Limited-behaviour items (runtime works, but with a documented caveat)

These are **not** deferred — they execute — but the current
behaviour falls short of the full contract in a specific way.
Website copy should acknowledge the gap without alarming users.

| Method | Caveat |
|---|---|
| `SaveOptions::with_linearize(true)` | Accepted but currently a no-op at save time. Use it to keep your code forward-compatible. |
| `PdfFormMut::set_checkbox` on kid-widget checkboxes | `/V` is written correctly. The per-kid `/AS` appearance state is **not** synced. Viewers that honour `/AS` may show stale visual state. Opening in a viewer that rebuilds appearances from `/V` (most major PDF viewers do this) works. |
| `PdfFormMut::set_radio` on kid-widget radio groups | Same pattern as above. |
| `EncryptOptions::aes128()` | The algorithm tag is accepted, but the current backend always emits AES-256 state. If your interop contract strictly requires 128-bit encryption, track this as a known deviation. |
| `PdfFormMut` on hierarchical field names | Only top-level `/AcroForm/Fields` entries are addressable. Fields nested under `/Kids` with fully-qualified names like `Address.Street` aren't reachable in 1.0. |

## wasm32 notes

On `wasm32-unknown-unknown`, two methods that work natively return
`Error::UnsupportedOnWasm` instead:

- `PdfDocument::to_docx(path)` — needs `pdf-docx` which pulls in
  a zip + quick-xml stack that doesn't compile on wasm.
- `PdfDocument::to_images(pattern, opts)` — needs `pdf-render`
  which builds on `vello_cpu`, not wasm-ready in 1.0.

Full matrix: [WASM_SUPPORT.md](../../../../WASM_SUPPORT.md).

## For translators

- Every `Error::…` path is a Rust identifier → do not translate.
- Every method name is a Rust identifier → do not translate.
- The category labels ("deferred", "limited-behaviour") MAY be
  translated, but keep the same number of categories.
- PR / issue numbers (`#1224`, `#1280`, etc.) are identifiers →
  keep verbatim.
