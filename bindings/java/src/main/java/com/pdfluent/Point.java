// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import java.util.Objects;

/**
 * A point in points: an x/y pair.
 */
public final class Point {
    private double x;
    private double y;

    /** Build a point from its coordinates. */
    public Point(double x, double y) {
        this.x = x;
        this.y = y;
    }

    /** Build the origin. */
    public Point() {
        this(0.0, 0.0);
    }

    /** Horizontal position, in points. */
    public double getX() { return x; }

    /** Vertical position, in points. */
    public double getY() { return y; }

    /** Set the horizontal position. */
    public void setX(double x) { this.x = x; }

    /** Set the vertical position. */
    public void setY(double y) { this.y = y; }

    PdfCapiLibrary.PdfCompatPoint toNative() {
        PdfCapiLibrary.PdfCompatPoint nativePoint = new PdfCapiLibrary.PdfCompatPoint();
        nativePoint.x = x;
        nativePoint.y = y;
        return nativePoint;
    }

    static Point fromNative(PdfCapiLibrary.PdfCompatPoint nativePoint) {
        nativePoint.read();
        return new Point(nativePoint.x, nativePoint.y);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Point)) return false;
        Point other = (Point) o;
        return Double.compare(x, other.x) == 0 && Double.compare(y, other.y) == 0;
    }

    @Override
    public int hashCode() {
        return Objects.hash(x, y);
    }

    @Override
    public String toString() {
        return "Point(" + x + ", " + y + ")";
    }
}
