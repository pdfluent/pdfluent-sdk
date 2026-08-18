# Capability inventory

**Generated** by `scripts/ci/capability_inventory.py` — do not edit by hand.
CI regenerates it and fails if this file has drifted from the code, which is
the whole point: three hand-written maps of this codebase were four months
stale at the same time, and each one still read as fact.

## Facade surface

`pdfluent::Document` exposes **78 public methods**, of which **5 do nothing at runtime**.

### Methods that exist and fail when called

These are the dangerous ones: the type system promises them and the
runtime refuses, so a caller cannot discover the gap without running it.

- `add_decoration()`
- `embed_font()`
- `flatten_forms()`
- `form_mut()`
- `linearize()`

### Feature flags that enable nothing

Enabling one of these is a no-op, which reads as consent. Some are
harmless (the dependency is unconditional anyway); check `cargo tree`
before assuming either way.

`signing` · `redaction` · `ocr-tesseract` · `ocr-paddle` · `html-to-pdf` · `docx-export` · `xlsx-export` · `pptx-export` · `xfa-flatten` · `wasm` · `internal-legacy`

## Crates reachable from the facade

A customer using the `pdfluent` crate gets these:

- `pdf-annot`
- `pdf-compliance`
- `pdf-docx`
- `pdf-engine`
- `pdf-interpret`
- `pdf-manip`
- `pdf-pptx`
- `pdf-redact`
- `pdf-render`
- `pdf-syntax`
- `pdf-xlsx`
- `pdfluent`
- `pdfluent-forms`
- `pdfluent-lopdf`
- `pdfluent-sign`
- `xfa-license`

## Published crates NOT reachable from the facade

These exist and are published, but a customer using `pdfluent` cannot call
them without adding the crate themselves. Every entry here is either a
deliberate split or an advertised capability that is not actually delivered.

- `formcalc-interpreter`
- `pdf-font`
- `pdf-invoice`
- `pdf-ocr`
- `pdf-standard-fonts`
- `pdf-text-format`
- `pdf-xfa`
- `pdfluent-extract`

## All public methods on the facade

| method | works |
|---|---|
| `add_decoration` | **stub** |
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
| `embed_font` | **stub** |
| `encrypt` | yes |
| `extract_pages` | yes |
| `extract_text` | yes |
| `find_text` | yes |
| `flatten_annotations` | yes |
| `flatten_forms` | **stub** |
| `form_fields` | yes |
| `form_model` | yes |
| `form_mut` | **stub** |
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

