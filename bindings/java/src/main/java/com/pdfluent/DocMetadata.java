// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Document metadata fields.
 *
 * <p>Mutable DTO with recorded defaults.
 */
public final class DocMetadata {
    /** Document title. */
    public String title = null;

    /** Document author. */
    public String author = null;

    /** Document subject. */
    public String subject = null;

    /** Keywords. */
    public String keywords = null;

    /** Creating application. */
    public String creator = null;

    /** Producing application. */
    public String producer = null;

    /** Creation date. */
    public String creationDate = null;

    /** Modification date. */
    public String modDate = null;

    /** File format. */
    public String format = null;

    /** Encryption description. */
    public String encryption = null;

    /** Trapped flag. */
    public String trapped = null;

}
