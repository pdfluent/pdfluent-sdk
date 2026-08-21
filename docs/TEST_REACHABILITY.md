<!-- Gegenereerd door scripts/ci/test_reachability.py — niet met de hand bewerken. -->

# Publieke functies die geen test bereikt

Een ondergrens, geen dekkingscijfer: dit zegt of een functie tijdens de tests ooit wordt uitgevoerd, niet of er iets over haar gedrag wordt beweerd. Wat hier staat wordt door niets uitgevoerd, en dus door niets bewaakt.

Niet meegeteld als test: bindingtests buiten de Rust-boom (Python, Node, Java, .NET, WASM), en `examples/`. `pdf-desktop`-commando's worden vanuit de frontend aangeroepen en horen hier dus thuis zonder dat het een bevinding is.

**115 van 679 publieke functies** (16.9%).

| crate | onbereikt | publiek |
|---|---:|---:|
| `pdf-compliance` | 16 | 178 |
| `pdf-desktop` | 16 | 26 |
| `pdf-manip` | 13 | 132 |
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

- `f64_abs`
- `is_dict_one_byte_op`
- `skip_index`
- `skip_number`

## formcalc-interpreter

- `call_dom_builtin`
- `call_som_builtin`

## hayro-jpeg2000

- `register_decoding_hook`

## lopdf

- `binary_mark`
- `decode_frame`
- `decode_row`
- `decrypt_object`
- `encode_row`
- `encode_utf8`
- `encrypt_object`
- `indirect_object`
- `substring`
- `xref_and_trailer`

## pdf-annot

- `parse_destination`

## pdf-bench

- `physical_footprint`

## pdf-compliance

- `check_font_file_subtype`
- `check_inline_image_filters`
- `check_marked_content_sequences`
- `check_name_utf8`
- `check_real_value_limits`
- `check_stream_empty_keys`
- `check_stream_external_refs`
- `check_stream_filters`
- `check_truetype_cmap_pdfa4`
- `check_undefined_operators`
- `check_xmp_schemas`
- `has_embedded_files`
- `parse_structure_tree`
- `validate_pdfa_with_progress`
- `validate_pdfx`
- `validate_with_progress`

## pdf-content-stream

- `find_span`
- `find_spans_containing`
- `identity_matrix`
- `matrix_concat`
- `matrix_origin`
- `matrix_translate`
- `serialize`
- `tj_text_len`
- `verify_contiguity`
- `verify_round_trip`

## pdf-desktop

- `build_menu`
- `close_document`
- `delete_annotation`
- `delete_page`
- `document_info`
- `get_bookmarks`
- `get_page_geometry`
- `is_document_dirty`
- `list_annotations`
- `open_document`
- `print_document`
- `redo_document`
- `run`
- `save_document_as`
- `search_document`
- `undo_document`

## pdf-docx

- `pdf_to_docx_text_only`

## pdf-engine

- `best_available_backend`
- `ocr_page_default`
- `read_with`

## pdf-extract

- `extract_page_blocks`

## pdf-forms

- `get_options`
- `set_multi_selection`

## pdf-manip

- `ensure_truetype_encoding`
- `fix_simple_truetype_widths`
- `fix_symbolic_font_flags`
- `fix_truetype_macroman_unicode_aliases`
- `fix_type0_cmap_cidsysteminfo`
- `fix_type1_standard_encoding`
- `fix_type1_widths`
- `open_encrypted`
- `rearrange_pages`
- `restore_stripped_encodings`
- `run_structure_fixups`
- `snapshot_font_encodings`
- `split_by_ranges`

## pdf-node

- `merge_pdfs`
- `open_pdf`
- `to_napi_error`

## pdf-ocr

- `classify_and_rotate_batch`
- `classify_angle`
- `detect_inference`
- `load_sessions`
- `recognize_batch`
- `recognize_inference`

## pdf-sign

- `check_revocation_embedded`
- `compute_vri_key`
- `get_docmdp_permission`
- `get_field_mdp_locks`
- `has_appearance_stream`
- `is_field_locked`
- `parse_seed_values`
- `request_timestamp`
- `sign_pdf_incremental`
- `sign_pdf_ltv`
- `verify_certificate_chain`

## pdf-xfa

- `draw_appearance`
- `execute_commands`
- `flatten_xfa_to_pdf_with_layout_dump_and_metadata`
- `generate_appearances`
- `multiline_appearance`

## xfa-cli

- `parse_page_list`
- `run`

## xfa-dom-resolver

- `resolve_data_som_single`

## xfa-golden-tests

- `compare_golden_files`

## xfa-test-runner

- `generate_issue_labels`
- `generate_resolved_comment`
- `generate_update_comment`
- `has_malformed_page_tree`
- `save_text_diff`
