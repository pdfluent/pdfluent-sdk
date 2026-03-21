package com.xfa.pdf;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Result of a PDF/A compliance validation.
 *
 * <p>Returned by {@link PdfUtils#validatePdfA(String, String)}.
 */
public class ComplianceReport {

    /** Whether the document fully conforms to the requested PDF/A level. */
    public final boolean compliant;

    /** Number of rule violations with severity {@code "error"}. */
    public final int errorCount;

    /** Number of rule violations with severity {@code "warning"}. */
    public final int warningCount;

    /** All reported issues (errors, warnings and info notices). */
    public final List<ComplianceIssue> issues;

    /**
     * @param compliant    {@code true} if the document fully conforms to the requested level
     * @param errorCount   number of error-severity violations
     * @param warningCount number of warning-severity violations
     * @param issues       full list of issues (will be wrapped in an unmodifiable view)
     */
    public ComplianceReport(boolean compliant, int errorCount, int warningCount,
                            List<ComplianceIssue> issues) {
        this.compliant    = compliant;
        this.errorCount   = errorCount;
        this.warningCount = warningCount;
        this.issues       = Collections.unmodifiableList(issues);
    }

    /**
     * Deserialize from the flat String[] returned by the native layer.
     *
     * <p>Format: [compliant, errorCount, warningCount,
     *             rule₀, severity₀, message₀, …]
     */
    static ComplianceReport fromArray(String[] arr) {
        if (arr == null || arr.length < 3) {
            return new ComplianceReport(false, 0, 0, Collections.emptyList());
        }
        boolean compliant    = Boolean.parseBoolean(arr[0]);
        int     errorCount   = Integer.parseInt(arr[1]);
        int     warningCount = Integer.parseInt(arr[2]);

        List<ComplianceIssue> issues = new ArrayList<>();
        for (int i = 3; i + 2 < arr.length; i += 3) {
            issues.add(new ComplianceIssue(arr[i], arr[i + 1], arr[i + 2]));
        }
        return new ComplianceReport(compliant, errorCount, warningCount, issues);
    }

    @Override
    public String toString() {
        return "ComplianceReport{compliant=" + compliant
                + ", errors=" + errorCount
                + ", warnings=" + warningCount
                + ", issues=" + issues.size() + "}";
    }
}
