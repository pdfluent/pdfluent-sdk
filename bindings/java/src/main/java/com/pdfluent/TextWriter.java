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
 * Staged text for a page rectangle.
 */
public final class TextWriter implements AutoCloseable {
    private Pointer handle;

    /**
     * Stage text for a page rectangle.
     *
     * @param rect page rectangle
     */
    public TextWriter(Rect rect) {
        this(rect, 1.0, null);
    }

    /**
     * Stage text for a page rectangle.
     *
     * @param rect    page rectangle
     * @param opacity opacity
     * @param color   text colour, or null
     */
    public TextWriter(Rect rect, double opacity, double[] color) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_text_writer_new(rect.toNative(), opacity, color, countOf(color), out), "TextWriter");
        this.handle = out.getValue();
    }

    TextWriter(Pointer handle) {
        this.handle = handle;
    }


    /**
     * Appended text span: the used box and the end point.
     */
    public static final class AppendResult {
        /** Used box. */
        public final Rect rect;
        /** End point. */
        public final Point point;

        AppendResult(Rect rect, Point point) {
            this.rect = rect;
            this.point = point;
        }
    }

    /**
     * Append text; returns the used box and end point.
     *
     * @param pos     insertion point
     * @param text    text to append
     * @param options layout options, or null for defaults
     * @return the used box and end point
     */
    public AppendResult append(Point pos, String text, AppendOptions options) {
        Font font = options == null ? null : options.font;
        double fontsize = options == null ? 11.0 : options.fontsize;
        String language = options == null ? null : options.language;
        boolean rightToLeft = options != null && options.rightToLeft;
        boolean smallCaps = options != null && options.smallCaps;
        PdfCapiLibrary.PdfCompatRect rectOut = new PdfCapiLibrary.PdfCompatRect();
        PdfCapiLibrary.PdfCompatPoint pointOut = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_text_writer_append(requireOpen(), pos.toNative(), text,
            font == null ? null : font.requireOpen(), fontsize, language,
            flag(rightToLeft), flag(smallCaps), rectOut, pointOut), "append");
        return new AppendResult(Rect.fromNative(rectOut), Point.fromNative(pointOut));
    }

    /**
     * Append text with default options.
     *
     * @param pos  insertion point
     * @param text text to append
     * @return the used box and end point
     */
    public AppendResult append(Point pos, String text) {
        return append(pos, text, null);
    }


    /**
     * Flush staged text onto a page.
     *
     * @param page    target page
     * @param options flush options
     */
    public void writeText(Page page, FlushOptions options) {
        check(lib().pdf_compat_text_writer_write_text(requireOpen(), page.getDoc().requireOpen(),
            page.getPage(), options.color, countOf(options.color), options.opacity,
            flag(options.overlay), options.morph, countOf(options.morph),
            options.matrix == null ? null : options.matrix.toNative(), options.renderMode,
            options.oc), "writeText");
    }


    /** Convenience overload with defaults. */
    public void writeText(Page page) {
        writeText(page, new FlushOptions());
    }

    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_text_writer_free(handle);
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
            throw new PdfluentException("writer is closed");
        }
        return handle;
    }
}
