# Known differences from the reference

Generated from specs/api/*.yaml (PyMuPDF==1.28.2) by scripts/compat/gen_known_differences.py. Do not edit by hand.

Members where we deliberately differ from the pinned reference, with the lane that owns the gap. A difference lands here only through the spec: flip the member status, regenerate, commit both.

| class | member | lane | contract |
| --- | --- | --- | --- |
| Document | apply_css | L4 |  |
| Document | convert_to_pdf | L2 | valid PDF with the same pages and content; bytes differ |
| Document | ez_save | L2 | valid PDF with the same content; bytes differ |
| Document | get_char_widths | L1 | glyph ids for non-embedded standard fonts |
| Document | journal_can_do | L4 |  |
| Document | journal_enable | L4 |  |
| Document | journal_is_enabled | L4 |  |
| Document | journal_load | L4 |  |
| Document | journal_op_name | L4 |  |
| Document | journal_position | L4 |  |
| Document | journal_redo | L4 |  |
| Document | journal_save | L4 |  |
| Document | journal_start_op | L4 |  |
| Document | journal_stop_op | L4 |  |
| Document | journal_undo | L4 |  |
| Document | layout | L2 |  |
| Document | repair | L4 |  |
| Document | resolve_link | L4 |  |
| Document | set_layer_ui_config | L4 |  |
| Document | write | L2 | valid PDF with the same content; bytes differ |
| Page | get_bboxlog | L2 | line-metric boxes instead of glyph-ink boxes |
| Page | get_displaylist | L2 |  |
| Page | get_svg_image | L3 | SVG without text runs (raster-based); vector SVG writer in 1.1.x |
| Page | write_text | L4 |  |
| Font | ascender | L4 |  |
| Font | bbox | L4 |  |
| Font | buffer | L4 |  |
| Font | char_lengths | L4 |  |
| Font | descender | L4 |  |
| Font | flags | L4 |  |
| Font | glyph_advance | L4 |  |
| Font | glyph_bbox | L4 |  |
| Font | glyph_count | L4 |  |
| Font | glyph_name_to_unicode | L4 |  |
| Font | has_glyph | L4 |  |
| Font | is_bold | L4 |  |
| Font | is_italic | L4 |  |
| Font | is_monospaced | L4 |  |
| Font | is_serif | L4 |  |
| Font | is_writable | L4 |  |
| Font | name | L4 |  |
| Font | text_length | L4 |  |
| Font | unicode_to_glyph_name | L4 |  |
| Font | valid_codepoints | L4 |  |
| module | UpdateFontInfo | L1 |  |
| module | css_for_pymupdf_font | L1 |  |
| module | get_highlight_selection | L5 |  |
| module | repair_mono_font | L1 |  |
| module | util_ensure_widget_calc | L5 |  |
