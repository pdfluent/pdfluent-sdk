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
 * Module-level compatible functions.
 */
public final class Compat {
    private Compat() {}

    /** Typeset width of a string in a base font. */
    public static double getTextLength(String text, String fontname, double fontsize, int encoding) {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_get_text_length(text, fontname, fontsize, encoding, out), "getTextLength");
        return out.getValue();
    }

    /** Convenience overload with defaults. */
    public static double getTextLength(String text) {
        return getTextLength(text, "helv", 11.0, 0);
    }

    /**
     * Named paper size in points: width and height.
     *
     * @param name paper name
     * @return width and height in points
     */
    public static double[] paperSize(String name) {
        DoubleByReference out = new DoubleByReference();
        DoubleByReference out2 = new DoubleByReference();
        check(lib().pdf_compat_paper_size(name, out, out2), "paperSize");
        return new double[] {out.getValue(), out2.getValue()};
    }

}
