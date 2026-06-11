package com.pdfluent;

/**
 * Test-scope copy of the licence exception.  Production source lives in
 * bindings/java/src/main/java/com/pdfluent/PdfluentLicenseException.java.
 */
public class PdfluentLicenseException extends PdfluentException {

    private static final long serialVersionUID = 2L;

    public PdfluentLicenseException(String message) {
        super(message);
    }

    public PdfluentLicenseException(String message, String code) {
        super(message, code);
    }

    public PdfluentLicenseException(String message, String code, Throwable cause) {
        super(message, code, cause);
    }
}
