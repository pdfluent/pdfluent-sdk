package com.pdfluent;

/**
 * Thrown when a PDF file is corrupt, truncated, or not a valid PDF at all.
 *
 * <p>Mirrors {@code PdfluentParseError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::InvalidPdf} variant.
 *
 * <p>Common causes:
 * <ul>
 *   <li>The byte array does not start with the PDF header {@code %PDF-}.</li>
 *   <li>The cross-reference table is missing or corrupt.</li>
 *   <li>Required dictionaries ({@code /Catalog}, {@code /Pages}) are absent.</li>
 * </ul>
 *
 * <p>Example:
 * <pre>{@code
 * try (PdfluentDocument doc = PdfluentDocument.open(bytes)) {
 *     // ...
 * } catch (PdfluentParseException e) {
 *     log.error("Not a valid PDF: {}", e.getMessage());
 * }
 * }</pre>
 */
public class PdfluentParseException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentParseException} with a detail message. */
    public PdfluentParseException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentParseException} with a detail message and a cause. */
    public PdfluentParseException(String message, Throwable cause) {
        super(message, cause);
    }
}
