# Capability register

**Generated** by `scripts/ci/capability_register.py`. Do not edit by hand —
`sanity:capability-register` regenerates it and fails the build if this file
has drifted from the code. That gate is the entire value: three hand-written
descriptions of this product were four months stale at the same time, and each
one still read as fact.

This is the one place to look before claiming what exists, estimating a build,
or answering "do we have X?". It is current by construction; your memory and
mine are not.

## Summary

- **13 of 13** advertised capabilities are implemented, reachable,
  tested, and covered by a CI job that actually runs the test.
- **78** public methods on `pdfluent::prelude::PdfDocument`, of which
  **4** fail at runtime.
- **3** published crates are
  absent from the facade's dependency graph;
  **13** are compiled in but
  not exposed as API.
- **0** published
  crates have no tests of any kind.
- **0** crates
  have tests that no CI job executes.

## What the states mean

| state | meaning |
|---|---|
| `shipped` | implemented, reachable from the facade, tested, and a CI job runs that test |
| `UNREACHABLE` | built and tested, but a customer using the `pdfluent` crate cannot call it |
| `NO CI JOB` | a test exists and nothing executes it — this reads as tested and is not |
| `UNTESTED` | no test calls it |

`NO CI JOB` deserves the shouting. It is indistinguishable from `shipped` in
every report that counts test files instead of test runs, which is how the
whole workspace suite sat unexecuted for months while every summary said the
features were covered.

## Advertised capabilities

### Merge PDF — `shipped`

*Combine several PDFs into one document in the order you choose*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `xfa-wasm`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdf-node/src/functions.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-capi/src/lib.rs` · `crates/pdf-manip/src/pages.rs` · `crates/pdf-python/src/lib.rs` · `crates/xfa-wasm/src/lib.rs` · `crates/xfa-wasm/tests/export_coverage.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:wasm-binding-smoke` |

### Split PDF — `shipped`

*Divide a document by page range to create separate files*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdf-capi/src/lib.rs` · `crates/pdf-manip/src/pages.rs` · `crates/pdf-manip/src/pdfa_cleanup.rs` · `crates/pdf-manip/src/pdfa_fixups.rs` · `crates/pdf-manip/src/pdfa_fonts.rs` |
| **Run in CI by** | `quality:cargo-test` |

### Organize PDF pages — `shipped`

*Reorder, rotate, or delete pages without leaving the app*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) |
| **Tested by** | `crates/pdf-manip/src/pages.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/merge.rs` |
| **Run in CI by** | `quality:cargo-test` |

### Compress PDF — `shipped`

*Reduce file size*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdf-manip/src/pdfa_colorspace.rs` · `crates/pdf-manip/src/pdfa_fonts.rs` · `crates/pdf-manip/src/unicode_font.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/dx_consolidation.rs` |
| **Run in CI by** | `quality:cargo-test` |

### Password protect PDF — `shipped`

*Set a password and restrict permissions on a document*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/encrypt.rs` · `crates/pdf-node/src/document.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | Python (pdfluent) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-manip/src/encrypt.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdf-node/src/document.rs` · `crates/pdf-python/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Run in CI by** | `quality:cargo-test` |

### Add watermark to PDF — `shipped`

*Add a text watermark across all pages*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdfluent/src/decoration.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/dx_consolidation.rs` · `crates/pdfluent/tests/wasm_surface.rs` |
| **Run in CI by** | `quality:cargo-test` |

### PDF to Word — `shipped`

*Convert PDF to Word*

| | |
|---|---|
| **Implemented in** | `pdf-docx`, `pdfluent` |
| **Defined at** | `crates/pdf-docx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | **Rust only** |
| **Tested by** | `crates/pdf-docx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` · `crates/pdfluent/tests/wasm_surface.rs` |
| **Run in CI by** | `quality:cargo-test` |

### PDF to Excel — `shipped`

*Convert PDF to Excel*

| | |
|---|---|
| **Implemented in** | `pdf-xlsx`, `pdfluent` |
| **Defined at** | `crates/pdf-xlsx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | **Rust only** |
| **Tested by** | `crates/pdf-xlsx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` |
| **Run in CI by** | `quality:cargo-test` |

### PDF to PowerPoint — `shipped`

*Convert PDF to PowerPoint*

| | |
|---|---|
| **Implemented in** | `pdf-pptx`, `pdfluent` |
| **Defined at** | `crates/pdf-pptx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | **Rust only** |
| **Tested by** | `crates/pdf-pptx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` |
| **Run in CI by** | `quality:cargo-test` |

### PDF to image — `shipped`

*Convert PDF to image*

| | |
|---|---|
| **Implemented in** | `pdf-render`, `pdf-engine`, `pdfluent`, `xfa-wasm` |
| **Defined at** | `crates/pdf-engine/src/api.rs` · `crates/pdf-engine/src/document.rs` · `crates/pdfluent/src/document.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-engine/src/document.rs` · `crates/pdf-engine/src/render.rs` · `crates/pdf-engine/tests/self_referencing_xobject_does_not_crash.rs` · `crates/pdf-engine/tests/xfa_hostile_script_does_not_crash.rs` · `crates/pdfluent/src/document.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:wasm-binding-smoke` |

### Redact PDF — `shipped`

*Permanently remove text or regions, not just cover them*

| | |
|---|---|
| **Implemented in** | `pdf-redact`, `pdfluent` |
| **Defined at** | `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · Python (pdfluent) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/ga_security.rs` · `crates/pdfluent/tests/qr15_security_matrix.rs` · `crates/pdfluent/tests/security.rs` |
| **Run in CI by** | `quality:cargo-test` |

### Digitally sign PDF — `shipped`

*PAdES signatures, and verification of existing ones*

| | |
|---|---|
| **Implemented in** | `pdf-sign`, `pdfluent` |
| **Defined at** | `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Node (napi) |
| **Tested by** | `crates/pdf-sign/src/sign.rs` · `crates/pdf-sign/src/signer.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/qr15_security_matrix.rs` · `crates/pdfluent/tests/security.rs` |
| **Run in CI by** | `quality:cargo-test` |

### PDF to PDF/A — `shipped`

*Convert PDF to PDF/A*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdf-compliance`, `pdfluent`, `xfa-wasm` |
| **Defined at** | `crates/pdf-compliance/src/lib.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdfluent/src/document.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-compliance/src/lib.rs` · `crates/pdf-compliance/src/xmp.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdf-manip/tests/content_stream_names_survive.rs` · `crates/pdf-manip/tests/conversion_is_deterministic.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:verapdf-on-our-own-output` · `sanity:wasm-binding-smoke` |

## Gaps worth acting on

Everything below is derived, so this list shortens only when the code changes
-- not when someone decides it is fine.

### Advertised in the SDK, available only in Rust

No binding exports these, so a Python, Node, Java, .NET or WASM customer
cannot call them at all. The Rust tests pass, the facade reaches them, and
the register would still say `shipped` -- which is why this list is
separate from the state column rather than folded into it.

- PDF to Word
- PDF to Excel
- PDF to PowerPoint

### Published crates absent from the facade entirely

Not merely unexposed — not in the dependency graph at all. Each is either a
deliberate split or a capability we imply we ship and never wired up.

`pdf-invoice` · `pdf-ocr` · `pdf-text-format`

## Facade surface

### Methods that exist and fail when called

The dangerous shape: the type system promises them, the runtime refuses, and
a caller cannot discover the gap without running it.

- `add_decoration()`
- `add_watermark()`
- `flatten_forms()`
- `linearize()`

### All public methods

| method | works |
|---|---|
| `add_decoration` | **stub** |
| `add_watermark` | **stub** |
| `annotations` | yes |
| `attachment_bytes` | yes |
| `attachments` | yes |
| `compress` | yes |
| `convert_to_pdfa` | yes |
| `create` | yes |
| `decrypt` | yes |
| `diagnostics` | yes |
| `dimensions` | yes |
| `embed_font` | yes |
| `encrypt` | yes |
| `extract_pages` | yes |
| `extract_text` | yes |
| `find_text` | yes |
| `flatten_annotations` | yes |
| `flatten_forms` | **stub** |
| `form_fields` | yes |
| `form_model` | yes |
| `form_mut` | yes |
| `from_bytes` | yes |
| `from_bytes_with` | yes |
| `from_reader` | yes |
| `has_xfa_form` | yes |
| `insert_image` | yes |
| `linearize` | **stub** |
| `metadata` | yes |
| `metadata_mut` | yes |
| `new` | yes |
| `number` | yes |
| `open` | yes |
| `open_with` | yes |
| `outlines` | yes |
| `page` | yes |
| `page_count` | yes |
| `page_labels` | yes |
| `pages` | yes |
| `parse` | yes |
| `redact` | yes |
| `redact_region` | yes |
| `regenerate_form_appearances` | yes |
| `render_page` | yes |
| `replace_text` | yes |
| `replace_text_matches` | yes |
| `rotate_page` | yes |
| `save` | yes |
| `save_with` | yes |
| `set_outlines` | yes |
| `set_xfa_field_value` | yes |
| `sign` | yes |
| `signatures` | yes |
| `split_pages` | yes |
| `strict_memory_limit` | yes |
| `structure_tree` | yes |
| `subset_fonts` | yes |
| `sync_engine` | yes |
| `take_diagnostics` | yes |
| `text` | yes |
| `text_with_layout` | yes |
| `to_bytes` | yes |
| `to_docx` | yes |
| `to_images` | yes |
| `to_incremental_bytes` | yes |
| `to_pptx` | yes |
| `to_xlsx` | yes |
| `validate_pdfa` | yes |
| `verify_signatures` | yes |
| `version` | yes |
| `with_incremental` | yes |
| `with_license_key` | yes |
| `with_linearize` | yes |
| `with_overwrite` | yes |
| `with_password` | yes |
| `with_processing_limits` | yes |
| `with_repair` | yes |
| `write_to` | yes |
| `xfa_form_model` | yes |

## Crates

`direct` = a customer using `pdfluent` can call it. `internal` = compiled into
the facade and used by the engine, but not exposed as API — a design choice.
`absent` = not in the facade's dependency graph at all, so genuinely not
delivered through it.

| crate | version | in facade | test files | unit tests | run by |
|---|---|---|---|---|---|
| `formcalc-interpreter` | 1.0.0 | internal | 5 | 113 | `quality:cargo-test` |
| `pdf-annot` | 1.0.0 | direct | — | 38 | `quality:cargo-test` |
| `pdf-compliance` | 1.0.0 | direct | — | 92 | `quality:cargo-test` |
| `pdf-docx` | 1.0.0 | direct | — | 24 | `quality:cargo-test` |
| `pdf-engine` | 1.0.0 | direct | 5 | 155 | `quality:cargo-test` |
| `pdf-font` | 1.0.0-beta.5 | internal | — | 117 | `quality:cargo-test` |
| `pdf-interpret` | 0.5.8 | direct | — | 130 | `quality:cargo-test` |
| `pdf-invoice` | 1.0.0 | **absent** | — | 53 | `quality:cargo-test` |
| `pdf-manip` | 1.0.0 | direct | 17 | 288 | `quality:cargo-test` |
| `pdf-ocr` | 1.0.0 | **absent** | 1 | 70 | `quality:cargo-test` |
| `pdf-pptx` | 1.0.0 | direct | — | 14 | `quality:cargo-test` |
| `pdf-redact` | 1.0.0 | direct | 6 | 57 | `quality:cargo-test` |
| `pdf-render` | 1.0.0 | direct | — | 7 | `quality:cargo-test` |
| `pdf-standard-fonts` | 1.0.0 | internal | — | 9 | `quality:cargo-test` |
| `pdf-syntax` | 0.5.6 | direct | — | 233 | `quality:cargo-test` |
| `pdf-text-format` | 1.0.0 | **absent** | — | 21 | `quality:cargo-test` |
| `pdf-xfa` | 1.0.0 | internal | 52 | 392 | `quality:cargo-test` |
| `pdf-xlsx` | 1.0.0 | direct | — | 19 | `quality:cargo-test` |
| `pdfluent` | 1.0.0 | direct | 32 | 24 | `quality:cargo-test` |
| `pdfluent-ccitt` | 0.2.2 | internal | — | 9 | `quality:cargo-test` |
| `pdfluent-cff` | 0.2.1 | internal | — | 26 | `quality:cargo-test` |
| `pdfluent-extract` | 1.0.0 | internal | — | 77 | `quality:cargo-test` |
| `pdfluent-forms` | 1.0.0 | direct | 2 | 70 | `quality:cargo-test` |
| `pdfluent-jbig2` | 0.2.3 | internal | 1 | 12 | `quality:cargo-test` |
| `pdfluent-jpeg2000` | 0.4.0 | internal | — | 12 | `quality:cargo-test` |
| `pdfluent-lopdf` | 0.39.5 | direct | 2 | 165 | `quality:cargo-test` |
| `pdfluent-sign` | 1.0.0 | direct | 1 | 62 | `quality:cargo-test` |
| `xfa-dom-resolver` | 1.0.0 | internal | 1 | 32 | `quality:cargo-test` |
| `xfa-js-sandboxed` | 1.0.0 | internal | 5 | 8 | `quality:cargo-test` |
| `xfa-json` | 1.0.0 | internal | — | 26 | `quality:cargo-test` |
| `xfa-layout-engine` | 1.0.0 | internal | 9 | 123 | `quality:cargo-test` |
| `xfa-license` | 1.0.0 | direct | — | 26 | `quality:cargo-test` |

## Feature flags that enable nothing

Turning one of these on is a no-op, which reads as consent. Some are harmless
(the dependency is unconditional anyway) — check `cargo tree` before assuming
either way.

`signing` · `redaction` · `ocr-tesseract` · `ocr-paddle` · `html-to-pdf` · `docx-export` · `xlsx-export` · `pptx-export` · `xfa-flatten` · `wasm` · `internal-legacy`

## Keeping it honest

- Add a capability to `PROMISES` in `scripts/ci/feature_promises.py` in the same
  change that adds it to the website or the editor. That list is what this
  register walks; a promise made outside it is invisible here.
- If `sanity:capability-register` is red, run the script and commit the result
  in the same change. Read the diff first — it is the news.
- The register proves a test exists and runs. It cannot prove the test asserts
  anything. For that, break the function on purpose and watch the test fail;
  three tests in this repo passed against broken code before anyone did.

