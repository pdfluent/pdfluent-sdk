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
 * A compatible pixel buffer.
 */
public final class Pixmap implements AutoCloseable {
    private Pointer handle;

    /**
     * Allocate a pixel buffer.
     *
     * @param colorspace colour space
     * @param bounds     pixel bounds
     * @param alpha      keep an alpha channel
     */
    public Pixmap(Colorspace colorspace, IRect bounds, boolean alpha) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_pixmap_new(colorspace.code, bounds.toNative(), flag(alpha), out), "Pixmap");
        this.handle = out.getValue();
    }

    Pixmap(Pointer handle) {
        this.handle = handle;
    }


    /** Whether an alpha channel is present. */
    public boolean getAlpha() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_alpha(requireOpen(), out), "getAlpha");
        return out.getValue() != 0;
    }

    /** Clear to a value, optionally clipped. */
    public void clearWith(Integer value, IRect bbox) {
        check(lib().pdf_compat_pixmap_clear_with(requireOpen(), value == null ? -1 : value, bbox == null ? null : bbox.toNative()), "clearWith");
    }

    /** Convenience overload with defaults. */
    public void clearWith() {
        clearWith(null, null);
    }

    /** Colour-space code: 1 grey, 3 RGB, 4 CMYK. */
    public int getColorspace() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_colorspace(requireOpen(), out), "getColorspace");
        return out.getValue();
    }

    /** Height in pixels. */
    public int getH() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_h(requireOpen(), out), "getH");
        return out.getValue();
    }

    /** Height in pixels. */
    public int getHeight() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_height(requireOpen(), out), "getHeight");
        return out.getValue();
    }

    /** Pixel bounds. */
    public IRect getIrect() {
        PdfCapiLibrary.PdfCompatIRect out = new PdfCapiLibrary.PdfCompatIRect();
        check(lib().pdf_compat_pixmap_irect(requireOpen(), out), "getIrect");
        return IRect.fromNative(out);
    }

    /** Channel count, excluding alpha. */
    public int getN() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_n(requireOpen(), out), "getN");
        return out.getValue();
    }

    /** A copy of the raw samples. */
    public byte[] getSamples() {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_pixmap_samples(requireOpen(), outData, outLen), "getSamples");
        return readBytes(outData, outLen);
    }

    /** Write the pixmap to a file. */
    public void save(String path, String output, int jpg_quality) {
        check(lib().pdf_compat_pixmap_save(requireOpen(), path, output, jpg_quality), "save");
    }

    /** Convenience overload with defaults. */
    public void save(String path) {
        save(path, null, 95);
    }

    /** Fill a box with a colour. */
    public void setRect(IRect bbox, byte[] color) {
        check(lib().pdf_compat_pixmap_set_rect(requireOpen(), bbox.toNative(), color, countOf(color)), "setRect");
    }

    /** Row stride in bytes. */
    public int getStride() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_stride(requireOpen(), out), "getStride");
        return out.getValue();
    }

    /** Encode the pixmap. */
    public byte[] tobytes(String output, int jpg_quality) {
        PointerByReference outData = new PointerByReference();
        NativeLongByReference outLen = new NativeLongByReference();
        check(lib().pdf_compat_pixmap_tobytes(requireOpen(), output, jpg_quality, outData, outLen), "tobytes");
        return readBytes(outData, outLen);
    }

    /** Convenience overload with defaults. */
    public byte[] tobytes() {
        return tobytes("png", 95);
    }

    /** Width in pixels. */
    public int getW() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_w(requireOpen(), out), "getW");
        return out.getValue();
    }

    /** Width in pixels. */
    public int getWidth() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_pixmap_width(requireOpen(), out), "getWidth");
        return out.getValue();
    }

    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_pixmap_free(handle);
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
            throw new PdfluentException("pixmap is closed");
        }
        return handle;
    }
}
