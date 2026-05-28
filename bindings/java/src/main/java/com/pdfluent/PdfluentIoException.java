package com.pdfluent;

/**
 * Thrown when a file I/O operation fails.
 *
 * <p>Mirrors {@code PdfluentIoError} in the Python binding (C2).
 * Corresponds to the native {@code EngineError::Io} variant and to
 * {@link java.io.IOException} raised while reading a PDF from the filesystem.
 *
 * <p>Common causes:
 * <ul>
 *   <li>The file path passed to {@link PdfluentDocument#open(java.nio.file.Path)} does
 *       not exist or is not readable.</li>
 *   <li>Disk I/O error while writing the output PDF.</li>
 *   <li>Insufficient permissions to access the file.</li>
 * </ul>
 *
 * <p>The {@link #getCause()} always contains the underlying {@link java.io.IOException}
 * when this exception is thrown from the file-path overload.
 */
public class PdfluentIoException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentIoException} with a detail message. */
    public PdfluentIoException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentIoException} with a detail message and a cause. */
    public PdfluentIoException(String message, Throwable cause) {
        super(message, cause);
    }

    /**
     * Constructs a {@code PdfluentIoException} with a detail message and a
     * canonical C8 error code (see {@link PdfluentException#getCode()}).
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     */
    public PdfluentIoException(String message, String code) {
        super(message, code);
    }

    /**
     * Constructs a {@code PdfluentIoException} with a detail message, a
     * canonical C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     * @param cause   the cause
     */
    public PdfluentIoException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
