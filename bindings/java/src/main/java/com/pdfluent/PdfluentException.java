package com.pdfluent;

/** Thrown when a native PDF operation fails. */
public class PdfluentException extends RuntimeException {

    public PdfluentException(String message) {
        super(message);
    }

    public PdfluentException(String message, Throwable cause) {
        super(message, cause);
    }
}
