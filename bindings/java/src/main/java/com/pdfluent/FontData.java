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
 * Extracted font bytes and facts.
 */
public final class FontData {
    private final String name;
    private final byte[] data;

    /** Build a FontData. */
    public FontData(String name, byte[] data) {
        this.name = name;
        this.data = data;
    }

    /** Font name. */
    public String getName() { return name; }

    /** Font bytes. */
    public byte[] getData() { return data; }

    static FontData read(Pointer handle) {
        Pointer list = handle;
        PointerByReference vname = new PointerByReference();
        check(lib().pdf_compat_font_data_name(list, vname), "read");
        String fName = readString(vname);
        PointerByReference vdata = new PointerByReference();
        NativeLongByReference vdataLen = new NativeLongByReference();
        check(lib().pdf_compat_font_data_data(list, vdata, vdataLen), "read");
        byte[] fData = readBytes(vdata, vdataLen);
        return new FontData(fName, fData);
    }
}
