// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import java.util.Objects;

/**
 * An integer rectangle: the same corners as {@link Rect}, rounded to pixels.
 */
public final class IRect {
    private int x0;
    private int y0;
    private int x1;
    private int y1;

    /** Build an integer rectangle from its four edges. */
    public IRect(int x0, int y0, int x1, int y1) {
        this.x0 = x0;
        this.y0 = y0;
        this.x1 = x1;
        this.y1 = y1;
    }

    /** Build an empty rectangle at the origin. */
    public IRect() {
        this(0, 0, 0, 0);
    }

    /** Left edge, in pixels. */
    public int getX0() { return x0; }

    /** Top edge, in pixels. */
    public int getY0() { return y0; }

    /** Right edge, in pixels. */
    public int getX1() { return x1; }

    /** Bottom edge, in pixels. */
    public int getY1() { return y1; }

    /** Set the left edge. */
    public void setX0(int x0) { this.x0 = x0; }

    /** Set the top edge. */
    public void setY0(int y0) { this.y0 = y0; }

    /** Set the right edge. */
    public void setX1(int x1) { this.x1 = x1; }

    /** Set the bottom edge. */
    public void setY1(int y1) { this.y1 = y1; }

    PdfCapiLibrary.PdfCompatIRect toNative() {
        PdfCapiLibrary.PdfCompatIRect nativeRect = new PdfCapiLibrary.PdfCompatIRect();
        nativeRect.x0 = x0;
        nativeRect.y0 = y0;
        nativeRect.x1 = x1;
        nativeRect.y1 = y1;
        return nativeRect;
    }

    static IRect fromNative(PdfCapiLibrary.PdfCompatIRect nativeRect) {
        nativeRect.read();
        return new IRect(nativeRect.x0, nativeRect.y0, nativeRect.x1, nativeRect.y1);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof IRect)) return false;
        IRect other = (IRect) o;
        return x0 == other.x0 && y0 == other.y0 && x1 == other.x1 && y1 == other.y1;
    }

    @Override
    public int hashCode() {
        return Objects.hash(x0, y0, x1, y1);
    }

    @Override
    public String toString() {
        return "IRect(" + x0 + ", " + y0 + ", " + x1 + ", " + y1 + ")";
    }
}
