package com.xfa.pdf;

/**
 * Exception thrown by PDF operations when the native engine encounters an error.
 */
public class PdfException extends RuntimeException {
    public PdfException(String message) {
        super(message);
    }

    public PdfException(String message, Throwable cause) {
        super(message, cause);
    }
}
