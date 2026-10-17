// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.NativeLongByReference;
import com.sun.jna.ptr.PointerByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.countOf;
import static com.pdfluent.CompatSupport.flag;
import static com.pdfluent.CompatSupport.lib;
import static com.pdfluent.CompatSupport.readDoubles;
import static com.pdfluent.CompatSupport.readInts;
import static com.pdfluent.CompatSupport.readString;

/**
 * A compatible document link: owned data shared with insert calls.
 */
public final class Link implements AutoCloseable {
    private Pointer handle;

    /**
     * Build a blank link.
     */
    public Link() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_link_new(out), "Link");
        this.handle = out.getValue();
    }

    Link(Pointer handle) {
        this.handle = handle;
    }

    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_link_free(handle);
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
            throw new PdfluentException("link is closed");
        }
        return handle;
    }

    /** Link box in points. */
    public Rect getRect() {
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_link_rect(requireOpen(), out), "getRect");
        return Rect.fromNative(out);
    }

    /** Set the link box. */
    public void setRect(Rect value) {
        check(lib().pdf_compat_link_set_rect(requireOpen(), value.toNative()), "setRect");
    }

    /** Link kind code. */
    public int getKind() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_kind(requireOpen(), out), "getKind");
        return out.getValue();
    }

    /** Set the link kind code. */
    public void setKind(int value) {
        check(lib().pdf_compat_link_set_kind(requireOpen(), value), "setKind");
    }

    /** Link target URI. */
    public String getUri() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_link_uri(requireOpen(), out), "getUri");
        return readString(out);
    }

    /** Set the link target URI. */
    public void setUri(String value) {
        check(lib().pdf_compat_link_set_uri(requireOpen(), value), "setUri");
    }

    /** Link object number. */
    public int getXref() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_xref(requireOpen(), out), "getXref");
        return out.getValue();
    }

    /** Set the link object number. */
    public void setXref(int value) {
        check(lib().pdf_compat_link_set_xref(requireOpen(), value), "setXref");
    }

    /** Link identifier. */
    public String getId() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_link_id(requireOpen(), out), "getId");
        return readString(out);
    }

    /** Set the link identifier. */
    public void setId(String value) {
        check(lib().pdf_compat_link_set_id(requireOpen(), value), "setId");
    }

    /** Link border width in points. */
    public double getBorderWidth() {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_link_border_width(requireOpen(), out), "getBorderWidth");
        return out.getValue();
    }

    /** Set the link border width. */
    public void setBorderWidth(double value) {
        check(lib().pdf_compat_link_set_border_width(requireOpen(), value), "setBorderWidth");
    }

    /** Link border style, or null when there is none. */
    public String getBorderStyle() {
        IntByReference has = new IntByReference();
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_link_border_style(requireOpen(), has, out), "getBorderStyle");
        String style = readString(out);
        return has.getValue() != 0 ? style : null;
    }

    /** Set the link border style; null clears it. */
    public void setBorderStyle(String value) {
        check(lib().pdf_compat_link_set_border_style(requireOpen(), value), "setBorderStyle");
    }

    /** Link border dashes. */
    public int[] getBorderDashes() {
        PointerByReference data = new PointerByReference();
        NativeLongByReference len = new NativeLongByReference();
        check(lib().pdf_compat_link_border_dashes(requireOpen(), data, len), "getBorderDashes");
        return readInts(data, len);
    }

    /** Set the link border dashes. */
    public void setBorderDashes(int[] values) {
        check(lib().pdf_compat_link_set_border_dashes(requireOpen(), values, countOf(values)),
            "setBorderDashes");
    }

    /** Link border cloud effect level. */
    public int getBorderClouds() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_border_clouds(requireOpen(), out), "getBorderClouds");
        return out.getValue();
    }

    /** Set the link border cloud effect level. */
    public void setBorderClouds(int value) {
        check(lib().pdf_compat_link_set_border_clouds(requireOpen(), value), "setBorderClouds");
    }

    /** Link stroke colour. */
    public double[] getStroke() {
        PointerByReference data = new PointerByReference();
        NativeLongByReference len = new NativeLongByReference();
        check(lib().pdf_compat_link_stroke(requireOpen(), data, len), "getStroke");
        return readDoubles(data, len);
    }

    /** Set the link stroke colour. */
    public void setStroke(double[] values) {
        check(lib().pdf_compat_link_set_stroke(requireOpen(), values, countOf(values)), "setStroke");
    }

    /** Link fill colour. */
    public double[] getFill() {
        PointerByReference data = new PointerByReference();
        NativeLongByReference len = new NativeLongByReference();
        check(lib().pdf_compat_link_fill(requireOpen(), data, len), "getFill");
        return readDoubles(data, len);
    }

    /** Set the link fill colour. */
    public void setFill(double[] values) {
        check(lib().pdf_compat_link_set_fill(requireOpen(), values, countOf(values)), "setFill");
    }

    /** Link flags. */
    public int getFlags() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_flags(requireOpen(), out), "getFlags");
        return out.getValue();
    }

    /** Set the link flags. */
    public void setFlags(int value) {
        check(lib().pdf_compat_link_set_flags(requireOpen(), value), "setFlags");
    }

    /** Whether the link carries a follow-on destination. */
    public boolean hasDest() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_has_dest(requireOpen(), out), "hasDest");
        return out.getValue() != 0;
    }

    /** Follow-on destination page, or -1 when there is none. */
    public int getDestPage() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_dest_page(requireOpen(), out), "getDestPage");
        return out.getValue();
    }

    /** Follow-on destination point, or the origin when there is none. */
    public Point getDestTarget() {
        PdfCapiLibrary.PdfCompatPoint out = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_link_dest_target(requireOpen(), out), "getDestTarget");
        return Point.fromNative(out);
    }

    /** Follow-on destination zoom, or 0 when there is none. */
    public double getDestZoom() {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_link_dest_zoom(requireOpen(), out), "getDestZoom");
        return out.getValue();
    }

    /** Clear the follow-on destination. */
    public void clearDest() {
        check(lib().pdf_compat_link_clear_dest(requireOpen()), "clearDest");
    }

    /** Whether the link chains to another. */
    public boolean hasNext() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_link_has_next(requireOpen(), out), "hasNext");
        return out.getValue() != 0;
    }

    /**
     * The chained link, or null when there is none. The caller owns it.
     *
     * @return the next link, or null
     */
    public Link getNext() {
        if (!hasNext()) {
            return null;
        }
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_link_next(requireOpen(), out), "getNext");
        return new Link(out.getValue());
    }

    /** Chain another link; null clears the chain. */
    public void setNext(Link next) {
        check(lib().pdf_compat_link_set_next(requireOpen(),
            next == null ? null : next.requireOpen()), "setNext");
    }

    static java.util.List<Link> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_link_list_count(list, count), "read");
        java.util.List<Link> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PointerByReference out = new PointerByReference();
            check(lib().pdf_compat_link_list_at(list, i, out), "read");
            result.add(new Link(out.getValue()));
        }
        return result;
    }
}
