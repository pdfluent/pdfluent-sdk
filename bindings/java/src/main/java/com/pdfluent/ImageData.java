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
 * Extracted image bytes and facts.
 */
public final class ImageData {
    private final int width;
    private final int height;
    private final int bits;
    private final int colorspace;
    private final String colorspaceName;
    private final String ext;
    private final int size;
    private final boolean masked;
    private final int xres;
    private final int yres;
    private final byte[] data;

    /** Build a ImageData. */
    public ImageData(int width, int height, int bits, int colorspace, String colorspaceName, String ext, int size, boolean masked, int xres, int yres, byte[] data) {
        this.width = width;
        this.height = height;
        this.bits = bits;
        this.colorspace = colorspace;
        this.colorspaceName = colorspaceName;
        this.ext = ext;
        this.size = size;
        this.masked = masked;
        this.xres = xres;
        this.yres = yres;
        this.data = data;
    }

    /** Width in pixels. */
    public int getWidth() { return width; }

    /** Height in pixels. */
    public int getHeight() { return height; }

    /** Bits per component. */
    public int getBits() { return bits; }

    /** Colour-space code. */
    public int getColorspace() { return colorspace; }

    /** Colour-space name. */
    public String getColorspaceName() { return colorspaceName; }

    /** File extension. */
    public String getExt() { return ext; }

    /** Payload size in bytes. */
    public int getSize() { return size; }

    /** Whether masked. */
    public boolean isMasked() { return masked; }

    /** Horizontal resolution. */
    public int getXres() { return xres; }

    /** Vertical resolution. */
    public int getYres() { return yres; }

    /** Raw payload bytes. */
    public byte[] getData() { return data; }

    static ImageData read(Pointer handle) {
        Pointer list = handle;
        IntByReference vwidth = new IntByReference();
        check(lib().pdf_compat_image_data_width(list, vwidth), "read");
        int fWidth = vwidth.getValue();
        IntByReference vheight = new IntByReference();
        check(lib().pdf_compat_image_data_height(list, vheight), "read");
        int fHeight = vheight.getValue();
        IntByReference vbits = new IntByReference();
        check(lib().pdf_compat_image_data_bits(list, vbits), "read");
        int fBits = vbits.getValue();
        IntByReference vcolorspace = new IntByReference();
        check(lib().pdf_compat_image_data_colorspace(list, vcolorspace), "read");
        int fColorspace = vcolorspace.getValue();
        PointerByReference vcolorspacename = new PointerByReference();
        check(lib().pdf_compat_image_data_colorspace_name(list, vcolorspacename), "read");
        String fColorspaceName = readString(vcolorspacename);
        PointerByReference vext = new PointerByReference();
        check(lib().pdf_compat_image_data_ext(list, vext), "read");
        String fExt = readString(vext);
        IntByReference vsize = new IntByReference();
        check(lib().pdf_compat_image_data_size(list, vsize), "read");
        int fSize = vsize.getValue();
        IntByReference vmasked = new IntByReference();
        check(lib().pdf_compat_image_data_masked(list, vmasked), "read");
        boolean fMasked = vmasked.getValue() != 0;
        IntByReference vxres = new IntByReference();
        check(lib().pdf_compat_image_data_xres(list, vxres), "read");
        int fXres = vxres.getValue();
        IntByReference vyres = new IntByReference();
        check(lib().pdf_compat_image_data_yres(list, vyres), "read");
        int fYres = vyres.getValue();
        PointerByReference vdata = new PointerByReference();
        NativeLongByReference vdataLen = new NativeLongByReference();
        check(lib().pdf_compat_image_data_data(list, vdata, vdataLen), "read");
        byte[] fData = readBytes(vdata, vdataLen);
        return new ImageData(fWidth, fHeight, fBits, fColorspace, fColorspaceName, fExt, fSize, fMasked, fXres, fYres, fData);
    }
}
