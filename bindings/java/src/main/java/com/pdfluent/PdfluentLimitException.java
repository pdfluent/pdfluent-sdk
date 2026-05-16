package com.pdfluent;

/**
 * Thrown when a resource or processing limit is exceeded.
 *
 * <p>Mirrors {@code PdfluentLimitError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::LimitExceeded} variant.
 *
 * <p>Common causes:
 * <ul>
 *   <li>A document exceeds the maximum number of pages supported by the engine.</li>
 *   <li>A rendering operation would produce a bitmap exceeding the configured memory limit.</li>
 *   <li>A FormCalc script execution depth or iteration count limit is hit.</li>
 * </ul>
 */
public class PdfluentLimitException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentLimitException} with a detail message. */
    public PdfluentLimitException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentLimitException} with a detail message and a cause. */
    public PdfluentLimitException(String message, Throwable cause) {
        super(message, cause);
    }
}
