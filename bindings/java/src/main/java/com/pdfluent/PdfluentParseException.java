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

    /**
     * Constructs a {@code PdfluentParseException} with a detail message and a
     * canonical C8 error code (see {@link PdfluentException#getCode()}).
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code (e.g. {@code "E-PARSE-INVALID-PDF"})
     */
    public PdfluentParseException(String message, String code) {
        super(message, code);
    }

    /**
     * Constructs a {@code PdfluentParseException} with a detail message, a
     * canonical C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     * @param cause   the cause
     */
    public PdfluentParseException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
