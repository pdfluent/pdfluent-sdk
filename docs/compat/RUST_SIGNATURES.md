# Compat Rust signatures

Generated from specs/api/*.yaml (PyMuPDF==1.28.2) by scripts/compat/gen_rust_signatures.py. Do not edit by hand.

One row per spec member: the recorded member, its kind, lane and tier, the
planned Rust path, the actual signature in the tree, and the first
differential case (plus how many more name the member). The lanes fill the
stub bodies; the bindings compile against these signatures.

Conventions:

- `Document` is an alias for the facade `PdfDocument`; document methods are
  inherent blocks in the lane files.
- `Page` methods on the shared view live in the L1/L2/L3 lane files; the
  editing methods live on `PageMut` in `edit.rs`, as do the L2 members that
  mutate (`clip_to_rect`, `wrap_contents`, `refresh`, `recolor`).
- Ordering members (`__lt__`/`__le__`/`__gt__`/`__ge__`) share one `compare`
  method per type: every recorded ordering attempt fails.
- `__eq__`/`__ne__` share the `PartialEq` impl, derived for data types and
  stubbed for document handles.
- Arithmetic members map to operator traits (`Add`, `Mul`, `Index`, ...);
  several impls may share one row.
- `Document::page_count`, `Document::pages`, and `Document::subset_fonts`
  are facade-provided: the signatures below are the existing facade methods
  (page spans compose through the `Pages` iterator; subset counts come from
  the report).

## Document

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `FormFonts` | property · L1 · T3 | `Document::form_fonts` | `pub fn form_fonts(&self) -> Result<Vec<FontInfo>>` | `l1-document-formfonts` |
| `__contains__` | operator · L2 · T3 | `Document::contains` | `pub fn contains(&self, location: &Location) -> Result<bool>` | `l2-document-contains` |
| `__delitem__` | operator · L2 · T3 | `Document::delete_at` | `pub fn delete_at(&mut self, index: i32) -> Result<()>` | `l2-document-delitem` |
| `__enter__` | operator · L2 · T3 | `Document::enter` | `pub fn enter(&self) -> &Self` | `l2.doc.context_manager` |
| `__eq__` | operator · L2 · T3 | `Document::PartialEq` | `impl PartialEq for PdfDocument` | `l2-document-eq` |
| `__exit__` | operator · L2 · T3 | `Document::exit` | `pub fn exit(&mut self) -> Result<()>` | `l2-document-exit` |
| `__ge__` | operator · L2 · T3 | `Document::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-document-ge` |
| `__getitem__` | operator · L2 · T3 | `Document::at` | `pub fn at(&self, index: i32) -> Result<Page<'_>>` | `l2.pages.getitem` (+2) |
| `__gt__` | operator · L2 · T3 | `Document::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-document-gt` |
| `__init__` | constructor · L2 · T1 | `Document::open_compat` | `pub fn open_compat(options: &DocumentOpenOptions) -> Result<Self>` | `l2.open.document_ctor` (+9) |
| `__le__` | operator · L2 · T3 | `Document::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-document-le` |
| `__len__` | operator · L2 · T3 | `Document::len` | `pub fn len(&self) -> usize` | `l2.pages.len` |
| `__lt__` | operator · L2 · T3 | `Document::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-document-lt` |
| `__ne__` | operator · L2 · T3 | `Document::PartialEq` | `impl PartialEq for PdfDocument` | `l2-document-ne` |
| `add_layer` | method · L4 · T3 | `Document::add_layer` | `pub fn add_layer( &mut self, _name: &str, _creator: Option<&str>, _on: Option<bool>, ) -> Result<()>` | `l4-document-add-layer` |
| `add_ocg` | method · L4 · T3 | `Document::add_ocg` | `pub fn add_ocg( &mut self, _name: &str, _config: i32, _on: bool, _intent: Option<&str>, _usage: Option<&str>, ) -> Result<i32>` | `l4-document-add-ocg` |
| `apply_css` | method · L4 · T3 | `Document::apply_css` | `pub fn apply_css(&mut self, _css: &str, _append: bool) -> Result<()>` | `l4-document-apply-css` |
| `authenticate` | method · L2 · T2 | `Document::authenticate` | `pub fn authenticate(&self, _password: &str) -> Result<i32>` | `l2.crypto.authenticate` |
| `bake` | method · L4 · T2 | `Document::bake` | `pub fn bake(&mut self, options: &BakeOptions) -> Result<()>` | `l4-document-bake` |
| `can_save_incrementally` | method · L2 · T3 | `Document::can_save_incrementally` | `pub fn can_save_incrementally(&self) -> Result<bool>` | `l2.doc.can_save_incrementally` |
| `chapter_count` | property · L2 · T3 | `Document::chapter_count` | `pub fn chapter_count(&self) -> Result<usize>` | `l2-document-chapter-count` |
| `chapter_page_count` | method · L2 · T3 | `Document::chapter_page_count` | `pub fn chapter_page_count(&self, chapter: i32) -> Result<usize>` | `l2-document-chapter-page-count` |
| `close` | method · L2 · T1 | `Document::close` | `pub fn close(self)` | `l2.doc.close_twice` (+1) |
| `convert_to_pdf` | method · L2 · T2 | `Document::convert_to_pdf` | `pub fn convert_to_pdf(&self, from_page: i32, to_page: i32, rotate: i32) -> Result<Vec<u8>>` | `l2-document-convert-to-pdf` |
| `copy_page` | method · L4 · T3 | `Document::copy_page` | `pub fn copy_page(&mut self, _page: i32, _to: i32) -> Result<()>` | `l2.edit.copy_page` |
| `del_toc_item` | method · L4 · T3 | `Document::del_toc_item` | `pub fn del_toc_item(&mut self, _index: i32) -> Result<()>` | `l2.toc.del_item` (+1) |
| `del_xml_metadata` | method · L4 · T3 | `Document::del_xml_metadata` | `pub fn del_xml_metadata(&mut self) -> Result<()>` | `l4-document-del-xml-metadata` |
| `delete_page` | method · L4 · T3 | `Document::delete_page` | `pub fn delete_page(&mut self, _page: i32) -> Result<()>` | `l2.edit.delete_page_default` (+2) |
| `delete_pages` | method · L4 · T3 | `Document::delete_pages` | `pub fn delete_pages(&mut self, _pages: &PageSelection) -> Result<()>` | `l2.edit.delete_pages_bad` (+2) |
| `embfile_add` | method · L4 · T3 | `Document::embfile_add` | `pub fn embfile_add( &mut self, _name: &str, _data: &[u8], _options: &EmbFileOptions, ) -> Result<i32>` | `l4-document-embfile-add` |
| `embfile_count` | method · L2 · T3 | `Document::embfile_count` | `pub fn embfile_count(&self) -> Result<usize>` | `l2-document-embfile-count` |
| `embfile_del` | method · L4 · T3 | `Document::embfile_del` | `pub fn embfile_del(&mut self, _item: &EmbItem) -> Result<()>` | `l4-document-embfile-del` |
| `embfile_get` | method · L2 · T3 | `Document::embfile_get` | `pub fn embfile_get(&self, item: &EmbItem) -> Result<Vec<u8>>` | `l2-document-embfile-get` |
| `embfile_info` | method · L2 · T3 | `Document::embfile_info` | `pub fn embfile_info(&self, item: &EmbItem) -> Result<EmbeddedFile>` | `l2-document-embfile-info` |
| `embfile_names` | method · L2 · T3 | `Document::embfile_names` | `pub fn embfile_names(&self) -> Result<Vec<String>>` | `l2-document-embfile-names` |
| `embfile_upd` | method · L4 · T3 | `Document::embfile_upd` | `pub fn embfile_upd( &mut self, _item: &EmbItem, _data: Option<&[u8]>, _options: &EmbFileOptions, ) -> Result<i32>` | `l4-document-embfile-upd` |
| `extract_font` | method · L1 · T2 | `Document::extract_font` | `pub fn extract_font( &self, _xref: i32, _info_only: bool, _name: Option<&str>, ) -> Result<FontData>` | `l1-document-extract-font` |
| `extract_image` | method · L3 · T1 | `Document::extract_image` | `pub fn extract_image(&self, _xref: i32) -> Result<ImageData>` | `l3-doc-extract-image` (+1) |
| `ez_save` | method · L2 · T3 | `Document::ez_save` | `pub fn ez_save(&self, path: &str, options: &SaveSettings) -> Result<()>` | `l2-document-ez-save` |
| `find_bookmark` | method · L2 · T3 | `Document::find_bookmark` | `pub fn find_bookmark(&self, mark: i64) -> Result<Location>` | `l2-document-find-bookmark` |
| `fullcopy_page` | method · L4 · T3 | `Document::fullcopy_page` | `pub fn fullcopy_page(&mut self, _page: i32, _to: i32) -> Result<()>` | `l2.edit.fullcopy_page` |
| `get_char_widths` | method · L1 · T3 | `Document::get_char_widths` | `pub fn get_char_widths( &self, _xref: i32, _limit: i32, _index: i32, _font_dict: Option<&Value>, ) -> Result<Vec<f64>>` | `l1-document-get-char-widths` |
| `get_layer` | method · L2 · T3 | `Document::get_layer` | `pub fn get_layer(&self, config: i32) -> Result<Ocg>` | `l2-document-get-layer` |
| `get_layers` | method · L2 · T3 | `Document::get_layers` | `pub fn get_layers(&self) -> Result<Vec<Ocg>>` | `l2-document-get-layers` |
| `get_new_xref` | method · L2 · T2 | `Document::get_new_xref` | `pub fn get_new_xref(&mut self) -> Result<i32>` | `l2-document-get-new-xref` |
| `get_oc` | method · L2 · T3 | `Document::get_oc` | `pub fn get_oc(&self, xref: i32) -> Result<i32>` | `l2-document-get-oc` |
| `get_ocgs` | method · L2 · T3 | `Document::get_ocgs` | `pub fn get_ocgs(&self) -> Result<Vec<Ocg>>` | `l2-document-get-ocgs` |
| `get_ocmd` | method · L2 · T3 | `Document::get_ocmd` | `pub fn get_ocmd(&self, xref: i32) -> Result<OcmdInfo>` | `l2-document-get-ocmd` |
| `get_outline_xrefs` | method · L2 · T3 | `Document::get_outline_xrefs` | `pub fn get_outline_xrefs(&self) -> Result<Vec<i32>>` | `l2-document-get-outline-xrefs` |
| `get_page_fonts` | method · L1 · T2 | `Document::get_page_fonts` | `pub fn get_page_fonts(&self, _page: i32, _full: bool) -> Result<Vec<FontInfo>>` | `l1-document-get-page-fonts` |
| `get_page_images` | method · L3 · T2 | `Document::get_page_images` | `pub fn get_page_images(&self, _page: i32, _full: bool) -> Result<Vec<ImageRef>>` | `l3-doc-get-page-images` |
| `get_page_labels` | method · L2 · T3 | `Document::get_page_labels` | `pub fn get_page_labels(&self) -> Result<Vec<PageLabel>>` | `l2-document-get-page-labels` |
| `get_page_numbers` | method · L2 · T3 | `Document::get_page_numbers` | `pub fn get_page_numbers(&self, label: &str, only_one: bool) -> Result<Vec<i32>>` | `l2-document-get-page-numbers` |
| `get_page_pixmap` | method · L3 · T3 | `Document::get_page_pixmap` | `pub fn get_page_pixmap(&self, _page: i32, _options: &RenderOptions) -> Result<Pixmap>` | `l3-doc-get-page-pixmap` |
| `get_page_text` | method · L1 · T3 | `Document::get_page_text` | `pub fn get_page_text(&self, _page: i32, _options: &TextOptions) -> Result<TextOutput>` | `L1.get_page_text.default` (+2) |
| `get_page_xobjects` | method · L2 · T3 | `Document::get_page_xobjects` | `pub fn get_page_xobjects(&self, page: i32) -> Result<Vec<i32>>` | `l3-page-get-xobjects` |
| `get_sigflags` | method · L2 · T3 | `Document::get_sigflags` | `pub fn get_sigflags(&self) -> Result<i32>` | `l2-document-get-sigflags` |
| `get_toc` | method · L2 · T1 | `Document::get_toc` | `pub fn get_toc(&self, simple: bool) -> Result<Vec<TocEntry>>` | `l2-toc` (+3) |
| `get_xml_metadata` | method · L2 · T3 | `Document::get_xml_metadata` | `pub fn get_xml_metadata(&self) -> Result<String>` | `l2-document-get-xml-metadata` |
| `has_annots` | method · L4 · T3 | `Document::has_annots` | `pub fn has_annots(&self) -> Result<bool>` | `l4-document-has-annots` |
| `has_links` | method · L4 · T3 | `Document::has_links` | `pub fn has_links(&self) -> Result<bool>` | `l2.doc.has_links` |
| `init_doc` | method · L2 · T3 | `Document::init_doc` | `pub fn init_doc(&mut self) -> Result<()>` | `l2-document-init-doc` |
| `insert_file` | method · L4 · T3 | `Document::insert_file` | `pub fn insert_file(&mut self, _path: &str, _options: &InsertPdfOptions) -> Result<()>` | `l4-document-insert-file` |
| `insert_page` | method · L4 · T3 | `Document::insert_page` | `pub fn insert_page(&mut self, _page: i32, _options: &InsertPageOptions) -> Result<i32>` | `l4-document-insert-page` |
| `insert_pdf` | method · L4 · T1 | `Document::insert_pdf` | `pub fn insert_pdf(&mut self, _source: &PdfDocument, _options: &InsertPdfOptions) -> Result<()>` | `l2.edit.insert_pdf_all` (+2) |
| `is_dirty` | property · L2 · T3 | `Document::is_dirty` | `pub fn is_dirty(&self) -> Result<bool>` | `l2.doc.is_dirty` |
| `is_fast_webaccess` | property · L2 · T3 | `Document::is_fast_webaccess` | `pub fn is_fast_webaccess(&self) -> Result<bool>` | `l2-document-is-fast-webaccess` |
| `is_form_pdf` | property · L2 · T2 | `Document::is_form_pdf` | `pub fn is_form_pdf(&self) -> Result<bool>` | `l2-document-is-form-pdf` |
| `is_pdf` | property · L2 · T2 | `Document::is_pdf` | `pub fn is_pdf(&self) -> Result<bool>` | `l2.doc.is_pdf` |
| `is_reflowable` | property · L2 · T3 | `Document::is_reflowable` | `pub fn is_reflowable(&self) -> Result<bool>` | `l2-document-is-reflowable` |
| `is_repaired` | property · L2 · T3 | `Document::is_repaired` | `pub fn is_repaired(&self) -> Result<bool>` | `l2-document-is-repaired` |
| `is_stream` | method · L2 · T3 | `Document::is_stream` | `pub fn is_stream(&self, xref: i32) -> Result<bool>` | `l2-document-is-stream` |
| `journal_can_do` | method · L4 · T3 | `Document::journal_can_do` | `pub fn journal_can_do(&self) -> Result<(bool, bool)>` | `l4-document-journal-can-do` |
| `journal_enable` | method · L4 · T3 | `Document::journal_enable` | `pub fn journal_enable(&mut self) -> Result<()>` | `l4-document-journal-enable` |
| `journal_is_enabled` | method · L4 · T3 | `Document::journal_is_enabled` | `pub fn journal_is_enabled(&self) -> Result<bool>` | `l4-document-journal-is-enabled` |
| `journal_load` | method · L4 · T3 | `Document::journal_load` | `pub fn journal_load(&mut self, _path: &str) -> Result<()>` | `l4-document-journal-load` |
| `journal_op_name` | method · L4 · T3 | `Document::journal_op_name` | `pub fn journal_op_name(&self, _step: i32) -> Result<String>` | `l4-document-journal-op-name` |
| `journal_position` | method · L4 · T3 | `Document::journal_position` | `pub fn journal_position(&self) -> Result<i32>` | `l4-document-journal-position` |
| `journal_redo` | method · L4 · T3 | `Document::journal_redo` | `pub fn journal_redo(&mut self) -> Result<()>` | `l4-document-journal-redo` |
| `journal_save` | method · L4 · T3 | `Document::journal_save` | `pub fn journal_save(&mut self, _path: &str) -> Result<()>` | `l4-document-journal-save` |
| `journal_start_op` | method · L4 · T3 | `Document::journal_start_op` | `pub fn journal_start_op(&mut self, _name: Option<&str>) -> Result<()>` | `l4-document-journal-start-op` |
| `journal_stop_op` | method · L4 · T3 | `Document::journal_stop_op` | `pub fn journal_stop_op(&mut self) -> Result<()>` | `l4-document-journal-stop-op` |
| `journal_undo` | method · L4 · T3 | `Document::journal_undo` | `pub fn journal_undo(&mut self) -> Result<()>` | `l4-document-journal-undo` |
| `language` | property · L2 · T3 | `Document::language` | `pub fn language(&self) -> Result<String>` | `l2-document-language` |
| `last_location` | property · L2 · T3 | `Document::last_location` | `pub fn last_location(&self) -> Result<Location>` | `l2-document-last-location` |
| `layer_ui_configs` | method · L2 · T3 | `Document::layer_ui_configs` | `pub fn layer_ui_configs(&self) -> Result<Vec<Value>>` | `l2-document-layer-ui-configs` |
| `layout` | method · L2 · T3 | `Document::layout` | `pub fn layout( &mut self, _rect: Option<Rect>, _width: f64, _height: f64, _fontsize: f64, ) -> Result<()>` | `l2-document-layout` |
| `load_page` | method · L2 · T1 | `Document::load_page` | `pub fn load_page(&self, number: i32) -> Result<Page<'_>>` | `l2.pages.load_page` (+1) |
| `location_from_page_number` | method · L2 · T3 | `Document::location_from_page_number` | `pub fn location_from_page_number(&self, page: i32) -> Result<Location>` | `l2-document-location-from-page-number` |
| `make_bookmark` | method · L2 · T3 | `Document::make_bookmark` | `pub fn make_bookmark(&self, location: Location) -> Result<i64>` | `l2-document-make-bookmark` |
| `markinfo` | property · L2 · T3 | `Document::markinfo` | `pub fn markinfo(&self) -> Result<Value>` | `l2-document-markinfo` |
| `move_page` | method · L4 · T3 | `Document::move_page` | `pub fn move_page(&mut self, _page: i32, _to: i32) -> Result<()>` | `l2.edit.move_page` (+1) |
| `name` | property · L2 · T3 | `Document::name` | `pub fn name(&self) -> Result<String>` | `l2.doc.name` |
| `need_appearances` | method · L2 · T3 | `Document::need_appearances` | `pub fn need_appearances(&self) -> Result<bool>` | `l2-document-need-appearances` |
| `needs_pass` | property · L2 · T1 | `Document::needs_pass` | `pub fn needs_pass(&self) -> Result<i32>` | `l2.crypto.needs_pass` |
| `new_page` | method · L4 · T1 | `Document::new_page` | `pub fn new_page(&mut self, _page: i32, _width: f64, _height: f64) -> Result<PageMut<'_>>` | `l2.edit.new_page_default` (+1) |
| `next_location` | method · L2 · T3 | `Document::next_location` | `pub fn next_location(&self, location: Location) -> Result<Location>` | `l2-document-next-location` |
| `outline` | property · L2 · T3 | `Document::outline` | `pub fn outline(&self) -> Result<Vec<TocEntry>>` | `l2-document-outline` |
| `page_annot_xrefs` | method · L4 · T3 | `Document::page_annot_xrefs` | `pub fn page_annot_xrefs(&self, _page: i32) -> Result<Vec<AnnotXrefRow>>` | `l4-document-page-annot-xrefs` |
| `page_count` | property · L2 · T1 | `Document::page_count` | `pub fn page_count(&self) -> usize` | `l2-page-count` (+1) |
| `page_count2` | property · L2 · T3 | `Document::page_count2` | `pub fn page_count2(&self) -> Result<(usize, usize)>` | `l2-document-page-count2` |
| `page_cropbox` | method · L2 · T3 | `Document::page_cropbox` | `pub fn page_cropbox(&self, page: i32) -> Result<Rect>` | `l2.page.cropbox` (+1) |
| `page_number_from_location` | method · L2 · T3 | `Document::page_number_from_location` | `pub fn page_number_from_location(&self, location: Location) -> Result<i32>` | `l2-document-page-number-from-location` |
| `page_xref` | method · L2 · T3 | `Document::page_xref` | `pub fn page_xref(&self, page: i32) -> Result<i32>` | `l2-document-page-xref` |
| `pagelayout` | property · L2 · T3 | `Document::pagelayout` | `pub fn pagelayout(&self) -> Result<String>` | `l2-document-pagelayout` |
| `pagemode` | property · L2 · T3 | `Document::pagemode` | `pub fn pagemode(&self) -> Result<String>` | `l2-document-pagemode` |
| `pages` | method · L2 · T2 | `Document::pages` | `pub fn pages(&self) -> Pages<'_>` | `l2.pages.iter` (+1) |
| `pdf_catalog` | method · L2 · T2 | `Document::pdf_catalog` | `pub fn pdf_catalog(&self) -> Result<String>` | `l2-document-pdf-catalog` |
| `pdf_trailer` | method · L2 · T3 | `Document::pdf_trailer` | `pub fn pdf_trailer(&self, _compressed: bool, ascii: bool) -> Result<String>` | `l2-document-pdf-trailer` |
| `permissions` | property · L2 · T3 | `Document::permissions` | `pub fn permissions(&self) -> Result<i32>` | `l2.doc.permissions` |
| `prev_location` | method · L2 · T3 | `Document::prev_location` | `pub fn prev_location(&self, location: Location) -> Result<Location>` | `l2-document-prev-location` |
| `recolor` | method · L2 · T3 | `Document::recolor` | `pub fn recolor(&mut self, _components: i32) -> Result<()>` | `l2-document-recolor` |
| `reload_page` | method · L2 · T3 | `Document::reload_page` | `pub fn reload_page(&self, page: Page<'_>) -> Result<Page<'_>>` | `l2-document-reload-page` |
| `repair` | method · L4 · T3 | `Document::repair` | `pub fn repair(&mut self) -> Result<()>` | `l4-document-repair` |
| `resolve_link` | method · L4 · T3 | `Document::resolve_link` | `pub fn resolve_link(&self, _uri: Option<&str>, _chapters: i32) -> Result<Value>` | `l4-document-resolve-link` |
| `resolve_names` | method · L2 · T3 | `Document::resolve_names` | `pub fn resolve_names(&mut self) -> Result<()>` | `l2-document-resolve-names` |
| `rewrite_images` | method · L3 · T3 | `Document::rewrite_images` | `pub fn rewrite_images(&mut self, _options: &RewriteOptions) -> Result<()>` | `l3-doc-rewrite-images` |
| `save` | method · L2 · T1 | `Document::save_full` | `pub fn save_full(&self, path: &str, options: &SaveSettings) -> Result<()>` | `l2.crypto.save_aes256_roundtrip` (+6) |
| `saveIncr` | method · L2 · T2 | `Document::save_incr` | `pub fn save_incr(&self) -> Result<()>` | `l2.save.saveIncr` |
| `save_snapshot` | method · L2 · T3 | `Document::save_snapshot` | `pub fn save_snapshot(&self, path: &str) -> Result<()>` | `l2-document-save-snapshot` |
| `scrub` | method · L4 · T3 | `Document::scrub` | `pub fn scrub(&mut self, _options: &ScrubOptions) -> Result<()>` | `l4-document-scrub` |
| `search_page_for` | method · L1 · T3 | `Document::search_page_for` | `pub fn search_page_for( &self, _page: i32, _needle: &str, _options: &SearchOptions, ) -> Result<SearchHits>` | `L1.search_for.empty_page` (+3) |
| `select` | method · L4 · T3 | `Document::select` | `pub fn select(&mut self, _pages: &[i32]) -> Result<()>` | `l2.edit.select` (+2) |
| `set_language` | method · L4 · T3 | `Document::set_language` | `pub fn set_language(&mut self, _language: Option<&str>) -> Result<bool>` | `l4-document-set-language` |
| `set_layer` | method · L4 · T3 | `Document::set_layer` | `pub fn set_layer(&mut self, _config: i32, _options: &LayerOptions) -> Result<()>` | `l4-document-set-layer` |
| `set_layer_ui_config` | method · L4 · T3 | `Document::set_layer_ui_config` | `pub fn set_layer_ui_config(&mut self, _number: i32, _action: i32) -> Result<()>` | `l4-document-set-layer-ui-config` |
| `set_markinfo` | method · L4 · T3 | `Document::set_markinfo` | `pub fn set_markinfo(&mut self, _markinfo: &Value) -> Result<bool>` | `l4-document-set-markinfo` |
| `set_metadata` | method · L4 · T1 | `Document::set_metadata` | `pub fn set_metadata(&mut self, _metadata: &DocMetadata) -> Result<()>` | `l2.metadata.set_badkey` (+3) |
| `set_oc` | method · L4 · T3 | `Document::set_oc` | `pub fn set_oc(&mut self, _xref: i32, _oc: i32) -> Result<()>` | `l4-document-set-oc` |
| `set_ocmd` | method · L4 · T3 | `Document::set_ocmd` | `pub fn set_ocmd(&mut self, _xref: i32, _options: &OcmdOptions) -> Result<i32>` | `l4-document-set-ocmd` |
| `set_page_labels` | method · L4 · T3 | `Document::set_page_labels` | `pub fn set_page_labels(&mut self, _labels: &[PageLabel]) -> Result<()>` | `l2.pages.labels_roundtrip` |
| `set_pagelayout` | method · L4 · T3 | `Document::set_pagelayout` | `pub fn set_pagelayout(&mut self, _pagelayout: &str) -> Result<()>` | `l4-document-set-pagelayout` |
| `set_pagemode` | method · L4 · T3 | `Document::set_pagemode` | `pub fn set_pagemode(&mut self, _pagemode: &str) -> Result<()>` | `l4-document-set-pagemode` |
| `set_toc` | method · L4 · T2 | `Document::set_toc` | `pub fn set_toc(&mut self, _toc: &[TocEntry], _collapse: i32) -> Result<i32>` | `l2.toc.set_bad` (+3) |
| `set_toc_item` | method · L4 · T3 | `Document::set_toc_item` | `pub fn set_toc_item(&mut self, _index: i32, _options: &TocItemOptions) -> Result<()>` | `l2.toc.set_item_title` |
| `set_xml_metadata` | method · L4 · T3 | `Document::set_xml_metadata` | `pub fn set_xml_metadata(&mut self, _metadata: &str) -> Result<()>` | `l4-document-set-xml-metadata` |
| `subset_fonts` | method · L4 · T3 | `Document::subset_fonts` | `pub fn subset_fonts(&mut self) -> Result<FontSubsetReport>` | `l4-document-subset-fonts` |
| `switch_layer` | method · L2 · T3 | `Document::switch_layer` | `pub fn switch_layer(&mut self, config: i32, _as_default: bool) -> Result<()>` | `l2-document-switch-layer` |
| `this` | property · L2 · T3 | `Document::this` | `pub fn this(&self) -> usize` | `l2-document-this` |
| `this_is_pdf` | property · L2 · T3 | `Document::this_is_pdf` | `pub fn this_is_pdf(&self) -> Result<bool>` | `l2-document-this-is-pdf` |
| `tobytes` | method · L2 · T1 | `Document::tobytes` | `pub fn tobytes(&self, options: &SaveSettings) -> Result<Vec<u8>>` | `l2.crypto.tobytes_encrypted` |
| `update_object` | method · L4 · T2 | `Document::update_object` | `pub fn update_object(&mut self, _xref: i32, _text: &str, _page: Option<i32>) -> Result<()>` | `l4-document-update-object` |
| `update_stream` | method · L4 · T2 | `Document::update_stream` | `pub fn update_stream( &mut self, _xref: i32, _data: Option<&[u8]>, _new: bool, _compress: bool, ) -> Result<()>` | `l4-document-update-stream` |
| `version_count` | property · L2 · T3 | `Document::version_count` | `pub fn version_count(&self) -> Result<usize>` | `l2-document-version-count` |
| `write` | method · L2 · T2 | `Document::write` | `pub fn write(&self, options: &SaveSettings) -> Result<Vec<u8>>` | `l2-document-write` |
| `xref` | property · L2 · T3 | `Document::xref` | `pub fn xref(&self) -> Result<i32>` | `l2-document-xref` |
| `xref_copy` | method · L4 · T3 | `Document::xref_copy` | `pub fn xref_copy( &mut self, _source: i32, _target: i32, _keep: Option<&[String]>, ) -> Result<()>` | `l4-document-xref-copy` |
| `xref_get_key` | method · L2 · T2 | `Document::xref_get_key` | `pub fn xref_get_key(&self, xref: i32, key: &str) -> Result<(String, String)>` | `l2-document-xref-get-key` |
| `xref_get_keys` | method · L2 · T3 | `Document::xref_get_keys` | `pub fn xref_get_keys(&self, xref: i32) -> Result<Vec<String>>` | `l2-document-xref-get-keys` |
| `xref_is_font` | method · L1 · T3 | `Document::xref_is_font` | `pub fn xref_is_font(&self, _xref: i32) -> Result<bool>` | `l1-document-xref-is-font` |
| `xref_is_image` | method · L3 · T3 | `Document::xref_is_image` | `pub fn xref_is_image(&self, _xref: i32) -> Result<bool>` | `l3-doc-xref-is-image` |
| `xref_is_stream` | method · L2 · T3 | `Document::xref_is_stream` | `pub fn xref_is_stream(&self, xref: i32) -> Result<bool>` | `l2-document-xref-is-stream` |
| `xref_is_xobject` | method · L2 · T3 | `Document::xref_is_xobject` | `pub fn xref_is_xobject(&self, xref: i32) -> Result<bool>` | `l2-document-xref-is-xobject` |
| `xref_length` | method · L2 · T2 | `Document::xref_length` | `pub fn xref_length(&self) -> Result<usize>` | `l2-document-xref-length` |
| `xref_object` | method · L2 · T2 | `Document::xref_object` | `pub fn xref_object(&self, xref: i32, _compressed: bool, ascii: bool) -> Result<String>` | `l2-document-xref-object` |
| `xref_set_key` | method · L4 · T2 | `Document::xref_set_key` | `pub fn xref_set_key(&mut self, _xref: i32, _key: &str, _value: &str) -> Result<()>` | `l4-document-xref-set-key` |
| `xref_stream` | method · L2 · T2 | `Document::xref_stream` | `pub fn xref_stream(&self, xref: i32) -> Result<Vec<u8>>` | `l2-document-xref-stream` |
| `xref_stream_raw` | method · L2 · T3 | `Document::xref_stream_raw` | `pub fn xref_stream_raw(&self, xref: i32) -> Result<Vec<u8>>` | `l2-document-xref-stream-raw` |
| `xref_xml_metadata` | method · L2 · T3 | `Document::xref_xml_metadata` | `pub fn xref_xml_metadata(&self) -> Result<i32>` | `l2-document-xref-xml-metadata` |

## Page

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L2 · T3 | `Page::PartialEq` | `impl PartialEq for Page` | `l2-page-eq` |
| `__ge__` | operator · L2 · T3 | `Page::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-page-ge` |
| `__gt__` | operator · L2 · T3 | `Page::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-page-gt` |
| `__init__` | constructor · L2 · T1 | `Page::new` | `pub fn new(doc: &'a PdfDocument, index: usize) -> Self` | `l2-page-init` |
| `__le__` | operator · L2 · T3 | `Page::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-page-le` |
| `__lt__` | operator · L2 · T3 | `Page::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-page-lt` |
| `__ne__` | operator · L2 · T3 | `Page::PartialEq` | `impl PartialEq for Page` | `l2-page-ne` |
| `add_caret_annot` | method · L4 · T3 | `Page::add_caret_annot` | `pub fn add_caret_annot(&mut self, _point: Point) -> Result<Annot<'_>>` | `L4.add_caret_annot.basic` |
| `add_circle_annot` | method · L4 · T3 | `Page::add_circle_annot` | `pub fn add_circle_annot(&mut self, _rect: Rect) -> Result<Annot<'_>>` | `L4.add_circle_annot.basic` |
| `add_file_annot` | method · L4 · T3 | `Page::add_file_annot` | `pub fn add_file_annot( &mut self, _point: Point, _data: &[u8], _options: &FileAnnotOptions, ) -> Result<Annot<'_>>` | `L4.add_file_annot.basic` |
| `add_freetext_annot` | method · L4 · T3 | `Page::add_freetext_annot` | `pub fn add_freetext_annot( &mut self, _rect: Rect, _text: &str, _options: &FreeTextOptions, ) -> Result<Annot<'_>>` | `L4.add_freetext_annot.basic` (+1) |
| `add_highlight_annot` | method · L4 · T2 | `Page::add_highlight_annot` | `pub fn add_highlight_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>>` | `L4.add_highlight_annot.quads` (+1) |
| `add_ink_annot` | method · L4 · T3 | `Page::add_ink_annot` | `pub fn add_ink_annot(&mut self, _handwriting: &[Vec<Point>]) -> Result<Annot<'_>>` | `L4.add_ink_annot.basic` |
| `add_line_annot` | method · L4 · T3 | `Page::add_line_annot` | `pub fn add_line_annot(&mut self, _p1: Point, _p2: Point) -> Result<Annot<'_>>` | `L4.add_line_annot.basic` |
| `add_polygon_annot` | method · L4 · T3 | `Page::add_polygon_annot` | `pub fn add_polygon_annot(&mut self, _points: &[Point]) -> Result<Annot<'_>>` | `L4.add_polygon_annot.basic` |
| `add_polyline_annot` | method · L4 · T3 | `Page::add_polyline_annot` | `pub fn add_polyline_annot(&mut self, _points: &[Point]) -> Result<Annot<'_>>` | `L4.add_polyline_annot.basic` |
| `add_rect_annot` | method · L4 · T2 | `Page::add_rect_annot` | `pub fn add_rect_annot(&mut self, _rect: Rect) -> Result<Annot<'_>>` | `L4.add_rect_annot.basic` |
| `add_redact_annot` | method · L4 · T1 | `Page::add_redact_annot` | `pub fn add_redact_annot( &mut self, _quad: Quad, _options: &RedactAnnotOptions, ) -> Result<Annot<'_>>` | `L4.add_redact_annot.basic` |
| `add_squiggly_annot` | method · L4 · T3 | `Page::add_squiggly_annot` | `pub fn add_squiggly_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>>` | `L4.add_squiggly_annot.quads` |
| `add_stamp_annot` | method · L4 · T3 | `Page::add_stamp_annot` | `pub fn add_stamp_annot(&mut self, _rect: Rect, _stamp: i32) -> Result<Annot<'_>>` | `L4.add_stamp_annot.basic` |
| `add_strikeout_annot` | method · L4 · T3 | `Page::add_strikeout_annot` | `pub fn add_strikeout_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>>` | `L4.add_strikeout_annot.quads` |
| `add_text_annot` | method · L4 · T2 | `Page::add_text_annot` | `pub fn add_text_annot(&mut self, _point: Point, _text: &str, _icon: &str) -> Result<Annot<'_>>` | `L4.add_text_annot.basic` |
| `add_underline_annot` | method · L4 · T3 | `Page::add_underline_annot` | `pub fn add_underline_annot(&mut self, _options: &HighlightOptions) -> Result<Annot<'_>>` | `L4.add_underline_annot.quads` |
| `add_widget` | method · L4 · T2 | `Page::add_widget` | `pub fn add_widget(&mut self, _widget: &Widget) -> Result<Annot<'_>>` | `L4.widget.add` |
| `annot_names` | method · L4 · T3 | `Page::annot_names` | `pub fn annot_names(&self) -> Result<Vec<String>>` | `L4.annot_names.list` |
| `annot_xrefs` | method · L4 · T3 | `Page::annot_xrefs` | `pub fn annot_xrefs(&self) -> Result<Vec<AnnotXref>>` | `l4-page-annot-xrefs` |
| `annots` | method · L4 · T2 | `Page::annots` | `pub fn annots(&self, _types: Option<&[i32]>) -> Result<Vec<AnnotEntry>>` | `L4.annots.list` (+1) |
| `apply_redactions` | method · L4 · T1 | `Page::apply_redactions` | `pub fn apply_redactions(&mut self, _options: &RedactApplyOptions) -> Result<bool>` | `L4.apply_redactions.text` |
| `artbox` | property · L2 · T3 | `Page::artbox` | `pub fn artbox(&self) -> Result<Rect>` | `l2.page.boxes` |
| `bleedbox` | property · L2 · T3 | `Page::bleedbox` | `pub fn bleedbox(&self) -> Result<Rect>` | `l2-page-bleedbox` |
| `bound` | method · L2 · T3 | `Page::bound` | `pub fn bound(&self) -> Result<Rect>` | `l2-page-bound` |
| `clean_contents` | method · L4 · T2 | `Page::clean_contents` | `pub fn clean_contents(&mut self, _sanitize: bool) -> Result<()>` | `L4.annot.appearance_state` |
| `clip_to_rect` | method · L2 · T3 | `Page::clip_to_rect` | `pub fn clip_to_rect(&mut self, rect: Rect) -> Result<()>` | `l2-page-clip-to-rect` |
| `cluster_drawings` | method · L4 · T3 | `Page::cluster_drawings` | `pub fn cluster_drawings(&self, _options: &ClusterOptions) -> Result<Vec<DrawingCluster>>` | `l4-page-cluster-drawings` |
| `cropbox` | property · L2 · T3 | `Page::cropbox` | `pub fn cropbox(&self) -> Result<Rect>` | `l2-page-cropbox` |
| `cropbox_position` | property · L2 · T3 | `Page::cropbox_position` | `pub fn cropbox_position(&self) -> Result<Point>` | `l2-page-cropbox-position` |
| `delete_annot` | method · L4 · T2 | `Page::delete_annot` | `pub fn delete_annot(&mut self, _annot: &AnnotEntry) -> Result<()>` | `L4.delete_annot.basic` |
| `delete_image` | method · L4 · T3 | `Page::delete_image` | `pub fn delete_image(&mut self, _xref: i32) -> Result<()>` | `l4-page-delete-image` |
| `delete_link` | method · L4 · T3 | `Page::delete_link` | `pub fn delete_link(&mut self, _link: &Link) -> Result<()>` | `l2.links.delete` (+1) |
| `delete_widget` | method · L4 · T3 | `Page::delete_widget` | `pub fn delete_widget(&mut self, _widget: &Widget) -> Result<Widget>` | `l4-page-delete-widget` |
| `derotation_matrix` | property · L2 · T3 | `Page::derotation_matrix` | `pub fn derotation_matrix(&self) -> Result<Matrix>` | `l2-page-derotation-matrix` |
| `draw_bezier` | method · L4 · T2 | `Page::draw_bezier` | `pub fn draw_bezier( &mut self, _p1: Point, _p2: Point, _p3: Point, _p4: Point, _options: &DrawOptions, ) -> Result<Point>` | `l4-page-draw-bezier` |
| `draw_circle` | method · L4 · T2 | `Page::draw_circle` | `pub fn draw_circle( &mut self, _center: Point, _radius: f64, _options: &DrawOptions, ) -> Result<Point>` | `L4.page.draw_shapes` |
| `draw_curve` | method · L4 · T3 | `Page::draw_curve` | `pub fn draw_curve( &mut self, _p1: Point, _p2: Point, _p3: Point, _options: &DrawOptions, ) -> Result<Point>` | `l4-page-draw-curve` |
| `draw_line` | method · L4 · T1 | `Page::draw_line` | `pub fn draw_line(&mut self, _p1: Point, _p2: Point, _options: &DrawOptions) -> Result<Point>` | `L4.page.draw_shapes` |
| `draw_oval` | method · L4 · T3 | `Page::draw_oval` | `pub fn draw_oval(&mut self, _rect: Rect, _options: &DrawOptions) -> Result<Point>` | `l4-page-draw-oval` |
| `draw_polyline` | method · L4 · T3 | `Page::draw_polyline` | `pub fn draw_polyline(&mut self, _points: &[Point], _options: &DrawOptions) -> Result<Point>` | `l4-page-draw-polyline` |
| `draw_quad` | method · L4 · T3 | `Page::draw_quad` | `pub fn draw_quad(&mut self, _quad: Quad, _options: &DrawOptions) -> Result<Point>` | `l4-page-draw-quad` |
| `draw_rect` | method · L4 · T1 | `Page::draw_rect` | `pub fn draw_rect( &mut self, _rect: Rect, _options: &DrawOptions, _radius: Option<f64>, ) -> Result<Point>` | `L4.page.draw_shapes` |
| `draw_sector` | method · L4 · T3 | `Page::draw_sector` | `pub fn draw_sector( &mut self, _center: Point, _point: Point, _beta: f64, _full_sector: bool, _options: &DrawOptions, ) -> Result<Point>` | `l4-page-draw-sector` |
| `draw_squiggle` | method · L4 · T3 | `Page::draw_squiggle` | `pub fn draw_squiggle( &mut self, _p1: Point, _p2: Point, _breadth: f64, _options: &DrawOptions, ) -> Result<Point>` | `l4-page-draw-squiggle` |
| `draw_zigzag` | method · L4 · T3 | `Page::draw_zigzag` | `pub fn draw_zigzag( &mut self, _p1: Point, _p2: Point, _breadth: f64, _options: &DrawOptions, ) -> Result<Point>` | `l4-page-draw-zigzag` |
| `extend_textpage` | method · L1 · T3 | `Page::extend_textpage` | `pub fn extend_textpage( &self, _target: &mut TextPage<'_>, _flags: i32, _matrix: Option<Matrix>, ) -> Result<()>` | `L1.extend_textpage.default` (+1) |
| `find_tables` | method · L1 · T1 | `Page::find_tables` | `pub fn find_tables(&self, _options: &TableOptions) -> Result<TableFinder>` | `L4.find_tables.hit` (+1) |
| `first_annot` | property · L4 · T2 | `Page::first_annot` | `pub fn first_annot(&self) -> Result<Option<AnnotEntry>>` | `L4.first_annot.chain` (+1) |
| `first_link` | property · L4 · T3 | `Page::first_link` | `pub fn first_link(&self) -> Result<Option<Link>>` | `l2.links.first_next` |
| `first_widget` | property · L4 · T3 | `Page::first_widget` | `pub fn first_widget(&self) -> Result<Option<Widget>>` | `l4-page-first-widget` |
| `get_bboxlog` | method · L2 · T3 | `Page::get_bboxlog` | `pub fn get_bboxlog(&self, layers: Option<&Value>) -> Result<Value>` | `l2-page-get-bboxlog` |
| `get_cdrawings` | method · L4 · T3 | `Page::get_cdrawings` | `pub fn get_cdrawings(&self, _options: &CdrawingsOptions) -> Result<Vec<Drawing>>` | `l4-page-get-cdrawings` |
| `get_contents` | method · L2 · T2 | `Page::get_contents` | `pub fn get_contents(&self) -> Result<Vec<i32>>` | `l2-page-get-contents` |
| `get_displaylist` | method · L2 · T3 | `Page::get_displaylist` | `pub fn get_displaylist(&self, _annots: bool) -> Result<DisplayList>` | `l2-page-get-displaylist` |
| `get_drawings` | method · L4 · T1 | `Page::get_drawings` | `pub fn get_drawings(&self, _extended: bool) -> Result<Vec<Drawing>>` | `L4.get_drawings.default` (+1) |
| `get_fonts` | method · L1 · T2 | `Page::get_fonts` | `pub fn get_fonts(&self, _full: bool) -> Result<Vec<FontInfo>>` | `L4.insert_font.basic` |
| `get_image_bbox` | method · L3 · T3 | `Page::get_image_bbox` | `pub fn get_image_bbox(&self, _name: &str, _transform: bool) -> Result<Rect>` | `l3-page-get-image-bbox` (+1) |
| `get_image_info` | method · L3 · T2 | `Page::get_image_info` | `pub fn get_image_info(&self, _hashes: bool, _xrefs: bool) -> Result<Vec<PlacedImage>>` | `l3-page-get-image-info` (+2) |
| `get_image_rects` | method · L3 · T2 | `Page::get_image_rects` | `pub fn get_image_rects(&self, _name: &str, _transform: bool) -> Result<Vec<Rect>>` | `l3-page-get-image-rects` (+2) |
| `get_images` | method · L3 · T1 | `Page::get_images` | `pub fn get_images(&self, _full: bool) -> Result<Vec<ImageRef>>` | `l3-page-get-images` (+2) |
| `get_label` | method · L2 · T3 | `Page::get_label` | `pub fn get_label(&self) -> Result<String>` | `l2-page-get-label` |
| `get_layout` | method · L2 · T3 | `Page::get_layout` | `pub fn get_layout(&self, _options: &LayoutOptions) -> Result<Option<Value>>` | `L1.get_layout.default` |
| `get_links` | method · L4 · T2 | `Page::get_links` | `pub fn get_links(&self) -> Result<Vec<Link>>` | `l2-page-links` (+1) |
| `get_oc_items` | method · L2 · T3 | `Page::get_oc_items` | `pub fn get_oc_items(&self) -> Result<Vec<Value>>` | `l2-page-get-oc-items` |
| `get_pixmap` | method · L3 · T1 | `Page::get_pixmap` | `pub fn get_pixmap(&self, _options: &RenderOptions) -> Result<Pixmap>` | `l3-page-get-pixmap-alpha` (+9) |
| `get_svg_image` | method · L3 · T2 | `Page::get_svg_image` | `pub fn get_svg_image(&self, _matrix: Option<Matrix>, _text_as_path: bool) -> Result<String>` | `l3-page-get-svg-image` (+1) |
| `get_text` | method · L1 · T1 | `Page::get_text` | `pub fn get_text(&self, _options: &TextOptions) -> Result<TextOutput>` | `L1.get_text.bogus_kw` (+28) |
| `get_text_blocks` | method · L1 · T3 | `Page::get_text_blocks` | `pub fn get_text_blocks(&self, _options: &BlockOptions) -> Result<Vec<TextBlock>>` | `L1.get_text.blocks` (+6) |
| `get_text_selection` | method · L1 · T3 | `Page::get_text_selection` | `pub fn get_text_selection( &self, _begin: Point, _end: Point, _clip: Option<Rect>, ) -> Result<String>` | `L1.get_text_selection.clip` (+3) |
| `get_text_words` | method · L1 · T3 | `Page::get_text_words` | `pub fn get_text_words(&self, _options: &WordOptions) -> Result<Vec<TextWord>>` | `L1.get_page_text.words_clip` (+10) |
| `get_textbox` | method · L1 · T2 | `Page::get_textbox` | `pub fn get_textbox(&self, _rect: Rect, _textpage: Option<TextPage<'_>>) -> Result<String>` | `L1.get_textbox.empty` (+2) |
| `get_textpage` | method · L1 · T3 | `Page::get_textpage` | `pub fn get_textpage( &self, _clip: Option<Rect>, _flags: i32, _matrix: Option<Matrix>, ) -> Result<TextPage<'_>>` | `L1.get_textpage.bogus_kw` (+6) |
| `get_textpage_ocr` | method · L1 · T3 | `Page::get_textpage_ocr` | `pub fn get_textpage_ocr(&self, _options: &OcrOptions) -> Result<TextPage<'_>>` | `l1-page-get-textpage-ocr` |
| `get_texttrace` | method · L1 · T2 | `Page::get_texttrace` | `pub fn get_texttrace(&self) -> Result<Vec<TextTrace>>` | `L1.get_texttrace.default` |
| `get_xobjects` | method · L2 · T3 | `Page::get_xobjects` | `pub fn get_xobjects(&self) -> Result<Vec<i32>>` | `l2-page-get-xobjects` |
| `insert_font` | method · L4 · T2 | `Page::insert_font` | `pub fn insert_font(&mut self, _options: &InsertFontOptions) -> Result<i32>` | `L4.insert_font.basic` |
| `insert_htmlbox` | method · L4 · T3 | `Page::insert_htmlbox` | `pub fn insert_htmlbox( &mut self, _rect: Rect, _text: &str, _options: &HtmlBoxOptions, ) -> Result<(f64, f64)>` | `l4-page-insert-htmlbox` |
| `insert_image` | method · L4 · T1 | `Page::insert_image` | `pub fn insert_image(&mut self, _rect: Rect, _options: &ImageInsertOptions) -> Result<i32>` | `L4.insert_image.pixmap` (+1) |
| `insert_link` | method · L4 · T2 | `Page::insert_link` | `pub fn insert_link(&mut self, _link: &Link, _mark: bool) -> Result<()>` | `l2.links.insert_bad` (+2) |
| `insert_text` | method · L4 · T1 | `Page::insert_text` | `pub fn insert_text( &mut self, _point: Point, _text: &TextArg, _options: &InsertTextOptions, ) -> Result<i32>` | `L4.insert_text.basic` (+2) |
| `insert_textbox` | method · L4 · T1 | `Page::insert_textbox` | `pub fn insert_textbox( &mut self, _rect: Rect, _text: &TextArg, _options: &TextBoxOptions, ) -> Result<f64>` | `L4.insert_textbox.align` (+1) |
| `is_wrapped` | property · L2 · T3 | `Page::is_wrapped` | `pub fn is_wrapped(&self) -> Result<bool>` | `l2-page-is-wrapped` |
| `language` | property · L2 · T3 | `Page::language` | `pub fn language(&self) -> Result<String>` | `l2-page-language` |
| `links` | method · L4 · T3 | `Page::links` | `pub fn links(&self, _kinds: Option<&[i32]>) -> Result<Vec<Link>>` | `l2.links.iter` (+1) |
| `load_annot` | method · L4 · T3 | `Page::load_annot` | `pub fn load_annot(&mut self, _ident: &AnnotIdent) -> Result<Annot<'_>>` | `L4.load_annot.basic` |
| `load_links` | method · L4 · T3 | `Page::load_links` | `pub fn load_links(&self) -> Result<Vec<Link>>` | `l4-page-load-links` |
| `load_widget` | method · L4 · T3 | `Page::load_widget` | `pub fn load_widget(&self, _xref: i32) -> Result<Widget>` | `L4.load_widget.delete` |
| `mediabox` | property · L2 · T2 | `Page::mediabox` | `pub fn mediabox(&self) -> Result<Rect>` | `l2.page.mediabox` |
| `mediabox_size` | property · L2 · T3 | `Page::mediabox_size` | `pub fn mediabox_size(&self) -> Result<(f64, f64)>` | `l2-page-mediabox-size` |
| `new_shape` | method · L4 · T2 | `Page::new_shape` | `pub fn new_shape(&mut self) -> Result<Shape<'_>>` | `L4.new_shape.bezier` (+6) |
| `read_contents` | method · L2 · T3 | `Page::read_contents` | `pub fn read_contents(&self) -> Result<Vec<u8>>` | `l2-page-read-contents` |
| `recolor` | method · L2 · T3 | `Page::recolor` | `pub fn recolor(&mut self, _components: i32) -> Result<()>` | `l2-page-recolor` |
| `rect` | property · L2 · T1 | `Page::rect` | `pub fn rect(&self) -> Result<Rect>` | `l2.page.rect` |
| `refresh` | method · L2 · T3 | `Page::refresh` | `pub fn refresh(&mut self) -> Result<()>` | `l2-page-refresh` |
| `remove_rotation` | method · L4 · T3 | `Page::remove_rotation` | `pub fn remove_rotation(&mut self) -> Result<Matrix>` | `l2.page.remove_rotation` |
| `replace_image` | method · L4 · T3 | `Page::replace_image` | `pub fn replace_image(&mut self, _xref: i32, _options: &ReplaceImageOptions) -> Result<()>` | `l4-page-replace-image` |
| `rotation` | property · L2 · T1 | `Page::rotation` | `pub fn rotation(&self) -> Result<i32>` | `l2-page-rotation` (+1) |
| `rotation_matrix` | property · L2 · T3 | `Page::rotation_matrix` | `pub fn rotation_matrix(&self) -> Result<Matrix>` | `l2-page-rotation-matrix` |
| `run` | method · L2 · T3 | `Page::run` | `pub fn run(&self, _display_list: &DisplayList, _matrix: Matrix) -> Result<()>` | `l2-page-run` |
| `search_for` | method · L1 · T1 | `Page::search_for` | `pub fn search_for(&self, _needle: &str, _options: &SearchOptions) -> Result<SearchHits>` | `L1.search_for.clip_miss` (+7) |
| `set_artbox` | method · L4 · T3 | `Page::set_artbox` | `pub fn set_artbox(&mut self, _rect: Rect) -> Result<()>` | `l4-page-set-artbox` |
| `set_bleedbox` | method · L4 · T3 | `Page::set_bleedbox` | `pub fn set_bleedbox(&mut self, _rect: Rect) -> Result<()>` | `l4-page-set-bleedbox` |
| `set_contents` | method · L4 · T3 | `Page::set_contents` | `pub fn set_contents(&mut self, _xref: i32) -> Result<()>` | `l4-page-set-contents` |
| `set_cropbox` | method · L4 · T2 | `Page::set_cropbox` | `pub fn set_cropbox(&mut self, _rect: Rect) -> Result<()>` | `l2.page.set_cropbox_bad` |
| `set_language` | method · L4 · T3 | `Page::set_language` | `pub fn set_language(&mut self, _language: Option<&str>) -> Result<()>` | `l4-page-set-language` |
| `set_mediabox` | method · L4 · T3 | `Page::set_mediabox` | `pub fn set_mediabox(&mut self, _rect: Rect) -> Result<()>` | `l2.page.set_mediabox` |
| `set_rotation` | method · L4 · T2 | `Page::set_rotation` | `pub fn set_rotation(&mut self, _rotation: i32) -> Result<()>` | `l2.page.set_rotation_180` (+4) |
| `set_trimbox` | method · L4 · T3 | `Page::set_trimbox` | `pub fn set_trimbox(&mut self, _rect: Rect) -> Result<()>` | `l4-page-set-trimbox` |
| `show_pdf_page` | method · L4 · T1 | `Page::show_pdf_page` | `pub fn show_pdf_page( &mut self, _rect: Rect, _source: &PdfDocument, _options: &ShowPageOptions, ) -> Result<i32>` | `l4-page-show-pdf-page` |
| `transformation_matrix` | property · L2 · T3 | `Page::transformation_matrix` | `pub fn transformation_matrix(&self) -> Result<Matrix>` | `l2-page-transformation-matrix` |
| `trimbox` | property · L2 · T3 | `Page::trimbox` | `pub fn trimbox(&self) -> Result<Rect>` | `l2-page-trimbox` |
| `update_link` | method · L4 · T3 | `Page::update_link` | `pub fn update_link(&mut self, _link: &Link) -> Result<()>` | `l4-page-update-link` |
| `widgets` | method · L4 · T2 | `Page::widgets` | `pub fn widgets(&self, _types: Option<&[i32]>) -> Result<Vec<Widget>>` | `L4.widgets.list` (+2) |
| `wrap_contents` | method · L2 · T3 | `Page::wrap_contents` | `pub fn wrap_contents(&mut self) -> Result<()>` | `l2-page-wrap-contents` |
| `write_text` | method · L4 · T3 | `Page::write_text` | `pub fn write_text(&mut self, _options: &WriteTextOptions) -> Result<()>` | `l4-page-write-text` |
| `xref` | property · L2 · T2 | `Page::xref` | `pub fn xref(&self) -> Result<i32>` | `l2-page-xref` |

## TextPage

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L1 · T3 | `TextPage::PartialEq` | `impl PartialEq for TextPage` | `l1-textpage-eq` |
| `__ge__` | operator · L1 · T3 | `TextPage::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l1-textpage-ge` |
| `__gt__` | operator · L1 · T3 | `TextPage::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l1-textpage-gt` |
| `__init__` | constructor · L1 · T3 | `TextPage::new` | `pub fn new(_page: Page<'a>) -> Self` | `l1-textpage-init` |
| `__le__` | operator · L1 · T3 | `TextPage::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l1-textpage-le` |
| `__lt__` | operator · L1 · T3 | `TextPage::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l1-textpage-lt` |
| `__ne__` | operator · L1 · T3 | `TextPage::PartialEq` | `impl PartialEq for TextPage` | `l1-textpage-ne` |
| `extractBLOCKS` | method · L1 · T3 | `TextPage::extract_blocks` | `pub fn extract_blocks(&self) -> Result<Vec<TextBlock>>` | `L1.textpage.extractBLOCKS` |
| `extractDICT` | method · L1 · T3 | `TextPage::extract_dict` | `pub fn extract_dict(&self, _sort: bool) -> Result<TextDict>` | `L1.textpage.extractDICT` (+1) |
| `extractHTML` | method · L1 · T3 | `TextPage::extract_html` | `pub fn extract_html(&self) -> Result<String>` | `L1.textpage.extractHTML` |
| `extractIMGINFO` | method · L1 · T3 | `TextPage::extract_imginfo` | `pub fn extract_imginfo(&self, _hashes: bool) -> Result<Vec<PlacedImage>>` | `L1.textpage.extractIMGINFO` |
| `extractJSON` | method · L1 · T3 | `TextPage::extract_json` | `pub fn extract_json(&self, _sort: bool) -> Result<String>` | `L1.textpage.extractJSON` |
| `extractRAWDICT` | method · L1 · T3 | `TextPage::extract_rawdict` | `pub fn extract_rawdict(&self, _sort: bool) -> Result<TextDict>` | `L1.textpage.extractRAWDICT` (+1) |
| `extractRAWJSON` | method · L1 · T3 | `TextPage::extract_rawjson` | `pub fn extract_rawjson(&self, _sort: bool) -> Result<String>` | `L1.textpage.extractRAWJSON` |
| `extractSelection` | method · L1 · T3 | `TextPage::extract_selection` | `pub fn extract_selection(&self, _begin: Point, _end: Point) -> Result<String>` | `L1.textpage.extractSelection` |
| `extractTEXT` | method · L1 · T3 | `TextPage::extract_text` | `pub fn extract_text(&self, _sort: bool) -> Result<String>` | `L1.textpage.empty_text` (+2) |
| `extractText` | method · L1 · T3 | `TextPage::extract_text` | `pub fn extract_text(&self, _sort: bool) -> Result<String>` | `L1.textpage.extractText.alias` |
| `extractTextbox` | method · L1 · T3 | `TextPage::extract_textbox` | `pub fn extract_textbox(&self, _rect: Rect) -> Result<String>` | `L1.textpage.extractTextbox` |
| `extractWORDS` | method · L1 · T3 | `TextPage::extract_words` | `pub fn extract_words(&self, _delimiters: Option<&str>) -> Result<Vec<TextWord>>` | `L1.textpage.extractWORDS` (+1) |
| `extractXHTML` | method · L1 · T3 | `TextPage::extract_xhtml` | `pub fn extract_xhtml(&self) -> Result<String>` | `L1.textpage.extractXHTML` |
| `extractXML` | method · L1 · T3 | `TextPage::extract_xml` | `pub fn extract_xml(&self) -> Result<String>` | `L1.textpage.extractXML` |
| `poolsize` | method · L1 · T3 | `TextPage::poolsize` | `pub fn poolsize(&self) -> Result<usize>` | `L1.textpage.poolsize` |
| `rect` | property · L1 · T3 | `TextPage::rect` | `pub fn rect(&self) -> Result<Rect>` | `L1.textpage.rect` (+1) |
| `search` | method · L1 · T3 | `TextPage::search` | `pub fn search(&self, _needle: &str, _hit_max: i32, _quads: bool) -> Result<SearchHits>` | `L1.textpage.search_hit_max` (+3) |

## Pixmap

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L3 · T3 | `Pixmap::PartialEq` | `derived PartialEq on Pixmap` | `l3-pixmap-eq` |
| `__ge__` | operator · L3 · T3 | `Pixmap::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-pixmap-ge` |
| `__gt__` | operator · L3 · T3 | `Pixmap::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-pixmap-gt` |
| `__init__` | constructor · L3 · T1 | `Pixmap::new` | `pub fn new(_colorspace: Colorspace, _bounds: IRect, _alpha: bool) -> Result<Self>` | `l3-pixmap-init-blank` (+4) |
| `__le__` | operator · L3 · T3 | `Pixmap::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-pixmap-le` |
| `__len__` | operator · L3 · T3 | `Pixmap::len` | `pub fn len(&self) -> usize` | `l3-pixmap-len` |
| `__lt__` | operator · L3 · T3 | `Pixmap::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-pixmap-order` |
| `__ne__` | operator · L3 · T3 | `Pixmap::PartialEq` | `derived PartialEq on Pixmap` | `l3-pixmap-ne` |
| `alpha` | property · L3 · T2 | `Pixmap::alpha` | `pub fn alpha(&self) -> bool` | `l3-doc-get-page-pixmap` |
| `clear_with` | method · L3 · T2 | `Pixmap::clear_with` | `pub fn clear_with(&mut self, _value: Option<u8>, _bbox: Option<IRect>) -> Result<()>` | `l3-pixmap-clear-with` |
| `color_count` | method · L3 · T3 | `Pixmap::color_count` | `pub fn color_count(&self, _colors: i32, _clip: Option<IRect>) -> Result<i32>` | `l3-pixmap-color-count` |
| `color_topusage` | method · L3 · T3 | `Pixmap::color_topusage` | `pub fn color_topusage(&self, _clip: Option<IRect>) -> Result<(i32, Vec<u8>)>` | `l3-pixmap-color-topusage` |
| `colorspace` | property · L3 · T2 | `Pixmap::colorspace` | `pub fn colorspace(&self) -> Colorspace` | `l3-pixmap-colorspace` |
| `copy` | method · L3 · T3 | `Pixmap::copy` | `pub fn copy(&mut self, _source: &Pixmap, _bbox: IRect) -> Result<()>` | `l3-pixmap-copy` |
| `digest` | property · L3 · T3 | `Pixmap::digest` | `pub fn digest(&self) -> Vec<u8>` | `l3-pixmap-digest` |
| `gamma_with` | method · L3 · T3 | `Pixmap::gamma_with` | `pub fn gamma_with(&mut self, _gamma: f64) -> Result<()>` | `l3-pixmap-gamma-with` |
| `h` | property · L3 · T2 | `Pixmap::h` | `pub fn h(&self) -> i32` | `l3-pixmap-size` |
| `height` | property · L3 · T1 | `Pixmap::height` | `pub fn height(&self) -> i32` | `l3-pixmap-size` |
| `invert_irect` | method · L3 · T3 | `Pixmap::invert_irect` | `pub fn invert_irect(&mut self, _bbox: Option<IRect>) -> Result<()>` | `l3-pixmap-invert-irect` |
| `irect` | property · L3 · T2 | `Pixmap::irect` | `pub fn irect(&self) -> IRect` | `l3-pixmap-layout` |
| `is_monochrome` | property · L3 · T3 | `Pixmap::is_monochrome` | `pub fn is_monochrome(&self) -> bool` | `l3-pixmap-is-monochrome` |
| `is_unicolor` | property · L3 · T3 | `Pixmap::is_unicolor` | `pub fn is_unicolor(&self) -> bool` | `l3-pixmap-unicolor` |
| `n` | property · L3 · T1 | `Pixmap::n` | `pub fn n(&self) -> i32` | `l3-pixmap-layout` |
| `pdfocr_save` | method · L3 · T3 | `Pixmap::pdfocr_save` | `pub fn pdfocr_save( &self, _path: &str, _compress: bool, _language: Option<&str>, _tessdata: Option<&str>, ) -> Result<()>` | `l3-pixmap-pdfocr-save` |
| `pdfocr_tobytes` | method · L3 · T3 | `Pixmap::pdfocr_tobytes` | `pub fn pdfocr_tobytes( &self, _compress: bool, _language: &str, _tessdata: Option<&str>, ) -> Result<Vec<u8>>` | `l3-pixmap-pdfocr-tobytes` |
| `pil_image` | method · L3 · T3 | `Pixmap::pil_image` | `pub fn pil_image(&self) -> Result<Value>` | `l3-pixmap-pil` |
| `pil_save` | method · L3 · T3 | `Pixmap::pil_save` | `pub fn pil_save(&self, _path: &str, _format: Option<&str>) -> Result<()>` | `l3-pixmap-pil-save` |
| `pil_tobytes` | method · L3 · T3 | `Pixmap::pil_tobytes` | `pub fn pil_tobytes(&self, _format: &str) -> Result<Vec<u8>>` | `l3-pixmap-pil-tobytes` |
| `pixel` | method · L3 · T3 | `Pixmap::pixel` | `pub fn pixel(&self, _x: i32, _y: i32) -> Result<Vec<u8>>` | `l3-pixmap-pixel` (+1) |
| `samples` | property · L3 · T1 | `Pixmap::samples` | `pub fn samples(&self) -> &[u8]` | `l3-pixmap-samples` |
| `samples_mv` | property · L3 · T3 | `Pixmap::samples_mv` | `pub fn samples_mv(&self) -> &[u8]` | `l3-pixmap-samples-mv` |
| `samples_ptr` | property · L3 · T3 | `Pixmap::samples_ptr` | `pub fn samples_ptr(&self) -> *const u8` | `l3-pixmap-samples-ptr` |
| `save` | method · L3 · T1 | `Pixmap::save` | `pub fn save(&self, _path: &str, _output: Option<&str>, _jpg_quality: i32) -> Result<()>` | `l3-pixmap-save` |
| `set_alpha` | method · L3 · T3 | `Pixmap::set_alpha` | `pub fn set_alpha(&mut self, _options: &AlphaOptions) -> Result<()>` | `l3-pixmap-set-alpha` |
| `set_dpi` | method · L3 · T3 | `Pixmap::set_dpi` | `pub fn set_dpi(&mut self, _xres: i32, _yres: i32) -> Result<()>` | `l3-pixmap-resolution` (+1) |
| `set_origin` | method · L3 · T3 | `Pixmap::set_origin` | `pub fn set_origin(&mut self, _x: i32, _y: i32) -> Result<()>` | `l3-pixmap-set-origin` |
| `set_pixel` | method · L3 · T3 | `Pixmap::set_pixel` | `pub fn set_pixel(&mut self, _x: i32, _y: i32, _color: &[u8]) -> Result<()>` | `l3-pixmap-set-pixel` |
| `set_rect` | method · L3 · T2 | `Pixmap::set_rect` | `pub fn set_rect(&mut self, _bbox: IRect, _color: &[u8]) -> Result<()>` | `l3-pixmap-set-rect` |
| `shrink` | method · L3 · T3 | `Pixmap::shrink` | `pub fn shrink(&mut self, _factor: i32) -> Result<()>` | `l3-pixmap-shrink` |
| `size` | property · L3 · T3 | `Pixmap::size` | `pub fn size(&self) -> usize` | `l3-pixmap-size` |
| `stride` | property · L3 · T2 | `Pixmap::stride` | `pub fn stride(&self) -> usize` | `l3-doc-get-page-pixmap` |
| `tint_with` | method · L3 · T3 | `Pixmap::tint_with` | `pub fn tint_with(&mut self, _black: i32, _white: i32) -> Result<()>` | `l3-pixmap-tint-with` |
| `tobytes` | method · L3 · T1 | `Pixmap::tobytes` | `pub fn tobytes(&self, _output: &str, _jpg_quality: i32) -> Result<Vec<u8>>` | `l3-pixmap-tobytes-bad` (+4) |
| `w` | property · L3 · T2 | `Pixmap::w` | `pub fn w(&self) -> i32` | `l3-pixmap-size` |
| `warp` | method · L3 · T3 | `Pixmap::warp` | `pub fn warp(&self, _quad: Quad, _width: i32, _height: i32) -> Result<Pixmap>` | `l3-pixmap-warp` |
| `width` | property · L3 · T1 | `Pixmap::width` | `pub fn width(&self) -> i32` | `l3-pixmap-72` |
| `x` | property · L3 · T3 | `Pixmap::x` | `pub fn x(&self) -> i32` | `l3-pixmap-x` |
| `xres` | property · L3 · T3 | `Pixmap::xres` | `pub fn xres(&self) -> i32` | `l3-pixmap-xres` |
| `y` | property · L3 · T3 | `Pixmap::y` | `pub fn y(&self) -> i32` | `l3-pixmap-y` |
| `yres` | property · L3 · T3 | `Pixmap::yres` | `pub fn yres(&self) -> i32` | `l3-pixmap-yres` |

## Annot

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__bool__` | operator · L4 · T3 | `Annot::to_bool` | `pub fn to_bool(&self) -> bool` | `l4-annot-bool` |
| `__eq__` | operator · L4 · T3 | `Annot::PartialEq` | `impl PartialEq for Annot` | `l4-annot-eq` |
| `__ge__` | operator · L4 · T3 | `Annot::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-annot-ge` |
| `__gt__` | operator · L4 · T3 | `Annot::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-annot-gt` |
| `__init__` | constructor · L4 · T3 | `Annot::new` | `pub fn new<'b>(_page: &'b mut PageMut<'_>, _xref: i32) -> Annot<'b>` | `l4-annot-init` |
| `__le__` | operator · L4 · T3 | `Annot::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-annot-le` |
| `__lt__` | operator · L4 · T3 | `Annot::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-annot-lt` |
| `__ne__` | operator · L4 · T3 | `Annot::PartialEq` | `impl PartialEq for Annot` | `l4-annot-ne` |
| `apn_bbox` | property · L4 · T3 | `Annot::apn_bbox` | `pub fn apn_bbox(&self) -> Result<Rect>` | `l4-annot-apn-bbox` |
| `apn_matrix` | property · L4 · T3 | `Annot::apn_matrix` | `pub fn apn_matrix(&self) -> Result<Matrix>` | `l4-annot-apn-matrix` |
| `blendmode` | property · L4 · T3 | `Annot::blendmode` | `pub fn blendmode(&self) -> Result<String>` | `L4.annot.appearance_state` |
| `border` | property · L4 · T3 | `Annot::border` | `pub fn border(&self) -> Result<Border>` | `L4.annot.border` |
| `clean_contents` | method · L4 · T3 | `Annot::clean_contents` | `pub fn clean_contents(&mut self, _sanitize: bool) -> Result<()>` | `l4-annot-clean-contents` |
| `colors` | property · L4 · T3 | `Annot::colors` | `pub fn colors(&self) -> Result<AnnotColors>` | `L4.annot.colors` |
| `delete_responses` | method · L4 · T3 | `Annot::delete_responses` | `pub fn delete_responses(&mut self) -> Result<()>` | `l4-annot-delete-responses` |
| `file_info` | property · L4 · T3 | `Annot::file_info` | `pub fn file_info(&self) -> Result<FileInfo>` | `L4.annot.file_roundtrip` |
| `flags` | property · L4 · T3 | `Annot::flags` | `pub fn flags(&self) -> Result<i32>` | `L4.annot.flags` |
| `get_file` | method · L4 · T3 | `Annot::get_file` | `pub fn get_file(&self) -> Result<Vec<u8>>` | `l4-annot-get-file` |
| `get_oc` | method · L4 · T3 | `Annot::get_oc` | `pub fn get_oc(&self) -> Result<i32>` | `l4-annot-get-oc` |
| `get_parent` | method · L4 · T3 | `Annot::get_parent` | `pub fn get_parent(&self) -> Result<AnnotEntry>` | `l4-annot-get-parent` |
| `get_pixmap` | method · L4 · T3 | `Annot::get_pixmap` | `pub fn get_pixmap(&self, _options: &RenderOptions) -> Result<Pixmap>` | `L4.annot.get_pixmap` |
| `get_sound` | method · L4 · T3 | `Annot::get_sound` | `pub fn get_sound(&self) -> Result<Vec<u8>>` | `l4-annot-get-sound` |
| `get_text` | method · L4 · T3 | `Annot::get_text` | `pub fn get_text(&self, _options: &TextOptions) -> Result<TextOutput>` | `l4-annot-get-text` |
| `get_textbox` | method · L4 · T3 | `Annot::get_textbox` | `pub fn get_textbox(&self, _rect: Option<Rect>) -> Result<String>` | `L4.annot.get_textbox` |
| `get_textpage` | method · L4 · T3 | `Annot::get_textpage` | `pub fn get_textpage(&self, _clip: Option<Rect>, _flags: i32) -> Result<TextPage<'_>>` | `L4.annot.get_textpage` |
| `has_popup` | property · L4 · T3 | `Annot::has_popup` | `pub fn has_popup(&self) -> Result<bool>` | `L4.annot.popup` |
| `info` | property · L4 · T3 | `Annot::info` | `pub fn info(&self) -> Result<AnnotInfo>` | `L4.annot.info` |
| `irt_xref` | property · L4 · T3 | `Annot::irt_xref` | `pub fn irt_xref(&self) -> Result<i32>` | `L4.annot.irt` |
| `is_open` | property · L4 · T3 | `Annot::is_open` | `pub fn is_open(&self) -> Result<bool>` | `l4-annot-is-open` |
| `language` | property · L4 · T3 | `Annot::language` | `pub fn language(&self) -> Result<String>` | `l4-annot-language` |
| `line_ends` | property · L4 · T3 | `Annot::line_ends` | `pub fn line_ends(&self) -> Result<Option<(i32, i32)>>` | `l4-annot-line-ends` |
| `next` | property · L4 · T3 | `Annot::next` | `pub fn next(&mut self) -> Result<Option<Annot<'_>>>` | `l4-annot-next` |
| `opacity` | property · L4 · T3 | `Annot::opacity` | `pub fn opacity(&self) -> Result<f64>` | `L4.annot.opacity` |
| `popup_rect` | property · L4 · T3 | `Annot::popup_rect` | `pub fn popup_rect(&self) -> Result<Rect>` | `l4-annot-popup-rect` |
| `popup_xref` | property · L4 · T3 | `Annot::popup_xref` | `pub fn popup_xref(&self) -> Result<i32>` | `l4-annot-popup-xref` |
| `rect` | property · L4 · T3 | `Annot::rect` | `pub fn rect(&self) -> Result<Rect>` | `L4.annot.rect` |
| `rect_delta` | property · L4 · T3 | `Annot::rect_delta` | `pub fn rect_delta(&self) -> Result<Rect>` | `l4-annot-rect-delta` |
| `rotation` | property · L4 · T3 | `Annot::rotation` | `pub fn rotation(&self) -> Result<i32>` | `L4.annot.rotation` |
| `set_apn_bbox` | method · L4 · T3 | `Annot::set_apn_bbox` | `pub fn set_apn_bbox(&mut self, _bbox: Rect) -> Result<()>` | `l4-annot-set-apn-bbox` |
| `set_apn_matrix` | method · L4 · T3 | `Annot::set_apn_matrix` | `pub fn set_apn_matrix(&mut self, _matrix: Matrix) -> Result<()>` | `l4-annot-set-apn-matrix` |
| `set_blendmode` | method · L4 · T3 | `Annot::set_blendmode` | `pub fn set_blendmode(&mut self, _blend_mode: &str) -> Result<()>` | `l4-annot-set-blendmode` |
| `set_border` | method · L4 · T3 | `Annot::set_border` | `pub fn set_border(&mut self, _options: &BorderOptions) -> Result<()>` | `l4-annot-set-border` |
| `set_colors` | method · L4 · T2 | `Annot::set_colors` | `pub fn set_colors(&mut self, _colors: &AnnotColors) -> Result<()>` | `L4.add_rect_annot.basic` |
| `set_flags` | method · L4 · T3 | `Annot::set_flags` | `pub fn set_flags(&mut self, _flags: i32) -> Result<()>` | `l4-annot-set-flags` |
| `set_info` | method · L4 · T3 | `Annot::set_info` | `pub fn set_info(&mut self, _options: &AnnotInfoOptions) -> Result<()>` | `l4-annot-set-info` |
| `set_irt_xref` | method · L4 · T3 | `Annot::set_irt_xref` | `pub fn set_irt_xref(&mut self, _xref: i32) -> Result<()>` | `l4-annot-set-irt-xref` |
| `set_language` | method · L4 · T3 | `Annot::set_language` | `pub fn set_language(&mut self, _language: Option<&str>) -> Result<()>` | `l4-annot-set-language` |
| `set_line_ends` | method · L4 · T3 | `Annot::set_line_ends` | `pub fn set_line_ends(&mut self, _start: i32, _end: i32) -> Result<()>` | `l4-annot-set-line-ends` |
| `set_name` | method · L4 · T3 | `Annot::set_name` | `pub fn set_name(&mut self, _name: &str) -> Result<()>` | `l4-annot-set-name` |
| `set_oc` | method · L4 · T3 | `Annot::set_oc` | `pub fn set_oc(&mut self, _oc: i32) -> Result<()>` | `l4-annot-set-oc` |
| `set_opacity` | method · L4 · T3 | `Annot::set_opacity` | `pub fn set_opacity(&mut self, _opacity: f64) -> Result<()>` | `l4-annot-set-opacity` |
| `set_open` | method · L4 · T3 | `Annot::set_open` | `pub fn set_open(&mut self, _is_open: bool) -> Result<()>` | `l4-annot-set-open` |
| `set_popup` | method · L4 · T3 | `Annot::set_popup` | `pub fn set_popup(&mut self, _rect: Rect) -> Result<()>` | `l4-annot-set-popup` |
| `set_rect` | method · L4 · T3 | `Annot::set_rect` | `pub fn set_rect(&mut self, _rect: Rect) -> Result<()>` | `l4-annot-set-rect` |
| `set_rotation` | method · L4 · T3 | `Annot::set_rotation` | `pub fn set_rotation(&mut self, _rotate: i32) -> Result<()>` | `l4-annot-set-rotation` |
| `type` | property · L4 · T3 | `Annot::annot_type` | `pub fn annot_type(&self) -> Result<AnnotType>` | `L4.annot.type` |
| `update` | method · L4 · T2 | `Annot::update` | `pub fn update(&mut self, _options: &AnnotUpdateOptions) -> Result<()>` | `L4.annot.update` |
| `update_file` | method · L4 · T3 | `Annot::update_file` | `pub fn update_file( &mut self, _data: &[u8], _filename: Option<&str>, _display_name: Option<&str>, _description: Option<&str>, ) -> Result<()>` | `l4-annot-update-file` |
| `update_timing_test` | method · L4 · T3 | `Annot::update_timing_test` | `pub fn update_timing_test(&self) -> Result<f64>` | `l4-annot-update-timing-test` |
| `vertices` | property · L4 · T3 | `Annot::vertices` | `pub fn vertices(&self) -> Result<Vec<Point>>` | `l4-annot-vertices` |
| `xref` | property · L4 · T3 | `Annot::xref` | `pub fn xref(&self) -> Result<i32>` | `l4-annot-xref` |

## Widget

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L4 · T3 | `Widget::PartialEq` | `derived PartialEq on Widget` | `l4-widget-eq` |
| `__ge__` | operator · L4 · T3 | `Widget::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-widget-ge` |
| `__gt__` | operator · L4 · T3 | `Widget::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-widget-gt` |
| `__init__` | constructor · L4 · T2 | `Widget::new` | `pub fn new() -> Self` | `L4.widget.add` |
| `__le__` | operator · L4 · T3 | `Widget::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-widget-le` |
| `__lt__` | operator · L4 · T3 | `Widget::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-widget-lt` |
| `__ne__` | operator · L4 · T3 | `Widget::PartialEq` | `derived PartialEq on Widget` | `l4-widget-ne` |
| `button_states` | method · L4 · T3 | `Widget::button_states` | `pub fn button_states(&self) -> Result<Vec<String>>` | `l4-widget-button-states` |
| `next` | property · L4 · T3 | `Widget::next` | `pub fn next(&self) -> Option<Widget>` | `l4-widget-next` |
| `on_state` | method · L4 · T3 | `Widget::on_state` | `pub fn on_state(&self) -> Result<String>` | `l4-widget-on-state` |
| `reset` | method · L4 · T3 | `Widget::reset` | `pub fn reset(&mut self) -> Result<()>` | `L4.widget.reset` |
| `update` | method · L4 · T3 | `Widget::update` | `pub fn update(&self) -> Result<()>` | `L4.widget.update_text` |

## Link

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L2 · T3 | `Link::PartialEq` | `derived PartialEq on Link` | `l2-link-eq` |
| `__ge__` | operator · L2 · T3 | `Link::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-link-ge` |
| `__gt__` | operator · L2 · T3 | `Link::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-link-gt` |
| `__init__` | constructor · L2 · T3 | `Link::new` | `pub fn new() -> Self` | `l2-link-init` |
| `__le__` | operator · L2 · T3 | `Link::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-link-le` |
| `__lt__` | operator · L2 · T3 | `Link::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l2-link-lt` |
| `__ne__` | operator · L2 · T3 | `Link::PartialEq` | `derived PartialEq on Link` | `l2-link-ne` |
| `border` | property · L2 · T3 | `Link::border` | `pub fn border(&self) -> Border` | `l2-link-border` |
| `colors` | property · L2 · T3 | `Link::colors` | `pub fn colors(&self) -> AnnotColors` | `l2-link-colors` |
| `dest` | property · L2 · T3 | `Link::dest` | `pub fn dest(&self) -> Option<LinkDest>` | `l2-link-dest` |
| `flags` | property · L2 · T3 | `Link::flags` | `pub fn flags(&self) -> i32` | `l2-link-flags` |
| `is_external` | property · L2 · T3 | `Link::is_external` | `pub fn is_external(&self) -> bool` | `l2-link-is-external` |
| `next` | property · L2 · T3 | `Link::next` | `pub fn next(&self) -> Option<Link>` | `l2-link-next` |
| `rect` | property · L2 · T3 | `Link::rect` | `pub fn rect(&self) -> Rect` | `l2-link-rect` |
| `set_border` | method · L2 · T3 | `Link::set_border` | `pub fn set_border(&mut self, options: &BorderOptions)` | `l2-link-set-border` |
| `set_colors` | method · L2 · T3 | `Link::set_colors` | `pub fn set_colors(&mut self, colors: &AnnotColors)` | `l2-link-set-colors` |
| `set_flags` | method · L2 · T3 | `Link::set_flags` | `pub fn set_flags(&mut self, flags: i32)` | `l2-link-set-flags` |
| `uri` | property · L2 · T3 | `Link::uri` | `pub fn uri(&self) -> String` | `l2-link-uri` |

## Shape

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L4 · T3 | `Shape::PartialEq` | `impl PartialEq for Shape` | `l4-shape-eq` |
| `__ge__` | operator · L4 · T3 | `Shape::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-shape-ge` |
| `__gt__` | operator · L4 · T3 | `Shape::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-shape-gt` |
| `__init__` | constructor · L4 · T3 | `Shape::new` | `pub fn new(_doc: &'a mut PdfDocument, _page: usize) -> Self` | `l4-shape-init` |
| `__le__` | operator · L4 · T3 | `Shape::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-shape-le` |
| `__lt__` | operator · L4 · T3 | `Shape::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-shape-lt` |
| `__ne__` | operator · L4 · T3 | `Shape::PartialEq` | `impl PartialEq for Shape` | `l4-shape-ne` |
| `commit` | method · L4 · T2 | `Shape::commit` | `pub fn commit(&mut self, _overlay: bool) -> Result<()>` | `L4.shape.commit_overlay` |
| `draw_bezier` | method · L4 · T3 | `Shape::draw_bezier` | `pub fn draw_bezier(&mut self, _p1: Point, _p2: Point, _p3: Point, _p4: Point) -> Result<Point>` | `l4-shape-draw-bezier` |
| `draw_circle` | method · L4 · T3 | `Shape::draw_circle` | `pub fn draw_circle(&mut self, _center: Point, _radius: f64) -> Result<Point>` | `l4-shape-draw-circle` |
| `draw_curve` | method · L4 · T3 | `Shape::draw_curve` | `pub fn draw_curve(&mut self, _p1: Point, _p2: Point, _p3: Point) -> Result<Point>` | `l4-shape-draw-curve` |
| `draw_line` | method · L4 · T2 | `Shape::draw_line` | `pub fn draw_line(&mut self, _p1: Point, _p2: Point) -> Result<Point>` | `L4.new_shape.line` |
| `draw_oval` | method · L4 · T3 | `Shape::draw_oval` | `pub fn draw_oval(&mut self, _rect: Rect) -> Result<Point>` | `l4-shape-draw-oval` |
| `draw_polyline` | method · L4 · T3 | `Shape::draw_polyline` | `pub fn draw_polyline(&mut self, _points: &[Point]) -> Result<Point>` | `l4-shape-draw-polyline` |
| `draw_quad` | method · L4 · T3 | `Shape::draw_quad` | `pub fn draw_quad(&mut self, _quad: Quad) -> Result<Point>` | `l4-shape-draw-quad` |
| `draw_rect` | method · L4 · T2 | `Shape::draw_rect` | `pub fn draw_rect(&mut self, _rect: Rect, _radius: Option<f64>) -> Result<Point>` | `L4.new_shape.rect` |
| `draw_sector` | method · L4 · T3 | `Shape::draw_sector` | `pub fn draw_sector( &mut self, _center: Point, _point: Point, _beta: f64, _full_sector: bool, ) -> Result<Point>` | `l4-shape-draw-sector` |
| `draw_squiggle` | method · L4 · T3 | `Shape::draw_squiggle` | `pub fn draw_squiggle(&mut self, _p1: Point, _p2: Point, _breadth: f64) -> Result<Point>` | `l4-shape-draw-squiggle` |
| `draw_zigzag` | method · L4 · T3 | `Shape::draw_zigzag` | `pub fn draw_zigzag(&mut self, _p1: Point, _p2: Point, _breadth: f64) -> Result<Point>` | `l4-shape-draw-zigzag` |
| `finish` | method · L4 · T2 | `Shape::finish` | `pub fn finish(&mut self, _options: &ShapeFinishOptions) -> Result<()>` | `L4.shape.finish_options` |
| `horizontal_angle` | method · L4 · T3 | `Shape::horizontal_angle` | `pub fn horizontal_angle(&self, _center: Point, _point: Point) -> f64` | `l4-shape-horizontal-angle` |
| `insert_text` | method · L4 · T3 | `Shape::insert_text` | `pub fn insert_text( &mut self, _point: Point, _text: &TextArg, _options: &InsertTextOptions, ) -> Result<i32>` | `L4.shape.insert_text` |
| `insert_textbox` | method · L4 · T3 | `Shape::insert_textbox` | `pub fn insert_textbox( &mut self, _rect: Rect, _text: &TextArg, _options: &TextBoxOptions, ) -> Result<f64>` | `l4-shape-insert-textbox` |
| `updateRect` | method · L4 · T3 | `Shape::update_rect` | `pub fn update_rect(&mut self, _rect: Rect) -> Result<()>` | `l4-shape-updaterect` |

## Rect

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__abs__` | operator · P0 · unranked | `Rect::abs` | `pub fn abs(&self) -> f64` | `p0-rect-abs` |
| `__add__` | operator · P0 · T3 | `Rect::Add` | `impl Add<Rect> for Rect; impl Add<f64> for Rect` | `p0-rect-add` |
| `__bool__` | operator · P0 · T3 | `Rect::to_bool` | `pub fn to_bool(&self) -> bool` | `p0-rect-bool` |
| `__contains__` | operator · P0 · T3 | `Rect::contains_point` | `pub fn contains_point(&self, point: Point) -> i32` | `p0-rect-contains` |
| `__eq__` | operator · P0 · T3 | `Rect::PartialEq` | `derived PartialEq on Rect` | `p0-rect-eq` |
| `__ge__` | operator · P0 · T3 | `Rect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-rect-ge` |
| `__getitem__` | operator · P0 · T3 | `Rect::Index` | `impl Index<usize> for Rect` | `p0-rect-getitem` |
| `__gt__` | operator · P0 · T3 | `Rect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-rect-gt` |
| `__init__` | constructor · P0 · T1 | `Rect::new` | `pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self` | `p0-rect-init` |
| `__le__` | operator · P0 · T3 | `Rect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-rect-le` |
| `__len__` | operator · P0 · T3 | `Rect::len` | `pub fn len(&self) -> usize` | `p0-rect-len` |
| `__lt__` | operator · P0 · T3 | `Rect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-rect-lt` |
| `__mul__` | operator · P0 · T3 | `Rect::Mul` | `impl Mul<Matrix> for Rect; impl Mul<f64> for Rect` | `p0-rect-mul` |
| `__ne__` | operator · P0 · T3 | `Rect::PartialEq` | `derived PartialEq on Rect` | `p0-rect-ne` |
| `__neg__` | operator · P0 · T3 | `Rect::Neg` | `impl Neg for Rect` | `p0-rect-neg` |
| `__pos__` | operator · P0 · T3 | `Rect::pos` | `pub fn pos(&self) -> Self` | `p0-rect-pos` |
| `__setitem__` | operator · P0 · T3 | `Rect::IndexMut` | `impl IndexMut<usize> for Rect` | `p0-rect-setitem` |
| `__sub__` | operator · P0 · T3 | `Rect::Sub` | `impl Sub<Rect> for Rect; impl Sub<f64> for Rect` | `p0-rect-sub` |
| `__truediv__` | operator · P0 · T3 | `Rect::Div` | `impl Div<Matrix> for Rect; impl Div<f64> for Rect` | `p0-rect-truediv` |
| `bl` | property · P0 · T3 | `Rect::bl` | `pub fn bl(&self) -> Point` | `p0-rect-bl` |
| `bottom_left` | property · P0 · T3 | `Rect::bottom_left` | `pub fn bottom_left(&self) -> Point` | `p0-rect-bottom-left` |
| `bottom_right` | property · P0 · T3 | `Rect::bottom_right` | `pub fn bottom_right(&self) -> Point` | `p0-rect-bottom-right` |
| `br` | property · P0 · T3 | `Rect::br` | `pub fn br(&self) -> Point` | `p0-rect-br` |
| `contains` | method · P0 · T2 | `Rect::contains_rect` | `pub fn contains_rect(&self, other: Rect) -> bool` | `p0-rect-contains-2` |
| `get_area` | method · P0 · T2 | `Rect::get_area` | `pub fn get_area(&self) -> f64` | `p0-rect-get-area` |
| `height` | property · P0 · T1 | `Rect::height` | `pub fn height(&self) -> f64` | `p0-rect-height` |
| `include_point` | method · P0 · T3 | `Rect::include_point` | `pub fn include_point(&mut self, point: Point) -> Rect` | `p0-rect-include-point` |
| `include_rect` | method · P0 · T2 | `Rect::include_rect` | `pub fn include_rect(&mut self, other: Rect) -> Rect` | `p0-rect-include-rect` |
| `intersect` | method · P0 · T2 | `Rect::intersect` | `pub fn intersect(&mut self, other: Rect) -> Rect` | `p0-rect-intersect` |
| `intersects` | method · P0 · T2 | `Rect::intersects` | `pub fn intersects(&self, other: Rect) -> bool` | `p0-rect-intersects` |
| `irect` | property · P0 · T3 | `Rect::irect` | `pub fn irect(&self) -> IRect` | `p0-rect-irect` |
| `is_empty` | property · P0 · T2 | `Rect::is_empty` | `pub fn is_empty(&self) -> bool` | `p0-rect-is-empty` |
| `is_infinite` | property · P0 · T2 | `Rect::is_infinite` | `pub fn is_infinite(&self) -> bool` | `p0-rect-is-infinite` |
| `is_valid` | property · P0 · T3 | `Rect::is_valid` | `pub fn is_valid(&self) -> bool` | `p0-rect-is-valid` |
| `morph` | method · P0 · T3 | `Rect::morph` | `pub fn morph(&self, point: Point, matrix: Matrix) -> Quad` | `p0-rect-morph` |
| `norm` | method · P0 · T3 | `Rect::norm` | `pub fn norm(&self) -> f64` | `p0-rect-norm` |
| `normalize` | method · P0 · T2 | `Rect::normalize` | `pub fn normalize(&mut self) -> Rect` | `p0-rect-normalize` |
| `quad` | property · P0 · T3 | `Rect::quad` | `pub fn quad(&self) -> Quad` | `p0-rect-quad` |
| `round` | method · P0 · T3 | `Rect::round` | `pub fn round(&self) -> IRect` | `p0-rect-round` |
| `tl` | property · P0 · T3 | `Rect::tl` | `pub fn tl(&self) -> Point` | `p0-rect-tl` |
| `top_left` | property · P0 · T3 | `Rect::top_left` | `pub fn top_left(&self) -> Point` | `p0-rect-top-left` |
| `top_right` | property · P0 · T3 | `Rect::top_right` | `pub fn top_right(&self) -> Point` | `p0-rect-top-right` |
| `torect` | method · P0 · T3 | `Rect::torect` | `pub fn torect(&self, target: Rect) -> Matrix` | `p0-rect-torect` |
| `tr` | property · P0 · T3 | `Rect::tr` | `pub fn tr(&self) -> Point` | `p0-rect-tr` |
| `transform` | method · P0 · T3 | `Rect::transform` | `pub fn transform(&mut self, matrix: Matrix) -> Rect` | `p0-rect-transform` |
| `width` | property · P0 · T1 | `Rect::width` | `pub fn width(&self) -> f64` | `p0-rect-width` |

## IRect

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__add__` | operator · P0 · T3 | `IRect::Add` | `impl Add<IRect> for IRect; impl Add<i32> for IRect` | `p0-irect-add` |
| `__contains__` | operator · P0 · T3 | `IRect::contains_point` | `pub fn contains_point(&self, point: Point) -> i32` | `p0-irect-contains` |
| `__eq__` | operator · P0 · T3 | `IRect::PartialEq` | `derived PartialEq on IRect` | `p0-irect-eq` |
| `__ge__` | operator · P0 · T3 | `IRect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-irect-ge` |
| `__getitem__` | operator · P0 · T3 | `IRect::Index` | `impl Index<usize> for IRect` | `p0-irect-getitem` |
| `__gt__` | operator · P0 · T3 | `IRect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-irect-gt` |
| `__init__` | constructor · P0 · T2 | `IRect::new` | `pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self` | `p0-irect-init` |
| `__le__` | operator · P0 · T3 | `IRect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-irect-le` |
| `__len__` | operator · P0 · T3 | `IRect::len` | `pub fn len(&self) -> usize` | `p0-irect-len` |
| `__lt__` | operator · P0 · T3 | `IRect::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-irect-lt` |
| `__mul__` | operator · P0 · T3 | `IRect::Mul` | `impl Mul<Matrix> for IRect; impl Mul<i32> for IRect` | `p0-irect-mul` |
| `__ne__` | operator · P0 · T3 | `IRect::PartialEq` | `derived PartialEq on IRect` | `p0-irect-ne` |
| `__neg__` | operator · P0 · T3 | `IRect::Neg` | `impl Neg for IRect` | `p0-irect-neg` |
| `__pos__` | operator · P0 · T3 | `IRect::pos` | `pub fn pos(&self) -> Self` | `p0-irect-pos` |
| `__setitem__` | operator · P0 · T3 | `IRect::IndexMut` | `impl IndexMut<usize> for IRect` | `p0-irect-setitem` |
| `__sub__` | operator · P0 · T3 | `IRect::Sub` | `impl Sub<IRect> for IRect; impl Sub<i32> for IRect` | `p0-irect-sub` |
| `__truediv__` | operator · P0 · T3 | `IRect::Div` | `impl Div<Matrix> for IRect; impl Div<i32> for IRect` | `p0-irect-truediv` |
| `bl` | property · P0 · T3 | `IRect::bl` | `pub fn bl(&self) -> Point` | `p0-irect-bl` |
| `bottom_left` | property · P0 · T3 | `IRect::bottom_left` | `pub fn bottom_left(&self) -> Point` | `p0-irect-bottom-left` |
| `bottom_right` | property · P0 · T3 | `IRect::bottom_right` | `pub fn bottom_right(&self) -> Point` | `p0-irect-bottom-right` |
| `br` | property · P0 · T3 | `IRect::br` | `pub fn br(&self) -> Point` | `p0-irect-br` |
| `contains` | method · P0 · T3 | `IRect::contains_rect` | `pub fn contains_rect(&self, other: IRect) -> bool` | `p0-irect-contains-2` |
| `get_area` | method · P0 · T3 | `IRect::get_area` | `pub fn get_area(&self) -> f64` | `p0-irect-get-area` |
| `height` | property · P0 · T3 | `IRect::height` | `pub fn height(&self) -> i32` | `p0-irect-height` |
| `include_point` | method · P0 · T3 | `IRect::include_point` | `pub fn include_point(&mut self, point: Point) -> IRect` | `p0-irect-include-point` |
| `include_rect` | method · P0 · T3 | `IRect::include_rect` | `pub fn include_rect(&mut self, other: IRect) -> IRect` | `p0-irect-include-rect` |
| `intersect` | method · P0 · T3 | `IRect::intersect` | `pub fn intersect(&mut self, other: IRect) -> IRect` | `p0-irect-intersect` |
| `intersects` | method · P0 · T3 | `IRect::intersects` | `pub fn intersects(&self, other: IRect) -> bool` | `p0-irect-intersects` |
| `is_empty` | property · P0 · T3 | `IRect::is_empty` | `pub fn is_empty(&self) -> bool` | `p0-irect-is-empty` |
| `is_infinite` | property · P0 · T3 | `IRect::is_infinite` | `pub fn is_infinite(&self) -> bool` | `p0-irect-is-infinite` |
| `is_valid` | property · P0 · T3 | `IRect::is_valid` | `pub fn is_valid(&self) -> bool` | `p0-irect-is-valid` |
| `morph` | method · P0 · T3 | `IRect::morph` | `pub fn morph(&self, point: Point, matrix: Matrix) -> Quad` | `p0-irect-morph` |
| `norm` | method · P0 · T3 | `IRect::norm` | `pub fn norm(&self) -> f64` | `p0-irect-norm` |
| `normalize` | method · P0 · T3 | `IRect::normalize` | `pub fn normalize(&mut self) -> IRect` | `p0-irect-normalize` |
| `quad` | property · P0 · T3 | `IRect::quad` | `pub fn quad(&self) -> Quad` | `p0-irect-quad` |
| `rect` | property · P0 · T3 | `IRect::rect` | `pub fn rect(&self) -> Rect` | `p0-irect-rect` |
| `tl` | property · P0 · T3 | `IRect::tl` | `pub fn tl(&self) -> Point` | `p0-irect-tl` |
| `top_left` | property · P0 · T3 | `IRect::top_left` | `pub fn top_left(&self) -> Point` | `p0-irect-top-left` |
| `top_right` | property · P0 · T3 | `IRect::top_right` | `pub fn top_right(&self) -> Point` | `p0-irect-top-right` |
| `torect` | method · P0 · T3 | `IRect::torect` | `pub fn torect(&self, target: IRect) -> Matrix` | `p0-irect-torect` |
| `tr` | property · P0 · T3 | `IRect::tr` | `pub fn tr(&self) -> Point` | `p0-irect-tr` |
| `transform` | method · P0 · T3 | `IRect::transform` | `pub fn transform(&mut self, matrix: Matrix) -> IRect` | `p0-irect-transform` |
| `width` | property · P0 · T3 | `IRect::width` | `pub fn width(&self) -> i32` | `p0-irect-width` |

## Point

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__abs__` | operator · P0 · unranked | `Point::abs` | `pub fn abs(&self) -> f64` | `p0-point-abs` |
| `__add__` | operator · P0 · T3 | `Point::Add` | `impl Add<Point> for Point; impl Add<f64> for Point` | `p0-point-add` |
| `__bool__` | operator · P0 · T3 | `Point::to_bool` | `pub fn to_bool(&self) -> bool` | `p0-point-bool` |
| `__eq__` | operator · P0 · T3 | `Point::PartialEq` | `derived PartialEq on Point` | `p0-point-eq` |
| `__ge__` | operator · P0 · T3 | `Point::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-point-ge` |
| `__getitem__` | operator · P0 · T3 | `Point::Index` | `impl Index<usize> for Point` | `p0-point-getitem` |
| `__gt__` | operator · P0 · T3 | `Point::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-point-gt` |
| `__init__` | constructor · P0 · T1 | `Point::new` | `pub fn new(x: f64, y: f64) -> Self` | `p0-point-init` |
| `__le__` | operator · P0 · T3 | `Point::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-point-le` |
| `__len__` | operator · P0 · T3 | `Point::len` | `pub fn len(&self) -> usize` | `p0-point-len` |
| `__lt__` | operator · P0 · T3 | `Point::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-point-lt` |
| `__mul__` | operator · P0 · T3 | `Point::Mul` | `impl Mul<Matrix> for Point; impl Mul<Point> for Point; impl Mul<f64> for Point` | `p0-point-mul` |
| `__ne__` | operator · P0 · T3 | `Point::PartialEq` | `derived PartialEq on Point` | `p0-point-ne` |
| `__neg__` | operator · P0 · T3 | `Point::Neg` | `impl Neg for Point` | `p0-point-neg` |
| `__pos__` | operator · P0 · T3 | `Point::pos` | `pub fn pos(&self) -> Self` | `p0-point-pos` |
| `__setitem__` | operator · P0 · T3 | `Point::IndexMut` | `impl IndexMut<usize> for Point` | `p0-point-setitem` |
| `__sub__` | operator · P0 · T3 | `Point::Sub` | `impl Sub<Point> for Point; impl Sub<f64> for Point` | `p0-point-sub` |
| `__truediv__` | operator · P0 · T3 | `Point::Div` | `impl Div<Matrix> for Point; impl Div<f64> for Point` | `p0-point-truediv` |
| `abs_unit` | property · P0 · T3 | `Point::abs_unit` | `pub fn abs_unit(&self) -> Point` | `p0-point-abs-unit` |
| `distance_to` | method · P0 · T3 | `Point::distance_to` | `pub fn distance_to(&self, other: Point) -> f64` | `p0-point-distance-to` |
| `norm` | method · P0 · T3 | `Point::norm` | `pub fn norm(&self) -> f64` | `p0-point-norm` |
| `transform` | method · P0 · T3 | `Point::transform` | `pub fn transform(&mut self, matrix: Matrix) -> Point` | `p0-point-transform` |
| `unit` | property · P0 · T3 | `Point::unit` | `pub fn unit(&self) -> Point` | `p0-point-unit` |

## Matrix

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__abs__` | operator · P0 · unranked | `Matrix::abs` | `pub fn abs(&self) -> f64` | `p0-matrix-abs` |
| `__add__` | operator · P0 · T3 | `Matrix::Add` | `impl Add<Matrix> for Matrix; impl Add<f64> for Matrix` | `p0-matrix-add` |
| `__bool__` | operator · P0 · T3 | `Matrix::to_bool` | `pub fn to_bool(&self) -> bool` | `p0-matrix-bool` |
| `__eq__` | operator · P0 · T3 | `Matrix::PartialEq` | `derived PartialEq on Matrix` | `p0-matrix-eq` |
| `__ge__` | operator · P0 · T3 | `Matrix::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-matrix-ge` |
| `__getitem__` | operator · P0 · T3 | `Matrix::Index` | `impl Index<usize> for Matrix` | `p0-matrix-getitem` |
| `__gt__` | operator · P0 · T3 | `Matrix::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-matrix-gt` |
| `__init__` | constructor · P0 · T1 | `Matrix::new` | `pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self` | `p0-matrix-init` |
| `__invert__` | operator · P0 · T3 | `Matrix::invert` | `pub fn invert(&self, src: Option<Matrix>) -> Matrix` | `p0-matrix-invert` |
| `__le__` | operator · P0 · T3 | `Matrix::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-matrix-le` |
| `__len__` | operator · P0 · T3 | `Matrix::len` | `pub fn len(&self) -> usize` | `p0-matrix-len` |
| `__lt__` | operator · P0 · T3 | `Matrix::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-matrix-lt` |
| `__mul__` | operator · P0 · T3 | `Matrix::Mul` | `impl Mul<Matrix> for Matrix; impl Mul<f64> for Matrix` | `p0-matrix-mul` |
| `__ne__` | operator · P0 · T3 | `Matrix::PartialEq` | `derived PartialEq on Matrix` | `p0-matrix-ne` |
| `__neg__` | operator · P0 · T3 | `Matrix::Neg` | `impl Neg for Matrix` | `p0-matrix-neg` |
| `__pos__` | operator · P0 · T3 | `Matrix::pos` | `pub fn pos(&self) -> Self` | `p0-matrix-pos` |
| `__setitem__` | operator · P0 · T3 | `Matrix::IndexMut` | `impl IndexMut<usize> for Matrix` | `p0-matrix-setitem` |
| `__sub__` | operator · P0 · T3 | `Matrix::Sub` | `impl Sub<Matrix> for Matrix; impl Sub<f64> for Matrix` | `p0-matrix-sub` |
| `__truediv__` | operator · P0 · T3 | `Matrix::Div` | `impl Div<Matrix> for Matrix; impl Div<f64> for Matrix` | `p0-matrix-truediv` |
| `concat` | method · P0 · T3 | `Matrix::concat` | `pub fn concat(&mut self, one: Matrix, two: Matrix) -> Matrix` | `p0-matrix-concat` |
| `invert` | method · P0 · T3 | `Matrix::invert` | `pub fn invert(&self, src: Option<Matrix>) -> Matrix` | `p0-matrix-invert-2` |
| `is_rectilinear` | property · P0 · T3 | `Matrix::is_rectilinear` | `pub fn is_rectilinear(&self) -> bool` | `p0-matrix-is-rectilinear` |
| `norm` | method · P0 · T3 | `Matrix::norm` | `pub fn norm(&self) -> f64` | `p0-matrix-norm` |
| `prerotate` | method · P0 · T2 | `Matrix::prerotate` | `pub fn prerotate(&mut self, theta: f64) -> Matrix` | `p0-matrix-prerotate` |
| `prescale` | method · P0 · T3 | `Matrix::prescale` | `pub fn prescale(&mut self, sx: f64, sy: f64) -> Matrix` | `p0-matrix-prescale` |
| `preshear` | method · P0 · T3 | `Matrix::preshear` | `pub fn preshear(&mut self, h: f64, v: f64) -> Matrix` | `p0-matrix-preshear` |
| `pretranslate` | method · P0 · T3 | `Matrix::pretranslate` | `pub fn pretranslate(&mut self, tx: f64, ty: f64) -> Matrix` | `p0-matrix-pretranslate` |

## Quad

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__abs__` | operator · P0 · unranked | `Quad::abs` | `pub fn abs(&self) -> f64` | `p0-quad-abs` |
| `__add__` | operator · P0 · T3 | `Quad::Add` | `impl Add<Quad> for Quad; impl Add<f64> for Quad` | `p0-quad-add` |
| `__bool__` | operator · P0 · T3 | `Quad::to_bool` | `pub fn to_bool(&self) -> bool` | `p0-quad-bool` |
| `__contains__` | operator · P0 · T3 | `Quad::contains_point` | `pub fn contains_point(&self, point: Point) -> bool` | `p0-quad-contains` |
| `__eq__` | operator · P0 · T3 | `Quad::PartialEq` | `derived PartialEq on Quad` | `p0-quad-eq` |
| `__ge__` | operator · P0 · T3 | `Quad::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-quad-ge` |
| `__getitem__` | operator · P0 · T3 | `Quad::Index` | `impl Index<usize> for Quad` | `p0-quad-getitem` |
| `__gt__` | operator · P0 · T3 | `Quad::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-quad-gt` |
| `__init__` | constructor · P0 · T3 | `Quad::new` | `pub fn new(ul: Point, ur: Point, ll: Point, lr: Point) -> Self` | `p0-quad-init` |
| `__le__` | operator · P0 · T3 | `Quad::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-quad-le` |
| `__len__` | operator · P0 · T3 | `Quad::len` | `pub fn len(&self) -> usize` | `p0-quad-len` |
| `__lt__` | operator · P0 · T3 | `Quad::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `p0-quad-lt` |
| `__mul__` | operator · P0 · T3 | `Quad::Mul` | `impl Mul<Matrix> for Quad; impl Mul<f64> for Quad` | `p0-quad-mul` |
| `__ne__` | operator · P0 · T3 | `Quad::PartialEq` | `derived PartialEq on Quad` | `p0-quad-ne` |
| `__neg__` | operator · P0 · T3 | `Quad::Neg` | `impl Neg for Quad` | `p0-quad-neg` |
| `__pos__` | operator · P0 · T3 | `Quad::pos` | `pub fn pos(&self) -> Self` | `p0-quad-pos` |
| `__setitem__` | operator · P0 · T3 | `Quad::IndexMut` | `impl IndexMut<usize> for Quad` | `p0-quad-setitem` |
| `__sub__` | operator · P0 · T3 | `Quad::Sub` | `impl Sub<Quad> for Quad; impl Sub<f64> for Quad` | `p0-quad-sub` |
| `__truediv__` | operator · P0 · T3 | `Quad::Div` | `impl Div<Matrix> for Quad; impl Div<f64> for Quad` | `p0-quad-truediv` |
| `height` | property · P0 · T3 | `Quad::height` | `pub fn height(&self) -> f64` | `p0-quad-height` |
| `is_convex` | property · P0 · T3 | `Quad::is_convex` | `pub fn is_convex(&self) -> bool` | `p0-quad-is-convex` |
| `is_empty` | property · P0 · T3 | `Quad::is_empty` | `pub fn is_empty(&self) -> bool` | `p0-quad-is-empty` |
| `is_infinite` | property · P0 · T3 | `Quad::is_infinite` | `pub fn is_infinite(&self) -> bool` | `p0-quad-is-infinite` |
| `is_rectangular` | property · P0 · T3 | `Quad::is_rectangular` | `pub fn is_rectangular(&self) -> bool` | `p0-quad-is-rectangular` |
| `morph` | method · P0 · T3 | `Quad::morph` | `pub fn morph(&self, point: Point, matrix: Matrix) -> Quad` | `p0-quad-morph` |
| `rect` | property · P0 · T3 | `Quad::rect` | `pub fn rect(&self) -> Rect` | `p0-quad-rect` |
| `transform` | method · P0 · T3 | `Quad::transform` | `pub fn transform(&mut self, matrix: Matrix) -> Quad` | `p0-quad-transform` |
| `width` | property · P0 · T3 | `Quad::width` | `pub fn width(&self) -> f64` | `p0-quad-width` |

## Font

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L4 · T3 | `Font::PartialEq` | `derived PartialEq on Font` | `l4-font-eq` |
| `__ge__` | operator · L4 · T3 | `Font::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-font-ge` |
| `__gt__` | operator · L4 · T3 | `Font::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-font-gt` |
| `__init__` | constructor · L4 · T1 | `Font::new` | `pub fn new(_options: &FontOptions) -> Result<Self>` | `l4-font-init` |
| `__le__` | operator · L4 · T3 | `Font::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-font-le` |
| `__lt__` | operator · L4 · T3 | `Font::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-font-lt` |
| `__ne__` | operator · L4 · T3 | `Font::PartialEq` | `derived PartialEq on Font` | `l4-font-ne` |
| `ascender` | property · L4 · T3 | `Font::ascender` | `pub fn ascender(&self) -> Result<f64>` | `l4-font-properties` |
| `bbox` | property · L4 · T3 | `Font::bbox` | `pub fn bbox(&self) -> Result<Rect>` | `l4-font-properties` |
| `buffer` | property · L4 · T3 | `Font::buffer` | `pub fn buffer(&self) -> Result<Vec<u8>>` | `l4-font-properties` |
| `char_lengths` | method · L4 · T3 | `Font::char_lengths` | `pub fn char_lengths(&self, _text: &str, _options: &FontMeasureOptions) -> Result<Vec<f64>>` | `l4-font-metrics` |
| `descender` | property · L4 · T3 | `Font::descender` | `pub fn descender(&self) -> Result<f64>` | `l4-font-properties` |
| `flags` | property · L4 · T3 | `Font::flags` | `pub fn flags(&self) -> Result<FontFlags>` | `l4-font-properties` |
| `glyph_advance` | method · L4 · T3 | `Font::glyph_advance` | `pub fn glyph_advance(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<f64>` | `l4-font-metrics` |
| `glyph_bbox` | method · L4 · T3 | `Font::glyph_bbox` | `pub fn glyph_bbox(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<Rect>` | `l4-font-metrics` |
| `glyph_count` | property · L4 · T3 | `Font::glyph_count` | `pub fn glyph_count(&self) -> Result<usize>` | `l4-font-properties` |
| `glyph_name_to_unicode` | method · L4 · T3 | `Font::glyph_name_to_unicode` | `pub fn glyph_name_to_unicode(&self, _name: &str) -> Result<i32>` | `l4-font-metrics` |
| `has_glyph` | method · L4 · T3 | `Font::has_glyph` | `pub fn has_glyph(&self, _chr: i32, _options: &FontMeasureOptions) -> Result<i32>` | `l4-font-metrics` |
| `is_bold` | property · L4 · T3 | `Font::is_bold` | `pub fn is_bold(&self) -> Result<bool>` | `l4-font-properties` |
| `is_italic` | property · L4 · T3 | `Font::is_italic` | `pub fn is_italic(&self) -> Result<bool>` | `l4-font-properties` |
| `is_monospaced` | property · L4 · T3 | `Font::is_monospaced` | `pub fn is_monospaced(&self) -> Result<bool>` | `l4-font-properties` |
| `is_serif` | property · L4 · T3 | `Font::is_serif` | `pub fn is_serif(&self) -> Result<bool>` | `l4-font-properties` |
| `is_writable` | property · L4 · T3 | `Font::is_writable` | `pub fn is_writable(&self) -> Result<bool>` | `l4-font-properties` |
| `name` | property · L4 · T3 | `Font::name` | `pub fn name(&self) -> Result<String>` | `l4-font-properties` |
| `text_length` | method · L4 · T2 | `Font::text_length` | `pub fn text_length(&self, _text: &str, _options: &FontMeasureOptions) -> Result<f64>` | `l4-font-metrics` |
| `unicode_to_glyph_name` | method · L4 · T3 | `Font::unicode_to_glyph_name` | `pub fn unicode_to_glyph_name(&self, _chr: i32) -> Result<String>` | `l4-font-metrics` |
| `valid_codepoints` | method · L4 · T3 | `Font::valid_codepoints` | `pub fn valid_codepoints(&self) -> Result<Vec<i32>>` | `l4-font-metrics` |

## TextWriter

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L4 · T3 | `TextWriter::PartialEq` | `derived PartialEq on TextWriter` | `l4-textwriter-eq` |
| `__ge__` | operator · L4 · T3 | `TextWriter::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-textwriter-ge` |
| `__gt__` | operator · L4 · T3 | `TextWriter::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-textwriter-gt` |
| `__init__` | constructor · L4 · T2 | `TextWriter::new` | `pub fn new(_rect: Rect, _options: &TextWriterOptions) -> Self` | `l4-textwriter-init` |
| `__le__` | operator · L4 · T3 | `TextWriter::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-textwriter-le` |
| `__lt__` | operator · L4 · T3 | `TextWriter::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l4-textwriter-lt` |
| `__ne__` | operator · L4 · T3 | `TextWriter::PartialEq` | `derived PartialEq on TextWriter` | `l4-textwriter-ne` |
| `append` | method · L4 · T2 | `TextWriter::append` | `pub fn append( &mut self, _pos: Point, _text: &str, _options: &AppendOptions, ) -> Result<(Rect, Point)>` | `l4-textwriter-append` |
| `appendv` | method · L4 · T3 | `TextWriter::appendv` | `pub fn appendv( &mut self, _pos: Point, _text: &str, _options: &AppendOptions, ) -> Result<(Rect, Point)>` | `l4-textwriter-appendv` |
| `clean_rtl` | method · L4 · T3 | `TextWriter::clean_rtl` | `pub fn clean_rtl(&self, _text: &str) -> String` | `l4-textwriter-clean-rtl` |
| `fill_textbox` | method · L4 · T3 | `TextWriter::fill_textbox` | `pub fn fill_textbox( &mut self, _rect: Rect, _text: &TextArg, _options: &FillTextBoxOptions, ) -> Result<Vec<(String, f64)>>` | `l4-textwriter-fill-textbox` |
| `write_text` | method · L4 · T2 | `TextWriter::write_text` | `pub fn write_text(&self, _page: &mut PageMut<'_>, _options: &FlushOptions) -> Result<()>` | `l4-textwriter-write-text` |

## Colorspace

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `__eq__` | operator · L3 · T3 | `Colorspace::PartialEq` | `derived PartialEq on Colorspace` | `l3-colorspace-eq` |
| `__ge__` | operator · L3 · T3 | `Colorspace::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-colorspace-ge` |
| `__gt__` | operator · L3 · T3 | `Colorspace::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-colorspace-gt` |
| `__init__` | constructor · L3 · T3 | `Colorspace::new` | `pub fn new(_kind: i32) -> Self` | `l3-colorspace-init` |
| `__le__` | operator · L3 · T3 | `Colorspace::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-colorspace-le` |
| `__lt__` | operator · L3 · T3 | `Colorspace::compare` | `pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool>` | `l3-colorspace-lt` |
| `__ne__` | operator · L3 · T3 | `Colorspace::PartialEq` | `derived PartialEq on Colorspace` | `l3-colorspace-ne` |
| `n` | property · L3 · T2 | `Colorspace::n` | `pub fn n(&self) -> i32` | `l3-colorspace-n` |
| `name` | property · L3 · T3 | `Colorspace::name` | `pub fn name(&self) -> &'static str` | `l3-colorspace-name` |

## module

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `ASSERT_PDF` | function · L5 · T3 | `assert_pdf` | `pub fn assert_pdf(cond: bool)` | `l5-module-assert-pdf` |
| `CheckColor` | function · L5 · T3 | `check_color` | `pub fn check_color(color: Option<&[f64]>) -> Result<()>` | `l5-module-checkcolor` |
| `CheckFont` | function · L1 · T3 | `check_font` | `pub fn check_font(_page: &Page<'_>, _fontname: &str) -> Result<Value>` | `l1-module-checkfont` |
| `CheckFontInfo` | function · L1 · T3 | `check_font_info` | `pub fn check_font_info(_doc: &PdfDocument, _xref: i32) -> Result<Value>` | `l1-module-checkfontinfo` |
| `CheckMarkerArg` | function · L5 · T3 | `check_marker_arg` | `pub fn check_marker_arg(quads: &Value) -> Result<Value>` | `l5-module-checkmarkerarg` |
| `CheckMorph` | function · L5 · T3 | `check_morph` | `pub fn check_morph(value: &Value) -> bool` | `l5-module-checkmorph` |
| `CheckParent` | function · L5 · T3 | `check_parent` | `pub fn check_parent(_value: &Value) -> Result<()>` | `l5-module-checkparent` |
| `CheckQuad` | function · L5 · T3 | `check_quad` | `pub fn check_quad(value: &Value) -> bool` | `l5-module-checkquad` |
| `CheckRect` | function · L5 · T3 | `check_rect` | `pub fn check_rect(value: &Value) -> bool` | `l5-module-checkrect` |
| `ColorCode` | function · L5 · T3 | `color_code` | `pub fn color_code(color: &Value, format: &str) -> String` | `l5-module-colorcode` |
| `ConversionHeader` | function · L5 · T3 | `conversion_header` | `pub fn conversion_header(kind: &str, filename: Option<&str>) -> String` | `l5-module-conversionheader` |
| `ConversionTrailer` | function · L5 · T3 | `conversion_trailer` | `pub fn conversion_trailer(kind: &str) -> String` | `l5-module-conversiontrailer` |
| `EMPTY_IRECT` | function · L5 · T3 | `empty_irect` | `pub fn empty_irect() -> IRect` | `l5-module-empty-irect` |
| `EMPTY_QUAD` | function · L5 · T3 | `empty_quad` | `pub fn empty_quad() -> Quad` | `l5-module-empty-quad` |
| `EMPTY_RECT` | function · L5 · T3 | `empty_rect` | `pub fn empty_rect() -> Rect` | `l5-module-empty-rect` |
| `ENSURE_OPERATION` | function · L5 · T3 | `ensure_operation` | `pub fn ensure_operation(_doc: &PdfDocument) -> Result<()>` | `l5-module-ensure-operation` |
| `INFINITE_IRECT` | function · L5 · T3 | `infinite_irect` | `pub fn infinite_irect() -> IRect` | `l5-module-infinite-irect` |
| `INFINITE_QUAD` | function · L5 · T3 | `infinite_quad` | `pub fn infinite_quad() -> Quad` | `l5-module-infinite-quad` |
| `INFINITE_RECT` | function · L5 · T3 | `infinite_rect` | `pub fn infinite_rect() -> Rect` | `l5-module-infinite-rect` |
| `PDF_NAME` | function · L5 · T3 | `pdf_name` | `pub fn pdf_name(value: &Value) -> String` | `l5-module-pdf-name` |
| `Page__add_text_marker` | function · L1 · T3 | `page_add_text_marker` | `pub fn page_add_text_marker<'b>( _page: &'b mut PageMut<'_>, _quads: &[Quad], _annot_kind: i32, ) -> Result<Annot<'b>>` | `l1-module-page-add-text-marker` |
| `PySequence_Check` | function · L5 · T3 | `py_sequence_check` | `pub fn py_sequence_check(value: &Value) -> bool` | `l5-module-pysequence-check` |
| `PySequence_Size` | function · L5 · T3 | `py_sequence_size` | `pub fn py_sequence_size(value: &Value) -> usize` | `l5-module-pysequence-size` |
| `PyUnicode_DecodeRawUnicodeEscape` | function · L5 · T3 | `py_unicode_decode_raw_unicode_escape` | `pub fn py_unicode_decode_raw_unicode_escape(text: &str, errors: &str) -> String` | `l5-module-pyunicode-decoderawunicodeescape` |
| `RAISEPY` | function · L5 · T3 | `raisepy` | `pub fn raisepy(message: &str, exc: &Value) -> Result<()>` | `l5-module-raisepy` |
| `UpdateFontInfo` | function · L1 · T3 | `update_font_info` | `pub fn update_font_info(_doc: &mut PdfDocument, _info: &[Value]) -> Result<()>` | `l1-module-updatefontinfo` |
| `annot_postprocess` | function · L5 · T3 | `annot_postprocess` | `pub fn annot_postprocess(page: &Page<'_>, _annot: &Annot<'_>) -> Result<()>` | `l5-module-annot-postprocess` |
| `annot_preprocess` | function · L5 · T3 | `annot_preprocess` | `pub fn annot_preprocess(page: &Page<'_>) -> Result<i32>` | `l5-module-annot-preprocess` |
| `apply_pages` | function · L5 · T3 | `apply_pages` | `pub fn apply_pages( path: &str, options: &ApplyPagesOptions, page_fn: &mut dyn FnMut(&Page<'_>) -> Result<()>, ) -> Result<()>` | `l5-module-apply-pages` |
| `args_match` | function · L5 · T3 | `args_match` | `pub fn args_match(args: &[Value], types: &[Value]) -> bool` | `l5-module-args-match` |
| `calc_image_matrix` | function · L3 · T3 | `calc_image_matrix` | `pub fn calc_image_matrix( _width: f64, _height: f64, _target: Rect, _rotate: i32, _keep: bool, ) -> Matrix` | `l3-module-calc-image-matrix` |
| `canon` | function · L1 · T3 | `canon` | `pub fn canon(_ch: char) -> char` | `l1-module-canon` |
| `chartocanon` | function · L1 · T3 | `chartocanon` | `pub fn chartocanon(_text: &str) -> String` | `l1-module-chartocanon` |
| `colors_pdf_dict` | function · L5 · T3 | `colors_pdf_dict` | `pub fn colors_pdf_dict() -> Value` | `l5-module-colors-pdf-dict` |
| `colors_wx_list` | function · L5 · T3 | `colors_wx_list` | `pub fn colors_wx_list() -> Vec<String>` | `l5-module-colors-wx-list` |
| `compute_scissor` | function · L5 · T3 | `compute_scissor` | `pub fn compute_scissor(_device: &Value) -> Value` | `l5-module-compute-scissor` |
| `css_for_pymupdf_font` | function · L1 · T3 | `css_for_font` | `pub fn css_for_font( _fontcode: &str, _css: Option<&str>, _archive: Option<Value>, _name: Option<&str>, ) -> Result<String>` | `l1-module-css-for-pymupdf-font` |
| `dest_is_valid` | function · L5 · T3 | `dest_is_valid` | `pub fn dest_is_valid( dest: &Value, page_count: i32, page_object_nums: &[i32], names: &[String], ) -> bool` | `l5-module-dest-is-valid` |
| `dest_is_valid_page` | function · L5 · T3 | `dest_is_valid_page` | `pub fn dest_is_valid_page(obj: &Value, page_object_nums: &[i32], page_count: i32) -> bool` | `l5-module-dest-is-valid-page` |
| `detect_super_script` | function · L5 · T3 | `detect_super_script` | `pub fn detect_super_script(_line: &Value, _ch: &Value) -> bool` | `l5-module-detect-super-script` |
| `dir_str` | function · L5 · T3 | `dir_str` | `pub fn dir_str(value: &Value) -> String` | `l5-module-dir-str` |
| `exception_info` | function · L5 · T3 | `exception_info` | `pub fn exception_info() -> Value` | `l5-module-exception-info` |
| `extra_FzDocument_insert_pdf` | function · L5 · T3 | `extra_insert_pdf` | `pub fn extra_insert_pdf( doc: &mut PdfDocument, src: &PdfDocument, options: &InsertPdfOptions, ) -> Result<()>` | `l5-module-extra-fzdocument-insert-pdf` |
| `find_string` | function · L5 · T3 | `find_string` | `pub fn find_string(text: &str, needle: &str) -> Option<usize>` | `l5-module-find-string` |
| `format_g` | function · L5 · T3 | `format_g` | `pub fn format_g(value: f64, fmt: &str) -> String` | `l5-module-format-g` |
| `getTJstr` | function · L5 · T3 | `get_tjstr` | `pub fn get_tjstr(text: &str, _glyphs: Option<&[i32]>, simple: bool, _ordering: i32) -> String` | `l5-module-gettjstr` |
| `get_env_bool` | function · L5 · T3 | `get_env_bool` | `pub fn get_env_bool(name: &str, default: bool) -> bool` | `l5-module-get-env-bool` |
| `get_env_int` | function · L5 · T3 | `get_env_int` | `pub fn get_env_int(name: &str, default: i32) -> i32` | `l5-module-get-env-int` |
| `get_highlight_selection` | function · L5 · T3 | `get_highlight_selection` | `pub fn get_highlight_selection( _page: &Page<'_>, _start: Option<Point>, _stop: Option<Point>, _clip: Option<Rect>, ) -> Result<Vec<Quad>>` | `l5-module-get-highlight-selection` |
| `get_pdf_now` | function · L5 · T3 | `get_pdf_now` | `pub fn get_pdf_now() -> String` | `l5-module-get-pdf-now` |
| `get_pdf_str` | function · L5 · T3 | `get_pdf_str` | `pub fn get_pdf_str(text: &str) -> String` | `l5-module-get-pdf-str` |
| `get_tessdata` | function · L1 · T3 | `get_tessdata` | `pub fn get_tessdata(_tessdata: Option<&str>) -> Result<String>` | `l1-module-get-tessdata` |
| `get_text` | function · L1 · T3 | `get_text` | `pub fn get_text(_path: &str, _options: &BulkTextOptions) -> Result<String>` | `l1-module-get-text` |
| `get_text_length` | function · L1 · T2 | `get_text_length` | `pub fn get_text_length(_text: &str, _fontname: &str, _fontsize: f64, _encoding: i32) -> f64` | `l1-module-get-text-length` |
| `glyph_name_to_unicode` | function · L1 · T3 | `glyph_name_to_unicode` | `pub fn glyph_name_to_unicode(_name: &str) -> i32` | `l1-module-glyph-name-to-unicode` |
| `hdist` | function · L5 · T3 | `hdist` | `pub fn hdist(_direction: &Value, a: &Value, b: &Value) -> f64` | `l5-module-hdist` |
| `image_profile` | function · L3 · T3 | `image_profile` | `pub fn image_profile(_data: &[u8]) -> Result<ImageProfile>` | `l3-module-image-profile` |
| `log` | function · L5 · T3 | `log` | `pub fn log(text: &str, caller: i32)` | `l5-module-log` |
| `make_escape` | function · L5 · T3 | `make_escape` | `pub fn make_escape(ch: char) -> String` | `l5-module-make-escape` |
| `make_story_elpos` | function · L5 · T3 | `make_story_elpos` | `pub fn make_story_elpos() -> Value` | `l5-module-make-story-elpos` |
| `make_table` | function · L1 · T3 | `make_table` | `pub fn make_table(_rect: Rect, _cols: i32, _rows: i32) -> Vec<TableCell>` | `l1-module-make-table` |
| `match_string` | function · L5 · T3 | `match_string` | `pub fn match_string(haystack: &Value, needle: &Value) -> bool` | `l5-module-match-string` |
| `message` | function · L5 · T3 | `message` | `pub fn message(text: &str)` | `l5-module-message` |
| `message_warning` | function · L5 · T3 | `message_warning` | `pub fn message_warning(text: &str)` | `l5-module-message-warning` |
| `no_recommend_layout` | function · L5 · T3 | `no_recommend_layout` | `pub fn no_recommend_layout() -> bool` | `l5-module-no-recommend-layout` |
| `on_highlight_char` | function · L1 · T3 | `on_highlight_char` | `pub fn on_highlight_char(_hits: &Value, _line: &Value, _ch: &Value) -> Result<Value>` | `l1-module-on-highlight-char` |
| `page_merge` | function · L5 · T3 | `page_merge` | `pub fn page_merge( target: &mut PdfDocument, source: &PdfDocument, page_from: i32, page_to: i32, options: &PageMergeOptions, ) -> Result<()>` | `l5-module-page-merge` |
| `paper_rect` | function · L5 · T3 | `paper_rect` | `pub fn paper_rect(name: &str) -> Result<Rect>` | `l5-module-paper-rect` |
| `paper_size` | function · L5 · T2 | `paper_size` | `pub fn paper_size(name: &str) -> Result<(f64, f64)>` | `l5-module-paper-size` |
| `paper_sizes` | function · L5 · T3 | `paper_sizes` | `pub fn paper_sizes() -> Vec<String>` | `l5-module-paper-sizes` |
| `pdf_lookup_page_loc` | function · L5 · T3 | `pdf_lookup_page_loc` | `pub fn pdf_lookup_page_loc(doc: &PdfDocument, needle: &Value) -> Result<Value>` | `l5-module-pdf-lookup-page-loc` |
| `pdfobj_string` | function · L5 · T3 | `pdfobj_string` | `pub fn pdfobj_string(obj: &Value, prefix: &str) -> String` | `l5-module-pdfobj-string` |
| `planish_line` | function · L5 · T3 | `planish_line` | `pub fn planish_line(p1: Point, p2: Point) -> Matrix` | `l5-module-planish-line` |
| `recover_bbox_quad` | function · L1 · T3 | `recover_bbox_quad` | `pub fn recover_bbox_quad(_direction: (f64, f64), _span: &DictSpan, _bbox: Rect) -> Quad` | `l1-module-recover-bbox-quad` |
| `recover_char_quad` | function · L1 · T3 | `recover_char_quad` | `pub fn recover_char_quad(_direction: (f64, f64), _span: &DictSpan, _ch: &RawChar) -> Quad` | `l1-module-recover-char-quad` |
| `recover_line_quad` | function · L1 · T3 | `recover_line_quad` | `pub fn recover_line_quad(_line: &DictLine, _spans: Option<&[DictSpan]>) -> Quad` | `l1-module-recover-line-quad` |
| `recover_quad` | function · L1 · T3 | `recover_quad` | `pub fn recover_quad(_direction: (f64, f64), _span: &DictSpan) -> Quad` | `l1-module-recover-quad` |
| `recover_span_quad` | function · L1 · T3 | `recover_span_quad` | `pub fn recover_span_quad( _direction: (f64, f64), _span: &DictSpan, _chars: Option<&[RawChar]>, ) -> Quad` | `l1-module-recover-span-quad` |
| `repair_mono_font` | function · L1 · T3 | `repair_mono_font` | `pub fn repair_mono_font(_page: &mut PageMut<'_>, _font: &FontInfo) -> Result<()>` | `l1-module-repair-mono-font` |
| `restore_aliases` | function · L5 · T3 | `restore_aliases` | `pub fn restore_aliases()` | `l5-module-restore-aliases` |
| `sRGB_to_pdf` | function · L5 · T3 | `srgb_to_pdf` | `pub fn srgb_to_pdf(srgb: i32) -> (f64, f64, f64)` | `l5-module-srgb-to-pdf` |
| `sRGB_to_rgb` | function · L5 · T3 | `srgb_to_rgb` | `pub fn srgb_to_rgb(srgb: i32) -> (f64, f64, f64)` | `l5-module-srgb-to-rgb` |
| `set_log` | function · L5 · T3 | `set_log` | `pub fn set_log(_options: &LogOptions)` | `l5-module-set-log` |
| `set_messages` | function · L5 · T3 | `set_messages` | `pub fn set_messages(_options: &LogOptions)` | `l5-module-set-messages` |
| `string_in_names_list` | function · L5 · T3 | `string_in_names_list` | `pub fn string_in_names_list(text: &str, names: &[String]) -> bool` | `l5-module-string-in-names-list` |
| `strip_outline` | function · L5 · T3 | `strip_outline` | `pub fn strip_outline( doc: &mut PdfDocument, outlines: &Value, page_count: i32, page_object_nums: &[i32], names: &[String], ) -> Result<()>` | `l5-module-strip-outline` |
| `strip_outlines` | function · L5 · T3 | `strip_outlines` | `pub fn strip_outlines( doc: &mut PdfDocument, outlines: &Value, page_count: i32, page_object_nums: &[i32], names: &[String], ) -> Result<()>` | `l5-module-strip-outlines` |
| `unicode_to_glyph_name` | function · L1 · T3 | `unicode_to_glyph_name` | `pub fn unicode_to_glyph_name(_code: i32) -> String` | `l1-module-unicode-to-glyph-name` |
| `util_concat_matrix` | function · L5 · T3 | `util_concat_matrix` | `pub fn util_concat_matrix(one: Matrix, two: Matrix) -> Matrix` | `l5-module-util-concat-matrix` |
| `util_ensure_widget_calc` | function · L5 · T3 | `util_ensure_widget_calc` | `pub fn util_ensure_widget_calc(_annot: &Annot<'_>) -> Result<()>` | `l5-module-util-ensure-widget-calc` |
| `util_hor_matrix` | function · L5 · T3 | `util_hor_matrix` | `pub fn util_hor_matrix(center: Point, point: Point) -> Matrix` | `l5-module-util-hor-matrix` |
| `util_include_point_in_rect` | function · L5 · T3 | `util_include_point_in_rect` | `pub fn util_include_point_in_rect(rect: Rect, point: Point) -> Rect` | `l5-module-util-include-point-in-rect` |
| `util_intersect_rect` | function · L5 · T3 | `util_intersect_rect` | `pub fn util_intersect_rect(one: Rect, two: Rect) -> Rect` | `l5-module-util-intersect-rect` |
| `util_invert_matrix` | function · L5 · T3 | `util_invert_matrix` | `pub fn util_invert_matrix(matrix: Matrix) -> Matrix` | `l5-module-util-invert-matrix` |
| `util_is_point_in_rect` | function · L5 · T3 | `util_is_point_in_rect` | `pub fn util_is_point_in_rect(point: Point, rect: Rect) -> i32` | `l5-module-util-is-point-in-rect` |
| `util_make_irect` | function · L5 · T3 | `util_make_irect` | `pub fn util_make_irect(x0: i32, y0: i32, x1: i32, y1: i32) -> IRect` | `l5-module-util-make-irect` |
| `util_make_rect` | function · L5 · T3 | `util_make_rect` | `pub fn util_make_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect` | `l5-module-util-make-rect` |
| `util_measure_string` | function · L5 · T3 | `util_measure_string` | `pub fn util_measure_string(text: &str, fontname: &str, fontsize: f64, _encoding: i32) -> f64` | `l5-module-util-measure-string` |
| `util_point_in_quad` | function · L5 · T3 | `util_point_in_quad` | `pub fn util_point_in_quad(point: Point, quad: Quad) -> bool` | `l5-module-util-point-in-quad` |
| `util_round_rect` | function · L5 · T3 | `util_round_rect` | `pub fn util_round_rect(rect: Rect) -> IRect` | `l5-module-util-round-rect` |
| `util_sine_between` | function · L5 · T3 | `util_sine_between` | `pub fn util_sine_between(center: Point, point: Point, other: Point) -> f64` | `l5-module-util-sine-between` |
| `util_transform_point` | function · L5 · T3 | `util_transform_point` | `pub fn util_transform_point(point: Point, matrix: Matrix) -> Point` | `l5-module-util-transform-point` |
| `util_transform_rect` | function · L5 · T3 | `util_transform_rect` | `pub fn util_transform_rect(rect: Rect, matrix: Matrix) -> Rect` | `l5-module-util-transform-rect` |
| `util_union_rect` | function · L5 · T3 | `util_union_rect` | `pub fn util_union_rect(one: Rect, two: Rect) -> Rect` | `l5-module-util-union-rect` |
| `vdist` | function · L5 · T3 | `vdist` | `pub fn vdist(_direction: &Value, a: &Value, b: &Value) -> f64` | `l5-module-vdist` |

## constants

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `EPSILON` | property · P0 · T3 | `EPSILON` | `pub const EPSILON: f64` | `p0-constants-epsilon` |
| `FLT_EPSILON` | property · P0 · T3 | `FLT_EPSILON` | `pub const FLT_EPSILON: f64` | `p0-constants-flt-epsilon` |
| `FZ_MAX_INF_RECT` | property · P0 · T3 | `FZ_MAX_INF_RECT` | `pub const FZ_MAX_INF_RECT: i32` | `p0-constants-fz-max-inf-rect` |
| `FZ_MIN_INF_RECT` | property · P0 · T3 | `FZ_MIN_INF_RECT` | `pub const FZ_MIN_INF_RECT: i32` | `p0-constants-fz-min-inf-rect` |
| `Identity` | property · P0 · T3 | `Identity` | `pub const Identity: Matrix` | `p0-constants-identity` |
| `TEXT_ACCURATE_ASCENDERS` | property · L1 · T3 | `TEXT_ACCURATE_ASCENDERS` | `pub const TEXT_ACCURATE_ASCENDERS: i32` | `l1-constants-text-accurate-ascenders` |
| `TEXT_ACCURATE_BBOXES` | property · L1 · T3 | `TEXT_ACCURATE_BBOXES` | `pub const TEXT_ACCURATE_BBOXES: i32` | `l1-constants-text-accurate-bboxes` |
| `TEXT_ACCURATE_SIDE_BEARINGS` | property · L1 · T3 | `TEXT_ACCURATE_SIDE_BEARINGS` | `pub const TEXT_ACCURATE_SIDE_BEARINGS: i32` | `l1-constants-text-accurate-side-bearings` |
| `TEXT_ALIGN_CENTER` | property · L1 · T2 | `TEXT_ALIGN_CENTER` | `pub const TEXT_ALIGN_CENTER: i32` | `l2-constants-all` |
| `TEXT_ALIGN_JUSTIFY` | property · L1 · T3 | `TEXT_ALIGN_JUSTIFY` | `pub const TEXT_ALIGN_JUSTIFY: i32` | `l1-constants-text-align-justify` |
| `TEXT_ALIGN_LEFT` | property · L1 · T2 | `TEXT_ALIGN_LEFT` | `pub const TEXT_ALIGN_LEFT: i32` | `l2-constants-all` |
| `TEXT_ALIGN_RIGHT` | property · L1 · T3 | `TEXT_ALIGN_RIGHT` | `pub const TEXT_ALIGN_RIGHT: i32` | `l1-constants-text-align-right` |
| `TEXT_CID_FOR_UNKNOWN_UNICODE` | property · L1 · T3 | `TEXT_CID_FOR_UNKNOWN_UNICODE` | `pub const TEXT_CID_FOR_UNKNOWN_UNICODE: i32` | `l1-constants-text-cid-for-unknown-unicode` |
| `TEXT_CLIP` | property · L1 · T3 | `TEXT_CLIP` | `pub const TEXT_CLIP: i32` | `l1-constants-text-clip` |
| `TEXT_CLIP_RECT` | property · L1 · T3 | `TEXT_CLIP_RECT` | `pub const TEXT_CLIP_RECT: i32` | `l1-constants-text-clip-rect` |
| `TEXT_COLLECT_STRUCTURE` | property · L1 · T3 | `TEXT_COLLECT_STRUCTURE` | `pub const TEXT_COLLECT_STRUCTURE: i32` | `l1-constants-text-collect-structure` |
| `TEXT_COLLECT_STYLES` | property · L1 · T3 | `TEXT_COLLECT_STYLES` | `pub const TEXT_COLLECT_STYLES: i32` | `l1-constants-text-collect-styles` |
| `TEXT_COLLECT_VECTORS` | property · L1 · T3 | `TEXT_COLLECT_VECTORS` | `pub const TEXT_COLLECT_VECTORS: i32` | `l1-constants-text-collect-vectors` |
| `TEXT_DEHYPHENATE` | property · L1 · T3 | `TEXT_DEHYPHENATE` | `pub const TEXT_DEHYPHENATE: i32` | `l1-constants-text-dehyphenate` |
| `TEXT_ENCODING_CYRILLIC` | property · L1 · T3 | `TEXT_ENCODING_CYRILLIC` | `pub const TEXT_ENCODING_CYRILLIC: i32` | `l1-constants-text-encoding-cyrillic` |
| `TEXT_ENCODING_GREEK` | property · L1 · T3 | `TEXT_ENCODING_GREEK` | `pub const TEXT_ENCODING_GREEK: i32` | `l1-constants-text-encoding-greek` |
| `TEXT_ENCODING_LATIN` | property · L1 · T3 | `TEXT_ENCODING_LATIN` | `pub const TEXT_ENCODING_LATIN: i32` | `l1-constants-text-encoding-latin` |
| `TEXT_FONT_BOLD` | property · L1 · T3 | `TEXT_FONT_BOLD` | `pub const TEXT_FONT_BOLD: i32` | `l1-constants-text-font-bold` |
| `TEXT_FONT_ITALIC` | property · L1 · T3 | `TEXT_FONT_ITALIC` | `pub const TEXT_FONT_ITALIC: i32` | `l1-constants-text-font-italic` |
| `TEXT_FONT_MONOSPACED` | property · L1 · T3 | `TEXT_FONT_MONOSPACED` | `pub const TEXT_FONT_MONOSPACED: i32` | `l1-constants-text-font-monospaced` |
| `TEXT_FONT_SERIFED` | property · L1 · T3 | `TEXT_FONT_SERIFED` | `pub const TEXT_FONT_SERIFED: i32` | `l1-constants-text-font-serifed` |
| `TEXT_FONT_SUPERSCRIPT` | property · L1 · T3 | `TEXT_FONT_SUPERSCRIPT` | `pub const TEXT_FONT_SUPERSCRIPT: i32` | `l1-constants-text-font-superscript` |
| `TEXT_FUZZY_VECTORS` | property · L1 · T3 | `TEXT_FUZZY_VECTORS` | `pub const TEXT_FUZZY_VECTORS: i32` | `l1-constants-text-fuzzy-vectors` |
| `TEXT_IGNORE_ACTUALTEXT` | property · L1 · T3 | `TEXT_IGNORE_ACTUALTEXT` | `pub const TEXT_IGNORE_ACTUALTEXT: i32` | `l1-constants-text-ignore-actualtext` |
| `TEXT_INHIBIT_SPACES` | property · L1 · T3 | `TEXT_INHIBIT_SPACES` | `pub const TEXT_INHIBIT_SPACES: i32` | `l1-constants-text-inhibit-spaces` |
| `TEXT_LAZY_VECTORS` | property · L1 · T3 | `TEXT_LAZY_VECTORS` | `pub const TEXT_LAZY_VECTORS: i32` | `l1-constants-text-lazy-vectors` |
| `TEXT_MEDIABOX_CLIP` | property · L1 · T3 | `TEXT_MEDIABOX_CLIP` | `pub const TEXT_MEDIABOX_CLIP: i32` | `l1-constants-text-mediabox-clip` |
| `TEXT_OUTPUT_HTML` | property · L1 · T3 | `TEXT_OUTPUT_HTML` | `pub const TEXT_OUTPUT_HTML: i32` | `l1-constants-text-output-html` |
| `TEXT_OUTPUT_JSON` | property · L1 · T3 | `TEXT_OUTPUT_JSON` | `pub const TEXT_OUTPUT_JSON: i32` | `l1-constants-text-output-json` |
| `TEXT_OUTPUT_TEXT` | property · L1 · T3 | `TEXT_OUTPUT_TEXT` | `pub const TEXT_OUTPUT_TEXT: i32` | `l1-constants-text-output-text` |
| `TEXT_OUTPUT_XHTML` | property · L1 · T3 | `TEXT_OUTPUT_XHTML` | `pub const TEXT_OUTPUT_XHTML: i32` | `l1-constants-text-output-xhtml` |
| `TEXT_OUTPUT_XML` | property · L1 · T3 | `TEXT_OUTPUT_XML` | `pub const TEXT_OUTPUT_XML: i32` | `l1-constants-text-output-xml` |
| `TEXT_PARAGRAPH_BREAK` | property · L1 · T3 | `TEXT_PARAGRAPH_BREAK` | `pub const TEXT_PARAGRAPH_BREAK: i32` | `l1-constants-text-paragraph-break` |
| `TEXT_PRESERVE_IMAGES` | property · L1 · T2 | `TEXT_PRESERVE_IMAGES` | `pub const TEXT_PRESERVE_IMAGES: i32` | `l2-constants-all` |
| `TEXT_PRESERVE_LIGATURES` | property · L1 · T2 | `TEXT_PRESERVE_LIGATURES` | `pub const TEXT_PRESERVE_LIGATURES: i32` | `l2-constants-all` |
| `TEXT_PRESERVE_SPANS` | property · L1 · T3 | `TEXT_PRESERVE_SPANS` | `pub const TEXT_PRESERVE_SPANS: i32` | `l1-constants-text-preserve-spans` |
| `TEXT_PRESERVE_WHITESPACE` | property · L1 · T2 | `TEXT_PRESERVE_WHITESPACE` | `pub const TEXT_PRESERVE_WHITESPACE: i32` | `l2-constants-all` |
| `TEXT_SEGMENT` | property · L1 · T3 | `TEXT_SEGMENT` | `pub const TEXT_SEGMENT: i32` | `l1-constants-text-segment` |
| `TEXT_STEXT_SEGMENT` | property · L1 · T3 | `TEXT_STEXT_SEGMENT` | `pub const TEXT_STEXT_SEGMENT: i32` | `l1-constants-text-stext-segment` |
| `TEXT_TABLE_HUNT` | property · L1 · T3 | `TEXT_TABLE_HUNT` | `pub const TEXT_TABLE_HUNT: i32` | `l1-constants-text-table-hunt` |
| `TEXT_USE_CID_FOR_UNKNOWN_UNICODE` | property · L1 · T3 | `TEXT_USE_CID_FOR_UNKNOWN_UNICODE` | `pub const TEXT_USE_CID_FOR_UNKNOWN_UNICODE: i32` | `l1-constants-text-use-cid-for-unknown-unicode` |
| `TEXT_USE_GID_FOR_UNKNOWN_UNICODE` | property · L1 · T3 | `TEXT_USE_GID_FOR_UNKNOWN_UNICODE` | `pub const TEXT_USE_GID_FOR_UNKNOWN_UNICODE: i32` | `l1-constants-text-use-gid-for-unknown-unicode` |
| `TEXTFLAGS_BLOCKS` | property · L1 · T3 | `TEXTFLAGS_BLOCKS` | `pub const TEXTFLAGS_BLOCKS: i32` | `l1-constants-textflags-blocks` |
| `TEXTFLAGS_DICT` | property · L1 · T2 | `TEXTFLAGS_DICT` | `pub const TEXTFLAGS_DICT: i32` | `l2-constants-all` |
| `TEXTFLAGS_HTML` | property · L1 · T3 | `TEXTFLAGS_HTML` | `pub const TEXTFLAGS_HTML: i32` | `l1-constants-textflags-html` |
| `TEXTFLAGS_RAWDICT` | property · L1 · T3 | `TEXTFLAGS_RAWDICT` | `pub const TEXTFLAGS_RAWDICT: i32` | `l1-constants-textflags-rawdict` |
| `TEXTFLAGS_SEARCH` | property · L1 · T3 | `TEXTFLAGS_SEARCH` | `pub const TEXTFLAGS_SEARCH: i32` | `l1-constants-textflags-search` |
| `TEXTFLAGS_TEXT` | property · L1 · T2 | `TEXTFLAGS_TEXT` | `pub const TEXTFLAGS_TEXT: i32` | `l2-constants-all` |
| `TEXTFLAGS_WORDS` | property · L1 · T3 | `TEXTFLAGS_WORDS` | `pub const TEXTFLAGS_WORDS: i32` | `l1-constants-textflags-words` |
| `TEXTFLAGS_XHTML` | property · L1 · T3 | `TEXTFLAGS_XHTML` | `pub const TEXTFLAGS_XHTML: i32` | `l1-constants-textflags-xhtml` |
| `TEXTFLAGS_XML` | property · L1 · T3 | `TEXTFLAGS_XML` | `pub const TEXTFLAGS_XML: i32` | `l1-constants-textflags-xml` |
| `dictkey_align` | property · L1 · T3 | `dictkey_align` | `pub const dictkey_align: &str` | `l1-constants-dictkey-align` |
| `dictkey_asc` | property · L1 · T3 | `dictkey_asc` | `pub const dictkey_asc: &str` | `l1-constants-dictkey-asc` |
| `dictkey_bbox` | property · L1 · T3 | `dictkey_bbox` | `pub const dictkey_bbox: &str` | `l1-constants-dictkey-bbox` |
| `dictkey_bidi` | property · L1 · T3 | `dictkey_bidi` | `pub const dictkey_bidi: &str` | `l1-constants-dictkey-bidi` |
| `dictkey_blocks` | property · L1 · T3 | `dictkey_blocks` | `pub const dictkey_blocks: &str` | `l1-constants-dictkey-blocks` |
| `dictkey_bpc` | property · L1 · T3 | `dictkey_bpc` | `pub const dictkey_bpc: &str` | `l1-constants-dictkey-bpc` |
| `dictkey_c` | property · L1 · T3 | `dictkey_c` | `pub const dictkey_c: &str` | `l1-constants-dictkey-c` |
| `dictkey_char_flags` | property · L1 · T3 | `dictkey_char_flags` | `pub const dictkey_char_flags: &str` | `l1-constants-dictkey-char-flags` |
| `dictkey_chars` | property · L1 · T3 | `dictkey_chars` | `pub const dictkey_chars: &str` | `l1-constants-dictkey-chars` |
| `dictkey_color` | property · L1 · T3 | `dictkey_color` | `pub const dictkey_color: &str` | `l1-constants-dictkey-color` |
| `dictkey_colorspace` | property · L1 · T3 | `dictkey_colorspace` | `pub const dictkey_colorspace: &str` | `l1-constants-dictkey-colorspace` |
| `dictkey_content` | property · L1 · T3 | `dictkey_content` | `pub const dictkey_content: &str` | `l1-constants-dictkey-content` |
| `dictkey_creationDate` | property · L1 · T3 | `dictkey_creationDate` | `pub const dictkey_creationDate: &str` | `l1-constants-dictkey-creationdate` |
| `dictkey_cs_name` | property · L1 · T3 | `dictkey_cs_name` | `pub const dictkey_cs_name: &str` | `l1-constants-dictkey-cs-name` |
| `dictkey_da` | property · L1 · T3 | `dictkey_da` | `pub const dictkey_da: &str` | `l1-constants-dictkey-da` |
| `dictkey_dashes` | property · L1 · T3 | `dictkey_dashes` | `pub const dictkey_dashes: &str` | `l1-constants-dictkey-dashes` |
| `dictkey_desc` | property · L1 · T3 | `dictkey_desc` | `pub const dictkey_desc: &str` | `l1-constants-dictkey-desc` |
| `dictkey_descr` | property · L1 · T3 | `dictkey_descr` | `pub const dictkey_descr: &str` | `l1-constants-dictkey-descr` |
| `dictkey_dir` | property · L1 · T3 | `dictkey_dir` | `pub const dictkey_dir: &str` | `l1-constants-dictkey-dir` |
| `dictkey_effect` | property · L1 · T3 | `dictkey_effect` | `pub const dictkey_effect: &str` | `l1-constants-dictkey-effect` |
| `dictkey_ext` | property · L1 · T3 | `dictkey_ext` | `pub const dictkey_ext: &str` | `l1-constants-dictkey-ext` |
| `dictkey_filename` | property · L1 · T3 | `dictkey_filename` | `pub const dictkey_filename: &str` | `l1-constants-dictkey-filename` |
| `dictkey_fill` | property · L1 · T3 | `dictkey_fill` | `pub const dictkey_fill: &str` | `l1-constants-dictkey-fill` |
| `dictkey_flags` | property · L1 · T3 | `dictkey_flags` | `pub const dictkey_flags: &str` | `l1-constants-dictkey-flags` |
| `dictkey_font` | property · L1 · T3 | `dictkey_font` | `pub const dictkey_font: &str` | `l1-constants-dictkey-font` |
| `dictkey_glyph` | property · L1 · T3 | `dictkey_glyph` | `pub const dictkey_glyph: &str` | `l1-constants-dictkey-glyph` |
| `dictkey_height` | property · L1 · T3 | `dictkey_height` | `pub const dictkey_height: &str` | `l1-constants-dictkey-height` |
| `dictkey_id` | property · L1 · T3 | `dictkey_id` | `pub const dictkey_id: &str` | `l1-constants-dictkey-id` |
| `dictkey_image` | property · L1 · T3 | `dictkey_image` | `pub const dictkey_image: &str` | `l1-constants-dictkey-image` |
| `dictkey_items` | property · L1 · T3 | `dictkey_items` | `pub const dictkey_items: &str` | `l1-constants-dictkey-items` |
| `dictkey_length` | property · L1 · T3 | `dictkey_length` | `pub const dictkey_length: &str` | `l1-constants-dictkey-length` |
| `dictkey_lines` | property · L1 · T3 | `dictkey_lines` | `pub const dictkey_lines: &str` | `l1-constants-dictkey-lines` |
| `dictkey_matrix` | property · L1 · T3 | `dictkey_matrix` | `pub const dictkey_matrix: &str` | `l1-constants-dictkey-matrix` |
| `dictkey_modDate` | property · L1 · T3 | `dictkey_modDate` | `pub const dictkey_modDate: &str` | `l1-constants-dictkey-moddate` |
| `dictkey_name` | property · L1 · T3 | `dictkey_name` | `pub const dictkey_name: &str` | `l1-constants-dictkey-name` |
| `dictkey_number` | property · L1 · T3 | `dictkey_number` | `pub const dictkey_number: &str` | `l1-constants-dictkey-number` |
| `dictkey_origin` | property · L1 · T3 | `dictkey_origin` | `pub const dictkey_origin: &str` | `l1-constants-dictkey-origin` |
| `dictkey_rect` | property · L1 · T3 | `dictkey_rect` | `pub const dictkey_rect: &str` | `l1-constants-dictkey-rect` |
| `dictkey_size` | property · L1 · T3 | `dictkey_size` | `pub const dictkey_size: &str` | `l1-constants-dictkey-size` |
| `dictkey_smask` | property · L1 · T3 | `dictkey_smask` | `pub const dictkey_smask: &str` | `l1-constants-dictkey-smask` |
| `dictkey_spans` | property · L1 · T3 | `dictkey_spans` | `pub const dictkey_spans: &str` | `l1-constants-dictkey-spans` |
| `dictkey_stroke` | property · L1 · T3 | `dictkey_stroke` | `pub const dictkey_stroke: &str` | `l1-constants-dictkey-stroke` |
| `dictkey_style` | property · L1 · T3 | `dictkey_style` | `pub const dictkey_style: &str` | `l1-constants-dictkey-style` |
| `dictkey_subject` | property · L1 · T3 | `dictkey_subject` | `pub const dictkey_subject: &str` | `l1-constants-dictkey-subject` |
| `dictkey_text` | property · L1 · T3 | `dictkey_text` | `pub const dictkey_text: &str` | `l1-constants-dictkey-text` |
| `dictkey_title` | property · L1 · T3 | `dictkey_title` | `pub const dictkey_title: &str` | `l1-constants-dictkey-title` |
| `dictkey_type` | property · L1 · T3 | `dictkey_type` | `pub const dictkey_type: &str` | `l1-constants-dictkey-type` |
| `dictkey_ufilename` | property · L1 · T3 | `dictkey_ufilename` | `pub const dictkey_ufilename: &str` | `l1-constants-dictkey-ufilename` |
| `dictkey_width` | property · L1 · T3 | `dictkey_width` | `pub const dictkey_width: &str` | `l1-constants-dictkey-width` |
| `dictkey_wmode` | property · L1 · T3 | `dictkey_wmode` | `pub const dictkey_wmode: &str` | `l1-constants-dictkey-wmode` |
| `dictkey_xref` | property · L1 · T3 | `dictkey_xref` | `pub const dictkey_xref: &str` | `l1-constants-dictkey-xref` |
| `dictkey_xres` | property · L1 · T3 | `dictkey_xres` | `pub const dictkey_xres: &str` | `l1-constants-dictkey-xres` |
| `dictkey_yres` | property · L1 · T3 | `dictkey_yres` | `pub const dictkey_yres: &str` | `l1-constants-dictkey-yres` |
| `PDF_STRUCT_BROKEN` | property · L1 · T3 | `PDF_STRUCT_BROKEN` | `pub const PDF_STRUCT_BROKEN: i32` | `l1-constants-pdf-struct-broken` |
| `PDF_STRUCT_FIXED` | property · L1 · T3 | `PDF_STRUCT_FIXED` | `pub const PDF_STRUCT_FIXED: i32` | `l1-constants-pdf-struct-fixed` |
| `PDF_STRUCT_HAS_ATTRIBUTES` | property · L1 · T3 | `PDF_STRUCT_HAS_ATTRIBUTES` | `pub const PDF_STRUCT_HAS_ATTRIBUTES: i32` | `l1-constants-pdf-struct-has-attributes` |
| `PDF_STRUCT_HAS_CYCLE` | property · L1 · T3 | `PDF_STRUCT_HAS_CYCLE` | `pub const PDF_STRUCT_HAS_CYCLE: i32` | `l1-constants-pdf-struct-has-cycle` |
| `PDF_STRUCT_HAS_TABLE_ATTRIBUTES` | property · L1 · T3 | `PDF_STRUCT_HAS_TABLE_ATTRIBUTES` | `pub const PDF_STRUCT_HAS_TABLE_ATTRIBUTES: i32` | `l1-constants-pdf-struct-has-table-attributes` |
| `PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES` | property · L1 · T3 | `PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES` | `pub const PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES: i32` | `l1-constants-pdf-struct-has-table-span-attributes` |
| `PDF_STRUCT_NOT_PRESENT` | property · L1 · T3 | `PDF_STRUCT_NOT_PRESENT` | `pub const PDF_STRUCT_NOT_PRESENT: i32` | `l1-constants-pdf-struct-not-present` |
| `PDF_STRUCT_PRESENT` | property · L1 · T3 | `PDF_STRUCT_PRESENT` | `pub const PDF_STRUCT_PRESENT: i32` | `l1-constants-pdf-struct-present` |
| `LINK_FLAG_B_VALID` | property · L2 · T3 | `LINK_FLAG_B_VALID` | `pub const LINK_FLAG_B_VALID: i32` | `l2-constants-link-flag-b-valid` |
| `LINK_FLAG_FIT_H` | property · L2 · T3 | `LINK_FLAG_FIT_H` | `pub const LINK_FLAG_FIT_H: i32` | `l2-constants-link-flag-fit-h` |
| `LINK_FLAG_FIT_V` | property · L2 · T3 | `LINK_FLAG_FIT_V` | `pub const LINK_FLAG_FIT_V: i32` | `l2-constants-link-flag-fit-v` |
| `LINK_FLAG_L_VALID` | property · L2 · T3 | `LINK_FLAG_L_VALID` | `pub const LINK_FLAG_L_VALID: i32` | `l2-constants-link-flag-l-valid` |
| `LINK_FLAG_R_IS_ZOOM` | property · L2 · T3 | `LINK_FLAG_R_IS_ZOOM` | `pub const LINK_FLAG_R_IS_ZOOM: i32` | `l2-constants-link-flag-r-is-zoom` |
| `LINK_FLAG_R_VALID` | property · L2 · T3 | `LINK_FLAG_R_VALID` | `pub const LINK_FLAG_R_VALID: i32` | `l2-constants-link-flag-r-valid` |
| `LINK_FLAG_T_VALID` | property · L2 · T3 | `LINK_FLAG_T_VALID` | `pub const LINK_FLAG_T_VALID: i32` | `l2-constants-link-flag-t-valid` |
| `LINK_GOTO` | property · L2 · T2 | `LINK_GOTO` | `pub const LINK_GOTO: i32` | `l2-constants-all` |
| `LINK_GOTOR` | property · L2 · T3 | `LINK_GOTOR` | `pub const LINK_GOTOR: i32` | `l2-constants-link-gotor` |
| `LINK_LAUNCH` | property · L2 · T3 | `LINK_LAUNCH` | `pub const LINK_LAUNCH: i32` | `l2-constants-link-launch` |
| `LINK_NAMED` | property · L2 · T3 | `LINK_NAMED` | `pub const LINK_NAMED: i32` | `l2-constants-link-named` |
| `LINK_NONE` | property · L2 · T3 | `LINK_NONE` | `pub const LINK_NONE: i32` | `l2-constants-link-none` |
| `LINK_URI` | property · L2 · T2 | `LINK_URI` | `pub const LINK_URI: i32` | `l2-constants-all` |
| `PDF_ENCRYPT_AES_128` | property · L2 · T3 | `PDF_ENCRYPT_AES_128` | `pub const PDF_ENCRYPT_AES_128: i32` | `l2-constants-pdf-encrypt-aes-128` |
| `PDF_ENCRYPT_AES_256` | property · L2 · T2 | `PDF_ENCRYPT_AES_256` | `pub const PDF_ENCRYPT_AES_256: i32` | `l2-constants-all` |
| `PDF_ENCRYPT_KEEP` | property · L2 · T2 | `PDF_ENCRYPT_KEEP` | `pub const PDF_ENCRYPT_KEEP: i32` | `l2-constants-all` |
| `PDF_ENCRYPT_NONE` | property · L2 · T2 | `PDF_ENCRYPT_NONE` | `pub const PDF_ENCRYPT_NONE: i32` | `l2-constants-all` |
| `PDF_ENCRYPT_RC4_128` | property · L2 · T3 | `PDF_ENCRYPT_RC4_128` | `pub const PDF_ENCRYPT_RC4_128: i32` | `l2-constants-pdf-encrypt-rc4-128` |
| `PDF_ENCRYPT_RC4_40` | property · L2 · T3 | `PDF_ENCRYPT_RC4_40` | `pub const PDF_ENCRYPT_RC4_40: i32` | `l2-constants-pdf-encrypt-rc4-40` |
| `PDF_ENCRYPT_UNKNOWN` | property · L2 · T3 | `PDF_ENCRYPT_UNKNOWN` | `pub const PDF_ENCRYPT_UNKNOWN: i32` | `l2-constants-pdf-encrypt-unknown` |
| `PDF_PERM_ACCESSIBILITY` | property · L2 · T3 | `PDF_PERM_ACCESSIBILITY` | `pub const PDF_PERM_ACCESSIBILITY: i32` | `l2-constants-pdf-perm-accessibility` |
| `PDF_PERM_ANNOTATE` | property · L2 · T2 | `PDF_PERM_ANNOTATE` | `pub const PDF_PERM_ANNOTATE: i32` | `l2-constants-all` |
| `PDF_PERM_ASSEMBLE` | property · L2 · T3 | `PDF_PERM_ASSEMBLE` | `pub const PDF_PERM_ASSEMBLE: i32` | `l2-constants-pdf-perm-assemble` |
| `PDF_PERM_COPY` | property · L2 · T2 | `PDF_PERM_COPY` | `pub const PDF_PERM_COPY: i32` | `l2-constants-all` |
| `PDF_PERM_FORM` | property · L2 · T2 | `PDF_PERM_FORM` | `pub const PDF_PERM_FORM: i32` | `l2-constants-all` |
| `PDF_PERM_MODIFY` | property · L2 · T2 | `PDF_PERM_MODIFY` | `pub const PDF_PERM_MODIFY: i32` | `l2-constants-all` |
| `PDF_PERM_PRINT` | property · L2 · T2 | `PDF_PERM_PRINT` | `pub const PDF_PERM_PRINT: i32` | `l2-constants-all` |
| `PDF_PERM_PRINT_HQ` | property · L2 · T2 | `PDF_PERM_PRINT_HQ` | `pub const PDF_PERM_PRINT_HQ: i32` | `l2-constants-all` |
| `PDF_PAGE_LABEL_ALPHA_LC` | property · L2 · T3 | `PDF_PAGE_LABEL_ALPHA_LC` | `pub const PDF_PAGE_LABEL_ALPHA_LC: &str` | `l2-constants-pdf-page-label-alpha-lc` |
| `PDF_PAGE_LABEL_ALPHA_UC` | property · L2 · T3 | `PDF_PAGE_LABEL_ALPHA_UC` | `pub const PDF_PAGE_LABEL_ALPHA_UC: &str` | `l2-constants-pdf-page-label-alpha-uc` |
| `PDF_PAGE_LABEL_DECIMAL` | property · L2 · T3 | `PDF_PAGE_LABEL_DECIMAL` | `pub const PDF_PAGE_LABEL_DECIMAL: &str` | `l2-constants-pdf-page-label-decimal` |
| `PDF_PAGE_LABEL_NONE` | property · L2 · T3 | `PDF_PAGE_LABEL_NONE` | `pub const PDF_PAGE_LABEL_NONE: i32` | `l2-constants-pdf-page-label-none` |
| `PDF_PAGE_LABEL_ROMAN_LC` | property · L2 · T3 | `PDF_PAGE_LABEL_ROMAN_LC` | `pub const PDF_PAGE_LABEL_ROMAN_LC: &str` | `l2-constants-pdf-page-label-roman-lc` |
| `PDF_PAGE_LABEL_ROMAN_UC` | property · L2 · T3 | `PDF_PAGE_LABEL_ROMAN_UC` | `pub const PDF_PAGE_LABEL_ROMAN_UC: &str` | `l2-constants-pdf-page-label-roman-uc` |
| `PDF_SIGNATURE_DEFAULT_APPEARANCE` | property · L2 · T3 | `PDF_SIGNATURE_DEFAULT_APPEARANCE` | `pub const PDF_SIGNATURE_DEFAULT_APPEARANCE: i32` | `l2-constants-pdf-signature-default-appearance` |
| `PDF_SIGNATURE_ERROR_DIGEST_FAILURE` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_DIGEST_FAILURE` | `pub const PDF_SIGNATURE_ERROR_DIGEST_FAILURE: i32` | `l2-constants-pdf-signature-error-digest-failure` |
| `PDF_SIGNATURE_ERROR_NOT_SIGNED` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_NOT_SIGNED` | `pub const PDF_SIGNATURE_ERROR_NOT_SIGNED: i32` | `l2-constants-pdf-signature-error-not-signed` |
| `PDF_SIGNATURE_ERROR_NOT_TRUSTED` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_NOT_TRUSTED` | `pub const PDF_SIGNATURE_ERROR_NOT_TRUSTED: i32` | `l2-constants-pdf-signature-error-not-trusted` |
| `PDF_SIGNATURE_ERROR_NO_CERTIFICATE` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_NO_CERTIFICATE` | `pub const PDF_SIGNATURE_ERROR_NO_CERTIFICATE: i32` | `l2-constants-pdf-signature-error-no-certificate` |
| `PDF_SIGNATURE_ERROR_NO_SIGNATURES` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_NO_SIGNATURES` | `pub const PDF_SIGNATURE_ERROR_NO_SIGNATURES: i32` | `l2-constants-pdf-signature-error-no-signatures` |
| `PDF_SIGNATURE_ERROR_OKAY` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_OKAY` | `pub const PDF_SIGNATURE_ERROR_OKAY: i32` | `l2-constants-pdf-signature-error-okay` |
| `PDF_SIGNATURE_ERROR_SELF_SIGNED` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_SELF_SIGNED` | `pub const PDF_SIGNATURE_ERROR_SELF_SIGNED: i32` | `l2-constants-pdf-signature-error-self-signed` |
| `PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN` | `pub const PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN: i32` | `l2-constants-pdf-signature-error-self-signed-in-chain` |
| `PDF_SIGNATURE_ERROR_UNKNOWN` | property · L2 · T3 | `PDF_SIGNATURE_ERROR_UNKNOWN` | `pub const PDF_SIGNATURE_ERROR_UNKNOWN: i32` | `l2-constants-pdf-signature-error-unknown` |
| `PDF_SIGNATURE_SHOW_DATE` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_DATE` | `pub const PDF_SIGNATURE_SHOW_DATE: i32` | `l2-constants-pdf-signature-show-date` |
| `PDF_SIGNATURE_SHOW_DN` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_DN` | `pub const PDF_SIGNATURE_SHOW_DN: i32` | `l2-constants-pdf-signature-show-dn` |
| `PDF_SIGNATURE_SHOW_GRAPHIC_NAME` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_GRAPHIC_NAME` | `pub const PDF_SIGNATURE_SHOW_GRAPHIC_NAME: i32` | `l2-constants-pdf-signature-show-graphic-name` |
| `PDF_SIGNATURE_SHOW_LABELS` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_LABELS` | `pub const PDF_SIGNATURE_SHOW_LABELS: i32` | `l2-constants-pdf-signature-show-labels` |
| `PDF_SIGNATURE_SHOW_LOGO` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_LOGO` | `pub const PDF_SIGNATURE_SHOW_LOGO: i32` | `l2-constants-pdf-signature-show-logo` |
| `PDF_SIGNATURE_SHOW_TEXT_NAME` | property · L2 · T3 | `PDF_SIGNATURE_SHOW_TEXT_NAME` | `pub const PDF_SIGNATURE_SHOW_TEXT_NAME: i32` | `l2-constants-pdf-signature-show-text-name` |
| `SigFlag_AppendOnly` | property · L2 · T3 | `SigFlag_AppendOnly` | `pub const SigFlag_AppendOnly: i32` | `l2-constants-sigflag-appendonly` |
| `SigFlag_SignaturesExist` | property · L2 · T3 | `SigFlag_SignaturesExist` | `pub const SigFlag_SignaturesExist: i32` | `l2-constants-sigflag-signaturesexist` |
| `PDF_ALERT_BUTTON_CANCEL` | property · L2 · T3 | `PDF_ALERT_BUTTON_CANCEL` | `pub const PDF_ALERT_BUTTON_CANCEL: i32` | `l2-constants-pdf-alert-button-cancel` |
| `PDF_ALERT_BUTTON_GROUP_OK` | property · L2 · T3 | `PDF_ALERT_BUTTON_GROUP_OK` | `pub const PDF_ALERT_BUTTON_GROUP_OK: i32` | `l2-constants-pdf-alert-button-group-ok` |
| `PDF_ALERT_BUTTON_GROUP_OK_CANCEL` | property · L2 · T3 | `PDF_ALERT_BUTTON_GROUP_OK_CANCEL` | `pub const PDF_ALERT_BUTTON_GROUP_OK_CANCEL: i32` | `l2-constants-pdf-alert-button-group-ok-cancel` |
| `PDF_ALERT_BUTTON_GROUP_YES_NO` | property · L2 · T3 | `PDF_ALERT_BUTTON_GROUP_YES_NO` | `pub const PDF_ALERT_BUTTON_GROUP_YES_NO: i32` | `l2-constants-pdf-alert-button-group-yes-no` |
| `PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL` | property · L2 · T3 | `PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL` | `pub const PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL: i32` | `l2-constants-pdf-alert-button-group-yes-no-cancel` |
| `PDF_ALERT_BUTTON_NO` | property · L2 · T3 | `PDF_ALERT_BUTTON_NO` | `pub const PDF_ALERT_BUTTON_NO: i32` | `l2-constants-pdf-alert-button-no` |
| `PDF_ALERT_BUTTON_NONE` | property · L2 · T3 | `PDF_ALERT_BUTTON_NONE` | `pub const PDF_ALERT_BUTTON_NONE: i32` | `l2-constants-pdf-alert-button-none` |
| `PDF_ALERT_BUTTON_OK` | property · L2 · T3 | `PDF_ALERT_BUTTON_OK` | `pub const PDF_ALERT_BUTTON_OK: i32` | `l2-constants-pdf-alert-button-ok` |
| `PDF_ALERT_BUTTON_YES` | property · L2 · T3 | `PDF_ALERT_BUTTON_YES` | `pub const PDF_ALERT_BUTTON_YES: i32` | `l2-constants-pdf-alert-button-yes` |
| `PDF_ALERT_ICON_ERROR` | property · L2 · T3 | `PDF_ALERT_ICON_ERROR` | `pub const PDF_ALERT_ICON_ERROR: i32` | `l2-constants-pdf-alert-icon-error` |
| `PDF_ALERT_ICON_QUESTION` | property · L2 · T3 | `PDF_ALERT_ICON_QUESTION` | `pub const PDF_ALERT_ICON_QUESTION: i32` | `l2-constants-pdf-alert-icon-question` |
| `PDF_ALERT_ICON_STATUS` | property · L2 · T3 | `PDF_ALERT_ICON_STATUS` | `pub const PDF_ALERT_ICON_STATUS: i32` | `l2-constants-pdf-alert-icon-status` |
| `PDF_ALERT_ICON_WARNING` | property · L2 · T3 | `PDF_ALERT_ICON_WARNING` | `pub const PDF_ALERT_ICON_WARNING: i32` | `l2-constants-pdf-alert-icon-warning` |
| `PDF_DOCUMENT_EVENT_ALERT` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_ALERT` | `pub const PDF_DOCUMENT_EVENT_ALERT: i32` | `l2-constants-pdf-document-event-alert` |
| `PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM` | `pub const PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM: i32` | `l2-constants-pdf-document-event-exec-menu-item` |
| `PDF_DOCUMENT_EVENT_LAUNCH_URL` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_LAUNCH_URL` | `pub const PDF_DOCUMENT_EVENT_LAUNCH_URL: i32` | `l2-constants-pdf-document-event-launch-url` |
| `PDF_DOCUMENT_EVENT_MAIL_DOC` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_MAIL_DOC` | `pub const PDF_DOCUMENT_EVENT_MAIL_DOC: i32` | `l2-constants-pdf-document-event-mail-doc` |
| `PDF_DOCUMENT_EVENT_PRINT` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_PRINT` | `pub const PDF_DOCUMENT_EVENT_PRINT: i32` | `l2-constants-pdf-document-event-print` |
| `PDF_DOCUMENT_EVENT_SUBMIT` | property · L2 · T3 | `PDF_DOCUMENT_EVENT_SUBMIT` | `pub const PDF_DOCUMENT_EVENT_SUBMIT: i32` | `l2-constants-pdf-document-event-submit` |
| `PDF_LAYER_UI_CHECKBOX` | property · L2 · T3 | `PDF_LAYER_UI_CHECKBOX` | `pub const PDF_LAYER_UI_CHECKBOX: i32` | `l2-constants-pdf-layer-ui-checkbox` |
| `PDF_LAYER_UI_LABEL` | property · L2 · T3 | `PDF_LAYER_UI_LABEL` | `pub const PDF_LAYER_UI_LABEL: i32` | `l2-constants-pdf-layer-ui-label` |
| `PDF_LAYER_UI_RADIOBOX` | property · L2 · T3 | `PDF_LAYER_UI_RADIOBOX` | `pub const PDF_LAYER_UI_RADIOBOX: i32` | `l2-constants-pdf-layer-ui-radiobox` |
| `PDF_OC_OFF` | property · L2 · T3 | `PDF_OC_OFF` | `pub const PDF_OC_OFF: i32` | `l2-constants-pdf-oc-off` |
| `PDF_OC_ON` | property · L2 · T3 | `PDF_OC_ON` | `pub const PDF_OC_ON: i32` | `l2-constants-pdf-oc-on` |
| `PDF_OC_TOGGLE` | property · L2 · T3 | `PDF_OC_TOGGLE` | `pub const PDF_OC_TOGGLE: i32` | `l2-constants-pdf-oc-toggle` |
| `PDF_NOT_ZUGFERD` | property · L2 · T3 | `PDF_NOT_ZUGFERD` | `pub const PDF_NOT_ZUGFERD: i32` | `l2-constants-pdf-not-zugferd` |
| `PDF_ZUGFERD_BASIC` | property · L2 · T3 | `PDF_ZUGFERD_BASIC` | `pub const PDF_ZUGFERD_BASIC: i32` | `l2-constants-pdf-zugferd-basic` |
| `PDF_ZUGFERD_BASIC_WL` | property · L2 · T3 | `PDF_ZUGFERD_BASIC_WL` | `pub const PDF_ZUGFERD_BASIC_WL: i32` | `l2-constants-pdf-zugferd-basic-wl` |
| `PDF_ZUGFERD_COMFORT` | property · L2 · T3 | `PDF_ZUGFERD_COMFORT` | `pub const PDF_ZUGFERD_COMFORT: i32` | `l2-constants-pdf-zugferd-comfort` |
| `PDF_ZUGFERD_EXTENDED` | property · L2 · T3 | `PDF_ZUGFERD_EXTENDED` | `pub const PDF_ZUGFERD_EXTENDED: i32` | `l2-constants-pdf-zugferd-extended` |
| `PDF_ZUGFERD_MINIMUM` | property · L2 · T3 | `PDF_ZUGFERD_MINIMUM` | `pub const PDF_ZUGFERD_MINIMUM: i32` | `l2-constants-pdf-zugferd-minimum` |
| `PDF_ZUGFERD_UNKNOWN` | property · L2 · T3 | `PDF_ZUGFERD_UNKNOWN` | `pub const PDF_ZUGFERD_UNKNOWN: i32` | `l2-constants-pdf-zugferd-unknown` |
| `PDF_ZUGFERD_XRECHNUNG` | property · L2 · T3 | `PDF_ZUGFERD_XRECHNUNG` | `pub const PDF_ZUGFERD_XRECHNUNG: i32` | `l2-constants-pdf-zugferd-xrechnung` |
| `PDF_CLEAN_STRUCTURE_DROP` | property · L2 · T3 | `PDF_CLEAN_STRUCTURE_DROP` | `pub const PDF_CLEAN_STRUCTURE_DROP: i32` | `l2-constants-pdf-clean-structure-drop` |
| `PDF_CLEAN_STRUCTURE_KEEP` | property · L2 · T3 | `PDF_CLEAN_STRUCTURE_KEEP` | `pub const PDF_CLEAN_STRUCTURE_KEEP: i32` | `l2-constants-pdf-clean-structure-keep` |
| `PDF_CLEAN_VECTORIZE_NO` | property · L2 · T3 | `PDF_CLEAN_VECTORIZE_NO` | `pub const PDF_CLEAN_VECTORIZE_NO: i32` | `l2-constants-pdf-clean-vectorize-no` |
| `PDF_CLEAN_VECTORIZE_YES` | property · L2 · T3 | `PDF_CLEAN_VECTORIZE_YES` | `pub const PDF_CLEAN_VECTORIZE_YES: i32` | `l2-constants-pdf-clean-vectorize-yes` |
| `CS_CMYK` | property · L3 · T3 | `CS_CMYK` | `pub const CS_CMYK: i32` | `l3-constants-cs-cmyk` |
| `CS_GRAY` | property · L3 · T3 | `CS_GRAY` | `pub const CS_GRAY: i32` | `l3-constants-cs-gray` |
| `CS_RGB` | property · L3 · T3 | `CS_RGB` | `pub const CS_RGB: i32` | `l3-constants-cs-rgb` |
| `csCMYK` | property · L3 · T3 | `csCMYK` | `pub const csCMYK: Colorspace` | `l3-constants-cscmyk` |
| `csGRAY` | property · L3 · T1 | `csGRAY` | `pub const csGRAY: Colorspace` | `l2-constants-all` |
| `csRGB` | property · L3 · T1 | `csRGB` | `pub const csRGB: Colorspace` | `l2-constants-all` |
| `FZ_RECOMPRESS_FAX` | property · L3 · T3 | `FZ_RECOMPRESS_FAX` | `pub const FZ_RECOMPRESS_FAX: i32` | `l3-constants-fz-recompress-fax` |
| `FZ_RECOMPRESS_J2K` | property · L3 · T3 | `FZ_RECOMPRESS_J2K` | `pub const FZ_RECOMPRESS_J2K: i32` | `l3-constants-fz-recompress-j2k` |
| `FZ_RECOMPRESS_JPEG` | property · L3 · T3 | `FZ_RECOMPRESS_JPEG` | `pub const FZ_RECOMPRESS_JPEG: i32` | `l3-constants-fz-recompress-jpeg` |
| `FZ_RECOMPRESS_LOSSLESS` | property · L3 · T3 | `FZ_RECOMPRESS_LOSSLESS` | `pub const FZ_RECOMPRESS_LOSSLESS: i32` | `l3-constants-fz-recompress-lossless` |
| `FZ_RECOMPRESS_NEVER` | property · L3 · T3 | `FZ_RECOMPRESS_NEVER` | `pub const FZ_RECOMPRESS_NEVER: i32` | `l3-constants-fz-recompress-never` |
| `FZ_RECOMPRESS_SAME` | property · L3 · T3 | `FZ_RECOMPRESS_SAME` | `pub const FZ_RECOMPRESS_SAME: i32` | `l3-constants-fz-recompress-same` |
| `FZ_SUBSAMPLE_AVERAGE` | property · L3 · T3 | `FZ_SUBSAMPLE_AVERAGE` | `pub const FZ_SUBSAMPLE_AVERAGE: i32` | `l3-constants-fz-subsample-average` |
| `FZ_SUBSAMPLE_BICUBIC` | property · L3 · T3 | `FZ_SUBSAMPLE_BICUBIC` | `pub const FZ_SUBSAMPLE_BICUBIC: i32` | `l3-constants-fz-subsample-bicubic` |
| `PDF_ANNOT_3D` | property · L4 · T3 | `PDF_ANNOT_3D` | `pub const PDF_ANNOT_3D: i32` | `l4-constants-pdf-annot-3d` |
| `PDF_ANNOT_CARET` | property · L4 · T3 | `PDF_ANNOT_CARET` | `pub const PDF_ANNOT_CARET: i32` | `l4-constants-pdf-annot-caret` |
| `PDF_ANNOT_CIRCLE` | property · L4 · T3 | `PDF_ANNOT_CIRCLE` | `pub const PDF_ANNOT_CIRCLE: i32` | `l4-constants-pdf-annot-circle` |
| `PDF_ANNOT_FILE_ATTACHMENT` | property · L4 · T3 | `PDF_ANNOT_FILE_ATTACHMENT` | `pub const PDF_ANNOT_FILE_ATTACHMENT: i32` | `l4-constants-pdf-annot-file-attachment` |
| `PDF_ANNOT_FREE_TEXT` | property · L4 · T3 | `PDF_ANNOT_FREE_TEXT` | `pub const PDF_ANNOT_FREE_TEXT: i32` | `l4-constants-pdf-annot-free-text` |
| `PDF_ANNOT_HIGHLIGHT` | property · L4 · T3 | `PDF_ANNOT_HIGHLIGHT` | `pub const PDF_ANNOT_HIGHLIGHT: i32` | `l4-constants-pdf-annot-highlight` |
| `PDF_ANNOT_INK` | property · L4 · T3 | `PDF_ANNOT_INK` | `pub const PDF_ANNOT_INK: i32` | `l4-constants-pdf-annot-ink` |
| `PDF_ANNOT_IS_HIDDEN` | property · L4 · T3 | `PDF_ANNOT_IS_HIDDEN` | `pub const PDF_ANNOT_IS_HIDDEN: i32` | `l4-constants-pdf-annot-is-hidden` |
| `PDF_ANNOT_IS_INVISIBLE` | property · L4 · T3 | `PDF_ANNOT_IS_INVISIBLE` | `pub const PDF_ANNOT_IS_INVISIBLE: i32` | `l4-constants-pdf-annot-is-invisible` |
| `PDF_ANNOT_IS_LOCKED` | property · L4 · T3 | `PDF_ANNOT_IS_LOCKED` | `pub const PDF_ANNOT_IS_LOCKED: i32` | `l4-constants-pdf-annot-is-locked` |
| `PDF_ANNOT_IS_LOCKED_CONTENTS` | property · L4 · T3 | `PDF_ANNOT_IS_LOCKED_CONTENTS` | `pub const PDF_ANNOT_IS_LOCKED_CONTENTS: i32` | `l4-constants-pdf-annot-is-locked-contents` |
| `PDF_ANNOT_IS_NO_ROTATE` | property · L4 · T3 | `PDF_ANNOT_IS_NO_ROTATE` | `pub const PDF_ANNOT_IS_NO_ROTATE: i32` | `l4-constants-pdf-annot-is-no-rotate` |
| `PDF_ANNOT_IS_NO_VIEW` | property · L4 · T3 | `PDF_ANNOT_IS_NO_VIEW` | `pub const PDF_ANNOT_IS_NO_VIEW: i32` | `l4-constants-pdf-annot-is-no-view` |
| `PDF_ANNOT_IS_NO_ZOOM` | property · L4 · T3 | `PDF_ANNOT_IS_NO_ZOOM` | `pub const PDF_ANNOT_IS_NO_ZOOM: i32` | `l4-constants-pdf-annot-is-no-zoom` |
| `PDF_ANNOT_IS_PRINT` | property · L4 · T3 | `PDF_ANNOT_IS_PRINT` | `pub const PDF_ANNOT_IS_PRINT: i32` | `l4-constants-pdf-annot-is-print` |
| `PDF_ANNOT_IS_READ_ONLY` | property · L4 · T3 | `PDF_ANNOT_IS_READ_ONLY` | `pub const PDF_ANNOT_IS_READ_ONLY: i32` | `l4-constants-pdf-annot-is-read-only` |
| `PDF_ANNOT_IS_TOGGLE_NO_VIEW` | property · L4 · T3 | `PDF_ANNOT_IS_TOGGLE_NO_VIEW` | `pub const PDF_ANNOT_IS_TOGGLE_NO_VIEW: i32` | `l4-constants-pdf-annot-is-toggle-no-view` |
| `PDF_ANNOT_IT_DEFAULT` | property · L4 · T3 | `PDF_ANNOT_IT_DEFAULT` | `pub const PDF_ANNOT_IT_DEFAULT: i32` | `l4-constants-pdf-annot-it-default` |
| `PDF_ANNOT_IT_FREETEXT_CALLOUT` | property · L4 · T3 | `PDF_ANNOT_IT_FREETEXT_CALLOUT` | `pub const PDF_ANNOT_IT_FREETEXT_CALLOUT: i32` | `l4-constants-pdf-annot-it-freetext-callout` |
| `PDF_ANNOT_IT_FREETEXT_TYPEWRITER` | property · L4 · T3 | `PDF_ANNOT_IT_FREETEXT_TYPEWRITER` | `pub const PDF_ANNOT_IT_FREETEXT_TYPEWRITER: i32` | `l4-constants-pdf-annot-it-freetext-typewriter` |
| `PDF_ANNOT_IT_LINE_ARROW` | property · L4 · T3 | `PDF_ANNOT_IT_LINE_ARROW` | `pub const PDF_ANNOT_IT_LINE_ARROW: i32` | `l4-constants-pdf-annot-it-line-arrow` |
| `PDF_ANNOT_IT_LINE_DIMENSION` | property · L4 · T3 | `PDF_ANNOT_IT_LINE_DIMENSION` | `pub const PDF_ANNOT_IT_LINE_DIMENSION: i32` | `l4-constants-pdf-annot-it-line-dimension` |
| `PDF_ANNOT_IT_POLYGON_CLOUD` | property · L4 · T3 | `PDF_ANNOT_IT_POLYGON_CLOUD` | `pub const PDF_ANNOT_IT_POLYGON_CLOUD: i32` | `l4-constants-pdf-annot-it-polygon-cloud` |
| `PDF_ANNOT_IT_POLYGON_DIMENSION` | property · L4 · T3 | `PDF_ANNOT_IT_POLYGON_DIMENSION` | `pub const PDF_ANNOT_IT_POLYGON_DIMENSION: i32` | `l4-constants-pdf-annot-it-polygon-dimension` |
| `PDF_ANNOT_IT_POLYLINE_DIMENSION` | property · L4 · T3 | `PDF_ANNOT_IT_POLYLINE_DIMENSION` | `pub const PDF_ANNOT_IT_POLYLINE_DIMENSION: i32` | `l4-constants-pdf-annot-it-polyline-dimension` |
| `PDF_ANNOT_IT_STAMP_IMAGE` | property · L4 · T3 | `PDF_ANNOT_IT_STAMP_IMAGE` | `pub const PDF_ANNOT_IT_STAMP_IMAGE: i32` | `l4-constants-pdf-annot-it-stamp-image` |
| `PDF_ANNOT_IT_STAMP_SNAPSHOT` | property · L4 · T3 | `PDF_ANNOT_IT_STAMP_SNAPSHOT` | `pub const PDF_ANNOT_IT_STAMP_SNAPSHOT: i32` | `l4-constants-pdf-annot-it-stamp-snapshot` |
| `PDF_ANNOT_IT_UNKNOWN` | property · L4 · T3 | `PDF_ANNOT_IT_UNKNOWN` | `pub const PDF_ANNOT_IT_UNKNOWN: i32` | `l4-constants-pdf-annot-it-unknown` |
| `PDF_ANNOT_LE_BUTT` | property · L4 · T3 | `PDF_ANNOT_LE_BUTT` | `pub const PDF_ANNOT_LE_BUTT: i32` | `l4-constants-pdf-annot-le-butt` |
| `PDF_ANNOT_LE_CIRCLE` | property · L4 · T3 | `PDF_ANNOT_LE_CIRCLE` | `pub const PDF_ANNOT_LE_CIRCLE: i32` | `l4-constants-pdf-annot-le-circle` |
| `PDF_ANNOT_LE_CLOSED_ARROW` | property · L4 · T3 | `PDF_ANNOT_LE_CLOSED_ARROW` | `pub const PDF_ANNOT_LE_CLOSED_ARROW: i32` | `l4-constants-pdf-annot-le-closed-arrow` |
| `PDF_ANNOT_LE_DIAMOND` | property · L4 · T3 | `PDF_ANNOT_LE_DIAMOND` | `pub const PDF_ANNOT_LE_DIAMOND: i32` | `l4-constants-pdf-annot-le-diamond` |
| `PDF_ANNOT_LE_NONE` | property · L4 · T3 | `PDF_ANNOT_LE_NONE` | `pub const PDF_ANNOT_LE_NONE: i32` | `l4-constants-pdf-annot-le-none` |
| `PDF_ANNOT_LE_OPEN_ARROW` | property · L4 · T3 | `PDF_ANNOT_LE_OPEN_ARROW` | `pub const PDF_ANNOT_LE_OPEN_ARROW: i32` | `l4-constants-pdf-annot-le-open-arrow` |
| `PDF_ANNOT_LE_R_CLOSED_ARROW` | property · L4 · T3 | `PDF_ANNOT_LE_R_CLOSED_ARROW` | `pub const PDF_ANNOT_LE_R_CLOSED_ARROW: i32` | `l4-constants-pdf-annot-le-r-closed-arrow` |
| `PDF_ANNOT_LE_R_OPEN_ARROW` | property · L4 · T3 | `PDF_ANNOT_LE_R_OPEN_ARROW` | `pub const PDF_ANNOT_LE_R_OPEN_ARROW: i32` | `l4-constants-pdf-annot-le-r-open-arrow` |
| `PDF_ANNOT_LE_SLASH` | property · L4 · T3 | `PDF_ANNOT_LE_SLASH` | `pub const PDF_ANNOT_LE_SLASH: i32` | `l4-constants-pdf-annot-le-slash` |
| `PDF_ANNOT_LE_SQUARE` | property · L4 · T3 | `PDF_ANNOT_LE_SQUARE` | `pub const PDF_ANNOT_LE_SQUARE: i32` | `l4-constants-pdf-annot-le-square` |
| `PDF_ANNOT_LINE` | property · L4 · T3 | `PDF_ANNOT_LINE` | `pub const PDF_ANNOT_LINE: i32` | `l4-constants-pdf-annot-line` |
| `PDF_ANNOT_LINK` | property · L4 · T3 | `PDF_ANNOT_LINK` | `pub const PDF_ANNOT_LINK: i32` | `l4-constants-pdf-annot-link` |
| `PDF_ANNOT_MOVIE` | property · L4 · T3 | `PDF_ANNOT_MOVIE` | `pub const PDF_ANNOT_MOVIE: i32` | `l4-constants-pdf-annot-movie` |
| `PDF_ANNOT_POLYGON` | property · L4 · T3 | `PDF_ANNOT_POLYGON` | `pub const PDF_ANNOT_POLYGON: i32` | `l4-constants-pdf-annot-polygon` |
| `PDF_ANNOT_POLY_LINE` | property · L4 · T3 | `PDF_ANNOT_POLY_LINE` | `pub const PDF_ANNOT_POLY_LINE: i32` | `l4-constants-pdf-annot-poly-line` |
| `PDF_ANNOT_POPUP` | property · L4 · T3 | `PDF_ANNOT_POPUP` | `pub const PDF_ANNOT_POPUP: i32` | `l4-constants-pdf-annot-popup` |
| `PDF_ANNOT_PRINTER_MARK` | property · L4 · T3 | `PDF_ANNOT_PRINTER_MARK` | `pub const PDF_ANNOT_PRINTER_MARK: i32` | `l4-constants-pdf-annot-printer-mark` |
| `PDF_ANNOT_PROJECTION` | property · L4 · T3 | `PDF_ANNOT_PROJECTION` | `pub const PDF_ANNOT_PROJECTION: i32` | `l4-constants-pdf-annot-projection` |
| `PDF_ANNOT_Q_CENTER` | property · L4 · T3 | `PDF_ANNOT_Q_CENTER` | `pub const PDF_ANNOT_Q_CENTER: i32` | `l4-constants-pdf-annot-q-center` |
| `PDF_ANNOT_Q_LEFT` | property · L4 · T3 | `PDF_ANNOT_Q_LEFT` | `pub const PDF_ANNOT_Q_LEFT: i32` | `l4-constants-pdf-annot-q-left` |
| `PDF_ANNOT_Q_RIGHT` | property · L4 · T3 | `PDF_ANNOT_Q_RIGHT` | `pub const PDF_ANNOT_Q_RIGHT: i32` | `l4-constants-pdf-annot-q-right` |
| `PDF_ANNOT_REDACT` | property · L4 · T3 | `PDF_ANNOT_REDACT` | `pub const PDF_ANNOT_REDACT: i32` | `l4-constants-pdf-annot-redact` |
| `PDF_ANNOT_RICH_MEDIA` | property · L4 · T3 | `PDF_ANNOT_RICH_MEDIA` | `pub const PDF_ANNOT_RICH_MEDIA: i32` | `l4-constants-pdf-annot-rich-media` |
| `PDF_ANNOT_SCREEN` | property · L4 · T3 | `PDF_ANNOT_SCREEN` | `pub const PDF_ANNOT_SCREEN: i32` | `l4-constants-pdf-annot-screen` |
| `PDF_ANNOT_SOUND` | property · L4 · T3 | `PDF_ANNOT_SOUND` | `pub const PDF_ANNOT_SOUND: i32` | `l4-constants-pdf-annot-sound` |
| `PDF_ANNOT_SQUARE` | property · L4 · T3 | `PDF_ANNOT_SQUARE` | `pub const PDF_ANNOT_SQUARE: i32` | `l4-constants-pdf-annot-square` |
| `PDF_ANNOT_SQUIGGLY` | property · L4 · T3 | `PDF_ANNOT_SQUIGGLY` | `pub const PDF_ANNOT_SQUIGGLY: i32` | `l4-constants-pdf-annot-squiggly` |
| `PDF_ANNOT_STAMP` | property · L4 · T3 | `PDF_ANNOT_STAMP` | `pub const PDF_ANNOT_STAMP: i32` | `l4-constants-pdf-annot-stamp` |
| `PDF_ANNOT_STRIKE_OUT` | property · L4 · T3 | `PDF_ANNOT_STRIKE_OUT` | `pub const PDF_ANNOT_STRIKE_OUT: i32` | `l4-constants-pdf-annot-strike-out` |
| `PDF_ANNOT_TEXT` | property · L4 · T3 | `PDF_ANNOT_TEXT` | `pub const PDF_ANNOT_TEXT: i32` | `l4-constants-pdf-annot-text` |
| `PDF_ANNOT_TRAP_NET` | property · L4 · T3 | `PDF_ANNOT_TRAP_NET` | `pub const PDF_ANNOT_TRAP_NET: i32` | `l4-constants-pdf-annot-trap-net` |
| `PDF_ANNOT_UNDERLINE` | property · L4 · T3 | `PDF_ANNOT_UNDERLINE` | `pub const PDF_ANNOT_UNDERLINE: i32` | `l4-constants-pdf-annot-underline` |
| `PDF_ANNOT_UNKNOWN` | property · L4 · T3 | `PDF_ANNOT_UNKNOWN` | `pub const PDF_ANNOT_UNKNOWN: i32` | `l4-constants-pdf-annot-unknown` |
| `PDF_ANNOT_WATERMARK` | property · L4 · T3 | `PDF_ANNOT_WATERMARK` | `pub const PDF_ANNOT_WATERMARK: i32` | `l4-constants-pdf-annot-watermark` |
| `PDF_ANNOT_WIDGET` | property · L4 · T3 | `PDF_ANNOT_WIDGET` | `pub const PDF_ANNOT_WIDGET: i32` | `l4-constants-pdf-annot-widget` |
| `PDF_BORDER_EFFECT_CLOUDY` | property · L4 · T3 | `PDF_BORDER_EFFECT_CLOUDY` | `pub const PDF_BORDER_EFFECT_CLOUDY: i32` | `l4-constants-pdf-border-effect-cloudy` |
| `PDF_BORDER_EFFECT_NONE` | property · L4 · T3 | `PDF_BORDER_EFFECT_NONE` | `pub const PDF_BORDER_EFFECT_NONE: i32` | `l4-constants-pdf-border-effect-none` |
| `PDF_BORDER_STYLE_BEVELED` | property · L4 · T3 | `PDF_BORDER_STYLE_BEVELED` | `pub const PDF_BORDER_STYLE_BEVELED: i32` | `l4-constants-pdf-border-style-beveled` |
| `PDF_BORDER_STYLE_DASHED` | property · L4 · T3 | `PDF_BORDER_STYLE_DASHED` | `pub const PDF_BORDER_STYLE_DASHED: i32` | `l4-constants-pdf-border-style-dashed` |
| `PDF_BORDER_STYLE_INSET` | property · L4 · T3 | `PDF_BORDER_STYLE_INSET` | `pub const PDF_BORDER_STYLE_INSET: i32` | `l4-constants-pdf-border-style-inset` |
| `PDF_BORDER_STYLE_SOLID` | property · L4 · T3 | `PDF_BORDER_STYLE_SOLID` | `pub const PDF_BORDER_STYLE_SOLID: i32` | `l4-constants-pdf-border-style-solid` |
| `PDF_BORDER_STYLE_UNDERLINE` | property · L4 · T3 | `PDF_BORDER_STYLE_UNDERLINE` | `pub const PDF_BORDER_STYLE_UNDERLINE: i32` | `l4-constants-pdf-border-style-underline` |
| `PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF` | property · L4 · T3 | `PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF` | `pub const PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF: i32` | `l4-constants-pdf-btn-field-is-no-toggle-to-off` |
| `PDF_BTN_FIELD_IS_PUSHBUTTON` | property · L4 · T3 | `PDF_BTN_FIELD_IS_PUSHBUTTON` | `pub const PDF_BTN_FIELD_IS_PUSHBUTTON: i32` | `l4-constants-pdf-btn-field-is-pushbutton` |
| `PDF_BTN_FIELD_IS_RADIO` | property · L4 · T3 | `PDF_BTN_FIELD_IS_RADIO` | `pub const PDF_BTN_FIELD_IS_RADIO: i32` | `l4-constants-pdf-btn-field-is-radio` |
| `PDF_BTN_FIELD_IS_RADIOS_IN_UNISON` | property · L4 · T3 | `PDF_BTN_FIELD_IS_RADIOS_IN_UNISON` | `pub const PDF_BTN_FIELD_IS_RADIOS_IN_UNISON: i32` | `l4-constants-pdf-btn-field-is-radios-in-unison` |
| `PDF_CH_FIELD_IS_COMBO` | property · L4 · T3 | `PDF_CH_FIELD_IS_COMBO` | `pub const PDF_CH_FIELD_IS_COMBO: i32` | `l4-constants-pdf-ch-field-is-combo` |
| `PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE` | property · L4 · T3 | `PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE` | `pub const PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE: i32` | `l4-constants-pdf-ch-field-is-commit-on-sel-change` |
| `PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK` | property · L4 · T3 | `PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK` | `pub const PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK: i32` | `l4-constants-pdf-ch-field-is-do-not-spell-check` |
| `PDF_CH_FIELD_IS_EDIT` | property · L4 · T3 | `PDF_CH_FIELD_IS_EDIT` | `pub const PDF_CH_FIELD_IS_EDIT: i32` | `l4-constants-pdf-ch-field-is-edit` |
| `PDF_CH_FIELD_IS_MULTI_SELECT` | property · L4 · T3 | `PDF_CH_FIELD_IS_MULTI_SELECT` | `pub const PDF_CH_FIELD_IS_MULTI_SELECT: i32` | `l4-constants-pdf-ch-field-is-multi-select` |
| `PDF_CH_FIELD_IS_SORT` | property · L4 · T3 | `PDF_CH_FIELD_IS_SORT` | `pub const PDF_CH_FIELD_IS_SORT: i32` | `l4-constants-pdf-ch-field-is-sort` |
| `PDF_FIELD_IS_NO_EXPORT` | property · L4 · T3 | `PDF_FIELD_IS_NO_EXPORT` | `pub const PDF_FIELD_IS_NO_EXPORT: i32` | `l4-constants-pdf-field-is-no-export` |
| `PDF_FIELD_IS_READ_ONLY` | property · L4 · T3 | `PDF_FIELD_IS_READ_ONLY` | `pub const PDF_FIELD_IS_READ_ONLY: i32` | `l4-constants-pdf-field-is-read-only` |
| `PDF_FIELD_IS_REQUIRED` | property · L4 · T3 | `PDF_FIELD_IS_REQUIRED` | `pub const PDF_FIELD_IS_REQUIRED: i32` | `l4-constants-pdf-field-is-required` |
| `PDF_TX_FIELD_IS_COMB` | property · L4 · T3 | `PDF_TX_FIELD_IS_COMB` | `pub const PDF_TX_FIELD_IS_COMB: i32` | `l4-constants-pdf-tx-field-is-comb` |
| `PDF_TX_FIELD_IS_DO_NOT_SCROLL` | property · L4 · T3 | `PDF_TX_FIELD_IS_DO_NOT_SCROLL` | `pub const PDF_TX_FIELD_IS_DO_NOT_SCROLL: i32` | `l4-constants-pdf-tx-field-is-do-not-scroll` |
| `PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK` | property · L4 · T3 | `PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK` | `pub const PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK: i32` | `l4-constants-pdf-tx-field-is-do-not-spell-check` |
| `PDF_TX_FIELD_IS_FILE_SELECT` | property · L4 · T3 | `PDF_TX_FIELD_IS_FILE_SELECT` | `pub const PDF_TX_FIELD_IS_FILE_SELECT: i32` | `l4-constants-pdf-tx-field-is-file-select` |
| `PDF_TX_FIELD_IS_MULTILINE` | property · L4 · T3 | `PDF_TX_FIELD_IS_MULTILINE` | `pub const PDF_TX_FIELD_IS_MULTILINE: i32` | `l4-constants-pdf-tx-field-is-multiline` |
| `PDF_TX_FIELD_IS_PASSWORD` | property · L4 · T3 | `PDF_TX_FIELD_IS_PASSWORD` | `pub const PDF_TX_FIELD_IS_PASSWORD: i32` | `l4-constants-pdf-tx-field-is-password` |
| `PDF_TX_FIELD_IS_RICH_TEXT` | property · L4 · T3 | `PDF_TX_FIELD_IS_RICH_TEXT` | `pub const PDF_TX_FIELD_IS_RICH_TEXT: i32` | `l4-constants-pdf-tx-field-is-rich-text` |
| `PDF_WIDGET_TX_FORMAT_DATE` | property · L4 · T3 | `PDF_WIDGET_TX_FORMAT_DATE` | `pub const PDF_WIDGET_TX_FORMAT_DATE: i32` | `l4-constants-pdf-widget-tx-format-date` |
| `PDF_WIDGET_TX_FORMAT_NONE` | property · L4 · T3 | `PDF_WIDGET_TX_FORMAT_NONE` | `pub const PDF_WIDGET_TX_FORMAT_NONE: i32` | `l4-constants-pdf-widget-tx-format-none` |
| `PDF_WIDGET_TX_FORMAT_NUMBER` | property · L4 · T3 | `PDF_WIDGET_TX_FORMAT_NUMBER` | `pub const PDF_WIDGET_TX_FORMAT_NUMBER: i32` | `l4-constants-pdf-widget-tx-format-number` |
| `PDF_WIDGET_TX_FORMAT_SPECIAL` | property · L4 · T3 | `PDF_WIDGET_TX_FORMAT_SPECIAL` | `pub const PDF_WIDGET_TX_FORMAT_SPECIAL: i32` | `l4-constants-pdf-widget-tx-format-special` |
| `PDF_WIDGET_TX_FORMAT_TIME` | property · L4 · T3 | `PDF_WIDGET_TX_FORMAT_TIME` | `pub const PDF_WIDGET_TX_FORMAT_TIME: i32` | `l4-constants-pdf-widget-tx-format-time` |
| `PDF_WIDGET_TYPE_BUTTON` | property · L4 · T3 | `PDF_WIDGET_TYPE_BUTTON` | `pub const PDF_WIDGET_TYPE_BUTTON: i32` | `l4-constants-pdf-widget-type-button` |
| `PDF_WIDGET_TYPE_CHECKBOX` | property · L4 · T3 | `PDF_WIDGET_TYPE_CHECKBOX` | `pub const PDF_WIDGET_TYPE_CHECKBOX: i32` | `l4-constants-pdf-widget-type-checkbox` |
| `PDF_WIDGET_TYPE_COMBOBOX` | property · L4 · T3 | `PDF_WIDGET_TYPE_COMBOBOX` | `pub const PDF_WIDGET_TYPE_COMBOBOX: i32` | `l4-constants-pdf-widget-type-combobox` |
| `PDF_WIDGET_TYPE_LISTBOX` | property · L4 · T3 | `PDF_WIDGET_TYPE_LISTBOX` | `pub const PDF_WIDGET_TYPE_LISTBOX: i32` | `l4-constants-pdf-widget-type-listbox` |
| `PDF_WIDGET_TYPE_RADIOBUTTON` | property · L4 · T3 | `PDF_WIDGET_TYPE_RADIOBUTTON` | `pub const PDF_WIDGET_TYPE_RADIOBUTTON: i32` | `l4-constants-pdf-widget-type-radiobutton` |
| `PDF_WIDGET_TYPE_SIGNATURE` | property · L4 · T3 | `PDF_WIDGET_TYPE_SIGNATURE` | `pub const PDF_WIDGET_TYPE_SIGNATURE: i32` | `l4-constants-pdf-widget-type-signature` |
| `PDF_WIDGET_TYPE_TEXT` | property · L4 · T2 | `PDF_WIDGET_TYPE_TEXT` | `pub const PDF_WIDGET_TYPE_TEXT: i32` | `l2-constants-all` |
| `PDF_WIDGET_TYPE_UNKNOWN` | property · L4 · T3 | `PDF_WIDGET_TYPE_UNKNOWN` | `pub const PDF_WIDGET_TYPE_UNKNOWN: i32` | `l4-constants-pdf-widget-type-unknown` |
| `STAMP_Approved` | property · L4 · T3 | `STAMP_Approved` | `pub const STAMP_Approved: i32` | `l4-constants-stamp-approved` |
| `STAMP_AsIs` | property · L4 · T3 | `STAMP_AsIs` | `pub const STAMP_AsIs: i32` | `l4-constants-stamp-asis` |
| `STAMP_Confidential` | property · L4 · T3 | `STAMP_Confidential` | `pub const STAMP_Confidential: i32` | `l4-constants-stamp-confidential` |
| `STAMP_Departmental` | property · L4 · T3 | `STAMP_Departmental` | `pub const STAMP_Departmental: i32` | `l4-constants-stamp-departmental` |
| `STAMP_Draft` | property · L4 · T3 | `STAMP_Draft` | `pub const STAMP_Draft: i32` | `l4-constants-stamp-draft` |
| `STAMP_Experimental` | property · L4 · T3 | `STAMP_Experimental` | `pub const STAMP_Experimental: i32` | `l4-constants-stamp-experimental` |
| `STAMP_Expired` | property · L4 · T3 | `STAMP_Expired` | `pub const STAMP_Expired: i32` | `l4-constants-stamp-expired` |
| `STAMP_Final` | property · L4 · T3 | `STAMP_Final` | `pub const STAMP_Final: i32` | `l4-constants-stamp-final` |
| `STAMP_ForComment` | property · L4 · T3 | `STAMP_ForComment` | `pub const STAMP_ForComment: i32` | `l4-constants-stamp-forcomment` |
| `STAMP_ForPublicRelease` | property · L4 · T3 | `STAMP_ForPublicRelease` | `pub const STAMP_ForPublicRelease: i32` | `l4-constants-stamp-forpublicrelease` |
| `STAMP_NotApproved` | property · L4 · T3 | `STAMP_NotApproved` | `pub const STAMP_NotApproved: i32` | `l4-constants-stamp-notapproved` |
| `STAMP_NotForPublicRelease` | property · L4 · T3 | `STAMP_NotForPublicRelease` | `pub const STAMP_NotForPublicRelease: i32` | `l4-constants-stamp-notforpublicrelease` |
| `STAMP_Sold` | property · L4 · T3 | `STAMP_Sold` | `pub const STAMP_Sold: i32` | `l4-constants-stamp-sold` |
| `STAMP_TopSecret` | property · L4 · T3 | `STAMP_TopSecret` | `pub const STAMP_TopSecret: i32` | `l4-constants-stamp-topsecret` |
| `PDF_BM_Color` | property · L4 · T3 | `PDF_BM_Color` | `pub const PDF_BM_Color: &str` | `l4-constants-pdf-bm-color` |
| `PDF_BM_ColorBurn` | property · L4 · T3 | `PDF_BM_ColorBurn` | `pub const PDF_BM_ColorBurn: &str` | `l4-constants-pdf-bm-colorburn` |
| `PDF_BM_ColorDodge` | property · L4 · T3 | `PDF_BM_ColorDodge` | `pub const PDF_BM_ColorDodge: &str` | `l4-constants-pdf-bm-colordodge` |
| `PDF_BM_Darken` | property · L4 · T3 | `PDF_BM_Darken` | `pub const PDF_BM_Darken: &str` | `l4-constants-pdf-bm-darken` |
| `PDF_BM_Difference` | property · L4 · T3 | `PDF_BM_Difference` | `pub const PDF_BM_Difference: &str` | `l4-constants-pdf-bm-difference` |
| `PDF_BM_Exclusion` | property · L4 · T3 | `PDF_BM_Exclusion` | `pub const PDF_BM_Exclusion: &str` | `l4-constants-pdf-bm-exclusion` |
| `PDF_BM_HardLight` | property · L4 · T3 | `PDF_BM_HardLight` | `pub const PDF_BM_HardLight: &str` | `l4-constants-pdf-bm-hardlight` |
| `PDF_BM_Hue` | property · L4 · T3 | `PDF_BM_Hue` | `pub const PDF_BM_Hue: &str` | `l4-constants-pdf-bm-hue` |
| `PDF_BM_Lighten` | property · L4 · T3 | `PDF_BM_Lighten` | `pub const PDF_BM_Lighten: &str` | `l4-constants-pdf-bm-lighten` |
| `PDF_BM_Luminosity` | property · L4 · T3 | `PDF_BM_Luminosity` | `pub const PDF_BM_Luminosity: &str` | `l4-constants-pdf-bm-luminosity` |
| `PDF_BM_Multiply` | property · L4 · T3 | `PDF_BM_Multiply` | `pub const PDF_BM_Multiply: &str` | `l4-constants-pdf-bm-multiply` |
| `PDF_BM_Normal` | property · L4 · T3 | `PDF_BM_Normal` | `pub const PDF_BM_Normal: &str` | `l4-constants-pdf-bm-normal` |
| `PDF_BM_Overlay` | property · L4 · T3 | `PDF_BM_Overlay` | `pub const PDF_BM_Overlay: &str` | `l4-constants-pdf-bm-overlay` |
| `PDF_BM_Saturation` | property · L4 · T3 | `PDF_BM_Saturation` | `pub const PDF_BM_Saturation: &str` | `l4-constants-pdf-bm-saturation` |
| `PDF_BM_Screen` | property · L4 · T3 | `PDF_BM_Screen` | `pub const PDF_BM_Screen: &str` | `l4-constants-pdf-bm-screen` |
| `PDF_BM_SoftLight` | property · L4 · T3 | `PDF_BM_SoftLight` | `pub const PDF_BM_SoftLight: &str` | `l4-constants-pdf-bm-softlight` |
| `PDF_REDACT_IMAGE_NONE` | property · L4 · T2 | `PDF_REDACT_IMAGE_NONE` | `pub const PDF_REDACT_IMAGE_NONE: i32` | `l2-constants-all` |
| `PDF_REDACT_IMAGE_PIXELS` | property · L4 · T3 | `PDF_REDACT_IMAGE_PIXELS` | `pub const PDF_REDACT_IMAGE_PIXELS: i32` | `l4-constants-pdf-redact-image-pixels` |
| `PDF_REDACT_IMAGE_REMOVE` | property · L4 · T3 | `PDF_REDACT_IMAGE_REMOVE` | `pub const PDF_REDACT_IMAGE_REMOVE: i32` | `l4-constants-pdf-redact-image-remove` |
| `PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE` | property · L4 · T3 | `PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE` | `pub const PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE: i32` | `l4-constants-pdf-redact-image-remove-unless-invisible` |
| `PDF_REDACT_LINE_ART_NONE` | property · L4 · T2 | `PDF_REDACT_LINE_ART_NONE` | `pub const PDF_REDACT_LINE_ART_NONE: i32` | `l2-constants-all` |
| `PDF_REDACT_LINE_ART_REMOVE_IF_COVERED` | property · L4 · T3 | `PDF_REDACT_LINE_ART_REMOVE_IF_COVERED` | `pub const PDF_REDACT_LINE_ART_REMOVE_IF_COVERED: i32` | `l4-constants-pdf-redact-line-art-remove-if-covered` |
| `PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED` | property · L4 · T3 | `PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED` | `pub const PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED: i32` | `l4-constants-pdf-redact-line-art-remove-if-touched` |
| `PDF_REDACT_TEXT_NONE` | property · L4 · T3 | `PDF_REDACT_TEXT_NONE` | `pub const PDF_REDACT_TEXT_NONE: i32` | `l4-constants-pdf-redact-text-none` |
| `PDF_REDACT_TEXT_REMOVE` | property · L4 · T3 | `PDF_REDACT_TEXT_REMOVE` | `pub const PDF_REDACT_TEXT_REMOVE: i32` | `l4-constants-pdf-redact-text-remove` |
| `PDF_REDACT_TEXT_REMOVE_INVISIBLE` | property · L4 · T3 | `PDF_REDACT_TEXT_REMOVE_INVISIBLE` | `pub const PDF_REDACT_TEXT_REMOVE_INVISIBLE: i32` | `l4-constants-pdf-redact-text-remove-invisible` |
| `UCDN_SCRIPT_ADLAM` | property · L4 · T3 | `UCDN_SCRIPT_ADLAM` | `pub const UCDN_SCRIPT_ADLAM: i32` | `l4-constants-ucdn-script-adlam` |
| `UCDN_SCRIPT_AHOM` | property · L4 · T3 | `UCDN_SCRIPT_AHOM` | `pub const UCDN_SCRIPT_AHOM: i32` | `l4-constants-ucdn-script-ahom` |
| `UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS` | property · L4 · T3 | `UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS` | `pub const UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS: i32` | `l4-constants-ucdn-script-anatolian-hieroglyphs` |
| `UCDN_SCRIPT_ARABIC` | property · L4 · T3 | `UCDN_SCRIPT_ARABIC` | `pub const UCDN_SCRIPT_ARABIC: i32` | `l4-constants-ucdn-script-arabic` |
| `UCDN_SCRIPT_ARMENIAN` | property · L4 · T3 | `UCDN_SCRIPT_ARMENIAN` | `pub const UCDN_SCRIPT_ARMENIAN: i32` | `l4-constants-ucdn-script-armenian` |
| `UCDN_SCRIPT_AVESTAN` | property · L4 · T3 | `UCDN_SCRIPT_AVESTAN` | `pub const UCDN_SCRIPT_AVESTAN: i32` | `l4-constants-ucdn-script-avestan` |
| `UCDN_SCRIPT_BALINESE` | property · L4 · T3 | `UCDN_SCRIPT_BALINESE` | `pub const UCDN_SCRIPT_BALINESE: i32` | `l4-constants-ucdn-script-balinese` |
| `UCDN_SCRIPT_BAMUM` | property · L4 · T3 | `UCDN_SCRIPT_BAMUM` | `pub const UCDN_SCRIPT_BAMUM: i32` | `l4-constants-ucdn-script-bamum` |
| `UCDN_SCRIPT_BASSA_VAH` | property · L4 · T3 | `UCDN_SCRIPT_BASSA_VAH` | `pub const UCDN_SCRIPT_BASSA_VAH: i32` | `l4-constants-ucdn-script-bassa-vah` |
| `UCDN_SCRIPT_BATAK` | property · L4 · T3 | `UCDN_SCRIPT_BATAK` | `pub const UCDN_SCRIPT_BATAK: i32` | `l4-constants-ucdn-script-batak` |
| `UCDN_SCRIPT_BENGALI` | property · L4 · T3 | `UCDN_SCRIPT_BENGALI` | `pub const UCDN_SCRIPT_BENGALI: i32` | `l4-constants-ucdn-script-bengali` |
| `UCDN_SCRIPT_BHAIKSUKI` | property · L4 · T3 | `UCDN_SCRIPT_BHAIKSUKI` | `pub const UCDN_SCRIPT_BHAIKSUKI: i32` | `l4-constants-ucdn-script-bhaiksuki` |
| `UCDN_SCRIPT_BOPOMOFO` | property · L4 · T3 | `UCDN_SCRIPT_BOPOMOFO` | `pub const UCDN_SCRIPT_BOPOMOFO: i32` | `l4-constants-ucdn-script-bopomofo` |
| `UCDN_SCRIPT_BRAHMI` | property · L4 · T3 | `UCDN_SCRIPT_BRAHMI` | `pub const UCDN_SCRIPT_BRAHMI: i32` | `l4-constants-ucdn-script-brahmi` |
| `UCDN_SCRIPT_BRAILLE` | property · L4 · T3 | `UCDN_SCRIPT_BRAILLE` | `pub const UCDN_SCRIPT_BRAILLE: i32` | `l4-constants-ucdn-script-braille` |
| `UCDN_SCRIPT_BUGINESE` | property · L4 · T3 | `UCDN_SCRIPT_BUGINESE` | `pub const UCDN_SCRIPT_BUGINESE: i32` | `l4-constants-ucdn-script-buginese` |
| `UCDN_SCRIPT_BUHID` | property · L4 · T3 | `UCDN_SCRIPT_BUHID` | `pub const UCDN_SCRIPT_BUHID: i32` | `l4-constants-ucdn-script-buhid` |
| `UCDN_SCRIPT_CANADIAN_ABORIGINAL` | property · L4 · T3 | `UCDN_SCRIPT_CANADIAN_ABORIGINAL` | `pub const UCDN_SCRIPT_CANADIAN_ABORIGINAL: i32` | `l4-constants-ucdn-script-canadian-aboriginal` |
| `UCDN_SCRIPT_CARIAN` | property · L4 · T3 | `UCDN_SCRIPT_CARIAN` | `pub const UCDN_SCRIPT_CARIAN: i32` | `l4-constants-ucdn-script-carian` |
| `UCDN_SCRIPT_CAUCASIAN_ALBANIAN` | property · L4 · T3 | `UCDN_SCRIPT_CAUCASIAN_ALBANIAN` | `pub const UCDN_SCRIPT_CAUCASIAN_ALBANIAN: i32` | `l4-constants-ucdn-script-caucasian-albanian` |
| `UCDN_SCRIPT_CHAKMA` | property · L4 · T3 | `UCDN_SCRIPT_CHAKMA` | `pub const UCDN_SCRIPT_CHAKMA: i32` | `l4-constants-ucdn-script-chakma` |
| `UCDN_SCRIPT_CHAM` | property · L4 · T3 | `UCDN_SCRIPT_CHAM` | `pub const UCDN_SCRIPT_CHAM: i32` | `l4-constants-ucdn-script-cham` |
| `UCDN_SCRIPT_CHEROKEE` | property · L4 · T3 | `UCDN_SCRIPT_CHEROKEE` | `pub const UCDN_SCRIPT_CHEROKEE: i32` | `l4-constants-ucdn-script-cherokee` |
| `UCDN_SCRIPT_CHORASMIAN` | property · L4 · T3 | `UCDN_SCRIPT_CHORASMIAN` | `pub const UCDN_SCRIPT_CHORASMIAN: i32` | `l4-constants-ucdn-script-chorasmian` |
| `UCDN_SCRIPT_COMMON` | property · L4 · T3 | `UCDN_SCRIPT_COMMON` | `pub const UCDN_SCRIPT_COMMON: i32` | `l4-constants-ucdn-script-common` |
| `UCDN_SCRIPT_COPTIC` | property · L4 · T3 | `UCDN_SCRIPT_COPTIC` | `pub const UCDN_SCRIPT_COPTIC: i32` | `l4-constants-ucdn-script-coptic` |
| `UCDN_SCRIPT_CUNEIFORM` | property · L4 · T3 | `UCDN_SCRIPT_CUNEIFORM` | `pub const UCDN_SCRIPT_CUNEIFORM: i32` | `l4-constants-ucdn-script-cuneiform` |
| `UCDN_SCRIPT_CYPRIOT` | property · L4 · T3 | `UCDN_SCRIPT_CYPRIOT` | `pub const UCDN_SCRIPT_CYPRIOT: i32` | `l4-constants-ucdn-script-cypriot` |
| `UCDN_SCRIPT_CYPRO_MINOAN` | property · L4 · T3 | `UCDN_SCRIPT_CYPRO_MINOAN` | `pub const UCDN_SCRIPT_CYPRO_MINOAN: i32` | `l4-constants-ucdn-script-cypro-minoan` |
| `UCDN_SCRIPT_CYRILLIC` | property · L4 · T3 | `UCDN_SCRIPT_CYRILLIC` | `pub const UCDN_SCRIPT_CYRILLIC: i32` | `l4-constants-ucdn-script-cyrillic` |
| `UCDN_SCRIPT_DESERET` | property · L4 · T3 | `UCDN_SCRIPT_DESERET` | `pub const UCDN_SCRIPT_DESERET: i32` | `l4-constants-ucdn-script-deseret` |
| `UCDN_SCRIPT_DEVANAGARI` | property · L4 · T3 | `UCDN_SCRIPT_DEVANAGARI` | `pub const UCDN_SCRIPT_DEVANAGARI: i32` | `l4-constants-ucdn-script-devanagari` |
| `UCDN_SCRIPT_DIVES_AKURU` | property · L4 · T3 | `UCDN_SCRIPT_DIVES_AKURU` | `pub const UCDN_SCRIPT_DIVES_AKURU: i32` | `l4-constants-ucdn-script-dives-akuru` |
| `UCDN_SCRIPT_DOGRA` | property · L4 · T3 | `UCDN_SCRIPT_DOGRA` | `pub const UCDN_SCRIPT_DOGRA: i32` | `l4-constants-ucdn-script-dogra` |
| `UCDN_SCRIPT_DUPLOYAN` | property · L4 · T3 | `UCDN_SCRIPT_DUPLOYAN` | `pub const UCDN_SCRIPT_DUPLOYAN: i32` | `l4-constants-ucdn-script-duployan` |
| `UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS` | property · L4 · T3 | `UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS` | `pub const UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS: i32` | `l4-constants-ucdn-script-egyptian-hieroglyphs` |
| `UCDN_SCRIPT_ELBASAN` | property · L4 · T3 | `UCDN_SCRIPT_ELBASAN` | `pub const UCDN_SCRIPT_ELBASAN: i32` | `l4-constants-ucdn-script-elbasan` |
| `UCDN_SCRIPT_ELYMAIC` | property · L4 · T3 | `UCDN_SCRIPT_ELYMAIC` | `pub const UCDN_SCRIPT_ELYMAIC: i32` | `l4-constants-ucdn-script-elymaic` |
| `UCDN_SCRIPT_ETHIOPIC` | property · L4 · T3 | `UCDN_SCRIPT_ETHIOPIC` | `pub const UCDN_SCRIPT_ETHIOPIC: i32` | `l4-constants-ucdn-script-ethiopic` |
| `UCDN_SCRIPT_GARAY` | property · L4 · T3 | `UCDN_SCRIPT_GARAY` | `pub const UCDN_SCRIPT_GARAY: i32` | `l4-constants-ucdn-script-garay` |
| `UCDN_SCRIPT_GEORGIAN` | property · L4 · T3 | `UCDN_SCRIPT_GEORGIAN` | `pub const UCDN_SCRIPT_GEORGIAN: i32` | `l4-constants-ucdn-script-georgian` |
| `UCDN_SCRIPT_GLAGOLITIC` | property · L4 · T3 | `UCDN_SCRIPT_GLAGOLITIC` | `pub const UCDN_SCRIPT_GLAGOLITIC: i32` | `l4-constants-ucdn-script-glagolitic` |
| `UCDN_SCRIPT_GOTHIC` | property · L4 · T3 | `UCDN_SCRIPT_GOTHIC` | `pub const UCDN_SCRIPT_GOTHIC: i32` | `l4-constants-ucdn-script-gothic` |
| `UCDN_SCRIPT_GRANTHA` | property · L4 · T3 | `UCDN_SCRIPT_GRANTHA` | `pub const UCDN_SCRIPT_GRANTHA: i32` | `l4-constants-ucdn-script-grantha` |
| `UCDN_SCRIPT_GREEK` | property · L4 · T3 | `UCDN_SCRIPT_GREEK` | `pub const UCDN_SCRIPT_GREEK: i32` | `l4-constants-ucdn-script-greek` |
| `UCDN_SCRIPT_GUJARATI` | property · L4 · T3 | `UCDN_SCRIPT_GUJARATI` | `pub const UCDN_SCRIPT_GUJARATI: i32` | `l4-constants-ucdn-script-gujarati` |
| `UCDN_SCRIPT_GUNJALA_GONDI` | property · L4 · T3 | `UCDN_SCRIPT_GUNJALA_GONDI` | `pub const UCDN_SCRIPT_GUNJALA_GONDI: i32` | `l4-constants-ucdn-script-gunjala-gondi` |
| `UCDN_SCRIPT_GURMUKHI` | property · L4 · T3 | `UCDN_SCRIPT_GURMUKHI` | `pub const UCDN_SCRIPT_GURMUKHI: i32` | `l4-constants-ucdn-script-gurmukhi` |
| `UCDN_SCRIPT_GURUNG_KHEMA` | property · L4 · T3 | `UCDN_SCRIPT_GURUNG_KHEMA` | `pub const UCDN_SCRIPT_GURUNG_KHEMA: i32` | `l4-constants-ucdn-script-gurung-khema` |
| `UCDN_SCRIPT_HAN` | property · L4 · T3 | `UCDN_SCRIPT_HAN` | `pub const UCDN_SCRIPT_HAN: i32` | `l4-constants-ucdn-script-han` |
| `UCDN_SCRIPT_HANGUL` | property · L4 · T3 | `UCDN_SCRIPT_HANGUL` | `pub const UCDN_SCRIPT_HANGUL: i32` | `l4-constants-ucdn-script-hangul` |
| `UCDN_SCRIPT_HANIFI_ROHINGYA` | property · L4 · T3 | `UCDN_SCRIPT_HANIFI_ROHINGYA` | `pub const UCDN_SCRIPT_HANIFI_ROHINGYA: i32` | `l4-constants-ucdn-script-hanifi-rohingya` |
| `UCDN_SCRIPT_HANUNOO` | property · L4 · T3 | `UCDN_SCRIPT_HANUNOO` | `pub const UCDN_SCRIPT_HANUNOO: i32` | `l4-constants-ucdn-script-hanunoo` |
| `UCDN_SCRIPT_HATRAN` | property · L4 · T3 | `UCDN_SCRIPT_HATRAN` | `pub const UCDN_SCRIPT_HATRAN: i32` | `l4-constants-ucdn-script-hatran` |
| `UCDN_SCRIPT_HEBREW` | property · L4 · T3 | `UCDN_SCRIPT_HEBREW` | `pub const UCDN_SCRIPT_HEBREW: i32` | `l4-constants-ucdn-script-hebrew` |
| `UCDN_SCRIPT_HIRAGANA` | property · L4 · T3 | `UCDN_SCRIPT_HIRAGANA` | `pub const UCDN_SCRIPT_HIRAGANA: i32` | `l4-constants-ucdn-script-hiragana` |
| `UCDN_SCRIPT_IMPERIAL_ARAMAIC` | property · L4 · T3 | `UCDN_SCRIPT_IMPERIAL_ARAMAIC` | `pub const UCDN_SCRIPT_IMPERIAL_ARAMAIC: i32` | `l4-constants-ucdn-script-imperial-aramaic` |
| `UCDN_SCRIPT_INHERITED` | property · L4 · T3 | `UCDN_SCRIPT_INHERITED` | `pub const UCDN_SCRIPT_INHERITED: i32` | `l4-constants-ucdn-script-inherited` |
| `UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI` | property · L4 · T3 | `UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI` | `pub const UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI: i32` | `l4-constants-ucdn-script-inscriptional-pahlavi` |
| `UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN` | property · L4 · T3 | `UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN` | `pub const UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN: i32` | `l4-constants-ucdn-script-inscriptional-parthian` |
| `UCDN_SCRIPT_JAVANESE` | property · L4 · T3 | `UCDN_SCRIPT_JAVANESE` | `pub const UCDN_SCRIPT_JAVANESE: i32` | `l4-constants-ucdn-script-javanese` |
| `UCDN_SCRIPT_KAITHI` | property · L4 · T3 | `UCDN_SCRIPT_KAITHI` | `pub const UCDN_SCRIPT_KAITHI: i32` | `l4-constants-ucdn-script-kaithi` |
| `UCDN_SCRIPT_KANNADA` | property · L4 · T3 | `UCDN_SCRIPT_KANNADA` | `pub const UCDN_SCRIPT_KANNADA: i32` | `l4-constants-ucdn-script-kannada` |
| `UCDN_SCRIPT_KATAKANA` | property · L4 · T3 | `UCDN_SCRIPT_KATAKANA` | `pub const UCDN_SCRIPT_KATAKANA: i32` | `l4-constants-ucdn-script-katakana` |
| `UCDN_SCRIPT_KAWI` | property · L4 · T3 | `UCDN_SCRIPT_KAWI` | `pub const UCDN_SCRIPT_KAWI: i32` | `l4-constants-ucdn-script-kawi` |
| `UCDN_SCRIPT_KAYAH_LI` | property · L4 · T3 | `UCDN_SCRIPT_KAYAH_LI` | `pub const UCDN_SCRIPT_KAYAH_LI: i32` | `l4-constants-ucdn-script-kayah-li` |
| `UCDN_SCRIPT_KHAROSHTHI` | property · L4 · T3 | `UCDN_SCRIPT_KHAROSHTHI` | `pub const UCDN_SCRIPT_KHAROSHTHI: i32` | `l4-constants-ucdn-script-kharoshthi` |
| `UCDN_SCRIPT_KHITAN_SMALL_SCRIPT` | property · L4 · T3 | `UCDN_SCRIPT_KHITAN_SMALL_SCRIPT` | `pub const UCDN_SCRIPT_KHITAN_SMALL_SCRIPT: i32` | `l4-constants-ucdn-script-khitan-small-script` |
| `UCDN_SCRIPT_KHMER` | property · L4 · T3 | `UCDN_SCRIPT_KHMER` | `pub const UCDN_SCRIPT_KHMER: i32` | `l4-constants-ucdn-script-khmer` |
| `UCDN_SCRIPT_KHOJKI` | property · L4 · T3 | `UCDN_SCRIPT_KHOJKI` | `pub const UCDN_SCRIPT_KHOJKI: i32` | `l4-constants-ucdn-script-khojki` |
| `UCDN_SCRIPT_KHUDAWADI` | property · L4 · T3 | `UCDN_SCRIPT_KHUDAWADI` | `pub const UCDN_SCRIPT_KHUDAWADI: i32` | `l4-constants-ucdn-script-khudawadi` |
| `UCDN_SCRIPT_KIRAT_RAI` | property · L4 · T3 | `UCDN_SCRIPT_KIRAT_RAI` | `pub const UCDN_SCRIPT_KIRAT_RAI: i32` | `l4-constants-ucdn-script-kirat-rai` |
| `UCDN_SCRIPT_LAO` | property · L4 · T3 | `UCDN_SCRIPT_LAO` | `pub const UCDN_SCRIPT_LAO: i32` | `l4-constants-ucdn-script-lao` |
| `UCDN_SCRIPT_LATIN` | property · L4 · T3 | `UCDN_SCRIPT_LATIN` | `pub const UCDN_SCRIPT_LATIN: i32` | `l4-constants-ucdn-script-latin` |
| `UCDN_SCRIPT_LEPCHA` | property · L4 · T3 | `UCDN_SCRIPT_LEPCHA` | `pub const UCDN_SCRIPT_LEPCHA: i32` | `l4-constants-ucdn-script-lepcha` |
| `UCDN_SCRIPT_LIMBU` | property · L4 · T3 | `UCDN_SCRIPT_LIMBU` | `pub const UCDN_SCRIPT_LIMBU: i32` | `l4-constants-ucdn-script-limbu` |
| `UCDN_SCRIPT_LINEAR_A` | property · L4 · T3 | `UCDN_SCRIPT_LINEAR_A` | `pub const UCDN_SCRIPT_LINEAR_A: i32` | `l4-constants-ucdn-script-linear-a` |
| `UCDN_SCRIPT_LINEAR_B` | property · L4 · T3 | `UCDN_SCRIPT_LINEAR_B` | `pub const UCDN_SCRIPT_LINEAR_B: i32` | `l4-constants-ucdn-script-linear-b` |
| `UCDN_SCRIPT_LISU` | property · L4 · T3 | `UCDN_SCRIPT_LISU` | `pub const UCDN_SCRIPT_LISU: i32` | `l4-constants-ucdn-script-lisu` |
| `UCDN_SCRIPT_LYCIAN` | property · L4 · T3 | `UCDN_SCRIPT_LYCIAN` | `pub const UCDN_SCRIPT_LYCIAN: i32` | `l4-constants-ucdn-script-lycian` |
| `UCDN_SCRIPT_LYDIAN` | property · L4 · T3 | `UCDN_SCRIPT_LYDIAN` | `pub const UCDN_SCRIPT_LYDIAN: i32` | `l4-constants-ucdn-script-lydian` |
| `UCDN_SCRIPT_MAHAJANI` | property · L4 · T3 | `UCDN_SCRIPT_MAHAJANI` | `pub const UCDN_SCRIPT_MAHAJANI: i32` | `l4-constants-ucdn-script-mahajani` |
| `UCDN_SCRIPT_MAKASAR` | property · L4 · T3 | `UCDN_SCRIPT_MAKASAR` | `pub const UCDN_SCRIPT_MAKASAR: i32` | `l4-constants-ucdn-script-makasar` |
| `UCDN_SCRIPT_MALAYALAM` | property · L4 · T3 | `UCDN_SCRIPT_MALAYALAM` | `pub const UCDN_SCRIPT_MALAYALAM: i32` | `l4-constants-ucdn-script-malayalam` |
| `UCDN_SCRIPT_MANDAIC` | property · L4 · T3 | `UCDN_SCRIPT_MANDAIC` | `pub const UCDN_SCRIPT_MANDAIC: i32` | `l4-constants-ucdn-script-mandaic` |
| `UCDN_SCRIPT_MANICHAEAN` | property · L4 · T3 | `UCDN_SCRIPT_MANICHAEAN` | `pub const UCDN_SCRIPT_MANICHAEAN: i32` | `l4-constants-ucdn-script-manichaean` |
| `UCDN_SCRIPT_MARCHEN` | property · L4 · T3 | `UCDN_SCRIPT_MARCHEN` | `pub const UCDN_SCRIPT_MARCHEN: i32` | `l4-constants-ucdn-script-marchen` |
| `UCDN_SCRIPT_MASARAM_GONDI` | property · L4 · T3 | `UCDN_SCRIPT_MASARAM_GONDI` | `pub const UCDN_SCRIPT_MASARAM_GONDI: i32` | `l4-constants-ucdn-script-masaram-gondi` |
| `UCDN_SCRIPT_MEDEFAIDRIN` | property · L4 · T3 | `UCDN_SCRIPT_MEDEFAIDRIN` | `pub const UCDN_SCRIPT_MEDEFAIDRIN: i32` | `l4-constants-ucdn-script-medefaidrin` |
| `UCDN_SCRIPT_MEETEI_MAYEK` | property · L4 · T3 | `UCDN_SCRIPT_MEETEI_MAYEK` | `pub const UCDN_SCRIPT_MEETEI_MAYEK: i32` | `l4-constants-ucdn-script-meetei-mayek` |
| `UCDN_SCRIPT_MENDE_KIKAKUI` | property · L4 · T3 | `UCDN_SCRIPT_MENDE_KIKAKUI` | `pub const UCDN_SCRIPT_MENDE_KIKAKUI: i32` | `l4-constants-ucdn-script-mende-kikakui` |
| `UCDN_SCRIPT_MEROITIC_CURSIVE` | property · L4 · T3 | `UCDN_SCRIPT_MEROITIC_CURSIVE` | `pub const UCDN_SCRIPT_MEROITIC_CURSIVE: i32` | `l4-constants-ucdn-script-meroitic-cursive` |
| `UCDN_SCRIPT_MEROITIC_HIEROGLYPHS` | property · L4 · T3 | `UCDN_SCRIPT_MEROITIC_HIEROGLYPHS` | `pub const UCDN_SCRIPT_MEROITIC_HIEROGLYPHS: i32` | `l4-constants-ucdn-script-meroitic-hieroglyphs` |
| `UCDN_SCRIPT_MIAO` | property · L4 · T3 | `UCDN_SCRIPT_MIAO` | `pub const UCDN_SCRIPT_MIAO: i32` | `l4-constants-ucdn-script-miao` |
| `UCDN_SCRIPT_MODI` | property · L4 · T3 | `UCDN_SCRIPT_MODI` | `pub const UCDN_SCRIPT_MODI: i32` | `l4-constants-ucdn-script-modi` |
| `UCDN_SCRIPT_MONGOLIAN` | property · L4 · T3 | `UCDN_SCRIPT_MONGOLIAN` | `pub const UCDN_SCRIPT_MONGOLIAN: i32` | `l4-constants-ucdn-script-mongolian` |
| `UCDN_SCRIPT_MRO` | property · L4 · T3 | `UCDN_SCRIPT_MRO` | `pub const UCDN_SCRIPT_MRO: i32` | `l4-constants-ucdn-script-mro` |
| `UCDN_SCRIPT_MULTANI` | property · L4 · T3 | `UCDN_SCRIPT_MULTANI` | `pub const UCDN_SCRIPT_MULTANI: i32` | `l4-constants-ucdn-script-multani` |
| `UCDN_SCRIPT_MYANMAR` | property · L4 · T3 | `UCDN_SCRIPT_MYANMAR` | `pub const UCDN_SCRIPT_MYANMAR: i32` | `l4-constants-ucdn-script-myanmar` |
| `UCDN_SCRIPT_NABATAEAN` | property · L4 · T3 | `UCDN_SCRIPT_NABATAEAN` | `pub const UCDN_SCRIPT_NABATAEAN: i32` | `l4-constants-ucdn-script-nabataean` |
| `UCDN_SCRIPT_NAG_MUNDARI` | property · L4 · T3 | `UCDN_SCRIPT_NAG_MUNDARI` | `pub const UCDN_SCRIPT_NAG_MUNDARI: i32` | `l4-constants-ucdn-script-nag-mundari` |
| `UCDN_SCRIPT_NANDINAGARI` | property · L4 · T3 | `UCDN_SCRIPT_NANDINAGARI` | `pub const UCDN_SCRIPT_NANDINAGARI: i32` | `l4-constants-ucdn-script-nandinagari` |
| `UCDN_SCRIPT_NEWA` | property · L4 · T3 | `UCDN_SCRIPT_NEWA` | `pub const UCDN_SCRIPT_NEWA: i32` | `l4-constants-ucdn-script-newa` |
| `UCDN_SCRIPT_NEW_TAI_LUE` | property · L4 · T3 | `UCDN_SCRIPT_NEW_TAI_LUE` | `pub const UCDN_SCRIPT_NEW_TAI_LUE: i32` | `l4-constants-ucdn-script-new-tai-lue` |
| `UCDN_SCRIPT_NKO` | property · L4 · T3 | `UCDN_SCRIPT_NKO` | `pub const UCDN_SCRIPT_NKO: i32` | `l4-constants-ucdn-script-nko` |
| `UCDN_SCRIPT_NUSHU` | property · L4 · T3 | `UCDN_SCRIPT_NUSHU` | `pub const UCDN_SCRIPT_NUSHU: i32` | `l4-constants-ucdn-script-nushu` |
| `UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG` | property · L4 · T3 | `UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG` | `pub const UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG: i32` | `l4-constants-ucdn-script-nyiakeng-puachue-hmong` |
| `UCDN_SCRIPT_OGHAM` | property · L4 · T3 | `UCDN_SCRIPT_OGHAM` | `pub const UCDN_SCRIPT_OGHAM: i32` | `l4-constants-ucdn-script-ogham` |
| `UCDN_SCRIPT_OLD_HUNGARIAN` | property · L4 · T3 | `UCDN_SCRIPT_OLD_HUNGARIAN` | `pub const UCDN_SCRIPT_OLD_HUNGARIAN: i32` | `l4-constants-ucdn-script-old-hungarian` |
| `UCDN_SCRIPT_OLD_ITALIC` | property · L4 · T3 | `UCDN_SCRIPT_OLD_ITALIC` | `pub const UCDN_SCRIPT_OLD_ITALIC: i32` | `l4-constants-ucdn-script-old-italic` |
| `UCDN_SCRIPT_OLD_NORTH_ARABIAN` | property · L4 · T3 | `UCDN_SCRIPT_OLD_NORTH_ARABIAN` | `pub const UCDN_SCRIPT_OLD_NORTH_ARABIAN: i32` | `l4-constants-ucdn-script-old-north-arabian` |
| `UCDN_SCRIPT_OLD_PERMIC` | property · L4 · T3 | `UCDN_SCRIPT_OLD_PERMIC` | `pub const UCDN_SCRIPT_OLD_PERMIC: i32` | `l4-constants-ucdn-script-old-permic` |
| `UCDN_SCRIPT_OLD_PERSIAN` | property · L4 · T3 | `UCDN_SCRIPT_OLD_PERSIAN` | `pub const UCDN_SCRIPT_OLD_PERSIAN: i32` | `l4-constants-ucdn-script-old-persian` |
| `UCDN_SCRIPT_OLD_SOGDIAN` | property · L4 · T3 | `UCDN_SCRIPT_OLD_SOGDIAN` | `pub const UCDN_SCRIPT_OLD_SOGDIAN: i32` | `l4-constants-ucdn-script-old-sogdian` |
| `UCDN_SCRIPT_OLD_SOUTH_ARABIAN` | property · L4 · T3 | `UCDN_SCRIPT_OLD_SOUTH_ARABIAN` | `pub const UCDN_SCRIPT_OLD_SOUTH_ARABIAN: i32` | `l4-constants-ucdn-script-old-south-arabian` |
| `UCDN_SCRIPT_OLD_TURKIC` | property · L4 · T3 | `UCDN_SCRIPT_OLD_TURKIC` | `pub const UCDN_SCRIPT_OLD_TURKIC: i32` | `l4-constants-ucdn-script-old-turkic` |
| `UCDN_SCRIPT_OLD_UYGHUR` | property · L4 · T3 | `UCDN_SCRIPT_OLD_UYGHUR` | `pub const UCDN_SCRIPT_OLD_UYGHUR: i32` | `l4-constants-ucdn-script-old-uyghur` |
| `UCDN_SCRIPT_OL_CHIKI` | property · L4 · T3 | `UCDN_SCRIPT_OL_CHIKI` | `pub const UCDN_SCRIPT_OL_CHIKI: i32` | `l4-constants-ucdn-script-ol-chiki` |
| `UCDN_SCRIPT_OL_ONAL` | property · L4 · T3 | `UCDN_SCRIPT_OL_ONAL` | `pub const UCDN_SCRIPT_OL_ONAL: i32` | `l4-constants-ucdn-script-ol-onal` |
| `UCDN_SCRIPT_ORIYA` | property · L4 · T3 | `UCDN_SCRIPT_ORIYA` | `pub const UCDN_SCRIPT_ORIYA: i32` | `l4-constants-ucdn-script-oriya` |
| `UCDN_SCRIPT_OSAGE` | property · L4 · T3 | `UCDN_SCRIPT_OSAGE` | `pub const UCDN_SCRIPT_OSAGE: i32` | `l4-constants-ucdn-script-osage` |
| `UCDN_SCRIPT_OSMANYA` | property · L4 · T3 | `UCDN_SCRIPT_OSMANYA` | `pub const UCDN_SCRIPT_OSMANYA: i32` | `l4-constants-ucdn-script-osmanya` |
| `UCDN_SCRIPT_PAHAWH_HMONG` | property · L4 · T3 | `UCDN_SCRIPT_PAHAWH_HMONG` | `pub const UCDN_SCRIPT_PAHAWH_HMONG: i32` | `l4-constants-ucdn-script-pahawh-hmong` |
| `UCDN_SCRIPT_PALMYRENE` | property · L4 · T3 | `UCDN_SCRIPT_PALMYRENE` | `pub const UCDN_SCRIPT_PALMYRENE: i32` | `l4-constants-ucdn-script-palmyrene` |
| `UCDN_SCRIPT_PAU_CIN_HAU` | property · L4 · T3 | `UCDN_SCRIPT_PAU_CIN_HAU` | `pub const UCDN_SCRIPT_PAU_CIN_HAU: i32` | `l4-constants-ucdn-script-pau-cin-hau` |
| `UCDN_SCRIPT_PHAGS_PA` | property · L4 · T3 | `UCDN_SCRIPT_PHAGS_PA` | `pub const UCDN_SCRIPT_PHAGS_PA: i32` | `l4-constants-ucdn-script-phags-pa` |
| `UCDN_SCRIPT_PHOENICIAN` | property · L4 · T3 | `UCDN_SCRIPT_PHOENICIAN` | `pub const UCDN_SCRIPT_PHOENICIAN: i32` | `l4-constants-ucdn-script-phoenician` |
| `UCDN_SCRIPT_PSALTER_PAHLAVI` | property · L4 · T3 | `UCDN_SCRIPT_PSALTER_PAHLAVI` | `pub const UCDN_SCRIPT_PSALTER_PAHLAVI: i32` | `l4-constants-ucdn-script-psalter-pahlavi` |
| `UCDN_SCRIPT_REJANG` | property · L4 · T3 | `UCDN_SCRIPT_REJANG` | `pub const UCDN_SCRIPT_REJANG: i32` | `l4-constants-ucdn-script-rejang` |
| `UCDN_SCRIPT_RUNIC` | property · L4 · T3 | `UCDN_SCRIPT_RUNIC` | `pub const UCDN_SCRIPT_RUNIC: i32` | `l4-constants-ucdn-script-runic` |
| `UCDN_SCRIPT_SAMARITAN` | property · L4 · T3 | `UCDN_SCRIPT_SAMARITAN` | `pub const UCDN_SCRIPT_SAMARITAN: i32` | `l4-constants-ucdn-script-samaritan` |
| `UCDN_SCRIPT_SAURASHTRA` | property · L4 · T3 | `UCDN_SCRIPT_SAURASHTRA` | `pub const UCDN_SCRIPT_SAURASHTRA: i32` | `l4-constants-ucdn-script-saurashtra` |
| `UCDN_SCRIPT_SHARADA` | property · L4 · T3 | `UCDN_SCRIPT_SHARADA` | `pub const UCDN_SCRIPT_SHARADA: i32` | `l4-constants-ucdn-script-sharada` |
| `UCDN_SCRIPT_SHAVIAN` | property · L4 · T3 | `UCDN_SCRIPT_SHAVIAN` | `pub const UCDN_SCRIPT_SHAVIAN: i32` | `l4-constants-ucdn-script-shavian` |
| `UCDN_SCRIPT_SIDDHAM` | property · L4 · T3 | `UCDN_SCRIPT_SIDDHAM` | `pub const UCDN_SCRIPT_SIDDHAM: i32` | `l4-constants-ucdn-script-siddham` |
| `UCDN_SCRIPT_SIGNWRITING` | property · L4 · T3 | `UCDN_SCRIPT_SIGNWRITING` | `pub const UCDN_SCRIPT_SIGNWRITING: i32` | `l4-constants-ucdn-script-signwriting` |
| `UCDN_SCRIPT_SINHALA` | property · L4 · T3 | `UCDN_SCRIPT_SINHALA` | `pub const UCDN_SCRIPT_SINHALA: i32` | `l4-constants-ucdn-script-sinhala` |
| `UCDN_SCRIPT_SOGDIAN` | property · L4 · T3 | `UCDN_SCRIPT_SOGDIAN` | `pub const UCDN_SCRIPT_SOGDIAN: i32` | `l4-constants-ucdn-script-sogdian` |
| `UCDN_SCRIPT_SORA_SOMPENG` | property · L4 · T3 | `UCDN_SCRIPT_SORA_SOMPENG` | `pub const UCDN_SCRIPT_SORA_SOMPENG: i32` | `l4-constants-ucdn-script-sora-sompeng` |
| `UCDN_SCRIPT_SOYOMBO` | property · L4 · T3 | `UCDN_SCRIPT_SOYOMBO` | `pub const UCDN_SCRIPT_SOYOMBO: i32` | `l4-constants-ucdn-script-soyombo` |
| `UCDN_SCRIPT_SUNDANESE` | property · L4 · T3 | `UCDN_SCRIPT_SUNDANESE` | `pub const UCDN_SCRIPT_SUNDANESE: i32` | `l4-constants-ucdn-script-sundanese` |
| `UCDN_SCRIPT_SUNUWAR` | property · L4 · T3 | `UCDN_SCRIPT_SUNUWAR` | `pub const UCDN_SCRIPT_SUNUWAR: i32` | `l4-constants-ucdn-script-sunuwar` |
| `UCDN_SCRIPT_SYLOTI_NAGRI` | property · L4 · T3 | `UCDN_SCRIPT_SYLOTI_NAGRI` | `pub const UCDN_SCRIPT_SYLOTI_NAGRI: i32` | `l4-constants-ucdn-script-syloti-nagri` |
| `UCDN_SCRIPT_SYRIAC` | property · L4 · T3 | `UCDN_SCRIPT_SYRIAC` | `pub const UCDN_SCRIPT_SYRIAC: i32` | `l4-constants-ucdn-script-syriac` |
| `UCDN_SCRIPT_TAGALOG` | property · L4 · T3 | `UCDN_SCRIPT_TAGALOG` | `pub const UCDN_SCRIPT_TAGALOG: i32` | `l4-constants-ucdn-script-tagalog` |
| `UCDN_SCRIPT_TAGBANWA` | property · L4 · T3 | `UCDN_SCRIPT_TAGBANWA` | `pub const UCDN_SCRIPT_TAGBANWA: i32` | `l4-constants-ucdn-script-tagbanwa` |
| `UCDN_SCRIPT_TAI_LE` | property · L4 · T3 | `UCDN_SCRIPT_TAI_LE` | `pub const UCDN_SCRIPT_TAI_LE: i32` | `l4-constants-ucdn-script-tai-le` |
| `UCDN_SCRIPT_TAI_THAM` | property · L4 · T3 | `UCDN_SCRIPT_TAI_THAM` | `pub const UCDN_SCRIPT_TAI_THAM: i32` | `l4-constants-ucdn-script-tai-tham` |
| `UCDN_SCRIPT_TAI_VIET` | property · L4 · T3 | `UCDN_SCRIPT_TAI_VIET` | `pub const UCDN_SCRIPT_TAI_VIET: i32` | `l4-constants-ucdn-script-tai-viet` |
| `UCDN_SCRIPT_TAKRI` | property · L4 · T3 | `UCDN_SCRIPT_TAKRI` | `pub const UCDN_SCRIPT_TAKRI: i32` | `l4-constants-ucdn-script-takri` |
| `UCDN_SCRIPT_TAMIL` | property · L4 · T3 | `UCDN_SCRIPT_TAMIL` | `pub const UCDN_SCRIPT_TAMIL: i32` | `l4-constants-ucdn-script-tamil` |
| `UCDN_SCRIPT_TANGSA` | property · L4 · T3 | `UCDN_SCRIPT_TANGSA` | `pub const UCDN_SCRIPT_TANGSA: i32` | `l4-constants-ucdn-script-tangsa` |
| `UCDN_SCRIPT_TANGUT` | property · L4 · T3 | `UCDN_SCRIPT_TANGUT` | `pub const UCDN_SCRIPT_TANGUT: i32` | `l4-constants-ucdn-script-tangut` |
| `UCDN_SCRIPT_TELUGU` | property · L4 · T3 | `UCDN_SCRIPT_TELUGU` | `pub const UCDN_SCRIPT_TELUGU: i32` | `l4-constants-ucdn-script-telugu` |
| `UCDN_SCRIPT_THAANA` | property · L4 · T3 | `UCDN_SCRIPT_THAANA` | `pub const UCDN_SCRIPT_THAANA: i32` | `l4-constants-ucdn-script-thaana` |
| `UCDN_SCRIPT_THAI` | property · L4 · T3 | `UCDN_SCRIPT_THAI` | `pub const UCDN_SCRIPT_THAI: i32` | `l4-constants-ucdn-script-thai` |
| `UCDN_SCRIPT_TIBETAN` | property · L4 · T3 | `UCDN_SCRIPT_TIBETAN` | `pub const UCDN_SCRIPT_TIBETAN: i32` | `l4-constants-ucdn-script-tibetan` |
| `UCDN_SCRIPT_TIFINAGH` | property · L4 · T3 | `UCDN_SCRIPT_TIFINAGH` | `pub const UCDN_SCRIPT_TIFINAGH: i32` | `l4-constants-ucdn-script-tifinagh` |
| `UCDN_SCRIPT_TIRHUTA` | property · L4 · T3 | `UCDN_SCRIPT_TIRHUTA` | `pub const UCDN_SCRIPT_TIRHUTA: i32` | `l4-constants-ucdn-script-tirhuta` |
| `UCDN_SCRIPT_TODHRI` | property · L4 · T3 | `UCDN_SCRIPT_TODHRI` | `pub const UCDN_SCRIPT_TODHRI: i32` | `l4-constants-ucdn-script-todhri` |
| `UCDN_SCRIPT_TOTO` | property · L4 · T3 | `UCDN_SCRIPT_TOTO` | `pub const UCDN_SCRIPT_TOTO: i32` | `l4-constants-ucdn-script-toto` |
| `UCDN_SCRIPT_TULU_TIGALARI` | property · L4 · T3 | `UCDN_SCRIPT_TULU_TIGALARI` | `pub const UCDN_SCRIPT_TULU_TIGALARI: i32` | `l4-constants-ucdn-script-tulu-tigalari` |
| `UCDN_SCRIPT_UGARITIC` | property · L4 · T3 | `UCDN_SCRIPT_UGARITIC` | `pub const UCDN_SCRIPT_UGARITIC: i32` | `l4-constants-ucdn-script-ugaritic` |
| `UCDN_SCRIPT_UNKNOWN` | property · L4 · T3 | `UCDN_SCRIPT_UNKNOWN` | `pub const UCDN_SCRIPT_UNKNOWN: i32` | `l4-constants-ucdn-script-unknown` |
| `UCDN_SCRIPT_VAI` | property · L4 · T3 | `UCDN_SCRIPT_VAI` | `pub const UCDN_SCRIPT_VAI: i32` | `l4-constants-ucdn-script-vai` |
| `UCDN_SCRIPT_VITHKUQI` | property · L4 · T3 | `UCDN_SCRIPT_VITHKUQI` | `pub const UCDN_SCRIPT_VITHKUQI: i32` | `l4-constants-ucdn-script-vithkuqi` |
| `UCDN_SCRIPT_WANCHO` | property · L4 · T3 | `UCDN_SCRIPT_WANCHO` | `pub const UCDN_SCRIPT_WANCHO: i32` | `l4-constants-ucdn-script-wancho` |
| `UCDN_SCRIPT_WARANG_CITI` | property · L4 · T3 | `UCDN_SCRIPT_WARANG_CITI` | `pub const UCDN_SCRIPT_WARANG_CITI: i32` | `l4-constants-ucdn-script-warang-citi` |
| `UCDN_SCRIPT_YEZIDI` | property · L4 · T3 | `UCDN_SCRIPT_YEZIDI` | `pub const UCDN_SCRIPT_YEZIDI: i32` | `l4-constants-ucdn-script-yezidi` |
| `UCDN_SCRIPT_YI` | property · L4 · T3 | `UCDN_SCRIPT_YI` | `pub const UCDN_SCRIPT_YI: i32` | `l4-constants-ucdn-script-yi` |
| `UCDN_SCRIPT_ZANABAZAR_SQUARE` | property · L4 · T3 | `UCDN_SCRIPT_ZANABAZAR_SQUARE` | `pub const UCDN_SCRIPT_ZANABAZAR_SQUARE: i32` | `l4-constants-ucdn-script-zanabazar-square` |
| `PDF_CID_FONT_RESOURCE` | property · L4 · T3 | `PDF_CID_FONT_RESOURCE` | `pub const PDF_CID_FONT_RESOURCE: i32` | `l4-constants-pdf-cid-font-resource` |
| `PDF_CJK_FONT_RESOURCE` | property · L4 · T3 | `PDF_CJK_FONT_RESOURCE` | `pub const PDF_CJK_FONT_RESOURCE: i32` | `l4-constants-pdf-cjk-font-resource` |
| `PDF_FD_ALL_CAP` | property · L4 · T3 | `PDF_FD_ALL_CAP` | `pub const PDF_FD_ALL_CAP: i32` | `l4-constants-pdf-fd-all-cap` |
| `PDF_FD_FIXED_PITCH` | property · L4 · T3 | `PDF_FD_FIXED_PITCH` | `pub const PDF_FD_FIXED_PITCH: i32` | `l4-constants-pdf-fd-fixed-pitch` |
| `PDF_FD_FORCE_BOLD` | property · L4 · T3 | `PDF_FD_FORCE_BOLD` | `pub const PDF_FD_FORCE_BOLD: i32` | `l4-constants-pdf-fd-force-bold` |
| `PDF_FD_ITALIC` | property · L4 · T3 | `PDF_FD_ITALIC` | `pub const PDF_FD_ITALIC: i32` | `l4-constants-pdf-fd-italic` |
| `PDF_FD_NONSYMBOLIC` | property · L4 · T3 | `PDF_FD_NONSYMBOLIC` | `pub const PDF_FD_NONSYMBOLIC: i32` | `l4-constants-pdf-fd-nonsymbolic` |
| `PDF_FD_SCRIPT` | property · L4 · T3 | `PDF_FD_SCRIPT` | `pub const PDF_FD_SCRIPT: i32` | `l4-constants-pdf-fd-script` |
| `PDF_FD_SERIF` | property · L4 · T3 | `PDF_FD_SERIF` | `pub const PDF_FD_SERIF: i32` | `l4-constants-pdf-fd-serif` |
| `PDF_FD_SMALL_CAP` | property · L4 · T3 | `PDF_FD_SMALL_CAP` | `pub const PDF_FD_SMALL_CAP: i32` | `l4-constants-pdf-fd-small-cap` |
| `PDF_FD_SYMBOLIC` | property · L4 · T3 | `PDF_FD_SYMBOLIC` | `pub const PDF_FD_SYMBOLIC: i32` | `l4-constants-pdf-fd-symbolic` |
| `PDF_SIMPLE_ENCODING_CYRILLIC` | property · L4 · T3 | `PDF_SIMPLE_ENCODING_CYRILLIC` | `pub const PDF_SIMPLE_ENCODING_CYRILLIC: i32` | `l4-constants-pdf-simple-encoding-cyrillic` |
| `PDF_SIMPLE_ENCODING_GREEK` | property · L4 · T3 | `PDF_SIMPLE_ENCODING_GREEK` | `pub const PDF_SIMPLE_ENCODING_GREEK: i32` | `l4-constants-pdf-simple-encoding-greek` |
| `PDF_SIMPLE_ENCODING_LATIN` | property · L4 · T3 | `PDF_SIMPLE_ENCODING_LATIN` | `pub const PDF_SIMPLE_ENCODING_LATIN: i32` | `l4-constants-pdf-simple-encoding-latin` |
| `PDF_SIMPLE_FONT_RESOURCE` | property · L4 · T3 | `PDF_SIMPLE_FONT_RESOURCE` | `pub const PDF_SIMPLE_FONT_RESOURCE: i32` | `l4-constants-pdf-simple-font-resource` |
| `Base14_fontdict` | property · L4 · T3 | `Base14_fontdict` | `pub const Base14_fontdict: &[(&str, &str)]` | `l4-constants-base14-fontdict` |
| `Base14_fontnames` | property · L4 · T3 | `Base14_fontnames` | `pub const Base14_fontnames: &[&str]` | `l4-constants-base14-fontnames` |
| `symbol_glyphs` | property · L4 · T3 | `symbol_glyphs` | `pub const symbol_glyphs: &[(i32, f64)]` | `l4-constants-symbol-glyphs` |
| `zapf_glyphs` | property · L4 · T3 | `zapf_glyphs` | `pub const zapf_glyphs: &[(i32, f64)]` | `l4-constants-zapf-glyphs` |
| `pdfcolor` | property · L4 · T3 | `pdfcolor` | `pub const pdfcolor: &[(&str, [f64; 3])]` | `l4-constants-pdfcolor` |
| `INVALID_NAME_CHARS` | property · L2 · T3 | `INVALID_NAME_CHARS` | `pub const INVALID_NAME_CHARS: &[&str]` | `l2-constants-invalid-name-chars` |
| `PDF_ENUM_FALSE` | property · L2 · T3 | `PDF_ENUM_FALSE` | `pub const PDF_ENUM_FALSE: i32` | `l2-constants-pdf-enum-false` |
| `PDF_ENUM_LIMIT` | property · L2 · T3 | `PDF_ENUM_LIMIT` | `pub const PDF_ENUM_LIMIT: i32` | `l2-constants-pdf-enum-limit` |
| `PDF_ENUM_NULL` | property · L2 · T3 | `PDF_ENUM_NULL` | `pub const PDF_ENUM_NULL: i32` | `l2-constants-pdf-enum-null` |
| `PDF_ENUM_TRUE` | property · L2 · T3 | `PDF_ENUM_TRUE` | `pub const PDF_ENUM_TRUE: i32` | `l2-constants-pdf-enum-true` |
| `PDF_LEXBUF_LARGE` | property · L2 · T3 | `PDF_LEXBUF_LARGE` | `pub const PDF_LEXBUF_LARGE: i32` | `l2-constants-pdf-lexbuf-large` |
| `PDF_LEXBUF_SMALL` | property · L2 · T3 | `PDF_LEXBUF_SMALL` | `pub const PDF_LEXBUF_SMALL: i32` | `l2-constants-pdf-lexbuf-small` |
| `PDF_MAX_GEN_NUMBER` | property · L2 · T3 | `PDF_MAX_GEN_NUMBER` | `pub const PDF_MAX_GEN_NUMBER: i32` | `l2-constants-pdf-max-gen-number` |
| `PDF_MAX_OBJECT_NUMBER` | property · L2 · T3 | `PDF_MAX_OBJECT_NUMBER` | `pub const PDF_MAX_OBJECT_NUMBER: i32` | `l2-constants-pdf-max-object-number` |
| `PDF_MRANGE_CAP` | property · L2 · T3 | `PDF_MRANGE_CAP` | `pub const PDF_MRANGE_CAP: i32` | `l2-constants-pdf-mrange-cap` |
| `PDF_NUM_TOKENS` | property · L2 · T3 | `PDF_NUM_TOKENS` | `pub const PDF_NUM_TOKENS: i32` | `l2-constants-pdf-num-tokens` |
| `PDF_PROCESSOR_REQUIRES_DECODED_IMAGES` | property · L2 · T3 | `PDF_PROCESSOR_REQUIRES_DECODED_IMAGES` | `pub const PDF_PROCESSOR_REQUIRES_DECODED_IMAGES: i32` | `l2-constants-pdf-processor-requires-decoded-images` |
| `PDF_TOK_CLOSE_ARRAY` | property · L2 · T3 | `PDF_TOK_CLOSE_ARRAY` | `pub const PDF_TOK_CLOSE_ARRAY: i32` | `l2-constants-pdf-tok-close-array` |
| `PDF_TOK_CLOSE_BRACE` | property · L2 · T3 | `PDF_TOK_CLOSE_BRACE` | `pub const PDF_TOK_CLOSE_BRACE: i32` | `l2-constants-pdf-tok-close-brace` |
| `PDF_TOK_CLOSE_DICT` | property · L2 · T3 | `PDF_TOK_CLOSE_DICT` | `pub const PDF_TOK_CLOSE_DICT: i32` | `l2-constants-pdf-tok-close-dict` |
| `PDF_TOK_ENDOBJ` | property · L2 · T3 | `PDF_TOK_ENDOBJ` | `pub const PDF_TOK_ENDOBJ: i32` | `l2-constants-pdf-tok-endobj` |
| `PDF_TOK_ENDSTREAM` | property · L2 · T3 | `PDF_TOK_ENDSTREAM` | `pub const PDF_TOK_ENDSTREAM: i32` | `l2-constants-pdf-tok-endstream` |
| `PDF_TOK_EOF` | property · L2 · T3 | `PDF_TOK_EOF` | `pub const PDF_TOK_EOF: i32` | `l2-constants-pdf-tok-eof` |
| `PDF_TOK_ERROR` | property · L2 · T3 | `PDF_TOK_ERROR` | `pub const PDF_TOK_ERROR: i32` | `l2-constants-pdf-tok-error` |
| `PDF_TOK_FALSE` | property · L2 · T3 | `PDF_TOK_FALSE` | `pub const PDF_TOK_FALSE: i32` | `l2-constants-pdf-tok-false` |
| `PDF_TOK_INT` | property · L2 · T3 | `PDF_TOK_INT` | `pub const PDF_TOK_INT: i32` | `l2-constants-pdf-tok-int` |
| `PDF_TOK_KEYWORD` | property · L2 · T3 | `PDF_TOK_KEYWORD` | `pub const PDF_TOK_KEYWORD: i32` | `l2-constants-pdf-tok-keyword` |
| `PDF_TOK_NAME` | property · L2 · T3 | `PDF_TOK_NAME` | `pub const PDF_TOK_NAME: i32` | `l2-constants-pdf-tok-name` |
| `PDF_TOK_NEWOBJ` | property · L2 · T3 | `PDF_TOK_NEWOBJ` | `pub const PDF_TOK_NEWOBJ: i32` | `l2-constants-pdf-tok-newobj` |
| `PDF_TOK_NULL` | property · L2 · T3 | `PDF_TOK_NULL` | `pub const PDF_TOK_NULL: i32` | `l2-constants-pdf-tok-null` |
| `PDF_TOK_OBJ` | property · L2 · T3 | `PDF_TOK_OBJ` | `pub const PDF_TOK_OBJ: i32` | `l2-constants-pdf-tok-obj` |
| `PDF_TOK_OPEN_ARRAY` | property · L2 · T3 | `PDF_TOK_OPEN_ARRAY` | `pub const PDF_TOK_OPEN_ARRAY: i32` | `l2-constants-pdf-tok-open-array` |
| `PDF_TOK_OPEN_BRACE` | property · L2 · T3 | `PDF_TOK_OPEN_BRACE` | `pub const PDF_TOK_OPEN_BRACE: i32` | `l2-constants-pdf-tok-open-brace` |
| `PDF_TOK_OPEN_DICT` | property · L2 · T3 | `PDF_TOK_OPEN_DICT` | `pub const PDF_TOK_OPEN_DICT: i32` | `l2-constants-pdf-tok-open-dict` |
| `PDF_TOK_R` | property · L2 · T3 | `PDF_TOK_R` | `pub const PDF_TOK_R: i32` | `l2-constants-pdf-tok-r` |
| `PDF_TOK_REAL` | property · L2 · T3 | `PDF_TOK_REAL` | `pub const PDF_TOK_REAL: i32` | `l2-constants-pdf-tok-real` |
| `PDF_TOK_STARTXREF` | property · L2 · T3 | `PDF_TOK_STARTXREF` | `pub const PDF_TOK_STARTXREF: i32` | `l2-constants-pdf-tok-startxref` |
| `PDF_TOK_STREAM` | property · L2 · T3 | `PDF_TOK_STREAM` | `pub const PDF_TOK_STREAM: i32` | `l2-constants-pdf-tok-stream` |
| `PDF_TOK_STRING` | property · L2 · T3 | `PDF_TOK_STRING` | `pub const PDF_TOK_STRING: i32` | `l2-constants-pdf-tok-string` |
| `PDF_TOK_TRAILER` | property · L2 · T3 | `PDF_TOK_TRAILER` | `pub const PDF_TOK_TRAILER: i32` | `l2-constants-pdf-tok-trailer` |
| `PDF_TOK_TRUE` | property · L2 · T3 | `PDF_TOK_TRUE` | `pub const PDF_TOK_TRUE: i32` | `l2-constants-pdf-tok-true` |
| `PDF_TOK_XREF` | property · L2 · T3 | `PDF_TOK_XREF` | `pub const PDF_TOK_XREF: i32` | `l2-constants-pdf-tok-xref` |
| `b` | property · L2 · T3 | `b` | `pub const b: i32` | `l2-constants-b` |
| `f` | property · L2 · T3 | `f` | `pub const f: &str` | `l2-constants-f` |
| `g` | property · L2 · T3 | `g` | `pub const g: i32` | `l2-constants-g` |
| `matrix_like` | property · L2 · T3 | `matrix_like` | `pub const matrix_like: &str` | `l2-constants-matrix-like` |
| `name` | property · L2 · T3 | `name` | `pub const name: &str` | `l2-constants-name` |
| `point_like` | property · L2 · T3 | `point_like` | `pub const point_like: &str` | `l2-constants-point-like` |
| `quad_like` | property · L2 · T3 | `quad_like` | `pub const quad_like: &str` | `l2-constants-quad-like` |
| `r` | property · L2 · T3 | `r` | `pub const r: i32` | `l2-constants-r` |
| `rect_like` | property · L2 · T3 | `rect_like` | `pub const rect_like: &str` | `l2-constants-rect-like` |
| `trace_device_CLIP_PATH` | property · L2 · T3 | `trace_device_CLIP_PATH` | `pub const trace_device_CLIP_PATH: i32` | `l2-constants-trace-device-clip-path` |
| `trace_device_CLIP_STROKE_PATH` | property · L2 · T3 | `trace_device_CLIP_STROKE_PATH` | `pub const trace_device_CLIP_STROKE_PATH: i32` | `l2-constants-trace-device-clip-stroke-path` |
| `trace_device_FILL_PATH` | property · L2 · T3 | `trace_device_FILL_PATH` | `pub const trace_device_FILL_PATH: i32` | `l2-constants-trace-device-fill-path` |
| `trace_device_STROKE_PATH` | property · L2 · T3 | `trace_device_STROKE_PATH` | `pub const trace_device_STROKE_PATH: i32` | `l2-constants-trace-device-stroke-path` |

## exceptions

| Member | Kind | Rust path | Rust signature | Case |
| --- | --- | --- | --- | --- |
| `EmptyFileError` | property · P0 · T2 | `ErrorKind::EmptyFile` | `variant ErrorKind::EmptyFile` | `p0-exceptions-emptyfileerror` |
| `FileDataError` | property · P0 · T2 | `ErrorKind::UnreadableFile` | `variant ErrorKind::UnreadableFile` | `p0-exceptions-filedataerror` |
| `FileNotFoundError` | property · P0 · T3 | `ErrorKind::MissingFile` | `variant ErrorKind::MissingFile` | `p0-exceptions-filenotfounderror` |
| `FitzDeprecation` | property · P0 · T3 | `ErrorKind::Deprecation` | `variant ErrorKind::Deprecation` | `p0-exceptions-deprecation` |
