package com.pdfluent;

/**
 * Thrown when a PDF operation fails a semantic validation check.
 *
 * <p>Mirrors {@code PdfluentValidationError} in the Python binding (C2).
 * Corresponds to validation failures in annotation-type constraints, merge
 * compatibility checks, and similar semantic rules that are distinct from
 * structural parse errors.
 *
 * <p>Common causes:
 * <ul>
 *   <li>Attempting to add an annotation of an incompatible type to a page.</li>
 *   <li>Merging documents with incompatible encryption settings.</li>
 *   <li>Setting a form-field value that violates field validation rules.</li>
 * </ul>
 */
public class PdfluentValidationException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentValidationException} with a detail message. */
    public PdfluentValidationException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentValidationException} with a detail message and a cause. */
    public PdfluentValidationException(String message, Throwable cause) {
        super(message, cause);
    }
}
