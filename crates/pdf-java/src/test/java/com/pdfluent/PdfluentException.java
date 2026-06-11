package com.pdfluent;

/**
 * Test-scope copy of the base exception.  Production source lives in
 * bindings/java/src/main/java/com/pdfluent/PdfluentException.java.
 *
 * Kept here so pdf-java tests compile and run without requiring
 * bindings/java on the classpath — the JNI native layer throws this
 * class by name and needs it available at runtime.
 */
public class PdfluentException extends RuntimeException {

    private static final long serialVersionUID = 2L;

    private final String code;

    public PdfluentException(String message) {
        super(message);
        this.code = null;
    }

    public PdfluentException(String message, String code) {
        super(message);
        this.code = code;
    }

    public PdfluentException(String message, String code, Throwable cause) {
        super(message, cause);
        this.code = code;
    }

    public String getCode() {
        return code;
    }
}
