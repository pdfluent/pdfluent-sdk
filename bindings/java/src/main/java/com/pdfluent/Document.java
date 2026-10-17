// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.NativeLongByReference;
import com.sun.jna.ptr.PointerByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.countOf;
import static com.pdfluent.CompatSupport.flag;
import static com.pdfluent.CompatSupport.lib;
import static com.pdfluent.CompatSupport.readBytes;
import static com.pdfluent.CompatSupport.readDoubles;
import static com.pdfluent.CompatSupport.readInts;
import static com.pdfluent.CompatSupport.readString;

/**
 * A compatible document: open, save, outline, and object access.
 *
 * <p>Pages are addressed through {@link Page} values; the document owns
 * the native handle and must be closed.
 */
public final class Document implements AutoCloseable {
    private Pointer handle;

    /**
     * Open a document from a file.
     *
     * @param filename path to the file
     */
    public Document(String filename) {
        this(openHandle(filename, null, null, null, 0.0, 0.0, 11.0, null));
    }

    /**
     * Open a document from bytes.
     *
     * @param data file bytes
     */
    public Document(byte[] data) {
        this(openHandle(null, data, null, null, 0.0, 0.0, 11.0, null));
    }

    /**
     * Open a document from bytes with a file-type hint.
     *
     * @param data     file bytes
     * @param filetype file-type hint
     */
    public Document(byte[] data, String filetype) {
        this(openHandle(null, data, filetype, null, 0.0, 0.0, 11.0, null));
    }

    /**
     * Open a fresh empty shell.
     */
    public Document() {
        this(openHandle(null, null, null, null, 0.0, 0.0, 11.0, null));
    }

    /**
     * Open a document with full options.
     *
     * @param options open options
     */
    public Document(DocumentOptions options) {
        this(openHandle(options.filename, options.data, options.filetype,
            options.rect == null ? null : options.rect.toNative(),
            options.width, options.height, options.fontsize, options.archiveJson));
    }

    private Document(Pointer handle) {
        this.handle = handle;
    }

    private static Pointer openHandle(String filename, byte[] data, String filetype,
            PdfCapiLibrary.PdfCompatRect rect, double width, double height,
            double fontsize, String archive) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_open(filename, data, countOf(data), filetype, rect,
            width, height, fontsize, archive, out), "open");
        return out.getValue();
    }


    /** Unlock with a password. */
    public int authenticate(String password) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_authenticate(requireOpen(), password, out), "authenticate");
        return out.getValue();
    }

    /** Flatten annotations and widgets into content. */
    public void bake(boolean annots, boolean widgets) {
        check(lib().pdf_compat_document_bake(requireOpen(), flag(annots), flag(widgets)), "bake");
    }

    /** Convenience overload with defaults. */
    public void bake() {
        bake(true, true);
    }

    /** Convert a page span to PDF content. */
    public byte[] convertToPdf(int from_page, int to_page, int rotate) {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_document_convert_to_pdf(requireOpen(), from_page, to_page, rotate, outData, outLen), "convertToPdf");
        return readBytes(outData, outLen);
    }

    /** Convenience overload with defaults. */
    public byte[] convertToPdf() {
        return convertToPdf(0, -1, 0);
    }

    /** Font bytes and facts. */
    public FontData extractFont(int xref, boolean info_only, String name) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_extract_font(requireOpen(), xref, flag(info_only), name, out), "extractFont");
        Pointer handle = out.getValue();
        try {
            return FontData.read(handle);
        } finally {
            lib().pdf_compat_font_data_free(handle);
        }
    }

    /** Convenience overload with defaults. */
    public FontData extractFont(int xref) {
        return extractFont(xref, false, null);
    }

    /** Image bytes and facts for an object number. */
    public ImageData extractImage(int xref) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_extract_image(requireOpen(), xref, out), "extractImage");
        Pointer handle = out.getValue();
        try {
            return ImageData.read(handle);
        } finally {
            lib().pdf_compat_image_data_free(handle);
        }
    }

    /** Reserve a fresh object number. */
    public int getNewXref() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_get_new_xref(requireOpen(), out), "getNewXref");
        return out.getValue();
    }

    /** Fonts referenced by a page. */
    public java.util.List<FontInfo> getPageFonts(int page, boolean full) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_get_page_fonts(requireOpen(), page, flag(full), out), "getPageFonts");
        Pointer list = out.getValue();
        try {
            return FontInfo.readList(list);
        } finally {
            lib().pdf_compat_font_info_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<FontInfo> getPageFonts(int page) {
        return getPageFonts(page, false);
    }

    /** Images referenced by a page. */
    public java.util.List<ImageRef> getPageImages(int page, boolean full) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_get_page_images(requireOpen(), page, flag(full), out), "getPageImages");
        Pointer list = out.getValue();
        try {
            return ImageRef.readList(list);
        } finally {
            lib().pdf_compat_image_ref_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<ImageRef> getPageImages(int page) {
        return getPageImages(page, false);
    }

    /** Outline entries. */
    public java.util.List<TocEntry> getToc(boolean simple) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_get_toc(requireOpen(), flag(simple), out), "getToc");
        Pointer list = out.getValue();
        try {
            return TocEntry.readList(list);
        } finally {
            lib().pdf_compat_toc_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<TocEntry> getToc() {
        return getToc(true);
    }

    /** Convenience overload with defaults. */
    public void insertPdf(Document source) {
        insertPdf(source, new InsertPdfOptions());
    }

    /** Whether this is a form PDF. */
    public boolean isFormPdf() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_is_form_pdf(requireOpen(), out), "isFormPdf");
        return out.getValue() != 0;
    }

    /** Whether this is a PDF document. */
    public boolean isPdf() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_is_pdf(requireOpen(), out), "isPdf");
        return out.getValue() != 0;
    }

    /** Undo/redo availability of the compatible journal. */
    public boolean[] journalCanDo() {
        IntByReference canUndo = new IntByReference();
        IntByReference canRedo = new IntByReference();
        check(lib().pdf_compat_document_journal_can_do(requireOpen(), canUndo, canRedo), "journalCanDo");
        return new boolean[] {canUndo.getValue() != 0, canRedo.getValue() != 0};
    }

    /** Enable the compatible journal. */
    public void journalEnable() {
        check(lib().pdf_compat_document_journal_enable(requireOpen()), "journalEnable");
    }

    /** Whether the compatible journal is enabled. */
    public boolean journalIsEnabled() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_journal_is_enabled(requireOpen(), out), "journalIsEnabled");
        return out.getValue() != 0;
    }

    /** Load a journal file over a never-enabled journal. */
    public void journalLoad(String path) {
        check(lib().pdf_compat_document_journal_load(requireOpen(), path), "journalLoad");
    }

    /** Journal step name. */
    public String journalOpName(int step) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_journal_op_name(requireOpen(), step, out), "journalOpName");
        return readString(out);
    }

    /** Journal position: the applied prefix length. */
    public int journalPosition() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_journal_position(requireOpen(), out), "journalPosition");
        return out.getValue();
    }

    /** Redo a journal step. */
    public void journalRedo() {
        check(lib().pdf_compat_document_journal_redo(requireOpen()), "journalRedo");
    }

    /** Save the journal file. */
    public void journalSave(String path) {
        check(lib().pdf_compat_document_journal_save(requireOpen(), path), "journalSave");
    }

    /** Start a journal step; the name may be null for an anonymous step. */
    public void journalStartOp(String name) {
        check(lib().pdf_compat_document_journal_start_op(requireOpen(), name), "journalStartOp");
    }

    /** Stop the journal step. */
    public void journalStopOp() {
        check(lib().pdf_compat_document_journal_stop_op(requireOpen()), "journalStopOp");
    }

    /** Undo a journal step. */
    public void journalUndo() {
        check(lib().pdf_compat_document_journal_undo(requireOpen()), "journalUndo");
    }

    /** Whether the document needs a password. */
    public int needsPass() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_needs_pass(requireOpen(), out), "needsPass");
        return out.getValue();
    }

    /** Convenience overload with defaults. */
    public Page newPage() {
        return newPage(-1, 595.0, 842.0);
    }

    /** Number of pages. Binds PdfDocument::len; the spec names no page_count (reported). */
    public int getPageCount() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_page_count(requireOpen(), out), "getPageCount");
        return out.getValue();
    }

    /** The document catalog. */
    public String pdfCatalog() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_pdf_catalog(requireOpen(), out), "pdfCatalog");
        return readString(out);
    }

    /** Save to a file. */
    public void save(String path, SaveOptions settings) {
        check(lib().pdf_compat_document_save(requireOpen(), path, settings == null ? null : settings.toJson()), "save");
    }

    /** Convenience overload with defaults. */
    public void save(String path) {
        save(path, null);
    }

    /** Save incrementally. */
    public void saveIncr() {
        check(lib().pdf_compat_document_save_incr(requireOpen()), "saveIncr");
    }

    /** Convenience overload with defaults. */
    public int setToc(java.util.List<TocEntry> entries) {
        return setToc(entries, 1);
    }

    /** Serialise the document to bytes. */
    public byte[] tobytes(SaveOptions settings) {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_document_tobytes(requireOpen(), settings == null ? null : settings.toJson(), outData, outLen), "tobytes");
        return readBytes(outData, outLen);
    }

    /** Convenience overload with defaults. */
    public byte[] tobytes() {
        return tobytes(null);
    }

    /** Replace one raw object. */
    public void updateObject(int xref, String text, Integer page) {
        check(lib().pdf_compat_document_update_object(requireOpen(), xref, text, page == null ? -1 : page), "updateObject");
    }

    /** Convenience overload with defaults. */
    public void updateObject(int xref, String text) {
        updateObject(xref, text, null);
    }

    /** Replace one stream payload. */
    public void updateStream(int xref, byte[] data, boolean is_new, boolean compress) {
        check(lib().pdf_compat_document_update_stream(requireOpen(), xref, data, countOf(data), flag(is_new), flag(compress)), "updateStream");
    }

    /** Convenience overload with defaults. */
    public void updateStream(int xref) {
        updateStream(xref, null, true, true);
    }

    /** Write the document to bytes. */
    public byte[] write(SaveOptions settings) {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_document_write(requireOpen(), settings == null ? null : settings.toJson(), outData, outLen), "write");
        return readBytes(outData, outLen);
    }

    /** Convenience overload with defaults. */
    public byte[] write() {
        return write(null);
    }

    /** Object count. */
    public int xrefLength() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_xref_length(requireOpen(), out), "xrefLength");
        return out.getValue();
    }

    /** One raw object. */
    public String xrefObject(int xref, boolean compressed, boolean ascii) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_document_xref_object(requireOpen(), xref, flag(compressed), flag(ascii), out), "xrefObject");
        return readString(out);
    }

    /** Convenience overload with defaults. */
    public String xrefObject(int xref) {
        return xrefObject(xref, false, false);
    }

    /** Set one object key. */
    public void xrefSetKey(int xref, String key, String value) {
        check(lib().pdf_compat_document_xref_set_key(requireOpen(), xref, key, value), "xrefSetKey");
    }

    /** One stream payload. */
    public byte[] xrefStream(int xref) {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_document_xref_stream(requireOpen(), xref, outData, outLen), "xrefStream");
        return readBytes(outData, outLen);
    }

    /**
     * Address a page by number.
     *
     * @param page 0-based page number
     * @return the page
     */
    public Page loadPage(int page) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_load_page(requireOpen(), page, out), "loadPage");
        return new Page(this, out.getValue());
    }


    /**
     * Insert a fresh page.
     *
     * @param at     insertion point
     * @param width  page width in points
     * @param height page height in points
     * @return the new page
     */
    public Page newPage(int at, double width, double height) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_document_new_page(requireOpen(), at, width, height, out), "newPage");
        return new Page(this, out.getValue());
    }


    /**
     * Replace the outline; returns its size.
     *
     * @param entries  outline entries
     * @param collapse collapse level
     * @return the outline size
     */
    public int setToc(java.util.List<TocEntry> entries, int collapse) {
        PointerByReference builder = new PointerByReference();
        check(lib().pdf_compat_toc_builder_new(builder), "setToc");
        Pointer handle = builder.getValue();
        try {
            for (TocEntry entry : entries) {
                Point target = entry.getDestTarget();
                check(lib().pdf_compat_toc_builder_push(handle, entry.getLevel(), entry.getTitle(),
                    entry.getPage(), flag(entry.hasDest()), entry.getDestKind(), entry.getDestPage(),
                    target == null ? 0.0 : target.getX(), target == null ? 0.0 : target.getY(),
                    entry.getDestZoom()), "setToc");
            }
            IntByReference out = new IntByReference();
            check(lib().pdf_compat_document_set_toc(requireOpen(), handle, collapse, out), "setToc");
            return out.getValue();
        } finally {
            lib().pdf_compat_toc_builder_free(handle);
        }
    }


    /**
     * Replace the document metadata.
     *
     * @param metadata new metadata, or null to clear
     */
    public void setMetadata(DocMetadata metadata) {
        String title = metadata == null ? null : metadata.title;
        String author = metadata == null ? null : metadata.author;
        String subject = metadata == null ? null : metadata.subject;
        String keywords = metadata == null ? null : metadata.keywords;
        String creator = metadata == null ? null : metadata.creator;
        String producer = metadata == null ? null : metadata.producer;
        String creationDate = metadata == null ? null : metadata.creationDate;
        String modDate = metadata == null ? null : metadata.modDate;
        String format = metadata == null ? null : metadata.format;
        String encryption = metadata == null ? null : metadata.encryption;
        String trapped = metadata == null ? null : metadata.trapped;
        check(lib().pdf_compat_document_set_metadata(requireOpen(), title, author, subject,
            keywords, creator, producer, creationDate, modDate, format, encryption,
            trapped), "setMetadata");
    }


    /**
     * One object key: value and type name.
     *
     * @param xref object number
     * @param key  key name
     * @return the value and its type name
     */
    public String[] xrefGetKey(int xref, String key) {
        PointerByReference out = new PointerByReference();
        PointerByReference out2 = new PointerByReference();
        check(lib().pdf_compat_document_xref_get_key(requireOpen(), xref, key, out, out2), "xrefGetKey");
        return new String[] {readString(out), readString(out2)};
    }


    /**
     * Insert pages from another document.
     *
     * @param source  source document
     * @param options import options
     */
    public void insertPdf(Document source, InsertPdfOptions options) {
        check(lib().pdf_compat_document_insert_pdf(requireOpen(), source.requireOpen(),
            options.fromPage, options.toPage, options.startAt, options.rotate,
            flag(options.links), flag(options.annots), flag(options.widgets),
            flag(options.joinDuplicates), flag(options.showProgress),
            flag(options.finish)), "insertPdf");
    }


    /**
     * All pages, in order.
     *
     * @return one {@link Page} per page
     */
    public java.util.List<Page> pages() {
        return pages(null, null, null);
    }

    /**
     * A page slice, in order.
     *
     * @param start first page, or null for the first
     * @param stop  stop before this page, or null for the end
     * @param step  stride, or null for 1
     * @return the selected pages
     */
    public java.util.List<Page> pages(Integer start, Integer stop, Integer step) {
        int count = getPageCount();
        int from = start == null ? 0 : start;
        int to = stop == null ? count : stop;
        int stride = step == null ? 1 : step;
        java.util.List<Page> result = new java.util.ArrayList<>();
        for (int i = from; i < to; i += stride) {
            result.add(loadPage(i));
        }
        return result;
    }


    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_document_close(handle);
            handle = null;
        }
    }

    /**
     * The native handle, failing when closed.
     *
     * @return the live handle
     */
    public Pointer requireOpen() {
        if (handle == null) {
            throw new PdfluentException("document is closed");
        }
        return handle;
    }
}
