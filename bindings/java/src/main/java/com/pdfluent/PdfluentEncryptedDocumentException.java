package com.pdfluent;

/**
 * Thrown when a password-protected document cannot be opened due to a wrong
 * or missing password, or when an encrypted document is opened without
 * supplying credentials at all.
 *
 * <p>Mirrors {@code PdfluentEncryptedError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::Encrypted} variant.
 *
 * <p>Common causes:
 * <ul>
 *   <li>Calling {@link PdfluentDocument#open(byte[])} on an encrypted PDF without
 *       supplying a password.</li>
 *   <li>Calling {@link PdfluentDocument#openWithPassword(byte[], String)} with an
 *       incorrect password.</li>
 * </ul>
 *
 * <p>Recovery pattern:
 * <pre>{@code
 * try (PdfluentDocument doc = PdfluentDocument.openWithPassword(bytes, password)) {
 *     // ...
 * } catch (PdfluentEncryptedDocumentException e) {
 *     // prompt the user to re-enter the password
 * }
 * }</pre>
 */
public class PdfluentEncryptedDocumentException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentEncryptedDocumentException} with a detail message. */
    public PdfluentEncryptedDocumentException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentEncryptedDocumentException} with a detail message and a cause. */
    public PdfluentEncryptedDocumentException(String message, Throwable cause) {
        super(message, cause);
    }

    /**
     * Constructs a {@code PdfluentEncryptedDocumentException} with a detail message and a
     * canonical C8 error code (see {@link PdfluentException#getCode()}).
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     */
    public PdfluentEncryptedDocumentException(String message, String code) {
        super(message, code);
    }

    /**
     * Constructs a {@code PdfluentEncryptedDocumentException} with a detail message, a
     * canonical C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     * @param cause   the cause
     */
    public PdfluentEncryptedDocumentException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
