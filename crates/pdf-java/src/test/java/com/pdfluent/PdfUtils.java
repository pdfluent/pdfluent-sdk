// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

package com.pdfluent;

/**
 * Test-scoped shim over the static half of {@code libpdfluent_java}
 * ({@code Java_com_pdfluent_PdfUtils_*}). See {@link PdfluentDocument} for why
 * this class exists in the test tree at all. Never shipped.
 */
final class PdfUtils {

    static {
        System.loadLibrary("pdfluent_java");
    }

    private PdfUtils() {}

    /**
     * Validate a file on disk against a PDF/A level ("1b", "2b", "3b", ...).
     *
     * <p>Returns a flat array: {@code [compliant, errorCount, warningCount]}
     * followed by {@code (rule, severity, message)} triples, one per issue.
     */
    static native String[] nativeValidatePdfa(String path, String level);
}
