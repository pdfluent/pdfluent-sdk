// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import java.util.Objects;

/**
 * A quadrilateral: four corners in upper-left, upper-right, lower-left,
 * lower-right order.
 */
public final class Quad {
    private Point ul;
    private Point ur;
    private Point ll;
    private Point lr;

    /** Build a quadrilateral from its four corners. */
    public Quad(Point ul, Point ur, Point ll, Point lr) {
        this.ul = ul;
        this.ur = ur;
        this.ll = ll;
        this.lr = lr;
    }

    /** Build a degenerate quadrilateral at the origin. */
    public Quad() {
        this(new Point(), new Point(), new Point(), new Point());
    }

    /** Upper-left corner. */
    public Point getUl() { return ul; }

    /** Upper-right corner. */
    public Point getUr() { return ur; }

    /** Lower-left corner. */
    public Point getLl() { return ll; }

    /** Lower-right corner. */
    public Point getLr() { return lr; }

    /** Set the upper-left corner. */
    public void setUl(Point ul) { this.ul = ul; }

    /** Set the upper-right corner. */
    public void setUr(Point ur) { this.ur = ur; }

    /** Set the lower-left corner. */
    public void setLl(Point ll) { this.ll = ll; }

    /** Set the lower-right corner. */
    public void setLr(Point lr) { this.lr = lr; }

    PdfCapiLibrary.PdfCompatQuad toNative() {
        PdfCapiLibrary.PdfCompatQuad nativeQuad = new PdfCapiLibrary.PdfCompatQuad();
        nativeQuad.ul.x = ul.getX();
        nativeQuad.ul.y = ul.getY();
        nativeQuad.ur.x = ur.getX();
        nativeQuad.ur.y = ur.getY();
        nativeQuad.ll.x = ll.getX();
        nativeQuad.ll.y = ll.getY();
        nativeQuad.lr.x = lr.getX();
        nativeQuad.lr.y = lr.getY();
        return nativeQuad;
    }

    static Quad fromNative(PdfCapiLibrary.PdfCompatQuad nativeQuad) {
        nativeQuad.read();
        return new Quad(
            new Point(nativeQuad.ul.x, nativeQuad.ul.y),
            new Point(nativeQuad.ur.x, nativeQuad.ur.y),
            new Point(nativeQuad.ll.x, nativeQuad.ll.y),
            new Point(nativeQuad.lr.x, nativeQuad.lr.y));
    }

    static PdfCapiLibrary.PdfCompatQuad[] toNativeArray(Quad[] quads) {
        PdfCapiLibrary.PdfCompatQuad[] out = new PdfCapiLibrary.PdfCompatQuad[quads.length];
        for (int i = 0; i < quads.length; i++) {
            out[i] = quads[i].toNative();
        }
        return out;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Quad)) return false;
        Quad other = (Quad) o;
        return Objects.equals(ul, other.ul)
            && Objects.equals(ur, other.ur)
            && Objects.equals(ll, other.ll)
            && Objects.equals(lr, other.lr);
    }

    @Override
    public int hashCode() {
        return Objects.hash(ul, ur, ll, lr);
    }

    @Override
    public String toString() {
        return "Quad(" + ul + ", " + ur + ", " + ll + ", " + lr + ")";
    }
}
