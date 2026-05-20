# PDFluent — Per-Binding Quickstart (canonical)

Canonical first-run guide for every supported binding. Each section is
backed by a **CI-verified example** under `pdfluent-examples/<lang>/`
(Golden Path 7/7 — `benchmarks/runs/ga_100_closure_v3/cookbook_examples/GOLDEN_PATH_REVALIDATION_REPORT.md`),
so the patterns shown here are compiled/run, not prose.

The canonical public API is the **`pdfluent` facade** (RFC 0001 frozen):
`PdfDocument::{open, from_bytes, page_count, text, extract_text, save,
to_bytes, render_page, metadata, form_fields, split_pages, encrypt,
decrypt, validate_pdfa, outlines, annotations, attachments}` + `PdfMerger`.
(The lower-level `pdf_engine` crate is an internal engine, not the public
surface — prefer `pdfluent`.)

> **Publish status (2026-05-20):** packages are **not yet published** to
> public registries. Install lines below show the **development (in-tree)**
> form that works today, and the **published** form that will work after
> the GA release train. This is verified by `audit-all-packages.sh --dry-run`
> at the artifact level; registry install is re-checked at publish.

> **XFA note:** XFA dynamic-form fidelity is a **separate beta track**. The
> non-XFA core (open/parse/text/render/forms/metadata/merge/split/encrypt/
> PDF-A/outlines/annotations/attachments) is what these quickstarts cover.

---

## Rust — `pdfluent`

- **Verified example:** `pdfluent-examples/rust/` (`cargo run`).
- **Install (dev, in-tree):**
  ```toml
  [dependencies]
  pdfluent = { path = "crates/pdfluent" }   # workspace crate during beta
  ```
- **Install (published, post-GA):** `pdfluent = "1"` from crates.io.
- **Minimal first run** (facade lifecycle — open, page count, metadata, text):
  ```rust
  use pdfluent::{Error, PdfDocument};

  fn main() -> Result<(), Error> {
      let doc = PdfDocument::open("invoice.pdf")?;
      println!("Pages   : {}", doc.page_count());
      println!("Version : {:?}", doc.version());
      let meta = doc.metadata();
      println!("Title   : {}", meta.title.as_deref().unwrap_or("(none)"));
      let text = doc.extract_text()?;
      println!("First chars: {}", &text[..text.len().min(120)]);
      Ok(())
  }
  ```
- **License:** `pdfluent::set_license_key(...)` / `pdfluent::license_info()`; Trial mode without a key. See [License UX](#license-flow).
- **Errors:** typed `pdfluent::Error` enum + `ResourceLimitKind` — match, never string-parse.
- **Expected output:** `Pages: N` + title + first text. **Publish-readiness:** dev-path today; crates.io at GA.

## C ABI — `pdf-capi`

- **Verified example:** `pdfluent-examples/c/strict-api/` (`make check`, `-Wall -Wextra -Werror`).
- **Install (dev):** build `crates/pdf-capi` → link `libpdfluent` + `pdfluent.h`. Published form: C-ABI tarball (`scripts/release/package_cabi.sh`).
- **First run:** `pdf_document_open_from_bytes(...)` → `pdf_page_count(...)` → free. Status codes are the typed-error surface (`PdfStatus`).
- **Publish-readiness:** dev/local tarball today.

## WASM / Browser — `pdfluent` wasm package

- **Verified example:** `pdfluent-examples/wasm/strict-ts-edit/` (tsc + Node runtime proof — `EDITOR_HANDOFF_SDK_RUNTIME_PROOF.md`).
- **Install (dev):** `wasm-pack build` the wasm crate; import the generated package. Published form: npm (name finalized at publish — see [feature/consistency notes]).
- **First run:** load the module, open bytes, read page count / text positions / render.
- **Publish-readiness:** dev build verified in Node; **browser load/init timing is a Performance-milestone item (PF-8)**.

## Node — `pdfluent` node binding

- **Verified example:** `pdfluent-examples/node/strict-ts/` (`tsc --strict`).
- **Install (dev):** build `crates/pdf-node` (napi-rs) and depend on it locally. Published form: npm.
- **First run:** `import { PdfDocument } from 'pdfluent'` → `open` → `pageCount` / `extractText`.
- **Errors:** typed exceptions (see [Error UX](#error-ux)). **Publish-readiness:** dev today.

## Python — `pdfluent`

- **Verified example:** `pdfluent-examples/python/main.py` (`python main.py <pdf>`).
- **Install (dev):** `pip install -e ../../crates/pdf-python`. **Published:** `pip install pdfluent` (pinned `1.0.0b7` in the example's `requirements.txt`).
- **First run:**
  ```python
  import pdfluent
  doc = pdfluent.PdfDocument.open("invoice.pdf")
  print("Pages:", doc.page_count())
  print(doc.extract_text()[:120])
  ```
- **Errors:** typed `PdfluentError` hierarchy (falls back to `ValueError`/`RuntimeError` on older builds). **Publish-readiness:** dev/editable today; PyPI at GA.

## .NET — `pdfluent`

- **Verified example:** `pdfluent-examples/dotnet/StrictApi/` (`dotnet build`, `TreatWarningsAsErrors`).
- **Install (dev):** project-reference the built binding. Published: NuGet.
- **First run:** `PdfDocument.Open(path)` → `PageCount` / `ExtractText`.
- **Publish-readiness:** dev today; NuGet at GA.

## Java — `pdfluent`

- **Verified example:** `pdfluent-examples/java/StrictApi/` (`mvn package`).
- **Install (dev):** local Maven install of the built artifact. Published: Maven Central.
- **First run:** `PdfDocument.open(path)` → `pageCount()` / `extractText()`.
- **Publish-readiness:** dev today; Maven Central at GA.

---

## <a id="license-flow"></a>License flow (all bindings)

Trial mode runs without a key (output may be marked). Activate via the
binding's `set_license_key` equivalent or the `PDFLUENT_LICENSE_KEY`
environment variable; query state via `license_info`. See
[../licensing.md](../licensing.md) and `DX_LICENSE_UX_CLOSURE.md`.

## <a id="error-ux"></a>Error UX (all bindings)

Errors are **typed with stable codes** (Rust `Error` enum →
[error_catalogue.md](../error_catalogue.md)); each binding maps them to its
idiomatic exception/status. Never rely on message strings. Cross-binding
mapping proof is a Quality-milestone item (QR-11).

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for install/build, native-lib
load, WASM init, license activation, and malformed/encrypted-PDF issues.
