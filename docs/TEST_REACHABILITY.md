<!-- Gegenereerd door scripts/ci/test_reachability.py — niet met de hand bewerken. -->

# Publieke functies die geen test bereikt

Een ondergrens, geen dekkingscijfer: dit zegt of een functie tijdens de tests ooit wordt uitgevoerd, niet of er iets over haar gedrag wordt beweerd. Wat hier staat wordt door niets uitgevoerd, en dus door niets bewaakt.

Niet meegeteld als test: bindingtests buiten de Rust-boom (Python, Node, Java, .NET, WASM), en `examples/`. `pdf-desktop`-commando's worden vanuit de frontend aangeroepen en horen hier dus thuis zonder dat het een bevinding is.

**113 van 679 publieke functies** (16.6%).

Daarvan dragen er **7** een uitleg in hun eigen doc-commentaar. De rest staat hier zonder dat iemand heeft opgeschreven waarom, en dat is het getal dat omlaag hoort. Een functie die hier bij komt zonder reden verschijnt in de diff als `(geen reden opgegeven)` — precies waar een reviewer kijkt.

| crate | onbereikt | publiek |
|---|---:|---:|
| `pdf-compliance` | 16 | 178 |
| `pdf-desktop` | 16 | 26 |
| `pdf-manip` | 11 | 132 |
| `pdf-sign` | 11 | 23 |
| `lopdf` | 10 | 25 |
| `pdf-content-stream` | 10 | 11 |
| `pdf-ocr` | 6 | 26 |
| `pdf-xfa` | 5 | 58 |
| `xfa-test-runner` | 5 | 29 |
| `cff-parser` | 4 | 8 |
| `pdf-engine` | 3 | 9 |
| `pdf-node` | 3 | 12 |
| `formcalc-interpreter` | 2 | 5 |
| `pdf-forms` | 2 | 29 |
| `xfa-cli` | 2 | 5 |
| `hayro-jpeg2000` | 1 | 1 |
| `pdf-annot` | 1 | 4 |
| `pdf-bench` | 1 | 1 |
| `pdf-docx` | 1 | 6 |
| `pdf-extract` | 1 | 13 |
| `xfa-dom-resolver` | 1 | 4 |
| `xfa-golden-tests` | 1 | 3 |

## cff-parser

- `f64_abs`  — *(geen reden opgegeven)*
- `is_dict_one_byte_op`  — *(geen reden opgegeven)*
- `skip_index`  — *(geen reden opgegeven)*
- `skip_number`  — *(geen reden opgegeven)*

## formcalc-interpreter

- `call_dom_builtin`  — *(geen reden opgegeven)*
- `call_som_builtin`  — *(geen reden opgegeven)*

## hayro-jpeg2000

- `register_decoding_hook`  — *(geen reden opgegeven)*

## lopdf

- `binary_mark`  — *(geen reden opgegeven)*
- `decode_frame`  — *(geen reden opgegeven)*
- `decode_row`  — *(geen reden opgegeven)*
- `decrypt_object`  — *(geen reden opgegeven)*
- `encode_row`  — *(geen reden opgegeven)*
- `encode_utf8`  — *(geen reden opgegeven)*
- `encrypt_object`  — *(geen reden opgegeven)*
- `indirect_object`  — *(geen reden opgegeven)*
- `substring`  — *(geen reden opgegeven)*
- `xref_and_trailer`  — *(geen reden opgegeven)*

## pdf-annot

- `parse_destination`  — *(geen reden opgegeven)*

## pdf-bench

- `physical_footprint`  — *(geen reden opgegeven)*

## pdf-compliance

- `check_font_file_subtype`  — *(geen reden opgegeven)*
- `check_inline_image_filters`  — *(geen reden opgegeven)*
- `check_marked_content_sequences`  — *(geen reden opgegeven)*
- `check_name_utf8`  — *(geen reden opgegeven)*
- `check_real_value_limits`  — *(geen reden opgegeven)*
- `check_stream_empty_keys`  — *(geen reden opgegeven)*
- `check_stream_external_refs`  — *(geen reden opgegeven)*
- `check_stream_filters`  — *(geen reden opgegeven)*
- `check_truetype_cmap_pdfa4`  — *(geen reden opgegeven)*
- `check_undefined_operators`  — *(geen reden opgegeven)*
- `check_xmp_schemas`  — *(geen reden opgegeven)*
- `has_embedded_files`  — *(geen reden opgegeven)*
- `parse_structure_tree`  — *(geen reden opgegeven)*
- `validate_pdfa_with_progress`  — *(geen reden opgegeven)*
- `validate_pdfx`  — *(geen reden opgegeven)*
- `validate_with_progress`  — *(geen reden opgegeven)*

## pdf-content-stream

- `find_span`  — *(geen reden opgegeven)*
- `find_spans_containing`  — *(geen reden opgegeven)*
- `identity_matrix`  — *(geen reden opgegeven)*
- `matrix_concat`  — *(geen reden opgegeven)*
- `matrix_origin`  — *(geen reden opgegeven)*
- `matrix_translate`  — *(geen reden opgegeven)*
- `serialize`  — *(geen reden opgegeven)*
- `tj_text_len`  — *(geen reden opgegeven)*
- `verify_contiguity`  — *(geen reden opgegeven)*
- `verify_round_trip`  — *(geen reden opgegeven)*

## pdf-desktop

- `build_menu`  — *(geen reden opgegeven)*
- `close_document`  — *(geen reden opgegeven)*
- `delete_annotation`  — *(geen reden opgegeven)*
- `delete_page`  — *(geen reden opgegeven)*
- `document_info`  — *(geen reden opgegeven)*
- `get_bookmarks`  — *(geen reden opgegeven)*
- `get_page_geometry`  — *(geen reden opgegeven)*
- `is_document_dirty`  — *(geen reden opgegeven)*
- `list_annotations`  — *(geen reden opgegeven)*
- `open_document`  — *(geen reden opgegeven)*
- `print_document`  — *(geen reden opgegeven)*
- `redo_document`  — *(geen reden opgegeven)*
- `run`  — *(geen reden opgegeven)*
- `save_document_as`  — *(geen reden opgegeven)*
- `search_document`  — *(geen reden opgegeven)*
- `undo_document`  — *(geen reden opgegeven)*

## pdf-docx

- `pdf_to_docx_text_only`  — *(geen reden opgegeven)*

## pdf-engine

- `best_available_backend`  — *(geen reden opgegeven)*
- `ocr_page_default`  — *(geen reden opgegeven)*
- `read_with`  — *(geen reden opgegeven)*

## pdf-extract

- `extract_page_blocks`  — *(geen reden opgegeven)*

## pdf-forms

- `get_options`  — *(geen reden opgegeven)*
- `set_multi_selection`  — *(geen reden opgegeven)*

## pdf-manip

- `ensure_truetype_encoding`
- `fix_simple_truetype_widths`
- `fix_symbolic_font_flags`
- `fix_truetype_macroman_unicode_aliases`
- `fix_type0_cmap_cidsysteminfo`
- `fix_type1_standard_encoding`
- `fix_type1_widths`
- `open_encrypted`  — *(geen reden opgegeven)*
- `rearrange_pages`  — *(geen reden opgegeven)*
- `run_structure_fixups`  — *(geen reden opgegeven)*
- `split_by_ranges`  — *(geen reden opgegeven)*

## pdf-node

- `merge_pdfs`  — *(geen reden opgegeven)*
- `open_pdf`  — *(geen reden opgegeven)*
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
- `compute_vri_key`  — *(geen reden opgegeven)*
- `get_docmdp_permission`  — *(geen reden opgegeven)*
- `get_field_mdp_locks`  — *(geen reden opgegeven)*
- `has_appearance_stream`  — *(geen reden opgegeven)*
- `is_field_locked`  — *(geen reden opgegeven)*
- `parse_seed_values`  — *(geen reden opgegeven)*
- `request_timestamp`  — *(geen reden opgegeven)*
- `sign_pdf_incremental`  — *(geen reden opgegeven)*
- `sign_pdf_ltv`  — *(geen reden opgegeven)*
- `verify_certificate_chain`  — *(geen reden opgegeven)*

## pdf-xfa

- `draw_appearance`  — *(geen reden opgegeven)*
- `execute_commands`  — *(geen reden opgegeven)*
- `flatten_xfa_to_pdf_with_layout_dump_and_metadata`  — *(geen reden opgegeven)*
- `generate_appearances`  — *(geen reden opgegeven)*
- `multiline_appearance`  — *(geen reden opgegeven)*

## xfa-cli

- `parse_page_list`  — *(geen reden opgegeven)*
- `run`  — *(geen reden opgegeven)*

## xfa-dom-resolver

- `resolve_data_som_single`  — *(geen reden opgegeven)*

## xfa-golden-tests

- `compare_golden_files`  — *(geen reden opgegeven)*

## xfa-test-runner

- `generate_issue_labels`  — *(geen reden opgegeven)*
- `generate_resolved_comment`  — *(geen reden opgegeven)*
- `generate_update_comment`  — *(geen reden opgegeven)*
- `has_malformed_page_tree`  — *(geen reden opgegeven)*
- `save_text_diff`  — *(geen reden opgegeven)*
