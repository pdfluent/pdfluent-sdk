// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

// Planned PyMuPDF-compatible surface (PyMuPDF==1.28.2).
//
// Generated from specs/api/*.yaml by scripts/compat/gen_dts.py. Do not edit
// by hand. Parameter names and shapes follow the spec; every type is any
// until the binding implements the member behind it.

export declare class Document {
  /** Document.FormFonts compatibility entry (case l1-document-formfonts). */
  formFonts: any;
  /** Document.__contains__ compatibility entry (case l2-document-contains). */
  contains(loc: any): any;
  /** Document.__delitem__ compatibility entry (case l2-document-delitem). */
  delitem(i: any): any;
  /** Document.__enter__ compatibility entry (case l2.doc.context_manager). */
  enter(): any;
  /** Document.__eq__ compatibility entry (case l2-document-eq). */
  eq(value: any): any;
  /** Document.__exit__ compatibility entry (case l2-document-exit). */
  exit(...args: any[]): any;
  /** Document.__ge__ compatibility entry (case l2-document-ge). */
  ge(value: any): any;
  /** Document.__getitem__ compatibility entry (case l2.pages.getitem). */
  getitem(i?: any): any;
  /** Document.__gt__ compatibility entry (case l2-document-gt). */
  gt(value: any): any;
  /** Document.__init__ compatibility entry (case l2.open.document_ctor). */
  constructor(filename?: any, stream?: any, filetype?: any, rect?: any, width?: any, height?: any, fontsize?: any, archive?: any);
  /** Document.__le__ compatibility entry (case l2-document-le). */
  le(value: any): any;
  /** Document.__len__ compatibility entry (case l2.pages.len). */
  len(): any;
  /** Document.__lt__ compatibility entry (case l2-document-lt). */
  lt(value: any): any;
  /** Document.__ne__ compatibility entry (case l2-document-ne). */
  ne(value: any): any;
  /** Document.add_layer compatibility entry (case l4-document-add-layer). */
  addLayer(name: any, creator?: any, on?: any): any;
  /** Document.add_ocg compatibility entry (case l4-document-add-ocg). */
  addOcg(name: any, config?: any, on?: any, intent?: any, usage?: any): any;
  /** Document.apply_css compatibility entry (case l4-document-apply-css). */
  applyCss(css: any, append?: any): any;
  /** Document.authenticate compatibility entry (case l2.crypto.authenticate). */
  authenticate(password: any): any;
  /** Document.bake compatibility entry (case l4-document-bake). */
  bake(annots?: any, widgets?: any): any;
  /** Document.can_save_incrementally compatibility entry (case l2.doc.can_save_incrementally). */
  canSaveIncrementally(): any;
  /** Document.chapter_count compatibility entry (case l2-document-chapter-count). */
  chapterCount: any;
  /** Document.chapter_page_count compatibility entry (case l2-document-chapter-page-count). */
  chapterPageCount(chapter: any): any;
  /** Document.close compatibility entry (case l2.doc.close_twice). */
  close(): any;
  /** Document.convert_to_pdf compatibility entry (case l2-document-convert-to-pdf). */
  convertToPdf(from_page?: any, to_page?: any, rotate?: any): any;
  /** Document.copy_page compatibility entry (case l2.edit.copy_page). */
  copyPage(pno: any, to?: any): any;
  /** Document.del_toc_item compatibility entry (case l2.toc.del_item). */
  delTocItem(idx: any): any;
  /** Document.del_xml_metadata compatibility entry (case l4-document-del-xml-metadata). */
  delXmlMetadata(): any;
  /** Document.delete_page compatibility entry (case l2.edit.delete_page_default). */
  deletePage(pno?: any): any;
  /** Document.delete_pages compatibility entry (case l2.edit.delete_pages_bad). */
  deletePages(...args: any[]): any;
  /** Document.embfile_add compatibility entry (case l4-document-embfile-add). */
  embfileAdd(name: any, buffer_: any, filename?: any, ufilename?: any, desc?: any): any;
  /** Document.embfile_count compatibility entry (case l2-document-embfile-count). */
  embfileCount(): any;
  /** Document.embfile_del compatibility entry (case l4-document-embfile-del). */
  embfileDel(item: any): any;
  /** Document.embfile_get compatibility entry (case l2-document-embfile-get). */
  embfileGet(item: any): any;
  /** Document.embfile_info compatibility entry (case l2-document-embfile-info). */
  embfileInfo(item: any): any;
  /** Document.embfile_names compatibility entry (case l2-document-embfile-names). */
  embfileNames(): any;
  /** Document.embfile_upd compatibility entry (case l4-document-embfile-upd). */
  embfileUpd(item: any, buffer_?: any, filename?: any, ufilename?: any, desc?: any): any;
  /** Document.extract_font compatibility entry (case l1-document-extract-font). */
  extractFont(xref?: any, info_only?: any, named?: any): any;
  /** Document.extract_image compatibility entry (case l3-doc-extract-image). */
  extractImage(xref: any): any;
  /** Document.ez_save compatibility entry (case l2-document-ez-save). */
  ezSave(filename: any, garbage?: any, clean?: any, deflate?: any, deflate_images?: any, deflate_fonts?: any, incremental?: any, ascii?: any, expand?: any, linear?: any, pretty?: any, encryption?: any, permissions?: any, owner_pw?: any, user_pw?: any, no_new_id?: any, preserve_metadata?: any, use_objstms?: any, compression_effort?: any, raise_on_repair?: any, reproducible?: any): any;
  /** Document.find_bookmark compatibility entry (case l2-document-find-bookmark). */
  findBookmark(bm: any): any;
  /** Document.fullcopy_page compatibility entry (case l2.edit.fullcopy_page). */
  fullcopyPage(pno: any, to?: any): any;
  /** Document.get_char_widths compatibility entry (case l1-document-get-char-widths). */
  getCharWidths(xref: any, limit?: any, idx?: any, fontdict?: any): any;
  /** Document.get_layer compatibility entry (case l2-document-get-layer). */
  getLayer(config?: any): any;
  /** Document.get_layers compatibility entry (case l2-document-get-layers). */
  getLayers(): any;
  /** Document.get_new_xref compatibility entry (case l2-document-get-new-xref). */
  getNewXref(): any;
  /** Document.get_oc compatibility entry (case l2-document-get-oc). */
  getOc(xref: any): any;
  /** Document.get_ocgs compatibility entry (case l2-document-get-ocgs). */
  getOcgs(): any;
  /** Document.get_ocmd compatibility entry (case l2-document-get-ocmd). */
  getOcmd(xref: any): any;
  /** Document.get_outline_xrefs compatibility entry (case l2-document-get-outline-xrefs). */
  getOutlineXrefs(): any;
  /** Document.get_page_fonts compatibility entry (case l1-document-get-page-fonts). */
  getPageFonts(pno: any, full?: any): any;
  /** Document.get_page_images compatibility entry (case l3-doc-get-page-images). */
  getPageImages(pno: any, full?: any): any;
  /** Document.get_page_labels compatibility entry (case l2-document-get-page-labels). */
  getPageLabels(): any;
  /** Document.get_page_numbers compatibility entry (case l2-document-get-page-numbers). */
  getPageNumbers(label: any, only_one?: any): any;
  /** Document.get_page_pixmap compatibility entry (case l3-doc-get-page-pixmap). */
  getPagePixmap(pno: any, matrix?: any, dpi?: any, colorspace?: any, clip?: any, alpha?: any, annots?: any): any;
  /** Document.get_page_text compatibility entry (case L1.get_page_text.default). */
  getPageText(pno: any, option?: any, clip?: any, flags?: any, textpage?: any, sort?: any): any;
  /** Document.get_page_xobjects compatibility entry (case l3-page-get-xobjects). */
  getPageXobjects(pno: any): any;
  /** Document.get_sigflags compatibility entry (case l2-document-get-sigflags). */
  getSigflags(): any;
  /** Document.get_toc compatibility entry (case l2-toc). */
  getToc(simple?: any): any;
  /** Document.get_xml_metadata compatibility entry (case l2-document-get-xml-metadata). */
  getXmlMetadata(): any;
  /** Document.has_annots compatibility entry (case l4-document-has-annots). */
  hasAnnots(): any;
  /** Document.has_links compatibility entry (case l2.doc.has_links). */
  hasLinks(): any;
  /** Document.init_doc compatibility entry (case l2-document-init-doc). */
  initDoc(): any;
  /** Document.insert_file compatibility entry (case l4-document-insert-file). */
  insertFile(infile: any, from_page?: any, to_page?: any, start_at?: any, rotate?: any, links?: any, annots?: any, show_progress?: any, final?: any): any;
  /** Document.insert_page compatibility entry (case l4-document-insert-page). */
  insertPage(pno: any, text?: any, fontsize?: any, width?: any, height?: any, fontname?: any, fontfile?: any, color?: any): any;
  /** Document.insert_pdf compatibility entry (case l2.edit.insert_pdf_all). */
  insertPdf(docsrc: any, from_page?: any, to_page?: any, start_at?: any, rotate?: any, links?: any, annots?: any, widgets?: any, join_duplicates?: any, show_progress?: any, final?: any, _gmap?: any): any;
  /** Document.is_dirty compatibility entry (case l2.doc.is_dirty). */
  isDirty: any;
  /** Document.is_fast_webaccess compatibility entry (case l2-document-is-fast-webaccess). */
  isFastWebaccess: any;
  /** Document.is_form_pdf compatibility entry (case l2-document-is-form-pdf). */
  isFormPdf: any;
  /** Document.is_pdf compatibility entry (case l2.doc.is_pdf). */
  isPdf: any;
  /** Document.is_reflowable compatibility entry (case l2-document-is-reflowable). */
  isReflowable: any;
  /** Document.is_repaired compatibility entry (case l2-document-is-repaired). */
  isRepaired: any;
  /** Document.is_stream compatibility entry (case l2-document-is-stream). */
  isStream(xref?: any): any;
  /** Document.journal_can_do compatibility entry (case l4-document-journal-can-do). */
  journalCanDo(): any;
  /** Document.journal_enable compatibility entry (case l4-document-journal-enable). */
  journalEnable(): any;
  /** Document.journal_is_enabled compatibility entry (case l4-document-journal-is-enabled). */
  journalIsEnabled(): any;
  /** Document.journal_load compatibility entry (case l4-document-journal-load). */
  journalLoad(filename: any): any;
  /** Document.journal_op_name compatibility entry (case l4-document-journal-op-name). */
  journalOpName(step: any): any;
  /** Document.journal_position compatibility entry (case l4-document-journal-position). */
  journalPosition(): any;
  /** Document.journal_redo compatibility entry (case l4-document-journal-redo). */
  journalRedo(): any;
  /** Document.journal_save compatibility entry (case l4-document-journal-save). */
  journalSave(filename: any): any;
  /** Document.journal_start_op compatibility entry (case l4-document-journal-start-op). */
  journalStartOp(name?: any): any;
  /** Document.journal_stop_op compatibility entry (case l4-document-journal-stop-op). */
  journalStopOp(): any;
  /** Document.journal_undo compatibility entry (case l4-document-journal-undo). */
  journalUndo(): any;
  /** Document.language compatibility entry (case l2-document-language). */
  language: any;
  /** Document.last_location compatibility entry (case l2-document-last-location). */
  lastLocation: any;
  /** Document.layer_ui_configs compatibility entry (case l2-document-layer-ui-configs). */
  layerUiConfigs(): any;
  /** Document.layout compatibility entry (case l2-document-layout). */
  layout(rect?: any, width?: any, height?: any, fontsize?: any): any;
  /** Document.load_page compatibility entry (case l2.pages.load_page). */
  loadPage(page_id: any): any;
  /** Document.location_from_page_number compatibility entry (case l2-document-location-from-page-number). */
  locationFromPageNumber(pno: any): any;
  /** Document.make_bookmark compatibility entry (case l2-document-make-bookmark). */
  makeBookmark(loc: any): any;
  /** Document.markinfo compatibility entry (case l2-document-markinfo). */
  markinfo: any;
  /** Document.move_page compatibility entry (case l2.edit.move_page). */
  movePage(pno: any, to?: any): any;
  /** Document.name compatibility entry (case l2.doc.name). */
  name: any;
  /** Document.need_appearances compatibility entry (case l2-document-need-appearances). */
  needAppearances(value?: any): any;
  /** Document.needs_pass compatibility entry (case l2.crypto.needs_pass). */
  needsPass: any;
  /** Document.new_page compatibility entry (case l2.edit.new_page_default). */
  newPage(pno?: any, width?: any, height?: any): any;
  /** Document.next_location compatibility entry (case l2-document-next-location). */
  nextLocation(page_id: any): any;
  /** Document.outline compatibility entry (case l2-document-outline). */
  outline: any;
  /** Document.page_annot_xrefs compatibility entry (case l4-document-page-annot-xrefs). */
  pageAnnotXrefs(n: any): any;
  /** Document.page_count compatibility entry (case l2-page-count). */
  pageCount: any;
  /** Document.page_count2 compatibility entry (case l2-document-page-count2). */
  pageCount2: any;
  /** Document.page_cropbox compatibility entry (case l2.page.cropbox). */
  pageCropbox(pno: any): any;
  /** Document.page_number_from_location compatibility entry (case l2-document-page-number-from-location). */
  pageNumberFromLocation(page_id: any): any;
  /** Document.page_xref compatibility entry (case l2-document-page-xref). */
  pageXref(pno: any): any;
  /** Document.pagelayout compatibility entry (case l2-document-pagelayout). */
  pagelayout: any;
  /** Document.pagemode compatibility entry (case l2-document-pagemode). */
  pagemode: any;
  /** Document.pages compatibility entry (case l2.pages.iter). */
  pages(start?: any, stop?: any, step?: any): any;
  /** Document.pdf_catalog compatibility entry (case l2-document-pdf-catalog). */
  pdfCatalog(): any;
  /** Document.pdf_trailer compatibility entry (case l2-document-pdf-trailer). */
  pdfTrailer(compressed?: any, ascii?: any): any;
  /** Document.permissions compatibility entry (case l2.doc.permissions). */
  permissions: any;
  /** Document.prev_location compatibility entry (case l2-document-prev-location). */
  prevLocation(page_id: any): any;
  /** Document.recolor compatibility entry (case l2-document-recolor). */
  recolor(components?: any): any;
  /** Document.reload_page compatibility entry (case l2-document-reload-page). */
  reloadPage(page: any): any;
  /** Document.repair compatibility entry (case l4-document-repair). */
  repair(): any;
  /** Document.resolve_link compatibility entry (case l4-document-resolve-link). */
  resolveLink(uri?: any, chapters?: any): any;
  /** Document.resolve_names compatibility entry (case l2-document-resolve-names). */
  resolveNames(): any;
  /** Document.rewrite_images compatibility entry (case l3-doc-rewrite-images). */
  rewriteImages(dpi_threshold?: any, dpi_target?: any, quality?: any, lossy?: any, lossless?: any, bitonal?: any, color?: any, gray?: any, set_to_gray?: any, options?: any): any;
  /** Document.save compatibility entry (case l2.crypto.save_aes256_roundtrip). */
  save(filename: any, garbage?: any, clean?: any, deflate?: any, deflate_images?: any, deflate_fonts?: any, incremental?: any, ascii?: any, expand?: any, linear?: any, no_new_id?: any, appearance?: any, pretty?: any, encryption?: any, permissions?: any, owner_pw?: any, user_pw?: any, preserve_metadata?: any, use_objstms?: any, compression_effort?: any, raise_on_repair?: any, reproducible?: any): any;
  /** Document.saveIncr compatibility entry (case l2.save.saveIncr). */
  saveIncr(): any;
  /** Document.save_snapshot compatibility entry (case l2-document-save-snapshot). */
  saveSnapshot(filename: any): any;
  /** Document.scrub compatibility entry (case l4-document-scrub). */
  scrub(attached_files?: any, clean_pages?: any, embedded_files?: any, hidden_text?: any, javascript?: any, metadata?: any, redactions?: any, redact_images?: any, remove_links?: any, reset_fields?: any, reset_responses?: any, thumbnails?: any, xml_metadata?: any): any;
  /** Document.search_page_for compatibility entry (case L1.search_for.empty_page). */
  searchPageFor(pno: any, text: any, quads?: any, clip?: any, flags?: any, textpage?: any): any;
  /** Document.select compatibility entry (case l2.edit.select). */
  select(pyliste: any): any;
  /** Document.set_language compatibility entry (case l4-document-set-language). */
  setLanguage(language?: any): any;
  /** Document.set_layer compatibility entry (case l4-document-set-layer). */
  setLayer(config: any, basestate?: any, on?: any, off?: any, rbgroups?: any, locked?: any): any;
  /** Document.set_layer_ui_config compatibility entry (case l4-document-set-layer-ui-config). */
  setLayerUiConfig(number_: any, action?: any): any;
  /** Document.set_markinfo compatibility entry (case l4-document-set-markinfo). */
  setMarkinfo(markinfo: any): any;
  /** Document.set_metadata compatibility entry (case l2.metadata.set_badkey). */
  setMetadata(m?: any): any;
  /** Document.set_oc compatibility entry (case l4-document-set-oc). */
  setOc(xref: any, oc: any): any;
  /** Document.set_ocmd compatibility entry (case l4-document-set-ocmd). */
  setOcmd(xref?: any, ocgs?: any, policy?: any, ve?: any): any;
  /** Document.set_page_labels compatibility entry (case l2.pages.labels_roundtrip). */
  setPageLabels(labels: any): any;
  /** Document.set_pagelayout compatibility entry (case l4-document-set-pagelayout). */
  setPagelayout(pagelayout: any): any;
  /** Document.set_pagemode compatibility entry (case l4-document-set-pagemode). */
  setPagemode(pagemode: any): any;
  /** Document.set_toc compatibility entry (case l2.toc.set_bad). */
  setToc(toc: any, collapse?: any): any;
  /** Document.set_toc_item compatibility entry (case l2.toc.set_item_title). */
  setTocItem(idx: any, dest_dict?: any, kind?: any, pno?: any, uri?: any, title?: any, to?: any, filename?: any, zoom?: any): any;
  /** Document.set_xml_metadata compatibility entry (case l4-document-set-xml-metadata). */
  setXmlMetadata(metadata: any): any;
  /** Document.subset_fonts compatibility entry (case l4-document-subset-fonts). */
  subsetFonts(verbose?: any, fallback?: any): any;
  /** Document.switch_layer compatibility entry (case l2-document-switch-layer). */
  switchLayer(config: any, as_default?: any): any;
  /** Document.this compatibility entry (case l2-document-this). */
  this: any;
  /** Document.this_is_pdf compatibility entry (case l2-document-this-is-pdf). */
  thisIsPdf: any;
  /** Document.tobytes compatibility entry (case l2.crypto.tobytes_encrypted). */
  tobytes(...args: any[]): any;
  /** Document.update_object compatibility entry (case l4-document-update-object). */
  updateObject(xref: any, text: any, page?: any): any;
  /** Document.update_stream compatibility entry (case l4-document-update-stream). */
  updateStream(xref?: any, stream?: any, new_?: any, compress?: any): any;
  /** Document.version_count compatibility entry (case l2-document-version-count). */
  versionCount: any;
  /** Document.write compatibility entry (case l2-document-write). */
  write(garbage?: any, clean?: any, deflate?: any, deflate_images?: any, deflate_fonts?: any, incremental?: any, ascii?: any, expand?: any, linear?: any, no_new_id?: any, appearance?: any, pretty?: any, encryption?: any, permissions?: any, owner_pw?: any, user_pw?: any, preserve_metadata?: any, use_objstms?: any, compression_effort?: any, raise_on_repair?: any, reproducible?: any): any;
  /** Document.xref compatibility entry (case l2-document-xref). */
  xref: any;
  /** Document.xref_copy compatibility entry (case l4-document-xref-copy). */
  xrefCopy(source: any, target: any, keep?: any): any;
  /** Document.xref_get_key compatibility entry (case l2-document-xref-get-key). */
  xrefGetKey(xref: any, key: any): any;
  /** Document.xref_get_keys compatibility entry (case l2-document-xref-get-keys). */
  xrefGetKeys(xref: any): any;
  /** Document.xref_is_font compatibility entry (case l1-document-xref-is-font). */
  xrefIsFont(xref: any): any;
  /** Document.xref_is_image compatibility entry (case l3-doc-xref-is-image). */
  xrefIsImage(xref: any): any;
  /** Document.xref_is_stream compatibility entry (case l2-document-xref-is-stream). */
  xrefIsStream(xref?: any): any;
  /** Document.xref_is_xobject compatibility entry (case l2-document-xref-is-xobject). */
  xrefIsXobject(xref: any): any;
  /** Document.xref_length compatibility entry (case l2-document-xref-length). */
  xrefLength(): any;
  /** Document.xref_object compatibility entry (case l2-document-xref-object). */
  xrefObject(xref: any, compressed?: any, ascii?: any): any;
  /** Document.xref_set_key compatibility entry (case l4-document-xref-set-key). */
  xrefSetKey(xref: any, key: any, value: any): any;
  /** Document.xref_stream compatibility entry (case l2-document-xref-stream). */
  xrefStream(xref: any): any;
  /** Document.xref_stream_raw compatibility entry (case l2-document-xref-stream-raw). */
  xrefStreamRaw(xref: any): any;
  /** Document.xref_xml_metadata compatibility entry (case l2-document-xref-xml-metadata). */
  xrefXmlMetadata(): any;
}

export declare class Page {
  /** Page.__eq__ compatibility entry (case l2-page-eq). */
  eq(value: any): any;
  /** Page.__ge__ compatibility entry (case l2-page-ge). */
  ge(value: any): any;
  /** Page.__gt__ compatibility entry (case l2-page-gt). */
  gt(value: any): any;
  /** Page.__init__ compatibility entry (case l2-page-init). */
  constructor(page: any, document: any);
  /** Page.__le__ compatibility entry (case l2-page-le). */
  le(value: any): any;
  /** Page.__lt__ compatibility entry (case l2-page-lt). */
  lt(value: any): any;
  /** Page.__ne__ compatibility entry (case l2-page-ne). */
  ne(value: any): any;
  /** Page.add_caret_annot compatibility entry (case L4.add_caret_annot.basic). */
  addCaretAnnot(point: any): any;
  /** Page.add_circle_annot compatibility entry (case L4.add_circle_annot.basic). */
  addCircleAnnot(rect: any): any;
  /** Page.add_file_annot compatibility entry (case L4.add_file_annot.basic). */
  addFileAnnot(point: any, buffer_: any, filename: any, ufilename?: any, desc?: any, icon?: any): any;
  /** Page.add_freetext_annot compatibility entry (case L4.add_freetext_annot.basic). */
  addFreetextAnnot(rect: any, text: any, fontsize?: any, fontname?: any, text_color?: any, fill_color?: any, border_color?: any, border_width?: any, dashes?: any, callout?: any, line_end?: any, opacity?: any, align?: any, rotate?: any, richtext?: any, style?: any): any;
  /** Page.add_highlight_annot compatibility entry (case L4.add_highlight_annot.quads). */
  addHighlightAnnot(quads?: any, start?: any, stop?: any, clip?: any): any;
  /** Page.add_ink_annot compatibility entry (case L4.add_ink_annot.basic). */
  addInkAnnot(handwriting: any): any;
  /** Page.add_line_annot compatibility entry (case L4.add_line_annot.basic). */
  addLineAnnot(p1: any, p2: any): any;
  /** Page.add_polygon_annot compatibility entry (case L4.add_polygon_annot.basic). */
  addPolygonAnnot(points: any): any;
  /** Page.add_polyline_annot compatibility entry (case L4.add_polyline_annot.basic). */
  addPolylineAnnot(points: any): any;
  /** Page.add_rect_annot compatibility entry (case L4.add_rect_annot.basic). */
  addRectAnnot(rect: any): any;
  /** Page.add_redact_annot compatibility entry (case L4.add_redact_annot.basic). */
  addRedactAnnot(quad: any, text?: any, fontname?: any, fontsize?: any, align?: any, fill?: any, text_color?: any, cross_out?: any): any;
  /** Page.add_squiggly_annot compatibility entry (case L4.add_squiggly_annot.quads). */
  addSquigglyAnnot(quads?: any, start?: any, stop?: any, clip?: any): any;
  /** Page.add_stamp_annot compatibility entry (case L4.add_stamp_annot.basic). */
  addStampAnnot(rect: any, stamp?: any): any;
  /** Page.add_strikeout_annot compatibility entry (case L4.add_strikeout_annot.quads). */
  addStrikeoutAnnot(quads?: any, start?: any, stop?: any, clip?: any): any;
  /** Page.add_text_annot compatibility entry (case L4.add_text_annot.basic). */
  addTextAnnot(point: any, text: any, icon?: any): any;
  /** Page.add_underline_annot compatibility entry (case L4.add_underline_annot.quads). */
  addUnderlineAnnot(quads?: any, start?: any, stop?: any, clip?: any): any;
  /** Page.add_widget compatibility entry (case L4.widget.add). */
  addWidget(widget: any): any;
  /** Page.annot_names compatibility entry (case L4.annot_names.list). */
  annotNames(): any;
  /** Page.annot_xrefs compatibility entry (case l4-page-annot-xrefs). */
  annotXrefs(): any;
  /** Page.annots compatibility entry (case L4.annots.list). */
  annots(types?: any): any;
  /** Page.apply_redactions compatibility entry (case L4.apply_redactions.text). */
  applyRedactions(images?: any, graphics?: any, text?: any): any;
  /** Page.artbox compatibility entry (case l2.page.boxes). */
  artbox: any;
  /** Page.bleedbox compatibility entry (case l2-page-bleedbox). */
  bleedbox: any;
  /** Page.bound compatibility entry (case l2-page-bound). */
  bound(): any;
  /** Page.clean_contents compatibility entry (case L4.annot.appearance_state). */
  cleanContents(sanitize?: any): any;
  /** Page.clip_to_rect compatibility entry (case l2-page-clip-to-rect). */
  clipToRect(rect: any): any;
  /** Page.cluster_drawings compatibility entry (case l4-page-cluster-drawings). */
  clusterDrawings(clip?: any, drawings?: any, x_tolerance?: any, y_tolerance?: any, final_filter?: any): any;
  /** Page.cropbox compatibility entry (case l2-page-cropbox). */
  cropbox: any;
  /** Page.cropbox_position compatibility entry (case l2-page-cropbox-position). */
  cropboxPosition: any;
  /** Page.delete_annot compatibility entry (case L4.delete_annot.basic). */
  deleteAnnot(annot: any): any;
  /** Page.delete_image compatibility entry (case l4-page-delete-image). */
  deleteImage(xref: any): any;
  /** Page.delete_link compatibility entry (case l2.links.delete). */
  deleteLink(linkdict: any): any;
  /** Page.delete_widget compatibility entry (case l4-page-delete-widget). */
  deleteWidget(widget: any): any;
  /** Page.derotation_matrix compatibility entry (case l2-page-derotation-matrix). */
  derotationMatrix: any;
  /** Page.draw_bezier compatibility entry (case l4-page-draw-bezier). */
  drawBezier(p1: any, p2: any, p3: any, p4: any, color?: any, fill?: any, dashes?: any, width?: any, morph?: any, closePath?: any, lineCap?: any, lineJoin?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_circle compatibility entry (case L4.page.draw_shapes). */
  drawCircle(center: any, radius: any, color?: any, fill?: any, morph?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_curve compatibility entry (case l4-page-draw-curve). */
  drawCurve(p1: any, p2: any, p3: any, color?: any, fill?: any, dashes?: any, width?: any, morph?: any, closePath?: any, lineCap?: any, lineJoin?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_line compatibility entry (case L4.page.draw_shapes). */
  drawLine(p1: any, p2: any, color?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, overlay?: any, morph?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_oval compatibility entry (case l4-page-draw-oval). */
  drawOval(rect: any, color?: any, fill?: any, dashes?: any, morph?: any, width?: any, lineCap?: any, lineJoin?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_polyline compatibility entry (case l4-page-draw-polyline). */
  drawPolyline(points: any, color?: any, fill?: any, dashes?: any, width?: any, morph?: any, lineCap?: any, lineJoin?: any, overlay?: any, closePath?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_quad compatibility entry (case l4-page-draw-quad). */
  drawQuad(quad: any, color?: any, fill?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, morph?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_rect compatibility entry (case L4.page.draw_shapes). */
  drawRect(rect: any, color?: any, fill?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, morph?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any, radius?: any): any;
  /** Page.draw_sector compatibility entry (case l4-page-draw-sector). */
  drawSector(center: any, point: any, beta: any, color?: any, fill?: any, dashes?: any, fullSector?: any, morph?: any, width?: any, closePath?: any, lineCap?: any, lineJoin?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_squiggle compatibility entry (case l4-page-draw-squiggle). */
  drawSquiggle(p1: any, p2: any, breadth?: any, color?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, overlay?: any, morph?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.draw_zigzag compatibility entry (case l4-page-draw-zigzag). */
  drawZigzag(p1: any, p2: any, breadth?: any, color?: any, dashes?: any, width?: any, lineCap?: any, lineJoin?: any, overlay?: any, morph?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.extend_textpage compatibility entry (case L1.extend_textpage.default). */
  extendTextpage(tpage: any, flags?: any, matrix?: any): any;
  /** Page.find_tables compatibility entry (case L4.find_tables.hit). */
  findTables(...args: any[]): any;
  /** Page.first_annot compatibility entry (case L4.first_annot.chain). */
  firstAnnot: any;
  /** Page.first_link compatibility entry (case l2.links.first_next). */
  firstLink: any;
  /** Page.first_widget compatibility entry (case l4-page-first-widget). */
  firstWidget: any;
  /** Page.get_bboxlog compatibility entry (case l2-page-get-bboxlog). */
  getBboxlog(layers?: any): any;
  /** Page.get_cdrawings compatibility entry (case l4-page-get-cdrawings). */
  getCdrawings(extended?: any, callback?: any, method?: any): any;
  /** Page.get_contents compatibility entry (case l2-page-get-contents). */
  getContents(): any;
  /** Page.get_displaylist compatibility entry (case l2-page-get-displaylist). */
  getDisplaylist(annots?: any): any;
  /** Page.get_drawings compatibility entry (case L4.get_drawings.default). */
  getDrawings(extended?: any): any;
  /** Page.get_fonts compatibility entry (case L4.insert_font.basic). */
  getFonts(full?: any): any;
  /** Page.get_image_bbox compatibility entry (case l3-page-get-image-bbox). */
  getImageBbox(name: any, transform?: any): any;
  /** Page.get_image_info compatibility entry (case l3-page-get-image-info). */
  getImageInfo(hashes?: any, xrefs?: any): any;
  /** Page.get_image_rects compatibility entry (case l3-page-get-image-rects). */
  getImageRects(name: any, transform?: any): any;
  /** Page.get_images compatibility entry (case l3-page-get-images). */
  getImages(full?: any): any;
  /** Page.get_label compatibility entry (case l2-page-get-label). */
  getLabel(): any;
  /** Page.get_layout compatibility entry (case L1.get_layout.default). */
  getLayout(...args: any[]): any;
  /** Page.get_links compatibility entry (case l2-page-links). */
  getLinks(): any;
  /** Page.get_oc_items compatibility entry (case l2-page-get-oc-items). */
  getOcItems(): any;
  /** Page.get_pixmap compatibility entry (case l3-page-get-pixmap-alpha). */
  getPixmap(matrix?: any, dpi?: any, colorspace?: any, clip?: any, alpha?: any, annots?: any): any;
  /** Page.get_svg_image compatibility entry (case l3-page-get-svg-image). */
  getSvgImage(matrix?: any, text_as_path?: any): any;
  /** Page.get_text compatibility entry (case L1.get_text.bogus_kw). */
  getText(...args: any[]): any;
  /** Page.get_text_blocks compatibility entry (case L1.get_text.blocks). */
  getTextBlocks(...args: any[]): any;
  /** Page.get_text_selection compatibility entry (case L1.get_text_selection.clip). */
  getTextSelection(...args: any[]): any;
  /** Page.get_text_words compatibility entry (case L1.get_page_text.words_clip). */
  getTextWords(...args: any[]): any;
  /** Page.get_textbox compatibility entry (case L1.get_textbox.empty). */
  getTextbox(rect: any, textpage?: any): any;
  /** Page.get_textpage compatibility entry (case L1.get_textpage.bogus_kw). */
  getTextpage(clip?: any, flags?: any, matrix?: any): any;
  /** Page.get_textpage_ocr compatibility entry (case l1-page-get-textpage-ocr). */
  getTextpageOcr(...args: any[]): any;
  /** Page.get_texttrace compatibility entry (case L1.get_texttrace.default). */
  getTexttrace(): any;
  /** Page.get_xobjects compatibility entry (case l2-page-get-xobjects). */
  getXobjects(): any;
  /** Page.insert_font compatibility entry (case L4.insert_font.basic). */
  insertFont(fontname?: any, fontfile?: any, fontbuffer?: any, set_simple?: any, wmode?: any, encoding?: any): any;
  /** Page.insert_htmlbox compatibility entry (case l4-page-insert-htmlbox). */
  insertHtmlbox(rect: any, text: any, css?: any, scale_low?: any, archive?: any, rotate?: any, oc?: any, opacity?: any, overlay?: any, _scale_word_width?: any, _verbose?: any): any;
  /** Page.insert_image compatibility entry (case L4.insert_image.pixmap). */
  insertImage(rect: any, alpha?: any, filename?: any, height?: any, keep_proportion?: any, mask?: any, oc?: any, overlay?: any, pixmap?: any, rotate?: any, stream?: any, width?: any, xref?: any): any;
  /** Page.insert_link compatibility entry (case l2.links.insert_bad). */
  insertLink(lnk: any, mark?: any): any;
  /** Page.insert_text compatibility entry (case L4.insert_text.basic). */
  insertText(point: any, text: any, fontsize?: any, lineheight?: any, fontname?: any, fontfile?: any, set_simple?: any, encoding?: any, color?: any, fill?: any, border_width?: any, miter_limit?: any, render_mode?: any, rotate?: any, morph?: any, overlay?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Page.insert_textbox compatibility entry (case L4.insert_textbox.align). */
  insertTextbox(rect: any, buffer: any, align?: any, border_width?: any, color?: any, encoding?: any, expandtabs?: any, fill_opacity?: any, fill?: any, fontfile?: any, fontname?: any, fontsize?: any, lineheight?: any, miter_limit?: any, morph?: any, oc?: any, overlay?: any, render_mode?: any, rotate?: any, set_simple?: any, stroke_opacity?: any): any;
  /** Page.is_wrapped compatibility entry (case l2-page-is-wrapped). */
  isWrapped: any;
  /** Page.language compatibility entry (case l2-page-language). */
  language: any;
  /** Page.links compatibility entry (case l2.links.iter). */
  links(kinds?: any): any;
  /** Page.load_annot compatibility entry (case L4.load_annot.basic). */
  loadAnnot(ident: any): any;
  /** Page.load_links compatibility entry (case l4-page-load-links). */
  loadLinks(): any;
  /** Page.load_widget compatibility entry (case L4.load_widget.delete). */
  loadWidget(xref: any): any;
  /** Page.mediabox compatibility entry (case l2.page.mediabox). */
  mediabox: any;
  /** Page.mediabox_size compatibility entry (case l2-page-mediabox-size). */
  mediaboxSize: any;
  /** Page.new_shape compatibility entry (case L4.new_shape.bezier). */
  newShape(): any;
  /** Page.read_contents compatibility entry (case l2-page-read-contents). */
  readContents(): any;
  /** Page.recolor compatibility entry (case l2-page-recolor). */
  recolor(components?: any): any;
  /** Page.rect compatibility entry (case l2.page.rect). */
  rect: any;
  /** Page.refresh compatibility entry (case l2-page-refresh). */
  refresh(): any;
  /** Page.remove_rotation compatibility entry (case l2.page.remove_rotation). */
  removeRotation(): any;
  /** Page.replace_image compatibility entry (case l4-page-replace-image). */
  replaceImage(xref: any, filename?: any, pixmap?: any, stream?: any): any;
  /** Page.rotation compatibility entry (case l2-page-rotation). */
  rotation: any;
  /** Page.rotation_matrix compatibility entry (case l2-page-rotation-matrix). */
  rotationMatrix: any;
  /** Page.run compatibility entry (case l2-page-run). */
  run(dw: any, m: any): any;
  /** Page.search_for compatibility entry (case L1.search_for.clip_miss). */
  searchFor(text: any, clip?: any, quads?: any, flags?: any, textpage?: any): any;
  /** Page.set_artbox compatibility entry (case l4-page-set-artbox). */
  setArtbox(rect: any): any;
  /** Page.set_bleedbox compatibility entry (case l4-page-set-bleedbox). */
  setBleedbox(rect: any): any;
  /** Page.set_contents compatibility entry (case l4-page-set-contents). */
  setContents(xref: any): any;
  /** Page.set_cropbox compatibility entry (case l2.page.set_cropbox_bad). */
  setCropbox(rect: any): any;
  /** Page.set_language compatibility entry (case l4-page-set-language). */
  setLanguage(language?: any): any;
  /** Page.set_mediabox compatibility entry (case l2.page.set_mediabox). */
  setMediabox(rect: any): any;
  /** Page.set_rotation compatibility entry (case l2.page.set_rotation_180). */
  setRotation(rotation: any): any;
  /** Page.set_trimbox compatibility entry (case l4-page-set-trimbox). */
  setTrimbox(rect: any): any;
  /** Page.show_pdf_page compatibility entry (case l4-page-show-pdf-page). */
  showPdfPage(rect: any, docsrc: any, pno?: any, keep_proportion?: any, overlay?: any, oc?: any, rotate?: any, clip?: any): any;
  /** Page.transformation_matrix compatibility entry (case l2-page-transformation-matrix). */
  transformationMatrix: any;
  /** Page.trimbox compatibility entry (case l2-page-trimbox). */
  trimbox: any;
  /** Page.update_link compatibility entry (case l4-page-update-link). */
  updateLink(lnk: any): any;
  /** Page.widgets compatibility entry (case L4.widgets.list). */
  widgets(types?: any): any;
  /** Page.wrap_contents compatibility entry (case l2-page-wrap-contents). */
  wrapContents(): any;
  /** Page.write_text compatibility entry (case l4-page-write-text). */
  writeText(rect?: any, writers?: any, overlay?: any, color?: any, opacity?: any, keep_proportion?: any, rotate?: any, oc?: any): any;
  /** Page.xref compatibility entry (case l2-page-xref). */
  xref: any;
}

export declare class TextPage {
  /** TextPage.__eq__ compatibility entry (case l1-textpage-eq). */
  eq(value: any): any;
  /** TextPage.__ge__ compatibility entry (case l1-textpage-ge). */
  ge(value: any): any;
  /** TextPage.__gt__ compatibility entry (case l1-textpage-gt). */
  gt(value: any): any;
  /** TextPage.__init__ compatibility entry (case l1-textpage-init). */
  constructor(...args: any[]);
  /** TextPage.__le__ compatibility entry (case l1-textpage-le). */
  le(value: any): any;
  /** TextPage.__lt__ compatibility entry (case l1-textpage-lt). */
  lt(value: any): any;
  /** TextPage.__ne__ compatibility entry (case l1-textpage-ne). */
  ne(value: any): any;
  /** TextPage.extractBLOCKS compatibility entry (case L1.textpage.extractBLOCKS). */
  extractBLOCKS(): any;
  /** TextPage.extractDICT compatibility entry (case L1.textpage.extractDICT). */
  extractDICT(cb?: any, sort?: any): any;
  /** TextPage.extractHTML compatibility entry (case L1.textpage.extractHTML). */
  extractHTML(): any;
  /** TextPage.extractIMGINFO compatibility entry (case L1.textpage.extractIMGINFO). */
  extractIMGINFO(hashes?: any): any;
  /** TextPage.extractJSON compatibility entry (case L1.textpage.extractJSON). */
  extractJSON(cb?: any, sort?: any): any;
  /** TextPage.extractRAWDICT compatibility entry (case L1.textpage.extractRAWDICT). */
  extractRAWDICT(cb?: any, sort?: any): any;
  /** TextPage.extractRAWJSON compatibility entry (case L1.textpage.extractRAWJSON). */
  extractRAWJSON(cb?: any, sort?: any): any;
  /** TextPage.extractSelection compatibility entry (case L1.textpage.extractSelection). */
  extractSelection(pointa: any, pointb: any): any;
  /** TextPage.extractTEXT compatibility entry (case L1.textpage.empty_text). */
  extractTEXT(sort?: any): any;
  /** TextPage.extractText compatibility entry (case L1.textpage.extractText.alias). */
  extractText(sort?: any): any;
  /** TextPage.extractTextbox compatibility entry (case L1.textpage.extractTextbox). */
  extractTextbox(rect: any): any;
  /** TextPage.extractWORDS compatibility entry (case L1.textpage.extractWORDS). */
  extractWORDS(delimiters?: any): any;
  /** TextPage.extractXHTML compatibility entry (case L1.textpage.extractXHTML). */
  extractXHTML(): any;
  /** TextPage.extractXML compatibility entry (case L1.textpage.extractXML). */
  extractXML(): any;
  /** TextPage.poolsize compatibility entry (case L1.textpage.poolsize). */
  poolsize(): any;
  /** TextPage.rect compatibility entry (case L1.textpage.rect). */
  rect: any;
  /** TextPage.search compatibility entry (case L1.textpage.search_hit_max). */
  search(needle: any, hit_max?: any, quads?: any): any;
}

export declare class Pixmap {
  /** Pixmap.__eq__ compatibility entry (case l3-pixmap-eq). */
  eq(value: any): any;
  /** Pixmap.__ge__ compatibility entry (case l3-pixmap-ge). */
  ge(value: any): any;
  /** Pixmap.__gt__ compatibility entry (case l3-pixmap-gt). */
  gt(value: any): any;
  /** Pixmap.__init__ compatibility entry (case l3-pixmap-init-blank). */
  constructor(...args: any[]);
  /** Pixmap.__le__ compatibility entry (case l3-pixmap-le). */
  le(value: any): any;
  /** Pixmap.__len__ compatibility entry (case l3-pixmap-len). */
  len(): any;
  /** Pixmap.__lt__ compatibility entry (case l3-pixmap-order). */
  lt(value: any): any;
  /** Pixmap.__ne__ compatibility entry (case l3-pixmap-ne). */
  ne(value: any): any;
  /** Pixmap.alpha compatibility entry (case l3-doc-get-page-pixmap). */
  alpha: any;
  /** Pixmap.clear_with compatibility entry (case l3-pixmap-clear-with). */
  clearWith(value?: any, bbox?: any): any;
  /** Pixmap.color_count compatibility entry (case l3-pixmap-color-count). */
  colorCount(colors?: any, clip?: any): any;
  /** Pixmap.color_topusage compatibility entry (case l3-pixmap-color-topusage). */
  colorTopusage(clip?: any): any;
  /** Pixmap.colorspace compatibility entry (case l3-pixmap-colorspace). */
  colorspace: any;
  /** Pixmap.copy compatibility entry (case l3-pixmap-copy). */
  copy(src: any, bbox: any): any;
  /** Pixmap.digest compatibility entry (case l3-pixmap-digest). */
  digest: any;
  /** Pixmap.gamma_with compatibility entry (case l3-pixmap-gamma-with). */
  gammaWith(gamma: any): any;
  /** Pixmap.h compatibility entry (case l3-pixmap-size). */
  h: any;
  /** Pixmap.height compatibility entry (case l3-pixmap-size). */
  height: any;
  /** Pixmap.invert_irect compatibility entry (case l3-pixmap-invert-irect). */
  invertIrect(bbox?: any): any;
  /** Pixmap.irect compatibility entry (case l3-pixmap-layout). */
  irect: any;
  /** Pixmap.is_monochrome compatibility entry (case l3-pixmap-is-monochrome). */
  isMonochrome: any;
  /** Pixmap.is_unicolor compatibility entry (case l3-pixmap-unicolor). */
  isUnicolor: any;
  /** Pixmap.n compatibility entry (case l3-pixmap-layout). */
  n: any;
  /** Pixmap.pdfocr_save compatibility entry (case l3-pixmap-pdfocr-save). */
  pdfocrSave(filename: any, compress?: any, language?: any, tessdata?: any): any;
  /** Pixmap.pdfocr_tobytes compatibility entry (case l3-pixmap-pdfocr-tobytes). */
  pdfocrTobytes(compress?: any, language?: any, tessdata?: any): any;
  /** Pixmap.pil_image compatibility entry (case l3-pixmap-pil). */
  pilImage(): any;
  /** Pixmap.pil_save compatibility entry (case l3-pixmap-pil-save). */
  pilSave(...args: any[]): any;
  /** Pixmap.pil_tobytes compatibility entry (case l3-pixmap-pil-tobytes). */
  pilTobytes(...args: any[]): any;
  /** Pixmap.pixel compatibility entry (case l3-pixmap-pixel). */
  pixel(x: any, y: any): any;
  /** Pixmap.samples compatibility entry (case l3-pixmap-samples). */
  samples: any;
  /** Pixmap.samples_mv compatibility entry (case l3-pixmap-samples-mv). */
  samplesMv: any;
  /** Pixmap.samples_ptr compatibility entry (case l3-pixmap-samples-ptr). */
  samplesPtr: any;
  /** Pixmap.save compatibility entry (case l3-pixmap-save). */
  save(filename: any, output?: any, jpg_quality?: any): any;
  /** Pixmap.set_alpha compatibility entry (case l3-pixmap-set-alpha). */
  setAlpha(alphavalues?: any, premultiply?: any, opaque?: any, matte?: any): any;
  /** Pixmap.set_dpi compatibility entry (case l3-pixmap-resolution). */
  setDpi(xres: any, yres: any): any;
  /** Pixmap.set_origin compatibility entry (case l3-pixmap-set-origin). */
  setOrigin(x: any, y: any): any;
  /** Pixmap.set_pixel compatibility entry (case l3-pixmap-set-pixel). */
  setPixel(x: any, y: any, color: any): any;
  /** Pixmap.set_rect compatibility entry (case l3-pixmap-set-rect). */
  setRect(bbox: any, color: any): any;
  /** Pixmap.shrink compatibility entry (case l3-pixmap-shrink). */
  shrink(factor: any): any;
  /** Pixmap.size compatibility entry (case l3-pixmap-size). */
  size: any;
  /** Pixmap.stride compatibility entry (case l3-doc-get-page-pixmap). */
  stride: any;
  /** Pixmap.tint_with compatibility entry (case l3-pixmap-tint-with). */
  tintWith(black: any, white: any): any;
  /** Pixmap.tobytes compatibility entry (case l3-pixmap-tobytes-bad). */
  tobytes(output?: any, jpg_quality?: any): any;
  /** Pixmap.w compatibility entry (case l3-pixmap-size). */
  w: any;
  /** Pixmap.warp compatibility entry (case l3-pixmap-warp). */
  warp(quad: any, width: any, height: any): any;
  /** Pixmap.width compatibility entry (case l3-pixmap-72). */
  width: any;
  /** Pixmap.x compatibility entry (case l3-pixmap-x). */
  x: any;
  /** Pixmap.xres compatibility entry (case l3-pixmap-xres). */
  xres: any;
  /** Pixmap.y compatibility entry (case l3-pixmap-y). */
  y: any;
  /** Pixmap.yres compatibility entry (case l3-pixmap-yres). */
  yres: any;
}

export declare class Annot {
  /** Annot.__bool__ compatibility entry (case l4-annot-bool). */
  bool(): any;
  /** Annot.__eq__ compatibility entry (case l4-annot-eq). */
  eq(value: any): any;
  /** Annot.__ge__ compatibility entry (case l4-annot-ge). */
  ge(value: any): any;
  /** Annot.__gt__ compatibility entry (case l4-annot-gt). */
  gt(value: any): any;
  /** Annot.__init__ compatibility entry (case l4-annot-init). */
  constructor(annot: any);
  /** Annot.__le__ compatibility entry (case l4-annot-le). */
  le(value: any): any;
  /** Annot.__lt__ compatibility entry (case l4-annot-lt). */
  lt(value: any): any;
  /** Annot.__ne__ compatibility entry (case l4-annot-ne). */
  ne(value: any): any;
  /** Annot.apn_bbox compatibility entry (case l4-annot-apn-bbox). */
  apnBbox: any;
  /** Annot.apn_matrix compatibility entry (case l4-annot-apn-matrix). */
  apnMatrix: any;
  /** Annot.blendmode compatibility entry (case L4.annot.appearance_state). */
  blendmode: any;
  /** Annot.border compatibility entry (case L4.annot.border). */
  border: any;
  /** Annot.clean_contents compatibility entry (case l4-annot-clean-contents). */
  cleanContents(sanitize?: any): any;
  /** Annot.colors compatibility entry (case L4.annot.colors). */
  colors: any;
  /** Annot.delete_responses compatibility entry (case l4-annot-delete-responses). */
  deleteResponses(): any;
  /** Annot.file_info compatibility entry (case L4.annot.file_roundtrip). */
  fileInfo: any;
  /** Annot.flags compatibility entry (case L4.annot.flags). */
  flags: any;
  /** Annot.get_file compatibility entry (case l4-annot-get-file). */
  getFile(): any;
  /** Annot.get_oc compatibility entry (case l4-annot-get-oc). */
  getOc(): any;
  /** Annot.get_parent compatibility entry (case l4-annot-get-parent). */
  getParent(): any;
  /** Annot.get_pixmap compatibility entry (case L4.annot.get_pixmap). */
  getPixmap(matrix?: any, dpi?: any, colorspace?: any, alpha?: any): any;
  /** Annot.get_sound compatibility entry (case l4-annot-get-sound). */
  getSound(): any;
  /** Annot.get_text compatibility entry (case l4-annot-get-text). */
  getText(...args: any[]): any;
  /** Annot.get_textbox compatibility entry (case L4.annot.get_textbox). */
  getTextbox(...args: any[]): any;
  /** Annot.get_textpage compatibility entry (case L4.annot.get_textpage). */
  getTextpage(clip?: any, flags?: any): any;
  /** Annot.has_popup compatibility entry (case L4.annot.popup). */
  hasPopup: any;
  /** Annot.info compatibility entry (case L4.annot.info). */
  info: any;
  /** Annot.irt_xref compatibility entry (case L4.annot.irt). */
  irtXref: any;
  /** Annot.is_open compatibility entry (case l4-annot-is-open). */
  isOpen: any;
  /** Annot.language compatibility entry (case l4-annot-language). */
  language: any;
  /** Annot.line_ends compatibility entry (case l4-annot-line-ends). */
  lineEnds: any;
  /** Annot.next compatibility entry (case l4-annot-next). */
  next: any;
  /** Annot.opacity compatibility entry (case L4.annot.opacity). */
  opacity: any;
  /** Annot.popup_rect compatibility entry (case l4-annot-popup-rect). */
  popupRect: any;
  /** Annot.popup_xref compatibility entry (case l4-annot-popup-xref). */
  popupXref: any;
  /** Annot.rect compatibility entry (case L4.annot.rect). */
  rect: any;
  /** Annot.rect_delta compatibility entry (case l4-annot-rect-delta). */
  rectDelta: any;
  /** Annot.rotation compatibility entry (case L4.annot.rotation). */
  rotation: any;
  /** Annot.set_apn_bbox compatibility entry (case l4-annot-set-apn-bbox). */
  setApnBbox(bbox: any): any;
  /** Annot.set_apn_matrix compatibility entry (case l4-annot-set-apn-matrix). */
  setApnMatrix(matrix: any): any;
  /** Annot.set_blendmode compatibility entry (case l4-annot-set-blendmode). */
  setBlendmode(blend_mode: any): any;
  /** Annot.set_border compatibility entry (case l4-annot-set-border). */
  setBorder(border?: any, width?: any, style?: any, dashes?: any, clouds?: any): any;
  /** Annot.set_colors compatibility entry (case L4.add_rect_annot.basic). */
  setColors(colors?: any, stroke?: any, fill?: any): any;
  /** Annot.set_flags compatibility entry (case l4-annot-set-flags). */
  setFlags(flags: any): any;
  /** Annot.set_info compatibility entry (case l4-annot-set-info). */
  setInfo(info?: any, content?: any, title?: any, creationDate?: any, modDate?: any, subject?: any): any;
  /** Annot.set_irt_xref compatibility entry (case l4-annot-set-irt-xref). */
  setIrtXref(xref: any): any;
  /** Annot.set_language compatibility entry (case l4-annot-set-language). */
  setLanguage(language?: any): any;
  /** Annot.set_line_ends compatibility entry (case l4-annot-set-line-ends). */
  setLineEnds(start: any, end: any): any;
  /** Annot.set_name compatibility entry (case l4-annot-set-name). */
  setName(name: any): any;
  /** Annot.set_oc compatibility entry (case l4-annot-set-oc). */
  setOc(oc?: any): any;
  /** Annot.set_opacity compatibility entry (case l4-annot-set-opacity). */
  setOpacity(opacity: any): any;
  /** Annot.set_open compatibility entry (case l4-annot-set-open). */
  setOpen(is_open: any): any;
  /** Annot.set_popup compatibility entry (case l4-annot-set-popup). */
  setPopup(rect: any): any;
  /** Annot.set_rect compatibility entry (case l4-annot-set-rect). */
  setRect(rect: any): any;
  /** Annot.set_rotation compatibility entry (case l4-annot-set-rotation). */
  setRotation(rotate?: any): any;
  /** Annot.type compatibility entry (case L4.annot.type). */
  type: any;
  /** Annot.update compatibility entry (case L4.annot.update). */
  update(blend_mode?: any, opacity?: any, fontsize?: any, fontname?: any, text_color?: any, border_color?: any, fill_color?: any, cross_out?: any, rotate?: any): any;
  /** Annot.update_file compatibility entry (case l4-annot-update-file). */
  updateFile(buffer_?: any, filename?: any, ufilename?: any, desc?: any): any;
  /** Annot.update_timing_test compatibility entry (case l4-annot-update-timing-test). */
  updateTimingTest(): any;
  /** Annot.vertices compatibility entry (case l4-annot-vertices). */
  vertices: any;
  /** Annot.xref compatibility entry (case l4-annot-xref). */
  xref: any;
}

export declare class Widget {
  /** Widget.__eq__ compatibility entry (case l4-widget-eq). */
  eq(value: any): any;
  /** Widget.__ge__ compatibility entry (case l4-widget-ge). */
  ge(value: any): any;
  /** Widget.__gt__ compatibility entry (case l4-widget-gt). */
  gt(value: any): any;
  /** Widget.__init__ compatibility entry (case L4.widget.add). */
  constructor();
  /** Widget.__le__ compatibility entry (case l4-widget-le). */
  le(value: any): any;
  /** Widget.__lt__ compatibility entry (case l4-widget-lt). */
  lt(value: any): any;
  /** Widget.__ne__ compatibility entry (case l4-widget-ne). */
  ne(value: any): any;
  /** Widget.button_states compatibility entry (case l4-widget-button-states). */
  buttonStates(): any;
  /** Widget.next compatibility entry (case l4-widget-next). */
  next: any;
  /** Widget.on_state compatibility entry (case l4-widget-on-state). */
  onState(): any;
  /** Widget.reset compatibility entry (case L4.widget.reset). */
  reset(): any;
  /** Widget.update compatibility entry (case L4.widget.update_text). */
  update(sync_flags?: any): any;
}

export declare class Link {
  /** Link.__eq__ compatibility entry (case l2-link-eq). */
  eq(value: any): any;
  /** Link.__ge__ compatibility entry (case l2-link-ge). */
  ge(value: any): any;
  /** Link.__gt__ compatibility entry (case l2-link-gt). */
  gt(value: any): any;
  /** Link.__init__ compatibility entry (case l2-link-init). */
  constructor(this_: any);
  /** Link.__le__ compatibility entry (case l2-link-le). */
  le(value: any): any;
  /** Link.__lt__ compatibility entry (case l2-link-lt). */
  lt(value: any): any;
  /** Link.__ne__ compatibility entry (case l2-link-ne). */
  ne(value: any): any;
  /** Link.border compatibility entry (case l2-link-border). */
  border: any;
  /** Link.colors compatibility entry (case l2-link-colors). */
  colors: any;
  /** Link.dest compatibility entry (case l2-link-dest). */
  dest: any;
  /** Link.flags compatibility entry (case l2-link-flags). */
  flags: any;
  /** Link.is_external compatibility entry (case l2-link-is-external). */
  isExternal: any;
  /** Link.next compatibility entry (case l2-link-next). */
  next: any;
  /** Link.rect compatibility entry (case l2-link-rect). */
  rect: any;
  /** Link.set_border compatibility entry (case l2-link-set-border). */
  setBorder(border?: any, width?: any, dashes?: any, style?: any): any;
  /** Link.set_colors compatibility entry (case l2-link-set-colors). */
  setColors(colors?: any, stroke?: any, fill?: any): any;
  /** Link.set_flags compatibility entry (case l2-link-set-flags). */
  setFlags(flags: any): any;
  /** Link.uri compatibility entry (case l2-link-uri). */
  uri: any;
}

export declare class Shape {
  /** Shape.__eq__ compatibility entry (case l4-shape-eq). */
  eq(value: any): any;
  /** Shape.__ge__ compatibility entry (case l4-shape-ge). */
  ge(value: any): any;
  /** Shape.__gt__ compatibility entry (case l4-shape-gt). */
  gt(value: any): any;
  /** Shape.__init__ compatibility entry (case l4-shape-init). */
  constructor(page: any);
  /** Shape.__le__ compatibility entry (case l4-shape-le). */
  le(value: any): any;
  /** Shape.__lt__ compatibility entry (case l4-shape-lt). */
  lt(value: any): any;
  /** Shape.__ne__ compatibility entry (case l4-shape-ne). */
  ne(value: any): any;
  /** Shape.commit compatibility entry (case L4.shape.commit_overlay). */
  commit(overlay?: any): any;
  /** Shape.draw_bezier compatibility entry (case l4-shape-draw-bezier). */
  drawBezier(p1: any, p2: any, p3: any, p4: any): any;
  /** Shape.draw_circle compatibility entry (case l4-shape-draw-circle). */
  drawCircle(center: any, radius: any): any;
  /** Shape.draw_curve compatibility entry (case l4-shape-draw-curve). */
  drawCurve(p1: any, p2: any, p3: any): any;
  /** Shape.draw_line compatibility entry (case L4.new_shape.line). */
  drawLine(p1: any, p2: any): any;
  /** Shape.draw_oval compatibility entry (case l4-shape-draw-oval). */
  drawOval(tetra: any): any;
  /** Shape.draw_polyline compatibility entry (case l4-shape-draw-polyline). */
  drawPolyline(points: any): any;
  /** Shape.draw_quad compatibility entry (case l4-shape-draw-quad). */
  drawQuad(quad: any): any;
  /** Shape.draw_rect compatibility entry (case L4.new_shape.rect). */
  drawRect(rect: any, radius?: any): any;
  /** Shape.draw_sector compatibility entry (case l4-shape-draw-sector). */
  drawSector(center: any, point: any, beta: any, fullSector?: any): any;
  /** Shape.draw_squiggle compatibility entry (case l4-shape-draw-squiggle). */
  drawSquiggle(p1: any, p2: any, breadth?: any): any;
  /** Shape.draw_zigzag compatibility entry (case l4-shape-draw-zigzag). */
  drawZigzag(p1: any, p2: any, breadth?: any): any;
  /** Shape.finish compatibility entry (case L4.shape.finish_options). */
  finish(width?: any, color?: any, fill?: any, lineCap?: any, lineJoin?: any, dashes?: any, even_odd?: any, morph?: any, closePath?: any, fill_opacity?: any, stroke_opacity?: any, oc?: any): any;
  /** Shape.horizontal_angle compatibility entry (case l4-shape-horizontal-angle). */
  horizontalAngle(C: any, P: any): any;
  /** Shape.insert_text compatibility entry (case L4.shape.insert_text). */
  insertText(point: any, buffer: any, fontsize?: any, lineheight?: any, fontname?: any, fontfile?: any, set_simple?: any, encoding?: any, color?: any, fill?: any, render_mode?: any, border_width?: any, miter_limit?: any, rotate?: any, morph?: any, stroke_opacity?: any, fill_opacity?: any, oc?: any): any;
  /** Shape.insert_textbox compatibility entry (case l4-shape-insert-textbox). */
  insertTextbox(rect: any, buffer: any, align?: any, border_width?: any, color?: any, encoding?: any, expandtabs?: any, fill_opacity?: any, fill?: any, fontfile?: any, fontname?: any, fontsize?: any, lineheight?: any, miter_limit?: any, morph?: any, oc?: any, render_mode?: any, rotate?: any, set_simple?: any, stroke_opacity?: any): any;
  /** Shape.updateRect compatibility entry (case l4-shape-updaterect). */
  updateRect(x: any): any;
}

export declare class Rect {
  /** Rect.__abs__ compatibility entry (case p0-rect-abs). */
  abs(): any;
  /** Rect.__add__ compatibility entry (case p0-rect-add). */
  add(p: any): any;
  /** Rect.__bool__ compatibility entry (case p0-rect-bool). */
  bool(): any;
  /** Rect.__contains__ compatibility entry (case p0-rect-contains). */
  containsOp(x: any): any;
  /** Rect.__eq__ compatibility entry (case p0-rect-eq). */
  eq(rect: any): any;
  /** Rect.__ge__ compatibility entry (case p0-rect-ge). */
  ge(value: any): any;
  /** Rect.__getitem__ compatibility entry (case p0-rect-getitem). */
  getitem(i: any): any;
  /** Rect.__gt__ compatibility entry (case p0-rect-gt). */
  gt(value: any): any;
  /** Rect.__init__ compatibility entry (case p0-rect-init). */
  constructor(...args: any[]);
  /** Rect.__le__ compatibility entry (case p0-rect-le). */
  le(value: any): any;
  /** Rect.__len__ compatibility entry (case p0-rect-len). */
  len(): any;
  /** Rect.__lt__ compatibility entry (case p0-rect-lt). */
  lt(value: any): any;
  /** Rect.__mul__ compatibility entry (case p0-rect-mul). */
  mul(m: any): any;
  /** Rect.__ne__ compatibility entry (case p0-rect-ne). */
  ne(value: any): any;
  /** Rect.__neg__ compatibility entry (case p0-rect-neg). */
  neg(): any;
  /** Rect.__pos__ compatibility entry (case p0-rect-pos). */
  pos(): any;
  /** Rect.__setitem__ compatibility entry (case p0-rect-setitem). */
  setitem(i: any, v: any): any;
  /** Rect.__sub__ compatibility entry (case p0-rect-sub). */
  sub(p: any): any;
  /** Rect.__truediv__ compatibility entry (case p0-rect-truediv). */
  truediv(m: any): any;
  /** Rect.bl compatibility entry (case p0-rect-bl). */
  bl: any;
  /** Rect.bottom_left compatibility entry (case p0-rect-bottom-left). */
  bottomLeft: any;
  /** Rect.bottom_right compatibility entry (case p0-rect-bottom-right). */
  bottomRight: any;
  /** Rect.br compatibility entry (case p0-rect-br). */
  br: any;
  /** Rect.contains compatibility entry (case p0-rect-contains-2). */
  contains(x: any): any;
  /** Rect.get_area compatibility entry (case p0-rect-get-area). */
  getArea(...args: any[]): any;
  /** Rect.height compatibility entry (case p0-rect-height). */
  height: any;
  /** Rect.include_point compatibility entry (case p0-rect-include-point). */
  includePoint(p: any): any;
  /** Rect.include_rect compatibility entry (case p0-rect-include-rect). */
  includeRect(r: any): any;
  /** Rect.intersect compatibility entry (case p0-rect-intersect). */
  intersect(r: any): any;
  /** Rect.intersects compatibility entry (case p0-rect-intersects). */
  intersects(x: any): any;
  /** Rect.irect compatibility entry (case p0-rect-irect). */
  irect: any;
  /** Rect.is_empty compatibility entry (case p0-rect-is-empty). */
  isEmpty: any;
  /** Rect.is_infinite compatibility entry (case p0-rect-is-infinite). */
  isInfinite: any;
  /** Rect.is_valid compatibility entry (case p0-rect-is-valid). */
  isValid: any;
  /** Rect.morph compatibility entry (case p0-rect-morph). */
  morph(p: any, m: any): any;
  /** Rect.norm compatibility entry (case p0-rect-norm). */
  norm(): any;
  /** Rect.normalize compatibility entry (case p0-rect-normalize). */
  normalize(): any;
  /** Rect.quad compatibility entry (case p0-rect-quad). */
  quad: any;
  /** Rect.round compatibility entry (case p0-rect-round). */
  round(): any;
  /** Rect.tl compatibility entry (case p0-rect-tl). */
  tl: any;
  /** Rect.top_left compatibility entry (case p0-rect-top-left). */
  topLeft: any;
  /** Rect.top_right compatibility entry (case p0-rect-top-right). */
  topRight: any;
  /** Rect.torect compatibility entry (case p0-rect-torect). */
  torect(r: any): any;
  /** Rect.tr compatibility entry (case p0-rect-tr). */
  tr: any;
  /** Rect.transform compatibility entry (case p0-rect-transform). */
  transform(m: any): any;
  /** Rect.width compatibility entry (case p0-rect-width). */
  width: any;
}

export declare class IRect {
  /** IRect.__add__ compatibility entry (case p0-irect-add). */
  add(p: any): any;
  /** IRect.__contains__ compatibility entry (case p0-irect-contains). */
  containsOp(x: any): any;
  /** IRect.__eq__ compatibility entry (case p0-irect-eq). */
  eq(r: any): any;
  /** IRect.__ge__ compatibility entry (case p0-irect-ge). */
  ge(value: any): any;
  /** IRect.__getitem__ compatibility entry (case p0-irect-getitem). */
  getitem(i: any): any;
  /** IRect.__gt__ compatibility entry (case p0-irect-gt). */
  gt(value: any): any;
  /** IRect.__init__ compatibility entry (case p0-irect-init). */
  constructor(...args: any[]);
  /** IRect.__le__ compatibility entry (case p0-irect-le). */
  le(value: any): any;
  /** IRect.__len__ compatibility entry (case p0-irect-len). */
  len(): any;
  /** IRect.__lt__ compatibility entry (case p0-irect-lt). */
  lt(value: any): any;
  /** IRect.__mul__ compatibility entry (case p0-irect-mul). */
  mul(m: any): any;
  /** IRect.__ne__ compatibility entry (case p0-irect-ne). */
  ne(value: any): any;
  /** IRect.__neg__ compatibility entry (case p0-irect-neg). */
  neg(): any;
  /** IRect.__pos__ compatibility entry (case p0-irect-pos). */
  pos(): any;
  /** IRect.__setitem__ compatibility entry (case p0-irect-setitem). */
  setitem(i: any, v: any): any;
  /** IRect.__sub__ compatibility entry (case p0-irect-sub). */
  sub(p: any): any;
  /** IRect.__truediv__ compatibility entry (case p0-irect-truediv). */
  truediv(m: any): any;
  /** IRect.bl compatibility entry (case p0-irect-bl). */
  bl: any;
  /** IRect.bottom_left compatibility entry (case p0-irect-bottom-left). */
  bottomLeft: any;
  /** IRect.bottom_right compatibility entry (case p0-irect-bottom-right). */
  bottomRight: any;
  /** IRect.br compatibility entry (case p0-irect-br). */
  br: any;
  /** IRect.contains compatibility entry (case p0-irect-contains-2). */
  contains(x: any): any;
  /** IRect.get_area compatibility entry (case p0-irect-get-area). */
  getArea(...args: any[]): any;
  /** IRect.height compatibility entry (case p0-irect-height). */
  height: any;
  /** IRect.include_point compatibility entry (case p0-irect-include-point). */
  includePoint(p: any): any;
  /** IRect.include_rect compatibility entry (case p0-irect-include-rect). */
  includeRect(r: any): any;
  /** IRect.intersect compatibility entry (case p0-irect-intersect). */
  intersect(r: any): any;
  /** IRect.intersects compatibility entry (case p0-irect-intersects). */
  intersects(x: any): any;
  /** IRect.is_empty compatibility entry (case p0-irect-is-empty). */
  isEmpty: any;
  /** IRect.is_infinite compatibility entry (case p0-irect-is-infinite). */
  isInfinite: any;
  /** IRect.is_valid compatibility entry (case p0-irect-is-valid). */
  isValid: any;
  /** IRect.morph compatibility entry (case p0-irect-morph). */
  morph(p: any, m: any): any;
  /** IRect.norm compatibility entry (case p0-irect-norm). */
  norm(): any;
  /** IRect.normalize compatibility entry (case p0-irect-normalize). */
  normalize(): any;
  /** IRect.quad compatibility entry (case p0-irect-quad). */
  quad: any;
  /** IRect.rect compatibility entry (case p0-irect-rect). */
  rect: any;
  /** IRect.tl compatibility entry (case p0-irect-tl). */
  tl: any;
  /** IRect.top_left compatibility entry (case p0-irect-top-left). */
  topLeft: any;
  /** IRect.top_right compatibility entry (case p0-irect-top-right). */
  topRight: any;
  /** IRect.torect compatibility entry (case p0-irect-torect). */
  torect(r: any): any;
  /** IRect.tr compatibility entry (case p0-irect-tr). */
  tr: any;
  /** IRect.transform compatibility entry (case p0-irect-transform). */
  transform(m: any): any;
  /** IRect.width compatibility entry (case p0-irect-width). */
  width: any;
}

export declare class Point {
  /** Point.__abs__ compatibility entry (case p0-point-abs). */
  abs(): any;
  /** Point.__add__ compatibility entry (case p0-point-add). */
  add(p: any): any;
  /** Point.__bool__ compatibility entry (case p0-point-bool). */
  bool(): any;
  /** Point.__eq__ compatibility entry (case p0-point-eq). */
  eq(p: any): any;
  /** Point.__ge__ compatibility entry (case p0-point-ge). */
  ge(value: any): any;
  /** Point.__getitem__ compatibility entry (case p0-point-getitem). */
  getitem(i: any): any;
  /** Point.__gt__ compatibility entry (case p0-point-gt). */
  gt(value: any): any;
  /** Point.__init__ compatibility entry (case p0-point-init). */
  constructor(...args: any[]);
  /** Point.__le__ compatibility entry (case p0-point-le). */
  le(value: any): any;
  /** Point.__len__ compatibility entry (case p0-point-len). */
  len(): any;
  /** Point.__lt__ compatibility entry (case p0-point-lt). */
  lt(value: any): any;
  /** Point.__mul__ compatibility entry (case p0-point-mul). */
  mul(m: any): any;
  /** Point.__ne__ compatibility entry (case p0-point-ne). */
  ne(value: any): any;
  /** Point.__neg__ compatibility entry (case p0-point-neg). */
  neg(): any;
  /** Point.__pos__ compatibility entry (case p0-point-pos). */
  pos(): any;
  /** Point.__setitem__ compatibility entry (case p0-point-setitem). */
  setitem(i: any, v: any): any;
  /** Point.__sub__ compatibility entry (case p0-point-sub). */
  sub(p: any): any;
  /** Point.__truediv__ compatibility entry (case p0-point-truediv). */
  truediv(m: any): any;
  /** Point.abs_unit compatibility entry (case p0-point-abs-unit). */
  absUnit: any;
  /** Point.distance_to compatibility entry (case p0-point-distance-to). */
  distanceTo(...args: any[]): any;
  /** Point.norm compatibility entry (case p0-point-norm). */
  norm(): any;
  /** Point.transform compatibility entry (case p0-point-transform). */
  transform(m: any): any;
  /** Point.unit compatibility entry (case p0-point-unit). */
  unit: any;
}

export declare class Matrix {
  /** Matrix.__abs__ compatibility entry (case p0-matrix-abs). */
  abs(): any;
  /** Matrix.__add__ compatibility entry (case p0-matrix-add). */
  add(m: any): any;
  /** Matrix.__bool__ compatibility entry (case p0-matrix-bool). */
  bool(): any;
  /** Matrix.__eq__ compatibility entry (case p0-matrix-eq). */
  eq(mat: any): any;
  /** Matrix.__ge__ compatibility entry (case p0-matrix-ge). */
  ge(value: any): any;
  /** Matrix.__getitem__ compatibility entry (case p0-matrix-getitem). */
  getitem(i: any): any;
  /** Matrix.__gt__ compatibility entry (case p0-matrix-gt). */
  gt(value: any): any;
  /** Matrix.__init__ compatibility entry (case p0-matrix-init). */
  constructor(...args: any[]);
  /** Matrix.__invert__ compatibility entry (case p0-matrix-invert). */
  invertOp(): any;
  /** Matrix.__le__ compatibility entry (case p0-matrix-le). */
  le(value: any): any;
  /** Matrix.__len__ compatibility entry (case p0-matrix-len). */
  len(): any;
  /** Matrix.__lt__ compatibility entry (case p0-matrix-lt). */
  lt(value: any): any;
  /** Matrix.__mul__ compatibility entry (case p0-matrix-mul). */
  mul(m: any): any;
  /** Matrix.__ne__ compatibility entry (case p0-matrix-ne). */
  ne(value: any): any;
  /** Matrix.__neg__ compatibility entry (case p0-matrix-neg). */
  neg(): any;
  /** Matrix.__pos__ compatibility entry (case p0-matrix-pos). */
  pos(): any;
  /** Matrix.__setitem__ compatibility entry (case p0-matrix-setitem). */
  setitem(i: any, v: any): any;
  /** Matrix.__sub__ compatibility entry (case p0-matrix-sub). */
  sub(m: any): any;
  /** Matrix.__truediv__ compatibility entry (case p0-matrix-truediv). */
  truediv(m: any): any;
  /** Matrix.concat compatibility entry (case p0-matrix-concat). */
  concat(one: any, two: any): any;
  /** Matrix.invert compatibility entry (case p0-matrix-invert-2). */
  invert(src?: any): any;
  /** Matrix.is_rectilinear compatibility entry (case p0-matrix-is-rectilinear). */
  isRectilinear: any;
  /** Matrix.norm compatibility entry (case p0-matrix-norm). */
  norm(): any;
  /** Matrix.prerotate compatibility entry (case p0-matrix-prerotate). */
  prerotate(theta: any): any;
  /** Matrix.prescale compatibility entry (case p0-matrix-prescale). */
  prescale(sx: any, sy: any): any;
  /** Matrix.preshear compatibility entry (case p0-matrix-preshear). */
  preshear(h: any, v: any): any;
  /** Matrix.pretranslate compatibility entry (case p0-matrix-pretranslate). */
  pretranslate(tx: any, ty: any): any;
}

export declare class Quad {
  /** Quad.__abs__ compatibility entry (case p0-quad-abs). */
  abs(): any;
  /** Quad.__add__ compatibility entry (case p0-quad-add). */
  add(q: any): any;
  /** Quad.__bool__ compatibility entry (case p0-quad-bool). */
  bool(): any;
  /** Quad.__contains__ compatibility entry (case p0-quad-contains). */
  contains(x: any): any;
  /** Quad.__eq__ compatibility entry (case p0-quad-eq). */
  eq(quad: any): any;
  /** Quad.__ge__ compatibility entry (case p0-quad-ge). */
  ge(value: any): any;
  /** Quad.__getitem__ compatibility entry (case p0-quad-getitem). */
  getitem(i: any): any;
  /** Quad.__gt__ compatibility entry (case p0-quad-gt). */
  gt(value: any): any;
  /** Quad.__init__ compatibility entry (case p0-quad-init). */
  constructor(...args: any[]);
  /** Quad.__le__ compatibility entry (case p0-quad-le). */
  le(value: any): any;
  /** Quad.__len__ compatibility entry (case p0-quad-len). */
  len(): any;
  /** Quad.__lt__ compatibility entry (case p0-quad-lt). */
  lt(value: any): any;
  /** Quad.__mul__ compatibility entry (case p0-quad-mul). */
  mul(m: any): any;
  /** Quad.__ne__ compatibility entry (case p0-quad-ne). */
  ne(value: any): any;
  /** Quad.__neg__ compatibility entry (case p0-quad-neg). */
  neg(): any;
  /** Quad.__pos__ compatibility entry (case p0-quad-pos). */
  pos(): any;
  /** Quad.__setitem__ compatibility entry (case p0-quad-setitem). */
  setitem(i: any, v: any): any;
  /** Quad.__sub__ compatibility entry (case p0-quad-sub). */
  sub(q: any): any;
  /** Quad.__truediv__ compatibility entry (case p0-quad-truediv). */
  truediv(m: any): any;
  /** Quad.height compatibility entry (case p0-quad-height). */
  height: any;
  /** Quad.is_convex compatibility entry (case p0-quad-is-convex). */
  isConvex: any;
  /** Quad.is_empty compatibility entry (case p0-quad-is-empty). */
  isEmpty: any;
  /** Quad.is_infinite compatibility entry (case p0-quad-is-infinite). */
  isInfinite: any;
  /** Quad.is_rectangular compatibility entry (case p0-quad-is-rectangular). */
  isRectangular: any;
  /** Quad.morph compatibility entry (case p0-quad-morph). */
  morph(p: any, m: any): any;
  /** Quad.rect compatibility entry (case p0-quad-rect). */
  rect: any;
  /** Quad.transform compatibility entry (case p0-quad-transform). */
  transform(m: any): any;
  /** Quad.width compatibility entry (case p0-quad-width). */
  width: any;
}

export declare class Font {
  /** Font.__eq__ compatibility entry (case l4-font-eq). */
  eq(value: any): any;
  /** Font.__ge__ compatibility entry (case l4-font-ge). */
  ge(value: any): any;
  /** Font.__gt__ compatibility entry (case l4-font-gt). */
  gt(value: any): any;
  /** Font.__init__ compatibility entry (case l4-font-init). */
  constructor(fontname?: any, fontfile?: any, fontbuffer?: any, script?: any, language?: any, ordering?: any, is_bold?: any, is_italic?: any, is_serif?: any, embed?: any);
  /** Font.__le__ compatibility entry (case l4-font-le). */
  le(value: any): any;
  /** Font.__lt__ compatibility entry (case l4-font-lt). */
  lt(value: any): any;
  /** Font.__ne__ compatibility entry (case l4-font-ne). */
  ne(value: any): any;
  /** Font.ascender compatibility entry (case l4-font-properties). */
  ascender: any;
  /** Font.bbox compatibility entry (case l4-font-properties). */
  bbox: any;
  /** Font.buffer compatibility entry (case l4-font-properties). */
  buffer: any;
  /** Font.char_lengths compatibility entry (case l4-font-metrics). */
  charLengths(text: any, fontsize?: any, language?: any, script?: any, wmode?: any, small_caps?: any): any;
  /** Font.descender compatibility entry (case l4-font-properties). */
  descender: any;
  /** Font.flags compatibility entry (case l4-font-properties). */
  flags: any;
  /** Font.glyph_advance compatibility entry (case l4-font-metrics). */
  glyphAdvance(chr_: any, language?: any, script?: any, wmode?: any, small_caps?: any): any;
  /** Font.glyph_bbox compatibility entry (case l4-font-metrics). */
  glyphBbox(char: any, language?: any, script?: any, small_caps?: any): any;
  /** Font.glyph_count compatibility entry (case l4-font-properties). */
  glyphCount: any;
  /** Font.glyph_name_to_unicode compatibility entry (case l4-font-metrics). */
  glyphNameToUnicode(name: any): any;
  /** Font.has_glyph compatibility entry (case l4-font-metrics). */
  hasGlyph(chr: any, language?: any, script?: any, fallback?: any, small_caps?: any): any;
  /** Font.is_bold compatibility entry (case l4-font-properties). */
  isBold: any;
  /** Font.is_italic compatibility entry (case l4-font-properties). */
  isItalic: any;
  /** Font.is_monospaced compatibility entry (case l4-font-properties). */
  isMonospaced: any;
  /** Font.is_serif compatibility entry (case l4-font-properties). */
  isSerif: any;
  /** Font.is_writable compatibility entry (case l4-font-properties). */
  isWritable: any;
  /** Font.name compatibility entry (case l4-font-properties). */
  name: any;
  /** Font.text_length compatibility entry (case l4-font-metrics). */
  textLength(text: any, fontsize?: any, language?: any, script?: any, wmode?: any, small_caps?: any): any;
  /** Font.unicode_to_glyph_name compatibility entry (case l4-font-metrics). */
  unicodeToGlyphName(ch: any): any;
  /** Font.valid_codepoints compatibility entry (case l4-font-metrics). */
  validCodepoints(): any;
}

export declare class TextWriter {
  /** TextWriter.__eq__ compatibility entry (case l4-textwriter-eq). */
  eq(value: any): any;
  /** TextWriter.__ge__ compatibility entry (case l4-textwriter-ge). */
  ge(value: any): any;
  /** TextWriter.__gt__ compatibility entry (case l4-textwriter-gt). */
  gt(value: any): any;
  /** TextWriter.__init__ compatibility entry (case l4-textwriter-init). */
  constructor(page_rect: any, opacity?: any, color?: any);
  /** TextWriter.__le__ compatibility entry (case l4-textwriter-le). */
  le(value: any): any;
  /** TextWriter.__lt__ compatibility entry (case l4-textwriter-lt). */
  lt(value: any): any;
  /** TextWriter.__ne__ compatibility entry (case l4-textwriter-ne). */
  ne(value: any): any;
  /** TextWriter.append compatibility entry (case l4-textwriter-append). */
  append(pos: any, text: any, font?: any, fontsize?: any, language?: any, right_to_left?: any, small_caps?: any): any;
  /** TextWriter.appendv compatibility entry (case l4-textwriter-appendv). */
  appendv(pos: any, text: any, font?: any, fontsize?: any, language?: any, small_caps?: any): any;
  /** TextWriter.clean_rtl compatibility entry (case l4-textwriter-clean-rtl). */
  cleanRtl(text: any): any;
  /** TextWriter.fill_textbox compatibility entry (case l4-textwriter-fill-textbox). */
  fillTextbox(rect: any, text: any, pos?: any, font?: any, fontsize?: any, lineheight?: any, align?: any, warn?: any, right_to_left?: any, small_caps?: any): any;
  /** TextWriter.write_text compatibility entry (case l4-textwriter-write-text). */
  writeText(page: any, color?: any, opacity?: any, overlay?: any, morph?: any, matrix?: any, render_mode?: any, oc?: any): any;
}

export declare class Colorspace {
  /** Colorspace.__eq__ compatibility entry (case l3-colorspace-eq). */
  eq(value: any): any;
  /** Colorspace.__ge__ compatibility entry (case l3-colorspace-ge). */
  ge(value: any): any;
  /** Colorspace.__gt__ compatibility entry (case l3-colorspace-gt). */
  gt(value: any): any;
  /** Colorspace.__init__ compatibility entry (case l3-colorspace-init). */
  constructor(type_: any);
  /** Colorspace.__le__ compatibility entry (case l3-colorspace-le). */
  le(value: any): any;
  /** Colorspace.__lt__ compatibility entry (case l3-colorspace-lt). */
  lt(value: any): any;
  /** Colorspace.__ne__ compatibility entry (case l3-colorspace-ne). */
  ne(value: any): any;
  /** Colorspace.n compatibility entry (case l3-colorspace-n). */
  n: any;
  /** Colorspace.name compatibility entry (case l3-colorspace-name). */
  name: any;
}

export declare class constants {
  /** constants.EPSILON compatibility entry (case p0-constants-epsilon). */
  EPSILON: any;
  /** constants.FLT_EPSILON compatibility entry (case p0-constants-flt-epsilon). */
  FLT_EPSILON: any;
  /** constants.FZ_MAX_INF_RECT compatibility entry (case p0-constants-fz-max-inf-rect). */
  FZ_MAX_INF_RECT: any;
  /** constants.FZ_MIN_INF_RECT compatibility entry (case p0-constants-fz-min-inf-rect). */
  FZ_MIN_INF_RECT: any;
  /** constants.Identity compatibility entry (case p0-constants-identity). */
  Identity: any;
  /** constants.TEXT_ACCURATE_ASCENDERS compatibility entry (case l1-constants-text-accurate-ascenders). */
  TEXT_ACCURATE_ASCENDERS: any;
  /** constants.TEXT_ACCURATE_BBOXES compatibility entry (case l1-constants-text-accurate-bboxes). */
  TEXT_ACCURATE_BBOXES: any;
  /** constants.TEXT_ACCURATE_SIDE_BEARINGS compatibility entry (case l1-constants-text-accurate-side-bearings). */
  TEXT_ACCURATE_SIDE_BEARINGS: any;
  /** constants.TEXT_ALIGN_CENTER compatibility entry (case l2-constants-all). */
  TEXT_ALIGN_CENTER: any;
  /** constants.TEXT_ALIGN_JUSTIFY compatibility entry (case l1-constants-text-align-justify). */
  TEXT_ALIGN_JUSTIFY: any;
  /** constants.TEXT_ALIGN_LEFT compatibility entry (case l2-constants-all). */
  TEXT_ALIGN_LEFT: any;
  /** constants.TEXT_ALIGN_RIGHT compatibility entry (case l1-constants-text-align-right). */
  TEXT_ALIGN_RIGHT: any;
  /** constants.TEXT_CID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-cid-for-unknown-unicode). */
  TEXT_CID_FOR_UNKNOWN_UNICODE: any;
  /** constants.TEXT_CLIP compatibility entry (case l1-constants-text-clip). */
  TEXT_CLIP: any;
  /** constants.TEXT_CLIP_RECT compatibility entry (case l1-constants-text-clip-rect). */
  TEXT_CLIP_RECT: any;
  /** constants.TEXT_COLLECT_STRUCTURE compatibility entry (case l1-constants-text-collect-structure). */
  TEXT_COLLECT_STRUCTURE: any;
  /** constants.TEXT_COLLECT_STYLES compatibility entry (case l1-constants-text-collect-styles). */
  TEXT_COLLECT_STYLES: any;
  /** constants.TEXT_COLLECT_VECTORS compatibility entry (case l1-constants-text-collect-vectors). */
  TEXT_COLLECT_VECTORS: any;
  /** constants.TEXT_DEHYPHENATE compatibility entry (case l1-constants-text-dehyphenate). */
  TEXT_DEHYPHENATE: any;
  /** constants.TEXT_ENCODING_CYRILLIC compatibility entry (case l1-constants-text-encoding-cyrillic). */
  TEXT_ENCODING_CYRILLIC: any;
  /** constants.TEXT_ENCODING_GREEK compatibility entry (case l1-constants-text-encoding-greek). */
  TEXT_ENCODING_GREEK: any;
  /** constants.TEXT_ENCODING_LATIN compatibility entry (case l1-constants-text-encoding-latin). */
  TEXT_ENCODING_LATIN: any;
  /** constants.TEXT_FONT_BOLD compatibility entry (case l1-constants-text-font-bold). */
  TEXT_FONT_BOLD: any;
  /** constants.TEXT_FONT_ITALIC compatibility entry (case l1-constants-text-font-italic). */
  TEXT_FONT_ITALIC: any;
  /** constants.TEXT_FONT_MONOSPACED compatibility entry (case l1-constants-text-font-monospaced). */
  TEXT_FONT_MONOSPACED: any;
  /** constants.TEXT_FONT_SERIFED compatibility entry (case l1-constants-text-font-serifed). */
  TEXT_FONT_SERIFED: any;
  /** constants.TEXT_FONT_SUPERSCRIPT compatibility entry (case l1-constants-text-font-superscript). */
  TEXT_FONT_SUPERSCRIPT: any;
  /** constants.TEXT_FUZZY_VECTORS compatibility entry (case l1-constants-text-fuzzy-vectors). */
  TEXT_FUZZY_VECTORS: any;
  /** constants.TEXT_IGNORE_ACTUALTEXT compatibility entry (case l1-constants-text-ignore-actualtext). */
  TEXT_IGNORE_ACTUALTEXT: any;
  /** constants.TEXT_INHIBIT_SPACES compatibility entry (case l1-constants-text-inhibit-spaces). */
  TEXT_INHIBIT_SPACES: any;
  /** constants.TEXT_LAZY_VECTORS compatibility entry (case l1-constants-text-lazy-vectors). */
  TEXT_LAZY_VECTORS: any;
  /** constants.TEXT_MEDIABOX_CLIP compatibility entry (case l1-constants-text-mediabox-clip). */
  TEXT_MEDIABOX_CLIP: any;
  /** constants.TEXT_OUTPUT_HTML compatibility entry (case l1-constants-text-output-html). */
  TEXT_OUTPUT_HTML: any;
  /** constants.TEXT_OUTPUT_JSON compatibility entry (case l1-constants-text-output-json). */
  TEXT_OUTPUT_JSON: any;
  /** constants.TEXT_OUTPUT_TEXT compatibility entry (case l1-constants-text-output-text). */
  TEXT_OUTPUT_TEXT: any;
  /** constants.TEXT_OUTPUT_XHTML compatibility entry (case l1-constants-text-output-xhtml). */
  TEXT_OUTPUT_XHTML: any;
  /** constants.TEXT_OUTPUT_XML compatibility entry (case l1-constants-text-output-xml). */
  TEXT_OUTPUT_XML: any;
  /** constants.TEXT_PARAGRAPH_BREAK compatibility entry (case l1-constants-text-paragraph-break). */
  TEXT_PARAGRAPH_BREAK: any;
  /** constants.TEXT_PRESERVE_IMAGES compatibility entry (case l2-constants-all). */
  TEXT_PRESERVE_IMAGES: any;
  /** constants.TEXT_PRESERVE_LIGATURES compatibility entry (case l2-constants-all). */
  TEXT_PRESERVE_LIGATURES: any;
  /** constants.TEXT_PRESERVE_SPANS compatibility entry (case l1-constants-text-preserve-spans). */
  TEXT_PRESERVE_SPANS: any;
  /** constants.TEXT_PRESERVE_WHITESPACE compatibility entry (case l2-constants-all). */
  TEXT_PRESERVE_WHITESPACE: any;
  /** constants.TEXT_SEGMENT compatibility entry (case l1-constants-text-segment). */
  TEXT_SEGMENT: any;
  /** constants.TEXT_STEXT_SEGMENT compatibility entry (case l1-constants-text-stext-segment). */
  TEXT_STEXT_SEGMENT: any;
  /** constants.TEXT_TABLE_HUNT compatibility entry (case l1-constants-text-table-hunt). */
  TEXT_TABLE_HUNT: any;
  /** constants.TEXT_USE_CID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-use-cid-for-unknown-unicode). */
  TEXT_USE_CID_FOR_UNKNOWN_UNICODE: any;
  /** constants.TEXT_USE_GID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-use-gid-for-unknown-unicode). */
  TEXT_USE_GID_FOR_UNKNOWN_UNICODE: any;
  /** constants.TEXTFLAGS_BLOCKS compatibility entry (case l1-constants-textflags-blocks). */
  TEXTFLAGS_BLOCKS: any;
  /** constants.TEXTFLAGS_DICT compatibility entry (case l2-constants-all). */
  TEXTFLAGS_DICT: any;
  /** constants.TEXTFLAGS_HTML compatibility entry (case l1-constants-textflags-html). */
  TEXTFLAGS_HTML: any;
  /** constants.TEXTFLAGS_RAWDICT compatibility entry (case l1-constants-textflags-rawdict). */
  TEXTFLAGS_RAWDICT: any;
  /** constants.TEXTFLAGS_SEARCH compatibility entry (case l1-constants-textflags-search). */
  TEXTFLAGS_SEARCH: any;
  /** constants.TEXTFLAGS_TEXT compatibility entry (case l2-constants-all). */
  TEXTFLAGS_TEXT: any;
  /** constants.TEXTFLAGS_WORDS compatibility entry (case l1-constants-textflags-words). */
  TEXTFLAGS_WORDS: any;
  /** constants.TEXTFLAGS_XHTML compatibility entry (case l1-constants-textflags-xhtml). */
  TEXTFLAGS_XHTML: any;
  /** constants.TEXTFLAGS_XML compatibility entry (case l1-constants-textflags-xml). */
  TEXTFLAGS_XML: any;
  /** constants.dictkey_align compatibility entry (case l1-constants-dictkey-align). */
  dictkey_align: any;
  /** constants.dictkey_asc compatibility entry (case l1-constants-dictkey-asc). */
  dictkey_asc: any;
  /** constants.dictkey_bbox compatibility entry (case l1-constants-dictkey-bbox). */
  dictkey_bbox: any;
  /** constants.dictkey_bidi compatibility entry (case l1-constants-dictkey-bidi). */
  dictkey_bidi: any;
  /** constants.dictkey_blocks compatibility entry (case l1-constants-dictkey-blocks). */
  dictkey_blocks: any;
  /** constants.dictkey_bpc compatibility entry (case l1-constants-dictkey-bpc). */
  dictkey_bpc: any;
  /** constants.dictkey_c compatibility entry (case l1-constants-dictkey-c). */
  dictkey_c: any;
  /** constants.dictkey_char_flags compatibility entry (case l1-constants-dictkey-char-flags). */
  dictkey_char_flags: any;
  /** constants.dictkey_chars compatibility entry (case l1-constants-dictkey-chars). */
  dictkey_chars: any;
  /** constants.dictkey_color compatibility entry (case l1-constants-dictkey-color). */
  dictkey_color: any;
  /** constants.dictkey_colorspace compatibility entry (case l1-constants-dictkey-colorspace). */
  dictkey_colorspace: any;
  /** constants.dictkey_content compatibility entry (case l1-constants-dictkey-content). */
  dictkey_content: any;
  /** constants.dictkey_creationDate compatibility entry (case l1-constants-dictkey-creationdate). */
  dictkey_creationDate: any;
  /** constants.dictkey_cs_name compatibility entry (case l1-constants-dictkey-cs-name). */
  dictkey_cs_name: any;
  /** constants.dictkey_da compatibility entry (case l1-constants-dictkey-da). */
  dictkey_da: any;
  /** constants.dictkey_dashes compatibility entry (case l1-constants-dictkey-dashes). */
  dictkey_dashes: any;
  /** constants.dictkey_desc compatibility entry (case l1-constants-dictkey-desc). */
  dictkey_desc: any;
  /** constants.dictkey_descr compatibility entry (case l1-constants-dictkey-descr). */
  dictkey_descr: any;
  /** constants.dictkey_dir compatibility entry (case l1-constants-dictkey-dir). */
  dictkey_dir: any;
  /** constants.dictkey_effect compatibility entry (case l1-constants-dictkey-effect). */
  dictkey_effect: any;
  /** constants.dictkey_ext compatibility entry (case l1-constants-dictkey-ext). */
  dictkey_ext: any;
  /** constants.dictkey_filename compatibility entry (case l1-constants-dictkey-filename). */
  dictkey_filename: any;
  /** constants.dictkey_fill compatibility entry (case l1-constants-dictkey-fill). */
  dictkey_fill: any;
  /** constants.dictkey_flags compatibility entry (case l1-constants-dictkey-flags). */
  dictkey_flags: any;
  /** constants.dictkey_font compatibility entry (case l1-constants-dictkey-font). */
  dictkey_font: any;
  /** constants.dictkey_glyph compatibility entry (case l1-constants-dictkey-glyph). */
  dictkey_glyph: any;
  /** constants.dictkey_height compatibility entry (case l1-constants-dictkey-height). */
  dictkey_height: any;
  /** constants.dictkey_id compatibility entry (case l1-constants-dictkey-id). */
  dictkey_id: any;
  /** constants.dictkey_image compatibility entry (case l1-constants-dictkey-image). */
  dictkey_image: any;
  /** constants.dictkey_items compatibility entry (case l1-constants-dictkey-items). */
  dictkey_items: any;
  /** constants.dictkey_length compatibility entry (case l1-constants-dictkey-length). */
  dictkey_length: any;
  /** constants.dictkey_lines compatibility entry (case l1-constants-dictkey-lines). */
  dictkey_lines: any;
  /** constants.dictkey_matrix compatibility entry (case l1-constants-dictkey-matrix). */
  dictkey_matrix: any;
  /** constants.dictkey_modDate compatibility entry (case l1-constants-dictkey-moddate). */
  dictkey_modDate: any;
  /** constants.dictkey_name compatibility entry (case l1-constants-dictkey-name). */
  dictkey_name: any;
  /** constants.dictkey_number compatibility entry (case l1-constants-dictkey-number). */
  dictkey_number: any;
  /** constants.dictkey_origin compatibility entry (case l1-constants-dictkey-origin). */
  dictkey_origin: any;
  /** constants.dictkey_rect compatibility entry (case l1-constants-dictkey-rect). */
  dictkey_rect: any;
  /** constants.dictkey_size compatibility entry (case l1-constants-dictkey-size). */
  dictkey_size: any;
  /** constants.dictkey_smask compatibility entry (case l1-constants-dictkey-smask). */
  dictkey_smask: any;
  /** constants.dictkey_spans compatibility entry (case l1-constants-dictkey-spans). */
  dictkey_spans: any;
  /** constants.dictkey_stroke compatibility entry (case l1-constants-dictkey-stroke). */
  dictkey_stroke: any;
  /** constants.dictkey_style compatibility entry (case l1-constants-dictkey-style). */
  dictkey_style: any;
  /** constants.dictkey_subject compatibility entry (case l1-constants-dictkey-subject). */
  dictkey_subject: any;
  /** constants.dictkey_text compatibility entry (case l1-constants-dictkey-text). */
  dictkey_text: any;
  /** constants.dictkey_title compatibility entry (case l1-constants-dictkey-title). */
  dictkey_title: any;
  /** constants.dictkey_type compatibility entry (case l1-constants-dictkey-type). */
  dictkey_type: any;
  /** constants.dictkey_ufilename compatibility entry (case l1-constants-dictkey-ufilename). */
  dictkey_ufilename: any;
  /** constants.dictkey_width compatibility entry (case l1-constants-dictkey-width). */
  dictkey_width: any;
  /** constants.dictkey_wmode compatibility entry (case l1-constants-dictkey-wmode). */
  dictkey_wmode: any;
  /** constants.dictkey_xref compatibility entry (case l1-constants-dictkey-xref). */
  dictkey_xref: any;
  /** constants.dictkey_xres compatibility entry (case l1-constants-dictkey-xres). */
  dictkey_xres: any;
  /** constants.dictkey_yres compatibility entry (case l1-constants-dictkey-yres). */
  dictkey_yres: any;
  /** constants.PDF_STRUCT_BROKEN compatibility entry (case l1-constants-pdf-struct-broken). */
  PDF_STRUCT_BROKEN: any;
  /** constants.PDF_STRUCT_FIXED compatibility entry (case l1-constants-pdf-struct-fixed). */
  PDF_STRUCT_FIXED: any;
  /** constants.PDF_STRUCT_HAS_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-attributes). */
  PDF_STRUCT_HAS_ATTRIBUTES: any;
  /** constants.PDF_STRUCT_HAS_CYCLE compatibility entry (case l1-constants-pdf-struct-has-cycle). */
  PDF_STRUCT_HAS_CYCLE: any;
  /** constants.PDF_STRUCT_HAS_TABLE_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-table-attributes). */
  PDF_STRUCT_HAS_TABLE_ATTRIBUTES: any;
  /** constants.PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-table-span-attributes). */
  PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES: any;
  /** constants.PDF_STRUCT_NOT_PRESENT compatibility entry (case l1-constants-pdf-struct-not-present). */
  PDF_STRUCT_NOT_PRESENT: any;
  /** constants.PDF_STRUCT_PRESENT compatibility entry (case l1-constants-pdf-struct-present). */
  PDF_STRUCT_PRESENT: any;
  /** constants.LINK_FLAG_B_VALID compatibility entry (case l2-constants-link-flag-b-valid). */
  LINK_FLAG_B_VALID: any;
  /** constants.LINK_FLAG_FIT_H compatibility entry (case l2-constants-link-flag-fit-h). */
  LINK_FLAG_FIT_H: any;
  /** constants.LINK_FLAG_FIT_V compatibility entry (case l2-constants-link-flag-fit-v). */
  LINK_FLAG_FIT_V: any;
  /** constants.LINK_FLAG_L_VALID compatibility entry (case l2-constants-link-flag-l-valid). */
  LINK_FLAG_L_VALID: any;
  /** constants.LINK_FLAG_R_IS_ZOOM compatibility entry (case l2-constants-link-flag-r-is-zoom). */
  LINK_FLAG_R_IS_ZOOM: any;
  /** constants.LINK_FLAG_R_VALID compatibility entry (case l2-constants-link-flag-r-valid). */
  LINK_FLAG_R_VALID: any;
  /** constants.LINK_FLAG_T_VALID compatibility entry (case l2-constants-link-flag-t-valid). */
  LINK_FLAG_T_VALID: any;
  /** constants.LINK_GOTO compatibility entry (case l2-constants-all). */
  LINK_GOTO: any;
  /** constants.LINK_GOTOR compatibility entry (case l2-constants-link-gotor). */
  LINK_GOTOR: any;
  /** constants.LINK_LAUNCH compatibility entry (case l2-constants-link-launch). */
  LINK_LAUNCH: any;
  /** constants.LINK_NAMED compatibility entry (case l2-constants-link-named). */
  LINK_NAMED: any;
  /** constants.LINK_NONE compatibility entry (case l2-constants-link-none). */
  LINK_NONE: any;
  /** constants.LINK_URI compatibility entry (case l2-constants-all). */
  LINK_URI: any;
  /** constants.PDF_ENCRYPT_AES_128 compatibility entry (case l2-constants-pdf-encrypt-aes-128). */
  PDF_ENCRYPT_AES_128: any;
  /** constants.PDF_ENCRYPT_AES_256 compatibility entry (case l2-constants-all). */
  PDF_ENCRYPT_AES_256: any;
  /** constants.PDF_ENCRYPT_KEEP compatibility entry (case l2-constants-all). */
  PDF_ENCRYPT_KEEP: any;
  /** constants.PDF_ENCRYPT_NONE compatibility entry (case l2-constants-all). */
  PDF_ENCRYPT_NONE: any;
  /** constants.PDF_ENCRYPT_RC4_128 compatibility entry (case l2-constants-pdf-encrypt-rc4-128). */
  PDF_ENCRYPT_RC4_128: any;
  /** constants.PDF_ENCRYPT_RC4_40 compatibility entry (case l2-constants-pdf-encrypt-rc4-40). */
  PDF_ENCRYPT_RC4_40: any;
  /** constants.PDF_ENCRYPT_UNKNOWN compatibility entry (case l2-constants-pdf-encrypt-unknown). */
  PDF_ENCRYPT_UNKNOWN: any;
  /** constants.PDF_PERM_ACCESSIBILITY compatibility entry (case l2-constants-pdf-perm-accessibility). */
  PDF_PERM_ACCESSIBILITY: any;
  /** constants.PDF_PERM_ANNOTATE compatibility entry (case l2-constants-all). */
  PDF_PERM_ANNOTATE: any;
  /** constants.PDF_PERM_ASSEMBLE compatibility entry (case l2-constants-pdf-perm-assemble). */
  PDF_PERM_ASSEMBLE: any;
  /** constants.PDF_PERM_COPY compatibility entry (case l2-constants-all). */
  PDF_PERM_COPY: any;
  /** constants.PDF_PERM_FORM compatibility entry (case l2-constants-all). */
  PDF_PERM_FORM: any;
  /** constants.PDF_PERM_MODIFY compatibility entry (case l2-constants-all). */
  PDF_PERM_MODIFY: any;
  /** constants.PDF_PERM_PRINT compatibility entry (case l2-constants-all). */
  PDF_PERM_PRINT: any;
  /** constants.PDF_PERM_PRINT_HQ compatibility entry (case l2-constants-all). */
  PDF_PERM_PRINT_HQ: any;
  /** constants.PDF_PAGE_LABEL_ALPHA_LC compatibility entry (case l2-constants-pdf-page-label-alpha-lc). */
  PDF_PAGE_LABEL_ALPHA_LC: any;
  /** constants.PDF_PAGE_LABEL_ALPHA_UC compatibility entry (case l2-constants-pdf-page-label-alpha-uc). */
  PDF_PAGE_LABEL_ALPHA_UC: any;
  /** constants.PDF_PAGE_LABEL_DECIMAL compatibility entry (case l2-constants-pdf-page-label-decimal). */
  PDF_PAGE_LABEL_DECIMAL: any;
  /** constants.PDF_PAGE_LABEL_NONE compatibility entry (case l2-constants-pdf-page-label-none). */
  PDF_PAGE_LABEL_NONE: any;
  /** constants.PDF_PAGE_LABEL_ROMAN_LC compatibility entry (case l2-constants-pdf-page-label-roman-lc). */
  PDF_PAGE_LABEL_ROMAN_LC: any;
  /** constants.PDF_PAGE_LABEL_ROMAN_UC compatibility entry (case l2-constants-pdf-page-label-roman-uc). */
  PDF_PAGE_LABEL_ROMAN_UC: any;
  /** constants.PDF_SIGNATURE_DEFAULT_APPEARANCE compatibility entry (case l2-constants-pdf-signature-default-appearance). */
  PDF_SIGNATURE_DEFAULT_APPEARANCE: any;
  /** constants.PDF_SIGNATURE_ERROR_DIGEST_FAILURE compatibility entry (case l2-constants-pdf-signature-error-digest-failure). */
  PDF_SIGNATURE_ERROR_DIGEST_FAILURE: any;
  /** constants.PDF_SIGNATURE_ERROR_NOT_SIGNED compatibility entry (case l2-constants-pdf-signature-error-not-signed). */
  PDF_SIGNATURE_ERROR_NOT_SIGNED: any;
  /** constants.PDF_SIGNATURE_ERROR_NOT_TRUSTED compatibility entry (case l2-constants-pdf-signature-error-not-trusted). */
  PDF_SIGNATURE_ERROR_NOT_TRUSTED: any;
  /** constants.PDF_SIGNATURE_ERROR_NO_CERTIFICATE compatibility entry (case l2-constants-pdf-signature-error-no-certificate). */
  PDF_SIGNATURE_ERROR_NO_CERTIFICATE: any;
  /** constants.PDF_SIGNATURE_ERROR_NO_SIGNATURES compatibility entry (case l2-constants-pdf-signature-error-no-signatures). */
  PDF_SIGNATURE_ERROR_NO_SIGNATURES: any;
  /** constants.PDF_SIGNATURE_ERROR_OKAY compatibility entry (case l2-constants-pdf-signature-error-okay). */
  PDF_SIGNATURE_ERROR_OKAY: any;
  /** constants.PDF_SIGNATURE_ERROR_SELF_SIGNED compatibility entry (case l2-constants-pdf-signature-error-self-signed). */
  PDF_SIGNATURE_ERROR_SELF_SIGNED: any;
  /** constants.PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN compatibility entry (case l2-constants-pdf-signature-error-self-signed-in-chain). */
  PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN: any;
  /** constants.PDF_SIGNATURE_ERROR_UNKNOWN compatibility entry (case l2-constants-pdf-signature-error-unknown). */
  PDF_SIGNATURE_ERROR_UNKNOWN: any;
  /** constants.PDF_SIGNATURE_SHOW_DATE compatibility entry (case l2-constants-pdf-signature-show-date). */
  PDF_SIGNATURE_SHOW_DATE: any;
  /** constants.PDF_SIGNATURE_SHOW_DN compatibility entry (case l2-constants-pdf-signature-show-dn). */
  PDF_SIGNATURE_SHOW_DN: any;
  /** constants.PDF_SIGNATURE_SHOW_GRAPHIC_NAME compatibility entry (case l2-constants-pdf-signature-show-graphic-name). */
  PDF_SIGNATURE_SHOW_GRAPHIC_NAME: any;
  /** constants.PDF_SIGNATURE_SHOW_LABELS compatibility entry (case l2-constants-pdf-signature-show-labels). */
  PDF_SIGNATURE_SHOW_LABELS: any;
  /** constants.PDF_SIGNATURE_SHOW_LOGO compatibility entry (case l2-constants-pdf-signature-show-logo). */
  PDF_SIGNATURE_SHOW_LOGO: any;
  /** constants.PDF_SIGNATURE_SHOW_TEXT_NAME compatibility entry (case l2-constants-pdf-signature-show-text-name). */
  PDF_SIGNATURE_SHOW_TEXT_NAME: any;
  /** constants.SigFlag_AppendOnly compatibility entry (case l2-constants-sigflag-appendonly). */
  SigFlag_AppendOnly: any;
  /** constants.SigFlag_SignaturesExist compatibility entry (case l2-constants-sigflag-signaturesexist). */
  SigFlag_SignaturesExist: any;
  /** constants.PDF_ALERT_BUTTON_CANCEL compatibility entry (case l2-constants-pdf-alert-button-cancel). */
  PDF_ALERT_BUTTON_CANCEL: any;
  /** constants.PDF_ALERT_BUTTON_GROUP_OK compatibility entry (case l2-constants-pdf-alert-button-group-ok). */
  PDF_ALERT_BUTTON_GROUP_OK: any;
  /** constants.PDF_ALERT_BUTTON_GROUP_OK_CANCEL compatibility entry (case l2-constants-pdf-alert-button-group-ok-cancel). */
  PDF_ALERT_BUTTON_GROUP_OK_CANCEL: any;
  /** constants.PDF_ALERT_BUTTON_GROUP_YES_NO compatibility entry (case l2-constants-pdf-alert-button-group-yes-no). */
  PDF_ALERT_BUTTON_GROUP_YES_NO: any;
  /** constants.PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL compatibility entry (case l2-constants-pdf-alert-button-group-yes-no-cancel). */
  PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL: any;
  /** constants.PDF_ALERT_BUTTON_NO compatibility entry (case l2-constants-pdf-alert-button-no). */
  PDF_ALERT_BUTTON_NO: any;
  /** constants.PDF_ALERT_BUTTON_NONE compatibility entry (case l2-constants-pdf-alert-button-none). */
  PDF_ALERT_BUTTON_NONE: any;
  /** constants.PDF_ALERT_BUTTON_OK compatibility entry (case l2-constants-pdf-alert-button-ok). */
  PDF_ALERT_BUTTON_OK: any;
  /** constants.PDF_ALERT_BUTTON_YES compatibility entry (case l2-constants-pdf-alert-button-yes). */
  PDF_ALERT_BUTTON_YES: any;
  /** constants.PDF_ALERT_ICON_ERROR compatibility entry (case l2-constants-pdf-alert-icon-error). */
  PDF_ALERT_ICON_ERROR: any;
  /** constants.PDF_ALERT_ICON_QUESTION compatibility entry (case l2-constants-pdf-alert-icon-question). */
  PDF_ALERT_ICON_QUESTION: any;
  /** constants.PDF_ALERT_ICON_STATUS compatibility entry (case l2-constants-pdf-alert-icon-status). */
  PDF_ALERT_ICON_STATUS: any;
  /** constants.PDF_ALERT_ICON_WARNING compatibility entry (case l2-constants-pdf-alert-icon-warning). */
  PDF_ALERT_ICON_WARNING: any;
  /** constants.PDF_DOCUMENT_EVENT_ALERT compatibility entry (case l2-constants-pdf-document-event-alert). */
  PDF_DOCUMENT_EVENT_ALERT: any;
  /** constants.PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM compatibility entry (case l2-constants-pdf-document-event-exec-menu-item). */
  PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM: any;
  /** constants.PDF_DOCUMENT_EVENT_LAUNCH_URL compatibility entry (case l2-constants-pdf-document-event-launch-url). */
  PDF_DOCUMENT_EVENT_LAUNCH_URL: any;
  /** constants.PDF_DOCUMENT_EVENT_MAIL_DOC compatibility entry (case l2-constants-pdf-document-event-mail-doc). */
  PDF_DOCUMENT_EVENT_MAIL_DOC: any;
  /** constants.PDF_DOCUMENT_EVENT_PRINT compatibility entry (case l2-constants-pdf-document-event-print). */
  PDF_DOCUMENT_EVENT_PRINT: any;
  /** constants.PDF_DOCUMENT_EVENT_SUBMIT compatibility entry (case l2-constants-pdf-document-event-submit). */
  PDF_DOCUMENT_EVENT_SUBMIT: any;
  /** constants.PDF_LAYER_UI_CHECKBOX compatibility entry (case l2-constants-pdf-layer-ui-checkbox). */
  PDF_LAYER_UI_CHECKBOX: any;
  /** constants.PDF_LAYER_UI_LABEL compatibility entry (case l2-constants-pdf-layer-ui-label). */
  PDF_LAYER_UI_LABEL: any;
  /** constants.PDF_LAYER_UI_RADIOBOX compatibility entry (case l2-constants-pdf-layer-ui-radiobox). */
  PDF_LAYER_UI_RADIOBOX: any;
  /** constants.PDF_OC_OFF compatibility entry (case l2-constants-pdf-oc-off). */
  PDF_OC_OFF: any;
  /** constants.PDF_OC_ON compatibility entry (case l2-constants-pdf-oc-on). */
  PDF_OC_ON: any;
  /** constants.PDF_OC_TOGGLE compatibility entry (case l2-constants-pdf-oc-toggle). */
  PDF_OC_TOGGLE: any;
  /** constants.PDF_NOT_ZUGFERD compatibility entry (case l2-constants-pdf-not-zugferd). */
  PDF_NOT_ZUGFERD: any;
  /** constants.PDF_ZUGFERD_BASIC compatibility entry (case l2-constants-pdf-zugferd-basic). */
  PDF_ZUGFERD_BASIC: any;
  /** constants.PDF_ZUGFERD_BASIC_WL compatibility entry (case l2-constants-pdf-zugferd-basic-wl). */
  PDF_ZUGFERD_BASIC_WL: any;
  /** constants.PDF_ZUGFERD_COMFORT compatibility entry (case l2-constants-pdf-zugferd-comfort). */
  PDF_ZUGFERD_COMFORT: any;
  /** constants.PDF_ZUGFERD_EXTENDED compatibility entry (case l2-constants-pdf-zugferd-extended). */
  PDF_ZUGFERD_EXTENDED: any;
  /** constants.PDF_ZUGFERD_MINIMUM compatibility entry (case l2-constants-pdf-zugferd-minimum). */
  PDF_ZUGFERD_MINIMUM: any;
  /** constants.PDF_ZUGFERD_UNKNOWN compatibility entry (case l2-constants-pdf-zugferd-unknown). */
  PDF_ZUGFERD_UNKNOWN: any;
  /** constants.PDF_ZUGFERD_XRECHNUNG compatibility entry (case l2-constants-pdf-zugferd-xrechnung). */
  PDF_ZUGFERD_XRECHNUNG: any;
  /** constants.PDF_CLEAN_STRUCTURE_DROP compatibility entry (case l2-constants-pdf-clean-structure-drop). */
  PDF_CLEAN_STRUCTURE_DROP: any;
  /** constants.PDF_CLEAN_STRUCTURE_KEEP compatibility entry (case l2-constants-pdf-clean-structure-keep). */
  PDF_CLEAN_STRUCTURE_KEEP: any;
  /** constants.PDF_CLEAN_VECTORIZE_NO compatibility entry (case l2-constants-pdf-clean-vectorize-no). */
  PDF_CLEAN_VECTORIZE_NO: any;
  /** constants.PDF_CLEAN_VECTORIZE_YES compatibility entry (case l2-constants-pdf-clean-vectorize-yes). */
  PDF_CLEAN_VECTORIZE_YES: any;
  /** constants.CS_CMYK compatibility entry (case l3-constants-cs-cmyk). */
  CS_CMYK: any;
  /** constants.CS_GRAY compatibility entry (case l3-constants-cs-gray). */
  CS_GRAY: any;
  /** constants.CS_RGB compatibility entry (case l3-constants-cs-rgb). */
  CS_RGB: any;
  /** constants.csCMYK compatibility entry (case l3-constants-cscmyk). */
  csCMYK: any;
  /** constants.csGRAY compatibility entry (case l2-constants-all). */
  csGRAY: any;
  /** constants.csRGB compatibility entry (case l2-constants-all). */
  csRGB: any;
  /** constants.FZ_RECOMPRESS_FAX compatibility entry (case l3-constants-fz-recompress-fax). */
  FZ_RECOMPRESS_FAX: any;
  /** constants.FZ_RECOMPRESS_J2K compatibility entry (case l3-constants-fz-recompress-j2k). */
  FZ_RECOMPRESS_J2K: any;
  /** constants.FZ_RECOMPRESS_JPEG compatibility entry (case l3-constants-fz-recompress-jpeg). */
  FZ_RECOMPRESS_JPEG: any;
  /** constants.FZ_RECOMPRESS_LOSSLESS compatibility entry (case l3-constants-fz-recompress-lossless). */
  FZ_RECOMPRESS_LOSSLESS: any;
  /** constants.FZ_RECOMPRESS_NEVER compatibility entry (case l3-constants-fz-recompress-never). */
  FZ_RECOMPRESS_NEVER: any;
  /** constants.FZ_RECOMPRESS_SAME compatibility entry (case l3-constants-fz-recompress-same). */
  FZ_RECOMPRESS_SAME: any;
  /** constants.FZ_SUBSAMPLE_AVERAGE compatibility entry (case l3-constants-fz-subsample-average). */
  FZ_SUBSAMPLE_AVERAGE: any;
  /** constants.FZ_SUBSAMPLE_BICUBIC compatibility entry (case l3-constants-fz-subsample-bicubic). */
  FZ_SUBSAMPLE_BICUBIC: any;
  /** constants.PDF_ANNOT_3D compatibility entry (case l4-constants-pdf-annot-3d). */
  PDF_ANNOT_3D: any;
  /** constants.PDF_ANNOT_CARET compatibility entry (case l4-constants-pdf-annot-caret). */
  PDF_ANNOT_CARET: any;
  /** constants.PDF_ANNOT_CIRCLE compatibility entry (case l4-constants-pdf-annot-circle). */
  PDF_ANNOT_CIRCLE: any;
  /** constants.PDF_ANNOT_FILE_ATTACHMENT compatibility entry (case l4-constants-pdf-annot-file-attachment). */
  PDF_ANNOT_FILE_ATTACHMENT: any;
  /** constants.PDF_ANNOT_FREE_TEXT compatibility entry (case l4-constants-pdf-annot-free-text). */
  PDF_ANNOT_FREE_TEXT: any;
  /** constants.PDF_ANNOT_HIGHLIGHT compatibility entry (case l4-constants-pdf-annot-highlight). */
  PDF_ANNOT_HIGHLIGHT: any;
  /** constants.PDF_ANNOT_INK compatibility entry (case l4-constants-pdf-annot-ink). */
  PDF_ANNOT_INK: any;
  /** constants.PDF_ANNOT_IS_HIDDEN compatibility entry (case l4-constants-pdf-annot-is-hidden). */
  PDF_ANNOT_IS_HIDDEN: any;
  /** constants.PDF_ANNOT_IS_INVISIBLE compatibility entry (case l4-constants-pdf-annot-is-invisible). */
  PDF_ANNOT_IS_INVISIBLE: any;
  /** constants.PDF_ANNOT_IS_LOCKED compatibility entry (case l4-constants-pdf-annot-is-locked). */
  PDF_ANNOT_IS_LOCKED: any;
  /** constants.PDF_ANNOT_IS_LOCKED_CONTENTS compatibility entry (case l4-constants-pdf-annot-is-locked-contents). */
  PDF_ANNOT_IS_LOCKED_CONTENTS: any;
  /** constants.PDF_ANNOT_IS_NO_ROTATE compatibility entry (case l4-constants-pdf-annot-is-no-rotate). */
  PDF_ANNOT_IS_NO_ROTATE: any;
  /** constants.PDF_ANNOT_IS_NO_VIEW compatibility entry (case l4-constants-pdf-annot-is-no-view). */
  PDF_ANNOT_IS_NO_VIEW: any;
  /** constants.PDF_ANNOT_IS_NO_ZOOM compatibility entry (case l4-constants-pdf-annot-is-no-zoom). */
  PDF_ANNOT_IS_NO_ZOOM: any;
  /** constants.PDF_ANNOT_IS_PRINT compatibility entry (case l4-constants-pdf-annot-is-print). */
  PDF_ANNOT_IS_PRINT: any;
  /** constants.PDF_ANNOT_IS_READ_ONLY compatibility entry (case l4-constants-pdf-annot-is-read-only). */
  PDF_ANNOT_IS_READ_ONLY: any;
  /** constants.PDF_ANNOT_IS_TOGGLE_NO_VIEW compatibility entry (case l4-constants-pdf-annot-is-toggle-no-view). */
  PDF_ANNOT_IS_TOGGLE_NO_VIEW: any;
  /** constants.PDF_ANNOT_IT_DEFAULT compatibility entry (case l4-constants-pdf-annot-it-default). */
  PDF_ANNOT_IT_DEFAULT: any;
  /** constants.PDF_ANNOT_IT_FREETEXT_CALLOUT compatibility entry (case l4-constants-pdf-annot-it-freetext-callout). */
  PDF_ANNOT_IT_FREETEXT_CALLOUT: any;
  /** constants.PDF_ANNOT_IT_FREETEXT_TYPEWRITER compatibility entry (case l4-constants-pdf-annot-it-freetext-typewriter). */
  PDF_ANNOT_IT_FREETEXT_TYPEWRITER: any;
  /** constants.PDF_ANNOT_IT_LINE_ARROW compatibility entry (case l4-constants-pdf-annot-it-line-arrow). */
  PDF_ANNOT_IT_LINE_ARROW: any;
  /** constants.PDF_ANNOT_IT_LINE_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-line-dimension). */
  PDF_ANNOT_IT_LINE_DIMENSION: any;
  /** constants.PDF_ANNOT_IT_POLYGON_CLOUD compatibility entry (case l4-constants-pdf-annot-it-polygon-cloud). */
  PDF_ANNOT_IT_POLYGON_CLOUD: any;
  /** constants.PDF_ANNOT_IT_POLYGON_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-polygon-dimension). */
  PDF_ANNOT_IT_POLYGON_DIMENSION: any;
  /** constants.PDF_ANNOT_IT_POLYLINE_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-polyline-dimension). */
  PDF_ANNOT_IT_POLYLINE_DIMENSION: any;
  /** constants.PDF_ANNOT_IT_STAMP_IMAGE compatibility entry (case l4-constants-pdf-annot-it-stamp-image). */
  PDF_ANNOT_IT_STAMP_IMAGE: any;
  /** constants.PDF_ANNOT_IT_STAMP_SNAPSHOT compatibility entry (case l4-constants-pdf-annot-it-stamp-snapshot). */
  PDF_ANNOT_IT_STAMP_SNAPSHOT: any;
  /** constants.PDF_ANNOT_IT_UNKNOWN compatibility entry (case l4-constants-pdf-annot-it-unknown). */
  PDF_ANNOT_IT_UNKNOWN: any;
  /** constants.PDF_ANNOT_LE_BUTT compatibility entry (case l4-constants-pdf-annot-le-butt). */
  PDF_ANNOT_LE_BUTT: any;
  /** constants.PDF_ANNOT_LE_CIRCLE compatibility entry (case l4-constants-pdf-annot-le-circle). */
  PDF_ANNOT_LE_CIRCLE: any;
  /** constants.PDF_ANNOT_LE_CLOSED_ARROW compatibility entry (case l4-constants-pdf-annot-le-closed-arrow). */
  PDF_ANNOT_LE_CLOSED_ARROW: any;
  /** constants.PDF_ANNOT_LE_DIAMOND compatibility entry (case l4-constants-pdf-annot-le-diamond). */
  PDF_ANNOT_LE_DIAMOND: any;
  /** constants.PDF_ANNOT_LE_NONE compatibility entry (case l4-constants-pdf-annot-le-none). */
  PDF_ANNOT_LE_NONE: any;
  /** constants.PDF_ANNOT_LE_OPEN_ARROW compatibility entry (case l4-constants-pdf-annot-le-open-arrow). */
  PDF_ANNOT_LE_OPEN_ARROW: any;
  /** constants.PDF_ANNOT_LE_R_CLOSED_ARROW compatibility entry (case l4-constants-pdf-annot-le-r-closed-arrow). */
  PDF_ANNOT_LE_R_CLOSED_ARROW: any;
  /** constants.PDF_ANNOT_LE_R_OPEN_ARROW compatibility entry (case l4-constants-pdf-annot-le-r-open-arrow). */
  PDF_ANNOT_LE_R_OPEN_ARROW: any;
  /** constants.PDF_ANNOT_LE_SLASH compatibility entry (case l4-constants-pdf-annot-le-slash). */
  PDF_ANNOT_LE_SLASH: any;
  /** constants.PDF_ANNOT_LE_SQUARE compatibility entry (case l4-constants-pdf-annot-le-square). */
  PDF_ANNOT_LE_SQUARE: any;
  /** constants.PDF_ANNOT_LINE compatibility entry (case l4-constants-pdf-annot-line). */
  PDF_ANNOT_LINE: any;
  /** constants.PDF_ANNOT_LINK compatibility entry (case l4-constants-pdf-annot-link). */
  PDF_ANNOT_LINK: any;
  /** constants.PDF_ANNOT_MOVIE compatibility entry (case l4-constants-pdf-annot-movie). */
  PDF_ANNOT_MOVIE: any;
  /** constants.PDF_ANNOT_POLYGON compatibility entry (case l4-constants-pdf-annot-polygon). */
  PDF_ANNOT_POLYGON: any;
  /** constants.PDF_ANNOT_POLY_LINE compatibility entry (case l4-constants-pdf-annot-poly-line). */
  PDF_ANNOT_POLY_LINE: any;
  /** constants.PDF_ANNOT_POPUP compatibility entry (case l4-constants-pdf-annot-popup). */
  PDF_ANNOT_POPUP: any;
  /** constants.PDF_ANNOT_PRINTER_MARK compatibility entry (case l4-constants-pdf-annot-printer-mark). */
  PDF_ANNOT_PRINTER_MARK: any;
  /** constants.PDF_ANNOT_PROJECTION compatibility entry (case l4-constants-pdf-annot-projection). */
  PDF_ANNOT_PROJECTION: any;
  /** constants.PDF_ANNOT_Q_CENTER compatibility entry (case l4-constants-pdf-annot-q-center). */
  PDF_ANNOT_Q_CENTER: any;
  /** constants.PDF_ANNOT_Q_LEFT compatibility entry (case l4-constants-pdf-annot-q-left). */
  PDF_ANNOT_Q_LEFT: any;
  /** constants.PDF_ANNOT_Q_RIGHT compatibility entry (case l4-constants-pdf-annot-q-right). */
  PDF_ANNOT_Q_RIGHT: any;
  /** constants.PDF_ANNOT_REDACT compatibility entry (case l4-constants-pdf-annot-redact). */
  PDF_ANNOT_REDACT: any;
  /** constants.PDF_ANNOT_RICH_MEDIA compatibility entry (case l4-constants-pdf-annot-rich-media). */
  PDF_ANNOT_RICH_MEDIA: any;
  /** constants.PDF_ANNOT_SCREEN compatibility entry (case l4-constants-pdf-annot-screen). */
  PDF_ANNOT_SCREEN: any;
  /** constants.PDF_ANNOT_SOUND compatibility entry (case l4-constants-pdf-annot-sound). */
  PDF_ANNOT_SOUND: any;
  /** constants.PDF_ANNOT_SQUARE compatibility entry (case l4-constants-pdf-annot-square). */
  PDF_ANNOT_SQUARE: any;
  /** constants.PDF_ANNOT_SQUIGGLY compatibility entry (case l4-constants-pdf-annot-squiggly). */
  PDF_ANNOT_SQUIGGLY: any;
  /** constants.PDF_ANNOT_STAMP compatibility entry (case l4-constants-pdf-annot-stamp). */
  PDF_ANNOT_STAMP: any;
  /** constants.PDF_ANNOT_STRIKE_OUT compatibility entry (case l4-constants-pdf-annot-strike-out). */
  PDF_ANNOT_STRIKE_OUT: any;
  /** constants.PDF_ANNOT_TEXT compatibility entry (case l4-constants-pdf-annot-text). */
  PDF_ANNOT_TEXT: any;
  /** constants.PDF_ANNOT_TRAP_NET compatibility entry (case l4-constants-pdf-annot-trap-net). */
  PDF_ANNOT_TRAP_NET: any;
  /** constants.PDF_ANNOT_UNDERLINE compatibility entry (case l4-constants-pdf-annot-underline). */
  PDF_ANNOT_UNDERLINE: any;
  /** constants.PDF_ANNOT_UNKNOWN compatibility entry (case l4-constants-pdf-annot-unknown). */
  PDF_ANNOT_UNKNOWN: any;
  /** constants.PDF_ANNOT_WATERMARK compatibility entry (case l4-constants-pdf-annot-watermark). */
  PDF_ANNOT_WATERMARK: any;
  /** constants.PDF_ANNOT_WIDGET compatibility entry (case l4-constants-pdf-annot-widget). */
  PDF_ANNOT_WIDGET: any;
  /** constants.PDF_BORDER_EFFECT_CLOUDY compatibility entry (case l4-constants-pdf-border-effect-cloudy). */
  PDF_BORDER_EFFECT_CLOUDY: any;
  /** constants.PDF_BORDER_EFFECT_NONE compatibility entry (case l4-constants-pdf-border-effect-none). */
  PDF_BORDER_EFFECT_NONE: any;
  /** constants.PDF_BORDER_STYLE_BEVELED compatibility entry (case l4-constants-pdf-border-style-beveled). */
  PDF_BORDER_STYLE_BEVELED: any;
  /** constants.PDF_BORDER_STYLE_DASHED compatibility entry (case l4-constants-pdf-border-style-dashed). */
  PDF_BORDER_STYLE_DASHED: any;
  /** constants.PDF_BORDER_STYLE_INSET compatibility entry (case l4-constants-pdf-border-style-inset). */
  PDF_BORDER_STYLE_INSET: any;
  /** constants.PDF_BORDER_STYLE_SOLID compatibility entry (case l4-constants-pdf-border-style-solid). */
  PDF_BORDER_STYLE_SOLID: any;
  /** constants.PDF_BORDER_STYLE_UNDERLINE compatibility entry (case l4-constants-pdf-border-style-underline). */
  PDF_BORDER_STYLE_UNDERLINE: any;
  /** constants.PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF compatibility entry (case l4-constants-pdf-btn-field-is-no-toggle-to-off). */
  PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF: any;
  /** constants.PDF_BTN_FIELD_IS_PUSHBUTTON compatibility entry (case l4-constants-pdf-btn-field-is-pushbutton). */
  PDF_BTN_FIELD_IS_PUSHBUTTON: any;
  /** constants.PDF_BTN_FIELD_IS_RADIO compatibility entry (case l4-constants-pdf-btn-field-is-radio). */
  PDF_BTN_FIELD_IS_RADIO: any;
  /** constants.PDF_BTN_FIELD_IS_RADIOS_IN_UNISON compatibility entry (case l4-constants-pdf-btn-field-is-radios-in-unison). */
  PDF_BTN_FIELD_IS_RADIOS_IN_UNISON: any;
  /** constants.PDF_CH_FIELD_IS_COMBO compatibility entry (case l4-constants-pdf-ch-field-is-combo). */
  PDF_CH_FIELD_IS_COMBO: any;
  /** constants.PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE compatibility entry (case l4-constants-pdf-ch-field-is-commit-on-sel-change). */
  PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE: any;
  /** constants.PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK compatibility entry (case l4-constants-pdf-ch-field-is-do-not-spell-check). */
  PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK: any;
  /** constants.PDF_CH_FIELD_IS_EDIT compatibility entry (case l4-constants-pdf-ch-field-is-edit). */
  PDF_CH_FIELD_IS_EDIT: any;
  /** constants.PDF_CH_FIELD_IS_MULTI_SELECT compatibility entry (case l4-constants-pdf-ch-field-is-multi-select). */
  PDF_CH_FIELD_IS_MULTI_SELECT: any;
  /** constants.PDF_CH_FIELD_IS_SORT compatibility entry (case l4-constants-pdf-ch-field-is-sort). */
  PDF_CH_FIELD_IS_SORT: any;
  /** constants.PDF_FIELD_IS_NO_EXPORT compatibility entry (case l4-constants-pdf-field-is-no-export). */
  PDF_FIELD_IS_NO_EXPORT: any;
  /** constants.PDF_FIELD_IS_READ_ONLY compatibility entry (case l4-constants-pdf-field-is-read-only). */
  PDF_FIELD_IS_READ_ONLY: any;
  /** constants.PDF_FIELD_IS_REQUIRED compatibility entry (case l4-constants-pdf-field-is-required). */
  PDF_FIELD_IS_REQUIRED: any;
  /** constants.PDF_TX_FIELD_IS_COMB compatibility entry (case l4-constants-pdf-tx-field-is-comb). */
  PDF_TX_FIELD_IS_COMB: any;
  /** constants.PDF_TX_FIELD_IS_DO_NOT_SCROLL compatibility entry (case l4-constants-pdf-tx-field-is-do-not-scroll). */
  PDF_TX_FIELD_IS_DO_NOT_SCROLL: any;
  /** constants.PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK compatibility entry (case l4-constants-pdf-tx-field-is-do-not-spell-check). */
  PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK: any;
  /** constants.PDF_TX_FIELD_IS_FILE_SELECT compatibility entry (case l4-constants-pdf-tx-field-is-file-select). */
  PDF_TX_FIELD_IS_FILE_SELECT: any;
  /** constants.PDF_TX_FIELD_IS_MULTILINE compatibility entry (case l4-constants-pdf-tx-field-is-multiline). */
  PDF_TX_FIELD_IS_MULTILINE: any;
  /** constants.PDF_TX_FIELD_IS_PASSWORD compatibility entry (case l4-constants-pdf-tx-field-is-password). */
  PDF_TX_FIELD_IS_PASSWORD: any;
  /** constants.PDF_TX_FIELD_IS_RICH_TEXT compatibility entry (case l4-constants-pdf-tx-field-is-rich-text). */
  PDF_TX_FIELD_IS_RICH_TEXT: any;
  /** constants.PDF_WIDGET_TX_FORMAT_DATE compatibility entry (case l4-constants-pdf-widget-tx-format-date). */
  PDF_WIDGET_TX_FORMAT_DATE: any;
  /** constants.PDF_WIDGET_TX_FORMAT_NONE compatibility entry (case l4-constants-pdf-widget-tx-format-none). */
  PDF_WIDGET_TX_FORMAT_NONE: any;
  /** constants.PDF_WIDGET_TX_FORMAT_NUMBER compatibility entry (case l4-constants-pdf-widget-tx-format-number). */
  PDF_WIDGET_TX_FORMAT_NUMBER: any;
  /** constants.PDF_WIDGET_TX_FORMAT_SPECIAL compatibility entry (case l4-constants-pdf-widget-tx-format-special). */
  PDF_WIDGET_TX_FORMAT_SPECIAL: any;
  /** constants.PDF_WIDGET_TX_FORMAT_TIME compatibility entry (case l4-constants-pdf-widget-tx-format-time). */
  PDF_WIDGET_TX_FORMAT_TIME: any;
  /** constants.PDF_WIDGET_TYPE_BUTTON compatibility entry (case l4-constants-pdf-widget-type-button). */
  PDF_WIDGET_TYPE_BUTTON: any;
  /** constants.PDF_WIDGET_TYPE_CHECKBOX compatibility entry (case l4-constants-pdf-widget-type-checkbox). */
  PDF_WIDGET_TYPE_CHECKBOX: any;
  /** constants.PDF_WIDGET_TYPE_COMBOBOX compatibility entry (case l4-constants-pdf-widget-type-combobox). */
  PDF_WIDGET_TYPE_COMBOBOX: any;
  /** constants.PDF_WIDGET_TYPE_LISTBOX compatibility entry (case l4-constants-pdf-widget-type-listbox). */
  PDF_WIDGET_TYPE_LISTBOX: any;
  /** constants.PDF_WIDGET_TYPE_RADIOBUTTON compatibility entry (case l4-constants-pdf-widget-type-radiobutton). */
  PDF_WIDGET_TYPE_RADIOBUTTON: any;
  /** constants.PDF_WIDGET_TYPE_SIGNATURE compatibility entry (case l4-constants-pdf-widget-type-signature). */
  PDF_WIDGET_TYPE_SIGNATURE: any;
  /** constants.PDF_WIDGET_TYPE_TEXT compatibility entry (case l2-constants-all). */
  PDF_WIDGET_TYPE_TEXT: any;
  /** constants.PDF_WIDGET_TYPE_UNKNOWN compatibility entry (case l4-constants-pdf-widget-type-unknown). */
  PDF_WIDGET_TYPE_UNKNOWN: any;
  /** constants.STAMP_Approved compatibility entry (case l4-constants-stamp-approved). */
  STAMP_Approved: any;
  /** constants.STAMP_AsIs compatibility entry (case l4-constants-stamp-asis). */
  STAMP_AsIs: any;
  /** constants.STAMP_Confidential compatibility entry (case l4-constants-stamp-confidential). */
  STAMP_Confidential: any;
  /** constants.STAMP_Departmental compatibility entry (case l4-constants-stamp-departmental). */
  STAMP_Departmental: any;
  /** constants.STAMP_Draft compatibility entry (case l4-constants-stamp-draft). */
  STAMP_Draft: any;
  /** constants.STAMP_Experimental compatibility entry (case l4-constants-stamp-experimental). */
  STAMP_Experimental: any;
  /** constants.STAMP_Expired compatibility entry (case l4-constants-stamp-expired). */
  STAMP_Expired: any;
  /** constants.STAMP_Final compatibility entry (case l4-constants-stamp-final). */
  STAMP_Final: any;
  /** constants.STAMP_ForComment compatibility entry (case l4-constants-stamp-forcomment). */
  STAMP_ForComment: any;
  /** constants.STAMP_ForPublicRelease compatibility entry (case l4-constants-stamp-forpublicrelease). */
  STAMP_ForPublicRelease: any;
  /** constants.STAMP_NotApproved compatibility entry (case l4-constants-stamp-notapproved). */
  STAMP_NotApproved: any;
  /** constants.STAMP_NotForPublicRelease compatibility entry (case l4-constants-stamp-notforpublicrelease). */
  STAMP_NotForPublicRelease: any;
  /** constants.STAMP_Sold compatibility entry (case l4-constants-stamp-sold). */
  STAMP_Sold: any;
  /** constants.STAMP_TopSecret compatibility entry (case l4-constants-stamp-topsecret). */
  STAMP_TopSecret: any;
  /** constants.PDF_BM_Color compatibility entry (case l4-constants-pdf-bm-color). */
  PDF_BM_Color: any;
  /** constants.PDF_BM_ColorBurn compatibility entry (case l4-constants-pdf-bm-colorburn). */
  PDF_BM_ColorBurn: any;
  /** constants.PDF_BM_ColorDodge compatibility entry (case l4-constants-pdf-bm-colordodge). */
  PDF_BM_ColorDodge: any;
  /** constants.PDF_BM_Darken compatibility entry (case l4-constants-pdf-bm-darken). */
  PDF_BM_Darken: any;
  /** constants.PDF_BM_Difference compatibility entry (case l4-constants-pdf-bm-difference). */
  PDF_BM_Difference: any;
  /** constants.PDF_BM_Exclusion compatibility entry (case l4-constants-pdf-bm-exclusion). */
  PDF_BM_Exclusion: any;
  /** constants.PDF_BM_HardLight compatibility entry (case l4-constants-pdf-bm-hardlight). */
  PDF_BM_HardLight: any;
  /** constants.PDF_BM_Hue compatibility entry (case l4-constants-pdf-bm-hue). */
  PDF_BM_Hue: any;
  /** constants.PDF_BM_Lighten compatibility entry (case l4-constants-pdf-bm-lighten). */
  PDF_BM_Lighten: any;
  /** constants.PDF_BM_Luminosity compatibility entry (case l4-constants-pdf-bm-luminosity). */
  PDF_BM_Luminosity: any;
  /** constants.PDF_BM_Multiply compatibility entry (case l4-constants-pdf-bm-multiply). */
  PDF_BM_Multiply: any;
  /** constants.PDF_BM_Normal compatibility entry (case l4-constants-pdf-bm-normal). */
  PDF_BM_Normal: any;
  /** constants.PDF_BM_Overlay compatibility entry (case l4-constants-pdf-bm-overlay). */
  PDF_BM_Overlay: any;
  /** constants.PDF_BM_Saturation compatibility entry (case l4-constants-pdf-bm-saturation). */
  PDF_BM_Saturation: any;
  /** constants.PDF_BM_Screen compatibility entry (case l4-constants-pdf-bm-screen). */
  PDF_BM_Screen: any;
  /** constants.PDF_BM_SoftLight compatibility entry (case l4-constants-pdf-bm-softlight). */
  PDF_BM_SoftLight: any;
  /** constants.PDF_REDACT_IMAGE_NONE compatibility entry (case l2-constants-all). */
  PDF_REDACT_IMAGE_NONE: any;
  /** constants.PDF_REDACT_IMAGE_PIXELS compatibility entry (case l4-constants-pdf-redact-image-pixels). */
  PDF_REDACT_IMAGE_PIXELS: any;
  /** constants.PDF_REDACT_IMAGE_REMOVE compatibility entry (case l4-constants-pdf-redact-image-remove). */
  PDF_REDACT_IMAGE_REMOVE: any;
  /** constants.PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE compatibility entry (case l4-constants-pdf-redact-image-remove-unless-invisible). */
  PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE: any;
  /** constants.PDF_REDACT_LINE_ART_NONE compatibility entry (case l2-constants-all). */
  PDF_REDACT_LINE_ART_NONE: any;
  /** constants.PDF_REDACT_LINE_ART_REMOVE_IF_COVERED compatibility entry (case l4-constants-pdf-redact-line-art-remove-if-covered). */
  PDF_REDACT_LINE_ART_REMOVE_IF_COVERED: any;
  /** constants.PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED compatibility entry (case l4-constants-pdf-redact-line-art-remove-if-touched). */
  PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED: any;
  /** constants.PDF_REDACT_TEXT_NONE compatibility entry (case l4-constants-pdf-redact-text-none). */
  PDF_REDACT_TEXT_NONE: any;
  /** constants.PDF_REDACT_TEXT_REMOVE compatibility entry (case l4-constants-pdf-redact-text-remove). */
  PDF_REDACT_TEXT_REMOVE: any;
  /** constants.PDF_REDACT_TEXT_REMOVE_INVISIBLE compatibility entry (case l4-constants-pdf-redact-text-remove-invisible). */
  PDF_REDACT_TEXT_REMOVE_INVISIBLE: any;
  /** constants.UCDN_SCRIPT_ADLAM compatibility entry (case l4-constants-ucdn-script-adlam). */
  UCDN_SCRIPT_ADLAM: any;
  /** constants.UCDN_SCRIPT_AHOM compatibility entry (case l4-constants-ucdn-script-ahom). */
  UCDN_SCRIPT_AHOM: any;
  /** constants.UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-anatolian-hieroglyphs). */
  UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS: any;
  /** constants.UCDN_SCRIPT_ARABIC compatibility entry (case l4-constants-ucdn-script-arabic). */
  UCDN_SCRIPT_ARABIC: any;
  /** constants.UCDN_SCRIPT_ARMENIAN compatibility entry (case l4-constants-ucdn-script-armenian). */
  UCDN_SCRIPT_ARMENIAN: any;
  /** constants.UCDN_SCRIPT_AVESTAN compatibility entry (case l4-constants-ucdn-script-avestan). */
  UCDN_SCRIPT_AVESTAN: any;
  /** constants.UCDN_SCRIPT_BALINESE compatibility entry (case l4-constants-ucdn-script-balinese). */
  UCDN_SCRIPT_BALINESE: any;
  /** constants.UCDN_SCRIPT_BAMUM compatibility entry (case l4-constants-ucdn-script-bamum). */
  UCDN_SCRIPT_BAMUM: any;
  /** constants.UCDN_SCRIPT_BASSA_VAH compatibility entry (case l4-constants-ucdn-script-bassa-vah). */
  UCDN_SCRIPT_BASSA_VAH: any;
  /** constants.UCDN_SCRIPT_BATAK compatibility entry (case l4-constants-ucdn-script-batak). */
  UCDN_SCRIPT_BATAK: any;
  /** constants.UCDN_SCRIPT_BENGALI compatibility entry (case l4-constants-ucdn-script-bengali). */
  UCDN_SCRIPT_BENGALI: any;
  /** constants.UCDN_SCRIPT_BHAIKSUKI compatibility entry (case l4-constants-ucdn-script-bhaiksuki). */
  UCDN_SCRIPT_BHAIKSUKI: any;
  /** constants.UCDN_SCRIPT_BOPOMOFO compatibility entry (case l4-constants-ucdn-script-bopomofo). */
  UCDN_SCRIPT_BOPOMOFO: any;
  /** constants.UCDN_SCRIPT_BRAHMI compatibility entry (case l4-constants-ucdn-script-brahmi). */
  UCDN_SCRIPT_BRAHMI: any;
  /** constants.UCDN_SCRIPT_BRAILLE compatibility entry (case l4-constants-ucdn-script-braille). */
  UCDN_SCRIPT_BRAILLE: any;
  /** constants.UCDN_SCRIPT_BUGINESE compatibility entry (case l4-constants-ucdn-script-buginese). */
  UCDN_SCRIPT_BUGINESE: any;
  /** constants.UCDN_SCRIPT_BUHID compatibility entry (case l4-constants-ucdn-script-buhid). */
  UCDN_SCRIPT_BUHID: any;
  /** constants.UCDN_SCRIPT_CANADIAN_ABORIGINAL compatibility entry (case l4-constants-ucdn-script-canadian-aboriginal). */
  UCDN_SCRIPT_CANADIAN_ABORIGINAL: any;
  /** constants.UCDN_SCRIPT_CARIAN compatibility entry (case l4-constants-ucdn-script-carian). */
  UCDN_SCRIPT_CARIAN: any;
  /** constants.UCDN_SCRIPT_CAUCASIAN_ALBANIAN compatibility entry (case l4-constants-ucdn-script-caucasian-albanian). */
  UCDN_SCRIPT_CAUCASIAN_ALBANIAN: any;
  /** constants.UCDN_SCRIPT_CHAKMA compatibility entry (case l4-constants-ucdn-script-chakma). */
  UCDN_SCRIPT_CHAKMA: any;
  /** constants.UCDN_SCRIPT_CHAM compatibility entry (case l4-constants-ucdn-script-cham). */
  UCDN_SCRIPT_CHAM: any;
  /** constants.UCDN_SCRIPT_CHEROKEE compatibility entry (case l4-constants-ucdn-script-cherokee). */
  UCDN_SCRIPT_CHEROKEE: any;
  /** constants.UCDN_SCRIPT_CHORASMIAN compatibility entry (case l4-constants-ucdn-script-chorasmian). */
  UCDN_SCRIPT_CHORASMIAN: any;
  /** constants.UCDN_SCRIPT_COMMON compatibility entry (case l4-constants-ucdn-script-common). */
  UCDN_SCRIPT_COMMON: any;
  /** constants.UCDN_SCRIPT_COPTIC compatibility entry (case l4-constants-ucdn-script-coptic). */
  UCDN_SCRIPT_COPTIC: any;
  /** constants.UCDN_SCRIPT_CUNEIFORM compatibility entry (case l4-constants-ucdn-script-cuneiform). */
  UCDN_SCRIPT_CUNEIFORM: any;
  /** constants.UCDN_SCRIPT_CYPRIOT compatibility entry (case l4-constants-ucdn-script-cypriot). */
  UCDN_SCRIPT_CYPRIOT: any;
  /** constants.UCDN_SCRIPT_CYPRO_MINOAN compatibility entry (case l4-constants-ucdn-script-cypro-minoan). */
  UCDN_SCRIPT_CYPRO_MINOAN: any;
  /** constants.UCDN_SCRIPT_CYRILLIC compatibility entry (case l4-constants-ucdn-script-cyrillic). */
  UCDN_SCRIPT_CYRILLIC: any;
  /** constants.UCDN_SCRIPT_DESERET compatibility entry (case l4-constants-ucdn-script-deseret). */
  UCDN_SCRIPT_DESERET: any;
  /** constants.UCDN_SCRIPT_DEVANAGARI compatibility entry (case l4-constants-ucdn-script-devanagari). */
  UCDN_SCRIPT_DEVANAGARI: any;
  /** constants.UCDN_SCRIPT_DIVES_AKURU compatibility entry (case l4-constants-ucdn-script-dives-akuru). */
  UCDN_SCRIPT_DIVES_AKURU: any;
  /** constants.UCDN_SCRIPT_DOGRA compatibility entry (case l4-constants-ucdn-script-dogra). */
  UCDN_SCRIPT_DOGRA: any;
  /** constants.UCDN_SCRIPT_DUPLOYAN compatibility entry (case l4-constants-ucdn-script-duployan). */
  UCDN_SCRIPT_DUPLOYAN: any;
  /** constants.UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-egyptian-hieroglyphs). */
  UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS: any;
  /** constants.UCDN_SCRIPT_ELBASAN compatibility entry (case l4-constants-ucdn-script-elbasan). */
  UCDN_SCRIPT_ELBASAN: any;
  /** constants.UCDN_SCRIPT_ELYMAIC compatibility entry (case l4-constants-ucdn-script-elymaic). */
  UCDN_SCRIPT_ELYMAIC: any;
  /** constants.UCDN_SCRIPT_ETHIOPIC compatibility entry (case l4-constants-ucdn-script-ethiopic). */
  UCDN_SCRIPT_ETHIOPIC: any;
  /** constants.UCDN_SCRIPT_GARAY compatibility entry (case l4-constants-ucdn-script-garay). */
  UCDN_SCRIPT_GARAY: any;
  /** constants.UCDN_SCRIPT_GEORGIAN compatibility entry (case l4-constants-ucdn-script-georgian). */
  UCDN_SCRIPT_GEORGIAN: any;
  /** constants.UCDN_SCRIPT_GLAGOLITIC compatibility entry (case l4-constants-ucdn-script-glagolitic). */
  UCDN_SCRIPT_GLAGOLITIC: any;
  /** constants.UCDN_SCRIPT_GOTHIC compatibility entry (case l4-constants-ucdn-script-gothic). */
  UCDN_SCRIPT_GOTHIC: any;
  /** constants.UCDN_SCRIPT_GRANTHA compatibility entry (case l4-constants-ucdn-script-grantha). */
  UCDN_SCRIPT_GRANTHA: any;
  /** constants.UCDN_SCRIPT_GREEK compatibility entry (case l4-constants-ucdn-script-greek). */
  UCDN_SCRIPT_GREEK: any;
  /** constants.UCDN_SCRIPT_GUJARATI compatibility entry (case l4-constants-ucdn-script-gujarati). */
  UCDN_SCRIPT_GUJARATI: any;
  /** constants.UCDN_SCRIPT_GUNJALA_GONDI compatibility entry (case l4-constants-ucdn-script-gunjala-gondi). */
  UCDN_SCRIPT_GUNJALA_GONDI: any;
  /** constants.UCDN_SCRIPT_GURMUKHI compatibility entry (case l4-constants-ucdn-script-gurmukhi). */
  UCDN_SCRIPT_GURMUKHI: any;
  /** constants.UCDN_SCRIPT_GURUNG_KHEMA compatibility entry (case l4-constants-ucdn-script-gurung-khema). */
  UCDN_SCRIPT_GURUNG_KHEMA: any;
  /** constants.UCDN_SCRIPT_HAN compatibility entry (case l4-constants-ucdn-script-han). */
  UCDN_SCRIPT_HAN: any;
  /** constants.UCDN_SCRIPT_HANGUL compatibility entry (case l4-constants-ucdn-script-hangul). */
  UCDN_SCRIPT_HANGUL: any;
  /** constants.UCDN_SCRIPT_HANIFI_ROHINGYA compatibility entry (case l4-constants-ucdn-script-hanifi-rohingya). */
  UCDN_SCRIPT_HANIFI_ROHINGYA: any;
  /** constants.UCDN_SCRIPT_HANUNOO compatibility entry (case l4-constants-ucdn-script-hanunoo). */
  UCDN_SCRIPT_HANUNOO: any;
  /** constants.UCDN_SCRIPT_HATRAN compatibility entry (case l4-constants-ucdn-script-hatran). */
  UCDN_SCRIPT_HATRAN: any;
  /** constants.UCDN_SCRIPT_HEBREW compatibility entry (case l4-constants-ucdn-script-hebrew). */
  UCDN_SCRIPT_HEBREW: any;
  /** constants.UCDN_SCRIPT_HIRAGANA compatibility entry (case l4-constants-ucdn-script-hiragana). */
  UCDN_SCRIPT_HIRAGANA: any;
  /** constants.UCDN_SCRIPT_IMPERIAL_ARAMAIC compatibility entry (case l4-constants-ucdn-script-imperial-aramaic). */
  UCDN_SCRIPT_IMPERIAL_ARAMAIC: any;
  /** constants.UCDN_SCRIPT_INHERITED compatibility entry (case l4-constants-ucdn-script-inherited). */
  UCDN_SCRIPT_INHERITED: any;
  /** constants.UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI compatibility entry (case l4-constants-ucdn-script-inscriptional-pahlavi). */
  UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI: any;
  /** constants.UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN compatibility entry (case l4-constants-ucdn-script-inscriptional-parthian). */
  UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN: any;
  /** constants.UCDN_SCRIPT_JAVANESE compatibility entry (case l4-constants-ucdn-script-javanese). */
  UCDN_SCRIPT_JAVANESE: any;
  /** constants.UCDN_SCRIPT_KAITHI compatibility entry (case l4-constants-ucdn-script-kaithi). */
  UCDN_SCRIPT_KAITHI: any;
  /** constants.UCDN_SCRIPT_KANNADA compatibility entry (case l4-constants-ucdn-script-kannada). */
  UCDN_SCRIPT_KANNADA: any;
  /** constants.UCDN_SCRIPT_KATAKANA compatibility entry (case l4-constants-ucdn-script-katakana). */
  UCDN_SCRIPT_KATAKANA: any;
  /** constants.UCDN_SCRIPT_KAWI compatibility entry (case l4-constants-ucdn-script-kawi). */
  UCDN_SCRIPT_KAWI: any;
  /** constants.UCDN_SCRIPT_KAYAH_LI compatibility entry (case l4-constants-ucdn-script-kayah-li). */
  UCDN_SCRIPT_KAYAH_LI: any;
  /** constants.UCDN_SCRIPT_KHAROSHTHI compatibility entry (case l4-constants-ucdn-script-kharoshthi). */
  UCDN_SCRIPT_KHAROSHTHI: any;
  /** constants.UCDN_SCRIPT_KHITAN_SMALL_SCRIPT compatibility entry (case l4-constants-ucdn-script-khitan-small-script). */
  UCDN_SCRIPT_KHITAN_SMALL_SCRIPT: any;
  /** constants.UCDN_SCRIPT_KHMER compatibility entry (case l4-constants-ucdn-script-khmer). */
  UCDN_SCRIPT_KHMER: any;
  /** constants.UCDN_SCRIPT_KHOJKI compatibility entry (case l4-constants-ucdn-script-khojki). */
  UCDN_SCRIPT_KHOJKI: any;
  /** constants.UCDN_SCRIPT_KHUDAWADI compatibility entry (case l4-constants-ucdn-script-khudawadi). */
  UCDN_SCRIPT_KHUDAWADI: any;
  /** constants.UCDN_SCRIPT_KIRAT_RAI compatibility entry (case l4-constants-ucdn-script-kirat-rai). */
  UCDN_SCRIPT_KIRAT_RAI: any;
  /** constants.UCDN_SCRIPT_LAO compatibility entry (case l4-constants-ucdn-script-lao). */
  UCDN_SCRIPT_LAO: any;
  /** constants.UCDN_SCRIPT_LATIN compatibility entry (case l4-constants-ucdn-script-latin). */
  UCDN_SCRIPT_LATIN: any;
  /** constants.UCDN_SCRIPT_LEPCHA compatibility entry (case l4-constants-ucdn-script-lepcha). */
  UCDN_SCRIPT_LEPCHA: any;
  /** constants.UCDN_SCRIPT_LIMBU compatibility entry (case l4-constants-ucdn-script-limbu). */
  UCDN_SCRIPT_LIMBU: any;
  /** constants.UCDN_SCRIPT_LINEAR_A compatibility entry (case l4-constants-ucdn-script-linear-a). */
  UCDN_SCRIPT_LINEAR_A: any;
  /** constants.UCDN_SCRIPT_LINEAR_B compatibility entry (case l4-constants-ucdn-script-linear-b). */
  UCDN_SCRIPT_LINEAR_B: any;
  /** constants.UCDN_SCRIPT_LISU compatibility entry (case l4-constants-ucdn-script-lisu). */
  UCDN_SCRIPT_LISU: any;
  /** constants.UCDN_SCRIPT_LYCIAN compatibility entry (case l4-constants-ucdn-script-lycian). */
  UCDN_SCRIPT_LYCIAN: any;
  /** constants.UCDN_SCRIPT_LYDIAN compatibility entry (case l4-constants-ucdn-script-lydian). */
  UCDN_SCRIPT_LYDIAN: any;
  /** constants.UCDN_SCRIPT_MAHAJANI compatibility entry (case l4-constants-ucdn-script-mahajani). */
  UCDN_SCRIPT_MAHAJANI: any;
  /** constants.UCDN_SCRIPT_MAKASAR compatibility entry (case l4-constants-ucdn-script-makasar). */
  UCDN_SCRIPT_MAKASAR: any;
  /** constants.UCDN_SCRIPT_MALAYALAM compatibility entry (case l4-constants-ucdn-script-malayalam). */
  UCDN_SCRIPT_MALAYALAM: any;
  /** constants.UCDN_SCRIPT_MANDAIC compatibility entry (case l4-constants-ucdn-script-mandaic). */
  UCDN_SCRIPT_MANDAIC: any;
  /** constants.UCDN_SCRIPT_MANICHAEAN compatibility entry (case l4-constants-ucdn-script-manichaean). */
  UCDN_SCRIPT_MANICHAEAN: any;
  /** constants.UCDN_SCRIPT_MARCHEN compatibility entry (case l4-constants-ucdn-script-marchen). */
  UCDN_SCRIPT_MARCHEN: any;
  /** constants.UCDN_SCRIPT_MASARAM_GONDI compatibility entry (case l4-constants-ucdn-script-masaram-gondi). */
  UCDN_SCRIPT_MASARAM_GONDI: any;
  /** constants.UCDN_SCRIPT_MEDEFAIDRIN compatibility entry (case l4-constants-ucdn-script-medefaidrin). */
  UCDN_SCRIPT_MEDEFAIDRIN: any;
  /** constants.UCDN_SCRIPT_MEETEI_MAYEK compatibility entry (case l4-constants-ucdn-script-meetei-mayek). */
  UCDN_SCRIPT_MEETEI_MAYEK: any;
  /** constants.UCDN_SCRIPT_MENDE_KIKAKUI compatibility entry (case l4-constants-ucdn-script-mende-kikakui). */
  UCDN_SCRIPT_MENDE_KIKAKUI: any;
  /** constants.UCDN_SCRIPT_MEROITIC_CURSIVE compatibility entry (case l4-constants-ucdn-script-meroitic-cursive). */
  UCDN_SCRIPT_MEROITIC_CURSIVE: any;
  /** constants.UCDN_SCRIPT_MEROITIC_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-meroitic-hieroglyphs). */
  UCDN_SCRIPT_MEROITIC_HIEROGLYPHS: any;
  /** constants.UCDN_SCRIPT_MIAO compatibility entry (case l4-constants-ucdn-script-miao). */
  UCDN_SCRIPT_MIAO: any;
  /** constants.UCDN_SCRIPT_MODI compatibility entry (case l4-constants-ucdn-script-modi). */
  UCDN_SCRIPT_MODI: any;
  /** constants.UCDN_SCRIPT_MONGOLIAN compatibility entry (case l4-constants-ucdn-script-mongolian). */
  UCDN_SCRIPT_MONGOLIAN: any;
  /** constants.UCDN_SCRIPT_MRO compatibility entry (case l4-constants-ucdn-script-mro). */
  UCDN_SCRIPT_MRO: any;
  /** constants.UCDN_SCRIPT_MULTANI compatibility entry (case l4-constants-ucdn-script-multani). */
  UCDN_SCRIPT_MULTANI: any;
  /** constants.UCDN_SCRIPT_MYANMAR compatibility entry (case l4-constants-ucdn-script-myanmar). */
  UCDN_SCRIPT_MYANMAR: any;
  /** constants.UCDN_SCRIPT_NABATAEAN compatibility entry (case l4-constants-ucdn-script-nabataean). */
  UCDN_SCRIPT_NABATAEAN: any;
  /** constants.UCDN_SCRIPT_NAG_MUNDARI compatibility entry (case l4-constants-ucdn-script-nag-mundari). */
  UCDN_SCRIPT_NAG_MUNDARI: any;
  /** constants.UCDN_SCRIPT_NANDINAGARI compatibility entry (case l4-constants-ucdn-script-nandinagari). */
  UCDN_SCRIPT_NANDINAGARI: any;
  /** constants.UCDN_SCRIPT_NEWA compatibility entry (case l4-constants-ucdn-script-newa). */
  UCDN_SCRIPT_NEWA: any;
  /** constants.UCDN_SCRIPT_NEW_TAI_LUE compatibility entry (case l4-constants-ucdn-script-new-tai-lue). */
  UCDN_SCRIPT_NEW_TAI_LUE: any;
  /** constants.UCDN_SCRIPT_NKO compatibility entry (case l4-constants-ucdn-script-nko). */
  UCDN_SCRIPT_NKO: any;
  /** constants.UCDN_SCRIPT_NUSHU compatibility entry (case l4-constants-ucdn-script-nushu). */
  UCDN_SCRIPT_NUSHU: any;
  /** constants.UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG compatibility entry (case l4-constants-ucdn-script-nyiakeng-puachue-hmong). */
  UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG: any;
  /** constants.UCDN_SCRIPT_OGHAM compatibility entry (case l4-constants-ucdn-script-ogham). */
  UCDN_SCRIPT_OGHAM: any;
  /** constants.UCDN_SCRIPT_OLD_HUNGARIAN compatibility entry (case l4-constants-ucdn-script-old-hungarian). */
  UCDN_SCRIPT_OLD_HUNGARIAN: any;
  /** constants.UCDN_SCRIPT_OLD_ITALIC compatibility entry (case l4-constants-ucdn-script-old-italic). */
  UCDN_SCRIPT_OLD_ITALIC: any;
  /** constants.UCDN_SCRIPT_OLD_NORTH_ARABIAN compatibility entry (case l4-constants-ucdn-script-old-north-arabian). */
  UCDN_SCRIPT_OLD_NORTH_ARABIAN: any;
  /** constants.UCDN_SCRIPT_OLD_PERMIC compatibility entry (case l4-constants-ucdn-script-old-permic). */
  UCDN_SCRIPT_OLD_PERMIC: any;
  /** constants.UCDN_SCRIPT_OLD_PERSIAN compatibility entry (case l4-constants-ucdn-script-old-persian). */
  UCDN_SCRIPT_OLD_PERSIAN: any;
  /** constants.UCDN_SCRIPT_OLD_SOGDIAN compatibility entry (case l4-constants-ucdn-script-old-sogdian). */
  UCDN_SCRIPT_OLD_SOGDIAN: any;
  /** constants.UCDN_SCRIPT_OLD_SOUTH_ARABIAN compatibility entry (case l4-constants-ucdn-script-old-south-arabian). */
  UCDN_SCRIPT_OLD_SOUTH_ARABIAN: any;
  /** constants.UCDN_SCRIPT_OLD_TURKIC compatibility entry (case l4-constants-ucdn-script-old-turkic). */
  UCDN_SCRIPT_OLD_TURKIC: any;
  /** constants.UCDN_SCRIPT_OLD_UYGHUR compatibility entry (case l4-constants-ucdn-script-old-uyghur). */
  UCDN_SCRIPT_OLD_UYGHUR: any;
  /** constants.UCDN_SCRIPT_OL_CHIKI compatibility entry (case l4-constants-ucdn-script-ol-chiki). */
  UCDN_SCRIPT_OL_CHIKI: any;
  /** constants.UCDN_SCRIPT_OL_ONAL compatibility entry (case l4-constants-ucdn-script-ol-onal). */
  UCDN_SCRIPT_OL_ONAL: any;
  /** constants.UCDN_SCRIPT_ORIYA compatibility entry (case l4-constants-ucdn-script-oriya). */
  UCDN_SCRIPT_ORIYA: any;
  /** constants.UCDN_SCRIPT_OSAGE compatibility entry (case l4-constants-ucdn-script-osage). */
  UCDN_SCRIPT_OSAGE: any;
  /** constants.UCDN_SCRIPT_OSMANYA compatibility entry (case l4-constants-ucdn-script-osmanya). */
  UCDN_SCRIPT_OSMANYA: any;
  /** constants.UCDN_SCRIPT_PAHAWH_HMONG compatibility entry (case l4-constants-ucdn-script-pahawh-hmong). */
  UCDN_SCRIPT_PAHAWH_HMONG: any;
  /** constants.UCDN_SCRIPT_PALMYRENE compatibility entry (case l4-constants-ucdn-script-palmyrene). */
  UCDN_SCRIPT_PALMYRENE: any;
  /** constants.UCDN_SCRIPT_PAU_CIN_HAU compatibility entry (case l4-constants-ucdn-script-pau-cin-hau). */
  UCDN_SCRIPT_PAU_CIN_HAU: any;
  /** constants.UCDN_SCRIPT_PHAGS_PA compatibility entry (case l4-constants-ucdn-script-phags-pa). */
  UCDN_SCRIPT_PHAGS_PA: any;
  /** constants.UCDN_SCRIPT_PHOENICIAN compatibility entry (case l4-constants-ucdn-script-phoenician). */
  UCDN_SCRIPT_PHOENICIAN: any;
  /** constants.UCDN_SCRIPT_PSALTER_PAHLAVI compatibility entry (case l4-constants-ucdn-script-psalter-pahlavi). */
  UCDN_SCRIPT_PSALTER_PAHLAVI: any;
  /** constants.UCDN_SCRIPT_REJANG compatibility entry (case l4-constants-ucdn-script-rejang). */
  UCDN_SCRIPT_REJANG: any;
  /** constants.UCDN_SCRIPT_RUNIC compatibility entry (case l4-constants-ucdn-script-runic). */
  UCDN_SCRIPT_RUNIC: any;
  /** constants.UCDN_SCRIPT_SAMARITAN compatibility entry (case l4-constants-ucdn-script-samaritan). */
  UCDN_SCRIPT_SAMARITAN: any;
  /** constants.UCDN_SCRIPT_SAURASHTRA compatibility entry (case l4-constants-ucdn-script-saurashtra). */
  UCDN_SCRIPT_SAURASHTRA: any;
  /** constants.UCDN_SCRIPT_SHARADA compatibility entry (case l4-constants-ucdn-script-sharada). */
  UCDN_SCRIPT_SHARADA: any;
  /** constants.UCDN_SCRIPT_SHAVIAN compatibility entry (case l4-constants-ucdn-script-shavian). */
  UCDN_SCRIPT_SHAVIAN: any;
  /** constants.UCDN_SCRIPT_SIDDHAM compatibility entry (case l4-constants-ucdn-script-siddham). */
  UCDN_SCRIPT_SIDDHAM: any;
  /** constants.UCDN_SCRIPT_SIGNWRITING compatibility entry (case l4-constants-ucdn-script-signwriting). */
  UCDN_SCRIPT_SIGNWRITING: any;
  /** constants.UCDN_SCRIPT_SINHALA compatibility entry (case l4-constants-ucdn-script-sinhala). */
  UCDN_SCRIPT_SINHALA: any;
  /** constants.UCDN_SCRIPT_SOGDIAN compatibility entry (case l4-constants-ucdn-script-sogdian). */
  UCDN_SCRIPT_SOGDIAN: any;
  /** constants.UCDN_SCRIPT_SORA_SOMPENG compatibility entry (case l4-constants-ucdn-script-sora-sompeng). */
  UCDN_SCRIPT_SORA_SOMPENG: any;
  /** constants.UCDN_SCRIPT_SOYOMBO compatibility entry (case l4-constants-ucdn-script-soyombo). */
  UCDN_SCRIPT_SOYOMBO: any;
  /** constants.UCDN_SCRIPT_SUNDANESE compatibility entry (case l4-constants-ucdn-script-sundanese). */
  UCDN_SCRIPT_SUNDANESE: any;
  /** constants.UCDN_SCRIPT_SUNUWAR compatibility entry (case l4-constants-ucdn-script-sunuwar). */
  UCDN_SCRIPT_SUNUWAR: any;
  /** constants.UCDN_SCRIPT_SYLOTI_NAGRI compatibility entry (case l4-constants-ucdn-script-syloti-nagri). */
  UCDN_SCRIPT_SYLOTI_NAGRI: any;
  /** constants.UCDN_SCRIPT_SYRIAC compatibility entry (case l4-constants-ucdn-script-syriac). */
  UCDN_SCRIPT_SYRIAC: any;
  /** constants.UCDN_SCRIPT_TAGALOG compatibility entry (case l4-constants-ucdn-script-tagalog). */
  UCDN_SCRIPT_TAGALOG: any;
  /** constants.UCDN_SCRIPT_TAGBANWA compatibility entry (case l4-constants-ucdn-script-tagbanwa). */
  UCDN_SCRIPT_TAGBANWA: any;
  /** constants.UCDN_SCRIPT_TAI_LE compatibility entry (case l4-constants-ucdn-script-tai-le). */
  UCDN_SCRIPT_TAI_LE: any;
  /** constants.UCDN_SCRIPT_TAI_THAM compatibility entry (case l4-constants-ucdn-script-tai-tham). */
  UCDN_SCRIPT_TAI_THAM: any;
  /** constants.UCDN_SCRIPT_TAI_VIET compatibility entry (case l4-constants-ucdn-script-tai-viet). */
  UCDN_SCRIPT_TAI_VIET: any;
  /** constants.UCDN_SCRIPT_TAKRI compatibility entry (case l4-constants-ucdn-script-takri). */
  UCDN_SCRIPT_TAKRI: any;
  /** constants.UCDN_SCRIPT_TAMIL compatibility entry (case l4-constants-ucdn-script-tamil). */
  UCDN_SCRIPT_TAMIL: any;
  /** constants.UCDN_SCRIPT_TANGSA compatibility entry (case l4-constants-ucdn-script-tangsa). */
  UCDN_SCRIPT_TANGSA: any;
  /** constants.UCDN_SCRIPT_TANGUT compatibility entry (case l4-constants-ucdn-script-tangut). */
  UCDN_SCRIPT_TANGUT: any;
  /** constants.UCDN_SCRIPT_TELUGU compatibility entry (case l4-constants-ucdn-script-telugu). */
  UCDN_SCRIPT_TELUGU: any;
  /** constants.UCDN_SCRIPT_THAANA compatibility entry (case l4-constants-ucdn-script-thaana). */
  UCDN_SCRIPT_THAANA: any;
  /** constants.UCDN_SCRIPT_THAI compatibility entry (case l4-constants-ucdn-script-thai). */
  UCDN_SCRIPT_THAI: any;
  /** constants.UCDN_SCRIPT_TIBETAN compatibility entry (case l4-constants-ucdn-script-tibetan). */
  UCDN_SCRIPT_TIBETAN: any;
  /** constants.UCDN_SCRIPT_TIFINAGH compatibility entry (case l4-constants-ucdn-script-tifinagh). */
  UCDN_SCRIPT_TIFINAGH: any;
  /** constants.UCDN_SCRIPT_TIRHUTA compatibility entry (case l4-constants-ucdn-script-tirhuta). */
  UCDN_SCRIPT_TIRHUTA: any;
  /** constants.UCDN_SCRIPT_TODHRI compatibility entry (case l4-constants-ucdn-script-todhri). */
  UCDN_SCRIPT_TODHRI: any;
  /** constants.UCDN_SCRIPT_TOTO compatibility entry (case l4-constants-ucdn-script-toto). */
  UCDN_SCRIPT_TOTO: any;
  /** constants.UCDN_SCRIPT_TULU_TIGALARI compatibility entry (case l4-constants-ucdn-script-tulu-tigalari). */
  UCDN_SCRIPT_TULU_TIGALARI: any;
  /** constants.UCDN_SCRIPT_UGARITIC compatibility entry (case l4-constants-ucdn-script-ugaritic). */
  UCDN_SCRIPT_UGARITIC: any;
  /** constants.UCDN_SCRIPT_UNKNOWN compatibility entry (case l4-constants-ucdn-script-unknown). */
  UCDN_SCRIPT_UNKNOWN: any;
  /** constants.UCDN_SCRIPT_VAI compatibility entry (case l4-constants-ucdn-script-vai). */
  UCDN_SCRIPT_VAI: any;
  /** constants.UCDN_SCRIPT_VITHKUQI compatibility entry (case l4-constants-ucdn-script-vithkuqi). */
  UCDN_SCRIPT_VITHKUQI: any;
  /** constants.UCDN_SCRIPT_WANCHO compatibility entry (case l4-constants-ucdn-script-wancho). */
  UCDN_SCRIPT_WANCHO: any;
  /** constants.UCDN_SCRIPT_WARANG_CITI compatibility entry (case l4-constants-ucdn-script-warang-citi). */
  UCDN_SCRIPT_WARANG_CITI: any;
  /** constants.UCDN_SCRIPT_YEZIDI compatibility entry (case l4-constants-ucdn-script-yezidi). */
  UCDN_SCRIPT_YEZIDI: any;
  /** constants.UCDN_SCRIPT_YI compatibility entry (case l4-constants-ucdn-script-yi). */
  UCDN_SCRIPT_YI: any;
  /** constants.UCDN_SCRIPT_ZANABAZAR_SQUARE compatibility entry (case l4-constants-ucdn-script-zanabazar-square). */
  UCDN_SCRIPT_ZANABAZAR_SQUARE: any;
  /** constants.PDF_CID_FONT_RESOURCE compatibility entry (case l4-constants-pdf-cid-font-resource). */
  PDF_CID_FONT_RESOURCE: any;
  /** constants.PDF_CJK_FONT_RESOURCE compatibility entry (case l4-constants-pdf-cjk-font-resource). */
  PDF_CJK_FONT_RESOURCE: any;
  /** constants.PDF_FD_ALL_CAP compatibility entry (case l4-constants-pdf-fd-all-cap). */
  PDF_FD_ALL_CAP: any;
  /** constants.PDF_FD_FIXED_PITCH compatibility entry (case l4-constants-pdf-fd-fixed-pitch). */
  PDF_FD_FIXED_PITCH: any;
  /** constants.PDF_FD_FORCE_BOLD compatibility entry (case l4-constants-pdf-fd-force-bold). */
  PDF_FD_FORCE_BOLD: any;
  /** constants.PDF_FD_ITALIC compatibility entry (case l4-constants-pdf-fd-italic). */
  PDF_FD_ITALIC: any;
  /** constants.PDF_FD_NONSYMBOLIC compatibility entry (case l4-constants-pdf-fd-nonsymbolic). */
  PDF_FD_NONSYMBOLIC: any;
  /** constants.PDF_FD_SCRIPT compatibility entry (case l4-constants-pdf-fd-script). */
  PDF_FD_SCRIPT: any;
  /** constants.PDF_FD_SERIF compatibility entry (case l4-constants-pdf-fd-serif). */
  PDF_FD_SERIF: any;
  /** constants.PDF_FD_SMALL_CAP compatibility entry (case l4-constants-pdf-fd-small-cap). */
  PDF_FD_SMALL_CAP: any;
  /** constants.PDF_FD_SYMBOLIC compatibility entry (case l4-constants-pdf-fd-symbolic). */
  PDF_FD_SYMBOLIC: any;
  /** constants.PDF_SIMPLE_ENCODING_CYRILLIC compatibility entry (case l4-constants-pdf-simple-encoding-cyrillic). */
  PDF_SIMPLE_ENCODING_CYRILLIC: any;
  /** constants.PDF_SIMPLE_ENCODING_GREEK compatibility entry (case l4-constants-pdf-simple-encoding-greek). */
  PDF_SIMPLE_ENCODING_GREEK: any;
  /** constants.PDF_SIMPLE_ENCODING_LATIN compatibility entry (case l4-constants-pdf-simple-encoding-latin). */
  PDF_SIMPLE_ENCODING_LATIN: any;
  /** constants.PDF_SIMPLE_FONT_RESOURCE compatibility entry (case l4-constants-pdf-simple-font-resource). */
  PDF_SIMPLE_FONT_RESOURCE: any;
  /** constants.Base14_fontdict compatibility entry (case l4-constants-base14-fontdict). */
  Base14_fontdict: any;
  /** constants.Base14_fontnames compatibility entry (case l4-constants-base14-fontnames). */
  Base14_fontnames: any;
  /** constants.symbol_glyphs compatibility entry (case l4-constants-symbol-glyphs). */
  symbol_glyphs: any;
  /** constants.zapf_glyphs compatibility entry (case l4-constants-zapf-glyphs). */
  zapf_glyphs: any;
  /** constants.pdfcolor compatibility entry (case l4-constants-pdfcolor). */
  pdfcolor: any;
  /** constants.INVALID_NAME_CHARS compatibility entry (case l2-constants-invalid-name-chars). */
  INVALID_NAME_CHARS: any;
  /** constants.PDF_ENUM_FALSE compatibility entry (case l2-constants-pdf-enum-false). */
  PDF_ENUM_FALSE: any;
  /** constants.PDF_ENUM_LIMIT compatibility entry (case l2-constants-pdf-enum-limit). */
  PDF_ENUM_LIMIT: any;
  /** constants.PDF_ENUM_NULL compatibility entry (case l2-constants-pdf-enum-null). */
  PDF_ENUM_NULL: any;
  /** constants.PDF_ENUM_TRUE compatibility entry (case l2-constants-pdf-enum-true). */
  PDF_ENUM_TRUE: any;
  /** constants.PDF_LEXBUF_LARGE compatibility entry (case l2-constants-pdf-lexbuf-large). */
  PDF_LEXBUF_LARGE: any;
  /** constants.PDF_LEXBUF_SMALL compatibility entry (case l2-constants-pdf-lexbuf-small). */
  PDF_LEXBUF_SMALL: any;
  /** constants.PDF_MAX_GEN_NUMBER compatibility entry (case l2-constants-pdf-max-gen-number). */
  PDF_MAX_GEN_NUMBER: any;
  /** constants.PDF_MAX_OBJECT_NUMBER compatibility entry (case l2-constants-pdf-max-object-number). */
  PDF_MAX_OBJECT_NUMBER: any;
  /** constants.PDF_MRANGE_CAP compatibility entry (case l2-constants-pdf-mrange-cap). */
  PDF_MRANGE_CAP: any;
  /** constants.PDF_NUM_TOKENS compatibility entry (case l2-constants-pdf-num-tokens). */
  PDF_NUM_TOKENS: any;
  /** constants.PDF_PROCESSOR_REQUIRES_DECODED_IMAGES compatibility entry (case l2-constants-pdf-processor-requires-decoded-images). */
  PDF_PROCESSOR_REQUIRES_DECODED_IMAGES: any;
  /** constants.PDF_TOK_CLOSE_ARRAY compatibility entry (case l2-constants-pdf-tok-close-array). */
  PDF_TOK_CLOSE_ARRAY: any;
  /** constants.PDF_TOK_CLOSE_BRACE compatibility entry (case l2-constants-pdf-tok-close-brace). */
  PDF_TOK_CLOSE_BRACE: any;
  /** constants.PDF_TOK_CLOSE_DICT compatibility entry (case l2-constants-pdf-tok-close-dict). */
  PDF_TOK_CLOSE_DICT: any;
  /** constants.PDF_TOK_ENDOBJ compatibility entry (case l2-constants-pdf-tok-endobj). */
  PDF_TOK_ENDOBJ: any;
  /** constants.PDF_TOK_ENDSTREAM compatibility entry (case l2-constants-pdf-tok-endstream). */
  PDF_TOK_ENDSTREAM: any;
  /** constants.PDF_TOK_EOF compatibility entry (case l2-constants-pdf-tok-eof). */
  PDF_TOK_EOF: any;
  /** constants.PDF_TOK_ERROR compatibility entry (case l2-constants-pdf-tok-error). */
  PDF_TOK_ERROR: any;
  /** constants.PDF_TOK_FALSE compatibility entry (case l2-constants-pdf-tok-false). */
  PDF_TOK_FALSE: any;
  /** constants.PDF_TOK_INT compatibility entry (case l2-constants-pdf-tok-int). */
  PDF_TOK_INT: any;
  /** constants.PDF_TOK_KEYWORD compatibility entry (case l2-constants-pdf-tok-keyword). */
  PDF_TOK_KEYWORD: any;
  /** constants.PDF_TOK_NAME compatibility entry (case l2-constants-pdf-tok-name). */
  PDF_TOK_NAME: any;
  /** constants.PDF_TOK_NEWOBJ compatibility entry (case l2-constants-pdf-tok-newobj). */
  PDF_TOK_NEWOBJ: any;
  /** constants.PDF_TOK_NULL compatibility entry (case l2-constants-pdf-tok-null). */
  PDF_TOK_NULL: any;
  /** constants.PDF_TOK_OBJ compatibility entry (case l2-constants-pdf-tok-obj). */
  PDF_TOK_OBJ: any;
  /** constants.PDF_TOK_OPEN_ARRAY compatibility entry (case l2-constants-pdf-tok-open-array). */
  PDF_TOK_OPEN_ARRAY: any;
  /** constants.PDF_TOK_OPEN_BRACE compatibility entry (case l2-constants-pdf-tok-open-brace). */
  PDF_TOK_OPEN_BRACE: any;
  /** constants.PDF_TOK_OPEN_DICT compatibility entry (case l2-constants-pdf-tok-open-dict). */
  PDF_TOK_OPEN_DICT: any;
  /** constants.PDF_TOK_R compatibility entry (case l2-constants-pdf-tok-r). */
  PDF_TOK_R: any;
  /** constants.PDF_TOK_REAL compatibility entry (case l2-constants-pdf-tok-real). */
  PDF_TOK_REAL: any;
  /** constants.PDF_TOK_STARTXREF compatibility entry (case l2-constants-pdf-tok-startxref). */
  PDF_TOK_STARTXREF: any;
  /** constants.PDF_TOK_STREAM compatibility entry (case l2-constants-pdf-tok-stream). */
  PDF_TOK_STREAM: any;
  /** constants.PDF_TOK_STRING compatibility entry (case l2-constants-pdf-tok-string). */
  PDF_TOK_STRING: any;
  /** constants.PDF_TOK_TRAILER compatibility entry (case l2-constants-pdf-tok-trailer). */
  PDF_TOK_TRAILER: any;
  /** constants.PDF_TOK_TRUE compatibility entry (case l2-constants-pdf-tok-true). */
  PDF_TOK_TRUE: any;
  /** constants.PDF_TOK_XREF compatibility entry (case l2-constants-pdf-tok-xref). */
  PDF_TOK_XREF: any;
  /** constants.b compatibility entry (case l2-constants-b). */
  b: any;
  /** constants.f compatibility entry (case l2-constants-f). */
  f: any;
  /** constants.g compatibility entry (case l2-constants-g). */
  g: any;
  /** constants.matrix_like compatibility entry (case l2-constants-matrix-like). */
  matrix_like: any;
  /** constants.name compatibility entry (case l2-constants-name). */
  name: any;
  /** constants.point_like compatibility entry (case l2-constants-point-like). */
  point_like: any;
  /** constants.quad_like compatibility entry (case l2-constants-quad-like). */
  quad_like: any;
  /** constants.r compatibility entry (case l2-constants-r). */
  r: any;
  /** constants.rect_like compatibility entry (case l2-constants-rect-like). */
  rect_like: any;
  /** constants.trace_device_CLIP_PATH compatibility entry (case l2-constants-trace-device-clip-path). */
  trace_device_CLIP_PATH: any;
  /** constants.trace_device_CLIP_STROKE_PATH compatibility entry (case l2-constants-trace-device-clip-stroke-path). */
  trace_device_CLIP_STROKE_PATH: any;
  /** constants.trace_device_FILL_PATH compatibility entry (case l2-constants-trace-device-fill-path). */
  trace_device_FILL_PATH: any;
  /** constants.trace_device_STROKE_PATH compatibility entry (case l2-constants-trace-device-stroke-path). */
  trace_device_STROKE_PATH: any;
}

export declare class exceptions {
  /** exceptions.EmptyFileError compatibility entry (case p0-exceptions-emptyfileerror). */
  EmptyFileError: any;
  /** exceptions.FileDataError compatibility entry (case p0-exceptions-filedataerror). */
  FileDataError: any;
  /** exceptions.FileNotFoundError compatibility entry (case p0-exceptions-filenotfounderror). */
  FileNotFoundError: any;
  /** exceptions.FitzDeprecation compatibility entry (case p0-exceptions-deprecation). */
  FitzDeprecation: any;
}

// Module-level entry points.
/** module.ASSERT_PDF compatibility entry (case l5-module-assert-pdf). */
export declare function assertPDF(cond: any): any;
/** module.CheckColor compatibility entry (case l5-module-checkcolor). */
export declare function checkColor(c: any): any;
/** module.CheckFont compatibility entry (case l1-module-checkfont). */
export declare function checkFont(page: any, fontname: any): any;
/** module.CheckFontInfo compatibility entry (case l1-module-checkfontinfo). */
export declare function checkFontInfo(doc: any, xref: any): any;
/** module.CheckMarkerArg compatibility entry (case l5-module-checkmarkerarg). */
export declare function checkMarkerArg(quads: any): any;
/** module.CheckMorph compatibility entry (case l5-module-checkmorph). */
export declare function checkMorph(o: any): any;
/** module.CheckParent compatibility entry (case l5-module-checkparent). */
export declare function checkParent(o: any): any;
/** module.CheckQuad compatibility entry (case l5-module-checkquad). */
export declare function checkQuad(q: any): any;
/** module.CheckRect compatibility entry (case l5-module-checkrect). */
export declare function checkRect(r: any): any;
/** module.ColorCode compatibility entry (case l5-module-colorcode). */
export declare function colorCode(c: any, f: any): any;
/** module.ConversionHeader compatibility entry (case l5-module-conversionheader). */
export declare function conversionHeader(i: any, filename?: any): any;
/** module.ConversionTrailer compatibility entry (case l5-module-conversiontrailer). */
export declare function conversionTrailer(i: any): any;
/** module.EMPTY_IRECT compatibility entry (case l5-module-empty-irect). */
export declare function emptyIRECT(): any;
/** module.EMPTY_QUAD compatibility entry (case l5-module-empty-quad). */
export declare function emptyQUAD(): any;
/** module.EMPTY_RECT compatibility entry (case l5-module-empty-rect). */
export declare function emptyRECT(): any;
/** module.ENSURE_OPERATION compatibility entry (case l5-module-ensure-operation). */
export declare function ensureOPERATION(pdf: any): any;
/** module.INFINITE_IRECT compatibility entry (case l5-module-infinite-irect). */
export declare function infiniteIRECT(): any;
/** module.INFINITE_QUAD compatibility entry (case l5-module-infinite-quad). */
export declare function infiniteQUAD(): any;
/** module.INFINITE_RECT compatibility entry (case l5-module-infinite-rect). */
export declare function infiniteRECT(): any;
/** module.PDF_NAME compatibility entry (case l5-module-pdf-name). */
export declare function pdfNAME(x: any): any;
/** module.Page__add_text_marker compatibility entry (case l1-module-page-add-text-marker). */
export declare function pageAddTextMarker(self: any, quads: any, annot_type: any): any;
/** module.PySequence_Check compatibility entry (case l5-module-pysequence-check). */
export declare function pysequenceCheck(s: any): any;
/** module.PySequence_Size compatibility entry (case l5-module-pysequence-size). */
export declare function pysequenceSize(s: any): any;
/** module.PyUnicode_DecodeRawUnicodeEscape compatibility entry (case l5-module-pyunicode-decoderawunicodeescape). */
export declare function pyunicodeDecodeRawUnicodeEscape(s: any, errors?: any): any;
/** module.RAISEPY compatibility entry (case l5-module-raisepy). */
export declare function rAISEPY(msg: any, exc: any): any;
/** module.UpdateFontInfo compatibility entry (case l1-module-updatefontinfo). */
export declare function updateFontInfo(doc: any, info: any): any;
/** module.annot_postprocess compatibility entry (case l5-module-annot-postprocess). */
export declare function annotPostprocess(page: any, annot: any): any;
/** module.annot_preprocess compatibility entry (case l5-module-annot-preprocess). */
export declare function annotPreprocess(page: any): any;
/** module.apply_pages compatibility entry (case l5-module-apply-pages). */
export declare function applyPages(path: any, pagefn: any, pagefn_args?: any, pagefn_kwargs?: any, initfn?: any, initfn_args?: any, initfn_kwargs?: any, pages?: any, method?: any, concurrency?: any, _stats?: any): any;
/** module.args_match compatibility entry (case l5-module-args-match). */
export declare function argsMatch(args: any, ...args: any[]): any;
/** module.calc_image_matrix compatibility entry (case l3-module-calc-image-matrix). */
export declare function calcImageMatrix(width: any, height: any, tr: any, rotate: any, keep: any): any;
/** module.canon compatibility entry (case l1-module-canon). */
export declare function canon(c: any): any;
/** module.chartocanon compatibility entry (case l1-module-chartocanon). */
export declare function chartocanon(s: any): any;
/** module.colors_pdf_dict compatibility entry (case l5-module-colors-pdf-dict). */
export declare function colorsPdfDict(): any;
/** module.colors_wx_list compatibility entry (case l5-module-colors-wx-list). */
export declare function colorsWxList(): any;
/** module.compute_scissor compatibility entry (case l5-module-compute-scissor). */
export declare function computeScissor(dev: any): any;
/** module.css_for_pymupdf_font compatibility entry (case l1-module-css-for-pymupdf-font). */
export declare function cssForFont(fontcode: any, CSS?: any, archive?: any, name?: any): any;
/** module.dest_is_valid compatibility entry (case l5-module-dest-is-valid). */
export declare function destIsValid(o: any, page_count: any, page_object_nums: any, names_list: any): any;
/** module.dest_is_valid_page compatibility entry (case l5-module-dest-is-valid-page). */
export declare function destIsValidPage(obj: any, page_object_nums: any, pagecount: any): any;
/** module.detect_super_script compatibility entry (case l5-module-detect-super-script). */
export declare function detectSuperScript(line: any, ch: any): any;
/** module.dir_str compatibility entry (case l5-module-dir-str). */
export declare function dirStr(x: any): any;
/** module.exception_info compatibility entry (case l5-module-exception-info). */
export declare function exceptionInfo(): any;
/** module.extra_FzDocument_insert_pdf compatibility entry (case l5-module-extra-fzdocument-insert-pdf). */
export declare function extraFzDocumentInsertPdf(doc: any, src: any, from_page: any, to_page: any, start_at: any, rotate: any, links: any, annots: any, show_progress: any, final: any, graft_map: any): any;
/** module.find_string compatibility entry (case l5-module-find-string). */
export declare function findString(s: any, needle: any): any;
/** module.format_g compatibility entry (case l5-module-format-g). */
export declare function formatG(value: any, fmt?: any): any;
/** module.getTJstr compatibility entry (case l5-module-gettjstr). */
export declare function getTJstr(text: any, glyphs: any, simple: any, ordering: any): any;
/** module.get_env_bool compatibility entry (case l5-module-get-env-bool). */
export declare function getEnvBool(name: any, default_: any): any;
/** module.get_env_int compatibility entry (case l5-module-get-env-int). */
export declare function getEnvInt(name: any, default_: any): any;
/** module.get_highlight_selection compatibility entry (case l5-module-get-highlight-selection). */
export declare function getHighlightSelection(page: any, start?: any, stop?: any, clip?: any): any;
/** module.get_pdf_now compatibility entry (case l5-module-get-pdf-now). */
export declare function getPdfNow(): any;
/** module.get_pdf_str compatibility entry (case l5-module-get-pdf-str). */
export declare function getPdfStr(s: any): any;
/** module.get_tessdata compatibility entry (case l1-module-get-tessdata). */
export declare function getTessdata(tessdata?: any): any;
/** module.get_text compatibility entry (case l1-module-get-text). */
export declare function getText(path: any, pages?: any, method?: any, concurrency?: any, option?: any, clip?: any, flags?: any, textpage?: any, sort?: any, delimiters?: any, _stats?: any): any;
/** module.get_text_length compatibility entry (case l1-module-get-text-length). */
export declare function getTextLength(text: any, fontname?: any, fontsize?: any, encoding?: any): any;
/** module.glyph_name_to_unicode compatibility entry (case l1-module-glyph-name-to-unicode). */
export declare function glyphNameToUnicode(name: any): any;
/** module.hdist compatibility entry (case l5-module-hdist). */
export declare function hdist(dir: any, a: any, b: any): any;
/** module.image_profile compatibility entry (case l3-module-image-profile). */
export declare function imageProfile(img: any): any;
/** module.log compatibility entry (case l5-module-log). */
export declare function log(text?: any, caller?: any): any;
/** module.make_escape compatibility entry (case l5-module-make-escape). */
export declare function makeEscape(ch: any): any;
/** module.make_story_elpos compatibility entry (case l5-module-make-story-elpos). */
export declare function makeStoryElpos(): any;
/** module.make_table compatibility entry (case l1-module-make-table). */
export declare function makeTable(rect?: any, cols?: any, rows?: any): any;
/** module.match_string compatibility entry (case l5-module-match-string). */
export declare function matchString(h0: any, n0: any): any;
/** module.message compatibility entry (case l5-module-message). */
export declare function message(text?: any): any;
/** module.message_warning compatibility entry (case l5-module-message-warning). */
export declare function messageWarning(text: any): any;
/** module.no_recommend_layout compatibility entry (case l5-module-no-recommend-layout). */
export declare function noRecommendLayout(): any;
/** module.on_highlight_char compatibility entry (case l1-module-on-highlight-char). */
export declare function onHighlightChar(hits: any, line: any, ch: any): any;
/** module.page_merge compatibility entry (case l5-module-page-merge). */
export declare function pageMerge(doc_des: any, doc_src: any, page_from: any, page_to: any, rotate: any, links: any, copy_annots: any, graft_map: any): any;
/** module.paper_rect compatibility entry (case l5-module-paper-rect). */
export declare function paperRect(s: any): any;
/** module.paper_size compatibility entry (case l5-module-paper-size). */
export declare function paperSize(s: any): any;
/** module.paper_sizes compatibility entry (case l5-module-paper-sizes). */
export declare function paperSizes(): any;
/** module.pdf_lookup_page_loc compatibility entry (case l5-module-pdf-lookup-page-loc). */
export declare function pdfLookupPageLoc(doc: any, needle: any): any;
/** module.pdfobj_string compatibility entry (case l5-module-pdfobj-string). */
export declare function pdfobjString(o: any, prefix?: any): any;
/** module.planish_line compatibility entry (case l5-module-planish-line). */
export declare function planishLine(p1: any, p2: any): any;
/** module.recover_bbox_quad compatibility entry (case l1-module-recover-bbox-quad). */
export declare function recoverBboxQuad(line_dir: any, span: any, bbox: any): any;
/** module.recover_char_quad compatibility entry (case l1-module-recover-char-quad). */
export declare function recoverCharQuad(line_dir: any, span: any, char: any): any;
/** module.recover_line_quad compatibility entry (case l1-module-recover-line-quad). */
export declare function recoverLineQuad(line: any, spans?: any): any;
/** module.recover_quad compatibility entry (case l1-module-recover-quad). */
export declare function recoverQuad(line_dir: any, span: any): any;
/** module.recover_span_quad compatibility entry (case l1-module-recover-span-quad). */
export declare function recoverSpanQuad(line_dir: any, span: any, chars?: any): any;
/** module.repair_mono_font compatibility entry (case l1-module-repair-mono-font). */
export declare function repairMonoFont(page: any, font: any): any;
/** module.restore_aliases compatibility entry (case l5-module-restore-aliases). */
export declare function restoreAliases(): any;
/** module.sRGB_to_pdf compatibility entry (case l5-module-srgb-to-pdf). */
export declare function srgbToPdf(srgb: any): any;
/** module.sRGB_to_rgb compatibility entry (case l5-module-srgb-to-rgb). */
export declare function srgbToRgb(srgb: any): any;
/** module.set_log compatibility entry (case l5-module-set-log). */
export declare function setLog(text?: any, fd?: any, stream?: any, path?: any, path_append?: any, pylogging?: any, pylogging_logger?: any, pylogging_level?: any, pylogging_name?: any): any;
/** module.set_messages compatibility entry (case l5-module-set-messages). */
export declare function setMessages(text?: any, fd?: any, stream?: any, path?: any, path_append?: any, pylogging?: any, pylogging_logger?: any, pylogging_level?: any, pylogging_name?: any): any;
/** module.string_in_names_list compatibility entry (case l5-module-string-in-names-list). */
export declare function stringInNamesList(p: any, names_list: any): any;
/** module.strip_outline compatibility entry (case l5-module-strip-outline). */
export declare function stripOutline(doc: any, outlines: any, page_count: any, page_object_nums: any, names_list: any): any;
/** module.strip_outlines compatibility entry (case l5-module-strip-outlines). */
export declare function stripOutlines(doc: any, outlines: any, page_count: any, page_object_nums: any, names_list: any): any;
/** module.unicode_to_glyph_name compatibility entry (case l1-module-unicode-to-glyph-name). */
export declare function unicodeToGlyphName(ch: any): any;
/** module.util_concat_matrix compatibility entry (case l5-module-util-concat-matrix). */
export declare function utilConcatMatrix(m1: any, m2: any): any;
/** module.util_ensure_widget_calc compatibility entry (case l5-module-util-ensure-widget-calc). */
export declare function utilEnsureWidgetCalc(annot: any): any;
/** module.util_hor_matrix compatibility entry (case l5-module-util-hor-matrix). */
export declare function utilHorMatrix(C: any, P: any): any;
/** module.util_include_point_in_rect compatibility entry (case l5-module-util-include-point-in-rect). */
export declare function utilIncludePointInRect(r: any, p: any): any;
/** module.util_intersect_rect compatibility entry (case l5-module-util-intersect-rect). */
export declare function utilIntersectRect(r1: any, r2: any): any;
/** module.util_invert_matrix compatibility entry (case l5-module-util-invert-matrix). */
export declare function utilInvertMatrix(matrix: any): any;
/** module.util_is_point_in_rect compatibility entry (case l5-module-util-is-point-in-rect). */
export declare function utilIsPointInRect(p: any, r: any): any;
/** module.util_make_irect compatibility entry (case l5-module-util-make-irect). */
export declare function utilMakeIrect(...args: any[]): any;
/** module.util_make_rect compatibility entry (case l5-module-util-make-rect). */
export declare function utilMakeRect(...args: any[]): any;
/** module.util_measure_string compatibility entry (case l5-module-util-measure-string). */
export declare function utilMeasureString(text: any, fontname: any, fontsize: any, encoding: any): any;
/** module.util_point_in_quad compatibility entry (case l5-module-util-point-in-quad). */
export declare function utilPointInQuad(P: any, Q: any): any;
/** module.util_round_rect compatibility entry (case l5-module-util-round-rect). */
export declare function utilRoundRect(rect: any): any;
/** module.util_sine_between compatibility entry (case l5-module-util-sine-between). */
export declare function utilSineBetween(C: any, P: any, Q: any): any;
/** module.util_transform_point compatibility entry (case l5-module-util-transform-point). */
export declare function utilTransformPoint(point: any, matrix: any): any;
/** module.util_transform_rect compatibility entry (case l5-module-util-transform-rect). */
export declare function utilTransformRect(rect: any, matrix: any): any;
/** module.util_union_rect compatibility entry (case l5-module-util-union-rect). */
export declare function utilUnionRect(r1: any, r2: any): any;
/** module.vdist compatibility entry (case l5-module-vdist). */
export declare function vdist(dir: any, a: any, b: any): any;
