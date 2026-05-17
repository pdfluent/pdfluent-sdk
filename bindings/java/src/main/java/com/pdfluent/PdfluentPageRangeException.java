package com.pdfluent;

/**
 * Thrown when a page index is out of the valid range {@code [0, pageCount)}.
 *
 * <p>Mirrors {@code PdfluentPageRangeError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::PageOutOfRange} variant.
 *
 * <p>All page indices in the PDFluent Java API are zero-based. A document
 * with {@code n} pages accepts indices {@code 0} through {@code n - 1}.
 *
 * <p>Example:
 * <pre>{@code
 * try {
 *     doc.extractText(doc.getPageCount()); // one past the end
 * } catch (PdfluentPageRangeException e) {
 *     System.err.println("Invalid page: " + e.getMessage());
 * }
 * }</pre>
 */
public class PdfluentPageRangeException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentPageRangeException} with a detail message. */
    public PdfluentPageRangeException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentPageRangeException} with a detail message and a cause. */
    public PdfluentPageRangeException(String message, Throwable cause) {
        super(message, cause);
    }
}
