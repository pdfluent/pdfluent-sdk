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
 * One placed image.
 */
public final class PlacedImage {
    private final Rect bbox;
    private final Matrix transform;
    private final int width;
    private final int height;
    private final int bits;
    private final int size;
    private final int number;
    private final String colorspaceName;
    private final boolean hasMask;
    private final int xres;
    private final int yres;
    private final boolean hasDigest;
    private final byte[] digest;

    /** Build a PlacedImage. */
    public PlacedImage(Rect bbox, Matrix transform, int width, int height, int bits, int size, int number, String colorspaceName, boolean hasMask, int xres, int yres, boolean hasDigest, byte[] digest) {
        this.bbox = bbox;
        this.transform = transform;
        this.width = width;
        this.height = height;
        this.bits = bits;
        this.size = size;
        this.number = number;
        this.colorspaceName = colorspaceName;
        this.hasMask = hasMask;
        this.xres = xres;
        this.yres = yres;
        this.hasDigest = hasDigest;
        this.digest = digest;
    }

    /** Placed box in points. */
    public Rect getBbox() { return bbox; }

    /** Image-to-page matrix. */
    public Matrix getTransform() { return transform; }

    /** Width in pixels. */
    public int getWidth() { return width; }

    /** Height in pixels. */
    public int getHeight() { return height; }

    /** Bits per component. */
    public int getBits() { return bits; }

    /** Payload size in bytes. */
    public int getSize() { return size; }

    /** Sequence number. */
    public int getNumber() { return number; }

    /** Colour-space name. */
    public String getColorspaceName() { return colorspaceName; }

    /** Whether masked. */
    public boolean hasMask() { return hasMask; }

    /** Horizontal resolution. */
    public int getXres() { return xres; }

    /** Vertical resolution. */
    public int getYres() { return yres; }

    /** Whether a digest is present. */
    public boolean hasDigest() { return hasDigest; }

    /** Digest bytes, or empty. */
    public byte[] getDigest() { return digest; }

    static java.util.List<PlacedImage> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_placed_image_list_count(list, count), "read");
        java.util.List<PlacedImage> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PdfCapiLibrary.PdfCompatRect vbbox = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_placed_image_list_bbox(list, i, vbbox), "read");
            Rect fBbox = Rect.fromNative(vbbox);
            PdfCapiLibrary.PdfCompatMatrix vtransform = new PdfCapiLibrary.PdfCompatMatrix();
            check(lib().pdf_compat_placed_image_list_transform(list, i, vtransform), "read");
            Matrix fTransform = Matrix.fromNative(vtransform);
            IntByReference vwidth = new IntByReference();
            check(lib().pdf_compat_placed_image_list_width(list, i, vwidth), "read");
            int fWidth = vwidth.getValue();
            IntByReference vheight = new IntByReference();
            check(lib().pdf_compat_placed_image_list_height(list, i, vheight), "read");
            int fHeight = vheight.getValue();
            IntByReference vbits = new IntByReference();
            check(lib().pdf_compat_placed_image_list_bits(list, i, vbits), "read");
            int fBits = vbits.getValue();
            IntByReference vsize = new IntByReference();
            check(lib().pdf_compat_placed_image_list_size(list, i, vsize), "read");
            int fSize = vsize.getValue();
            IntByReference vnumber = new IntByReference();
            check(lib().pdf_compat_placed_image_list_number(list, i, vnumber), "read");
            int fNumber = vnumber.getValue();
            PointerByReference vcolorspacename = new PointerByReference();
            check(lib().pdf_compat_placed_image_list_colorspace_name(list, i, vcolorspacename), "read");
            String fColorspaceName = readString(vcolorspacename);
            IntByReference vhasmask = new IntByReference();
            check(lib().pdf_compat_placed_image_list_has_mask(list, i, vhasmask), "read");
            boolean fHasMask = vhasmask.getValue() != 0;
            IntByReference vxres = new IntByReference();
            check(lib().pdf_compat_placed_image_list_xres(list, i, vxres), "read");
            int fXres = vxres.getValue();
            IntByReference vyres = new IntByReference();
            check(lib().pdf_compat_placed_image_list_yres(list, i, vyres), "read");
            int fYres = vyres.getValue();
            IntByReference vhasdigest = new IntByReference();
            check(lib().pdf_compat_placed_image_list_has_digest(list, i, vhasdigest), "read");
            boolean fHasDigest = vhasdigest.getValue() != 0;
            PointerByReference vdigest = new PointerByReference();
            NativeLongByReference vdigestLen = new NativeLongByReference();
            check(lib().pdf_compat_placed_image_list_digest(list, i, vdigest, vdigestLen), "read");
            byte[] fDigest = readBytes(vdigest, vdigestLen);
            result.add(new PlacedImage(fBbox, fTransform, fWidth, fHeight, fBits, fSize, fNumber, fColorspaceName, fHasMask, fXres, fYres, fHasDigest, fDigest));
        }
        return result;
    }
}
