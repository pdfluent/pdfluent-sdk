// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Page-font registration options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class InsertFontOptions {
    /** Font name. */
    public String fontname = "helv";

    /** Font file, if any. */
    public String fontfile = null;

    /** Font bytes, if any. */
    public byte[] fontbuffer = null;

    /** Use simple positioning. */
    public boolean setSimple = false;

    /** Writing mode. */
    public int wmode = 0;

    /** Text encoding. */
    public int encoding = 0;

}
