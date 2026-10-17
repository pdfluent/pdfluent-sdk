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
 * One image reference.
 */
public final class ImageRef {
    private final int xref;
    private final int mask;
    private final int width;
    private final int height;
    private final int bits;
    private final String colorspaceName;
    private final String alt;
    private final String name;
    private final String filter;

    /** Build a ImageRef. */
    public ImageRef(int xref, int mask, int width, int height, int bits, String colorspaceName, String alt, String name, String filter) {
        this.xref = xref;
        this.mask = mask;
        this.width = width;
        this.height = height;
        this.bits = bits;
        this.colorspaceName = colorspaceName;
        this.alt = alt;
        this.name = name;
        this.filter = filter;
    }

    /** Image object number. */
    public int getXref() { return xref; }

    /** Mask object number. */
    public int getMask() { return mask; }

    /** Width in pixels. */
    public int getWidth() { return width; }

    /** Height in pixels. */
    public int getHeight() { return height; }

    /** Bits per component. */
    public int getBits() { return bits; }

    /** Colour-space name. */
    public String getColorspaceName() { return colorspaceName; }

    /** Alternate name. */
    public String getAlt() { return alt; }

    /** Resource name. */
    public String getName() { return name; }

    /** Filter name. */
    public String getFilter() { return filter; }

    static java.util.List<ImageRef> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_image_ref_list_count(list, count), "read");
        java.util.List<ImageRef> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            IntByReference vxref = new IntByReference();
            check(lib().pdf_compat_image_ref_list_xref(list, i, vxref), "read");
            int fXref = vxref.getValue();
            IntByReference vmask = new IntByReference();
            check(lib().pdf_compat_image_ref_list_mask(list, i, vmask), "read");
            int fMask = vmask.getValue();
            IntByReference vwidth = new IntByReference();
            check(lib().pdf_compat_image_ref_list_width(list, i, vwidth), "read");
            int fWidth = vwidth.getValue();
            IntByReference vheight = new IntByReference();
            check(lib().pdf_compat_image_ref_list_height(list, i, vheight), "read");
            int fHeight = vheight.getValue();
            IntByReference vbits = new IntByReference();
            check(lib().pdf_compat_image_ref_list_bits(list, i, vbits), "read");
            int fBits = vbits.getValue();
            PointerByReference vcolorspacename = new PointerByReference();
            check(lib().pdf_compat_image_ref_list_colorspace_name(list, i, vcolorspacename), "read");
            String fColorspaceName = readString(vcolorspacename);
            PointerByReference valt = new PointerByReference();
            check(lib().pdf_compat_image_ref_list_alt(list, i, valt), "read");
            String fAlt = readString(valt);
            PointerByReference vname = new PointerByReference();
            check(lib().pdf_compat_image_ref_list_name(list, i, vname), "read");
            String fName = readString(vname);
            PointerByReference vfilter = new PointerByReference();
            check(lib().pdf_compat_image_ref_list_filter(list, i, vfilter), "read");
            String fFilter = readString(vfilter);
            result.add(new ImageRef(fXref, fMask, fWidth, fHeight, fBits, fColorspaceName, fAlt, fName, fFilter));
        }
        return result;
    }
}
