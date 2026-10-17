// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Page-stamp options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class ShowPageOptions {
    /** Source page number. */
    public int pageNo = 0;

    /** Keep the aspect ratio. */
    public boolean keepProportion = true;

    /** Draw over content. */
    public boolean overlay = true;

    /** Optional-content id. */
    public int oc = 0;

    /** Rotation in degrees. */
    public int rotate = 0;

    /** Clip box, if any. */
    public Rect clip = null;

}
