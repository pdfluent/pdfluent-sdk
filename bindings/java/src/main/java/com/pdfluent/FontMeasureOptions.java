// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Text-measurement options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class FontMeasureOptions {
    /** Font size in points. */
    public double fontsize = 11.0;

    /** Language tag, if any. */
    public String language = null;

    /** Script code. */
    public int script = 0;

    /** Writing mode. */
    public int wmode = 0;

    /** Small capitals. */
    public boolean smallCaps = false;

    /** Fallback selector. */
    public int fallback = 0;

}
