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
 * A compatible annotation, addressed by document, page, and xref.
 */
public final class Annot {
    private final Document doc;
    private final int page;
    private final int xref;

    /**
     * Address an annotation.
     *
     * @param document owning document
     * @param page 0-based page number
     * @param xref annotation xref
     */
    public Annot(Document document, int page, int xref) {
        this.doc = document;
        this.page = page;
        this.xref = xref;
    }

    /**
     * Push appearance changes.
     *
     * @param options appearance options
     */
    public void update(AnnotUpdateOptions options) {
        check(lib().pdf_compat_annot_update(doc.requireOpen(), page, xref,
            options.blendMode, options.opacity == null ? Double.NaN : options.opacity,
            options.fontsize, options.fontname, options.textColor, countOf(options.textColor),
            options.borderColor, countOf(options.borderColor), options.fillColor,
            countOf(options.fillColor), flag(options.crossOut), options.rotate), "update");
    }


    /** Set stroke and fill colours. */
    public void setColors(double[] stroke, double[] fill) {
        check(lib().pdf_compat_annot_set_colors(doc.requireOpen(), page, xref, stroke, countOf(stroke), fill, countOf(fill)), "setColors");
    }

    /** Convenience overload with defaults. */
    public void update() {
        update(new AnnotUpdateOptions());
    }
}
