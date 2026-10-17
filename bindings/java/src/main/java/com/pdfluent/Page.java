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
 * A compatible page: text, images, and drawing.
 *
 * <p>A lightweight address of a document page; all calls delegate
 * through the owning document.
 */
public final class Page {
    private final Document doc;
    private final int page;

    Document getDoc() {
        return doc;
    }

    int getPage() {
        return page;
    }

    /**
     * Address a page of a document.
     *
     * @param document owning document
     * @param page     0-based page number
     */
    public Page(Document document, int page) {
        this.doc = document;
        this.page = page;
    }


    /** Highlight text; returns the annotation xref. */
    public int addHighlightAnnot(Quad[] quads, Point start, Point stop, Rect clip) {
        PdfCapiLibrary.PdfCompatQuad[] nativeQuads = quads == null ? null : Quad.toNativeArray(quads);
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_add_highlight_annot(doc.requireOpen(), page, nativeQuads, countOf(nativeQuads), start == null ? null : start.toNative(), stop == null ? null : stop.toNative(), clip == null ? null : clip.toNative(), out), "addHighlightAnnot");
        return out.getValue();
    }

    /** Convenience overload with defaults. */
    public int addHighlightAnnot() {
        return addHighlightAnnot(null, null, null, null);
    }

    /** Add a rectangle annotation; returns its xref. */
    public int addRectAnnot(Rect rect) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_add_rect_annot(doc.requireOpen(), page, rect.toNative(), out), "addRectAnnot");
        return out.getValue();
    }

    /** Convenience overload with defaults. */
    public int addRedactAnnot(Quad quad) {
        return addRedactAnnot(quad, new RedactAnnotOptions());
    }

    /** Add a text annotation; returns its xref. */
    public int addTextAnnot(Point at, String text, String icon) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_add_text_annot(doc.requireOpen(), page, at.toNative(), text, icon, out), "addTextAnnot");
        return out.getValue();
    }

    /** Convenience overload with defaults. */
    public int addTextAnnot(Point at, String text) {
        return addTextAnnot(at, text, "Note");
    }

    /** Add a form widget; returns its xref. */
    public int addWidget(Widget widget) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_add_widget(doc.requireOpen(), page, widget.requireOpen(), out), "addWidget");
        return out.getValue();
    }

    /** Annotations on the page. */
    public java.util.List<AnnotEntry> annots(int[] types) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_annots(doc.requireOpen(), page, types, countOf(types), out), "annots");
        Pointer list = out.getValue();
        try {
            return AnnotEntry.readList(list);
        } finally {
            lib().pdf_compat_annot_entry_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<AnnotEntry> annots() {
        return annots(null);
    }

    /** Burn pending redactions in. */
    public boolean applyRedactions(int images, int graphics, int text) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_apply_redactions(doc.requireOpen(), page, images, graphics, text, out), "applyRedactions");
        return out.getValue() != 0;
    }

    /** Convenience overload with defaults. */
    public boolean applyRedactions() {
        return applyRedactions(2, 1, 0);
    }

    /** Tidy the content stream. */
    public void cleanContents(boolean sanitize) {
        check(lib().pdf_compat_page_clean_contents(doc.requireOpen(), page, flag(sanitize)), "cleanContents");
    }

    /** Convenience overload with defaults. */
    public void cleanContents() {
        cleanContents(true);
    }

    /** Delete an annotation. */
    public void deleteAnnot(int xref, int type_code, String type_name, Rect rect) {
        check(lib().pdf_compat_page_delete_annot(doc.requireOpen(), page, xref, type_code, type_name, rect.toNative()), "deleteAnnot");
    }

    /** Convenience overload with defaults. */
    public void deleteAnnot(AnnotEntry entry) {
        deleteAnnot(entry.getXref(), entry.getTypeCode(), entry.getTypeName(), entry.getRect());
    }

    /** Stroke a Bezier curve. */
    public Point drawBezier(Point p1, Point p2, Point p3, Point p4, DrawOptions options) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_page_draw_bezier(doc.requireOpen(), page, p1.toNative(), p2.toNative(), p3.toNative(), p4.toNative(), options == null ? null : options.toJson(), out), "drawBezier");
        return Point.fromNative(out);
    }

    /** Convenience overload with defaults. */
    public Point drawBezier(Point p1, Point p2, Point p3, Point p4) {
        return drawBezier(p1, p2, p3, p4, null);
    }

    /** Stroke a circle. */
    public Point drawCircle(Point center, double radius, DrawOptions options) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_page_draw_circle(doc.requireOpen(), page, center.toNative(), radius, options == null ? null : options.toJson(), out), "drawCircle");
        return Point.fromNative(out);
    }

    /** Convenience overload with defaults. */
    public Point drawCircle(Point center, double radius) {
        return drawCircle(center, radius, null);
    }

    /** Stroke a line. */
    public Point drawLine(Point p1, Point p2, DrawOptions options) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_page_draw_line(doc.requireOpen(), page, p1.toNative(), p2.toNative(), options == null ? null : options.toJson(), out), "drawLine");
        return Point.fromNative(out);
    }

    /** Convenience overload with defaults. */
    public Point drawLine(Point p1, Point p2) {
        return drawLine(p1, p2, null);
    }

    /** Stroke a rectangle. */
    public Point drawRect(Rect rect, DrawOptions options, Double radius) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_page_draw_rect(doc.requireOpen(), page, rect.toNative(), options == null ? null : options.toJson(), radius == null ? Double.NaN : radius, out), "drawRect");
        return Point.fromNative(out);
    }

    /** Convenience overload with defaults. */
    public Point drawRect(Rect rect) {
        return drawRect(rect, null, null);
    }

    /** Detect tables on the page. */
    public TableFinder findTables() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_find_tables(doc.requireOpen(), page, out), "findTables");
        return TableFinder.read(out.getValue());
    }

    /** First annotation, if any. */
    public java.util.List<AnnotEntry> getFirstAnnot() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_first_annot(doc.requireOpen(), page, out), "getFirstAnnot");
        Pointer list = out.getValue();
        try {
            return AnnotEntry.readList(list);
        } finally {
            lib().pdf_compat_annot_entry_list_free(list);
        }
    }

    /** Content-stream object numbers. */
    public int[] getContents() {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_page_get_contents(doc.requireOpen(), page, outData, outLen), "getContents");
        return readInts(outData, outLen);
    }

    /** Vector drawings on the page. */
    public java.util.List<Drawing> getDrawings(boolean extended) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_drawings(doc.requireOpen(), page, flag(extended), out), "getDrawings");
        Pointer list = out.getValue();
        try {
            return Drawing.readList(list);
        } finally {
            lib().pdf_compat_drawing_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<Drawing> getDrawings() {
        return getDrawings(false);
    }

    /** Fonts used on the page. */
    public java.util.List<FontInfo> getFonts(boolean full) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_fonts(doc.requireOpen(), page, flag(full), out), "getFonts");
        Pointer list = out.getValue();
        try {
            return FontInfo.readList(list);
        } finally {
            lib().pdf_compat_font_info_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<FontInfo> getFonts() {
        return getFonts(false);
    }

    /** Placed images with geometry. */
    public java.util.List<PlacedImage> getImageInfo(boolean hashes, boolean xrefs) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_image_info(doc.requireOpen(), page, flag(hashes), flag(xrefs), out), "getImageInfo");
        Pointer list = out.getValue();
        try {
            return PlacedImage.readList(list);
        } finally {
            lib().pdf_compat_placed_image_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<PlacedImage> getImageInfo() {
        return getImageInfo(false, false);
    }

    /** Boxes where a named image appears. */
    public java.util.List<Rect> getImageRects(String name, boolean transform) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_image_rects(doc.requireOpen(), page, name, flag(transform), out), "getImageRects");
        Pointer list = out.getValue();
        try {
            IntByReference count = new IntByReference();
            check(lib().pdf_compat_rect_list_count(list, count), "getImageRects");
            java.util.List<Rect> result = new java.util.ArrayList<>();
            for (int i = 0; i < count.getValue(); i++) {
                PdfCapiLibrary.PdfCompatRect r = new PdfCapiLibrary.PdfCompatRect();
                check(lib().pdf_compat_rect_list_at(list, i, r), "getImageRects");
                result.add(Rect.fromNative(r));
            }
            return result;
        } finally {
            lib().pdf_compat_rect_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<Rect> getImageRects(String name) {
        return getImageRects(name, false);
    }

    /** Images on the page. */
    public java.util.List<ImageRef> getImages(boolean full) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_images(doc.requireOpen(), page, flag(full), out), "getImages");
        Pointer list = out.getValue();
        try {
            return ImageRef.readList(list);
        } finally {
            lib().pdf_compat_image_ref_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<ImageRef> getImages() {
        return getImages(false);
    }

    /** Links on the page. */
    public java.util.List<Link> getLinks() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_links(doc.requireOpen(), page, out), "getLinks");
        Pointer list = out.getValue();
        try {
            return Link.readList(list);
        } finally {
            lib().pdf_compat_link_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public Pixmap getPixmap() {
        return getPixmap(new RenderOptions());
    }

    /** Render the page to SVG text. */
    public String getSvgImage(Matrix matrix, boolean text_as_path) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_svg_image(doc.requireOpen(), page, matrix == null ? null : matrix.toNative(), flag(text_as_path), out), "getSvgImage");
        return readString(out);
    }

    /** Convenience overload with defaults. */
    public String getSvgImage() {
        return getSvgImage(null, true);
    }

    /** Extract text in the named format. */
    public TextOutput getText(String format, Rect clip, Integer flags, boolean sort, boolean use_textpage) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_text(doc.requireOpen(), page, format, clip == null ? null : clip.toNative(), flags == null ? -1 : flags, flag(sort), flag(use_textpage), out), "getText");
        return TextOutput.read(out.getValue());
    }

    /** Convenience overload with defaults. */
    public TextOutput getText() {
        return getText("text", null, null, false, false);
    }

    /** Convenience overload with defaults. */
    public TextOutput getText(String format) {
        return getText(format, null, null, false, false);
    }

    /** Text inside a rectangle. */
    public String getTextbox(Rect rect, boolean use_textpage) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_textbox(doc.requireOpen(), page, rect.toNative(), flag(use_textpage), out), "getTextbox");
        return readString(out);
    }

    /** Convenience overload with defaults. */
    public String getTextbox(Rect rect) {
        return getTextbox(rect, false);
    }

    /** Typeset spans with glyph origins. */
    public java.util.List<TextTrace> getTexttrace() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_texttrace(doc.requireOpen(), page, out), "getTexttrace");
        Pointer list = out.getValue();
        try {
            return TextTrace.readList(list);
        } finally {
            lib().pdf_compat_text_trace_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public int insertFont() {
        return insertFont(new InsertFontOptions());
    }

    /** Convenience overload with defaults. */
    public int insertImage(Rect rect) {
        return insertImage(rect, new ImageInsertOptions());
    }

    /** Add a link to the page. */
    public void insertLink(Link link, boolean mark) {
        check(lib().pdf_compat_page_insert_link(doc.requireOpen(), page, link.requireOpen(), flag(mark)), "insertLink");
    }

    /** Convenience overload with defaults. */
    public void insertLink(Link link) {
        insertLink(link, true);
    }

    /** Media box in points. */
    public Rect getMediabox() {
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_page_mediabox(doc.requireOpen(), page, out), "getMediabox");
        return Rect.fromNative(out);
    }

    /** Open a drawing canvas on the page. */
    public void newShape() {
        check(lib().pdf_compat_page_new_shape(doc.requireOpen(), page), "newShape");
    }

    /** Page box in points. */
    public Rect getRect() {
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_page_rect(doc.requireOpen(), page, out), "getRect");
        return Rect.fromNative(out);
    }

    /** Page rotation in degrees. */
    public int getRotation() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_rotation(doc.requireOpen(), page, out), "getRotation");
        return out.getValue();
    }

    /** Search for text; rectangles or quadrilaterals. */
    public SearchHits searchFor(String needle, Rect clip, boolean quads, Integer flags, int limit, boolean use_textpage) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_search_for(doc.requireOpen(), page, needle, clip == null ? null : clip.toNative(), flag(quads), flags == null ? -1 : flags, limit, flag(use_textpage), out), "searchFor");
        return SearchHits.read(out.getValue());
    }

    /** Convenience overload with defaults. */
    public SearchHits searchFor(String needle) {
        return searchFor(needle, null, false, null, 0, false);
    }

    /** Set the crop box. */
    public void setCropbox(Rect rect) {
        check(lib().pdf_compat_page_set_cropbox(doc.requireOpen(), page, rect.toNative()), "setCropbox");
    }

    /** Set the rotation. */
    public void setRotation(int rotation) {
        check(lib().pdf_compat_page_set_rotation(doc.requireOpen(), page, rotation), "setRotation");
    }

    /** Convenience overload with defaults. */
    public int showPdfPage(Rect rect, Document source) {
        return showPdfPage(rect, source, 0, new ShowPageOptions());
    }

    /** Form widgets on the page. */
    public java.util.List<Widget> widgets(int[] types) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_widgets(doc.requireOpen(), page, types, countOf(types), out), "widgets");
        Pointer list = out.getValue();
        try {
            return Widget.readList(list);
        } finally {
            lib().pdf_compat_widget_list_free(list);
        }
    }

    /** Convenience overload with defaults. */
    public java.util.List<Widget> widgets() {
        return widgets(null);
    }

    /** Page object number. */
    public int getXref() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_xref(doc.requireOpen(), page, out), "getXref");
        return out.getValue();
    }

    /**
     * Render the page to a pixel buffer.
     *
     * @param options render options
     * @return the rendered pixmap
     */
    public Pixmap getPixmap(RenderOptions options) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_page_get_pixmap(doc.requireOpen(), page, options.matrix.toNative(),
            options.dpi == null ? Double.NaN : options.dpi,
            options.colorspace == null ? -1 : options.colorspace.code,
            options.clip == null ? null : options.clip.toNative(),
            flag(options.alpha), flag(options.annots), out), "getPixmap");
        return new Pixmap(out.getValue());
    }


    /**
     * Insert text at a point.
     *
     * @param at      insertion point
     * @param text    text to insert
     * @param options layout options, or null for defaults
     * @return the insertion result code
     */
    public int insertText(Point at, String text, InsertTextOptions options) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_insert_text(doc.requireOpen(), page, at.toNative(), text,
            null, countOf((String[]) null), options == null ? null : options.toJson(), out), "insertText");
        return out.getValue();
    }

    /**
     * Insert text lines at a point.
     *
     * @param at      insertion point
     * @param lines   text lines to insert
     * @param options layout options, or null for defaults
     * @return the insertion result code
     */
    public int insertText(Point at, java.util.List<String> lines, InsertTextOptions options) {
        String[] array = lines == null ? null : lines.toArray(new String[0]);
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_insert_text(doc.requireOpen(), page, at.toNative(), null,
            array, countOf(array), options == null ? null : options.toJson(), out), "insertText");
        return out.getValue();
    }

    /**
     * Insert text at a point with default options.
     *
     * @param at   insertion point
     * @param text text to insert
     * @return the insertion result code
     */
    public int insertText(Point at, String text) {
        return insertText(at, text, null);
    }

    /**
     * Insert text lines at a point with default options.
     *
     * @param at    insertion point
     * @param lines text lines to insert
     * @return the insertion result code
     */
    public int insertText(Point at, java.util.List<String> lines) {
        return insertText(at, lines, null);
    }


    /**
     * Flow text into a rectangle.
     *
     * @param rect    box to fill
     * @param text    text to flow
     * @param options layout options, or null for defaults
     * @return the used height
     */
    public double insertTextbox(Rect rect, String text, TextBoxOptions options) {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_page_insert_textbox(doc.requireOpen(), page, rect.toNative(), text,
            null, countOf((String[]) null), options == null ? null : options.toJson(), out), "insertTextbox");
        return out.getValue();
    }

    /**
     * Flow text lines into a rectangle.
     *
     * @param rect    box to fill
     * @param lines   text lines to flow
     * @param options layout options, or null for defaults
     * @return the used height
     */
    public double insertTextbox(Rect rect, java.util.List<String> lines, TextBoxOptions options) {
        String[] array = lines == null ? null : lines.toArray(new String[0]);
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_page_insert_textbox(doc.requireOpen(), page, rect.toNative(), null,
            array, countOf(array), options == null ? null : options.toJson(), out), "insertTextbox");
        return out.getValue();
    }

    /**
     * Flow text into a rectangle with default options.
     *
     * @param rect box to fill
     * @param text text to flow
     * @return the used height
     */
    public double insertTextbox(Rect rect, String text) {
        return insertTextbox(rect, text, null);
    }

    /**
     * Flow text lines into a rectangle with default options.
     *
     * @param rect  box to fill
     * @param lines text lines to flow
     * @return the used height
     */
    public double insertTextbox(Rect rect, java.util.List<String> lines) {
        return insertTextbox(rect, lines, null);
    }


    /**
     * Place an image on the page.
     *
     * @param rect    box to fill
     * @param options placement options
     * @return the image object number
     */
    public int insertImage(Rect rect, ImageInsertOptions options) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_insert_image(doc.requireOpen(), page, rect.toNative(),
            options.alpha, options.filename, options.height, flag(options.keepProportion),
            options.maskJson, options.oc, flag(options.overlay),
            options.pixmap == null ? null : options.pixmap.requireOpen(), options.rotate,
            options.stream, countOf(options.stream), options.width, options.xref, out), "insertImage");
        return out.getValue();
    }


    /**
     * Register a font for the page.
     *
     * @param options font options
     * @return the font object number
     */
    public int insertFont(InsertFontOptions options) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_insert_font(doc.requireOpen(), page, options.fontname,
            options.fontfile, options.fontbuffer, countOf(options.fontbuffer),
            flag(options.setSimple), options.wmode, options.encoding, out), "insertFont");
        return out.getValue();
    }


    /**
     * Stamp a source page onto this one.
     *
     * @param rect    box to fill
     * @param source  source document
     * @param pageNo  source page number
     * @param options placement options
     * @return the stamp result code
     */
    public int showPdfPage(Rect rect, Document source, int pageNo, ShowPageOptions options) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_show_pdf_page(doc.requireOpen(), page, rect.toNative(),
            source.requireOpen(), pageNo, flag(options.keepProportion), flag(options.overlay),
            options.oc, options.rotate,
            options.clip == null ? null : options.clip.toNative(), out), "showPdfPage");
        return out.getValue();
    }


    /**
     * Mark a quadrilateral for redaction; returns its xref.
     *
     * @param quad    quadrilateral to cover
     * @param options redaction options
     * @return the annotation xref
     */
    public int addRedactAnnot(Quad quad, RedactAnnotOptions options) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_page_add_redact_annot(doc.requireOpen(), page, quad.toNative(),
            options.text, options.fontname, options.fontsize, options.align,
            options.fill, countOf(options.fill), options.textColor, countOf(options.textColor),
            flag(options.crossOut), out), "addRedactAnnot");
        return out.getValue();
    }

}
