// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Image-placement options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class ImageInsertOptions {
    /** Alpha selector. */
    public int alpha = -1;

    /** Image file, if any. */
    public String filename = null;

    /** Display height. */
    public double height = 0.0;

    /** Keep the aspect ratio. */
    public boolean keepProportion = true;

    /** Mask value as JSON, if any. */
    public String maskJson = null;

    /** Optional-content id. */
    public int oc = 0;

    /** Draw over content. */
    public boolean overlay = true;

    /** Pixel source, if any. */
    public Pixmap pixmap = null;

    /** Rotation in degrees. */
    public int rotate = 0;

    /** Image bytes, if any. */
    public byte[] stream = null;

    /** Display width. */
    public double width = 0.0;

    /** Image object number. */
    public int xref = 0;

}
