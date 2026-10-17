// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Staged-text flush options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class FlushOptions {
    /** Text colour, if any. */
    public double[] color = null;

    /** Opacity. */
    public double opacity = -1.0;

    /** Draw over content. */
    public boolean overlay = true;

    /** Morph matrix, if any. */
    public double[] morph = null;

    /** Placement matrix, if any. */
    public Matrix matrix = null;

    /** Text rendering mode. */
    public int renderMode = 0;

    /** Optional-content id. */
    public int oc = 0;

}
