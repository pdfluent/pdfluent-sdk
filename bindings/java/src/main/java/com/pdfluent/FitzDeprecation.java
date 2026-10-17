// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

/**
 * Compatible failure: a retired spelling was used.
 *
 * <p>One of the recorded exception shapes of the compatible API; raised from
 * the compat failure kind the native layer reports, not from the status
 * code. Extends {@link PdfluentException} so existing handlers keep working.
 */
public class FitzDeprecation extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /**
     * Constructs the exception with a detail message.
     *
     * @param message the detail message
     */
    public FitzDeprecation(String message) {
        super(message);
    }

    /**
     * Constructs the exception with a detail message and a cause.
     *
     * @param message the detail message
     * @param cause   the cause
     */
    public FitzDeprecation(String message, Throwable cause) {
        super(message, cause);
    }
}
