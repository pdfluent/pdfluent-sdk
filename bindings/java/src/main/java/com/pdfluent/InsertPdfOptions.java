// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Page-import options.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class InsertPdfOptions {
    /** First source page. */
    public int fromPage = -1;

    /** Last source page. */
    public int toPage = -1;

    /** Insertion point. */
    public int startAt = -1;

    /** Rotation. */
    public int rotate = -1;

    /** Copy links. */
    public boolean links = true;

    /** Copy annotations. */
    public boolean annots = true;

    /** Copy widgets. */
    public boolean widgets = true;

    /** Join duplicate resources. */
    public boolean joinDuplicates = false;

    /** Report progress. */
    public boolean showProgress = false;

    /** Finish the import. */
    public boolean finish = true;

}
