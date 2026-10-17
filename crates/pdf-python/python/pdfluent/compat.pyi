# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Planned PyMuPDF-compatible surface (PyMuPDF==1.28.2).

Generated from specs/api/*.yaml by scripts/compat/gen_pyi.py. Do not edit by
hand. Parameter names and shapes follow the spec; every type is Any until the
binding implements the member behind it.
"""

from typing import Any as _Any



class Document:
    """Document compatibility surface."""
    @property
    def FormFonts(self) -> _Any:
        """Document.FormFonts compatibility entry (case l1-document-formfonts)."""
        ...
    def __contains__(self, loc: _Any) -> _Any:
        """Document.__contains__ compatibility entry (case l2-document-contains)."""
        ...
    def __delitem__(self, i: _Any) -> _Any:
        """Document.__delitem__ compatibility entry (case l2-document-delitem)."""
        ...
    def __enter__(self) -> _Any:
        """Document.__enter__ compatibility entry (case l2.doc.context_manager)."""
        ...
    def __eq__(self, value: _Any) -> _Any:
        """Document.__eq__ compatibility entry (case l2-document-eq)."""
        ...
    def __exit__(self, *args: _Any) -> _Any:
        """Document.__exit__ compatibility entry (case l2-document-exit)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Document.__ge__ compatibility entry (case l2-document-ge)."""
        ...
    def __getitem__(self, i: _Any = ...) -> _Any:
        """Document.__getitem__ compatibility entry (case l2.pages.getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Document.__gt__ compatibility entry (case l2-document-gt)."""
        ...
    def __init__(self, filename: _Any = ..., stream: _Any = ..., filetype: _Any = ..., rect: _Any = ..., width: _Any = ..., height: _Any = ..., fontsize: _Any = ..., archive: _Any = ...) -> None:
        """Document.__init__ compatibility entry (case l2.open.document_ctor)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Document.__le__ compatibility entry (case l2-document-le)."""
        ...
    def __len__(self) -> _Any:
        """Document.__len__ compatibility entry (case l2.pages.len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Document.__lt__ compatibility entry (case l2-document-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Document.__ne__ compatibility entry (case l2-document-ne)."""
        ...
    def add_layer(self, name: _Any, creator: _Any = ..., on: _Any = ...) -> _Any:
        """Document.add_layer compatibility entry (case l4-document-add-layer)."""
        ...
    def add_ocg(self, name: _Any, config: _Any = ..., on: _Any = ..., intent: _Any = ..., usage: _Any = ...) -> _Any:
        """Document.add_ocg compatibility entry (case l4-document-add-ocg)."""
        ...
    def apply_css(self, css: _Any, append: _Any = ...) -> _Any:
        """Document.apply_css compatibility entry (case l4-document-apply-css)."""
        ...
    def authenticate(self, password: _Any) -> _Any:
        """Document.authenticate compatibility entry (case l2.crypto.authenticate)."""
        ...
    def bake(self, *, annots: _Any = ..., widgets: _Any = ...) -> _Any:
        """Document.bake compatibility entry (case l4-document-bake)."""
        ...
    def can_save_incrementally(self) -> _Any:
        """Document.can_save_incrementally compatibility entry (case l2.doc.can_save_incrementally)."""
        ...
    @property
    def chapter_count(self) -> _Any:
        """Document.chapter_count compatibility entry (case l2-document-chapter-count)."""
        ...
    def chapter_page_count(self, chapter: _Any) -> _Any:
        """Document.chapter_page_count compatibility entry (case l2-document-chapter-page-count)."""
        ...
    def close(self) -> _Any:
        """Document.close compatibility entry (case l2.doc.close_twice)."""
        ...
    def convert_to_pdf(self, from_page: _Any = ..., to_page: _Any = ..., rotate: _Any = ...) -> _Any:
        """Document.convert_to_pdf compatibility entry (case l2-document-convert-to-pdf)."""
        ...
    def copy_page(self, pno: _Any, to: _Any = ...) -> _Any:
        """Document.copy_page compatibility entry (case l2.edit.copy_page)."""
        ...
    def del_toc_item(self, idx: _Any) -> _Any:
        """Document.del_toc_item compatibility entry (case l2.toc.del_item)."""
        ...
    def del_xml_metadata(self) -> _Any:
        """Document.del_xml_metadata compatibility entry (case l4-document-del-xml-metadata)."""
        ...
    def delete_page(self, pno: _Any = ...) -> _Any:
        """Document.delete_page compatibility entry (case l2.edit.delete_page_default)."""
        ...
    def delete_pages(self, *args: _Any, **kw: _Any) -> _Any:
        """Document.delete_pages compatibility entry (case l2.edit.delete_pages_bad)."""
        ...
    def embfile_add(self, name: _Any, buffer_: _Any, filename: _Any = ..., ufilename: _Any = ..., desc: _Any = ...) -> _Any:
        """Document.embfile_add compatibility entry (case l4-document-embfile-add)."""
        ...
    def embfile_count(self) -> _Any:
        """Document.embfile_count compatibility entry (case l2-document-embfile-count)."""
        ...
    def embfile_del(self, item: _Any) -> _Any:
        """Document.embfile_del compatibility entry (case l4-document-embfile-del)."""
        ...
    def embfile_get(self, item: _Any) -> _Any:
        """Document.embfile_get compatibility entry (case l2-document-embfile-get)."""
        ...
    def embfile_info(self, item: _Any) -> _Any:
        """Document.embfile_info compatibility entry (case l2-document-embfile-info)."""
        ...
    def embfile_names(self) -> _Any:
        """Document.embfile_names compatibility entry (case l2-document-embfile-names)."""
        ...
    def embfile_upd(self, item: _Any, buffer_: _Any = ..., filename: _Any = ..., ufilename: _Any = ..., desc: _Any = ...) -> _Any:
        """Document.embfile_upd compatibility entry (case l4-document-embfile-upd)."""
        ...
    def extract_font(self, xref: _Any = ..., info_only: _Any = ..., named: _Any = ...) -> _Any:
        """Document.extract_font compatibility entry (case l1-document-extract-font)."""
        ...
    def extract_image(self, xref: _Any) -> _Any:
        """Document.extract_image compatibility entry (case l3-doc-extract-image)."""
        ...
    def ez_save(self, filename: _Any, garbage: _Any = ..., clean: _Any = ..., deflate: _Any = ..., deflate_images: _Any = ..., deflate_fonts: _Any = ..., incremental: _Any = ..., ascii: _Any = ..., expand: _Any = ..., linear: _Any = ..., pretty: _Any = ..., encryption: _Any = ..., permissions: _Any = ..., owner_pw: _Any = ..., user_pw: _Any = ..., no_new_id: _Any = ..., preserve_metadata: _Any = ..., use_objstms: _Any = ..., compression_effort: _Any = ..., raise_on_repair: _Any = ..., reproducible: _Any = ...) -> _Any:
        """Document.ez_save compatibility entry (case l2-document-ez-save)."""
        ...
    def find_bookmark(self, bm: _Any) -> _Any:
        """Document.find_bookmark compatibility entry (case l2-document-find-bookmark)."""
        ...
    def fullcopy_page(self, pno: _Any, to: _Any = ...) -> _Any:
        """Document.fullcopy_page compatibility entry (case l2.edit.fullcopy_page)."""
        ...
    def get_char_widths(self, xref: _Any, limit: _Any = ..., idx: _Any = ..., fontdict: _Any = ...) -> _Any:
        """Document.get_char_widths compatibility entry (case l1-document-get-char-widths)."""
        ...
    def get_layer(self, config: _Any = ...) -> _Any:
        """Document.get_layer compatibility entry (case l2-document-get-layer)."""
        ...
    def get_layers(self) -> _Any:
        """Document.get_layers compatibility entry (case l2-document-get-layers)."""
        ...
    def get_new_xref(self) -> _Any:
        """Document.get_new_xref compatibility entry (case l2-document-get-new-xref)."""
        ...
    def get_oc(self, xref: _Any) -> _Any:
        """Document.get_oc compatibility entry (case l2-document-get-oc)."""
        ...
    def get_ocgs(self) -> _Any:
        """Document.get_ocgs compatibility entry (case l2-document-get-ocgs)."""
        ...
    def get_ocmd(self, xref: _Any) -> _Any:
        """Document.get_ocmd compatibility entry (case l2-document-get-ocmd)."""
        ...
    def get_outline_xrefs(self) -> _Any:
        """Document.get_outline_xrefs compatibility entry (case l2-document-get-outline-xrefs)."""
        ...
    def get_page_fonts(self, pno: _Any, full: _Any = ...) -> _Any:
        """Document.get_page_fonts compatibility entry (case l1-document-get-page-fonts)."""
        ...
    def get_page_images(self, pno: _Any, full: _Any = ...) -> _Any:
        """Document.get_page_images compatibility entry (case l3-doc-get-page-images)."""
        ...
    def get_page_labels(self) -> _Any:
        """Document.get_page_labels compatibility entry (case l2-document-get-page-labels)."""
        ...
    def get_page_numbers(self, label: _Any, only_one: _Any = ...) -> _Any:
        """Document.get_page_numbers compatibility entry (case l2-document-get-page-numbers)."""
        ...
    def get_page_pixmap(self, pno: _Any, *, matrix: _Any = ..., dpi: _Any = ..., colorspace: _Any = ..., clip: _Any = ..., alpha: _Any = ..., annots: _Any = ...) -> _Any:
        """Document.get_page_pixmap compatibility entry (case l3-doc-get-page-pixmap)."""
        ...
    def get_page_text(self, pno: _Any, option: _Any = ..., clip: _Any = ..., flags: _Any = ..., textpage: _Any = ..., sort: _Any = ...) -> _Any:
        """Document.get_page_text compatibility entry (case L1.get_page_text.default)."""
        ...
    def get_page_xobjects(self, pno: _Any) -> _Any:
        """Document.get_page_xobjects compatibility entry (case l3-page-get-xobjects)."""
        ...
    def get_sigflags(self) -> _Any:
        """Document.get_sigflags compatibility entry (case l2-document-get-sigflags)."""
        ...
    def get_toc(self, simple: _Any = ...) -> _Any:
        """Document.get_toc compatibility entry (case l2-toc)."""
        ...
    def get_xml_metadata(self) -> _Any:
        """Document.get_xml_metadata compatibility entry (case l2-document-get-xml-metadata)."""
        ...
    def has_annots(self) -> _Any:
        """Document.has_annots compatibility entry (case l4-document-has-annots)."""
        ...
    def has_links(self) -> _Any:
        """Document.has_links compatibility entry (case l2.doc.has_links)."""
        ...
    def init_doc(self) -> _Any:
        """Document.init_doc compatibility entry (case l2-document-init-doc)."""
        ...
    def insert_file(self, infile: _Any, from_page: _Any = ..., to_page: _Any = ..., start_at: _Any = ..., rotate: _Any = ..., links: _Any = ..., annots: _Any = ..., show_progress: _Any = ..., final: _Any = ...) -> _Any:
        """Document.insert_file compatibility entry (case l4-document-insert-file)."""
        ...
    def insert_page(self, pno: _Any, text: _Any = ..., fontsize: _Any = ..., width: _Any = ..., height: _Any = ..., fontname: _Any = ..., fontfile: _Any = ..., color: _Any = ...) -> _Any:
        """Document.insert_page compatibility entry (case l4-document-insert-page)."""
        ...
    def insert_pdf(self, docsrc: _Any, *, from_page: _Any = ..., to_page: _Any = ..., start_at: _Any = ..., rotate: _Any = ..., links: _Any = ..., annots: _Any = ..., widgets: _Any = ..., join_duplicates: _Any = ..., show_progress: _Any = ..., final: _Any = ..., _gmap: _Any = ...) -> _Any:
        """Document.insert_pdf compatibility entry (case l2.edit.insert_pdf_all)."""
        ...
    @property
    def is_dirty(self) -> _Any:
        """Document.is_dirty compatibility entry (case l2.doc.is_dirty)."""
        ...
    @property
    def is_fast_webaccess(self) -> _Any:
        """Document.is_fast_webaccess compatibility entry (case l2-document-is-fast-webaccess)."""
        ...
    @property
    def is_form_pdf(self) -> _Any:
        """Document.is_form_pdf compatibility entry (case l2-document-is-form-pdf)."""
        ...
    @property
    def is_pdf(self) -> _Any:
        """Document.is_pdf compatibility entry (case l2.doc.is_pdf)."""
        ...
    @property
    def is_reflowable(self) -> _Any:
        """Document.is_reflowable compatibility entry (case l2-document-is-reflowable)."""
        ...
    @property
    def is_repaired(self) -> _Any:
        """Document.is_repaired compatibility entry (case l2-document-is-repaired)."""
        ...
    def is_stream(self, xref: _Any = ...) -> _Any:
        """Document.is_stream compatibility entry (case l2-document-is-stream)."""
        ...
    def journal_can_do(self) -> _Any:
        """Document.journal_can_do compatibility entry (case l4-document-journal-can-do)."""
        ...
    def journal_enable(self) -> _Any:
        """Document.journal_enable compatibility entry (case l4-document-journal-enable)."""
        ...
    def journal_is_enabled(self) -> _Any:
        """Document.journal_is_enabled compatibility entry (case l4-document-journal-is-enabled)."""
        ...
    def journal_load(self, filename: _Any) -> _Any:
        """Document.journal_load compatibility entry (case l4-document-journal-load)."""
        ...
    def journal_op_name(self, step: _Any) -> _Any:
        """Document.journal_op_name compatibility entry (case l4-document-journal-op-name)."""
        ...
    def journal_position(self) -> _Any:
        """Document.journal_position compatibility entry (case l4-document-journal-position)."""
        ...
    def journal_redo(self) -> _Any:
        """Document.journal_redo compatibility entry (case l4-document-journal-redo)."""
        ...
    def journal_save(self, filename: _Any) -> _Any:
        """Document.journal_save compatibility entry (case l4-document-journal-save)."""
        ...
    def journal_start_op(self, name: _Any = ...) -> _Any:
        """Document.journal_start_op compatibility entry (case l4-document-journal-start-op)."""
        ...
    def journal_stop_op(self) -> _Any:
        """Document.journal_stop_op compatibility entry (case l4-document-journal-stop-op)."""
        ...
    def journal_undo(self) -> _Any:
        """Document.journal_undo compatibility entry (case l4-document-journal-undo)."""
        ...
    @property
    def language(self) -> _Any:
        """Document.language compatibility entry (case l2-document-language)."""
        ...
    @property
    def last_location(self) -> _Any:
        """Document.last_location compatibility entry (case l2-document-last-location)."""
        ...
    def layer_ui_configs(self) -> _Any:
        """Document.layer_ui_configs compatibility entry (case l2-document-layer-ui-configs)."""
        ...
    def layout(self, rect: _Any = ..., width: _Any = ..., height: _Any = ..., fontsize: _Any = ...) -> _Any:
        """Document.layout compatibility entry (case l2-document-layout)."""
        ...
    def load_page(self, page_id: _Any) -> _Any:
        """Document.load_page compatibility entry (case l2.pages.load_page)."""
        ...
    def location_from_page_number(self, pno: _Any) -> _Any:
        """Document.location_from_page_number compatibility entry (case l2-document-location-from-page-number)."""
        ...
    def make_bookmark(self, loc: _Any) -> _Any:
        """Document.make_bookmark compatibility entry (case l2-document-make-bookmark)."""
        ...
    @property
    def markinfo(self) -> _Any:
        """Document.markinfo compatibility entry (case l2-document-markinfo)."""
        ...
    def move_page(self, pno: _Any, to: _Any = ...) -> _Any:
        """Document.move_page compatibility entry (case l2.edit.move_page)."""
        ...
    @property
    def name(self) -> _Any:
        """Document.name compatibility entry (case l2.doc.name)."""
        ...
    def need_appearances(self, value: _Any = ...) -> _Any:
        """Document.need_appearances compatibility entry (case l2-document-need-appearances)."""
        ...
    @property
    def needs_pass(self) -> _Any:
        """Document.needs_pass compatibility entry (case l2.crypto.needs_pass)."""
        ...
    def new_page(self, pno: _Any = ..., width: _Any = ..., height: _Any = ...) -> _Any:
        """Document.new_page compatibility entry (case l2.edit.new_page_default)."""
        ...
    def next_location(self, page_id: _Any) -> _Any:
        """Document.next_location compatibility entry (case l2-document-next-location)."""
        ...
    @property
    def outline(self) -> _Any:
        """Document.outline compatibility entry (case l2-document-outline)."""
        ...
    def page_annot_xrefs(self, n: _Any) -> _Any:
        """Document.page_annot_xrefs compatibility entry (case l4-document-page-annot-xrefs)."""
        ...
    @property
    def page_count(self) -> _Any:
        """Document.page_count compatibility entry (case l2-page-count)."""
        ...
    @property
    def page_count2(self) -> _Any:
        """Document.page_count2 compatibility entry (case l2-document-page-count2)."""
        ...
    def page_cropbox(self, pno: _Any) -> _Any:
        """Document.page_cropbox compatibility entry (case l2.page.cropbox)."""
        ...
    def page_number_from_location(self, page_id: _Any) -> _Any:
        """Document.page_number_from_location compatibility entry (case l2-document-page-number-from-location)."""
        ...
    def page_xref(self, pno: _Any) -> _Any:
        """Document.page_xref compatibility entry (case l2-document-page-xref)."""
        ...
    @property
    def pagelayout(self) -> _Any:
        """Document.pagelayout compatibility entry (case l2-document-pagelayout)."""
        ...
    @property
    def pagemode(self) -> _Any:
        """Document.pagemode compatibility entry (case l2-document-pagemode)."""
        ...
    def pages(self, start: _Any = ..., stop: _Any = ..., step: _Any = ...) -> _Any:
        """Document.pages compatibility entry (case l2.pages.iter)."""
        ...
    def pdf_catalog(self) -> _Any:
        """Document.pdf_catalog compatibility entry (case l2-document-pdf-catalog)."""
        ...
    def pdf_trailer(self, compressed: _Any = ..., ascii: _Any = ...) -> _Any:
        """Document.pdf_trailer compatibility entry (case l2-document-pdf-trailer)."""
        ...
    @property
    def permissions(self) -> _Any:
        """Document.permissions compatibility entry (case l2.doc.permissions)."""
        ...
    def prev_location(self, page_id: _Any) -> _Any:
        """Document.prev_location compatibility entry (case l2-document-prev-location)."""
        ...
    def recolor(self, components: _Any = ...) -> _Any:
        """Document.recolor compatibility entry (case l2-document-recolor)."""
        ...
    def reload_page(self, page: _Any) -> _Any:
        """Document.reload_page compatibility entry (case l2-document-reload-page)."""
        ...
    def repair(self) -> _Any:
        """Document.repair compatibility entry (case l4-document-repair)."""
        ...
    def resolve_link(self, uri: _Any = ..., chapters: _Any = ...) -> _Any:
        """Document.resolve_link compatibility entry (case l4-document-resolve-link)."""
        ...
    def resolve_names(self) -> _Any:
        """Document.resolve_names compatibility entry (case l2-document-resolve-names)."""
        ...
    def rewrite_images(self, dpi_threshold: _Any = ..., dpi_target: _Any = ..., quality: _Any = ..., lossy: _Any = ..., lossless: _Any = ..., bitonal: _Any = ..., color: _Any = ..., gray: _Any = ..., set_to_gray: _Any = ..., options: _Any = ...) -> _Any:
        """Document.rewrite_images compatibility entry (case l3-doc-rewrite-images)."""
        ...
    def save(self, filename: _Any, garbage: _Any = ..., clean: _Any = ..., deflate: _Any = ..., deflate_images: _Any = ..., deflate_fonts: _Any = ..., incremental: _Any = ..., ascii: _Any = ..., expand: _Any = ..., linear: _Any = ..., no_new_id: _Any = ..., appearance: _Any = ..., pretty: _Any = ..., encryption: _Any = ..., permissions: _Any = ..., owner_pw: _Any = ..., user_pw: _Any = ..., preserve_metadata: _Any = ..., use_objstms: _Any = ..., compression_effort: _Any = ..., raise_on_repair: _Any = ..., reproducible: _Any = ...) -> _Any:
        """Document.save compatibility entry (case l2.crypto.save_aes256_roundtrip)."""
        ...
    def saveIncr(self) -> _Any:
        """Document.saveIncr compatibility entry (case l2.save.saveIncr)."""
        ...
    def save_snapshot(self, filename: _Any) -> _Any:
        """Document.save_snapshot compatibility entry (case l2-document-save-snapshot)."""
        ...
    def scrub(self, attached_files: _Any = ..., clean_pages: _Any = ..., embedded_files: _Any = ..., hidden_text: _Any = ..., javascript: _Any = ..., metadata: _Any = ..., redactions: _Any = ..., redact_images: _Any = ..., remove_links: _Any = ..., reset_fields: _Any = ..., reset_responses: _Any = ..., thumbnails: _Any = ..., xml_metadata: _Any = ...) -> _Any:
        """Document.scrub compatibility entry (case l4-document-scrub)."""
        ...
    def search_page_for(self, pno: _Any, text: _Any, quads: _Any = ..., clip: _Any = ..., flags: _Any = ..., textpage: _Any = ...) -> _Any:
        """Document.search_page_for compatibility entry (case L1.search_for.empty_page)."""
        ...
    def select(self, pyliste: _Any) -> _Any:
        """Document.select compatibility entry (case l2.edit.select)."""
        ...
    def set_language(self, language: _Any = ...) -> _Any:
        """Document.set_language compatibility entry (case l4-document-set-language)."""
        ...
    def set_layer(self, config: _Any, basestate: _Any = ..., on: _Any = ..., off: _Any = ..., rbgroups: _Any = ..., locked: _Any = ...) -> _Any:
        """Document.set_layer compatibility entry (case l4-document-set-layer)."""
        ...
    def set_layer_ui_config(self, number: _Any, action: _Any = ...) -> _Any:
        """Document.set_layer_ui_config compatibility entry (case l4-document-set-layer-ui-config)."""
        ...
    def set_markinfo(self, markinfo: _Any) -> _Any:
        """Document.set_markinfo compatibility entry (case l4-document-set-markinfo)."""
        ...
    def set_metadata(self, m: _Any = ...) -> _Any:
        """Document.set_metadata compatibility entry (case l2.metadata.set_badkey)."""
        ...
    def set_oc(self, xref: _Any, oc: _Any) -> _Any:
        """Document.set_oc compatibility entry (case l4-document-set-oc)."""
        ...
    def set_ocmd(self, xref: _Any = ..., ocgs: _Any = ..., policy: _Any = ..., ve: _Any = ...) -> _Any:
        """Document.set_ocmd compatibility entry (case l4-document-set-ocmd)."""
        ...
    def set_page_labels(self, labels: _Any) -> _Any:
        """Document.set_page_labels compatibility entry (case l2.pages.labels_roundtrip)."""
        ...
    def set_pagelayout(self, pagelayout: _Any) -> _Any:
        """Document.set_pagelayout compatibility entry (case l4-document-set-pagelayout)."""
        ...
    def set_pagemode(self, pagemode: _Any) -> _Any:
        """Document.set_pagemode compatibility entry (case l4-document-set-pagemode)."""
        ...
    def set_toc(self, toc: _Any, collapse: _Any = ...) -> _Any:
        """Document.set_toc compatibility entry (case l2.toc.set_bad)."""
        ...
    def set_toc_item(self, idx: _Any, dest_dict: _Any = ..., kind: _Any = ..., pno: _Any = ..., uri: _Any = ..., title: _Any = ..., to: _Any = ..., filename: _Any = ..., zoom: _Any = ...) -> _Any:
        """Document.set_toc_item compatibility entry (case l2.toc.set_item_title)."""
        ...
    def set_xml_metadata(self, metadata: _Any) -> _Any:
        """Document.set_xml_metadata compatibility entry (case l4-document-set-xml-metadata)."""
        ...
    def subset_fonts(self, verbose: _Any = ..., fallback: _Any = ...) -> _Any:
        """Document.subset_fonts compatibility entry (case l4-document-subset-fonts)."""
        ...
    def switch_layer(self, config: _Any, as_default: _Any = ...) -> _Any:
        """Document.switch_layer compatibility entry (case l2-document-switch-layer)."""
        ...
    @property
    def this(self) -> _Any:
        """Document.this compatibility entry (case l2-document-this)."""
        ...
    @property
    def this_is_pdf(self) -> _Any:
        """Document.this_is_pdf compatibility entry (case l2-document-this-is-pdf)."""
        ...
    def tobytes(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Document.tobytes compatibility entry (case l2.crypto.tobytes_encrypted)."""
        ...
    def update_object(self, xref: _Any, text: _Any, page: _Any = ...) -> _Any:
        """Document.update_object compatibility entry (case l4-document-update-object)."""
        ...
    def update_stream(self, xref: _Any = ..., stream: _Any = ..., new: _Any = ..., compress: _Any = ...) -> _Any:
        """Document.update_stream compatibility entry (case l4-document-update-stream)."""
        ...
    @property
    def version_count(self) -> _Any:
        """Document.version_count compatibility entry (case l2-document-version-count)."""
        ...
    def write(self, garbage: _Any = ..., clean: _Any = ..., deflate: _Any = ..., deflate_images: _Any = ..., deflate_fonts: _Any = ..., incremental: _Any = ..., ascii: _Any = ..., expand: _Any = ..., linear: _Any = ..., no_new_id: _Any = ..., appearance: _Any = ..., pretty: _Any = ..., encryption: _Any = ..., permissions: _Any = ..., owner_pw: _Any = ..., user_pw: _Any = ..., preserve_metadata: _Any = ..., use_objstms: _Any = ..., compression_effort: _Any = ..., raise_on_repair: _Any = ..., reproducible: _Any = ...) -> _Any:
        """Document.write compatibility entry (case l2-document-write)."""
        ...
    @property
    def xref(self) -> _Any:
        """Document.xref compatibility entry (case l2-document-xref)."""
        ...
    def xref_copy(self, source: _Any, target: _Any, *, keep: _Any = ...) -> _Any:
        """Document.xref_copy compatibility entry (case l4-document-xref-copy)."""
        ...
    def xref_get_key(self, xref: _Any, key: _Any) -> _Any:
        """Document.xref_get_key compatibility entry (case l2-document-xref-get-key)."""
        ...
    def xref_get_keys(self, xref: _Any) -> _Any:
        """Document.xref_get_keys compatibility entry (case l2-document-xref-get-keys)."""
        ...
    def xref_is_font(self, xref: _Any) -> _Any:
        """Document.xref_is_font compatibility entry (case l1-document-xref-is-font)."""
        ...
    def xref_is_image(self, xref: _Any) -> _Any:
        """Document.xref_is_image compatibility entry (case l3-doc-xref-is-image)."""
        ...
    def xref_is_stream(self, xref: _Any = ...) -> _Any:
        """Document.xref_is_stream compatibility entry (case l2-document-xref-is-stream)."""
        ...
    def xref_is_xobject(self, xref: _Any) -> _Any:
        """Document.xref_is_xobject compatibility entry (case l2-document-xref-is-xobject)."""
        ...
    def xref_length(self) -> _Any:
        """Document.xref_length compatibility entry (case l2-document-xref-length)."""
        ...
    def xref_object(self, xref: _Any, compressed: _Any = ..., ascii: _Any = ...) -> _Any:
        """Document.xref_object compatibility entry (case l2-document-xref-object)."""
        ...
    def xref_set_key(self, xref: _Any, key: _Any, value: _Any) -> _Any:
        """Document.xref_set_key compatibility entry (case l4-document-xref-set-key)."""
        ...
    def xref_stream(self, xref: _Any) -> _Any:
        """Document.xref_stream compatibility entry (case l2-document-xref-stream)."""
        ...
    def xref_stream_raw(self, xref: _Any) -> _Any:
        """Document.xref_stream_raw compatibility entry (case l2-document-xref-stream-raw)."""
        ...
    def xref_xml_metadata(self) -> _Any:
        """Document.xref_xml_metadata compatibility entry (case l2-document-xref-xml-metadata)."""
        ...


class Page:
    """Page compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Page.__eq__ compatibility entry (case l2-page-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Page.__ge__ compatibility entry (case l2-page-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Page.__gt__ compatibility entry (case l2-page-gt)."""
        ...
    def __init__(self, page: _Any, document: _Any) -> None:
        """Page.__init__ compatibility entry (case l2-page-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Page.__le__ compatibility entry (case l2-page-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Page.__lt__ compatibility entry (case l2-page-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Page.__ne__ compatibility entry (case l2-page-ne)."""
        ...
    def add_caret_annot(self, point: _Any) -> _Any:
        """Page.add_caret_annot compatibility entry (case L4.add_caret_annot.basic)."""
        ...
    def add_circle_annot(self, rect: _Any) -> _Any:
        """Page.add_circle_annot compatibility entry (case L4.add_circle_annot.basic)."""
        ...
    def add_file_annot(self, point: _Any, buffer_: _Any, filename: _Any, ufilename: _Any = ..., desc: _Any = ..., icon: _Any = ...) -> _Any:
        """Page.add_file_annot compatibility entry (case L4.add_file_annot.basic)."""
        ...
    def add_freetext_annot(self, rect: _Any, text: _Any, *, fontsize: _Any = ..., fontname: _Any = ..., text_color: _Any = ..., fill_color: _Any = ..., border_color: _Any = ..., border_width: _Any = ..., dashes: _Any = ..., callout: _Any = ..., line_end: _Any = ..., opacity: _Any = ..., align: _Any = ..., rotate: _Any = ..., richtext: _Any = ..., style: _Any = ...) -> _Any:
        """Page.add_freetext_annot compatibility entry (case L4.add_freetext_annot.basic)."""
        ...
    def add_highlight_annot(self, quads: _Any = ..., start: _Any = ..., stop: _Any = ..., clip: _Any = ...) -> _Any:
        """Page.add_highlight_annot compatibility entry (case L4.add_highlight_annot.quads)."""
        ...
    def add_ink_annot(self, handwriting: _Any) -> _Any:
        """Page.add_ink_annot compatibility entry (case L4.add_ink_annot.basic)."""
        ...
    def add_line_annot(self, p1: _Any, p2: _Any) -> _Any:
        """Page.add_line_annot compatibility entry (case L4.add_line_annot.basic)."""
        ...
    def add_polygon_annot(self, points: _Any) -> _Any:
        """Page.add_polygon_annot compatibility entry (case L4.add_polygon_annot.basic)."""
        ...
    def add_polyline_annot(self, points: _Any) -> _Any:
        """Page.add_polyline_annot compatibility entry (case L4.add_polyline_annot.basic)."""
        ...
    def add_rect_annot(self, rect: _Any) -> _Any:
        """Page.add_rect_annot compatibility entry (case L4.add_rect_annot.basic)."""
        ...
    def add_redact_annot(self, quad: _Any, text: _Any = ..., fontname: _Any = ..., fontsize: _Any = ..., align: _Any = ..., fill: _Any = ..., text_color: _Any = ..., cross_out: _Any = ...) -> _Any:
        """Page.add_redact_annot compatibility entry (case L4.add_redact_annot.basic)."""
        ...
    def add_squiggly_annot(self, quads: _Any = ..., start: _Any = ..., stop: _Any = ..., clip: _Any = ...) -> _Any:
        """Page.add_squiggly_annot compatibility entry (case L4.add_squiggly_annot.quads)."""
        ...
    def add_stamp_annot(self, rect: _Any, stamp: _Any = ...) -> _Any:
        """Page.add_stamp_annot compatibility entry (case L4.add_stamp_annot.basic)."""
        ...
    def add_strikeout_annot(self, quads: _Any = ..., start: _Any = ..., stop: _Any = ..., clip: _Any = ...) -> _Any:
        """Page.add_strikeout_annot compatibility entry (case L4.add_strikeout_annot.quads)."""
        ...
    def add_text_annot(self, point: _Any, text: _Any, icon: _Any = ...) -> _Any:
        """Page.add_text_annot compatibility entry (case L4.add_text_annot.basic)."""
        ...
    def add_underline_annot(self, quads: _Any = ..., start: _Any = ..., stop: _Any = ..., clip: _Any = ...) -> _Any:
        """Page.add_underline_annot compatibility entry (case L4.add_underline_annot.quads)."""
        ...
    def add_widget(self, widget: _Any) -> _Any:
        """Page.add_widget compatibility entry (case L4.widget.add)."""
        ...
    def annot_names(self) -> _Any:
        """Page.annot_names compatibility entry (case L4.annot_names.list)."""
        ...
    def annot_xrefs(self) -> _Any:
        """Page.annot_xrefs compatibility entry (case l4-page-annot-xrefs)."""
        ...
    def annots(self, types: _Any = ...) -> _Any:
        """Page.annots compatibility entry (case L4.annots.list)."""
        ...
    def apply_redactions(self, images: _Any = ..., graphics: _Any = ..., text: _Any = ...) -> _Any:
        """Page.apply_redactions compatibility entry (case L4.apply_redactions.text)."""
        ...
    @property
    def artbox(self) -> _Any:
        """Page.artbox compatibility entry (case l2.page.boxes)."""
        ...
    @property
    def bleedbox(self) -> _Any:
        """Page.bleedbox compatibility entry (case l2-page-bleedbox)."""
        ...
    def bound(self) -> _Any:
        """Page.bound compatibility entry (case l2-page-bound)."""
        ...
    def clean_contents(self, sanitize: _Any = ...) -> _Any:
        """Page.clean_contents compatibility entry (case L4.annot.appearance_state)."""
        ...
    def clip_to_rect(self, rect: _Any) -> _Any:
        """Page.clip_to_rect compatibility entry (case l2-page-clip-to-rect)."""
        ...
    def cluster_drawings(self, clip: _Any = ..., drawings: _Any = ..., x_tolerance: _Any = ..., y_tolerance: _Any = ..., final_filter: _Any = ...) -> _Any:
        """Page.cluster_drawings compatibility entry (case l4-page-cluster-drawings)."""
        ...
    @property
    def cropbox(self) -> _Any:
        """Page.cropbox compatibility entry (case l2-page-cropbox)."""
        ...
    @property
    def cropbox_position(self) -> _Any:
        """Page.cropbox_position compatibility entry (case l2-page-cropbox-position)."""
        ...
    def delete_annot(self, annot: _Any) -> _Any:
        """Page.delete_annot compatibility entry (case L4.delete_annot.basic)."""
        ...
    def delete_image(self, xref: _Any) -> _Any:
        """Page.delete_image compatibility entry (case l4-page-delete-image)."""
        ...
    def delete_link(self, linkdict: _Any) -> _Any:
        """Page.delete_link compatibility entry (case l2.links.delete)."""
        ...
    def delete_widget(self, widget: _Any) -> _Any:
        """Page.delete_widget compatibility entry (case l4-page-delete-widget)."""
        ...
    @property
    def derotation_matrix(self) -> _Any:
        """Page.derotation_matrix compatibility entry (case l2-page-derotation-matrix)."""
        ...
    def draw_bezier(self, p1: _Any, p2: _Any, p3: _Any, p4: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., width: _Any = ..., morph: _Any = ..., closePath: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_bezier compatibility entry (case l4-page-draw-bezier)."""
        ...
    def draw_circle(self, center: _Any, radius: _Any, color: _Any = ..., fill: _Any = ..., morph: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_circle compatibility entry (case L4.page.draw_shapes)."""
        ...
    def draw_curve(self, p1: _Any, p2: _Any, p3: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., width: _Any = ..., morph: _Any = ..., closePath: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_curve compatibility entry (case l4-page-draw-curve)."""
        ...
    def draw_line(self, p1: _Any, p2: _Any, color: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., morph: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_line compatibility entry (case L4.page.draw_shapes)."""
        ...
    def draw_oval(self, rect: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., morph: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_oval compatibility entry (case l4-page-draw-oval)."""
        ...
    def draw_polyline(self, points: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., width: _Any = ..., morph: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., closePath: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_polyline compatibility entry (case l4-page-draw-polyline)."""
        ...
    def draw_quad(self, quad: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., morph: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_quad compatibility entry (case l4-page-draw-quad)."""
        ...
    def draw_rect(self, rect: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., morph: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ..., radius: _Any = ...) -> _Any:
        """Page.draw_rect compatibility entry (case L4.page.draw_shapes)."""
        ...
    def draw_sector(self, center: _Any, point: _Any, beta: _Any, color: _Any = ..., fill: _Any = ..., dashes: _Any = ..., fullSector: _Any = ..., morph: _Any = ..., width: _Any = ..., closePath: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_sector compatibility entry (case l4-page-draw-sector)."""
        ...
    def draw_squiggle(self, p1: _Any, p2: _Any, breadth: _Any = ..., color: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., morph: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_squiggle compatibility entry (case l4-page-draw-squiggle)."""
        ...
    def draw_zigzag(self, p1: _Any, p2: _Any, breadth: _Any = ..., color: _Any = ..., dashes: _Any = ..., width: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., overlay: _Any = ..., morph: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.draw_zigzag compatibility entry (case l4-page-draw-zigzag)."""
        ...
    def extend_textpage(self, tpage: _Any, flags: _Any = ..., matrix: _Any = ...) -> _Any:
        """Page.extend_textpage compatibility entry (case L1.extend_textpage.default)."""
        ...
    def find_tables(self, **kwargs: _Any) -> _Any:
        """Page.find_tables compatibility entry (case L4.find_tables.hit)."""
        ...
    @property
    def first_annot(self) -> _Any:
        """Page.first_annot compatibility entry (case L4.first_annot.chain)."""
        ...
    @property
    def first_link(self) -> _Any:
        """Page.first_link compatibility entry (case l2.links.first_next)."""
        ...
    @property
    def first_widget(self) -> _Any:
        """Page.first_widget compatibility entry (case l4-page-first-widget)."""
        ...
    def get_bboxlog(self, layers: _Any = ...) -> _Any:
        """Page.get_bboxlog compatibility entry (case l2-page-get-bboxlog)."""
        ...
    def get_cdrawings(self, extended: _Any = ..., callback: _Any = ..., method: _Any = ...) -> _Any:
        """Page.get_cdrawings compatibility entry (case l4-page-get-cdrawings)."""
        ...
    def get_contents(self) -> _Any:
        """Page.get_contents compatibility entry (case l2-page-get-contents)."""
        ...
    def get_displaylist(self, annots: _Any = ...) -> _Any:
        """Page.get_displaylist compatibility entry (case l2-page-get-displaylist)."""
        ...
    def get_drawings(self, extended: _Any = ...) -> _Any:
        """Page.get_drawings compatibility entry (case L4.get_drawings.default)."""
        ...
    def get_fonts(self, full: _Any = ...) -> _Any:
        """Page.get_fonts compatibility entry (case L4.insert_font.basic)."""
        ...
    def get_image_bbox(self, name: _Any, transform: _Any = ...) -> _Any:
        """Page.get_image_bbox compatibility entry (case l3-page-get-image-bbox)."""
        ...
    def get_image_info(self, hashes: _Any = ..., xrefs: _Any = ...) -> _Any:
        """Page.get_image_info compatibility entry (case l3-page-get-image-info)."""
        ...
    def get_image_rects(self, name: _Any, transform: _Any = ...) -> _Any:
        """Page.get_image_rects compatibility entry (case l3-page-get-image-rects)."""
        ...
    def get_images(self, full: _Any = ...) -> _Any:
        """Page.get_images compatibility entry (case l3-page-get-images)."""
        ...
    def get_label(self) -> _Any:
        """Page.get_label compatibility entry (case l2-page-get-label)."""
        ...
    def get_layout(self, **kwargs: _Any) -> _Any:
        """Page.get_layout compatibility entry (case L1.get_layout.default)."""
        ...
    def get_links(self) -> _Any:
        """Page.get_links compatibility entry (case l2-page-links)."""
        ...
    def get_oc_items(self) -> _Any:
        """Page.get_oc_items compatibility entry (case l2-page-get-oc-items)."""
        ...
    def get_pixmap(self, *, matrix: _Any = ..., dpi: _Any = ..., colorspace: _Any = ..., clip: _Any = ..., alpha: _Any = ..., annots: _Any = ...) -> _Any:
        """Page.get_pixmap compatibility entry (case l3-page-get-pixmap-alpha)."""
        ...
    def get_svg_image(self, matrix: _Any = ..., text_as_path: _Any = ...) -> _Any:
        """Page.get_svg_image compatibility entry (case l3-page-get-svg-image)."""
        ...
    def get_text(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Page.get_text compatibility entry (case L1.get_text.bogus_kw)."""
        ...
    def get_text_blocks(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Page.get_text_blocks compatibility entry (case L1.get_text.blocks)."""
        ...
    def get_text_selection(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Page.get_text_selection compatibility entry (case L1.get_text_selection.clip)."""
        ...
    def get_text_words(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Page.get_text_words compatibility entry (case L1.get_page_text.words_clip)."""
        ...
    def get_textbox(self, rect: _Any, textpage: _Any = ...) -> _Any:
        """Page.get_textbox compatibility entry (case L1.get_textbox.empty)."""
        ...
    def get_textpage(self, clip: _Any = ..., flags: _Any = ..., matrix: _Any = ...) -> _Any:
        """Page.get_textpage compatibility entry (case L1.get_textpage.bogus_kw)."""
        ...
    def get_textpage_ocr(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Page.get_textpage_ocr compatibility entry (case l1-page-get-textpage-ocr)."""
        ...
    def get_texttrace(self) -> _Any:
        """Page.get_texttrace compatibility entry (case L1.get_texttrace.default)."""
        ...
    def get_xobjects(self) -> _Any:
        """Page.get_xobjects compatibility entry (case l2-page-get-xobjects)."""
        ...
    def insert_font(self, fontname: _Any = ..., fontfile: _Any = ..., fontbuffer: _Any = ..., set_simple: _Any = ..., wmode: _Any = ..., encoding: _Any = ...) -> _Any:
        """Page.insert_font compatibility entry (case L4.insert_font.basic)."""
        ...
    def insert_htmlbox(self, rect: _Any, text: _Any, *, css: _Any = ..., scale_low: _Any = ..., archive: _Any = ..., rotate: _Any = ..., oc: _Any = ..., opacity: _Any = ..., overlay: _Any = ..., _scale_word_width: _Any = ..., _verbose: _Any = ...) -> _Any:
        """Page.insert_htmlbox compatibility entry (case l4-page-insert-htmlbox)."""
        ...
    def insert_image(self, rect: _Any, *, alpha: _Any = ..., filename: _Any = ..., height: _Any = ..., keep_proportion: _Any = ..., mask: _Any = ..., oc: _Any = ..., overlay: _Any = ..., pixmap: _Any = ..., rotate: _Any = ..., stream: _Any = ..., width: _Any = ..., xref: _Any = ...) -> _Any:
        """Page.insert_image compatibility entry (case L4.insert_image.pixmap)."""
        ...
    def insert_link(self, lnk: _Any, mark: _Any = ...) -> _Any:
        """Page.insert_link compatibility entry (case l2.links.insert_bad)."""
        ...
    def insert_text(self, point: _Any, text: _Any, *, fontsize: _Any = ..., lineheight: _Any = ..., fontname: _Any = ..., fontfile: _Any = ..., set_simple: _Any = ..., encoding: _Any = ..., color: _Any = ..., fill: _Any = ..., border_width: _Any = ..., miter_limit: _Any = ..., render_mode: _Any = ..., rotate: _Any = ..., morph: _Any = ..., overlay: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.insert_text compatibility entry (case L4.insert_text.basic)."""
        ...
    def insert_textbox(self, rect: _Any, buffer: _Any, *, align: _Any = ..., border_width: _Any = ..., color: _Any = ..., encoding: _Any = ..., expandtabs: _Any = ..., fill_opacity: _Any = ..., fill: _Any = ..., fontfile: _Any = ..., fontname: _Any = ..., fontsize: _Any = ..., lineheight: _Any = ..., miter_limit: _Any = ..., morph: _Any = ..., oc: _Any = ..., overlay: _Any = ..., render_mode: _Any = ..., rotate: _Any = ..., set_simple: _Any = ..., stroke_opacity: _Any = ...) -> _Any:
        """Page.insert_textbox compatibility entry (case L4.insert_textbox.align)."""
        ...
    @property
    def is_wrapped(self) -> _Any:
        """Page.is_wrapped compatibility entry (case l2-page-is-wrapped)."""
        ...
    @property
    def language(self) -> _Any:
        """Page.language compatibility entry (case l2-page-language)."""
        ...
    def links(self, kinds: _Any = ...) -> _Any:
        """Page.links compatibility entry (case l2.links.iter)."""
        ...
    def load_annot(self, ident: _Any) -> _Any:
        """Page.load_annot compatibility entry (case L4.load_annot.basic)."""
        ...
    def load_links(self) -> _Any:
        """Page.load_links compatibility entry (case l4-page-load-links)."""
        ...
    def load_widget(self, xref: _Any) -> _Any:
        """Page.load_widget compatibility entry (case L4.load_widget.delete)."""
        ...
    @property
    def mediabox(self) -> _Any:
        """Page.mediabox compatibility entry (case l2.page.mediabox)."""
        ...
    @property
    def mediabox_size(self) -> _Any:
        """Page.mediabox_size compatibility entry (case l2-page-mediabox-size)."""
        ...
    def new_shape(self) -> _Any:
        """Page.new_shape compatibility entry (case L4.new_shape.bezier)."""
        ...
    def read_contents(self) -> _Any:
        """Page.read_contents compatibility entry (case l2-page-read-contents)."""
        ...
    def recolor(self, components: _Any = ...) -> _Any:
        """Page.recolor compatibility entry (case l2-page-recolor)."""
        ...
    @property
    def rect(self) -> _Any:
        """Page.rect compatibility entry (case l2.page.rect)."""
        ...
    def refresh(self) -> _Any:
        """Page.refresh compatibility entry (case l2-page-refresh)."""
        ...
    def remove_rotation(self) -> _Any:
        """Page.remove_rotation compatibility entry (case l2.page.remove_rotation)."""
        ...
    def replace_image(self, xref: _Any, *, filename: _Any = ..., pixmap: _Any = ..., stream: _Any = ...) -> _Any:
        """Page.replace_image compatibility entry (case l4-page-replace-image)."""
        ...
    @property
    def rotation(self) -> _Any:
        """Page.rotation compatibility entry (case l2-page-rotation)."""
        ...
    @property
    def rotation_matrix(self) -> _Any:
        """Page.rotation_matrix compatibility entry (case l2-page-rotation-matrix)."""
        ...
    def run(self, dw: _Any, m: _Any) -> _Any:
        """Page.run compatibility entry (case l2-page-run)."""
        ...
    def search_for(self, text: _Any, *, clip: _Any = ..., quads: _Any = ..., flags: _Any = ..., textpage: _Any = ...) -> _Any:
        """Page.search_for compatibility entry (case L1.search_for.clip_miss)."""
        ...
    def set_artbox(self, rect: _Any) -> _Any:
        """Page.set_artbox compatibility entry (case l4-page-set-artbox)."""
        ...
    def set_bleedbox(self, rect: _Any) -> _Any:
        """Page.set_bleedbox compatibility entry (case l4-page-set-bleedbox)."""
        ...
    def set_contents(self, xref: _Any) -> _Any:
        """Page.set_contents compatibility entry (case l4-page-set-contents)."""
        ...
    def set_cropbox(self, rect: _Any) -> _Any:
        """Page.set_cropbox compatibility entry (case l2.page.set_cropbox_bad)."""
        ...
    def set_language(self, language: _Any = ...) -> _Any:
        """Page.set_language compatibility entry (case l4-page-set-language)."""
        ...
    def set_mediabox(self, rect: _Any) -> _Any:
        """Page.set_mediabox compatibility entry (case l2.page.set_mediabox)."""
        ...
    def set_rotation(self, rotation: _Any) -> _Any:
        """Page.set_rotation compatibility entry (case l2.page.set_rotation_180)."""
        ...
    def set_trimbox(self, rect: _Any) -> _Any:
        """Page.set_trimbox compatibility entry (case l4-page-set-trimbox)."""
        ...
    def show_pdf_page(self, rect: _Any, docsrc: _Any, pno: _Any = ..., keep_proportion: _Any = ..., overlay: _Any = ..., oc: _Any = ..., rotate: _Any = ..., clip: _Any = ...) -> _Any:
        """Page.show_pdf_page compatibility entry (case l4-page-show-pdf-page)."""
        ...
    @property
    def transformation_matrix(self) -> _Any:
        """Page.transformation_matrix compatibility entry (case l2-page-transformation-matrix)."""
        ...
    @property
    def trimbox(self) -> _Any:
        """Page.trimbox compatibility entry (case l2-page-trimbox)."""
        ...
    def update_link(self, lnk: _Any) -> _Any:
        """Page.update_link compatibility entry (case l4-page-update-link)."""
        ...
    def widgets(self, types: _Any = ...) -> _Any:
        """Page.widgets compatibility entry (case L4.widgets.list)."""
        ...
    def wrap_contents(self) -> _Any:
        """Page.wrap_contents compatibility entry (case l2-page-wrap-contents)."""
        ...
    def write_text(self, rect: _Any = ..., writers: _Any = ..., overlay: _Any = ..., color: _Any = ..., opacity: _Any = ..., keep_proportion: _Any = ..., rotate: _Any = ..., oc: _Any = ...) -> _Any:
        """Page.write_text compatibility entry (case l4-page-write-text)."""
        ...
    @property
    def xref(self) -> _Any:
        """Page.xref compatibility entry (case l2-page-xref)."""
        ...


class TextPage:
    """TextPage compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """TextPage.__eq__ compatibility entry (case l1-textpage-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """TextPage.__ge__ compatibility entry (case l1-textpage-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """TextPage.__gt__ compatibility entry (case l1-textpage-gt)."""
        ...
    def __init__(self, *args: _Any) -> None:
        """TextPage.__init__ compatibility entry (case l1-textpage-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """TextPage.__le__ compatibility entry (case l1-textpage-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """TextPage.__lt__ compatibility entry (case l1-textpage-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """TextPage.__ne__ compatibility entry (case l1-textpage-ne)."""
        ...
    def extractBLOCKS(self) -> _Any:
        """TextPage.extractBLOCKS compatibility entry (case L1.textpage.extractBLOCKS)."""
        ...
    def extractDICT(self, cb: _Any = ..., sort: _Any = ...) -> _Any:
        """TextPage.extractDICT compatibility entry (case L1.textpage.extractDICT)."""
        ...
    def extractHTML(self) -> _Any:
        """TextPage.extractHTML compatibility entry (case L1.textpage.extractHTML)."""
        ...
    def extractIMGINFO(self, hashes: _Any = ...) -> _Any:
        """TextPage.extractIMGINFO compatibility entry (case L1.textpage.extractIMGINFO)."""
        ...
    def extractJSON(self, cb: _Any = ..., sort: _Any = ...) -> _Any:
        """TextPage.extractJSON compatibility entry (case L1.textpage.extractJSON)."""
        ...
    def extractRAWDICT(self, cb: _Any = ..., sort: _Any = ...) -> _Any:
        """TextPage.extractRAWDICT compatibility entry (case L1.textpage.extractRAWDICT)."""
        ...
    def extractRAWJSON(self, cb: _Any = ..., sort: _Any = ...) -> _Any:
        """TextPage.extractRAWJSON compatibility entry (case L1.textpage.extractRAWJSON)."""
        ...
    def extractSelection(self, pointa: _Any, pointb: _Any) -> _Any:
        """TextPage.extractSelection compatibility entry (case L1.textpage.extractSelection)."""
        ...
    def extractTEXT(self, sort: _Any = ...) -> _Any:
        """TextPage.extractTEXT compatibility entry (case L1.textpage.empty_text)."""
        ...
    def extractText(self, sort: _Any = ...) -> _Any:
        """TextPage.extractText compatibility entry (case L1.textpage.extractText.alias)."""
        ...
    def extractTextbox(self, rect: _Any) -> _Any:
        """TextPage.extractTextbox compatibility entry (case L1.textpage.extractTextbox)."""
        ...
    def extractWORDS(self, delimiters: _Any = ...) -> _Any:
        """TextPage.extractWORDS compatibility entry (case L1.textpage.extractWORDS)."""
        ...
    def extractXHTML(self) -> _Any:
        """TextPage.extractXHTML compatibility entry (case L1.textpage.extractXHTML)."""
        ...
    def extractXML(self) -> _Any:
        """TextPage.extractXML compatibility entry (case L1.textpage.extractXML)."""
        ...
    def poolsize(self) -> _Any:
        """TextPage.poolsize compatibility entry (case L1.textpage.poolsize)."""
        ...
    @property
    def rect(self) -> _Any:
        """TextPage.rect compatibility entry (case L1.textpage.rect)."""
        ...
    def search(self, needle: _Any, hit_max: _Any = ..., quads: _Any = ...) -> _Any:
        """TextPage.search compatibility entry (case L1.textpage.search_hit_max)."""
        ...


class Pixmap:
    """Pixmap compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Pixmap.__eq__ compatibility entry (case l3-pixmap-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Pixmap.__ge__ compatibility entry (case l3-pixmap-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Pixmap.__gt__ compatibility entry (case l3-pixmap-gt)."""
        ...
    def __init__(self, *args: _Any) -> None:
        """Pixmap.__init__ compatibility entry (case l3-pixmap-init-blank)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Pixmap.__le__ compatibility entry (case l3-pixmap-le)."""
        ...
    def __len__(self) -> _Any:
        """Pixmap.__len__ compatibility entry (case l3-pixmap-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Pixmap.__lt__ compatibility entry (case l3-pixmap-order)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Pixmap.__ne__ compatibility entry (case l3-pixmap-ne)."""
        ...
    @property
    def alpha(self) -> _Any:
        """Pixmap.alpha compatibility entry (case l3-doc-get-page-pixmap)."""
        ...
    def clear_with(self, value: _Any = ..., bbox: _Any = ...) -> _Any:
        """Pixmap.clear_with compatibility entry (case l3-pixmap-clear-with)."""
        ...
    def color_count(self, colors: _Any = ..., clip: _Any = ...) -> _Any:
        """Pixmap.color_count compatibility entry (case l3-pixmap-color-count)."""
        ...
    def color_topusage(self, clip: _Any = ...) -> _Any:
        """Pixmap.color_topusage compatibility entry (case l3-pixmap-color-topusage)."""
        ...
    @property
    def colorspace(self) -> _Any:
        """Pixmap.colorspace compatibility entry (case l3-pixmap-colorspace)."""
        ...
    def copy(self, src: _Any, bbox: _Any) -> _Any:
        """Pixmap.copy compatibility entry (case l3-pixmap-copy)."""
        ...
    @property
    def digest(self) -> _Any:
        """Pixmap.digest compatibility entry (case l3-pixmap-digest)."""
        ...
    def gamma_with(self, gamma: _Any) -> _Any:
        """Pixmap.gamma_with compatibility entry (case l3-pixmap-gamma-with)."""
        ...
    @property
    def h(self) -> _Any:
        """Pixmap.h compatibility entry (case l3-pixmap-size)."""
        ...
    @property
    def height(self) -> _Any:
        """Pixmap.height compatibility entry (case l3-pixmap-size)."""
        ...
    def invert_irect(self, bbox: _Any = ...) -> _Any:
        """Pixmap.invert_irect compatibility entry (case l3-pixmap-invert-irect)."""
        ...
    @property
    def irect(self) -> _Any:
        """Pixmap.irect compatibility entry (case l3-pixmap-layout)."""
        ...
    @property
    def is_monochrome(self) -> _Any:
        """Pixmap.is_monochrome compatibility entry (case l3-pixmap-is-monochrome)."""
        ...
    @property
    def is_unicolor(self) -> _Any:
        """Pixmap.is_unicolor compatibility entry (case l3-pixmap-unicolor)."""
        ...
    @property
    def n(self) -> _Any:
        """Pixmap.n compatibility entry (case l3-pixmap-layout)."""
        ...
    def pdfocr_save(self, filename: _Any, compress: _Any = ..., language: _Any = ..., tessdata: _Any = ...) -> _Any:
        """Pixmap.pdfocr_save compatibility entry (case l3-pixmap-pdfocr-save)."""
        ...
    def pdfocr_tobytes(self, compress: _Any = ..., language: _Any = ..., tessdata: _Any = ...) -> _Any:
        """Pixmap.pdfocr_tobytes compatibility entry (case l3-pixmap-pdfocr-tobytes)."""
        ...
    def pil_image(self) -> _Any:
        """Pixmap.pil_image compatibility entry (case l3-pixmap-pil)."""
        ...
    def pil_save(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Pixmap.pil_save compatibility entry (case l3-pixmap-pil-save)."""
        ...
    def pil_tobytes(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Pixmap.pil_tobytes compatibility entry (case l3-pixmap-pil-tobytes)."""
        ...
    def pixel(self, x: _Any, y: _Any) -> _Any:
        """Pixmap.pixel compatibility entry (case l3-pixmap-pixel)."""
        ...
    @property
    def samples(self) -> _Any:
        """Pixmap.samples compatibility entry (case l3-pixmap-samples)."""
        ...
    @property
    def samples_mv(self) -> _Any:
        """Pixmap.samples_mv compatibility entry (case l3-pixmap-samples-mv)."""
        ...
    @property
    def samples_ptr(self) -> _Any:
        """Pixmap.samples_ptr compatibility entry (case l3-pixmap-samples-ptr)."""
        ...
    def save(self, filename: _Any, output: _Any = ..., jpg_quality: _Any = ...) -> _Any:
        """Pixmap.save compatibility entry (case l3-pixmap-save)."""
        ...
    def set_alpha(self, alphavalues: _Any = ..., premultiply: _Any = ..., opaque: _Any = ..., matte: _Any = ...) -> _Any:
        """Pixmap.set_alpha compatibility entry (case l3-pixmap-set-alpha)."""
        ...
    def set_dpi(self, xres: _Any, yres: _Any) -> _Any:
        """Pixmap.set_dpi compatibility entry (case l3-pixmap-resolution)."""
        ...
    def set_origin(self, x: _Any, y: _Any) -> _Any:
        """Pixmap.set_origin compatibility entry (case l3-pixmap-set-origin)."""
        ...
    def set_pixel(self, x: _Any, y: _Any, color: _Any) -> _Any:
        """Pixmap.set_pixel compatibility entry (case l3-pixmap-set-pixel)."""
        ...
    def set_rect(self, bbox: _Any, color: _Any) -> _Any:
        """Pixmap.set_rect compatibility entry (case l3-pixmap-set-rect)."""
        ...
    def shrink(self, factor: _Any) -> _Any:
        """Pixmap.shrink compatibility entry (case l3-pixmap-shrink)."""
        ...
    @property
    def size(self) -> _Any:
        """Pixmap.size compatibility entry (case l3-pixmap-size)."""
        ...
    @property
    def stride(self) -> _Any:
        """Pixmap.stride compatibility entry (case l3-doc-get-page-pixmap)."""
        ...
    def tint_with(self, black: _Any, white: _Any) -> _Any:
        """Pixmap.tint_with compatibility entry (case l3-pixmap-tint-with)."""
        ...
    def tobytes(self, output: _Any = ..., jpg_quality: _Any = ...) -> _Any:
        """Pixmap.tobytes compatibility entry (case l3-pixmap-tobytes-bad)."""
        ...
    @property
    def w(self) -> _Any:
        """Pixmap.w compatibility entry (case l3-pixmap-size)."""
        ...
    def warp(self, quad: _Any, width: _Any, height: _Any) -> _Any:
        """Pixmap.warp compatibility entry (case l3-pixmap-warp)."""
        ...
    @property
    def width(self) -> _Any:
        """Pixmap.width compatibility entry (case l3-pixmap-72)."""
        ...
    @property
    def x(self) -> _Any:
        """Pixmap.x compatibility entry (case l3-pixmap-x)."""
        ...
    @property
    def xres(self) -> _Any:
        """Pixmap.xres compatibility entry (case l3-pixmap-xres)."""
        ...
    @property
    def y(self) -> _Any:
        """Pixmap.y compatibility entry (case l3-pixmap-y)."""
        ...
    @property
    def yres(self) -> _Any:
        """Pixmap.yres compatibility entry (case l3-pixmap-yres)."""
        ...


class Annot:
    """Annot compatibility surface."""
    def __bool__(self) -> _Any:
        """Annot.__bool__ compatibility entry (case l4-annot-bool)."""
        ...
    def __eq__(self, value: _Any) -> _Any:
        """Annot.__eq__ compatibility entry (case l4-annot-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Annot.__ge__ compatibility entry (case l4-annot-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Annot.__gt__ compatibility entry (case l4-annot-gt)."""
        ...
    def __init__(self, annot: _Any) -> None:
        """Annot.__init__ compatibility entry (case l4-annot-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Annot.__le__ compatibility entry (case l4-annot-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Annot.__lt__ compatibility entry (case l4-annot-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Annot.__ne__ compatibility entry (case l4-annot-ne)."""
        ...
    @property
    def apn_bbox(self) -> _Any:
        """Annot.apn_bbox compatibility entry (case l4-annot-apn-bbox)."""
        ...
    @property
    def apn_matrix(self) -> _Any:
        """Annot.apn_matrix compatibility entry (case l4-annot-apn-matrix)."""
        ...
    @property
    def blendmode(self) -> _Any:
        """Annot.blendmode compatibility entry (case L4.annot.appearance_state)."""
        ...
    @property
    def border(self) -> _Any:
        """Annot.border compatibility entry (case L4.annot.border)."""
        ...
    def clean_contents(self, sanitize: _Any = ...) -> _Any:
        """Annot.clean_contents compatibility entry (case l4-annot-clean-contents)."""
        ...
    @property
    def colors(self) -> _Any:
        """Annot.colors compatibility entry (case L4.annot.colors)."""
        ...
    def delete_responses(self) -> _Any:
        """Annot.delete_responses compatibility entry (case l4-annot-delete-responses)."""
        ...
    @property
    def file_info(self) -> _Any:
        """Annot.file_info compatibility entry (case L4.annot.file_roundtrip)."""
        ...
    @property
    def flags(self) -> _Any:
        """Annot.flags compatibility entry (case L4.annot.flags)."""
        ...
    def get_file(self) -> _Any:
        """Annot.get_file compatibility entry (case l4-annot-get-file)."""
        ...
    def get_oc(self) -> _Any:
        """Annot.get_oc compatibility entry (case l4-annot-get-oc)."""
        ...
    def get_parent(self) -> _Any:
        """Annot.get_parent compatibility entry (case l4-annot-get-parent)."""
        ...
    def get_pixmap(self, matrix: _Any = ..., dpi: _Any = ..., colorspace: _Any = ..., alpha: _Any = ...) -> _Any:
        """Annot.get_pixmap compatibility entry (case L4.annot.get_pixmap)."""
        ...
    def get_sound(self) -> _Any:
        """Annot.get_sound compatibility entry (case l4-annot-get-sound)."""
        ...
    def get_text(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Annot.get_text compatibility entry (case l4-annot-get-text)."""
        ...
    def get_textbox(self, *args: _Any, **kwargs: _Any) -> _Any:
        """Annot.get_textbox compatibility entry (case L4.annot.get_textbox)."""
        ...
    def get_textpage(self, clip: _Any = ..., flags: _Any = ...) -> _Any:
        """Annot.get_textpage compatibility entry (case L4.annot.get_textpage)."""
        ...
    @property
    def has_popup(self) -> _Any:
        """Annot.has_popup compatibility entry (case L4.annot.popup)."""
        ...
    @property
    def info(self) -> _Any:
        """Annot.info compatibility entry (case L4.annot.info)."""
        ...
    @property
    def irt_xref(self) -> _Any:
        """Annot.irt_xref compatibility entry (case L4.annot.irt)."""
        ...
    @property
    def is_open(self) -> _Any:
        """Annot.is_open compatibility entry (case l4-annot-is-open)."""
        ...
    @property
    def language(self) -> _Any:
        """Annot.language compatibility entry (case l4-annot-language)."""
        ...
    @property
    def line_ends(self) -> _Any:
        """Annot.line_ends compatibility entry (case l4-annot-line-ends)."""
        ...
    @property
    def next(self) -> _Any:
        """Annot.next compatibility entry (case l4-annot-next)."""
        ...
    @property
    def opacity(self) -> _Any:
        """Annot.opacity compatibility entry (case L4.annot.opacity)."""
        ...
    @property
    def popup_rect(self) -> _Any:
        """Annot.popup_rect compatibility entry (case l4-annot-popup-rect)."""
        ...
    @property
    def popup_xref(self) -> _Any:
        """Annot.popup_xref compatibility entry (case l4-annot-popup-xref)."""
        ...
    @property
    def rect(self) -> _Any:
        """Annot.rect compatibility entry (case L4.annot.rect)."""
        ...
    @property
    def rect_delta(self) -> _Any:
        """Annot.rect_delta compatibility entry (case l4-annot-rect-delta)."""
        ...
    @property
    def rotation(self) -> _Any:
        """Annot.rotation compatibility entry (case L4.annot.rotation)."""
        ...
    def set_apn_bbox(self, bbox: _Any) -> _Any:
        """Annot.set_apn_bbox compatibility entry (case l4-annot-set-apn-bbox)."""
        ...
    def set_apn_matrix(self, matrix: _Any) -> _Any:
        """Annot.set_apn_matrix compatibility entry (case l4-annot-set-apn-matrix)."""
        ...
    def set_blendmode(self, blend_mode: _Any) -> _Any:
        """Annot.set_blendmode compatibility entry (case l4-annot-set-blendmode)."""
        ...
    def set_border(self, border: _Any = ..., width: _Any = ..., style: _Any = ..., dashes: _Any = ..., clouds: _Any = ...) -> _Any:
        """Annot.set_border compatibility entry (case l4-annot-set-border)."""
        ...
    def set_colors(self, colors: _Any = ..., stroke: _Any = ..., fill: _Any = ...) -> _Any:
        """Annot.set_colors compatibility entry (case L4.add_rect_annot.basic)."""
        ...
    def set_flags(self, flags: _Any) -> _Any:
        """Annot.set_flags compatibility entry (case l4-annot-set-flags)."""
        ...
    def set_info(self, info: _Any = ..., content: _Any = ..., title: _Any = ..., creationDate: _Any = ..., modDate: _Any = ..., subject: _Any = ...) -> _Any:
        """Annot.set_info compatibility entry (case l4-annot-set-info)."""
        ...
    def set_irt_xref(self, xref: _Any) -> _Any:
        """Annot.set_irt_xref compatibility entry (case l4-annot-set-irt-xref)."""
        ...
    def set_language(self, language: _Any = ...) -> _Any:
        """Annot.set_language compatibility entry (case l4-annot-set-language)."""
        ...
    def set_line_ends(self, start: _Any, end: _Any) -> _Any:
        """Annot.set_line_ends compatibility entry (case l4-annot-set-line-ends)."""
        ...
    def set_name(self, name: _Any) -> _Any:
        """Annot.set_name compatibility entry (case l4-annot-set-name)."""
        ...
    def set_oc(self, oc: _Any = ...) -> _Any:
        """Annot.set_oc compatibility entry (case l4-annot-set-oc)."""
        ...
    def set_opacity(self, opacity: _Any) -> _Any:
        """Annot.set_opacity compatibility entry (case l4-annot-set-opacity)."""
        ...
    def set_open(self, is_open: _Any) -> _Any:
        """Annot.set_open compatibility entry (case l4-annot-set-open)."""
        ...
    def set_popup(self, rect: _Any) -> _Any:
        """Annot.set_popup compatibility entry (case l4-annot-set-popup)."""
        ...
    def set_rect(self, rect: _Any) -> _Any:
        """Annot.set_rect compatibility entry (case l4-annot-set-rect)."""
        ...
    def set_rotation(self, rotate: _Any = ...) -> _Any:
        """Annot.set_rotation compatibility entry (case l4-annot-set-rotation)."""
        ...
    @property
    def type(self) -> _Any:
        """Annot.type compatibility entry (case L4.annot.type)."""
        ...
    def update(self, blend_mode: _Any = ..., opacity: _Any = ..., fontsize: _Any = ..., fontname: _Any = ..., text_color: _Any = ..., border_color: _Any = ..., fill_color: _Any = ..., cross_out: _Any = ..., rotate: _Any = ...) -> _Any:
        """Annot.update compatibility entry (case L4.annot.update)."""
        ...
    def update_file(self, buffer_: _Any = ..., filename: _Any = ..., ufilename: _Any = ..., desc: _Any = ...) -> _Any:
        """Annot.update_file compatibility entry (case l4-annot-update-file)."""
        ...
    def update_timing_test(self) -> _Any:
        """Annot.update_timing_test compatibility entry (case l4-annot-update-timing-test)."""
        ...
    @property
    def vertices(self) -> _Any:
        """Annot.vertices compatibility entry (case l4-annot-vertices)."""
        ...
    @property
    def xref(self) -> _Any:
        """Annot.xref compatibility entry (case l4-annot-xref)."""
        ...


class Widget:
    """Widget compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Widget.__eq__ compatibility entry (case l4-widget-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Widget.__ge__ compatibility entry (case l4-widget-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Widget.__gt__ compatibility entry (case l4-widget-gt)."""
        ...
    def __init__(self) -> None:
        """Widget.__init__ compatibility entry (case L4.widget.add)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Widget.__le__ compatibility entry (case l4-widget-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Widget.__lt__ compatibility entry (case l4-widget-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Widget.__ne__ compatibility entry (case l4-widget-ne)."""
        ...
    def button_states(self) -> _Any:
        """Widget.button_states compatibility entry (case l4-widget-button-states)."""
        ...
    @property
    def next(self) -> _Any:
        """Widget.next compatibility entry (case l4-widget-next)."""
        ...
    def on_state(self) -> _Any:
        """Widget.on_state compatibility entry (case l4-widget-on-state)."""
        ...
    def reset(self) -> _Any:
        """Widget.reset compatibility entry (case L4.widget.reset)."""
        ...
    def update(self, sync_flags: _Any = ...) -> _Any:
        """Widget.update compatibility entry (case L4.widget.update_text)."""
        ...


class Link:
    """Link compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Link.__eq__ compatibility entry (case l2-link-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Link.__ge__ compatibility entry (case l2-link-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Link.__gt__ compatibility entry (case l2-link-gt)."""
        ...
    def __init__(self, this: _Any) -> None:
        """Link.__init__ compatibility entry (case l2-link-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Link.__le__ compatibility entry (case l2-link-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Link.__lt__ compatibility entry (case l2-link-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Link.__ne__ compatibility entry (case l2-link-ne)."""
        ...
    @property
    def border(self) -> _Any:
        """Link.border compatibility entry (case l2-link-border)."""
        ...
    @property
    def colors(self) -> _Any:
        """Link.colors compatibility entry (case l2-link-colors)."""
        ...
    @property
    def dest(self) -> _Any:
        """Link.dest compatibility entry (case l2-link-dest)."""
        ...
    @property
    def flags(self) -> _Any:
        """Link.flags compatibility entry (case l2-link-flags)."""
        ...
    @property
    def is_external(self) -> _Any:
        """Link.is_external compatibility entry (case l2-link-is-external)."""
        ...
    @property
    def next(self) -> _Any:
        """Link.next compatibility entry (case l2-link-next)."""
        ...
    @property
    def rect(self) -> _Any:
        """Link.rect compatibility entry (case l2-link-rect)."""
        ...
    def set_border(self, border: _Any = ..., width: _Any = ..., dashes: _Any = ..., style: _Any = ...) -> _Any:
        """Link.set_border compatibility entry (case l2-link-set-border)."""
        ...
    def set_colors(self, colors: _Any = ..., stroke: _Any = ..., fill: _Any = ...) -> _Any:
        """Link.set_colors compatibility entry (case l2-link-set-colors)."""
        ...
    def set_flags(self, flags: _Any) -> _Any:
        """Link.set_flags compatibility entry (case l2-link-set-flags)."""
        ...
    @property
    def uri(self) -> _Any:
        """Link.uri compatibility entry (case l2-link-uri)."""
        ...


class Shape:
    """Shape compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Shape.__eq__ compatibility entry (case l4-shape-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Shape.__ge__ compatibility entry (case l4-shape-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Shape.__gt__ compatibility entry (case l4-shape-gt)."""
        ...
    def __init__(self, page: _Any) -> None:
        """Shape.__init__ compatibility entry (case l4-shape-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Shape.__le__ compatibility entry (case l4-shape-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Shape.__lt__ compatibility entry (case l4-shape-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Shape.__ne__ compatibility entry (case l4-shape-ne)."""
        ...
    def commit(self, overlay: _Any = ...) -> _Any:
        """Shape.commit compatibility entry (case L4.shape.commit_overlay)."""
        ...
    def draw_bezier(self, p1: _Any, p2: _Any, p3: _Any, p4: _Any) -> _Any:
        """Shape.draw_bezier compatibility entry (case l4-shape-draw-bezier)."""
        ...
    def draw_circle(self, center: _Any, radius: _Any) -> _Any:
        """Shape.draw_circle compatibility entry (case l4-shape-draw-circle)."""
        ...
    def draw_curve(self, p1: _Any, p2: _Any, p3: _Any) -> _Any:
        """Shape.draw_curve compatibility entry (case l4-shape-draw-curve)."""
        ...
    def draw_line(self, p1: _Any, p2: _Any) -> _Any:
        """Shape.draw_line compatibility entry (case L4.new_shape.line)."""
        ...
    def draw_oval(self, tetra: _Any) -> _Any:
        """Shape.draw_oval compatibility entry (case l4-shape-draw-oval)."""
        ...
    def draw_polyline(self, points: _Any) -> _Any:
        """Shape.draw_polyline compatibility entry (case l4-shape-draw-polyline)."""
        ...
    def draw_quad(self, quad: _Any) -> _Any:
        """Shape.draw_quad compatibility entry (case l4-shape-draw-quad)."""
        ...
    def draw_rect(self, rect: _Any, *, radius: _Any = ...) -> _Any:
        """Shape.draw_rect compatibility entry (case L4.new_shape.rect)."""
        ...
    def draw_sector(self, center: _Any, point: _Any, beta: _Any, fullSector: _Any = ...) -> _Any:
        """Shape.draw_sector compatibility entry (case l4-shape-draw-sector)."""
        ...
    def draw_squiggle(self, p1: _Any, p2: _Any, breadth: _Any = ...) -> _Any:
        """Shape.draw_squiggle compatibility entry (case l4-shape-draw-squiggle)."""
        ...
    def draw_zigzag(self, p1: _Any, p2: _Any, breadth: _Any = ...) -> _Any:
        """Shape.draw_zigzag compatibility entry (case l4-shape-draw-zigzag)."""
        ...
    def finish(self, width: _Any = ..., color: _Any = ..., fill: _Any = ..., lineCap: _Any = ..., lineJoin: _Any = ..., dashes: _Any = ..., even_odd: _Any = ..., morph: _Any = ..., closePath: _Any = ..., fill_opacity: _Any = ..., stroke_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Shape.finish compatibility entry (case L4.shape.finish_options)."""
        ...
    def horizontal_angle(self, C: _Any, P: _Any) -> _Any:
        """Shape.horizontal_angle compatibility entry (case l4-shape-horizontal-angle)."""
        ...
    def insert_text(self, point: _Any, buffer: _Any, *, fontsize: _Any = ..., lineheight: _Any = ..., fontname: _Any = ..., fontfile: _Any = ..., set_simple: _Any = ..., encoding: _Any = ..., color: _Any = ..., fill: _Any = ..., render_mode: _Any = ..., border_width: _Any = ..., miter_limit: _Any = ..., rotate: _Any = ..., morph: _Any = ..., stroke_opacity: _Any = ..., fill_opacity: _Any = ..., oc: _Any = ...) -> _Any:
        """Shape.insert_text compatibility entry (case L4.shape.insert_text)."""
        ...
    def insert_textbox(self, rect: _Any, buffer: _Any, *, align: _Any = ..., border_width: _Any = ..., color: _Any = ..., encoding: _Any = ..., expandtabs: _Any = ..., fill_opacity: _Any = ..., fill: _Any = ..., fontfile: _Any = ..., fontname: _Any = ..., fontsize: _Any = ..., lineheight: _Any = ..., miter_limit: _Any = ..., morph: _Any = ..., oc: _Any = ..., render_mode: _Any = ..., rotate: _Any = ..., set_simple: _Any = ..., stroke_opacity: _Any = ...) -> _Any:
        """Shape.insert_textbox compatibility entry (case l4-shape-insert-textbox)."""
        ...
    def updateRect(self, x: _Any) -> _Any:
        """Shape.updateRect compatibility entry (case l4-shape-updaterect)."""
        ...


class Rect:
    """Rect compatibility surface."""
    def __abs__(self) -> _Any:
        """Rect.__abs__ compatibility entry (case p0-rect-abs)."""
        ...
    def __add__(self, p: _Any) -> _Any:
        """Rect.__add__ compatibility entry (case p0-rect-add)."""
        ...
    def __bool__(self) -> _Any:
        """Rect.__bool__ compatibility entry (case p0-rect-bool)."""
        ...
    def __contains__(self, x: _Any) -> _Any:
        """Rect.__contains__ compatibility entry (case p0-rect-contains)."""
        ...
    def __eq__(self, rect: _Any) -> _Any:
        """Rect.__eq__ compatibility entry (case p0-rect-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Rect.__ge__ compatibility entry (case p0-rect-ge)."""
        ...
    def __getitem__(self, i: _Any) -> _Any:
        """Rect.__getitem__ compatibility entry (case p0-rect-getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Rect.__gt__ compatibility entry (case p0-rect-gt)."""
        ...
    def __init__(self, *args: _Any, p0: _Any = ..., p1: _Any = ..., x0: _Any = ..., y0: _Any = ..., x1: _Any = ..., y1: _Any = ...) -> None:
        """Rect.__init__ compatibility entry (case p0-rect-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Rect.__le__ compatibility entry (case p0-rect-le)."""
        ...
    def __len__(self) -> _Any:
        """Rect.__len__ compatibility entry (case p0-rect-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Rect.__lt__ compatibility entry (case p0-rect-lt)."""
        ...
    def __mul__(self, m: _Any) -> _Any:
        """Rect.__mul__ compatibility entry (case p0-rect-mul)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Rect.__ne__ compatibility entry (case p0-rect-ne)."""
        ...
    def __neg__(self) -> _Any:
        """Rect.__neg__ compatibility entry (case p0-rect-neg)."""
        ...
    def __pos__(self) -> _Any:
        """Rect.__pos__ compatibility entry (case p0-rect-pos)."""
        ...
    def __setitem__(self, i: _Any, v: _Any) -> _Any:
        """Rect.__setitem__ compatibility entry (case p0-rect-setitem)."""
        ...
    def __sub__(self, p: _Any) -> _Any:
        """Rect.__sub__ compatibility entry (case p0-rect-sub)."""
        ...
    def __truediv__(self, m: _Any) -> _Any:
        """Rect.__truediv__ compatibility entry (case p0-rect-truediv)."""
        ...
    @property
    def bl(self) -> _Any:
        """Rect.bl compatibility entry (case p0-rect-bl)."""
        ...
    @property
    def bottom_left(self) -> _Any:
        """Rect.bottom_left compatibility entry (case p0-rect-bottom-left)."""
        ...
    @property
    def bottom_right(self) -> _Any:
        """Rect.bottom_right compatibility entry (case p0-rect-bottom-right)."""
        ...
    @property
    def br(self) -> _Any:
        """Rect.br compatibility entry (case p0-rect-br)."""
        ...
    def contains(self, x: _Any) -> _Any:
        """Rect.contains compatibility entry (case p0-rect-contains-2)."""
        ...
    def get_area(self, *args: _Any) -> _Any:
        """Rect.get_area compatibility entry (case p0-rect-get-area)."""
        ...
    @property
    def height(self) -> _Any:
        """Rect.height compatibility entry (case p0-rect-height)."""
        ...
    def include_point(self, p: _Any) -> _Any:
        """Rect.include_point compatibility entry (case p0-rect-include-point)."""
        ...
    def include_rect(self, r: _Any) -> _Any:
        """Rect.include_rect compatibility entry (case p0-rect-include-rect)."""
        ...
    def intersect(self, r: _Any) -> _Any:
        """Rect.intersect compatibility entry (case p0-rect-intersect)."""
        ...
    def intersects(self, x: _Any) -> _Any:
        """Rect.intersects compatibility entry (case p0-rect-intersects)."""
        ...
    @property
    def irect(self) -> _Any:
        """Rect.irect compatibility entry (case p0-rect-irect)."""
        ...
    @property
    def is_empty(self) -> _Any:
        """Rect.is_empty compatibility entry (case p0-rect-is-empty)."""
        ...
    @property
    def is_infinite(self) -> _Any:
        """Rect.is_infinite compatibility entry (case p0-rect-is-infinite)."""
        ...
    @property
    def is_valid(self) -> _Any:
        """Rect.is_valid compatibility entry (case p0-rect-is-valid)."""
        ...
    def morph(self, p: _Any, m: _Any) -> _Any:
        """Rect.morph compatibility entry (case p0-rect-morph)."""
        ...
    def norm(self) -> _Any:
        """Rect.norm compatibility entry (case p0-rect-norm)."""
        ...
    def normalize(self) -> _Any:
        """Rect.normalize compatibility entry (case p0-rect-normalize)."""
        ...
    @property
    def quad(self) -> _Any:
        """Rect.quad compatibility entry (case p0-rect-quad)."""
        ...
    def round(self) -> _Any:
        """Rect.round compatibility entry (case p0-rect-round)."""
        ...
    @property
    def tl(self) -> _Any:
        """Rect.tl compatibility entry (case p0-rect-tl)."""
        ...
    @property
    def top_left(self) -> _Any:
        """Rect.top_left compatibility entry (case p0-rect-top-left)."""
        ...
    @property
    def top_right(self) -> _Any:
        """Rect.top_right compatibility entry (case p0-rect-top-right)."""
        ...
    def torect(self, r: _Any) -> _Any:
        """Rect.torect compatibility entry (case p0-rect-torect)."""
        ...
    @property
    def tr(self) -> _Any:
        """Rect.tr compatibility entry (case p0-rect-tr)."""
        ...
    def transform(self, m: _Any) -> _Any:
        """Rect.transform compatibility entry (case p0-rect-transform)."""
        ...
    @property
    def width(self) -> _Any:
        """Rect.width compatibility entry (case p0-rect-width)."""
        ...


class IRect:
    """IRect compatibility surface."""
    def __add__(self, p: _Any) -> _Any:
        """IRect.__add__ compatibility entry (case p0-irect-add)."""
        ...
    def __contains__(self, x: _Any) -> _Any:
        """IRect.__contains__ compatibility entry (case p0-irect-contains)."""
        ...
    def __eq__(self, r: _Any) -> _Any:
        """IRect.__eq__ compatibility entry (case p0-irect-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """IRect.__ge__ compatibility entry (case p0-irect-ge)."""
        ...
    def __getitem__(self, i: _Any) -> _Any:
        """IRect.__getitem__ compatibility entry (case p0-irect-getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """IRect.__gt__ compatibility entry (case p0-irect-gt)."""
        ...
    def __init__(self, *args: _Any, p0: _Any = ..., p1: _Any = ..., x0: _Any = ..., y0: _Any = ..., x1: _Any = ..., y1: _Any = ...) -> None:
        """IRect.__init__ compatibility entry (case p0-irect-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """IRect.__le__ compatibility entry (case p0-irect-le)."""
        ...
    def __len__(self) -> _Any:
        """IRect.__len__ compatibility entry (case p0-irect-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """IRect.__lt__ compatibility entry (case p0-irect-lt)."""
        ...
    def __mul__(self, m: _Any) -> _Any:
        """IRect.__mul__ compatibility entry (case p0-irect-mul)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """IRect.__ne__ compatibility entry (case p0-irect-ne)."""
        ...
    def __neg__(self) -> _Any:
        """IRect.__neg__ compatibility entry (case p0-irect-neg)."""
        ...
    def __pos__(self) -> _Any:
        """IRect.__pos__ compatibility entry (case p0-irect-pos)."""
        ...
    def __setitem__(self, i: _Any, v: _Any) -> _Any:
        """IRect.__setitem__ compatibility entry (case p0-irect-setitem)."""
        ...
    def __sub__(self, p: _Any) -> _Any:
        """IRect.__sub__ compatibility entry (case p0-irect-sub)."""
        ...
    def __truediv__(self, m: _Any) -> _Any:
        """IRect.__truediv__ compatibility entry (case p0-irect-truediv)."""
        ...
    @property
    def bl(self) -> _Any:
        """IRect.bl compatibility entry (case p0-irect-bl)."""
        ...
    @property
    def bottom_left(self) -> _Any:
        """IRect.bottom_left compatibility entry (case p0-irect-bottom-left)."""
        ...
    @property
    def bottom_right(self) -> _Any:
        """IRect.bottom_right compatibility entry (case p0-irect-bottom-right)."""
        ...
    @property
    def br(self) -> _Any:
        """IRect.br compatibility entry (case p0-irect-br)."""
        ...
    def contains(self, x: _Any) -> _Any:
        """IRect.contains compatibility entry (case p0-irect-contains-2)."""
        ...
    def get_area(self, *args: _Any) -> _Any:
        """IRect.get_area compatibility entry (case p0-irect-get-area)."""
        ...
    @property
    def height(self) -> _Any:
        """IRect.height compatibility entry (case p0-irect-height)."""
        ...
    def include_point(self, p: _Any) -> _Any:
        """IRect.include_point compatibility entry (case p0-irect-include-point)."""
        ...
    def include_rect(self, r: _Any) -> _Any:
        """IRect.include_rect compatibility entry (case p0-irect-include-rect)."""
        ...
    def intersect(self, r: _Any) -> _Any:
        """IRect.intersect compatibility entry (case p0-irect-intersect)."""
        ...
    def intersects(self, x: _Any) -> _Any:
        """IRect.intersects compatibility entry (case p0-irect-intersects)."""
        ...
    @property
    def is_empty(self) -> _Any:
        """IRect.is_empty compatibility entry (case p0-irect-is-empty)."""
        ...
    @property
    def is_infinite(self) -> _Any:
        """IRect.is_infinite compatibility entry (case p0-irect-is-infinite)."""
        ...
    @property
    def is_valid(self) -> _Any:
        """IRect.is_valid compatibility entry (case p0-irect-is-valid)."""
        ...
    def morph(self, p: _Any, m: _Any) -> _Any:
        """IRect.morph compatibility entry (case p0-irect-morph)."""
        ...
    def norm(self) -> _Any:
        """IRect.norm compatibility entry (case p0-irect-norm)."""
        ...
    def normalize(self) -> _Any:
        """IRect.normalize compatibility entry (case p0-irect-normalize)."""
        ...
    @property
    def quad(self) -> _Any:
        """IRect.quad compatibility entry (case p0-irect-quad)."""
        ...
    @property
    def rect(self) -> _Any:
        """IRect.rect compatibility entry (case p0-irect-rect)."""
        ...
    @property
    def tl(self) -> _Any:
        """IRect.tl compatibility entry (case p0-irect-tl)."""
        ...
    @property
    def top_left(self) -> _Any:
        """IRect.top_left compatibility entry (case p0-irect-top-left)."""
        ...
    @property
    def top_right(self) -> _Any:
        """IRect.top_right compatibility entry (case p0-irect-top-right)."""
        ...
    def torect(self, r: _Any) -> _Any:
        """IRect.torect compatibility entry (case p0-irect-torect)."""
        ...
    @property
    def tr(self) -> _Any:
        """IRect.tr compatibility entry (case p0-irect-tr)."""
        ...
    def transform(self, m: _Any) -> _Any:
        """IRect.transform compatibility entry (case p0-irect-transform)."""
        ...
    @property
    def width(self) -> _Any:
        """IRect.width compatibility entry (case p0-irect-width)."""
        ...


class Point:
    """Point compatibility surface."""
    def __abs__(self) -> _Any:
        """Point.__abs__ compatibility entry (case p0-point-abs)."""
        ...
    def __add__(self, p: _Any) -> _Any:
        """Point.__add__ compatibility entry (case p0-point-add)."""
        ...
    def __bool__(self) -> _Any:
        """Point.__bool__ compatibility entry (case p0-point-bool)."""
        ...
    def __eq__(self, p: _Any) -> _Any:
        """Point.__eq__ compatibility entry (case p0-point-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Point.__ge__ compatibility entry (case p0-point-ge)."""
        ...
    def __getitem__(self, i: _Any) -> _Any:
        """Point.__getitem__ compatibility entry (case p0-point-getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Point.__gt__ compatibility entry (case p0-point-gt)."""
        ...
    def __init__(self, *args: _Any, x: _Any = ..., y: _Any = ...) -> None:
        """Point.__init__ compatibility entry (case p0-point-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Point.__le__ compatibility entry (case p0-point-le)."""
        ...
    def __len__(self) -> _Any:
        """Point.__len__ compatibility entry (case p0-point-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Point.__lt__ compatibility entry (case p0-point-lt)."""
        ...
    def __mul__(self, m: _Any) -> _Any:
        """Point.__mul__ compatibility entry (case p0-point-mul)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Point.__ne__ compatibility entry (case p0-point-ne)."""
        ...
    def __neg__(self) -> _Any:
        """Point.__neg__ compatibility entry (case p0-point-neg)."""
        ...
    def __pos__(self) -> _Any:
        """Point.__pos__ compatibility entry (case p0-point-pos)."""
        ...
    def __setitem__(self, i: _Any, v: _Any) -> _Any:
        """Point.__setitem__ compatibility entry (case p0-point-setitem)."""
        ...
    def __sub__(self, p: _Any) -> _Any:
        """Point.__sub__ compatibility entry (case p0-point-sub)."""
        ...
    def __truediv__(self, m: _Any) -> _Any:
        """Point.__truediv__ compatibility entry (case p0-point-truediv)."""
        ...
    @property
    def abs_unit(self) -> _Any:
        """Point.abs_unit compatibility entry (case p0-point-abs-unit)."""
        ...
    def distance_to(self, *args: _Any) -> _Any:
        """Point.distance_to compatibility entry (case p0-point-distance-to)."""
        ...
    def norm(self) -> _Any:
        """Point.norm compatibility entry (case p0-point-norm)."""
        ...
    def transform(self, m: _Any) -> _Any:
        """Point.transform compatibility entry (case p0-point-transform)."""
        ...
    @property
    def unit(self) -> _Any:
        """Point.unit compatibility entry (case p0-point-unit)."""
        ...


class Matrix:
    """Matrix compatibility surface."""
    def __abs__(self) -> _Any:
        """Matrix.__abs__ compatibility entry (case p0-matrix-abs)."""
        ...
    def __add__(self, m: _Any) -> _Any:
        """Matrix.__add__ compatibility entry (case p0-matrix-add)."""
        ...
    def __bool__(self) -> _Any:
        """Matrix.__bool__ compatibility entry (case p0-matrix-bool)."""
        ...
    def __eq__(self, mat: _Any) -> _Any:
        """Matrix.__eq__ compatibility entry (case p0-matrix-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Matrix.__ge__ compatibility entry (case p0-matrix-ge)."""
        ...
    def __getitem__(self, i: _Any) -> _Any:
        """Matrix.__getitem__ compatibility entry (case p0-matrix-getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Matrix.__gt__ compatibility entry (case p0-matrix-gt)."""
        ...
    def __init__(self, *args: _Any, a: _Any = ..., b: _Any = ..., c: _Any = ..., d: _Any = ..., e: _Any = ..., f: _Any = ...) -> None:
        """Matrix.__init__ compatibility entry (case p0-matrix-init)."""
        ...
    def __invert__(self) -> _Any:
        """Matrix.__invert__ compatibility entry (case p0-matrix-invert)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Matrix.__le__ compatibility entry (case p0-matrix-le)."""
        ...
    def __len__(self) -> _Any:
        """Matrix.__len__ compatibility entry (case p0-matrix-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Matrix.__lt__ compatibility entry (case p0-matrix-lt)."""
        ...
    def __mul__(self, m: _Any) -> _Any:
        """Matrix.__mul__ compatibility entry (case p0-matrix-mul)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Matrix.__ne__ compatibility entry (case p0-matrix-ne)."""
        ...
    def __neg__(self) -> _Any:
        """Matrix.__neg__ compatibility entry (case p0-matrix-neg)."""
        ...
    def __pos__(self) -> _Any:
        """Matrix.__pos__ compatibility entry (case p0-matrix-pos)."""
        ...
    def __setitem__(self, i: _Any, v: _Any) -> _Any:
        """Matrix.__setitem__ compatibility entry (case p0-matrix-setitem)."""
        ...
    def __sub__(self, m: _Any) -> _Any:
        """Matrix.__sub__ compatibility entry (case p0-matrix-sub)."""
        ...
    def __truediv__(self, m: _Any) -> _Any:
        """Matrix.__truediv__ compatibility entry (case p0-matrix-truediv)."""
        ...
    def concat(self, one: _Any, two: _Any) -> _Any:
        """Matrix.concat compatibility entry (case p0-matrix-concat)."""
        ...
    def invert(self, src: _Any = ...) -> _Any:
        """Matrix.invert compatibility entry (case p0-matrix-invert-2)."""
        ...
    @property
    def is_rectilinear(self) -> _Any:
        """Matrix.is_rectilinear compatibility entry (case p0-matrix-is-rectilinear)."""
        ...
    def norm(self) -> _Any:
        """Matrix.norm compatibility entry (case p0-matrix-norm)."""
        ...
    def prerotate(self, theta: _Any) -> _Any:
        """Matrix.prerotate compatibility entry (case p0-matrix-prerotate)."""
        ...
    def prescale(self, sx: _Any, sy: _Any) -> _Any:
        """Matrix.prescale compatibility entry (case p0-matrix-prescale)."""
        ...
    def preshear(self, h: _Any, v: _Any) -> _Any:
        """Matrix.preshear compatibility entry (case p0-matrix-preshear)."""
        ...
    def pretranslate(self, tx: _Any, ty: _Any) -> _Any:
        """Matrix.pretranslate compatibility entry (case p0-matrix-pretranslate)."""
        ...


class Quad:
    """Quad compatibility surface."""
    def __abs__(self) -> _Any:
        """Quad.__abs__ compatibility entry (case p0-quad-abs)."""
        ...
    def __add__(self, q: _Any) -> _Any:
        """Quad.__add__ compatibility entry (case p0-quad-add)."""
        ...
    def __bool__(self) -> _Any:
        """Quad.__bool__ compatibility entry (case p0-quad-bool)."""
        ...
    def __contains__(self, x: _Any) -> _Any:
        """Quad.__contains__ compatibility entry (case p0-quad-contains)."""
        ...
    def __eq__(self, quad: _Any) -> _Any:
        """Quad.__eq__ compatibility entry (case p0-quad-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Quad.__ge__ compatibility entry (case p0-quad-ge)."""
        ...
    def __getitem__(self, i: _Any) -> _Any:
        """Quad.__getitem__ compatibility entry (case p0-quad-getitem)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Quad.__gt__ compatibility entry (case p0-quad-gt)."""
        ...
    def __init__(self, *args: _Any, ul: _Any = ..., ur: _Any = ..., ll: _Any = ..., lr: _Any = ...) -> None:
        """Quad.__init__ compatibility entry (case p0-quad-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Quad.__le__ compatibility entry (case p0-quad-le)."""
        ...
    def __len__(self) -> _Any:
        """Quad.__len__ compatibility entry (case p0-quad-len)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Quad.__lt__ compatibility entry (case p0-quad-lt)."""
        ...
    def __mul__(self, m: _Any) -> _Any:
        """Quad.__mul__ compatibility entry (case p0-quad-mul)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Quad.__ne__ compatibility entry (case p0-quad-ne)."""
        ...
    def __neg__(self) -> _Any:
        """Quad.__neg__ compatibility entry (case p0-quad-neg)."""
        ...
    def __pos__(self) -> _Any:
        """Quad.__pos__ compatibility entry (case p0-quad-pos)."""
        ...
    def __setitem__(self, i: _Any, v: _Any) -> _Any:
        """Quad.__setitem__ compatibility entry (case p0-quad-setitem)."""
        ...
    def __sub__(self, q: _Any) -> _Any:
        """Quad.__sub__ compatibility entry (case p0-quad-sub)."""
        ...
    def __truediv__(self, m: _Any) -> _Any:
        """Quad.__truediv__ compatibility entry (case p0-quad-truediv)."""
        ...
    @property
    def height(self) -> _Any:
        """Quad.height compatibility entry (case p0-quad-height)."""
        ...
    @property
    def is_convex(self) -> _Any:
        """Quad.is_convex compatibility entry (case p0-quad-is-convex)."""
        ...
    @property
    def is_empty(self) -> _Any:
        """Quad.is_empty compatibility entry (case p0-quad-is-empty)."""
        ...
    @property
    def is_infinite(self) -> _Any:
        """Quad.is_infinite compatibility entry (case p0-quad-is-infinite)."""
        ...
    @property
    def is_rectangular(self) -> _Any:
        """Quad.is_rectangular compatibility entry (case p0-quad-is-rectangular)."""
        ...
    def morph(self, p: _Any, m: _Any) -> _Any:
        """Quad.morph compatibility entry (case p0-quad-morph)."""
        ...
    @property
    def rect(self) -> _Any:
        """Quad.rect compatibility entry (case p0-quad-rect)."""
        ...
    def transform(self, m: _Any) -> _Any:
        """Quad.transform compatibility entry (case p0-quad-transform)."""
        ...
    @property
    def width(self) -> _Any:
        """Quad.width compatibility entry (case p0-quad-width)."""
        ...


class Font:
    """Font compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Font.__eq__ compatibility entry (case l4-font-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Font.__ge__ compatibility entry (case l4-font-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Font.__gt__ compatibility entry (case l4-font-gt)."""
        ...
    def __init__(self, fontname: _Any = ..., fontfile: _Any = ..., fontbuffer: _Any = ..., script: _Any = ..., language: _Any = ..., ordering: _Any = ..., is_bold: _Any = ..., is_italic: _Any = ..., is_serif: _Any = ..., embed: _Any = ...) -> None:
        """Font.__init__ compatibility entry (case l4-font-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Font.__le__ compatibility entry (case l4-font-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Font.__lt__ compatibility entry (case l4-font-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Font.__ne__ compatibility entry (case l4-font-ne)."""
        ...
    @property
    def ascender(self) -> _Any:
        """Font.ascender compatibility entry (case l4-font-properties)."""
        ...
    @property
    def bbox(self) -> _Any:
        """Font.bbox compatibility entry (case l4-font-properties)."""
        ...
    @property
    def buffer(self) -> _Any:
        """Font.buffer compatibility entry (case l4-font-properties)."""
        ...
    def char_lengths(self, text: _Any, fontsize: _Any = ..., language: _Any = ..., script: _Any = ..., wmode: _Any = ..., small_caps: _Any = ...) -> _Any:
        """Font.char_lengths compatibility entry (case l4-font-metrics)."""
        ...
    @property
    def descender(self) -> _Any:
        """Font.descender compatibility entry (case l4-font-properties)."""
        ...
    @property
    def flags(self) -> _Any:
        """Font.flags compatibility entry (case l4-font-properties)."""
        ...
    def glyph_advance(self, chr_: _Any, language: _Any = ..., script: _Any = ..., wmode: _Any = ..., small_caps: _Any = ...) -> _Any:
        """Font.glyph_advance compatibility entry (case l4-font-metrics)."""
        ...
    def glyph_bbox(self, char: _Any, language: _Any = ..., script: _Any = ..., small_caps: _Any = ...) -> _Any:
        """Font.glyph_bbox compatibility entry (case l4-font-metrics)."""
        ...
    @property
    def glyph_count(self) -> _Any:
        """Font.glyph_count compatibility entry (case l4-font-properties)."""
        ...
    def glyph_name_to_unicode(self, name: _Any) -> _Any:
        """Font.glyph_name_to_unicode compatibility entry (case l4-font-metrics)."""
        ...
    def has_glyph(self, chr: _Any, language: _Any = ..., script: _Any = ..., fallback: _Any = ..., small_caps: _Any = ...) -> _Any:
        """Font.has_glyph compatibility entry (case l4-font-metrics)."""
        ...
    @property
    def is_bold(self) -> _Any:
        """Font.is_bold compatibility entry (case l4-font-properties)."""
        ...
    @property
    def is_italic(self) -> _Any:
        """Font.is_italic compatibility entry (case l4-font-properties)."""
        ...
    @property
    def is_monospaced(self) -> _Any:
        """Font.is_monospaced compatibility entry (case l4-font-properties)."""
        ...
    @property
    def is_serif(self) -> _Any:
        """Font.is_serif compatibility entry (case l4-font-properties)."""
        ...
    @property
    def is_writable(self) -> _Any:
        """Font.is_writable compatibility entry (case l4-font-properties)."""
        ...
    @property
    def name(self) -> _Any:
        """Font.name compatibility entry (case l4-font-properties)."""
        ...
    def text_length(self, text: _Any, fontsize: _Any = ..., language: _Any = ..., script: _Any = ..., wmode: _Any = ..., small_caps: _Any = ...) -> _Any:
        """Font.text_length compatibility entry (case l4-font-metrics)."""
        ...
    def unicode_to_glyph_name(self, ch: _Any) -> _Any:
        """Font.unicode_to_glyph_name compatibility entry (case l4-font-metrics)."""
        ...
    def valid_codepoints(self) -> _Any:
        """Font.valid_codepoints compatibility entry (case l4-font-metrics)."""
        ...


class TextWriter:
    """TextWriter compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """TextWriter.__eq__ compatibility entry (case l4-textwriter-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """TextWriter.__ge__ compatibility entry (case l4-textwriter-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """TextWriter.__gt__ compatibility entry (case l4-textwriter-gt)."""
        ...
    def __init__(self, page_rect: _Any, opacity: _Any = ..., color: _Any = ...) -> None:
        """TextWriter.__init__ compatibility entry (case l4-textwriter-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """TextWriter.__le__ compatibility entry (case l4-textwriter-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """TextWriter.__lt__ compatibility entry (case l4-textwriter-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """TextWriter.__ne__ compatibility entry (case l4-textwriter-ne)."""
        ...
    def append(self, pos: _Any, text: _Any, font: _Any = ..., fontsize: _Any = ..., language: _Any = ..., right_to_left: _Any = ..., small_caps: _Any = ...) -> _Any:
        """TextWriter.append compatibility entry (case l4-textwriter-append)."""
        ...
    def appendv(self, pos: _Any, text: _Any, font: _Any = ..., fontsize: _Any = ..., language: _Any = ..., small_caps: _Any = ...) -> _Any:
        """TextWriter.appendv compatibility entry (case l4-textwriter-appendv)."""
        ...
    def clean_rtl(self, text: _Any) -> _Any:
        """TextWriter.clean_rtl compatibility entry (case l4-textwriter-clean-rtl)."""
        ...
    def fill_textbox(self, rect: _Any, text: _Any, pos: _Any = ..., font: _Any = ..., fontsize: _Any = ..., lineheight: _Any = ..., align: _Any = ..., warn: _Any = ..., right_to_left: _Any = ..., small_caps: _Any = ...) -> _Any:
        """TextWriter.fill_textbox compatibility entry (case l4-textwriter-fill-textbox)."""
        ...
    def write_text(self, page: _Any, color: _Any = ..., opacity: _Any = ..., overlay: _Any = ..., morph: _Any = ..., matrix: _Any = ..., render_mode: _Any = ..., oc: _Any = ...) -> _Any:
        """TextWriter.write_text compatibility entry (case l4-textwriter-write-text)."""
        ...


class Colorspace:
    """Colorspace compatibility surface."""
    def __eq__(self, value: _Any) -> _Any:
        """Colorspace.__eq__ compatibility entry (case l3-colorspace-eq)."""
        ...
    def __ge__(self, value: _Any) -> _Any:
        """Colorspace.__ge__ compatibility entry (case l3-colorspace-ge)."""
        ...
    def __gt__(self, value: _Any) -> _Any:
        """Colorspace.__gt__ compatibility entry (case l3-colorspace-gt)."""
        ...
    def __init__(self, type_: _Any) -> None:
        """Colorspace.__init__ compatibility entry (case l3-colorspace-init)."""
        ...
    def __le__(self, value: _Any) -> _Any:
        """Colorspace.__le__ compatibility entry (case l3-colorspace-le)."""
        ...
    def __lt__(self, value: _Any) -> _Any:
        """Colorspace.__lt__ compatibility entry (case l3-colorspace-lt)."""
        ...
    def __ne__(self, value: _Any) -> _Any:
        """Colorspace.__ne__ compatibility entry (case l3-colorspace-ne)."""
        ...
    @property
    def n(self) -> _Any:
        """Colorspace.n compatibility entry (case l3-colorspace-n)."""
        ...
    @property
    def name(self) -> _Any:
        """Colorspace.name compatibility entry (case l3-colorspace-name)."""
        ...


class constants:
    """constants compatibility surface."""
    @property
    def EPSILON(self) -> _Any:
        """constants.EPSILON compatibility entry (case p0-constants-epsilon)."""
        ...
    @property
    def FLT_EPSILON(self) -> _Any:
        """constants.FLT_EPSILON compatibility entry (case p0-constants-flt-epsilon)."""
        ...
    @property
    def FZ_MAX_INF_RECT(self) -> _Any:
        """constants.FZ_MAX_INF_RECT compatibility entry (case p0-constants-fz-max-inf-rect)."""
        ...
    @property
    def FZ_MIN_INF_RECT(self) -> _Any:
        """constants.FZ_MIN_INF_RECT compatibility entry (case p0-constants-fz-min-inf-rect)."""
        ...
    @property
    def Identity(self) -> _Any:
        """constants.Identity compatibility entry (case p0-constants-identity)."""
        ...
    @property
    def TEXT_ACCURATE_ASCENDERS(self) -> _Any:
        """constants.TEXT_ACCURATE_ASCENDERS compatibility entry (case l1-constants-text-accurate-ascenders)."""
        ...
    @property
    def TEXT_ACCURATE_BBOXES(self) -> _Any:
        """constants.TEXT_ACCURATE_BBOXES compatibility entry (case l1-constants-text-accurate-bboxes)."""
        ...
    @property
    def TEXT_ACCURATE_SIDE_BEARINGS(self) -> _Any:
        """constants.TEXT_ACCURATE_SIDE_BEARINGS compatibility entry (case l1-constants-text-accurate-side-bearings)."""
        ...
    @property
    def TEXT_ALIGN_CENTER(self) -> _Any:
        """constants.TEXT_ALIGN_CENTER compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXT_ALIGN_JUSTIFY(self) -> _Any:
        """constants.TEXT_ALIGN_JUSTIFY compatibility entry (case l1-constants-text-align-justify)."""
        ...
    @property
    def TEXT_ALIGN_LEFT(self) -> _Any:
        """constants.TEXT_ALIGN_LEFT compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXT_ALIGN_RIGHT(self) -> _Any:
        """constants.TEXT_ALIGN_RIGHT compatibility entry (case l1-constants-text-align-right)."""
        ...
    @property
    def TEXT_CID_FOR_UNKNOWN_UNICODE(self) -> _Any:
        """constants.TEXT_CID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-cid-for-unknown-unicode)."""
        ...
    @property
    def TEXT_CLIP(self) -> _Any:
        """constants.TEXT_CLIP compatibility entry (case l1-constants-text-clip)."""
        ...
    @property
    def TEXT_CLIP_RECT(self) -> _Any:
        """constants.TEXT_CLIP_RECT compatibility entry (case l1-constants-text-clip-rect)."""
        ...
    @property
    def TEXT_COLLECT_STRUCTURE(self) -> _Any:
        """constants.TEXT_COLLECT_STRUCTURE compatibility entry (case l1-constants-text-collect-structure)."""
        ...
    @property
    def TEXT_COLLECT_STYLES(self) -> _Any:
        """constants.TEXT_COLLECT_STYLES compatibility entry (case l1-constants-text-collect-styles)."""
        ...
    @property
    def TEXT_COLLECT_VECTORS(self) -> _Any:
        """constants.TEXT_COLLECT_VECTORS compatibility entry (case l1-constants-text-collect-vectors)."""
        ...
    @property
    def TEXT_DEHYPHENATE(self) -> _Any:
        """constants.TEXT_DEHYPHENATE compatibility entry (case l1-constants-text-dehyphenate)."""
        ...
    @property
    def TEXT_ENCODING_CYRILLIC(self) -> _Any:
        """constants.TEXT_ENCODING_CYRILLIC compatibility entry (case l1-constants-text-encoding-cyrillic)."""
        ...
    @property
    def TEXT_ENCODING_GREEK(self) -> _Any:
        """constants.TEXT_ENCODING_GREEK compatibility entry (case l1-constants-text-encoding-greek)."""
        ...
    @property
    def TEXT_ENCODING_LATIN(self) -> _Any:
        """constants.TEXT_ENCODING_LATIN compatibility entry (case l1-constants-text-encoding-latin)."""
        ...
    @property
    def TEXT_FONT_BOLD(self) -> _Any:
        """constants.TEXT_FONT_BOLD compatibility entry (case l1-constants-text-font-bold)."""
        ...
    @property
    def TEXT_FONT_ITALIC(self) -> _Any:
        """constants.TEXT_FONT_ITALIC compatibility entry (case l1-constants-text-font-italic)."""
        ...
    @property
    def TEXT_FONT_MONOSPACED(self) -> _Any:
        """constants.TEXT_FONT_MONOSPACED compatibility entry (case l1-constants-text-font-monospaced)."""
        ...
    @property
    def TEXT_FONT_SERIFED(self) -> _Any:
        """constants.TEXT_FONT_SERIFED compatibility entry (case l1-constants-text-font-serifed)."""
        ...
    @property
    def TEXT_FONT_SUPERSCRIPT(self) -> _Any:
        """constants.TEXT_FONT_SUPERSCRIPT compatibility entry (case l1-constants-text-font-superscript)."""
        ...
    @property
    def TEXT_FUZZY_VECTORS(self) -> _Any:
        """constants.TEXT_FUZZY_VECTORS compatibility entry (case l1-constants-text-fuzzy-vectors)."""
        ...
    @property
    def TEXT_IGNORE_ACTUALTEXT(self) -> _Any:
        """constants.TEXT_IGNORE_ACTUALTEXT compatibility entry (case l1-constants-text-ignore-actualtext)."""
        ...
    @property
    def TEXT_INHIBIT_SPACES(self) -> _Any:
        """constants.TEXT_INHIBIT_SPACES compatibility entry (case l1-constants-text-inhibit-spaces)."""
        ...
    @property
    def TEXT_LAZY_VECTORS(self) -> _Any:
        """constants.TEXT_LAZY_VECTORS compatibility entry (case l1-constants-text-lazy-vectors)."""
        ...
    @property
    def TEXT_MEDIABOX_CLIP(self) -> _Any:
        """constants.TEXT_MEDIABOX_CLIP compatibility entry (case l1-constants-text-mediabox-clip)."""
        ...
    @property
    def TEXT_OUTPUT_HTML(self) -> _Any:
        """constants.TEXT_OUTPUT_HTML compatibility entry (case l1-constants-text-output-html)."""
        ...
    @property
    def TEXT_OUTPUT_JSON(self) -> _Any:
        """constants.TEXT_OUTPUT_JSON compatibility entry (case l1-constants-text-output-json)."""
        ...
    @property
    def TEXT_OUTPUT_TEXT(self) -> _Any:
        """constants.TEXT_OUTPUT_TEXT compatibility entry (case l1-constants-text-output-text)."""
        ...
    @property
    def TEXT_OUTPUT_XHTML(self) -> _Any:
        """constants.TEXT_OUTPUT_XHTML compatibility entry (case l1-constants-text-output-xhtml)."""
        ...
    @property
    def TEXT_OUTPUT_XML(self) -> _Any:
        """constants.TEXT_OUTPUT_XML compatibility entry (case l1-constants-text-output-xml)."""
        ...
    @property
    def TEXT_PARAGRAPH_BREAK(self) -> _Any:
        """constants.TEXT_PARAGRAPH_BREAK compatibility entry (case l1-constants-text-paragraph-break)."""
        ...
    @property
    def TEXT_PRESERVE_IMAGES(self) -> _Any:
        """constants.TEXT_PRESERVE_IMAGES compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXT_PRESERVE_LIGATURES(self) -> _Any:
        """constants.TEXT_PRESERVE_LIGATURES compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXT_PRESERVE_SPANS(self) -> _Any:
        """constants.TEXT_PRESERVE_SPANS compatibility entry (case l1-constants-text-preserve-spans)."""
        ...
    @property
    def TEXT_PRESERVE_WHITESPACE(self) -> _Any:
        """constants.TEXT_PRESERVE_WHITESPACE compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXT_SEGMENT(self) -> _Any:
        """constants.TEXT_SEGMENT compatibility entry (case l1-constants-text-segment)."""
        ...
    @property
    def TEXT_STEXT_SEGMENT(self) -> _Any:
        """constants.TEXT_STEXT_SEGMENT compatibility entry (case l1-constants-text-stext-segment)."""
        ...
    @property
    def TEXT_TABLE_HUNT(self) -> _Any:
        """constants.TEXT_TABLE_HUNT compatibility entry (case l1-constants-text-table-hunt)."""
        ...
    @property
    def TEXT_USE_CID_FOR_UNKNOWN_UNICODE(self) -> _Any:
        """constants.TEXT_USE_CID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-use-cid-for-unknown-unicode)."""
        ...
    @property
    def TEXT_USE_GID_FOR_UNKNOWN_UNICODE(self) -> _Any:
        """constants.TEXT_USE_GID_FOR_UNKNOWN_UNICODE compatibility entry (case l1-constants-text-use-gid-for-unknown-unicode)."""
        ...
    @property
    def TEXTFLAGS_BLOCKS(self) -> _Any:
        """constants.TEXTFLAGS_BLOCKS compatibility entry (case l1-constants-textflags-blocks)."""
        ...
    @property
    def TEXTFLAGS_DICT(self) -> _Any:
        """constants.TEXTFLAGS_DICT compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXTFLAGS_HTML(self) -> _Any:
        """constants.TEXTFLAGS_HTML compatibility entry (case l1-constants-textflags-html)."""
        ...
    @property
    def TEXTFLAGS_RAWDICT(self) -> _Any:
        """constants.TEXTFLAGS_RAWDICT compatibility entry (case l1-constants-textflags-rawdict)."""
        ...
    @property
    def TEXTFLAGS_SEARCH(self) -> _Any:
        """constants.TEXTFLAGS_SEARCH compatibility entry (case l1-constants-textflags-search)."""
        ...
    @property
    def TEXTFLAGS_TEXT(self) -> _Any:
        """constants.TEXTFLAGS_TEXT compatibility entry (case l2-constants-all)."""
        ...
    @property
    def TEXTFLAGS_WORDS(self) -> _Any:
        """constants.TEXTFLAGS_WORDS compatibility entry (case l1-constants-textflags-words)."""
        ...
    @property
    def TEXTFLAGS_XHTML(self) -> _Any:
        """constants.TEXTFLAGS_XHTML compatibility entry (case l1-constants-textflags-xhtml)."""
        ...
    @property
    def TEXTFLAGS_XML(self) -> _Any:
        """constants.TEXTFLAGS_XML compatibility entry (case l1-constants-textflags-xml)."""
        ...
    @property
    def dictkey_align(self) -> _Any:
        """constants.dictkey_align compatibility entry (case l1-constants-dictkey-align)."""
        ...
    @property
    def dictkey_asc(self) -> _Any:
        """constants.dictkey_asc compatibility entry (case l1-constants-dictkey-asc)."""
        ...
    @property
    def dictkey_bbox(self) -> _Any:
        """constants.dictkey_bbox compatibility entry (case l1-constants-dictkey-bbox)."""
        ...
    @property
    def dictkey_bidi(self) -> _Any:
        """constants.dictkey_bidi compatibility entry (case l1-constants-dictkey-bidi)."""
        ...
    @property
    def dictkey_blocks(self) -> _Any:
        """constants.dictkey_blocks compatibility entry (case l1-constants-dictkey-blocks)."""
        ...
    @property
    def dictkey_bpc(self) -> _Any:
        """constants.dictkey_bpc compatibility entry (case l1-constants-dictkey-bpc)."""
        ...
    @property
    def dictkey_c(self) -> _Any:
        """constants.dictkey_c compatibility entry (case l1-constants-dictkey-c)."""
        ...
    @property
    def dictkey_char_flags(self) -> _Any:
        """constants.dictkey_char_flags compatibility entry (case l1-constants-dictkey-char-flags)."""
        ...
    @property
    def dictkey_chars(self) -> _Any:
        """constants.dictkey_chars compatibility entry (case l1-constants-dictkey-chars)."""
        ...
    @property
    def dictkey_color(self) -> _Any:
        """constants.dictkey_color compatibility entry (case l1-constants-dictkey-color)."""
        ...
    @property
    def dictkey_colorspace(self) -> _Any:
        """constants.dictkey_colorspace compatibility entry (case l1-constants-dictkey-colorspace)."""
        ...
    @property
    def dictkey_content(self) -> _Any:
        """constants.dictkey_content compatibility entry (case l1-constants-dictkey-content)."""
        ...
    @property
    def dictkey_creationDate(self) -> _Any:
        """constants.dictkey_creationDate compatibility entry (case l1-constants-dictkey-creationdate)."""
        ...
    @property
    def dictkey_cs_name(self) -> _Any:
        """constants.dictkey_cs_name compatibility entry (case l1-constants-dictkey-cs-name)."""
        ...
    @property
    def dictkey_da(self) -> _Any:
        """constants.dictkey_da compatibility entry (case l1-constants-dictkey-da)."""
        ...
    @property
    def dictkey_dashes(self) -> _Any:
        """constants.dictkey_dashes compatibility entry (case l1-constants-dictkey-dashes)."""
        ...
    @property
    def dictkey_desc(self) -> _Any:
        """constants.dictkey_desc compatibility entry (case l1-constants-dictkey-desc)."""
        ...
    @property
    def dictkey_descr(self) -> _Any:
        """constants.dictkey_descr compatibility entry (case l1-constants-dictkey-descr)."""
        ...
    @property
    def dictkey_dir(self) -> _Any:
        """constants.dictkey_dir compatibility entry (case l1-constants-dictkey-dir)."""
        ...
    @property
    def dictkey_effect(self) -> _Any:
        """constants.dictkey_effect compatibility entry (case l1-constants-dictkey-effect)."""
        ...
    @property
    def dictkey_ext(self) -> _Any:
        """constants.dictkey_ext compatibility entry (case l1-constants-dictkey-ext)."""
        ...
    @property
    def dictkey_filename(self) -> _Any:
        """constants.dictkey_filename compatibility entry (case l1-constants-dictkey-filename)."""
        ...
    @property
    def dictkey_fill(self) -> _Any:
        """constants.dictkey_fill compatibility entry (case l1-constants-dictkey-fill)."""
        ...
    @property
    def dictkey_flags(self) -> _Any:
        """constants.dictkey_flags compatibility entry (case l1-constants-dictkey-flags)."""
        ...
    @property
    def dictkey_font(self) -> _Any:
        """constants.dictkey_font compatibility entry (case l1-constants-dictkey-font)."""
        ...
    @property
    def dictkey_glyph(self) -> _Any:
        """constants.dictkey_glyph compatibility entry (case l1-constants-dictkey-glyph)."""
        ...
    @property
    def dictkey_height(self) -> _Any:
        """constants.dictkey_height compatibility entry (case l1-constants-dictkey-height)."""
        ...
    @property
    def dictkey_id(self) -> _Any:
        """constants.dictkey_id compatibility entry (case l1-constants-dictkey-id)."""
        ...
    @property
    def dictkey_image(self) -> _Any:
        """constants.dictkey_image compatibility entry (case l1-constants-dictkey-image)."""
        ...
    @property
    def dictkey_items(self) -> _Any:
        """constants.dictkey_items compatibility entry (case l1-constants-dictkey-items)."""
        ...
    @property
    def dictkey_length(self) -> _Any:
        """constants.dictkey_length compatibility entry (case l1-constants-dictkey-length)."""
        ...
    @property
    def dictkey_lines(self) -> _Any:
        """constants.dictkey_lines compatibility entry (case l1-constants-dictkey-lines)."""
        ...
    @property
    def dictkey_matrix(self) -> _Any:
        """constants.dictkey_matrix compatibility entry (case l1-constants-dictkey-matrix)."""
        ...
    @property
    def dictkey_modDate(self) -> _Any:
        """constants.dictkey_modDate compatibility entry (case l1-constants-dictkey-moddate)."""
        ...
    @property
    def dictkey_name(self) -> _Any:
        """constants.dictkey_name compatibility entry (case l1-constants-dictkey-name)."""
        ...
    @property
    def dictkey_number(self) -> _Any:
        """constants.dictkey_number compatibility entry (case l1-constants-dictkey-number)."""
        ...
    @property
    def dictkey_origin(self) -> _Any:
        """constants.dictkey_origin compatibility entry (case l1-constants-dictkey-origin)."""
        ...
    @property
    def dictkey_rect(self) -> _Any:
        """constants.dictkey_rect compatibility entry (case l1-constants-dictkey-rect)."""
        ...
    @property
    def dictkey_size(self) -> _Any:
        """constants.dictkey_size compatibility entry (case l1-constants-dictkey-size)."""
        ...
    @property
    def dictkey_smask(self) -> _Any:
        """constants.dictkey_smask compatibility entry (case l1-constants-dictkey-smask)."""
        ...
    @property
    def dictkey_spans(self) -> _Any:
        """constants.dictkey_spans compatibility entry (case l1-constants-dictkey-spans)."""
        ...
    @property
    def dictkey_stroke(self) -> _Any:
        """constants.dictkey_stroke compatibility entry (case l1-constants-dictkey-stroke)."""
        ...
    @property
    def dictkey_style(self) -> _Any:
        """constants.dictkey_style compatibility entry (case l1-constants-dictkey-style)."""
        ...
    @property
    def dictkey_subject(self) -> _Any:
        """constants.dictkey_subject compatibility entry (case l1-constants-dictkey-subject)."""
        ...
    @property
    def dictkey_text(self) -> _Any:
        """constants.dictkey_text compatibility entry (case l1-constants-dictkey-text)."""
        ...
    @property
    def dictkey_title(self) -> _Any:
        """constants.dictkey_title compatibility entry (case l1-constants-dictkey-title)."""
        ...
    @property
    def dictkey_type(self) -> _Any:
        """constants.dictkey_type compatibility entry (case l1-constants-dictkey-type)."""
        ...
    @property
    def dictkey_ufilename(self) -> _Any:
        """constants.dictkey_ufilename compatibility entry (case l1-constants-dictkey-ufilename)."""
        ...
    @property
    def dictkey_width(self) -> _Any:
        """constants.dictkey_width compatibility entry (case l1-constants-dictkey-width)."""
        ...
    @property
    def dictkey_wmode(self) -> _Any:
        """constants.dictkey_wmode compatibility entry (case l1-constants-dictkey-wmode)."""
        ...
    @property
    def dictkey_xref(self) -> _Any:
        """constants.dictkey_xref compatibility entry (case l1-constants-dictkey-xref)."""
        ...
    @property
    def dictkey_xres(self) -> _Any:
        """constants.dictkey_xres compatibility entry (case l1-constants-dictkey-xres)."""
        ...
    @property
    def dictkey_yres(self) -> _Any:
        """constants.dictkey_yres compatibility entry (case l1-constants-dictkey-yres)."""
        ...
    @property
    def PDF_STRUCT_BROKEN(self) -> _Any:
        """constants.PDF_STRUCT_BROKEN compatibility entry (case l1-constants-pdf-struct-broken)."""
        ...
    @property
    def PDF_STRUCT_FIXED(self) -> _Any:
        """constants.PDF_STRUCT_FIXED compatibility entry (case l1-constants-pdf-struct-fixed)."""
        ...
    @property
    def PDF_STRUCT_HAS_ATTRIBUTES(self) -> _Any:
        """constants.PDF_STRUCT_HAS_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-attributes)."""
        ...
    @property
    def PDF_STRUCT_HAS_CYCLE(self) -> _Any:
        """constants.PDF_STRUCT_HAS_CYCLE compatibility entry (case l1-constants-pdf-struct-has-cycle)."""
        ...
    @property
    def PDF_STRUCT_HAS_TABLE_ATTRIBUTES(self) -> _Any:
        """constants.PDF_STRUCT_HAS_TABLE_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-table-attributes)."""
        ...
    @property
    def PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES(self) -> _Any:
        """constants.PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES compatibility entry (case l1-constants-pdf-struct-has-table-span-attributes)."""
        ...
    @property
    def PDF_STRUCT_NOT_PRESENT(self) -> _Any:
        """constants.PDF_STRUCT_NOT_PRESENT compatibility entry (case l1-constants-pdf-struct-not-present)."""
        ...
    @property
    def PDF_STRUCT_PRESENT(self) -> _Any:
        """constants.PDF_STRUCT_PRESENT compatibility entry (case l1-constants-pdf-struct-present)."""
        ...
    @property
    def LINK_FLAG_B_VALID(self) -> _Any:
        """constants.LINK_FLAG_B_VALID compatibility entry (case l2-constants-link-flag-b-valid)."""
        ...
    @property
    def LINK_FLAG_FIT_H(self) -> _Any:
        """constants.LINK_FLAG_FIT_H compatibility entry (case l2-constants-link-flag-fit-h)."""
        ...
    @property
    def LINK_FLAG_FIT_V(self) -> _Any:
        """constants.LINK_FLAG_FIT_V compatibility entry (case l2-constants-link-flag-fit-v)."""
        ...
    @property
    def LINK_FLAG_L_VALID(self) -> _Any:
        """constants.LINK_FLAG_L_VALID compatibility entry (case l2-constants-link-flag-l-valid)."""
        ...
    @property
    def LINK_FLAG_R_IS_ZOOM(self) -> _Any:
        """constants.LINK_FLAG_R_IS_ZOOM compatibility entry (case l2-constants-link-flag-r-is-zoom)."""
        ...
    @property
    def LINK_FLAG_R_VALID(self) -> _Any:
        """constants.LINK_FLAG_R_VALID compatibility entry (case l2-constants-link-flag-r-valid)."""
        ...
    @property
    def LINK_FLAG_T_VALID(self) -> _Any:
        """constants.LINK_FLAG_T_VALID compatibility entry (case l2-constants-link-flag-t-valid)."""
        ...
    @property
    def LINK_GOTO(self) -> _Any:
        """constants.LINK_GOTO compatibility entry (case l2-constants-all)."""
        ...
    @property
    def LINK_GOTOR(self) -> _Any:
        """constants.LINK_GOTOR compatibility entry (case l2-constants-link-gotor)."""
        ...
    @property
    def LINK_LAUNCH(self) -> _Any:
        """constants.LINK_LAUNCH compatibility entry (case l2-constants-link-launch)."""
        ...
    @property
    def LINK_NAMED(self) -> _Any:
        """constants.LINK_NAMED compatibility entry (case l2-constants-link-named)."""
        ...
    @property
    def LINK_NONE(self) -> _Any:
        """constants.LINK_NONE compatibility entry (case l2-constants-link-none)."""
        ...
    @property
    def LINK_URI(self) -> _Any:
        """constants.LINK_URI compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_ENCRYPT_AES_128(self) -> _Any:
        """constants.PDF_ENCRYPT_AES_128 compatibility entry (case l2-constants-pdf-encrypt-aes-128)."""
        ...
    @property
    def PDF_ENCRYPT_AES_256(self) -> _Any:
        """constants.PDF_ENCRYPT_AES_256 compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_ENCRYPT_KEEP(self) -> _Any:
        """constants.PDF_ENCRYPT_KEEP compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_ENCRYPT_NONE(self) -> _Any:
        """constants.PDF_ENCRYPT_NONE compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_ENCRYPT_RC4_128(self) -> _Any:
        """constants.PDF_ENCRYPT_RC4_128 compatibility entry (case l2-constants-pdf-encrypt-rc4-128)."""
        ...
    @property
    def PDF_ENCRYPT_RC4_40(self) -> _Any:
        """constants.PDF_ENCRYPT_RC4_40 compatibility entry (case l2-constants-pdf-encrypt-rc4-40)."""
        ...
    @property
    def PDF_ENCRYPT_UNKNOWN(self) -> _Any:
        """constants.PDF_ENCRYPT_UNKNOWN compatibility entry (case l2-constants-pdf-encrypt-unknown)."""
        ...
    @property
    def PDF_PERM_ACCESSIBILITY(self) -> _Any:
        """constants.PDF_PERM_ACCESSIBILITY compatibility entry (case l2-constants-pdf-perm-accessibility)."""
        ...
    @property
    def PDF_PERM_ANNOTATE(self) -> _Any:
        """constants.PDF_PERM_ANNOTATE compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PERM_ASSEMBLE(self) -> _Any:
        """constants.PDF_PERM_ASSEMBLE compatibility entry (case l2-constants-pdf-perm-assemble)."""
        ...
    @property
    def PDF_PERM_COPY(self) -> _Any:
        """constants.PDF_PERM_COPY compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PERM_FORM(self) -> _Any:
        """constants.PDF_PERM_FORM compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PERM_MODIFY(self) -> _Any:
        """constants.PDF_PERM_MODIFY compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PERM_PRINT(self) -> _Any:
        """constants.PDF_PERM_PRINT compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PERM_PRINT_HQ(self) -> _Any:
        """constants.PDF_PERM_PRINT_HQ compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_PAGE_LABEL_ALPHA_LC(self) -> _Any:
        """constants.PDF_PAGE_LABEL_ALPHA_LC compatibility entry (case l2-constants-pdf-page-label-alpha-lc)."""
        ...
    @property
    def PDF_PAGE_LABEL_ALPHA_UC(self) -> _Any:
        """constants.PDF_PAGE_LABEL_ALPHA_UC compatibility entry (case l2-constants-pdf-page-label-alpha-uc)."""
        ...
    @property
    def PDF_PAGE_LABEL_DECIMAL(self) -> _Any:
        """constants.PDF_PAGE_LABEL_DECIMAL compatibility entry (case l2-constants-pdf-page-label-decimal)."""
        ...
    @property
    def PDF_PAGE_LABEL_NONE(self) -> _Any:
        """constants.PDF_PAGE_LABEL_NONE compatibility entry (case l2-constants-pdf-page-label-none)."""
        ...
    @property
    def PDF_PAGE_LABEL_ROMAN_LC(self) -> _Any:
        """constants.PDF_PAGE_LABEL_ROMAN_LC compatibility entry (case l2-constants-pdf-page-label-roman-lc)."""
        ...
    @property
    def PDF_PAGE_LABEL_ROMAN_UC(self) -> _Any:
        """constants.PDF_PAGE_LABEL_ROMAN_UC compatibility entry (case l2-constants-pdf-page-label-roman-uc)."""
        ...
    @property
    def PDF_SIGNATURE_DEFAULT_APPEARANCE(self) -> _Any:
        """constants.PDF_SIGNATURE_DEFAULT_APPEARANCE compatibility entry (case l2-constants-pdf-signature-default-appearance)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_DIGEST_FAILURE(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_DIGEST_FAILURE compatibility entry (case l2-constants-pdf-signature-error-digest-failure)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_NOT_SIGNED(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_NOT_SIGNED compatibility entry (case l2-constants-pdf-signature-error-not-signed)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_NOT_TRUSTED(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_NOT_TRUSTED compatibility entry (case l2-constants-pdf-signature-error-not-trusted)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_NO_CERTIFICATE(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_NO_CERTIFICATE compatibility entry (case l2-constants-pdf-signature-error-no-certificate)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_NO_SIGNATURES(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_NO_SIGNATURES compatibility entry (case l2-constants-pdf-signature-error-no-signatures)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_OKAY(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_OKAY compatibility entry (case l2-constants-pdf-signature-error-okay)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_SELF_SIGNED(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_SELF_SIGNED compatibility entry (case l2-constants-pdf-signature-error-self-signed)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN compatibility entry (case l2-constants-pdf-signature-error-self-signed-in-chain)."""
        ...
    @property
    def PDF_SIGNATURE_ERROR_UNKNOWN(self) -> _Any:
        """constants.PDF_SIGNATURE_ERROR_UNKNOWN compatibility entry (case l2-constants-pdf-signature-error-unknown)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_DATE(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_DATE compatibility entry (case l2-constants-pdf-signature-show-date)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_DN(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_DN compatibility entry (case l2-constants-pdf-signature-show-dn)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_GRAPHIC_NAME(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_GRAPHIC_NAME compatibility entry (case l2-constants-pdf-signature-show-graphic-name)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_LABELS(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_LABELS compatibility entry (case l2-constants-pdf-signature-show-labels)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_LOGO(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_LOGO compatibility entry (case l2-constants-pdf-signature-show-logo)."""
        ...
    @property
    def PDF_SIGNATURE_SHOW_TEXT_NAME(self) -> _Any:
        """constants.PDF_SIGNATURE_SHOW_TEXT_NAME compatibility entry (case l2-constants-pdf-signature-show-text-name)."""
        ...
    @property
    def SigFlag_AppendOnly(self) -> _Any:
        """constants.SigFlag_AppendOnly compatibility entry (case l2-constants-sigflag-appendonly)."""
        ...
    @property
    def SigFlag_SignaturesExist(self) -> _Any:
        """constants.SigFlag_SignaturesExist compatibility entry (case l2-constants-sigflag-signaturesexist)."""
        ...
    @property
    def PDF_ALERT_BUTTON_CANCEL(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_CANCEL compatibility entry (case l2-constants-pdf-alert-button-cancel)."""
        ...
    @property
    def PDF_ALERT_BUTTON_GROUP_OK(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_GROUP_OK compatibility entry (case l2-constants-pdf-alert-button-group-ok)."""
        ...
    @property
    def PDF_ALERT_BUTTON_GROUP_OK_CANCEL(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_GROUP_OK_CANCEL compatibility entry (case l2-constants-pdf-alert-button-group-ok-cancel)."""
        ...
    @property
    def PDF_ALERT_BUTTON_GROUP_YES_NO(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_GROUP_YES_NO compatibility entry (case l2-constants-pdf-alert-button-group-yes-no)."""
        ...
    @property
    def PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL compatibility entry (case l2-constants-pdf-alert-button-group-yes-no-cancel)."""
        ...
    @property
    def PDF_ALERT_BUTTON_NO(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_NO compatibility entry (case l2-constants-pdf-alert-button-no)."""
        ...
    @property
    def PDF_ALERT_BUTTON_NONE(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_NONE compatibility entry (case l2-constants-pdf-alert-button-none)."""
        ...
    @property
    def PDF_ALERT_BUTTON_OK(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_OK compatibility entry (case l2-constants-pdf-alert-button-ok)."""
        ...
    @property
    def PDF_ALERT_BUTTON_YES(self) -> _Any:
        """constants.PDF_ALERT_BUTTON_YES compatibility entry (case l2-constants-pdf-alert-button-yes)."""
        ...
    @property
    def PDF_ALERT_ICON_ERROR(self) -> _Any:
        """constants.PDF_ALERT_ICON_ERROR compatibility entry (case l2-constants-pdf-alert-icon-error)."""
        ...
    @property
    def PDF_ALERT_ICON_QUESTION(self) -> _Any:
        """constants.PDF_ALERT_ICON_QUESTION compatibility entry (case l2-constants-pdf-alert-icon-question)."""
        ...
    @property
    def PDF_ALERT_ICON_STATUS(self) -> _Any:
        """constants.PDF_ALERT_ICON_STATUS compatibility entry (case l2-constants-pdf-alert-icon-status)."""
        ...
    @property
    def PDF_ALERT_ICON_WARNING(self) -> _Any:
        """constants.PDF_ALERT_ICON_WARNING compatibility entry (case l2-constants-pdf-alert-icon-warning)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_ALERT(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_ALERT compatibility entry (case l2-constants-pdf-document-event-alert)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM compatibility entry (case l2-constants-pdf-document-event-exec-menu-item)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_LAUNCH_URL(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_LAUNCH_URL compatibility entry (case l2-constants-pdf-document-event-launch-url)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_MAIL_DOC(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_MAIL_DOC compatibility entry (case l2-constants-pdf-document-event-mail-doc)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_PRINT(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_PRINT compatibility entry (case l2-constants-pdf-document-event-print)."""
        ...
    @property
    def PDF_DOCUMENT_EVENT_SUBMIT(self) -> _Any:
        """constants.PDF_DOCUMENT_EVENT_SUBMIT compatibility entry (case l2-constants-pdf-document-event-submit)."""
        ...
    @property
    def PDF_LAYER_UI_CHECKBOX(self) -> _Any:
        """constants.PDF_LAYER_UI_CHECKBOX compatibility entry (case l2-constants-pdf-layer-ui-checkbox)."""
        ...
    @property
    def PDF_LAYER_UI_LABEL(self) -> _Any:
        """constants.PDF_LAYER_UI_LABEL compatibility entry (case l2-constants-pdf-layer-ui-label)."""
        ...
    @property
    def PDF_LAYER_UI_RADIOBOX(self) -> _Any:
        """constants.PDF_LAYER_UI_RADIOBOX compatibility entry (case l2-constants-pdf-layer-ui-radiobox)."""
        ...
    @property
    def PDF_OC_OFF(self) -> _Any:
        """constants.PDF_OC_OFF compatibility entry (case l2-constants-pdf-oc-off)."""
        ...
    @property
    def PDF_OC_ON(self) -> _Any:
        """constants.PDF_OC_ON compatibility entry (case l2-constants-pdf-oc-on)."""
        ...
    @property
    def PDF_OC_TOGGLE(self) -> _Any:
        """constants.PDF_OC_TOGGLE compatibility entry (case l2-constants-pdf-oc-toggle)."""
        ...
    @property
    def PDF_NOT_ZUGFERD(self) -> _Any:
        """constants.PDF_NOT_ZUGFERD compatibility entry (case l2-constants-pdf-not-zugferd)."""
        ...
    @property
    def PDF_ZUGFERD_BASIC(self) -> _Any:
        """constants.PDF_ZUGFERD_BASIC compatibility entry (case l2-constants-pdf-zugferd-basic)."""
        ...
    @property
    def PDF_ZUGFERD_BASIC_WL(self) -> _Any:
        """constants.PDF_ZUGFERD_BASIC_WL compatibility entry (case l2-constants-pdf-zugferd-basic-wl)."""
        ...
    @property
    def PDF_ZUGFERD_COMFORT(self) -> _Any:
        """constants.PDF_ZUGFERD_COMFORT compatibility entry (case l2-constants-pdf-zugferd-comfort)."""
        ...
    @property
    def PDF_ZUGFERD_EXTENDED(self) -> _Any:
        """constants.PDF_ZUGFERD_EXTENDED compatibility entry (case l2-constants-pdf-zugferd-extended)."""
        ...
    @property
    def PDF_ZUGFERD_MINIMUM(self) -> _Any:
        """constants.PDF_ZUGFERD_MINIMUM compatibility entry (case l2-constants-pdf-zugferd-minimum)."""
        ...
    @property
    def PDF_ZUGFERD_UNKNOWN(self) -> _Any:
        """constants.PDF_ZUGFERD_UNKNOWN compatibility entry (case l2-constants-pdf-zugferd-unknown)."""
        ...
    @property
    def PDF_ZUGFERD_XRECHNUNG(self) -> _Any:
        """constants.PDF_ZUGFERD_XRECHNUNG compatibility entry (case l2-constants-pdf-zugferd-xrechnung)."""
        ...
    @property
    def PDF_CLEAN_STRUCTURE_DROP(self) -> _Any:
        """constants.PDF_CLEAN_STRUCTURE_DROP compatibility entry (case l2-constants-pdf-clean-structure-drop)."""
        ...
    @property
    def PDF_CLEAN_STRUCTURE_KEEP(self) -> _Any:
        """constants.PDF_CLEAN_STRUCTURE_KEEP compatibility entry (case l2-constants-pdf-clean-structure-keep)."""
        ...
    @property
    def PDF_CLEAN_VECTORIZE_NO(self) -> _Any:
        """constants.PDF_CLEAN_VECTORIZE_NO compatibility entry (case l2-constants-pdf-clean-vectorize-no)."""
        ...
    @property
    def PDF_CLEAN_VECTORIZE_YES(self) -> _Any:
        """constants.PDF_CLEAN_VECTORIZE_YES compatibility entry (case l2-constants-pdf-clean-vectorize-yes)."""
        ...
    @property
    def CS_CMYK(self) -> _Any:
        """constants.CS_CMYK compatibility entry (case l3-constants-cs-cmyk)."""
        ...
    @property
    def CS_GRAY(self) -> _Any:
        """constants.CS_GRAY compatibility entry (case l3-constants-cs-gray)."""
        ...
    @property
    def CS_RGB(self) -> _Any:
        """constants.CS_RGB compatibility entry (case l3-constants-cs-rgb)."""
        ...
    @property
    def csCMYK(self) -> _Any:
        """constants.csCMYK compatibility entry (case l3-constants-cscmyk)."""
        ...
    @property
    def csGRAY(self) -> _Any:
        """constants.csGRAY compatibility entry (case l2-constants-all)."""
        ...
    @property
    def csRGB(self) -> _Any:
        """constants.csRGB compatibility entry (case l2-constants-all)."""
        ...
    @property
    def FZ_RECOMPRESS_FAX(self) -> _Any:
        """constants.FZ_RECOMPRESS_FAX compatibility entry (case l3-constants-fz-recompress-fax)."""
        ...
    @property
    def FZ_RECOMPRESS_J2K(self) -> _Any:
        """constants.FZ_RECOMPRESS_J2K compatibility entry (case l3-constants-fz-recompress-j2k)."""
        ...
    @property
    def FZ_RECOMPRESS_JPEG(self) -> _Any:
        """constants.FZ_RECOMPRESS_JPEG compatibility entry (case l3-constants-fz-recompress-jpeg)."""
        ...
    @property
    def FZ_RECOMPRESS_LOSSLESS(self) -> _Any:
        """constants.FZ_RECOMPRESS_LOSSLESS compatibility entry (case l3-constants-fz-recompress-lossless)."""
        ...
    @property
    def FZ_RECOMPRESS_NEVER(self) -> _Any:
        """constants.FZ_RECOMPRESS_NEVER compatibility entry (case l3-constants-fz-recompress-never)."""
        ...
    @property
    def FZ_RECOMPRESS_SAME(self) -> _Any:
        """constants.FZ_RECOMPRESS_SAME compatibility entry (case l3-constants-fz-recompress-same)."""
        ...
    @property
    def FZ_SUBSAMPLE_AVERAGE(self) -> _Any:
        """constants.FZ_SUBSAMPLE_AVERAGE compatibility entry (case l3-constants-fz-subsample-average)."""
        ...
    @property
    def FZ_SUBSAMPLE_BICUBIC(self) -> _Any:
        """constants.FZ_SUBSAMPLE_BICUBIC compatibility entry (case l3-constants-fz-subsample-bicubic)."""
        ...
    @property
    def PDF_ANNOT_3D(self) -> _Any:
        """constants.PDF_ANNOT_3D compatibility entry (case l4-constants-pdf-annot-3d)."""
        ...
    @property
    def PDF_ANNOT_CARET(self) -> _Any:
        """constants.PDF_ANNOT_CARET compatibility entry (case l4-constants-pdf-annot-caret)."""
        ...
    @property
    def PDF_ANNOT_CIRCLE(self) -> _Any:
        """constants.PDF_ANNOT_CIRCLE compatibility entry (case l4-constants-pdf-annot-circle)."""
        ...
    @property
    def PDF_ANNOT_FILE_ATTACHMENT(self) -> _Any:
        """constants.PDF_ANNOT_FILE_ATTACHMENT compatibility entry (case l4-constants-pdf-annot-file-attachment)."""
        ...
    @property
    def PDF_ANNOT_FREE_TEXT(self) -> _Any:
        """constants.PDF_ANNOT_FREE_TEXT compatibility entry (case l4-constants-pdf-annot-free-text)."""
        ...
    @property
    def PDF_ANNOT_HIGHLIGHT(self) -> _Any:
        """constants.PDF_ANNOT_HIGHLIGHT compatibility entry (case l4-constants-pdf-annot-highlight)."""
        ...
    @property
    def PDF_ANNOT_INK(self) -> _Any:
        """constants.PDF_ANNOT_INK compatibility entry (case l4-constants-pdf-annot-ink)."""
        ...
    @property
    def PDF_ANNOT_IS_HIDDEN(self) -> _Any:
        """constants.PDF_ANNOT_IS_HIDDEN compatibility entry (case l4-constants-pdf-annot-is-hidden)."""
        ...
    @property
    def PDF_ANNOT_IS_INVISIBLE(self) -> _Any:
        """constants.PDF_ANNOT_IS_INVISIBLE compatibility entry (case l4-constants-pdf-annot-is-invisible)."""
        ...
    @property
    def PDF_ANNOT_IS_LOCKED(self) -> _Any:
        """constants.PDF_ANNOT_IS_LOCKED compatibility entry (case l4-constants-pdf-annot-is-locked)."""
        ...
    @property
    def PDF_ANNOT_IS_LOCKED_CONTENTS(self) -> _Any:
        """constants.PDF_ANNOT_IS_LOCKED_CONTENTS compatibility entry (case l4-constants-pdf-annot-is-locked-contents)."""
        ...
    @property
    def PDF_ANNOT_IS_NO_ROTATE(self) -> _Any:
        """constants.PDF_ANNOT_IS_NO_ROTATE compatibility entry (case l4-constants-pdf-annot-is-no-rotate)."""
        ...
    @property
    def PDF_ANNOT_IS_NO_VIEW(self) -> _Any:
        """constants.PDF_ANNOT_IS_NO_VIEW compatibility entry (case l4-constants-pdf-annot-is-no-view)."""
        ...
    @property
    def PDF_ANNOT_IS_NO_ZOOM(self) -> _Any:
        """constants.PDF_ANNOT_IS_NO_ZOOM compatibility entry (case l4-constants-pdf-annot-is-no-zoom)."""
        ...
    @property
    def PDF_ANNOT_IS_PRINT(self) -> _Any:
        """constants.PDF_ANNOT_IS_PRINT compatibility entry (case l4-constants-pdf-annot-is-print)."""
        ...
    @property
    def PDF_ANNOT_IS_READ_ONLY(self) -> _Any:
        """constants.PDF_ANNOT_IS_READ_ONLY compatibility entry (case l4-constants-pdf-annot-is-read-only)."""
        ...
    @property
    def PDF_ANNOT_IS_TOGGLE_NO_VIEW(self) -> _Any:
        """constants.PDF_ANNOT_IS_TOGGLE_NO_VIEW compatibility entry (case l4-constants-pdf-annot-is-toggle-no-view)."""
        ...
    @property
    def PDF_ANNOT_IT_DEFAULT(self) -> _Any:
        """constants.PDF_ANNOT_IT_DEFAULT compatibility entry (case l4-constants-pdf-annot-it-default)."""
        ...
    @property
    def PDF_ANNOT_IT_FREETEXT_CALLOUT(self) -> _Any:
        """constants.PDF_ANNOT_IT_FREETEXT_CALLOUT compatibility entry (case l4-constants-pdf-annot-it-freetext-callout)."""
        ...
    @property
    def PDF_ANNOT_IT_FREETEXT_TYPEWRITER(self) -> _Any:
        """constants.PDF_ANNOT_IT_FREETEXT_TYPEWRITER compatibility entry (case l4-constants-pdf-annot-it-freetext-typewriter)."""
        ...
    @property
    def PDF_ANNOT_IT_LINE_ARROW(self) -> _Any:
        """constants.PDF_ANNOT_IT_LINE_ARROW compatibility entry (case l4-constants-pdf-annot-it-line-arrow)."""
        ...
    @property
    def PDF_ANNOT_IT_LINE_DIMENSION(self) -> _Any:
        """constants.PDF_ANNOT_IT_LINE_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-line-dimension)."""
        ...
    @property
    def PDF_ANNOT_IT_POLYGON_CLOUD(self) -> _Any:
        """constants.PDF_ANNOT_IT_POLYGON_CLOUD compatibility entry (case l4-constants-pdf-annot-it-polygon-cloud)."""
        ...
    @property
    def PDF_ANNOT_IT_POLYGON_DIMENSION(self) -> _Any:
        """constants.PDF_ANNOT_IT_POLYGON_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-polygon-dimension)."""
        ...
    @property
    def PDF_ANNOT_IT_POLYLINE_DIMENSION(self) -> _Any:
        """constants.PDF_ANNOT_IT_POLYLINE_DIMENSION compatibility entry (case l4-constants-pdf-annot-it-polyline-dimension)."""
        ...
    @property
    def PDF_ANNOT_IT_STAMP_IMAGE(self) -> _Any:
        """constants.PDF_ANNOT_IT_STAMP_IMAGE compatibility entry (case l4-constants-pdf-annot-it-stamp-image)."""
        ...
    @property
    def PDF_ANNOT_IT_STAMP_SNAPSHOT(self) -> _Any:
        """constants.PDF_ANNOT_IT_STAMP_SNAPSHOT compatibility entry (case l4-constants-pdf-annot-it-stamp-snapshot)."""
        ...
    @property
    def PDF_ANNOT_IT_UNKNOWN(self) -> _Any:
        """constants.PDF_ANNOT_IT_UNKNOWN compatibility entry (case l4-constants-pdf-annot-it-unknown)."""
        ...
    @property
    def PDF_ANNOT_LE_BUTT(self) -> _Any:
        """constants.PDF_ANNOT_LE_BUTT compatibility entry (case l4-constants-pdf-annot-le-butt)."""
        ...
    @property
    def PDF_ANNOT_LE_CIRCLE(self) -> _Any:
        """constants.PDF_ANNOT_LE_CIRCLE compatibility entry (case l4-constants-pdf-annot-le-circle)."""
        ...
    @property
    def PDF_ANNOT_LE_CLOSED_ARROW(self) -> _Any:
        """constants.PDF_ANNOT_LE_CLOSED_ARROW compatibility entry (case l4-constants-pdf-annot-le-closed-arrow)."""
        ...
    @property
    def PDF_ANNOT_LE_DIAMOND(self) -> _Any:
        """constants.PDF_ANNOT_LE_DIAMOND compatibility entry (case l4-constants-pdf-annot-le-diamond)."""
        ...
    @property
    def PDF_ANNOT_LE_NONE(self) -> _Any:
        """constants.PDF_ANNOT_LE_NONE compatibility entry (case l4-constants-pdf-annot-le-none)."""
        ...
    @property
    def PDF_ANNOT_LE_OPEN_ARROW(self) -> _Any:
        """constants.PDF_ANNOT_LE_OPEN_ARROW compatibility entry (case l4-constants-pdf-annot-le-open-arrow)."""
        ...
    @property
    def PDF_ANNOT_LE_R_CLOSED_ARROW(self) -> _Any:
        """constants.PDF_ANNOT_LE_R_CLOSED_ARROW compatibility entry (case l4-constants-pdf-annot-le-r-closed-arrow)."""
        ...
    @property
    def PDF_ANNOT_LE_R_OPEN_ARROW(self) -> _Any:
        """constants.PDF_ANNOT_LE_R_OPEN_ARROW compatibility entry (case l4-constants-pdf-annot-le-r-open-arrow)."""
        ...
    @property
    def PDF_ANNOT_LE_SLASH(self) -> _Any:
        """constants.PDF_ANNOT_LE_SLASH compatibility entry (case l4-constants-pdf-annot-le-slash)."""
        ...
    @property
    def PDF_ANNOT_LE_SQUARE(self) -> _Any:
        """constants.PDF_ANNOT_LE_SQUARE compatibility entry (case l4-constants-pdf-annot-le-square)."""
        ...
    @property
    def PDF_ANNOT_LINE(self) -> _Any:
        """constants.PDF_ANNOT_LINE compatibility entry (case l4-constants-pdf-annot-line)."""
        ...
    @property
    def PDF_ANNOT_LINK(self) -> _Any:
        """constants.PDF_ANNOT_LINK compatibility entry (case l4-constants-pdf-annot-link)."""
        ...
    @property
    def PDF_ANNOT_MOVIE(self) -> _Any:
        """constants.PDF_ANNOT_MOVIE compatibility entry (case l4-constants-pdf-annot-movie)."""
        ...
    @property
    def PDF_ANNOT_POLYGON(self) -> _Any:
        """constants.PDF_ANNOT_POLYGON compatibility entry (case l4-constants-pdf-annot-polygon)."""
        ...
    @property
    def PDF_ANNOT_POLY_LINE(self) -> _Any:
        """constants.PDF_ANNOT_POLY_LINE compatibility entry (case l4-constants-pdf-annot-poly-line)."""
        ...
    @property
    def PDF_ANNOT_POPUP(self) -> _Any:
        """constants.PDF_ANNOT_POPUP compatibility entry (case l4-constants-pdf-annot-popup)."""
        ...
    @property
    def PDF_ANNOT_PRINTER_MARK(self) -> _Any:
        """constants.PDF_ANNOT_PRINTER_MARK compatibility entry (case l4-constants-pdf-annot-printer-mark)."""
        ...
    @property
    def PDF_ANNOT_PROJECTION(self) -> _Any:
        """constants.PDF_ANNOT_PROJECTION compatibility entry (case l4-constants-pdf-annot-projection)."""
        ...
    @property
    def PDF_ANNOT_Q_CENTER(self) -> _Any:
        """constants.PDF_ANNOT_Q_CENTER compatibility entry (case l4-constants-pdf-annot-q-center)."""
        ...
    @property
    def PDF_ANNOT_Q_LEFT(self) -> _Any:
        """constants.PDF_ANNOT_Q_LEFT compatibility entry (case l4-constants-pdf-annot-q-left)."""
        ...
    @property
    def PDF_ANNOT_Q_RIGHT(self) -> _Any:
        """constants.PDF_ANNOT_Q_RIGHT compatibility entry (case l4-constants-pdf-annot-q-right)."""
        ...
    @property
    def PDF_ANNOT_REDACT(self) -> _Any:
        """constants.PDF_ANNOT_REDACT compatibility entry (case l4-constants-pdf-annot-redact)."""
        ...
    @property
    def PDF_ANNOT_RICH_MEDIA(self) -> _Any:
        """constants.PDF_ANNOT_RICH_MEDIA compatibility entry (case l4-constants-pdf-annot-rich-media)."""
        ...
    @property
    def PDF_ANNOT_SCREEN(self) -> _Any:
        """constants.PDF_ANNOT_SCREEN compatibility entry (case l4-constants-pdf-annot-screen)."""
        ...
    @property
    def PDF_ANNOT_SOUND(self) -> _Any:
        """constants.PDF_ANNOT_SOUND compatibility entry (case l4-constants-pdf-annot-sound)."""
        ...
    @property
    def PDF_ANNOT_SQUARE(self) -> _Any:
        """constants.PDF_ANNOT_SQUARE compatibility entry (case l4-constants-pdf-annot-square)."""
        ...
    @property
    def PDF_ANNOT_SQUIGGLY(self) -> _Any:
        """constants.PDF_ANNOT_SQUIGGLY compatibility entry (case l4-constants-pdf-annot-squiggly)."""
        ...
    @property
    def PDF_ANNOT_STAMP(self) -> _Any:
        """constants.PDF_ANNOT_STAMP compatibility entry (case l4-constants-pdf-annot-stamp)."""
        ...
    @property
    def PDF_ANNOT_STRIKE_OUT(self) -> _Any:
        """constants.PDF_ANNOT_STRIKE_OUT compatibility entry (case l4-constants-pdf-annot-strike-out)."""
        ...
    @property
    def PDF_ANNOT_TEXT(self) -> _Any:
        """constants.PDF_ANNOT_TEXT compatibility entry (case l4-constants-pdf-annot-text)."""
        ...
    @property
    def PDF_ANNOT_TRAP_NET(self) -> _Any:
        """constants.PDF_ANNOT_TRAP_NET compatibility entry (case l4-constants-pdf-annot-trap-net)."""
        ...
    @property
    def PDF_ANNOT_UNDERLINE(self) -> _Any:
        """constants.PDF_ANNOT_UNDERLINE compatibility entry (case l4-constants-pdf-annot-underline)."""
        ...
    @property
    def PDF_ANNOT_UNKNOWN(self) -> _Any:
        """constants.PDF_ANNOT_UNKNOWN compatibility entry (case l4-constants-pdf-annot-unknown)."""
        ...
    @property
    def PDF_ANNOT_WATERMARK(self) -> _Any:
        """constants.PDF_ANNOT_WATERMARK compatibility entry (case l4-constants-pdf-annot-watermark)."""
        ...
    @property
    def PDF_ANNOT_WIDGET(self) -> _Any:
        """constants.PDF_ANNOT_WIDGET compatibility entry (case l4-constants-pdf-annot-widget)."""
        ...
    @property
    def PDF_BORDER_EFFECT_CLOUDY(self) -> _Any:
        """constants.PDF_BORDER_EFFECT_CLOUDY compatibility entry (case l4-constants-pdf-border-effect-cloudy)."""
        ...
    @property
    def PDF_BORDER_EFFECT_NONE(self) -> _Any:
        """constants.PDF_BORDER_EFFECT_NONE compatibility entry (case l4-constants-pdf-border-effect-none)."""
        ...
    @property
    def PDF_BORDER_STYLE_BEVELED(self) -> _Any:
        """constants.PDF_BORDER_STYLE_BEVELED compatibility entry (case l4-constants-pdf-border-style-beveled)."""
        ...
    @property
    def PDF_BORDER_STYLE_DASHED(self) -> _Any:
        """constants.PDF_BORDER_STYLE_DASHED compatibility entry (case l4-constants-pdf-border-style-dashed)."""
        ...
    @property
    def PDF_BORDER_STYLE_INSET(self) -> _Any:
        """constants.PDF_BORDER_STYLE_INSET compatibility entry (case l4-constants-pdf-border-style-inset)."""
        ...
    @property
    def PDF_BORDER_STYLE_SOLID(self) -> _Any:
        """constants.PDF_BORDER_STYLE_SOLID compatibility entry (case l4-constants-pdf-border-style-solid)."""
        ...
    @property
    def PDF_BORDER_STYLE_UNDERLINE(self) -> _Any:
        """constants.PDF_BORDER_STYLE_UNDERLINE compatibility entry (case l4-constants-pdf-border-style-underline)."""
        ...
    @property
    def PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF(self) -> _Any:
        """constants.PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF compatibility entry (case l4-constants-pdf-btn-field-is-no-toggle-to-off)."""
        ...
    @property
    def PDF_BTN_FIELD_IS_PUSHBUTTON(self) -> _Any:
        """constants.PDF_BTN_FIELD_IS_PUSHBUTTON compatibility entry (case l4-constants-pdf-btn-field-is-pushbutton)."""
        ...
    @property
    def PDF_BTN_FIELD_IS_RADIO(self) -> _Any:
        """constants.PDF_BTN_FIELD_IS_RADIO compatibility entry (case l4-constants-pdf-btn-field-is-radio)."""
        ...
    @property
    def PDF_BTN_FIELD_IS_RADIOS_IN_UNISON(self) -> _Any:
        """constants.PDF_BTN_FIELD_IS_RADIOS_IN_UNISON compatibility entry (case l4-constants-pdf-btn-field-is-radios-in-unison)."""
        ...
    @property
    def PDF_CH_FIELD_IS_COMBO(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_COMBO compatibility entry (case l4-constants-pdf-ch-field-is-combo)."""
        ...
    @property
    def PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE compatibility entry (case l4-constants-pdf-ch-field-is-commit-on-sel-change)."""
        ...
    @property
    def PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK compatibility entry (case l4-constants-pdf-ch-field-is-do-not-spell-check)."""
        ...
    @property
    def PDF_CH_FIELD_IS_EDIT(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_EDIT compatibility entry (case l4-constants-pdf-ch-field-is-edit)."""
        ...
    @property
    def PDF_CH_FIELD_IS_MULTI_SELECT(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_MULTI_SELECT compatibility entry (case l4-constants-pdf-ch-field-is-multi-select)."""
        ...
    @property
    def PDF_CH_FIELD_IS_SORT(self) -> _Any:
        """constants.PDF_CH_FIELD_IS_SORT compatibility entry (case l4-constants-pdf-ch-field-is-sort)."""
        ...
    @property
    def PDF_FIELD_IS_NO_EXPORT(self) -> _Any:
        """constants.PDF_FIELD_IS_NO_EXPORT compatibility entry (case l4-constants-pdf-field-is-no-export)."""
        ...
    @property
    def PDF_FIELD_IS_READ_ONLY(self) -> _Any:
        """constants.PDF_FIELD_IS_READ_ONLY compatibility entry (case l4-constants-pdf-field-is-read-only)."""
        ...
    @property
    def PDF_FIELD_IS_REQUIRED(self) -> _Any:
        """constants.PDF_FIELD_IS_REQUIRED compatibility entry (case l4-constants-pdf-field-is-required)."""
        ...
    @property
    def PDF_TX_FIELD_IS_COMB(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_COMB compatibility entry (case l4-constants-pdf-tx-field-is-comb)."""
        ...
    @property
    def PDF_TX_FIELD_IS_DO_NOT_SCROLL(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_DO_NOT_SCROLL compatibility entry (case l4-constants-pdf-tx-field-is-do-not-scroll)."""
        ...
    @property
    def PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK compatibility entry (case l4-constants-pdf-tx-field-is-do-not-spell-check)."""
        ...
    @property
    def PDF_TX_FIELD_IS_FILE_SELECT(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_FILE_SELECT compatibility entry (case l4-constants-pdf-tx-field-is-file-select)."""
        ...
    @property
    def PDF_TX_FIELD_IS_MULTILINE(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_MULTILINE compatibility entry (case l4-constants-pdf-tx-field-is-multiline)."""
        ...
    @property
    def PDF_TX_FIELD_IS_PASSWORD(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_PASSWORD compatibility entry (case l4-constants-pdf-tx-field-is-password)."""
        ...
    @property
    def PDF_TX_FIELD_IS_RICH_TEXT(self) -> _Any:
        """constants.PDF_TX_FIELD_IS_RICH_TEXT compatibility entry (case l4-constants-pdf-tx-field-is-rich-text)."""
        ...
    @property
    def PDF_WIDGET_TX_FORMAT_DATE(self) -> _Any:
        """constants.PDF_WIDGET_TX_FORMAT_DATE compatibility entry (case l4-constants-pdf-widget-tx-format-date)."""
        ...
    @property
    def PDF_WIDGET_TX_FORMAT_NONE(self) -> _Any:
        """constants.PDF_WIDGET_TX_FORMAT_NONE compatibility entry (case l4-constants-pdf-widget-tx-format-none)."""
        ...
    @property
    def PDF_WIDGET_TX_FORMAT_NUMBER(self) -> _Any:
        """constants.PDF_WIDGET_TX_FORMAT_NUMBER compatibility entry (case l4-constants-pdf-widget-tx-format-number)."""
        ...
    @property
    def PDF_WIDGET_TX_FORMAT_SPECIAL(self) -> _Any:
        """constants.PDF_WIDGET_TX_FORMAT_SPECIAL compatibility entry (case l4-constants-pdf-widget-tx-format-special)."""
        ...
    @property
    def PDF_WIDGET_TX_FORMAT_TIME(self) -> _Any:
        """constants.PDF_WIDGET_TX_FORMAT_TIME compatibility entry (case l4-constants-pdf-widget-tx-format-time)."""
        ...
    @property
    def PDF_WIDGET_TYPE_BUTTON(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_BUTTON compatibility entry (case l4-constants-pdf-widget-type-button)."""
        ...
    @property
    def PDF_WIDGET_TYPE_CHECKBOX(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_CHECKBOX compatibility entry (case l4-constants-pdf-widget-type-checkbox)."""
        ...
    @property
    def PDF_WIDGET_TYPE_COMBOBOX(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_COMBOBOX compatibility entry (case l4-constants-pdf-widget-type-combobox)."""
        ...
    @property
    def PDF_WIDGET_TYPE_LISTBOX(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_LISTBOX compatibility entry (case l4-constants-pdf-widget-type-listbox)."""
        ...
    @property
    def PDF_WIDGET_TYPE_RADIOBUTTON(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_RADIOBUTTON compatibility entry (case l4-constants-pdf-widget-type-radiobutton)."""
        ...
    @property
    def PDF_WIDGET_TYPE_SIGNATURE(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_SIGNATURE compatibility entry (case l4-constants-pdf-widget-type-signature)."""
        ...
    @property
    def PDF_WIDGET_TYPE_TEXT(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_TEXT compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_WIDGET_TYPE_UNKNOWN(self) -> _Any:
        """constants.PDF_WIDGET_TYPE_UNKNOWN compatibility entry (case l4-constants-pdf-widget-type-unknown)."""
        ...
    @property
    def STAMP_Approved(self) -> _Any:
        """constants.STAMP_Approved compatibility entry (case l4-constants-stamp-approved)."""
        ...
    @property
    def STAMP_AsIs(self) -> _Any:
        """constants.STAMP_AsIs compatibility entry (case l4-constants-stamp-asis)."""
        ...
    @property
    def STAMP_Confidential(self) -> _Any:
        """constants.STAMP_Confidential compatibility entry (case l4-constants-stamp-confidential)."""
        ...
    @property
    def STAMP_Departmental(self) -> _Any:
        """constants.STAMP_Departmental compatibility entry (case l4-constants-stamp-departmental)."""
        ...
    @property
    def STAMP_Draft(self) -> _Any:
        """constants.STAMP_Draft compatibility entry (case l4-constants-stamp-draft)."""
        ...
    @property
    def STAMP_Experimental(self) -> _Any:
        """constants.STAMP_Experimental compatibility entry (case l4-constants-stamp-experimental)."""
        ...
    @property
    def STAMP_Expired(self) -> _Any:
        """constants.STAMP_Expired compatibility entry (case l4-constants-stamp-expired)."""
        ...
    @property
    def STAMP_Final(self) -> _Any:
        """constants.STAMP_Final compatibility entry (case l4-constants-stamp-final)."""
        ...
    @property
    def STAMP_ForComment(self) -> _Any:
        """constants.STAMP_ForComment compatibility entry (case l4-constants-stamp-forcomment)."""
        ...
    @property
    def STAMP_ForPublicRelease(self) -> _Any:
        """constants.STAMP_ForPublicRelease compatibility entry (case l4-constants-stamp-forpublicrelease)."""
        ...
    @property
    def STAMP_NotApproved(self) -> _Any:
        """constants.STAMP_NotApproved compatibility entry (case l4-constants-stamp-notapproved)."""
        ...
    @property
    def STAMP_NotForPublicRelease(self) -> _Any:
        """constants.STAMP_NotForPublicRelease compatibility entry (case l4-constants-stamp-notforpublicrelease)."""
        ...
    @property
    def STAMP_Sold(self) -> _Any:
        """constants.STAMP_Sold compatibility entry (case l4-constants-stamp-sold)."""
        ...
    @property
    def STAMP_TopSecret(self) -> _Any:
        """constants.STAMP_TopSecret compatibility entry (case l4-constants-stamp-topsecret)."""
        ...
    @property
    def PDF_BM_Color(self) -> _Any:
        """constants.PDF_BM_Color compatibility entry (case l4-constants-pdf-bm-color)."""
        ...
    @property
    def PDF_BM_ColorBurn(self) -> _Any:
        """constants.PDF_BM_ColorBurn compatibility entry (case l4-constants-pdf-bm-colorburn)."""
        ...
    @property
    def PDF_BM_ColorDodge(self) -> _Any:
        """constants.PDF_BM_ColorDodge compatibility entry (case l4-constants-pdf-bm-colordodge)."""
        ...
    @property
    def PDF_BM_Darken(self) -> _Any:
        """constants.PDF_BM_Darken compatibility entry (case l4-constants-pdf-bm-darken)."""
        ...
    @property
    def PDF_BM_Difference(self) -> _Any:
        """constants.PDF_BM_Difference compatibility entry (case l4-constants-pdf-bm-difference)."""
        ...
    @property
    def PDF_BM_Exclusion(self) -> _Any:
        """constants.PDF_BM_Exclusion compatibility entry (case l4-constants-pdf-bm-exclusion)."""
        ...
    @property
    def PDF_BM_HardLight(self) -> _Any:
        """constants.PDF_BM_HardLight compatibility entry (case l4-constants-pdf-bm-hardlight)."""
        ...
    @property
    def PDF_BM_Hue(self) -> _Any:
        """constants.PDF_BM_Hue compatibility entry (case l4-constants-pdf-bm-hue)."""
        ...
    @property
    def PDF_BM_Lighten(self) -> _Any:
        """constants.PDF_BM_Lighten compatibility entry (case l4-constants-pdf-bm-lighten)."""
        ...
    @property
    def PDF_BM_Luminosity(self) -> _Any:
        """constants.PDF_BM_Luminosity compatibility entry (case l4-constants-pdf-bm-luminosity)."""
        ...
    @property
    def PDF_BM_Multiply(self) -> _Any:
        """constants.PDF_BM_Multiply compatibility entry (case l4-constants-pdf-bm-multiply)."""
        ...
    @property
    def PDF_BM_Normal(self) -> _Any:
        """constants.PDF_BM_Normal compatibility entry (case l4-constants-pdf-bm-normal)."""
        ...
    @property
    def PDF_BM_Overlay(self) -> _Any:
        """constants.PDF_BM_Overlay compatibility entry (case l4-constants-pdf-bm-overlay)."""
        ...
    @property
    def PDF_BM_Saturation(self) -> _Any:
        """constants.PDF_BM_Saturation compatibility entry (case l4-constants-pdf-bm-saturation)."""
        ...
    @property
    def PDF_BM_Screen(self) -> _Any:
        """constants.PDF_BM_Screen compatibility entry (case l4-constants-pdf-bm-screen)."""
        ...
    @property
    def PDF_BM_SoftLight(self) -> _Any:
        """constants.PDF_BM_SoftLight compatibility entry (case l4-constants-pdf-bm-softlight)."""
        ...
    @property
    def PDF_REDACT_IMAGE_NONE(self) -> _Any:
        """constants.PDF_REDACT_IMAGE_NONE compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_REDACT_IMAGE_PIXELS(self) -> _Any:
        """constants.PDF_REDACT_IMAGE_PIXELS compatibility entry (case l4-constants-pdf-redact-image-pixels)."""
        ...
    @property
    def PDF_REDACT_IMAGE_REMOVE(self) -> _Any:
        """constants.PDF_REDACT_IMAGE_REMOVE compatibility entry (case l4-constants-pdf-redact-image-remove)."""
        ...
    @property
    def PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE(self) -> _Any:
        """constants.PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE compatibility entry (case l4-constants-pdf-redact-image-remove-unless-invisible)."""
        ...
    @property
    def PDF_REDACT_LINE_ART_NONE(self) -> _Any:
        """constants.PDF_REDACT_LINE_ART_NONE compatibility entry (case l2-constants-all)."""
        ...
    @property
    def PDF_REDACT_LINE_ART_REMOVE_IF_COVERED(self) -> _Any:
        """constants.PDF_REDACT_LINE_ART_REMOVE_IF_COVERED compatibility entry (case l4-constants-pdf-redact-line-art-remove-if-covered)."""
        ...
    @property
    def PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED(self) -> _Any:
        """constants.PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED compatibility entry (case l4-constants-pdf-redact-line-art-remove-if-touched)."""
        ...
    @property
    def PDF_REDACT_TEXT_NONE(self) -> _Any:
        """constants.PDF_REDACT_TEXT_NONE compatibility entry (case l4-constants-pdf-redact-text-none)."""
        ...
    @property
    def PDF_REDACT_TEXT_REMOVE(self) -> _Any:
        """constants.PDF_REDACT_TEXT_REMOVE compatibility entry (case l4-constants-pdf-redact-text-remove)."""
        ...
    @property
    def PDF_REDACT_TEXT_REMOVE_INVISIBLE(self) -> _Any:
        """constants.PDF_REDACT_TEXT_REMOVE_INVISIBLE compatibility entry (case l4-constants-pdf-redact-text-remove-invisible)."""
        ...
    @property
    def UCDN_SCRIPT_ADLAM(self) -> _Any:
        """constants.UCDN_SCRIPT_ADLAM compatibility entry (case l4-constants-ucdn-script-adlam)."""
        ...
    @property
    def UCDN_SCRIPT_AHOM(self) -> _Any:
        """constants.UCDN_SCRIPT_AHOM compatibility entry (case l4-constants-ucdn-script-ahom)."""
        ...
    @property
    def UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS(self) -> _Any:
        """constants.UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-anatolian-hieroglyphs)."""
        ...
    @property
    def UCDN_SCRIPT_ARABIC(self) -> _Any:
        """constants.UCDN_SCRIPT_ARABIC compatibility entry (case l4-constants-ucdn-script-arabic)."""
        ...
    @property
    def UCDN_SCRIPT_ARMENIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_ARMENIAN compatibility entry (case l4-constants-ucdn-script-armenian)."""
        ...
    @property
    def UCDN_SCRIPT_AVESTAN(self) -> _Any:
        """constants.UCDN_SCRIPT_AVESTAN compatibility entry (case l4-constants-ucdn-script-avestan)."""
        ...
    @property
    def UCDN_SCRIPT_BALINESE(self) -> _Any:
        """constants.UCDN_SCRIPT_BALINESE compatibility entry (case l4-constants-ucdn-script-balinese)."""
        ...
    @property
    def UCDN_SCRIPT_BAMUM(self) -> _Any:
        """constants.UCDN_SCRIPT_BAMUM compatibility entry (case l4-constants-ucdn-script-bamum)."""
        ...
    @property
    def UCDN_SCRIPT_BASSA_VAH(self) -> _Any:
        """constants.UCDN_SCRIPT_BASSA_VAH compatibility entry (case l4-constants-ucdn-script-bassa-vah)."""
        ...
    @property
    def UCDN_SCRIPT_BATAK(self) -> _Any:
        """constants.UCDN_SCRIPT_BATAK compatibility entry (case l4-constants-ucdn-script-batak)."""
        ...
    @property
    def UCDN_SCRIPT_BENGALI(self) -> _Any:
        """constants.UCDN_SCRIPT_BENGALI compatibility entry (case l4-constants-ucdn-script-bengali)."""
        ...
    @property
    def UCDN_SCRIPT_BHAIKSUKI(self) -> _Any:
        """constants.UCDN_SCRIPT_BHAIKSUKI compatibility entry (case l4-constants-ucdn-script-bhaiksuki)."""
        ...
    @property
    def UCDN_SCRIPT_BOPOMOFO(self) -> _Any:
        """constants.UCDN_SCRIPT_BOPOMOFO compatibility entry (case l4-constants-ucdn-script-bopomofo)."""
        ...
    @property
    def UCDN_SCRIPT_BRAHMI(self) -> _Any:
        """constants.UCDN_SCRIPT_BRAHMI compatibility entry (case l4-constants-ucdn-script-brahmi)."""
        ...
    @property
    def UCDN_SCRIPT_BRAILLE(self) -> _Any:
        """constants.UCDN_SCRIPT_BRAILLE compatibility entry (case l4-constants-ucdn-script-braille)."""
        ...
    @property
    def UCDN_SCRIPT_BUGINESE(self) -> _Any:
        """constants.UCDN_SCRIPT_BUGINESE compatibility entry (case l4-constants-ucdn-script-buginese)."""
        ...
    @property
    def UCDN_SCRIPT_BUHID(self) -> _Any:
        """constants.UCDN_SCRIPT_BUHID compatibility entry (case l4-constants-ucdn-script-buhid)."""
        ...
    @property
    def UCDN_SCRIPT_CANADIAN_ABORIGINAL(self) -> _Any:
        """constants.UCDN_SCRIPT_CANADIAN_ABORIGINAL compatibility entry (case l4-constants-ucdn-script-canadian-aboriginal)."""
        ...
    @property
    def UCDN_SCRIPT_CARIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_CARIAN compatibility entry (case l4-constants-ucdn-script-carian)."""
        ...
    @property
    def UCDN_SCRIPT_CAUCASIAN_ALBANIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_CAUCASIAN_ALBANIAN compatibility entry (case l4-constants-ucdn-script-caucasian-albanian)."""
        ...
    @property
    def UCDN_SCRIPT_CHAKMA(self) -> _Any:
        """constants.UCDN_SCRIPT_CHAKMA compatibility entry (case l4-constants-ucdn-script-chakma)."""
        ...
    @property
    def UCDN_SCRIPT_CHAM(self) -> _Any:
        """constants.UCDN_SCRIPT_CHAM compatibility entry (case l4-constants-ucdn-script-cham)."""
        ...
    @property
    def UCDN_SCRIPT_CHEROKEE(self) -> _Any:
        """constants.UCDN_SCRIPT_CHEROKEE compatibility entry (case l4-constants-ucdn-script-cherokee)."""
        ...
    @property
    def UCDN_SCRIPT_CHORASMIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_CHORASMIAN compatibility entry (case l4-constants-ucdn-script-chorasmian)."""
        ...
    @property
    def UCDN_SCRIPT_COMMON(self) -> _Any:
        """constants.UCDN_SCRIPT_COMMON compatibility entry (case l4-constants-ucdn-script-common)."""
        ...
    @property
    def UCDN_SCRIPT_COPTIC(self) -> _Any:
        """constants.UCDN_SCRIPT_COPTIC compatibility entry (case l4-constants-ucdn-script-coptic)."""
        ...
    @property
    def UCDN_SCRIPT_CUNEIFORM(self) -> _Any:
        """constants.UCDN_SCRIPT_CUNEIFORM compatibility entry (case l4-constants-ucdn-script-cuneiform)."""
        ...
    @property
    def UCDN_SCRIPT_CYPRIOT(self) -> _Any:
        """constants.UCDN_SCRIPT_CYPRIOT compatibility entry (case l4-constants-ucdn-script-cypriot)."""
        ...
    @property
    def UCDN_SCRIPT_CYPRO_MINOAN(self) -> _Any:
        """constants.UCDN_SCRIPT_CYPRO_MINOAN compatibility entry (case l4-constants-ucdn-script-cypro-minoan)."""
        ...
    @property
    def UCDN_SCRIPT_CYRILLIC(self) -> _Any:
        """constants.UCDN_SCRIPT_CYRILLIC compatibility entry (case l4-constants-ucdn-script-cyrillic)."""
        ...
    @property
    def UCDN_SCRIPT_DESERET(self) -> _Any:
        """constants.UCDN_SCRIPT_DESERET compatibility entry (case l4-constants-ucdn-script-deseret)."""
        ...
    @property
    def UCDN_SCRIPT_DEVANAGARI(self) -> _Any:
        """constants.UCDN_SCRIPT_DEVANAGARI compatibility entry (case l4-constants-ucdn-script-devanagari)."""
        ...
    @property
    def UCDN_SCRIPT_DIVES_AKURU(self) -> _Any:
        """constants.UCDN_SCRIPT_DIVES_AKURU compatibility entry (case l4-constants-ucdn-script-dives-akuru)."""
        ...
    @property
    def UCDN_SCRIPT_DOGRA(self) -> _Any:
        """constants.UCDN_SCRIPT_DOGRA compatibility entry (case l4-constants-ucdn-script-dogra)."""
        ...
    @property
    def UCDN_SCRIPT_DUPLOYAN(self) -> _Any:
        """constants.UCDN_SCRIPT_DUPLOYAN compatibility entry (case l4-constants-ucdn-script-duployan)."""
        ...
    @property
    def UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS(self) -> _Any:
        """constants.UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-egyptian-hieroglyphs)."""
        ...
    @property
    def UCDN_SCRIPT_ELBASAN(self) -> _Any:
        """constants.UCDN_SCRIPT_ELBASAN compatibility entry (case l4-constants-ucdn-script-elbasan)."""
        ...
    @property
    def UCDN_SCRIPT_ELYMAIC(self) -> _Any:
        """constants.UCDN_SCRIPT_ELYMAIC compatibility entry (case l4-constants-ucdn-script-elymaic)."""
        ...
    @property
    def UCDN_SCRIPT_ETHIOPIC(self) -> _Any:
        """constants.UCDN_SCRIPT_ETHIOPIC compatibility entry (case l4-constants-ucdn-script-ethiopic)."""
        ...
    @property
    def UCDN_SCRIPT_GARAY(self) -> _Any:
        """constants.UCDN_SCRIPT_GARAY compatibility entry (case l4-constants-ucdn-script-garay)."""
        ...
    @property
    def UCDN_SCRIPT_GEORGIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_GEORGIAN compatibility entry (case l4-constants-ucdn-script-georgian)."""
        ...
    @property
    def UCDN_SCRIPT_GLAGOLITIC(self) -> _Any:
        """constants.UCDN_SCRIPT_GLAGOLITIC compatibility entry (case l4-constants-ucdn-script-glagolitic)."""
        ...
    @property
    def UCDN_SCRIPT_GOTHIC(self) -> _Any:
        """constants.UCDN_SCRIPT_GOTHIC compatibility entry (case l4-constants-ucdn-script-gothic)."""
        ...
    @property
    def UCDN_SCRIPT_GRANTHA(self) -> _Any:
        """constants.UCDN_SCRIPT_GRANTHA compatibility entry (case l4-constants-ucdn-script-grantha)."""
        ...
    @property
    def UCDN_SCRIPT_GREEK(self) -> _Any:
        """constants.UCDN_SCRIPT_GREEK compatibility entry (case l4-constants-ucdn-script-greek)."""
        ...
    @property
    def UCDN_SCRIPT_GUJARATI(self) -> _Any:
        """constants.UCDN_SCRIPT_GUJARATI compatibility entry (case l4-constants-ucdn-script-gujarati)."""
        ...
    @property
    def UCDN_SCRIPT_GUNJALA_GONDI(self) -> _Any:
        """constants.UCDN_SCRIPT_GUNJALA_GONDI compatibility entry (case l4-constants-ucdn-script-gunjala-gondi)."""
        ...
    @property
    def UCDN_SCRIPT_GURMUKHI(self) -> _Any:
        """constants.UCDN_SCRIPT_GURMUKHI compatibility entry (case l4-constants-ucdn-script-gurmukhi)."""
        ...
    @property
    def UCDN_SCRIPT_GURUNG_KHEMA(self) -> _Any:
        """constants.UCDN_SCRIPT_GURUNG_KHEMA compatibility entry (case l4-constants-ucdn-script-gurung-khema)."""
        ...
    @property
    def UCDN_SCRIPT_HAN(self) -> _Any:
        """constants.UCDN_SCRIPT_HAN compatibility entry (case l4-constants-ucdn-script-han)."""
        ...
    @property
    def UCDN_SCRIPT_HANGUL(self) -> _Any:
        """constants.UCDN_SCRIPT_HANGUL compatibility entry (case l4-constants-ucdn-script-hangul)."""
        ...
    @property
    def UCDN_SCRIPT_HANIFI_ROHINGYA(self) -> _Any:
        """constants.UCDN_SCRIPT_HANIFI_ROHINGYA compatibility entry (case l4-constants-ucdn-script-hanifi-rohingya)."""
        ...
    @property
    def UCDN_SCRIPT_HANUNOO(self) -> _Any:
        """constants.UCDN_SCRIPT_HANUNOO compatibility entry (case l4-constants-ucdn-script-hanunoo)."""
        ...
    @property
    def UCDN_SCRIPT_HATRAN(self) -> _Any:
        """constants.UCDN_SCRIPT_HATRAN compatibility entry (case l4-constants-ucdn-script-hatran)."""
        ...
    @property
    def UCDN_SCRIPT_HEBREW(self) -> _Any:
        """constants.UCDN_SCRIPT_HEBREW compatibility entry (case l4-constants-ucdn-script-hebrew)."""
        ...
    @property
    def UCDN_SCRIPT_HIRAGANA(self) -> _Any:
        """constants.UCDN_SCRIPT_HIRAGANA compatibility entry (case l4-constants-ucdn-script-hiragana)."""
        ...
    @property
    def UCDN_SCRIPT_IMPERIAL_ARAMAIC(self) -> _Any:
        """constants.UCDN_SCRIPT_IMPERIAL_ARAMAIC compatibility entry (case l4-constants-ucdn-script-imperial-aramaic)."""
        ...
    @property
    def UCDN_SCRIPT_INHERITED(self) -> _Any:
        """constants.UCDN_SCRIPT_INHERITED compatibility entry (case l4-constants-ucdn-script-inherited)."""
        ...
    @property
    def UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI(self) -> _Any:
        """constants.UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI compatibility entry (case l4-constants-ucdn-script-inscriptional-pahlavi)."""
        ...
    @property
    def UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN compatibility entry (case l4-constants-ucdn-script-inscriptional-parthian)."""
        ...
    @property
    def UCDN_SCRIPT_JAVANESE(self) -> _Any:
        """constants.UCDN_SCRIPT_JAVANESE compatibility entry (case l4-constants-ucdn-script-javanese)."""
        ...
    @property
    def UCDN_SCRIPT_KAITHI(self) -> _Any:
        """constants.UCDN_SCRIPT_KAITHI compatibility entry (case l4-constants-ucdn-script-kaithi)."""
        ...
    @property
    def UCDN_SCRIPT_KANNADA(self) -> _Any:
        """constants.UCDN_SCRIPT_KANNADA compatibility entry (case l4-constants-ucdn-script-kannada)."""
        ...
    @property
    def UCDN_SCRIPT_KATAKANA(self) -> _Any:
        """constants.UCDN_SCRIPT_KATAKANA compatibility entry (case l4-constants-ucdn-script-katakana)."""
        ...
    @property
    def UCDN_SCRIPT_KAWI(self) -> _Any:
        """constants.UCDN_SCRIPT_KAWI compatibility entry (case l4-constants-ucdn-script-kawi)."""
        ...
    @property
    def UCDN_SCRIPT_KAYAH_LI(self) -> _Any:
        """constants.UCDN_SCRIPT_KAYAH_LI compatibility entry (case l4-constants-ucdn-script-kayah-li)."""
        ...
    @property
    def UCDN_SCRIPT_KHAROSHTHI(self) -> _Any:
        """constants.UCDN_SCRIPT_KHAROSHTHI compatibility entry (case l4-constants-ucdn-script-kharoshthi)."""
        ...
    @property
    def UCDN_SCRIPT_KHITAN_SMALL_SCRIPT(self) -> _Any:
        """constants.UCDN_SCRIPT_KHITAN_SMALL_SCRIPT compatibility entry (case l4-constants-ucdn-script-khitan-small-script)."""
        ...
    @property
    def UCDN_SCRIPT_KHMER(self) -> _Any:
        """constants.UCDN_SCRIPT_KHMER compatibility entry (case l4-constants-ucdn-script-khmer)."""
        ...
    @property
    def UCDN_SCRIPT_KHOJKI(self) -> _Any:
        """constants.UCDN_SCRIPT_KHOJKI compatibility entry (case l4-constants-ucdn-script-khojki)."""
        ...
    @property
    def UCDN_SCRIPT_KHUDAWADI(self) -> _Any:
        """constants.UCDN_SCRIPT_KHUDAWADI compatibility entry (case l4-constants-ucdn-script-khudawadi)."""
        ...
    @property
    def UCDN_SCRIPT_KIRAT_RAI(self) -> _Any:
        """constants.UCDN_SCRIPT_KIRAT_RAI compatibility entry (case l4-constants-ucdn-script-kirat-rai)."""
        ...
    @property
    def UCDN_SCRIPT_LAO(self) -> _Any:
        """constants.UCDN_SCRIPT_LAO compatibility entry (case l4-constants-ucdn-script-lao)."""
        ...
    @property
    def UCDN_SCRIPT_LATIN(self) -> _Any:
        """constants.UCDN_SCRIPT_LATIN compatibility entry (case l4-constants-ucdn-script-latin)."""
        ...
    @property
    def UCDN_SCRIPT_LEPCHA(self) -> _Any:
        """constants.UCDN_SCRIPT_LEPCHA compatibility entry (case l4-constants-ucdn-script-lepcha)."""
        ...
    @property
    def UCDN_SCRIPT_LIMBU(self) -> _Any:
        """constants.UCDN_SCRIPT_LIMBU compatibility entry (case l4-constants-ucdn-script-limbu)."""
        ...
    @property
    def UCDN_SCRIPT_LINEAR_A(self) -> _Any:
        """constants.UCDN_SCRIPT_LINEAR_A compatibility entry (case l4-constants-ucdn-script-linear-a)."""
        ...
    @property
    def UCDN_SCRIPT_LINEAR_B(self) -> _Any:
        """constants.UCDN_SCRIPT_LINEAR_B compatibility entry (case l4-constants-ucdn-script-linear-b)."""
        ...
    @property
    def UCDN_SCRIPT_LISU(self) -> _Any:
        """constants.UCDN_SCRIPT_LISU compatibility entry (case l4-constants-ucdn-script-lisu)."""
        ...
    @property
    def UCDN_SCRIPT_LYCIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_LYCIAN compatibility entry (case l4-constants-ucdn-script-lycian)."""
        ...
    @property
    def UCDN_SCRIPT_LYDIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_LYDIAN compatibility entry (case l4-constants-ucdn-script-lydian)."""
        ...
    @property
    def UCDN_SCRIPT_MAHAJANI(self) -> _Any:
        """constants.UCDN_SCRIPT_MAHAJANI compatibility entry (case l4-constants-ucdn-script-mahajani)."""
        ...
    @property
    def UCDN_SCRIPT_MAKASAR(self) -> _Any:
        """constants.UCDN_SCRIPT_MAKASAR compatibility entry (case l4-constants-ucdn-script-makasar)."""
        ...
    @property
    def UCDN_SCRIPT_MALAYALAM(self) -> _Any:
        """constants.UCDN_SCRIPT_MALAYALAM compatibility entry (case l4-constants-ucdn-script-malayalam)."""
        ...
    @property
    def UCDN_SCRIPT_MANDAIC(self) -> _Any:
        """constants.UCDN_SCRIPT_MANDAIC compatibility entry (case l4-constants-ucdn-script-mandaic)."""
        ...
    @property
    def UCDN_SCRIPT_MANICHAEAN(self) -> _Any:
        """constants.UCDN_SCRIPT_MANICHAEAN compatibility entry (case l4-constants-ucdn-script-manichaean)."""
        ...
    @property
    def UCDN_SCRIPT_MARCHEN(self) -> _Any:
        """constants.UCDN_SCRIPT_MARCHEN compatibility entry (case l4-constants-ucdn-script-marchen)."""
        ...
    @property
    def UCDN_SCRIPT_MASARAM_GONDI(self) -> _Any:
        """constants.UCDN_SCRIPT_MASARAM_GONDI compatibility entry (case l4-constants-ucdn-script-masaram-gondi)."""
        ...
    @property
    def UCDN_SCRIPT_MEDEFAIDRIN(self) -> _Any:
        """constants.UCDN_SCRIPT_MEDEFAIDRIN compatibility entry (case l4-constants-ucdn-script-medefaidrin)."""
        ...
    @property
    def UCDN_SCRIPT_MEETEI_MAYEK(self) -> _Any:
        """constants.UCDN_SCRIPT_MEETEI_MAYEK compatibility entry (case l4-constants-ucdn-script-meetei-mayek)."""
        ...
    @property
    def UCDN_SCRIPT_MENDE_KIKAKUI(self) -> _Any:
        """constants.UCDN_SCRIPT_MENDE_KIKAKUI compatibility entry (case l4-constants-ucdn-script-mende-kikakui)."""
        ...
    @property
    def UCDN_SCRIPT_MEROITIC_CURSIVE(self) -> _Any:
        """constants.UCDN_SCRIPT_MEROITIC_CURSIVE compatibility entry (case l4-constants-ucdn-script-meroitic-cursive)."""
        ...
    @property
    def UCDN_SCRIPT_MEROITIC_HIEROGLYPHS(self) -> _Any:
        """constants.UCDN_SCRIPT_MEROITIC_HIEROGLYPHS compatibility entry (case l4-constants-ucdn-script-meroitic-hieroglyphs)."""
        ...
    @property
    def UCDN_SCRIPT_MIAO(self) -> _Any:
        """constants.UCDN_SCRIPT_MIAO compatibility entry (case l4-constants-ucdn-script-miao)."""
        ...
    @property
    def UCDN_SCRIPT_MODI(self) -> _Any:
        """constants.UCDN_SCRIPT_MODI compatibility entry (case l4-constants-ucdn-script-modi)."""
        ...
    @property
    def UCDN_SCRIPT_MONGOLIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_MONGOLIAN compatibility entry (case l4-constants-ucdn-script-mongolian)."""
        ...
    @property
    def UCDN_SCRIPT_MRO(self) -> _Any:
        """constants.UCDN_SCRIPT_MRO compatibility entry (case l4-constants-ucdn-script-mro)."""
        ...
    @property
    def UCDN_SCRIPT_MULTANI(self) -> _Any:
        """constants.UCDN_SCRIPT_MULTANI compatibility entry (case l4-constants-ucdn-script-multani)."""
        ...
    @property
    def UCDN_SCRIPT_MYANMAR(self) -> _Any:
        """constants.UCDN_SCRIPT_MYANMAR compatibility entry (case l4-constants-ucdn-script-myanmar)."""
        ...
    @property
    def UCDN_SCRIPT_NABATAEAN(self) -> _Any:
        """constants.UCDN_SCRIPT_NABATAEAN compatibility entry (case l4-constants-ucdn-script-nabataean)."""
        ...
    @property
    def UCDN_SCRIPT_NAG_MUNDARI(self) -> _Any:
        """constants.UCDN_SCRIPT_NAG_MUNDARI compatibility entry (case l4-constants-ucdn-script-nag-mundari)."""
        ...
    @property
    def UCDN_SCRIPT_NANDINAGARI(self) -> _Any:
        """constants.UCDN_SCRIPT_NANDINAGARI compatibility entry (case l4-constants-ucdn-script-nandinagari)."""
        ...
    @property
    def UCDN_SCRIPT_NEWA(self) -> _Any:
        """constants.UCDN_SCRIPT_NEWA compatibility entry (case l4-constants-ucdn-script-newa)."""
        ...
    @property
    def UCDN_SCRIPT_NEW_TAI_LUE(self) -> _Any:
        """constants.UCDN_SCRIPT_NEW_TAI_LUE compatibility entry (case l4-constants-ucdn-script-new-tai-lue)."""
        ...
    @property
    def UCDN_SCRIPT_NKO(self) -> _Any:
        """constants.UCDN_SCRIPT_NKO compatibility entry (case l4-constants-ucdn-script-nko)."""
        ...
    @property
    def UCDN_SCRIPT_NUSHU(self) -> _Any:
        """constants.UCDN_SCRIPT_NUSHU compatibility entry (case l4-constants-ucdn-script-nushu)."""
        ...
    @property
    def UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG(self) -> _Any:
        """constants.UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG compatibility entry (case l4-constants-ucdn-script-nyiakeng-puachue-hmong)."""
        ...
    @property
    def UCDN_SCRIPT_OGHAM(self) -> _Any:
        """constants.UCDN_SCRIPT_OGHAM compatibility entry (case l4-constants-ucdn-script-ogham)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_HUNGARIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_HUNGARIAN compatibility entry (case l4-constants-ucdn-script-old-hungarian)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_ITALIC(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_ITALIC compatibility entry (case l4-constants-ucdn-script-old-italic)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_NORTH_ARABIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_NORTH_ARABIAN compatibility entry (case l4-constants-ucdn-script-old-north-arabian)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_PERMIC(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_PERMIC compatibility entry (case l4-constants-ucdn-script-old-permic)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_PERSIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_PERSIAN compatibility entry (case l4-constants-ucdn-script-old-persian)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_SOGDIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_SOGDIAN compatibility entry (case l4-constants-ucdn-script-old-sogdian)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_SOUTH_ARABIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_SOUTH_ARABIAN compatibility entry (case l4-constants-ucdn-script-old-south-arabian)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_TURKIC(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_TURKIC compatibility entry (case l4-constants-ucdn-script-old-turkic)."""
        ...
    @property
    def UCDN_SCRIPT_OLD_UYGHUR(self) -> _Any:
        """constants.UCDN_SCRIPT_OLD_UYGHUR compatibility entry (case l4-constants-ucdn-script-old-uyghur)."""
        ...
    @property
    def UCDN_SCRIPT_OL_CHIKI(self) -> _Any:
        """constants.UCDN_SCRIPT_OL_CHIKI compatibility entry (case l4-constants-ucdn-script-ol-chiki)."""
        ...
    @property
    def UCDN_SCRIPT_OL_ONAL(self) -> _Any:
        """constants.UCDN_SCRIPT_OL_ONAL compatibility entry (case l4-constants-ucdn-script-ol-onal)."""
        ...
    @property
    def UCDN_SCRIPT_ORIYA(self) -> _Any:
        """constants.UCDN_SCRIPT_ORIYA compatibility entry (case l4-constants-ucdn-script-oriya)."""
        ...
    @property
    def UCDN_SCRIPT_OSAGE(self) -> _Any:
        """constants.UCDN_SCRIPT_OSAGE compatibility entry (case l4-constants-ucdn-script-osage)."""
        ...
    @property
    def UCDN_SCRIPT_OSMANYA(self) -> _Any:
        """constants.UCDN_SCRIPT_OSMANYA compatibility entry (case l4-constants-ucdn-script-osmanya)."""
        ...
    @property
    def UCDN_SCRIPT_PAHAWH_HMONG(self) -> _Any:
        """constants.UCDN_SCRIPT_PAHAWH_HMONG compatibility entry (case l4-constants-ucdn-script-pahawh-hmong)."""
        ...
    @property
    def UCDN_SCRIPT_PALMYRENE(self) -> _Any:
        """constants.UCDN_SCRIPT_PALMYRENE compatibility entry (case l4-constants-ucdn-script-palmyrene)."""
        ...
    @property
    def UCDN_SCRIPT_PAU_CIN_HAU(self) -> _Any:
        """constants.UCDN_SCRIPT_PAU_CIN_HAU compatibility entry (case l4-constants-ucdn-script-pau-cin-hau)."""
        ...
    @property
    def UCDN_SCRIPT_PHAGS_PA(self) -> _Any:
        """constants.UCDN_SCRIPT_PHAGS_PA compatibility entry (case l4-constants-ucdn-script-phags-pa)."""
        ...
    @property
    def UCDN_SCRIPT_PHOENICIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_PHOENICIAN compatibility entry (case l4-constants-ucdn-script-phoenician)."""
        ...
    @property
    def UCDN_SCRIPT_PSALTER_PAHLAVI(self) -> _Any:
        """constants.UCDN_SCRIPT_PSALTER_PAHLAVI compatibility entry (case l4-constants-ucdn-script-psalter-pahlavi)."""
        ...
    @property
    def UCDN_SCRIPT_REJANG(self) -> _Any:
        """constants.UCDN_SCRIPT_REJANG compatibility entry (case l4-constants-ucdn-script-rejang)."""
        ...
    @property
    def UCDN_SCRIPT_RUNIC(self) -> _Any:
        """constants.UCDN_SCRIPT_RUNIC compatibility entry (case l4-constants-ucdn-script-runic)."""
        ...
    @property
    def UCDN_SCRIPT_SAMARITAN(self) -> _Any:
        """constants.UCDN_SCRIPT_SAMARITAN compatibility entry (case l4-constants-ucdn-script-samaritan)."""
        ...
    @property
    def UCDN_SCRIPT_SAURASHTRA(self) -> _Any:
        """constants.UCDN_SCRIPT_SAURASHTRA compatibility entry (case l4-constants-ucdn-script-saurashtra)."""
        ...
    @property
    def UCDN_SCRIPT_SHARADA(self) -> _Any:
        """constants.UCDN_SCRIPT_SHARADA compatibility entry (case l4-constants-ucdn-script-sharada)."""
        ...
    @property
    def UCDN_SCRIPT_SHAVIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_SHAVIAN compatibility entry (case l4-constants-ucdn-script-shavian)."""
        ...
    @property
    def UCDN_SCRIPT_SIDDHAM(self) -> _Any:
        """constants.UCDN_SCRIPT_SIDDHAM compatibility entry (case l4-constants-ucdn-script-siddham)."""
        ...
    @property
    def UCDN_SCRIPT_SIGNWRITING(self) -> _Any:
        """constants.UCDN_SCRIPT_SIGNWRITING compatibility entry (case l4-constants-ucdn-script-signwriting)."""
        ...
    @property
    def UCDN_SCRIPT_SINHALA(self) -> _Any:
        """constants.UCDN_SCRIPT_SINHALA compatibility entry (case l4-constants-ucdn-script-sinhala)."""
        ...
    @property
    def UCDN_SCRIPT_SOGDIAN(self) -> _Any:
        """constants.UCDN_SCRIPT_SOGDIAN compatibility entry (case l4-constants-ucdn-script-sogdian)."""
        ...
    @property
    def UCDN_SCRIPT_SORA_SOMPENG(self) -> _Any:
        """constants.UCDN_SCRIPT_SORA_SOMPENG compatibility entry (case l4-constants-ucdn-script-sora-sompeng)."""
        ...
    @property
    def UCDN_SCRIPT_SOYOMBO(self) -> _Any:
        """constants.UCDN_SCRIPT_SOYOMBO compatibility entry (case l4-constants-ucdn-script-soyombo)."""
        ...
    @property
    def UCDN_SCRIPT_SUNDANESE(self) -> _Any:
        """constants.UCDN_SCRIPT_SUNDANESE compatibility entry (case l4-constants-ucdn-script-sundanese)."""
        ...
    @property
    def UCDN_SCRIPT_SUNUWAR(self) -> _Any:
        """constants.UCDN_SCRIPT_SUNUWAR compatibility entry (case l4-constants-ucdn-script-sunuwar)."""
        ...
    @property
    def UCDN_SCRIPT_SYLOTI_NAGRI(self) -> _Any:
        """constants.UCDN_SCRIPT_SYLOTI_NAGRI compatibility entry (case l4-constants-ucdn-script-syloti-nagri)."""
        ...
    @property
    def UCDN_SCRIPT_SYRIAC(self) -> _Any:
        """constants.UCDN_SCRIPT_SYRIAC compatibility entry (case l4-constants-ucdn-script-syriac)."""
        ...
    @property
    def UCDN_SCRIPT_TAGALOG(self) -> _Any:
        """constants.UCDN_SCRIPT_TAGALOG compatibility entry (case l4-constants-ucdn-script-tagalog)."""
        ...
    @property
    def UCDN_SCRIPT_TAGBANWA(self) -> _Any:
        """constants.UCDN_SCRIPT_TAGBANWA compatibility entry (case l4-constants-ucdn-script-tagbanwa)."""
        ...
    @property
    def UCDN_SCRIPT_TAI_LE(self) -> _Any:
        """constants.UCDN_SCRIPT_TAI_LE compatibility entry (case l4-constants-ucdn-script-tai-le)."""
        ...
    @property
    def UCDN_SCRIPT_TAI_THAM(self) -> _Any:
        """constants.UCDN_SCRIPT_TAI_THAM compatibility entry (case l4-constants-ucdn-script-tai-tham)."""
        ...
    @property
    def UCDN_SCRIPT_TAI_VIET(self) -> _Any:
        """constants.UCDN_SCRIPT_TAI_VIET compatibility entry (case l4-constants-ucdn-script-tai-viet)."""
        ...
    @property
    def UCDN_SCRIPT_TAKRI(self) -> _Any:
        """constants.UCDN_SCRIPT_TAKRI compatibility entry (case l4-constants-ucdn-script-takri)."""
        ...
    @property
    def UCDN_SCRIPT_TAMIL(self) -> _Any:
        """constants.UCDN_SCRIPT_TAMIL compatibility entry (case l4-constants-ucdn-script-tamil)."""
        ...
    @property
    def UCDN_SCRIPT_TANGSA(self) -> _Any:
        """constants.UCDN_SCRIPT_TANGSA compatibility entry (case l4-constants-ucdn-script-tangsa)."""
        ...
    @property
    def UCDN_SCRIPT_TANGUT(self) -> _Any:
        """constants.UCDN_SCRIPT_TANGUT compatibility entry (case l4-constants-ucdn-script-tangut)."""
        ...
    @property
    def UCDN_SCRIPT_TELUGU(self) -> _Any:
        """constants.UCDN_SCRIPT_TELUGU compatibility entry (case l4-constants-ucdn-script-telugu)."""
        ...
    @property
    def UCDN_SCRIPT_THAANA(self) -> _Any:
        """constants.UCDN_SCRIPT_THAANA compatibility entry (case l4-constants-ucdn-script-thaana)."""
        ...
    @property
    def UCDN_SCRIPT_THAI(self) -> _Any:
        """constants.UCDN_SCRIPT_THAI compatibility entry (case l4-constants-ucdn-script-thai)."""
        ...
    @property
    def UCDN_SCRIPT_TIBETAN(self) -> _Any:
        """constants.UCDN_SCRIPT_TIBETAN compatibility entry (case l4-constants-ucdn-script-tibetan)."""
        ...
    @property
    def UCDN_SCRIPT_TIFINAGH(self) -> _Any:
        """constants.UCDN_SCRIPT_TIFINAGH compatibility entry (case l4-constants-ucdn-script-tifinagh)."""
        ...
    @property
    def UCDN_SCRIPT_TIRHUTA(self) -> _Any:
        """constants.UCDN_SCRIPT_TIRHUTA compatibility entry (case l4-constants-ucdn-script-tirhuta)."""
        ...
    @property
    def UCDN_SCRIPT_TODHRI(self) -> _Any:
        """constants.UCDN_SCRIPT_TODHRI compatibility entry (case l4-constants-ucdn-script-todhri)."""
        ...
    @property
    def UCDN_SCRIPT_TOTO(self) -> _Any:
        """constants.UCDN_SCRIPT_TOTO compatibility entry (case l4-constants-ucdn-script-toto)."""
        ...
    @property
    def UCDN_SCRIPT_TULU_TIGALARI(self) -> _Any:
        """constants.UCDN_SCRIPT_TULU_TIGALARI compatibility entry (case l4-constants-ucdn-script-tulu-tigalari)."""
        ...
    @property
    def UCDN_SCRIPT_UGARITIC(self) -> _Any:
        """constants.UCDN_SCRIPT_UGARITIC compatibility entry (case l4-constants-ucdn-script-ugaritic)."""
        ...
    @property
    def UCDN_SCRIPT_UNKNOWN(self) -> _Any:
        """constants.UCDN_SCRIPT_UNKNOWN compatibility entry (case l4-constants-ucdn-script-unknown)."""
        ...
    @property
    def UCDN_SCRIPT_VAI(self) -> _Any:
        """constants.UCDN_SCRIPT_VAI compatibility entry (case l4-constants-ucdn-script-vai)."""
        ...
    @property
    def UCDN_SCRIPT_VITHKUQI(self) -> _Any:
        """constants.UCDN_SCRIPT_VITHKUQI compatibility entry (case l4-constants-ucdn-script-vithkuqi)."""
        ...
    @property
    def UCDN_SCRIPT_WANCHO(self) -> _Any:
        """constants.UCDN_SCRIPT_WANCHO compatibility entry (case l4-constants-ucdn-script-wancho)."""
        ...
    @property
    def UCDN_SCRIPT_WARANG_CITI(self) -> _Any:
        """constants.UCDN_SCRIPT_WARANG_CITI compatibility entry (case l4-constants-ucdn-script-warang-citi)."""
        ...
    @property
    def UCDN_SCRIPT_YEZIDI(self) -> _Any:
        """constants.UCDN_SCRIPT_YEZIDI compatibility entry (case l4-constants-ucdn-script-yezidi)."""
        ...
    @property
    def UCDN_SCRIPT_YI(self) -> _Any:
        """constants.UCDN_SCRIPT_YI compatibility entry (case l4-constants-ucdn-script-yi)."""
        ...
    @property
    def UCDN_SCRIPT_ZANABAZAR_SQUARE(self) -> _Any:
        """constants.UCDN_SCRIPT_ZANABAZAR_SQUARE compatibility entry (case l4-constants-ucdn-script-zanabazar-square)."""
        ...
    @property
    def PDF_CID_FONT_RESOURCE(self) -> _Any:
        """constants.PDF_CID_FONT_RESOURCE compatibility entry (case l4-constants-pdf-cid-font-resource)."""
        ...
    @property
    def PDF_CJK_FONT_RESOURCE(self) -> _Any:
        """constants.PDF_CJK_FONT_RESOURCE compatibility entry (case l4-constants-pdf-cjk-font-resource)."""
        ...
    @property
    def PDF_FD_ALL_CAP(self) -> _Any:
        """constants.PDF_FD_ALL_CAP compatibility entry (case l4-constants-pdf-fd-all-cap)."""
        ...
    @property
    def PDF_FD_FIXED_PITCH(self) -> _Any:
        """constants.PDF_FD_FIXED_PITCH compatibility entry (case l4-constants-pdf-fd-fixed-pitch)."""
        ...
    @property
    def PDF_FD_FORCE_BOLD(self) -> _Any:
        """constants.PDF_FD_FORCE_BOLD compatibility entry (case l4-constants-pdf-fd-force-bold)."""
        ...
    @property
    def PDF_FD_ITALIC(self) -> _Any:
        """constants.PDF_FD_ITALIC compatibility entry (case l4-constants-pdf-fd-italic)."""
        ...
    @property
    def PDF_FD_NONSYMBOLIC(self) -> _Any:
        """constants.PDF_FD_NONSYMBOLIC compatibility entry (case l4-constants-pdf-fd-nonsymbolic)."""
        ...
    @property
    def PDF_FD_SCRIPT(self) -> _Any:
        """constants.PDF_FD_SCRIPT compatibility entry (case l4-constants-pdf-fd-script)."""
        ...
    @property
    def PDF_FD_SERIF(self) -> _Any:
        """constants.PDF_FD_SERIF compatibility entry (case l4-constants-pdf-fd-serif)."""
        ...
    @property
    def PDF_FD_SMALL_CAP(self) -> _Any:
        """constants.PDF_FD_SMALL_CAP compatibility entry (case l4-constants-pdf-fd-small-cap)."""
        ...
    @property
    def PDF_FD_SYMBOLIC(self) -> _Any:
        """constants.PDF_FD_SYMBOLIC compatibility entry (case l4-constants-pdf-fd-symbolic)."""
        ...
    @property
    def PDF_SIMPLE_ENCODING_CYRILLIC(self) -> _Any:
        """constants.PDF_SIMPLE_ENCODING_CYRILLIC compatibility entry (case l4-constants-pdf-simple-encoding-cyrillic)."""
        ...
    @property
    def PDF_SIMPLE_ENCODING_GREEK(self) -> _Any:
        """constants.PDF_SIMPLE_ENCODING_GREEK compatibility entry (case l4-constants-pdf-simple-encoding-greek)."""
        ...
    @property
    def PDF_SIMPLE_ENCODING_LATIN(self) -> _Any:
        """constants.PDF_SIMPLE_ENCODING_LATIN compatibility entry (case l4-constants-pdf-simple-encoding-latin)."""
        ...
    @property
    def PDF_SIMPLE_FONT_RESOURCE(self) -> _Any:
        """constants.PDF_SIMPLE_FONT_RESOURCE compatibility entry (case l4-constants-pdf-simple-font-resource)."""
        ...
    @property
    def Base14_fontdict(self) -> _Any:
        """constants.Base14_fontdict compatibility entry (case l4-constants-base14-fontdict)."""
        ...
    @property
    def Base14_fontnames(self) -> _Any:
        """constants.Base14_fontnames compatibility entry (case l4-constants-base14-fontnames)."""
        ...
    @property
    def symbol_glyphs(self) -> _Any:
        """constants.symbol_glyphs compatibility entry (case l4-constants-symbol-glyphs)."""
        ...
    @property
    def zapf_glyphs(self) -> _Any:
        """constants.zapf_glyphs compatibility entry (case l4-constants-zapf-glyphs)."""
        ...
    @property
    def pdfcolor(self) -> _Any:
        """constants.pdfcolor compatibility entry (case l4-constants-pdfcolor)."""
        ...
    @property
    def INVALID_NAME_CHARS(self) -> _Any:
        """constants.INVALID_NAME_CHARS compatibility entry (case l2-constants-invalid-name-chars)."""
        ...
    @property
    def PDF_ENUM_FALSE(self) -> _Any:
        """constants.PDF_ENUM_FALSE compatibility entry (case l2-constants-pdf-enum-false)."""
        ...
    @property
    def PDF_ENUM_LIMIT(self) -> _Any:
        """constants.PDF_ENUM_LIMIT compatibility entry (case l2-constants-pdf-enum-limit)."""
        ...
    @property
    def PDF_ENUM_NULL(self) -> _Any:
        """constants.PDF_ENUM_NULL compatibility entry (case l2-constants-pdf-enum-null)."""
        ...
    @property
    def PDF_ENUM_TRUE(self) -> _Any:
        """constants.PDF_ENUM_TRUE compatibility entry (case l2-constants-pdf-enum-true)."""
        ...
    @property
    def PDF_LEXBUF_LARGE(self) -> _Any:
        """constants.PDF_LEXBUF_LARGE compatibility entry (case l2-constants-pdf-lexbuf-large)."""
        ...
    @property
    def PDF_LEXBUF_SMALL(self) -> _Any:
        """constants.PDF_LEXBUF_SMALL compatibility entry (case l2-constants-pdf-lexbuf-small)."""
        ...
    @property
    def PDF_MAX_GEN_NUMBER(self) -> _Any:
        """constants.PDF_MAX_GEN_NUMBER compatibility entry (case l2-constants-pdf-max-gen-number)."""
        ...
    @property
    def PDF_MAX_OBJECT_NUMBER(self) -> _Any:
        """constants.PDF_MAX_OBJECT_NUMBER compatibility entry (case l2-constants-pdf-max-object-number)."""
        ...
    @property
    def PDF_MRANGE_CAP(self) -> _Any:
        """constants.PDF_MRANGE_CAP compatibility entry (case l2-constants-pdf-mrange-cap)."""
        ...
    @property
    def PDF_NUM_TOKENS(self) -> _Any:
        """constants.PDF_NUM_TOKENS compatibility entry (case l2-constants-pdf-num-tokens)."""
        ...
    @property
    def PDF_PROCESSOR_REQUIRES_DECODED_IMAGES(self) -> _Any:
        """constants.PDF_PROCESSOR_REQUIRES_DECODED_IMAGES compatibility entry (case l2-constants-pdf-processor-requires-decoded-images)."""
        ...
    @property
    def PDF_TOK_CLOSE_ARRAY(self) -> _Any:
        """constants.PDF_TOK_CLOSE_ARRAY compatibility entry (case l2-constants-pdf-tok-close-array)."""
        ...
    @property
    def PDF_TOK_CLOSE_BRACE(self) -> _Any:
        """constants.PDF_TOK_CLOSE_BRACE compatibility entry (case l2-constants-pdf-tok-close-brace)."""
        ...
    @property
    def PDF_TOK_CLOSE_DICT(self) -> _Any:
        """constants.PDF_TOK_CLOSE_DICT compatibility entry (case l2-constants-pdf-tok-close-dict)."""
        ...
    @property
    def PDF_TOK_ENDOBJ(self) -> _Any:
        """constants.PDF_TOK_ENDOBJ compatibility entry (case l2-constants-pdf-tok-endobj)."""
        ...
    @property
    def PDF_TOK_ENDSTREAM(self) -> _Any:
        """constants.PDF_TOK_ENDSTREAM compatibility entry (case l2-constants-pdf-tok-endstream)."""
        ...
    @property
    def PDF_TOK_EOF(self) -> _Any:
        """constants.PDF_TOK_EOF compatibility entry (case l2-constants-pdf-tok-eof)."""
        ...
    @property
    def PDF_TOK_ERROR(self) -> _Any:
        """constants.PDF_TOK_ERROR compatibility entry (case l2-constants-pdf-tok-error)."""
        ...
    @property
    def PDF_TOK_FALSE(self) -> _Any:
        """constants.PDF_TOK_FALSE compatibility entry (case l2-constants-pdf-tok-false)."""
        ...
    @property
    def PDF_TOK_INT(self) -> _Any:
        """constants.PDF_TOK_INT compatibility entry (case l2-constants-pdf-tok-int)."""
        ...
    @property
    def PDF_TOK_KEYWORD(self) -> _Any:
        """constants.PDF_TOK_KEYWORD compatibility entry (case l2-constants-pdf-tok-keyword)."""
        ...
    @property
    def PDF_TOK_NAME(self) -> _Any:
        """constants.PDF_TOK_NAME compatibility entry (case l2-constants-pdf-tok-name)."""
        ...
    @property
    def PDF_TOK_NEWOBJ(self) -> _Any:
        """constants.PDF_TOK_NEWOBJ compatibility entry (case l2-constants-pdf-tok-newobj)."""
        ...
    @property
    def PDF_TOK_NULL(self) -> _Any:
        """constants.PDF_TOK_NULL compatibility entry (case l2-constants-pdf-tok-null)."""
        ...
    @property
    def PDF_TOK_OBJ(self) -> _Any:
        """constants.PDF_TOK_OBJ compatibility entry (case l2-constants-pdf-tok-obj)."""
        ...
    @property
    def PDF_TOK_OPEN_ARRAY(self) -> _Any:
        """constants.PDF_TOK_OPEN_ARRAY compatibility entry (case l2-constants-pdf-tok-open-array)."""
        ...
    @property
    def PDF_TOK_OPEN_BRACE(self) -> _Any:
        """constants.PDF_TOK_OPEN_BRACE compatibility entry (case l2-constants-pdf-tok-open-brace)."""
        ...
    @property
    def PDF_TOK_OPEN_DICT(self) -> _Any:
        """constants.PDF_TOK_OPEN_DICT compatibility entry (case l2-constants-pdf-tok-open-dict)."""
        ...
    @property
    def PDF_TOK_R(self) -> _Any:
        """constants.PDF_TOK_R compatibility entry (case l2-constants-pdf-tok-r)."""
        ...
    @property
    def PDF_TOK_REAL(self) -> _Any:
        """constants.PDF_TOK_REAL compatibility entry (case l2-constants-pdf-tok-real)."""
        ...
    @property
    def PDF_TOK_STARTXREF(self) -> _Any:
        """constants.PDF_TOK_STARTXREF compatibility entry (case l2-constants-pdf-tok-startxref)."""
        ...
    @property
    def PDF_TOK_STREAM(self) -> _Any:
        """constants.PDF_TOK_STREAM compatibility entry (case l2-constants-pdf-tok-stream)."""
        ...
    @property
    def PDF_TOK_STRING(self) -> _Any:
        """constants.PDF_TOK_STRING compatibility entry (case l2-constants-pdf-tok-string)."""
        ...
    @property
    def PDF_TOK_TRAILER(self) -> _Any:
        """constants.PDF_TOK_TRAILER compatibility entry (case l2-constants-pdf-tok-trailer)."""
        ...
    @property
    def PDF_TOK_TRUE(self) -> _Any:
        """constants.PDF_TOK_TRUE compatibility entry (case l2-constants-pdf-tok-true)."""
        ...
    @property
    def PDF_TOK_XREF(self) -> _Any:
        """constants.PDF_TOK_XREF compatibility entry (case l2-constants-pdf-tok-xref)."""
        ...
    @property
    def b(self) -> _Any:
        """constants.b compatibility entry (case l2-constants-b)."""
        ...
    @property
    def f(self) -> _Any:
        """constants.f compatibility entry (case l2-constants-f)."""
        ...
    @property
    def g(self) -> _Any:
        """constants.g compatibility entry (case l2-constants-g)."""
        ...
    @property
    def matrix_like(self) -> _Any:
        """constants.matrix_like compatibility entry (case l2-constants-matrix-like)."""
        ...
    @property
    def name(self) -> _Any:
        """constants.name compatibility entry (case l2-constants-name)."""
        ...
    @property
    def point_like(self) -> _Any:
        """constants.point_like compatibility entry (case l2-constants-point-like)."""
        ...
    @property
    def quad_like(self) -> _Any:
        """constants.quad_like compatibility entry (case l2-constants-quad-like)."""
        ...
    @property
    def r(self) -> _Any:
        """constants.r compatibility entry (case l2-constants-r)."""
        ...
    @property
    def rect_like(self) -> _Any:
        """constants.rect_like compatibility entry (case l2-constants-rect-like)."""
        ...
    @property
    def trace_device_CLIP_PATH(self) -> _Any:
        """constants.trace_device_CLIP_PATH compatibility entry (case l2-constants-trace-device-clip-path)."""
        ...
    @property
    def trace_device_CLIP_STROKE_PATH(self) -> _Any:
        """constants.trace_device_CLIP_STROKE_PATH compatibility entry (case l2-constants-trace-device-clip-stroke-path)."""
        ...
    @property
    def trace_device_FILL_PATH(self) -> _Any:
        """constants.trace_device_FILL_PATH compatibility entry (case l2-constants-trace-device-fill-path)."""
        ...
    @property
    def trace_device_STROKE_PATH(self) -> _Any:
        """constants.trace_device_STROKE_PATH compatibility entry (case l2-constants-trace-device-stroke-path)."""
        ...


class exceptions:
    """exceptions compatibility surface."""
    @property
    def EmptyFileError(self) -> _Any:
        """exceptions.EmptyFileError compatibility entry (case p0-exceptions-emptyfileerror)."""
        ...
    @property
    def FileDataError(self) -> _Any:
        """exceptions.FileDataError compatibility entry (case p0-exceptions-filedataerror)."""
        ...
    @property
    def FileNotFoundError(self) -> _Any:
        """exceptions.FileNotFoundError compatibility entry (case p0-exceptions-filenotfounderror)."""
        ...
    @property
    def FitzDeprecation(self) -> _Any:
        """exceptions.FitzDeprecation compatibility entry (case p0-exceptions-deprecation)."""
        ...


# Module-level entry points.

def ASSERT_PDF(cond: _Any) -> _Any:
    """module.ASSERT_PDF compatibility entry (case l5-module-assert-pdf)."""
    ...

def CheckColor(c: _Any) -> _Any:
    """module.CheckColor compatibility entry (case l5-module-checkcolor)."""
    ...

def CheckFont(page: _Any, fontname: _Any) -> _Any:
    """module.CheckFont compatibility entry (case l1-module-checkfont)."""
    ...

def CheckFontInfo(doc: _Any, xref: _Any) -> _Any:
    """module.CheckFontInfo compatibility entry (case l1-module-checkfontinfo)."""
    ...

def CheckMarkerArg(quads: _Any) -> _Any:
    """module.CheckMarkerArg compatibility entry (case l5-module-checkmarkerarg)."""
    ...

def CheckMorph(o: _Any) -> _Any:
    """module.CheckMorph compatibility entry (case l5-module-checkmorph)."""
    ...

def CheckParent(o: _Any) -> _Any:
    """module.CheckParent compatibility entry (case l5-module-checkparent)."""
    ...

def CheckQuad(q: _Any) -> _Any:
    """module.CheckQuad compatibility entry (case l5-module-checkquad)."""
    ...

def CheckRect(r: _Any) -> _Any:
    """module.CheckRect compatibility entry (case l5-module-checkrect)."""
    ...

def ColorCode(c: _Any, f: _Any) -> _Any:
    """module.ColorCode compatibility entry (case l5-module-colorcode)."""
    ...

def ConversionHeader(i: _Any, filename: _Any = ...) -> _Any:
    """module.ConversionHeader compatibility entry (case l5-module-conversionheader)."""
    ...

def ConversionTrailer(i: _Any) -> _Any:
    """module.ConversionTrailer compatibility entry (case l5-module-conversiontrailer)."""
    ...

def EMPTY_IRECT() -> _Any:
    """module.EMPTY_IRECT compatibility entry (case l5-module-empty-irect)."""
    ...

def EMPTY_QUAD() -> _Any:
    """module.EMPTY_QUAD compatibility entry (case l5-module-empty-quad)."""
    ...

def EMPTY_RECT() -> _Any:
    """module.EMPTY_RECT compatibility entry (case l5-module-empty-rect)."""
    ...

def ENSURE_OPERATION(pdf: _Any) -> _Any:
    """module.ENSURE_OPERATION compatibility entry (case l5-module-ensure-operation)."""
    ...

def INFINITE_IRECT() -> _Any:
    """module.INFINITE_IRECT compatibility entry (case l5-module-infinite-irect)."""
    ...

def INFINITE_QUAD() -> _Any:
    """module.INFINITE_QUAD compatibility entry (case l5-module-infinite-quad)."""
    ...

def INFINITE_RECT() -> _Any:
    """module.INFINITE_RECT compatibility entry (case l5-module-infinite-rect)."""
    ...

def PDF_NAME(x: _Any) -> _Any:
    """module.PDF_NAME compatibility entry (case l5-module-pdf-name)."""
    ...

def Page__add_text_marker(self: _Any, quads: _Any, annot_type: _Any) -> _Any:
    """module.Page__add_text_marker compatibility entry (case l1-module-page-add-text-marker)."""
    ...

def PySequence_Check(s: _Any) -> _Any:
    """module.PySequence_Check compatibility entry (case l5-module-pysequence-check)."""
    ...

def PySequence_Size(s: _Any) -> _Any:
    """module.PySequence_Size compatibility entry (case l5-module-pysequence-size)."""
    ...

def PyUnicode_DecodeRawUnicodeEscape(s: _Any, errors: _Any = ...) -> _Any:
    """module.PyUnicode_DecodeRawUnicodeEscape compatibility entry (case l5-module-pyunicode-decoderawunicodeescape)."""
    ...

def RAISEPY(msg: _Any, exc: _Any) -> _Any:
    """module.RAISEPY compatibility entry (case l5-module-raisepy)."""
    ...

def UpdateFontInfo(doc: _Any, info: _Any) -> _Any:
    """module.UpdateFontInfo compatibility entry (case l1-module-updatefontinfo)."""
    ...

def annot_postprocess(page: _Any, annot: _Any) -> _Any:
    """module.annot_postprocess compatibility entry (case l5-module-annot-postprocess)."""
    ...

def annot_preprocess(page: _Any) -> _Any:
    """module.annot_preprocess compatibility entry (case l5-module-annot-preprocess)."""
    ...

def apply_pages(path: _Any, pagefn: _Any, *, pagefn_args: _Any = ..., pagefn_kwargs: _Any = ..., initfn: _Any = ..., initfn_args: _Any = ..., initfn_kwargs: _Any = ..., pages: _Any = ..., method: _Any = ..., concurrency: _Any = ..., _stats: _Any = ...) -> _Any:
    """module.apply_pages compatibility entry (case l5-module-apply-pages)."""
    ...

def args_match(args: _Any, *types: _Any) -> _Any:
    """module.args_match compatibility entry (case l5-module-args-match)."""
    ...

def calc_image_matrix(width: _Any, height: _Any, tr: _Any, rotate: _Any, keep: _Any) -> _Any:
    """module.calc_image_matrix compatibility entry (case l3-module-calc-image-matrix)."""
    ...

def canon(c: _Any) -> _Any:
    """module.canon compatibility entry (case l1-module-canon)."""
    ...

def chartocanon(s: _Any) -> _Any:
    """module.chartocanon compatibility entry (case l1-module-chartocanon)."""
    ...

def colors_pdf_dict() -> _Any:
    """module.colors_pdf_dict compatibility entry (case l5-module-colors-pdf-dict)."""
    ...

def colors_wx_list() -> _Any:
    """module.colors_wx_list compatibility entry (case l5-module-colors-wx-list)."""
    ...

def compute_scissor(dev: _Any) -> _Any:
    """module.compute_scissor compatibility entry (case l5-module-compute-scissor)."""
    ...

def css_for_font(fontcode: _Any, *, CSS: _Any = ..., archive: _Any = ..., name: _Any = ...) -> _Any:
    """module.css_for_pymupdf_font compatibility entry (case l1-module-css-for-pymupdf-font)."""
    ...

def dest_is_valid(o: _Any, page_count: _Any, page_object_nums: _Any, names_list: _Any) -> _Any:
    """module.dest_is_valid compatibility entry (case l5-module-dest-is-valid)."""
    ...

def dest_is_valid_page(obj: _Any, page_object_nums: _Any, pagecount: _Any) -> _Any:
    """module.dest_is_valid_page compatibility entry (case l5-module-dest-is-valid-page)."""
    ...

def detect_super_script(line: _Any, ch: _Any) -> _Any:
    """module.detect_super_script compatibility entry (case l5-module-detect-super-script)."""
    ...

def dir_str(x: _Any) -> _Any:
    """module.dir_str compatibility entry (case l5-module-dir-str)."""
    ...

def exception_info() -> _Any:
    """module.exception_info compatibility entry (case l5-module-exception-info)."""
    ...

def extra_FzDocument_insert_pdf(doc: _Any, src: _Any, from_page: _Any, to_page: _Any, start_at: _Any, rotate: _Any, links: _Any, annots: _Any, show_progress: _Any, final: _Any, graft_map: _Any) -> _Any:
    """module.extra_FzDocument_insert_pdf compatibility entry (case l5-module-extra-fzdocument-insert-pdf)."""
    ...

def find_string(s: _Any, needle: _Any) -> _Any:
    """module.find_string compatibility entry (case l5-module-find-string)."""
    ...

def format_g(value: _Any, *, fmt: _Any = ...) -> _Any:
    """module.format_g compatibility entry (case l5-module-format-g)."""
    ...

def getTJstr(text: _Any, glyphs: _Any, simple: _Any, ordering: _Any) -> _Any:
    """module.getTJstr compatibility entry (case l5-module-gettjstr)."""
    ...

def get_env_bool(name: _Any, default: _Any) -> _Any:
    """module.get_env_bool compatibility entry (case l5-module-get-env-bool)."""
    ...

def get_env_int(name: _Any, default: _Any) -> _Any:
    """module.get_env_int compatibility entry (case l5-module-get-env-int)."""
    ...

def get_highlight_selection(page: _Any, start: _Any = ..., stop: _Any = ..., clip: _Any = ...) -> _Any:
    """module.get_highlight_selection compatibility entry (case l5-module-get-highlight-selection)."""
    ...

def get_pdf_now() -> _Any:
    """module.get_pdf_now compatibility entry (case l5-module-get-pdf-now)."""
    ...

def get_pdf_str(s: _Any) -> _Any:
    """module.get_pdf_str compatibility entry (case l5-module-get-pdf-str)."""
    ...

def get_tessdata(tessdata: _Any = ...) -> _Any:
    """module.get_tessdata compatibility entry (case l1-module-get-tessdata)."""
    ...

def get_text(path: _Any, *, pages: _Any = ..., method: _Any = ..., concurrency: _Any = ..., option: _Any = ..., clip: _Any = ..., flags: _Any = ..., textpage: _Any = ..., sort: _Any = ..., delimiters: _Any = ..., _stats: _Any = ...) -> _Any:
    """module.get_text compatibility entry (case l1-module-get-text)."""
    ...

def get_text_length(text: _Any, fontname: _Any = ..., fontsize: _Any = ..., encoding: _Any = ...) -> _Any:
    """module.get_text_length compatibility entry (case l1-module-get-text-length)."""
    ...

def glyph_name_to_unicode(name: _Any) -> _Any:
    """module.glyph_name_to_unicode compatibility entry (case l1-module-glyph-name-to-unicode)."""
    ...

def hdist(dir: _Any, a: _Any, b: _Any) -> _Any:
    """module.hdist compatibility entry (case l5-module-hdist)."""
    ...

def image_profile(img: _Any) -> _Any:
    """module.image_profile compatibility entry (case l3-module-image-profile)."""
    ...

def log(text: _Any = ..., caller: _Any = ...) -> _Any:
    """module.log compatibility entry (case l5-module-log)."""
    ...

def make_escape(ch: _Any) -> _Any:
    """module.make_escape compatibility entry (case l5-module-make-escape)."""
    ...

def make_story_elpos() -> _Any:
    """module.make_story_elpos compatibility entry (case l5-module-make-story-elpos)."""
    ...

def make_table(rect: _Any = ..., cols: _Any = ..., rows: _Any = ...) -> _Any:
    """module.make_table compatibility entry (case l1-module-make-table)."""
    ...

def match_string(h0: _Any, n0: _Any) -> _Any:
    """module.match_string compatibility entry (case l5-module-match-string)."""
    ...

def message(text: _Any = ...) -> _Any:
    """module.message compatibility entry (case l5-module-message)."""
    ...

def message_warning(text: _Any) -> _Any:
    """module.message_warning compatibility entry (case l5-module-message-warning)."""
    ...

def no_recommend_layout() -> _Any:
    """module.no_recommend_layout compatibility entry (case l5-module-no-recommend-layout)."""
    ...

def on_highlight_char(hits: _Any, line: _Any, ch: _Any) -> _Any:
    """module.on_highlight_char compatibility entry (case l1-module-on-highlight-char)."""
    ...

def page_merge(doc_des: _Any, doc_src: _Any, page_from: _Any, page_to: _Any, rotate: _Any, links: _Any, copy_annots: _Any, graft_map: _Any) -> _Any:
    """module.page_merge compatibility entry (case l5-module-page-merge)."""
    ...

def paper_rect(s: _Any) -> _Any:
    """module.paper_rect compatibility entry (case l5-module-paper-rect)."""
    ...

def paper_size(s: _Any) -> _Any:
    """module.paper_size compatibility entry (case l5-module-paper-size)."""
    ...

def paper_sizes() -> _Any:
    """module.paper_sizes compatibility entry (case l5-module-paper-sizes)."""
    ...

def pdf_lookup_page_loc(doc: _Any, needle: _Any) -> _Any:
    """module.pdf_lookup_page_loc compatibility entry (case l5-module-pdf-lookup-page-loc)."""
    ...

def pdfobj_string(o: _Any, prefix: _Any = ...) -> _Any:
    """module.pdfobj_string compatibility entry (case l5-module-pdfobj-string)."""
    ...

def planish_line(p1: _Any, p2: _Any) -> _Any:
    """module.planish_line compatibility entry (case l5-module-planish-line)."""
    ...

def recover_bbox_quad(line_dir: _Any, span: _Any, bbox: _Any) -> _Any:
    """module.recover_bbox_quad compatibility entry (case l1-module-recover-bbox-quad)."""
    ...

def recover_char_quad(line_dir: _Any, span: _Any, char: _Any) -> _Any:
    """module.recover_char_quad compatibility entry (case l1-module-recover-char-quad)."""
    ...

def recover_line_quad(line: _Any, spans: _Any = ...) -> _Any:
    """module.recover_line_quad compatibility entry (case l1-module-recover-line-quad)."""
    ...

def recover_quad(line_dir: _Any, span: _Any) -> _Any:
    """module.recover_quad compatibility entry (case l1-module-recover-quad)."""
    ...

def recover_span_quad(line_dir: _Any, span: _Any, chars: _Any = ...) -> _Any:
    """module.recover_span_quad compatibility entry (case l1-module-recover-span-quad)."""
    ...

def repair_mono_font(page: _Any, font: _Any) -> _Any:
    """module.repair_mono_font compatibility entry (case l1-module-repair-mono-font)."""
    ...

def restore_aliases() -> _Any:
    """module.restore_aliases compatibility entry (case l5-module-restore-aliases)."""
    ...

def sRGB_to_pdf(srgb: _Any) -> _Any:
    """module.sRGB_to_pdf compatibility entry (case l5-module-srgb-to-pdf)."""
    ...

def sRGB_to_rgb(srgb: _Any) -> _Any:
    """module.sRGB_to_rgb compatibility entry (case l5-module-srgb-to-rgb)."""
    ...

def set_log(*, text: _Any = ..., fd: _Any = ..., stream: _Any = ..., path: _Any = ..., path_append: _Any = ..., pylogging: _Any = ..., pylogging_logger: _Any = ..., pylogging_level: _Any = ..., pylogging_name: _Any = ...) -> _Any:
    """module.set_log compatibility entry (case l5-module-set-log)."""
    ...

def set_messages(*, text: _Any = ..., fd: _Any = ..., stream: _Any = ..., path: _Any = ..., path_append: _Any = ..., pylogging: _Any = ..., pylogging_logger: _Any = ..., pylogging_level: _Any = ..., pylogging_name: _Any = ...) -> _Any:
    """module.set_messages compatibility entry (case l5-module-set-messages)."""
    ...

def string_in_names_list(p: _Any, names_list: _Any) -> _Any:
    """module.string_in_names_list compatibility entry (case l5-module-string-in-names-list)."""
    ...

def strip_outline(doc: _Any, outlines: _Any, page_count: _Any, page_object_nums: _Any, names_list: _Any) -> _Any:
    """module.strip_outline compatibility entry (case l5-module-strip-outline)."""
    ...

def strip_outlines(doc: _Any, outlines: _Any, page_count: _Any, page_object_nums: _Any, names_list: _Any) -> _Any:
    """module.strip_outlines compatibility entry (case l5-module-strip-outlines)."""
    ...

def unicode_to_glyph_name(ch: _Any) -> _Any:
    """module.unicode_to_glyph_name compatibility entry (case l1-module-unicode-to-glyph-name)."""
    ...

def util_concat_matrix(m1: _Any, m2: _Any) -> _Any:
    """module.util_concat_matrix compatibility entry (case l5-module-util-concat-matrix)."""
    ...

def util_ensure_widget_calc(annot: _Any) -> _Any:
    """module.util_ensure_widget_calc compatibility entry (case l5-module-util-ensure-widget-calc)."""
    ...

def util_hor_matrix(C: _Any, P: _Any) -> _Any:
    """module.util_hor_matrix compatibility entry (case l5-module-util-hor-matrix)."""
    ...

def util_include_point_in_rect(r: _Any, p: _Any) -> _Any:
    """module.util_include_point_in_rect compatibility entry (case l5-module-util-include-point-in-rect)."""
    ...

def util_intersect_rect(r1: _Any, r2: _Any) -> _Any:
    """module.util_intersect_rect compatibility entry (case l5-module-util-intersect-rect)."""
    ...

def util_invert_matrix(matrix: _Any) -> _Any:
    """module.util_invert_matrix compatibility entry (case l5-module-util-invert-matrix)."""
    ...

def util_is_point_in_rect(p: _Any, r: _Any) -> _Any:
    """module.util_is_point_in_rect compatibility entry (case l5-module-util-is-point-in-rect)."""
    ...

def util_make_irect(*args: _Any, p0: _Any = ..., p1: _Any = ..., x0: _Any = ..., y0: _Any = ..., x1: _Any = ..., y1: _Any = ...) -> _Any:
    """module.util_make_irect compatibility entry (case l5-module-util-make-irect)."""
    ...

def util_make_rect(*args: _Any, p0: _Any = ..., p1: _Any = ..., x0: _Any = ..., y0: _Any = ..., x1: _Any = ..., y1: _Any = ...) -> _Any:
    """module.util_make_rect compatibility entry (case l5-module-util-make-rect)."""
    ...

def util_measure_string(text: _Any, fontname: _Any, fontsize: _Any, encoding: _Any) -> _Any:
    """module.util_measure_string compatibility entry (case l5-module-util-measure-string)."""
    ...

def util_point_in_quad(P: _Any, Q: _Any) -> _Any:
    """module.util_point_in_quad compatibility entry (case l5-module-util-point-in-quad)."""
    ...

def util_round_rect(rect: _Any) -> _Any:
    """module.util_round_rect compatibility entry (case l5-module-util-round-rect)."""
    ...

def util_sine_between(C: _Any, P: _Any, Q: _Any) -> _Any:
    """module.util_sine_between compatibility entry (case l5-module-util-sine-between)."""
    ...

def util_transform_point(point: _Any, matrix: _Any) -> _Any:
    """module.util_transform_point compatibility entry (case l5-module-util-transform-point)."""
    ...

def util_transform_rect(rect: _Any, matrix: _Any) -> _Any:
    """module.util_transform_rect compatibility entry (case l5-module-util-transform-rect)."""
    ...

def util_union_rect(r1: _Any, r2: _Any) -> _Any:
    """module.util_union_rect compatibility entry (case l5-module-util-union-rect)."""
    ...

def vdist(dir: _Any, a: _Any, b: _Any) -> _Any:
    """module.vdist compatibility entry (case l5-module-vdist)."""
    ...
