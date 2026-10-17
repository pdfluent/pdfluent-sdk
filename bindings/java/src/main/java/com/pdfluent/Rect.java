// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;

import java.util.Objects;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.lib;

/**
 * A rectangle in points: two diagonally opposite corners.
 */
public final class Rect {
    private double x0;
    private double y0;
    private double x1;
    private double y1;

    /** Build a rectangle from its four edges. */
    public Rect(double x0, double y0, double x1, double y1) {
        this.x0 = x0;
        this.y0 = y0;
        this.x1 = x1;
        this.y1 = y1;
    }

    /** Build an empty rectangle at the origin. */
    public Rect() {
        this(0.0, 0.0, 0.0, 0.0);
    }

    /** Left edge, in points. */
    public double getX0() { return x0; }

    /** Top edge, in points. */
    public double getY0() { return y0; }

    /** Right edge, in points. */
    public double getX1() { return x1; }

    /** Bottom edge, in points. */
    public double getY1() { return y1; }

    /** Set the left edge. */
    public void setX0(double x0) { this.x0 = x0; }

    /** Set the top edge. */
    public void setY0(double y0) { this.y0 = y0; }

    /** Set the right edge. */
    public void setX1(double x1) { this.x1 = x1; }

    /** Set the bottom edge. */
    public void setY1(double y1) { this.y1 = y1; }

    /** Rectangle height, in points. */
    public double getHeight() {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_rect_get_height(toNative(), out), "height");
        return out.getValue();
    }

    /** Rectangle width, in points. */
    public double getWidth() {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_rect_get_width(toNative(), out), "width");
        return out.getValue();
    }

    /** Rectangle area, in square points. */
    public double getArea() {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_rect_get_area(toNative(), out), "getArea");
        return out.getValue();
    }

    /**
     * Whether this rectangle contains another.
     *
     * @param other the other rectangle
     * @return true when it contains it
     */
    public boolean contains(Rect other) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_rect_contains(toNative(), other.toNative(), out), "contains");
        return out.getValue() != 0;
    }

    /**
     * Whether this rectangle overlaps another.
     *
     * @param other the other rectangle
     * @return true when they overlap
     */
    public boolean intersects(Rect other) {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_rect_intersects(toNative(), other.toNative(), out), "intersects");
        return out.getValue() != 0;
    }

    /** Whether this rectangle is empty. */
    public boolean isEmpty() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_rect_is_empty(toNative(), out), "isEmpty");
        return out.getValue() != 0;
    }

    /** Whether this rectangle is infinite. */
    public boolean isInfinite() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_rect_is_infinite(toNative(), out), "isInfinite");
        return out.getValue() != 0;
    }

    /**
     * Grow this rectangle in place to include another.
     *
     * @param other the other rectangle
     * @return this rectangle, grown
     */
    public Rect includeRect(Rect other) {
        PdfCapiLibrary.PdfCompatRect current = toNative();
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_rect_include_rect(current, other.toNative(), out), "includeRect");
        assign(out);
        return this;
    }

    /**
     * Intersect this rectangle in place with another.
     *
     * @param other the other rectangle
     * @return this rectangle, intersected
     */
    public Rect intersect(Rect other) {
        PdfCapiLibrary.PdfCompatRect current = toNative();
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_rect_intersect(current, other.toNative(), out), "intersect");
        assign(out);
        return this;
    }

    /**
     * Normalize this rectangle in place.
     *
     * @return this rectangle, normalized
     */
    public Rect normalize() {
        PdfCapiLibrary.PdfCompatRect current = toNative();
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_rect_normalize(current, out), "normalize");
        assign(out);
        return this;
    }

    private void assign(PdfCapiLibrary.PdfCompatRect nativeRect) {
        nativeRect.read();
        this.x0 = nativeRect.x0;
        this.y0 = nativeRect.y0;
        this.x1 = nativeRect.x1;
        this.y1 = nativeRect.y1;
    }

    PdfCapiLibrary.PdfCompatRect toNative() {
        PdfCapiLibrary.PdfCompatRect nativeRect = new PdfCapiLibrary.PdfCompatRect();
        nativeRect.x0 = x0;
        nativeRect.y0 = y0;
        nativeRect.x1 = x1;
        nativeRect.y1 = y1;
        return nativeRect;
    }

    static Rect fromNative(PdfCapiLibrary.PdfCompatRect nativeRect) {
        nativeRect.read();
        return new Rect(nativeRect.x0, nativeRect.y0, nativeRect.x1, nativeRect.y1);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Rect)) return false;
        Rect other = (Rect) o;
        return Double.compare(x0, other.x0) == 0
            && Double.compare(y0, other.y0) == 0
            && Double.compare(x1, other.x1) == 0
            && Double.compare(y1, other.y1) == 0;
    }

    @Override
    public int hashCode() {
        return Objects.hash(x0, y0, x1, y1);
    }

    @Override
    public String toString() {
        return "Rect(" + x0 + ", " + y0 + ", " + x1 + ", " + y1 + ")";
    }
}
