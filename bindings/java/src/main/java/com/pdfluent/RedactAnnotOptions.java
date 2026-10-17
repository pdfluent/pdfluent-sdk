// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Redaction-mark options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class RedactAnnotOptions {
    /** Replacement text, if any. */
    public String text = null;

    /** Font name, if any. */
    public String fontname = null;

    /** Font size. */
    public double fontsize = 11.0;

    /** Text alignment. */
    public int align = 0;

    /** Fill colour, if any. */
    public double[] fill = null;

    /** Text colour, if any. */
    public double[] textColor = null;

    /** Cross out instead of covering. */
    public boolean crossOut = true;

}
