// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import java.util.Objects;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.lib;

/**
 * A 2D affine matrix: six coefficients in {@code a b c d e f} order.
 */
public final class Matrix {
    private double a;
    private double b;
    private double c;
    private double d;
    private double e;
    private double f;

    /** Build a matrix from its six coefficients. */
    public Matrix(double a, double b, double c, double d, double e, double f) {
        this.a = a;
        this.b = b;
        this.c = c;
        this.d = d;
        this.e = e;
        this.f = f;
    }

    /** Build the zero matrix. */
    public Matrix() {
        this(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    }

    /** Horizontal scale factor. */
    public double getA() { return a; }

    /** Vertical shear factor. */
    public double getB() { return b; }

    /** Horizontal shear factor. */
    public double getC() { return c; }

    /** Vertical scale factor. */
    public double getD() { return d; }

    /** Horizontal translation, in points. */
    public double getE() { return e; }

    /** Vertical translation, in points. */
    public double getF() { return f; }

    /** Set the horizontal scale factor. */
    public void setA(double a) { this.a = a; }

    /** Set the vertical shear factor. */
    public void setB(double b) { this.b = b; }

    /** Set the horizontal shear factor. */
    public void setC(double c) { this.c = c; }

    /** Set the vertical scale factor. */
    public void setD(double d) { this.d = d; }

    /** Set the horizontal translation. */
    public void setE(double e) { this.e = e; }

    /** Set the vertical translation. */
    public void setF(double f) { this.f = f; }

    /**
     * Rotate this matrix in place by a quarter-turn count.
     *
     * @param theta rotation amount
     * @return this matrix, rotated
     */
    public Matrix prerotate(double theta) {
        PdfCapiLibrary.PdfCompatMatrix current = toNative();
        PdfCapiLibrary.PdfCompatMatrix out = new PdfCapiLibrary.PdfCompatMatrix();
        check(lib().pdf_compat_matrix_prerotate(current, theta, out), "prerotate");
        out.read();
        this.a = out.a;
        this.b = out.b;
        this.c = out.c;
        this.d = out.d;
        this.e = out.e;
        this.f = out.f;
        return this;
    }

    PdfCapiLibrary.PdfCompatMatrix toNative() {
        PdfCapiLibrary.PdfCompatMatrix nativeMatrix = new PdfCapiLibrary.PdfCompatMatrix();
        nativeMatrix.a = a;
        nativeMatrix.b = b;
        nativeMatrix.c = c;
        nativeMatrix.d = d;
        nativeMatrix.e = e;
        nativeMatrix.f = f;
        return nativeMatrix;
    }

    static Matrix fromNative(PdfCapiLibrary.PdfCompatMatrix nativeMatrix) {
        nativeMatrix.read();
        return new Matrix(nativeMatrix.a, nativeMatrix.b, nativeMatrix.c,
            nativeMatrix.d, nativeMatrix.e, nativeMatrix.f);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Matrix)) return false;
        Matrix other = (Matrix) o;
        return Double.compare(a, other.a) == 0
            && Double.compare(b, other.b) == 0
            && Double.compare(c, other.c) == 0
            && Double.compare(d, other.d) == 0
            && Double.compare(e, other.e) == 0
            && Double.compare(f, other.f) == 0;
    }

    @Override
    public int hashCode() {
        return Objects.hash(a, b, c, d, e, f);
    }

    @Override
    public String toString() {
        return "Matrix(" + a + ", " + b + ", " + c + ", " + d + ", " + e + ", " + f + ")";
    }
}
