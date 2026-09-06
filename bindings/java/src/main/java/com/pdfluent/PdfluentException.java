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
 * <h2>Canonical C8 Error Codes</h2>
 * <p>Every {@code PdfluentException} carries an optional stable identifier exposed via
 * {@link #getCode()}. The string comes from the C8 error catalogue
 * ({@code docs/error_catalogue.md}) — examples include {@code E-PARSE-INVALID-PDF}
 * and {@code E-WASM-PAGE-OUT-OF-RANGE}. Codes are append-only and frozen once
 * assigned, so callers can safely branch on them without parsing localized
 * message strings:
 * <pre>{@code
 * try {
 *     doc.page(index);
 * } catch (PdfluentPageRangeException e) {
 *     if ("E-WASM-PAGE-OUT-OF-RANGE".equals(e.getCode())) {
 *         // tell the user the page does not exist
 *     }
 * }
 * }</pre>
 *
 * <p>Codes are <i>nullable</i> — older call sites that pre-date C8 still throw the
 * two-argument {@link #PdfluentException(String)} or
 * {@link #PdfluentException(String, Throwable)} constructors, which leave {@code code}
 * as {@code null}. Always null-check before equality testing, or use
 * {@link java.util.Objects#equals(Object, Object)}.
 *
 * <h2>JNI Exception Bridge</h2>
 * <p>The JNI native layer returns typed error codes via return-value conventions (0 / null
 * signals failure). The Java layer inspects these and throws the most specific subclass it
 * can infer from context, populating {@link #getCode()} where a C8 mapping is known.
 */
public class PdfluentException extends RuntimeException {

    private static final long serialVersionUID = 2L;

    /**
     * Canonical stable error code from the C8 catalogue
     * ({@code docs/error_catalogue.md}), e.g. {@code E-LICENSE-INVALID}.
     *
     * <p>May be {@code null} for legacy unmapped call sites; license-class
     * exceptions always populate this when thrown from the licensing
     * subsystem.
     */
    private final String code;

    /**
     * Constructs a {@code PdfluentException} with a detail message and no canonical code.
     *
     * @param message the detail message
     */
    public PdfluentException(String message) {
        super(message);
        this.code = null;
    }

    /**
     * Constructs a {@code PdfluentException} with a detail message and a cause.
     *
     * @param message the detail message
     * @param cause   the cause
     */
    public PdfluentException(String message, Throwable cause) {
        super(message, cause);
        this.code = null;
    }

    /**
     * Constructs a {@code PdfluentException} with a detail message and a canonical
     * C8 error code from {@code docs/error_catalogue.md}.
     *
     * @param message the detail message
     * @param code    canonical stable error code, e.g. {@code "E-LICENSE-INVALID"};
     *                may be {@code null} for unmapped errors
     */
    public PdfluentException(String message, String code) {
        super(message);
        this.code = code;
    }

    /**
     * Constructs a {@code PdfluentException} with a detail message, a canonical
     * C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical stable error code, e.g. {@code "E-LICENSE-INVALID"};
     *                may be {@code null} for unmapped errors
     * @param cause   the cause
     */
    public PdfluentException(String message, String code, Throwable cause) {
        super(message, cause);
        this.code = code;
    }

    /**
     * Return the canonical stable error code from the C8 catalogue, or {@code null}
     * if this exception was constructed without one. See the class-level
     * documentation for the recommended usage pattern.
     *
     * @return the canonical C8 error code (e.g. {@code "E-LICENSE-INVALID"}) or
     *         {@code null} for legacy unmapped errors
     */
    public String getCode() {
        return code;
    }
}
