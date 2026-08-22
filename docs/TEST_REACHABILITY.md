<!-- Gegenereerd door scripts/ci/test_reachability.py — niet met de hand bewerken. -->

# Publieke functies die geen test bereikt

Een ondergrens, geen dekkingscijfer: dit zegt of een functie tijdens de tests ooit wordt uitgevoerd, niet of er iets over haar gedrag wordt beweerd. Wat hier staat wordt door niets uitgevoerd, en dus door niets bewaakt.

Niet meegeteld als test: bindingtests buiten de Rust-boom (Python, Node, Java, .NET, WASM), en `examples/`. `pdf-desktop`-commando's worden vanuit de frontend aangeroepen en horen hier dus thuis zonder dat het een bevinding is.

**75 van 653 publieke functies** (11.5%).

Daarvan dragen er **27** een uitleg: 16 worden per constructie van buiten Rust aangeroepen (`#[tauri::command]`, een binding-attribuut, een C-ABI-export), 7 zijn een omhulsel dat niets doet dan doorgeven aan een variant die wél getest wordt, en 4 dragen een gemeten besluit in hun eigen doc-commentaar.

De overige **48** staan hier zonder uitleg. Dat is het getal dat omlaag hoort, en het is geen verzameling besluiten maar een lijst gaten: niemand heeft ze getest en niemand heeft opgeschreven waarom niet. Een functie die hier bij komt verschijnt in de diff als `(geen reden opgegeven)` — precies waar een reviewer kijkt.

| crate | onbereikt | publiek |
|---|---:|---:|
| `pdf-desktop` | 16 | 26 |
| `pdf-compliance` | 11 | 178 |
| `pdf-manip` | 6 | 109 |
| `pdf-ocr` | 6 | 26 |
| `pdf-sign` | 6 | 23 |
| `pdf-xfa` | 5 | 58 |
| `xfa-test-runner` | 4 | 26 |
| `cff-parser` | 3 | 8 |
| `lopdf` | 3 | 25 |
| `pdf-node` | 3 | 12 |
| `formcalc-interpreter` | 2 | 5 |
| `pdf-engine` | 2 | 9 |
| `hayro-jpeg2000` | 1 | 1 |
| `pdf-annot` | 1 | 4 |
| `pdf-bench` | 1 | 1 |
| `pdf-extract` | 1 | 13 |
| `pdf-forms` | 1 | 29 |
| `xfa-cli` | 1 | 5 |
| `xfa-dom-resolver` | 1 | 4 |
| `xfa-golden-tests` | 1 | 3 |

## cff-parser

- `f64_abs`  — *(geen reden opgegeven)*
- `skip_index`  — *(geen reden opgegeven)*
- `skip_number`  — *(geen reden opgegeven)*

## formcalc-interpreter

- `call_dom_builtin`  — *(geen reden opgegeven)*
- `call_som_builtin`  — *(geen reden opgegeven)*

## hayro-jpeg2000

- `register_decoding_hook`  — *(geen reden opgegeven)*

## lopdf

- `binary_mark`  — *(geen reden opgegeven)*
- `indirect_object`  — *(geen reden opgegeven)*
- `xref_and_trailer`  — *(geen reden opgegeven)*

## pdf-annot

- `parse_destination`  — *(geen reden opgegeven)*

## pdf-bench

- `physical_footprint`  — *(geen reden opgegeven)*

## pdf-compliance

- `check_font_file_subtype`  — *delegatie*
- `check_inline_image_filters`  — *(geen reden opgegeven)*
- `check_name_utf8`  — *delegatie*
- `check_real_value_limits`  — *delegatie*
- `check_stream_empty_keys`  — *delegatie*
- `check_stream_external_refs`  — *delegatie*
- `check_stream_filters`  — *delegatie*
- `check_truetype_cmap_pdfa4`  — *(geen reden opgegeven)*
- `check_xmp_schemas`  — *(geen reden opgegeven)*
- `has_embedded_files`  — *(geen reden opgegeven)*
- `parse_structure_tree`  — *(geen reden opgegeven)*

## pdf-desktop

- `build_menu`  — *(geen reden opgegeven)*
- `close_document`  — *buiten Rust*
- `delete_annotation`  — *buiten Rust*
- `delete_page`  — *buiten Rust*
- `document_info`  — *buiten Rust*
- `get_bookmarks`  — *buiten Rust*
- `get_page_geometry`  — *buiten Rust*
- `is_document_dirty`  — *buiten Rust*
- `list_annotations`  — *buiten Rust*
- `open_document`  — *buiten Rust*
- `print_document`  — *buiten Rust*
- `redo_document`  — *buiten Rust*
- `run`  — *(geen reden opgegeven)*
- `save_document_as`  — *buiten Rust*
- `search_document`  — *buiten Rust*
- `undo_document`  — *buiten Rust*

## pdf-engine

- `ocr_page_default`  — *(geen reden opgegeven)*
- `read_with`  — *(geen reden opgegeven)*

## pdf-extract

- `extract_page_blocks`  — *(geen reden opgegeven)*

## pdf-forms

- `set_multi_selection`  — *(geen reden opgegeven)*

## pdf-manip

- `ensure_truetype_encoding`  — *gemeten besluit*
- `fix_simple_truetype_widths`  — *gemeten besluit*
- `fix_type0_cmap_cidsysteminfo`  — *gemeten besluit*
- `fix_type1_widths`  — *gemeten besluit*
- `rearrange_pages`  — *delegatie*
- `run_structure_fixups`  — *(geen reden opgegeven)*

## pdf-node

- `merge_pdfs`  — *buiten Rust*
- `open_pdf`  — *buiten Rust*
- `to_napi_error`  — *(geen reden opgegeven)*

## pdf-ocr

- `classify_and_rotate_batch`  — *(geen reden opgegeven)*
- `classify_angle`  — *(geen reden opgegeven)*
- `detect_inference`  — *(geen reden opgegeven)*
- `load_sessions`  — *(geen reden opgegeven)*
- `recognize_batch`  — *(geen reden opgegeven)*
- `recognize_inference`  — *(geen reden opgegeven)*

## pdf-sign

- `check_revocation_embedded`  — *(geen reden opgegeven)*
- `has_appearance_stream`  — *(geen reden opgegeven)*
- `parse_seed_values`  — *(geen reden opgegeven)*
- `request_timestamp`  — *(geen reden opgegeven)*
- `sign_pdf_ltv`  — *(geen reden opgegeven)*
- `verify_certificate_chain`  — *(geen reden opgegeven)*

## pdf-xfa

- `draw_appearance`  — *(geen reden opgegeven)*
- `execute_commands`  — *(geen reden opgegeven)*
- `flatten_xfa_to_pdf_with_layout_dump_and_metadata`  — *(geen reden opgegeven)*
- `generate_appearances`  — *(geen reden opgegeven)*
- `multiline_appearance`  — *(geen reden opgegeven)*

## xfa-cli

- `run`  — *(geen reden opgegeven)*

## xfa-dom-resolver

- `resolve_data_som_single`  — *(geen reden opgegeven)*

## xfa-golden-tests

- `compare_golden_files`  — *(geen reden opgegeven)*

## xfa-test-runner

- `generate_issue_labels`  — *(geen reden opgegeven)*
- `generate_resolved_comment`  — *(geen reden opgegeven)*
- `generate_update_comment`  — *(geen reden opgegeven)*
- `save_text_diff`  — *(geen reden opgegeven)*
