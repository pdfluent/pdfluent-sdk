package com.pdfluent;

/**
 * Thrown when license activation or validation fails.
 *
 * <p>Mirrors {@code PdfluentLicenseError} in the Python binding (C2) and
 * {@code PDFluent.PdfluentLicenseException} in the .NET binding (C5).
 *
 * <p>Common causes:
 * <ul>
 *   <li>The license key is empty, malformed, or not valid JSON / Base64-JSON.</li>
 *   <li>The license has expired.</li>
 *   <li>The license is for a different product tier.</li>
 *   <li>The {@code PDFLUENT_LICENSE_KEY} environment variable is unset and no key
 *       was supplied programmatically.</li>
 *   <li>A capability is gated behind a Cargo feature that is not compiled in.</li>
 * </ul>
 *
 * <p>When thrown from the licensing subsystem, {@link #getCode()} always
 * returns one of the canonical C8 codes:
 * <ul>
 *   <li>{@code "E-LICENSE-INVALID"} — bad key string or already-set conflict</li>
 *   <li>{@code "E-LICENSE-FEATURE-NOT-IN-TIER"} — current tier lacks the capability</li>
 *   <li>{@code "E-LICENSE-CAPABILITY-NOT-COMPILED"} — feature flag missing at build</li>
 * </ul>
 * matching the Rust {@code pdfluent::Error} enum and the Python, Node, and .NET
 * bindings.
 */
public class PdfluentLicenseException extends PdfluentException {

    private static final long serialVersionUID = 2L;

    /** Constructs a {@code PdfluentLicenseException} with a detail message and no canonical code. */
    public PdfluentLicenseException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentLicenseException} with a detail message and a cause. */
    public PdfluentLicenseException(String message, Throwable cause) {
        super(message, cause);
    }

    /**
     * Constructs a {@code PdfluentLicenseException} with a detail message and a
     * canonical C8 error code, e.g. {@code "E-LICENSE-INVALID"}.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     */
    public PdfluentLicenseException(String message, String code) {
        super(message, code);
    }

    /**
     * Constructs a {@code PdfluentLicenseException} with a detail message, a
     * canonical C8 error code, and a cause.
     *
     * @param message the detail message
     * @param code    canonical C8 catalogue code
     * @param cause   the cause
     */
    public PdfluentLicenseException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
