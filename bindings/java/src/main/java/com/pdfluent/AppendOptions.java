// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Text-append options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class AppendOptions {
    /** Font, if any. */
    public Font font = null;

    /** Font size in points. */
    public double fontsize = 11.0;

    /** Language tag, if any. */
    public String language = null;

    /** Right-to-left text. */
    public boolean rightToLeft = false;

    /** Small capitals. */
    public boolean smallCaps = false;

}
