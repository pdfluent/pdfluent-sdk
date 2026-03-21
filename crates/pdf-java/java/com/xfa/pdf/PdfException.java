package com.xfa.pdf;

/**
 * Thrown when the PDF engine encounters an error.
 */
public class PdfException extends Exception {

    /**
     * @param message human-readable error description
     */
    public PdfException(String message) {
        super(message);
    }

    /**
     * @param message human-readable error description
     * @param cause   the underlying exception
     */
    public PdfException(String message, Throwable cause) {
        super(message, cause);
    }
}
