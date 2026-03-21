package com.xfa.pdf;

/**
 * A single rule violation or warning from a PDF/A compliance check.
 */
public class ComplianceIssue {

    /** The violated rule identifier. */
    public final String rule;

    /**
     * Severity: {@code "error"}, {@code "warning"}, or {@code "info"}.
     */
    public final String severity;

    /** Human-readable description of the violation. */
    public final String message;

    /**
     * @param rule     violated rule identifier
     * @param severity {@code "error"}, {@code "warning"}, or {@code "info"}
     * @param message  human-readable description
     */
    public ComplianceIssue(String rule, String severity, String message) {
        this.rule     = rule;
        this.severity = severity;
        this.message  = message;
    }

    @Override
    public String toString() {
        return "[" + severity.toUpperCase() + "] " + rule + ": " + message;
    }
}
