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
 * One outline entry.
 */
public final class TocEntry {
    private final int level;
    private final String title;
    private final int page;
    private final boolean hasDest;
    private final int destKind;
    private final int destPage;
    private final Point destTarget;
    private final double destZoom;

    /** Build a TocEntry. */
    public TocEntry(int level, String title, int page, boolean hasDest, int destKind, int destPage, Point destTarget, double destZoom) {
        this.level = level;
        this.title = title;
        this.page = page;
        this.hasDest = hasDest;
        this.destKind = destKind;
        this.destPage = destPage;
        this.destTarget = destTarget;
        this.destZoom = destZoom;
    }

    /** Nesting level, from 1. */
    public int getLevel() { return level; }

    /** Entry title. */
    public String getTitle() { return title; }

    /** 1-based destination page. */
    public int getPage() { return page; }

    /** Whether a follow-on destination exists. */
    public boolean hasDest() { return hasDest; }

    /** Destination kind, or -1. */
    public int getDestKind() { return destKind; }

    /** Destination page, or -1. */
    public int getDestPage() { return destPage; }

    /** Destination point, or the origin. */
    public Point getDestTarget() { return destTarget; }

    /** Destination zoom, or 0. */
    public double getDestZoom() { return destZoom; }

    static java.util.List<TocEntry> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_toc_list_count(list, count), "read");
        java.util.List<TocEntry> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            IntByReference vlevel = new IntByReference();
            check(lib().pdf_compat_toc_list_level(list, i, vlevel), "read");
            int fLevel = vlevel.getValue();
            PointerByReference vtitle = new PointerByReference();
            check(lib().pdf_compat_toc_list_title(list, i, vtitle), "read");
            String fTitle = readString(vtitle);
            IntByReference vpage = new IntByReference();
            check(lib().pdf_compat_toc_list_page(list, i, vpage), "read");
            int fPage = vpage.getValue();
            IntByReference vhasdest = new IntByReference();
            check(lib().pdf_compat_toc_list_has_dest(list, i, vhasdest), "read");
            boolean fHasDest = vhasdest.getValue() != 0;
            IntByReference vdestkind = new IntByReference();
            check(lib().pdf_compat_toc_list_dest_kind(list, i, vdestkind), "read");
            int fDestKind = vdestkind.getValue();
            IntByReference vdestpage = new IntByReference();
            check(lib().pdf_compat_toc_list_dest_page(list, i, vdestpage), "read");
            int fDestPage = vdestpage.getValue();
            PdfCapiLibrary.PdfCompatPoint vdesttarget = new PdfCapiLibrary.PdfCompatPoint();
            check(lib().pdf_compat_toc_list_dest_target(list, i, vdesttarget), "read");
            Point fDestTarget = Point.fromNative(vdesttarget);
            DoubleByReference vdestzoom = new DoubleByReference();
            check(lib().pdf_compat_toc_list_dest_zoom(list, i, vdestzoom), "read");
            double fDestZoom = vdestzoom.getValue();
            result.add(new TocEntry(fLevel, fTitle, fPage, fHasDest, fDestKind, fDestPage, fDestTarget, fDestZoom));
        }
        return result;
    }
}
