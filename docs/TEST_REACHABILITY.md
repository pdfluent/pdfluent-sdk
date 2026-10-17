<!-- Gegenereerd door scripts/ci/test_reachability.py — niet met de hand bewerken. -->

# Publieke functies die geen test bereikt

Een ondergrens, geen dekkingscijfer: dit zegt of een functie tijdens de tests ooit wordt uitgevoerd, niet of er iets over haar gedrag wordt beweerd. Wat hier staat wordt door niets uitgevoerd, en dus door niets bewaakt.

Niet meegeteld als test: bindingtests buiten de Rust-boom (Python, Node, Java, .NET, WASM), en `examples/`. `pdf-desktop`-commando's worden vanuit de frontend aangeroepen en horen hier dus thuis zonder dat het een bevinding is.

**257 van 1138 publieke functies** (22.6%).

Daarvan dragen er **57** een uitleg: 28 worden per constructie van buiten Rust aangeroepen (`#[tauri::command]`, een binding-attribuut, een C-ABI-export), 13 zijn een omhulsel dat niets doet dan doorgeven aan een variant die wél getest wordt, en 16 dragen een gemeten besluit in hun eigen doc-commentaar.

De overige **200** staan hier zonder uitleg. Dat is het getal dat omlaag hoort, en het is geen verzameling besluiten maar een lijst gaten: niemand heeft ze getest en niemand heeft opgeschreven waarom niet. Een functie die hier bij komt verschijnt in de diff als `(geen reden opgegeven)` — precies waar een reviewer kijkt.

| crate | onbereikt | publiek |
|---|---:|---:|
| `pdfluent` | 99 | 139 |
| `pdf-node` | 86 | 119 |
| `pdf-desktop` | 15 | 26 |
| `pdf-compliance` | 11 | 179 |
| `pdf-engine` | 7 | 169 |
| `pdf-sign` | 7 | 29 |
| `pdf-manip` | 6 | 119 |
| `pdf-ocr` | 6 | 40 |
| `xfa-test-runner` | 4 | 26 |
| `cff-parser` | 3 | 8 |
| `xfa-wasm` | 3 | 14 |
| `formcalc-interpreter` | 1 | 6 |
| `hayro-jpeg2000` | 1 | 1 |
| `lopdf` | 1 | 28 |
| `pdf-annot` | 1 | 5 |
| `pdf-bench` | 1 | 1 |
| `pdf-forms` | 1 | 29 |
| `pdf-python` | 1 | 3 |
| `xfa-cli` | 1 | 5 |
| `xfa-dom-resolver` | 1 | 4 |
| `xfa-golden-tests` | 1 | 3 |

## cff-parser

- `f64_abs`  — *(geen reden opgegeven)*
- `skip_index`  — *(geen reden opgegeven)*
- `skip_number`  — *(geen reden opgegeven)*

## formcalc-interpreter

- `stack_position`  — *gemeten besluit*

## hayro-jpeg2000

- `register_decoding_hook`  — *(geen reden opgegeven)*

## lopdf

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

- `class_probabilities`  — *(geen reden opgegeven)*
- `explain`  — *(geen reden opgegeven)*
- `fill_repeats`  — *(geen reden opgegeven)*
- `from_edges`  — *(geen reden opgegeven)*
- `ocr_page_default`  — *(geen reden opgegeven)*
- `read_with`  — *(geen reden opgegeven)*
- `widen_spans`  — *gemeten besluit*

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

- `annot_postprocess`  — *(geen reden opgegeven)*
- `annot_preprocess`  — *(geen reden opgegeven)*
- `apply_pages`  — *(geen reden opgegeven)*
- `args_match`  — *(geen reden opgegeven)*
- `assert_pdf`  — *(geen reden opgegeven)*
- `base14_fontdict`  — *buiten Rust*
- `base14_fontnames`  — *buiten Rust*
- `calc_image_matrix`  — *(geen reden opgegeven)*
- `canon`  — *(geen reden opgegeven)*
- `chartocanon`  — *(geen reden opgegeven)*
- `check_color`  — *(geen reden opgegeven)*
- `check_font`  — *(geen reden opgegeven)*
- `check_font_info`  — *(geen reden opgegeven)*
- `check_marker_arg`  — *(geen reden opgegeven)*
- `check_morph`  — *(geen reden opgegeven)*
- `check_quad`  — *(geen reden opgegeven)*
- `check_rect`  — *(geen reden opgegeven)*
- `colors_pdf_dict`  — *(geen reden opgegeven)*
- `colors_wx_list`  — *(geen reden opgegeven)*
- `compute_scissor`  — *(geen reden opgegeven)*
- `cs_cmyk`  — *buiten Rust*
- `cs_gray`  — *buiten Rust*
- `cs_rgb`  — *buiten Rust*
- `css_for_font`  — *(geen reden opgegeven)*
- `dest_is_valid`  — *(geen reden opgegeven)*
- `dest_is_valid_page`  — *(geen reden opgegeven)*
- `detect_super_script`  — *(geen reden opgegeven)*
- `dir_str`  — *(geen reden opgegeven)*
- `ensure_operation`  — *(geen reden opgegeven)*
- `exception_info`  — *(geen reden opgegeven)*
- `extra_insert_pdf`  — *(geen reden opgegeven)*
- `find_string`  — *(geen reden opgegeven)*
- `format_g`  — *(geen reden opgegeven)*
- `get_env_bool`  — *(geen reden opgegeven)*
- `get_env_int`  — *(geen reden opgegeven)*
- `get_pdf_now`  — *(geen reden opgegeven)*
- `get_tessdata`  — *(geen reden opgegeven)*
- `get_text_length`  — *(geen reden opgegeven)*
- `hdist`  — *(geen reden opgegeven)*
- `image_profile`  — *(geen reden opgegeven)*
- `infinite_irect`  — *(geen reden opgegeven)*
- `infinite_quad`  — *(geen reden opgegeven)*
- `invalid_name_chars`  — *buiten Rust*
- `log`  — *(geen reden opgegeven)*
- `make_story_elpos`  — *(geen reden opgegeven)*
- `merge_pdfs`  — *buiten Rust*
- `message_warning`  — *(geen reden opgegeven)*
- `no_recommend_layout`  — *(geen reden opgegeven)*
- `on_highlight_char`  — *(geen reden opgegeven)*
- `open_pdf`  — *buiten Rust*
- `page_add_text_marker`  — *(geen reden opgegeven)*
- `page_merge`  — *(geen reden opgegeven)*
- `pdf_lookup_page_loc`  — *(geen reden opgegeven)*
- `pdf_name`  — *(geen reden opgegeven)*
- `pdfcolor`  — *buiten Rust*
- `pdfobj_string`  — *(geen reden opgegeven)*
- `py_sequence_check`  — *(geen reden opgegeven)*
- `py_sequence_size`  — *(geen reden opgegeven)*
- `py_unicode_decode_raw_unicode_escape`  — *(geen reden opgegeven)*
- `raisepy`  — *(geen reden opgegeven)*
- `recover_bbox_quad`  — *delegatie*
- `recover_char_quad`  — *delegatie*
- `recover_line_quad`  — *(geen reden opgegeven)*
- `recover_quad`  — *delegatie*
- `recover_span_quad`  — *(geen reden opgegeven)*
- `restore_aliases`  — *(geen reden opgegeven)*
- `set_log`  — *(geen reden opgegeven)*
- `set_messages`  — *(geen reden opgegeven)*
- `srgb_to_pdf`  — *(geen reden opgegeven)*
- `srgb_to_rgb`  — *(geen reden opgegeven)*
- `string_in_names_list`  — *(geen reden opgegeven)*
- `strip_outline`  — *(geen reden opgegeven)*
- `strip_outlines`  — *(geen reden opgegeven)*
- `symbol_glyphs`  — *buiten Rust*
- `util_concat_matrix`  — *(geen reden opgegeven)*
- `util_include_point_in_rect`  — *(geen reden opgegeven)*
- `util_invert_matrix`  — *(geen reden opgegeven)*
- `util_is_point_in_rect`  — *(geen reden opgegeven)*
- `util_make_irect`  — *(geen reden opgegeven)*
- `util_make_rect`  — *(geen reden opgegeven)*
- `util_point_in_quad`  — *(geen reden opgegeven)*
- `util_transform_point`  — *(geen reden opgegeven)*
- `util_transform_rect`  — *(geen reden opgegeven)*
- `util_union_rect`  — *(geen reden opgegeven)*
- `vdist`  — *(geen reden opgegeven)*
- `zapf_glyphs`  — *buiten Rust*

## pdf-ocr

- `classify_and_rotate_batch`  — *gemeten besluit*
- `classify_angle`  — *gemeten besluit*
- `detect_inference`  — *gemeten besluit*
- `load_sessions`  — *gemeten besluit*
- `recognize_batch`  — *gemeten besluit*
- `recognize_inference`  — *gemeten besluit*

## pdf-python

- `extract_office`  — *buiten Rust*

## pdf-sign

- `build_cms_signed_data`  — *(geen reden opgegeven)*
- `check_revocation_embedded`  — *gemeten besluit*
- `has_appearance_stream`  — *(geen reden opgegeven)*
- `parse_seed_values`  — *(geen reden opgegeven)*
- `request_timestamp`  — *gemeten besluit*
- `sign_pdf_ltv`  — *gemeten besluit*
- `verify_certificate_chain`  — *gemeten besluit*

## pdfluent

- `annot_postprocess`  — *(geen reden opgegeven)*
- `annot_preprocess`  — *(geen reden opgegeven)*
- `apply_pages`  — *(geen reden opgegeven)*
- `args_match`  — *(geen reden opgegeven)*
- `assert_pdf`  — *(geen reden opgegeven)*
- `calc_image_matrix`  — *(geen reden opgegeven)*
- `canon`  — *(geen reden opgegeven)*
- `chartocanon`  — *(geen reden opgegeven)*
- `check_color`  — *(geen reden opgegeven)*
- `check_font`  — *(geen reden opgegeven)*
- `check_font_in_bytes`  — *(geen reden opgegeven)*
- `check_font_info`  — *(geen reden opgegeven)*
- `check_marker_arg`  — *(geen reden opgegeven)*
- `check_morph`  — *(geen reden opgegeven)*
- `check_quad`  — *(geen reden opgegeven)*
- `check_rect`  — *(geen reden opgegeven)*
- `code_to_glyph_name`  — *(geen reden opgegeven)*
- `colors_pdf_dict`  — *(geen reden opgegeven)*
- `colors_wx_list`  — *(geen reden opgegeven)*
- `compute_scissor`  — *(geen reden opgegeven)*
- `css_for_font`  — *(geen reden opgegeven)*
- `dest_is_valid`  — *(geen reden opgegeven)*
- `dest_is_valid_page`  — *(geen reden opgegeven)*
- `detect_super_script`  — *(geen reden opgegeven)*
- `dir_str`  — *(geen reden opgegeven)*
- `ensure_operation`  — *(geen reden opgegeven)*
- `estimate_text_length`  — *(geen reden opgegeven)*
- `exception_info`  — *(geen reden opgegeven)*
- `extra_insert_pdf`  — *(geen reden opgegeven)*
- `extract_font_from_bytes`  — *(geen reden opgegeven)*
- `extract_image_in`  — *(geen reden opgegeven)*
- `extract_named_font_from_bytes`  — *(geen reden opgegeven)*
- `figures_to_imginfo`  — *(geen reden opgegeven)*
- `find_page_tables`  — *(geen reden opgegeven)*
- `find_string`  — *(geen reden opgegeven)*
- `form_fonts_from_bytes`  — *(geen reden opgegeven)*
- `format_g`  — *(geen reden opgegeven)*
- `get_env_bool`  — *(geen reden opgegeven)*
- `get_env_int`  — *(geen reden opgegeven)*
- `get_pdf_now`  — *(geen reden opgegeven)*
- `get_tessdata`  — *(geen reden opgegeven)*
- `get_text_length`  — *(geen reden opgegeven)*
- `hdist`  — *(geen reden opgegeven)*
- `image_mask_xref`  — *(geen reden opgegeven)*
- `image_profile`  — *(geen reden opgegeven)*
- `infinite_irect`  — *(geen reden opgegeven)*
- `infinite_quad`  — *(geen reden opgegeven)*
- `log`  — *(geen reden opgegeven)*
- `make_story_elpos`  — *(geen reden opgegeven)*
- `message_warning`  — *(geen reden opgegeven)*
- `no_recommend_layout`  — *(geen reden opgegeven)*
- `on_highlight_char`  — *(geen reden opgegeven)*
- `page_add_text_marker`  — *(geen reden opgegeven)*
- `page_form_xobject_rows`  — *(geen reden opgegeven)*
- `page_form_xobjects`  — *(geen reden opgegeven)*
- `page_image_bbox_in`  — *(geen reden opgegeven)*
- `page_image_facts_in`  — *(geen reden opgegeven)*
- `page_images_in`  — *(geen reden opgegeven)*
- `page_merge`  — *(geen reden opgegeven)*
- `page_number_id`  — *(geen reden opgegeven)*
- `page_object_id`  — *(geen reden opgegeven)*
- `page_placements_in`  — *(geen reden opgegeven)*
- `pdf_lookup_page_loc`  — *(geen reden opgegeven)*
- `pdf_name`  — *(geen reden opgegeven)*
- `pdfobj_string`  — *(geen reden opgegeven)*
- `py_sequence_check`  — *(geen reden opgegeven)*
- `py_sequence_size`  — *(geen reden opgegeven)*
- `py_unicode_decode_raw_unicode_escape`  — *(geen reden opgegeven)*
- `raisepy`  — *(geen reden opgegeven)*
- `recover_bbox_quad`  — *delegatie*
- `recover_char_quad`  — *delegatie*
- `recover_line_quad`  — *(geen reden opgegeven)*
- `recover_quad`  — *delegatie*
- `recover_span_quad`  — *(geen reden opgegeven)*
- `render_pixmap`  — *(geen reden opgegeven)*
- `restore_aliases`  — *(geen reden opgegeven)*
- `rewrite_images_in`  — *(geen reden opgegeven)*
- `set_log`  — *(geen reden opgegeven)*
- `set_messages`  — *(geen reden opgegeven)*
- `srgb_to_pdf`  — *(geen reden opgegeven)*
- `srgb_to_rgb`  — *(geen reden opgegeven)*
- `string_in_names_list`  — *(geen reden opgegeven)*
- `strip_outline`  — *(geen reden opgegeven)*
- `strip_outlines`  — *(geen reden opgegeven)*
- `svg_for_page`  — *(geen reden opgegeven)*
- `textpage_window`  — *(geen reden opgegeven)*
- `util_concat_matrix`  — *(geen reden opgegeven)*
- `util_include_point_in_rect`  — *(geen reden opgegeven)*
- `util_invert_matrix`  — *(geen reden opgegeven)*
- `util_is_point_in_rect`  — *(geen reden opgegeven)*
- `util_make_irect`  — *(geen reden opgegeven)*
- `util_make_rect`  — *(geen reden opgegeven)*
- `util_point_in_quad`  — *(geen reden opgegeven)*
- `util_transform_point`  — *(geen reden opgegeven)*
- `util_transform_rect`  — *(geen reden opgegeven)*
- `util_union_rect`  — *(geen reden opgegeven)*
- `vdist`  — *(geen reden opgegeven)*
- `xref_is_font_in_bytes`  — *(geen reden opgegeven)*
- `xref_is_image_in`  — *(geen reden opgegeven)*

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

## xfa-wasm

- `decrypt_pdf`  — *buiten Rust*
- `encrypt_pdf`  — *buiten Rust*
- `extract_images`  — *buiten Rust*
