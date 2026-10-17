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
 * One typeset span.
 */
public final class TextTrace {
    private final Rect bbox;
    private final double ascender;
    private final int bidiDir;
    private final int bidiLevel;
    private final java.util.List<TraceChar> chars;

    /** Build a TextTrace. */
    public TextTrace(Rect bbox, double ascender, int bidiDir, int bidiLevel, java.util.List<TraceChar> chars) {
        this.bbox = bbox;
        this.ascender = ascender;
        this.bidiDir = bidiDir;
        this.bidiLevel = bidiLevel;
        this.chars = chars;
    }

    /** Span box in points. */
    public Rect getBbox() { return bbox; }

    /** Ascender height. */
    public double getAscender() { return ascender; }

    /** Writing direction. */
    public int getBidiDir() { return bidiDir; }

    /** Bidi level. */
    public int getBidiLevel() { return bidiLevel; }

    /** Characters. */
    public java.util.List<TraceChar> getChars() { return chars; }

    /** One traced character. */
    public static final class TraceChar {
        private final int codepoint;
        private final int glyph;
        private final Point origin;
        private final Rect bbox;

        /** Build a traced character. */
        public TraceChar(int codepoint, int glyph, Point origin, Rect bbox) {
            this.codepoint = codepoint;
            this.glyph = glyph;
            this.origin = origin;
            this.bbox = bbox;
        }

        /** Character codepoint. */
        public int getCodepoint() { return codepoint; }

        /** Glyph id. */
        public int getGlyph() { return glyph; }

        /** Glyph origin. */
        public Point getOrigin() { return origin; }

        /** Glyph box. */
        public Rect getBbox() { return bbox; }
    }

    static java.util.List<TextTrace> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_text_trace_list_count(list, count), "read");
        java.util.List<TextTrace> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PdfCapiLibrary.PdfCompatRect vbbox = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_text_trace_list_bbox(list, i, vbbox), "read");
            Rect fBbox = Rect.fromNative(vbbox);
            DoubleByReference vascender = new DoubleByReference();
            check(lib().pdf_compat_text_trace_list_ascender(list, i, vascender), "read");
            double fAscender = vascender.getValue();
            IntByReference vbididir = new IntByReference();
            check(lib().pdf_compat_text_trace_list_bidi_dir(list, i, vbididir), "read");
            int fBidiDir = vbididir.getValue();
            IntByReference vbidilevel = new IntByReference();
            check(lib().pdf_compat_text_trace_list_bidi_level(list, i, vbidilevel), "read");
            int fBidiLevel = vbidilevel.getValue();
            IntByReference vcharcount = new IntByReference();
            check(lib().pdf_compat_text_trace_list_char_count(list, i, vcharcount), "read");
            java.util.List<TraceChar> fChars = new java.util.ArrayList<>();
            for (int j = 0; j < vcharcount.getValue(); j++) {
                IntByReference vcp = new IntByReference();
                check(lib().pdf_compat_text_trace_list_char_codepoint(list, i, j, vcp), "read");
                IntByReference vgl = new IntByReference();
                check(lib().pdf_compat_text_trace_list_char_glyph(list, i, j, vgl), "read");
                PdfCapiLibrary.PdfCompatPoint vor = new PdfCapiLibrary.PdfCompatPoint();
                check(lib().pdf_compat_text_trace_list_char_origin(list, i, j, vor), "read");
                PdfCapiLibrary.PdfCompatRect vbx = new PdfCapiLibrary.PdfCompatRect();
                check(lib().pdf_compat_text_trace_list_char_bbox(list, i, j, vbx), "read");
                fChars.add(new TraceChar(vcp.getValue(), vgl.getValue(), Point.fromNative(vor), Rect.fromNative(vbx)));
            }
            result.add(new TextTrace(fBbox, fAscender, fBidiDir, fBidiLevel, fChars));
        }
        return result;
    }
}
