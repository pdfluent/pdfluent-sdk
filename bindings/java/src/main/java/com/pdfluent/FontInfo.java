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
 * One font reference.
 */
public final class FontInfo {
    private final int xref;
    private final String name;

    /** Build a FontInfo. */
    public FontInfo(int xref, String name) {
        this.xref = xref;
        this.name = name;
    }

    /** Font object number. */
    public int getXref() { return xref; }

    /** Font name. */
    public String getName() { return name; }

    static java.util.List<FontInfo> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_font_info_list_count(list, count), "read");
        java.util.List<FontInfo> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            IntByReference vxref = new IntByReference();
            check(lib().pdf_compat_font_info_list_xref(list, i, vxref), "read");
            int fXref = vxref.getValue();
            PointerByReference vname = new PointerByReference();
            check(lib().pdf_compat_font_info_list_name(list, i, vname), "read");
            String fName = readString(vname);
            result.add(new FontInfo(fXref, fName));
        }
        return result;
    }
}
