package com.pdfluent;

/**
 * Thrown when a page rendering or XFA flatten operation fails.
 *
 * <p>Mirrors {@code PdfluentRenderError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::RenderError} and
 * {@code XfaFlattenFailed} variants.
 *
 * <p>Common causes:
 * <ul>
 *   <li>The rendering engine encounters an unsupported PDF feature on the page.</li>
 *   <li>XFA form flattening fails due to a FormCalc scripting error.</li>
 *   <li>The pixel buffer returned by the native layer is malformed.</li>
 * </ul>
 */
public class PdfluentRenderException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentRenderException} with a detail message. */
    public PdfluentRenderException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentRenderException} with a detail message and a cause. */
    public PdfluentRenderException(String message, Throwable cause) {
        super(message, cause);
    }
}
