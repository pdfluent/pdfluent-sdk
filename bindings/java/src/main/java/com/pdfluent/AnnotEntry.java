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
 * One annotation listing.
 */
public final class AnnotEntry {
    private final int xref;
    private final int typeCode;
    private final String typeName;
    private final Rect rect;

    /** Build a AnnotEntry. */
    public AnnotEntry(int xref, int typeCode, String typeName, Rect rect) {
        this.xref = xref;
        this.typeCode = typeCode;
        this.typeName = typeName;
        this.rect = rect;
    }

    /** Annotation object number. */
    public int getXref() { return xref; }

    /** Annotation type code. */
    public int getTypeCode() { return typeCode; }

    /** Annotation type name. */
    public String getTypeName() { return typeName; }

    /** Annotation box in points. */
    public Rect getRect() { return rect; }

    static java.util.List<AnnotEntry> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_annot_entry_list_count(list, count), "read");
        java.util.List<AnnotEntry> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            IntByReference vxref = new IntByReference();
            check(lib().pdf_compat_annot_entry_list_xref(list, i, vxref), "read");
            int fXref = vxref.getValue();
            IntByReference vtypecode = new IntByReference();
            check(lib().pdf_compat_annot_entry_list_type_code(list, i, vtypecode), "read");
            int fTypeCode = vtypecode.getValue();
            PointerByReference vtypename = new PointerByReference();
            check(lib().pdf_compat_annot_entry_list_type_name(list, i, vtypename), "read");
            String fTypeName = readString(vtypename);
            PdfCapiLibrary.PdfCompatRect vrect = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_annot_entry_list_rect(list, i, vrect), "read");
            Rect fRect = Rect.fromNative(vrect);
            result.add(new AnnotEntry(fXref, fTypeCode, fTypeName, fRect));
        }
        return result;
    }
}
