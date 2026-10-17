// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Annotation appearance options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class AnnotUpdateOptions {
    /** Blend mode, if any. */
    public String blendMode = null;

    /** Opacity, if any. */
    public Double opacity = null;

    /** Font size. */
    public double fontsize = 0.0;

    /** Font name, if any. */
    public String fontname = null;

    /** Text colour, if any. */
    public double[] textColor = null;

    /** Border colour, if any. */
    public double[] borderColor = null;

    /** Fill colour, if any. */
    public double[] fillColor = null;

    /** Cross out. */
    public boolean crossOut = false;

    /** Rotation in degrees. */
    public int rotate = 0;

}
