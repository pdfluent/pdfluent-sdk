package com.xfa.pdf;

/**
 * Static utility methods for document-level operations that do not require
 * an open {@link PdfDocument} instance.
 */
public final class PdfUtils {

    static {
        // Same library as PdfDocument, and the same caveat: it loads, but the
        // symbols are Java_com_pdfluent_PdfUtils_*, not Java_com_xfa_pdf_PdfUtils_*.
        System.loadLibrary("pdfluent_java");
    }

    private PdfUtils() {}

    /**
     * Merges multiple PDF files into a single output file.
     *
     * <p>Pages appear in the same order as the input array.
     *
     * @param inputPaths array of absolute or relative paths to the source PDFs
     * @param outputPath destination path for the merged PDF
     * @throws PdfException if any source file cannot be opened or the merge fails
     */
    public static void mergePdfs(String[] inputPaths, String outputPath) throws PdfException {
        nativeMergePdfs(inputPaths, outputPath);
    }

    /**
     * Validates a PDF file against the specified PDF/A conformance level.
     *
     * <p>Supported level strings: {@code "1b"}, {@code "2b"} (default),
     * {@code "3b"}, {@code "1a"}, {@code "2a"}, {@code "3a"},
     * {@code "2u"}, {@code "3u"}, {@code "4"}.
     *
     * @param path  path to the PDF file
     * @param level PDF/A conformance level
     * @return compliance report
     * @throws PdfException if the file cannot be opened
     */
    public static ComplianceReport validatePdfA(String path, String level) throws PdfException {
        String[] raw = nativeValidatePdfa(path, level);
        return ComplianceReport.fromArray(raw);
    }

    // -----------------------------------------------------------------------
    // Native method declarations
    // -----------------------------------------------------------------------

    private static native void     nativeMergePdfs(String[] paths, String outputPath);
    private static native String[] nativeValidatePdfa(String path, String level);
}
