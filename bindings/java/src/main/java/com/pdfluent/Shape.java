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
 * A compatible drawing canvas over one page.
 */
public final class Shape {
    private final Document doc;
    private final int page;

    /**
     * Address a shape canvas.
     *
     * @param document owning document
     * @param page 0-based page number
     */
    public Shape(Document document, int page) {
        this.doc = document;
        this.page = page;
    }

    /** Commit pending marks. */
    public void commit(boolean overlay) {
        check(lib().pdf_compat_shape_commit(doc.requireOpen(), page, flag(overlay)), "commit");
    }

    /** Convenience overload with defaults. */
    public void commit() {
        commit(true);
    }

    /** Queue a line. */
    public Point drawLine(Point p1, Point p2) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_shape_draw_line(doc.requireOpen(), page, p1.toNative(), p2.toNative(), out), "drawLine");
        return Point.fromNative(out);
    }

    /** Queue a rectangle. */
    public Point drawRect(Rect rect, Double radius) {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_shape_draw_rect(doc.requireOpen(), page, rect.toNative(), radius == null ? Double.NaN : radius, out), "drawRect");
        return Point.fromNative(out);
    }

    /** Convenience overload with defaults. */
    public Point drawRect(Rect rect) {
        return drawRect(rect, null);
    }

    /** Stroke and fill the queued path. */
    public void finish(ShapeFinishOptions options) {
        check(lib().pdf_compat_shape_finish(doc.requireOpen(), page, options == null ? null : options.toJson()), "finish");
    }

    /** Convenience overload with defaults. */
    public void finish() {
        finish(null);
    }
}
