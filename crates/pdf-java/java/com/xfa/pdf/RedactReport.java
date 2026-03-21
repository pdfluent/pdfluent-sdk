package com.xfa.pdf;

/**
 * Summary of a text-redaction operation.
 *
 * <p>Returned by {@link PdfDocument#redactText(int, String)}.
 */
public class RedactReport {

    /** Number of text occurrences found in the search. */
    public final int matchesFound;

    /** Number of rectangular areas that were blacked out. */
    public final int areasRedacted;

    /** Number of pages on which at least one redaction was applied. */
    public final int pagesAffected;

    /**
     * @param matchesFound   number of text occurrences found
     * @param areasRedacted  number of rectangular areas blacked out
     * @param pagesAffected  number of pages on which at least one redaction was applied
     */
    public RedactReport(int matchesFound, int areasRedacted, int pagesAffected) {
        this.matchesFound  = matchesFound;
        this.areasRedacted = areasRedacted;
        this.pagesAffected = pagesAffected;
    }

    @Override
    public String toString() {
        return "RedactReport{matches=" + matchesFound
                + ", redacted=" + areasRedacted
                + ", pages=" + pagesAffected + "}";
    }
}
