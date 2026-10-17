// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Page-render options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class RenderOptions {
    /** Render matrix. */
    public Matrix matrix = new Matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /** Resolution, if any. */
    public Double dpi = null;

    /** Colour space, if any. */
    public Colorspace colorspace = null;

    /** Clip box, if any. */
    public Rect clip = null;

    /** Keep an alpha channel. */
    public boolean alpha = false;

    /** Render annotations. */
    public boolean annots = true;

}
