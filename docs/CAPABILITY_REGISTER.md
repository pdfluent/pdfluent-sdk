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

- **14 of 14** advertised capabilities are implemented, reachable,
  tested, and covered by a CI job that actually runs the test.
- **90** public methods on `pdfluent::prelude::PdfDocument`, of which
  **1** fail at runtime.
- **3** published crates are
  absent from the facade's dependency graph;
  **16** are compiled in but
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
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdf-node/src/functions.rs` · `crates/pdfluent/src/compat/text.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-capi/src/lib.rs` · `crates/pdf-manip/src/pages.rs` · `crates/pdf-python/src/lib.rs` · `crates/pdfluent/src/compat/text.rs` · `crates/pdfluent/tests/incremental_trailer_inheritance.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:wasm-binding-smoke` |
| **CLI** | `pdfluent merge` |

### Split PDF — `shipped`

*Divide a document by page range to create separate files*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdf-capi/src/lib.rs` · `crates/pdf-capi/tests/plain_known_images.rs` · `crates/pdf-manip/src/cmap_codes.rs` · `crates/pdf-manip/src/pages.rs` · `crates/pdf-manip/src/pdfa_cleanup.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent split` |

### Organize PDF pages — `shipped`

*Reorder, rotate, or delete pages without leaving the app*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/pages.rs` · `crates/pdf-node/src/compat/edit.rs` · `crates/pdfluent/src/compat/edit.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) |
| **Tested by** | `crates/pdf-manip/src/pages.rs` · `crates/pdfluent/src/compat/edit.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/merge.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent pages` |

### Compress PDF — `shipped`

*Reduce file size*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdf-manip/src/cff_append.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdf-manip/src/pdfa_colorspace.rs` · `crates/pdf-manip/src/pdfa_fonts.rs` · `crates/pdf-manip/src/pdfa_subset.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent compress` |

### Password protect PDF — `shipped`

*Set a password and restrict permissions on a document*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-manip/src/encrypt.rs` · `crates/pdf-node/src/document.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | Python (pdfluent) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-manip/src/encrypt.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdf-node/src/document.rs` · `crates/pdf-python/src/compat/document.rs` · `crates/pdf-python/src/lib.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent protect` · `pdfluent unprotect` |

### Add watermark to PDF — `shipped`

*Add a text watermark across all pages*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdfluent`, `pdf-capi`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdfluent/src/decoration.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) |
| **Tested by** | `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/dx_consolidation.rs` · `crates/pdfluent/tests/wasm_surface.rs` · `crates/pdfluent/tests/watermark.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent watermark` |

### PDF to Word — `shipped`

*Convert PDF to Word*

| | |
|---|---|
| **Implemented in** | `pdf-docx`, `pdfluent` |
| **Defined at** | `crates/pdf-docx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Node (napi) |
| **Tested by** | `crates/pdf-docx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` · `crates/pdfluent/tests/wasm_surface.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent convert --to docx` |

### PDF to Excel — `shipped`

*Convert PDF to Excel*

| | |
|---|---|
| **Implemented in** | `pdf-xlsx`, `pdfluent` |
| **Defined at** | `crates/pdf-xlsx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Node (napi) |
| **Tested by** | `crates/pdf-xlsx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent convert --to xlsx` |

### PDF to PowerPoint — `shipped`

*Convert PDF to PowerPoint*

| | |
|---|---|
| **Implemented in** | `pdf-pptx`, `pdfluent` |
| **Defined at** | `crates/pdf-pptx/src/lib.rs` · `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Node (napi) |
| **Tested by** | `crates/pdf-pptx/src/lib.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/parity_methods.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent convert --to pptx` |

### PDF to image — `shipped`

*Convert PDF to image*

| | |
|---|---|
| **Implemented in** | `pdf-render`, `pdf-engine`, `pdfluent`, `xfa-wasm` |
| **Defined at** | `crates/pdf-engine/src/api.rs` · `crates/pdf-engine/src/document.rs` · `crates/pdfluent/src/document.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-engine/src/document.rs` · `crates/pdf-engine/src/layout.rs` · `crates/pdf-engine/src/learned/tables_tatr.rs` · `crates/pdf-engine/src/render.rs` · `crates/pdf-engine/src/vision.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:wasm-binding-smoke` |
| **CLI** | `pdfluent convert --to png` · `pdfluent convert --to jpeg` |

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
| **CLI** | `pdfluent redact` |

### Digitally sign PDF — `shipped`

*PAdES signatures, and verification of existing ones*

| | |
|---|---|
| **Implemented in** | `pdf-sign`, `pdfluent` |
| **Defined at** | `crates/pdfluent/src/document.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Node (napi) |
| **Tested by** | `crates/pdf-sign/src/sign.rs` · `crates/pdf-sign/src/signer.rs` · `crates/pdfluent/src/document.rs` · `crates/pdfluent/tests/every_fixture_is_what_its_name_says.rs` · `crates/pdfluent/tests/pades_profiles_produce_what_they_name.rs` |
| **Run in CI by** | `quality:cargo-test` |
| **CLI** | `pdfluent sign` |

### PDF to PDF/A — `shipped`

*Convert PDF to PDF/A*

| | |
|---|---|
| **Implemented in** | `pdf-manip`, `pdf-compliance`, `pdfluent`, `xfa-wasm` |
| **Defined at** | `crates/pdf-compliance/src/lib.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdfluent/src/document.rs` · `crates/xfa-wasm/src/lib.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-compliance/src/lib.rs` · `crates/pdf-compliance/src/xmp.rs` · `crates/pdf-manip/src/pdfa.rs` · `crates/pdf-manip/src/pdfa_fixups.rs` · `crates/pdf-manip/tests/a_face_that_cannot_draw_the_declared_glyphs_is_the_wrong_face.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:verapdf-on-our-own-output` · `sanity:wasm-binding-smoke` |
| **CLI** | `pdfluent convert --to pdfa` · `pdfluent validate --pdfa` |
| **Conformance levels measured** | PDF/A-1b · PDF/A-2b · PDF/A-2u (engine only) · PDF/A-3b · PDF/A-3u (engine only) |

### Structured extraction — `shipped`

*Extract a document as markdown or as blocks -- headings, lists, tables and reading order -- for RAG and conversion*

| | |
|---|---|
| **Implemented in** | `pdf-engine`, `pdfluent`, `pdf-capi`, `xfa-wasm`, `pdf-node`, `pdf-python` |
| **Defined at** | `crates/pdf-engine/src/document.rs` · `crates/pdf-engine/src/json.rs` · `crates/pdf-engine/src/learned/tables.rs` · `crates/pdf-engine/src/markdown.rs` |
| **Reachable from facade** | yes |
| **Exposed in bindings** | WASM (@pdfluent/sdk-wasm) · Python (pdfluent) · C ABI (voedt .NET/Java/Node) · Java (JNI) · Node (napi) |
| **Tested by** | `crates/pdf-capi/src/lib.rs` · `crates/pdf-engine/src/balanced.rs` · `crates/pdf-engine/src/blocks.rs` · `crates/pdf-engine/src/document.rs` · `crates/pdf-engine/src/json.rs` |
| **Run in CI by** | `quality:cargo-test` · `sanity:wasm-binding-smoke` |
| **CLI** | `pdfluent extract` |

## Gaps worth acting on

Everything below is derived, so this list shortens only when the code changes
-- not when someone decides it is fine.

### Published crates absent from the facade entirely

Not merely unexposed — not in the dependency graph at all. Each is either a
deliberate split or a capability we imply we ship and never wired up.

`pdf-invoice` · `pdf-text-format` · `pdfluent-cli`

## Facade surface

### Methods that exist and fail when called

The dangerous shape: the type system promises them, the runtime refuses, and
a caller cannot discover the gap without running it.

- `linearize()`

### All public methods

| method | works |
|---|---|
| `add_decoration` | yes |
| `add_watermark` | yes |
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
| `extract_blocks` | yes |
| `extract_docling` | yes |
| `extract_markdown` | yes |
| `extract_pages` | yes |
| `extract_text` | yes |
| `extract_text_pages` | yes |
| `find_text` | yes |
| `flatten_annotations` | yes |
| `flatten_forms` | yes |
| `flatten_xfa` | yes |
| `flatten_xfa_with_password` | yes |
| `form_fields` | yes |
| `form_model` | yes |
| `form_mut` | yes |
| `from_bytes` | yes |
| `from_bytes_with` | yes |
| `from_reader` | yes |
| `has_xfa_form` | yes |
| `insert_image` | yes |
| `is_complete` | yes |
| `keep_ligatures` | yes |
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
| `render_page_with_options` | yes |
| `replace_text` | yes |
| `replace_text_matches` | yes |
| `rotate_page` | yes |
| `save` | yes |
| `save_with` | yes |
| `set_outlines` | yes |
| `set_xfa_field_value` | yes |
| `sign` | yes |
| `sign_timestamped` | yes |
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
| `to_docx_bytes` | yes |
| `to_images` | yes |
| `to_incremental_bytes` | yes |
| `to_pptx` | yes |
| `to_pptx_bytes` | yes |
| `to_xlsx` | yes |
| `to_xlsx_bytes` | yes |
| `validate_pdfa` | yes |
| `verify_signatures` | yes |
| `version` | yes |
| `with_incremental` | yes |
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
| `epub-extract` | 1.1.0 | direct | 1 | 20 | `quality:cargo-test` |
| `formcalc-interpreter` | 1.1.0 | internal | 5 | 113 | `quality:cargo-test` |
| `pdf-annot` | 1.1.0 | direct | — | 40 | `quality:cargo-test` |
| `pdf-compliance` | 1.1.0 | direct | — | 94 | `quality:cargo-test` |
| `pdf-docx` | 1.1.0 | direct | — | 29 | `quality:cargo-test` |
| `pdf-engine` | 1.1.0 | direct | 162 | 819 | `quality:cargo-test` |
| `pdf-font` | 1.0.0-beta.6 | internal | — | 120 | `quality:cargo-test` |
| `pdf-interpret` | 0.6.0 | direct | — | 162 | `quality:cargo-test` |
| `pdf-invoice` | 1.1.0 | **absent** | — | 53 | `quality:cargo-test` |
| `pdf-manip` | 1.1.0 | direct | 46 | 387 | `quality:cargo-test` |
| `pdf-ocr` | 1.1.0 | internal | 2 | 96 | `quality:cargo-test` |
| `pdf-pptx` | 1.1.0 | direct | — | 14 | `quality:cargo-test` |
| `pdf-redact` | 1.1.0 | direct | 6 | 57 | `quality:cargo-test` |
| `pdf-render` | 1.1.0 | direct | 2 | 23 | `quality:cargo-test` |
| `pdf-standard-fonts` | 1.1.0 | direct | — | 12 | `quality:cargo-test` |
| `pdf-substitute-fonts` | 1.1.0 | internal | — | 3 | `quality:cargo-test` |
| `pdf-syntax` | 0.6.0 | direct | — | 237 | `quality:cargo-test` |
| `pdf-text-format` | 1.1.0 | **absent** | — | 21 | `quality:cargo-test` |
| `pdf-xfa` | 1.1.0 | internal | 53 | 396 | `quality:cargo-test` |
| `pdf-xlsx` | 1.1.0 | direct | — | 23 | `quality:cargo-test` |
| `pdfluent` | 1.1.0 | direct | 68 | 58 | `quality:cargo-test` |
| `pdfluent-ccitt` | 0.2.3 | internal | — | 9 | `quality:cargo-test` |
| `pdfluent-cff` | 0.2.2 | internal | — | 26 | `quality:cargo-test` |
| `pdfluent-cli` | 1.1.0 | **absent** | 12 | 47 | `quality:cargo-test` |
| `pdfluent-extract` | 1.1.0 | internal | — | 92 | `quality:cargo-test` |
| `pdfluent-forms` | 1.1.0 | direct | 2 | 70 | `quality:cargo-test` |
| `pdfluent-jbig2` | 0.3.0 | internal | 1 | 12 | `quality:cargo-test` |
| `pdfluent-jpeg2000` | 0.4.0 | internal | — | 12 | `quality:cargo-test` |
| `pdfluent-lopdf` | 0.39.6 | direct | 5 | 180 | `quality:cargo-test` |
| `pdfluent-sign` | 1.1.0 | direct | 3 | 68 | `quality:cargo-test` |
| `pptx-extract` | 1.1.0 | internal | 1 | 11 | `quality:cargo-test` |
| `xfa-dom-resolver` | 1.1.0 | internal | 1 | 32 | `quality:cargo-test` |
| `xfa-js-sandboxed` | 1.1.0 | internal | 5 | 8 | `quality:cargo-test` |
| `xfa-json` | 1.1.0 | internal | — | 26 | `quality:cargo-test` |
| `xfa-layout-engine` | 1.1.0 | internal | 9 | 123 | `quality:cargo-test` |
| `xlsx-extract` | 1.1.0 | internal | 1 | 5 | `quality:cargo-test` |

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

