// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Document open options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class DocumentOptions {
    /** File path, if any. */
    public String filename = null;

    /** File bytes, if any. */
    public byte[] data = null;

    /** File-type hint, if any. */
    public String filetype = null;

    /** Page box for a fresh shell, if any. */
    public Rect rect = null;

    /** Page width for a fresh shell. */
    public double width = 0.0;

    /** Page height for a fresh shell. */
    public double height = 0.0;

    /** Default font size. */
    public double fontsize = 11.0;

    /** Archive value as JSON, if any. */
    public String archiveJson = null;

}
