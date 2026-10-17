// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Pixel colour spaces for pixmaps, by channel count.
 */
public enum Colorspace {
    /** One grey component. */
    GRAY(1),
    /** Red, green, and blue components. */
    RGB(3),
    /** Cyan, magenta, yellow, and black components. */
    CMYK(4);

    /** Channel count, excluding alpha; the native wire code. */
    public final int code;

    Colorspace(int code) {
        this.code = code;
    }

    /**
     * Channel count, excluding alpha.
     *
     * @return the channel count
     */
    public int getN() {
        com.sun.jna.ptr.IntByReference out = new com.sun.jna.ptr.IntByReference();
        CompatSupport.check(CompatSupport.lib().pdf_compat_colorspace_n(code, out), "getN");
        return out.getValue();
    }

    /**
     * Colour space for a native wire code.
     *
     * @param code channel count
     * @return the colour space
     * @throws PdfluentValidationException for an unknown code
     */
    public static Colorspace fromCode(int code) {
        for (Colorspace space : values()) {
            if (space.code == code) {
                return space;
            }
        }
        throw new PdfluentValidationException("unknown colorspace code: " + code);
    }
}
