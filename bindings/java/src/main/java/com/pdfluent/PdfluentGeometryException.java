package com.pdfluent;

/**
 * Thrown when an operation encounters an invalid page geometry.
 *
 * <p>Mirrors {@code PdfluentGeometryError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::InvalidPageGeometry} variant.
 *
 * <p>Common causes:
 * <ul>
 *   <li>A page has a zero-area or negative-dimension media box.</li>
 *   <li>A crop box lies entirely outside the media box.</li>
 *   <li>An operation requires a minimum page dimension that the page does not meet.</li>
 * </ul>
 */
public class PdfluentGeometryException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentGeometryException} with a detail message. */
    public PdfluentGeometryException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentGeometryException} with a detail message and a cause. */
    public PdfluentGeometryException(String message, Throwable cause) {
        super(message, cause);
    }

    /**
     * Constructs a {@code PdfluentGeometryException} with a detail message and a
     * canonical C8 error code (see {@link PdfluentException#getCode()}).
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     */
    public PdfluentGeometryException(String message, String code) {
        super(message, code);
    }

    /**
     * Constructs a {@code PdfluentGeometryException} with a detail message, a
     * canonical C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     * @param cause   the cause
     */
    public PdfluentGeometryException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
