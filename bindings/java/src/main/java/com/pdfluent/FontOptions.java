// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Font construction options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class FontOptions {
    /** Font name, if any. */
    public String fontname = null;

    /** Font file, if any. */
    public String fontfile = null;

    /** Font bytes, if any. */
    public byte[] fontbuffer = null;

    /** Script code. */
    public int script = 0;

    /** Language tag, if any. */
    public String language = null;

    /** Character collection. */
    public int ordering = -1;

    /** Bold face. */
    public boolean isBold = false;

    /** Italic face. */
    public boolean isItalic = false;

    /** Serif face. */
    public boolean isSerif = false;

    /** Embed the font. */
    public boolean embed = true;

}
