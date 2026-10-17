// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.lib;

/**
 * Text-search hits: rectangles, or quadrilaterals when requested.
 */
public final class SearchHits {
    private final boolean quads;
    private final java.util.List<Rect> rects;
    private final java.util.List<Quad> quadList;

    SearchHits(boolean quads, java.util.List<Rect> rects, java.util.List<Quad> quadList) {
        this.quads = quads;
        this.rects = rects;
        this.quadList = quadList;
    }

    /** Whether hits are quadrilaterals rather than rectangles. */
    public boolean isQuads() { return quads; }

    /** Hit rectangles; empty when hits are quadrilaterals. */
    public java.util.List<Rect> getRects() { return rects; }

    /** Hit quadrilaterals; empty when hits are rectangles. */
    public java.util.List<Quad> getQuads() { return quadList; }

    /** Hit count. */
    public int size() {
        return quads ? quadList.size() : rects.size();
    }

    static SearchHits read(Pointer handle) {
        try {
            IntByReference isQuads = new IntByReference();
            check(lib().pdf_compat_search_hits_is_quads(handle, isQuads), "read");
            IntByReference count = new IntByReference();
            check(lib().pdf_compat_search_hits_count(handle, count), "read");
            java.util.List<Rect> rects = new java.util.ArrayList<>();
            java.util.List<Quad> quadList = new java.util.ArrayList<>();
            if (isQuads.getValue() != 0) {
                for (int i = 0; i < count.getValue(); i++) {
                    PdfCapiLibrary.PdfCompatQuad quad = new PdfCapiLibrary.PdfCompatQuad();
                    check(lib().pdf_compat_search_hits_quad_at(handle, i, quad), "read");
                    quadList.add(Quad.fromNative(quad));
                }
                return new SearchHits(true, rects, quadList);
            }
            for (int i = 0; i < count.getValue(); i++) {
                PdfCapiLibrary.PdfCompatRect rect = new PdfCapiLibrary.PdfCompatRect();
                check(lib().pdf_compat_search_hits_rect_at(handle, i, rect), "read");
                rects.add(Rect.fromNative(rect));
            }
            return new SearchHits(false, rects, quadList);
        } finally {
            lib().pdf_compat_search_hits_free(handle);
        }
    }
}
