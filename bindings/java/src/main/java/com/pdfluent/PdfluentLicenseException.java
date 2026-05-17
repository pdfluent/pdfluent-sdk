package com.pdfluent;

/**
 * Thrown when license activation or validation fails.
 *
 * <p>Mirrors {@code PdfluentLicenseError} in the Python binding (C2).
 *
 * <p>Common causes:
 * <ul>
 *   <li>The license key is empty, malformed, or not valid JSON / Base64-JSON.</li>
 *   <li>The license has expired.</li>
 *   <li>The license is for a different product tier.</li>
 *   <li>The {@code PDFLUENT_LICENSE_KEY} environment variable is unset and no key
 *       was supplied programmatically.</li>
 * </ul>
 */
public class PdfluentLicenseException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Constructs a {@code PdfluentLicenseException} with a detail message. */
    public PdfluentLicenseException(String message) {
        super(message);
    }

    /** Constructs a {@code PdfluentLicenseException} with a detail message and a cause. */
    public PdfluentLicenseException(String message, Throwable cause) {
        super(message, cause);
    }
}
