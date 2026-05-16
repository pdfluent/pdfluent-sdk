package com.pdfluent;

/**
 * Base exception for all PDFluent SDK errors.
 *
 * <p>All exceptions thrown by {@link PdfluentDocument} and related classes extend this class,
 * allowing callers to catch the full error surface with a single {@code catch} clause:
 *
 * <pre>{@code
 * try (PdfluentDocument doc = PdfluentDocument.open(path)) {
 *     String text = doc.extractText(0);
 * } catch (PdfluentEncryptedDocumentException e) {
 *     // re-prompt for password
 * } catch (PdfluentIoException e) {
 *     // file unreadable
 * } catch (PdfluentException e) {
 *     // all other PDF errors
 * }
 * }</pre>
 *
 * <h2>Exception Hierarchy</h2>
 * <pre>
 * PdfluentException (extends RuntimeException)
 * ├── PdfluentParseException          — corrupt or invalid PDF structure
 * ├── PdfluentValidationException     — semantic validation failures (merge, annotation type)
 * ├── PdfluentRenderException         — page rendering / XFA flatten failures
 * ├── PdfluentEncryptedDocumentException — wrong or missing password
 * ├── PdfluentPageRangeException      — page index out of bounds
 * ├── PdfluentIoException             — file I/O failures
 * ├── PdfluentLicenseException        — license parse or validation errors
 * ├── PdfluentGeometryException       — invalid page geometry
 * └── PdfluentLimitException          — resource or processing limit exceeded
 * </pre>
 *
 * <h2>Checked vs Unchecked Decision</h2>
 * <p>{@code PdfluentException} extends {@link RuntimeException} (unchecked). Rationale:
 * <ul>
 *   <li>PDF errors are not generally recoverable by caller logic — they indicate invalid
 *       input, resource exhaustion, or licence issues, not transient conditions to retry.</li>
 *   <li>Checked exceptions require every call site to declare {@code throws PdfluentException},
 *       which degrades developer experience significantly in APIs with many PDF operations.</li>
 *   <li>Modern Java library design (Spring, Hibernate, JPA) favours unchecked exceptions
 *       for infrastructure-level errors.</li>
 *   <li>Parity with the Python (C2) and .NET (C5) bindings, which use unchecked exceptions
 *       ({@code RuntimeError} subclasses and {@link RuntimeException} subclasses respectively).
 *       All three bindings mirror the same hierarchy 1:1.</li>
 * </ul>
 * Callers that need to handle specific errors should catch the typed subclass. Callers that
 * want a safety net should catch {@code PdfluentException}.
 *
 * <h2>JNI Exception Bridge</h2>
 * <p>The JNI native layer returns typed error codes via return-value conventions (0 / null
 * signals failure). The Java layer inspects these and throws the most specific subclass it
 * can infer from context. A future milestone will upgrade the native layer to call
 * {@code ThrowNew} with the exact subclass, removing the need for contextual inference.
 */
public class PdfluentException extends RuntimeException {

    private static final long serialVersionUID = 1L;

    /**
     * Constructs a {@code PdfluentException} with a detail message.
     *
     * @param message the detail message
     */
    public PdfluentException(String message) {
        super(message);
    }

    /**
     * Constructs a {@code PdfluentException} with a detail message and a cause.
     *
     * @param message the detail message
     * @param cause   the cause
     */
    public PdfluentException(String message, Throwable cause) {
        super(message, cause);
    }
}
