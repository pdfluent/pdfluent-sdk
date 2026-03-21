package com.xfa.pdf;

import org.junit.jupiter.api.Test;

import java.io.InputStream;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Integration tests for {@link PdfDocument} and {@link PdfUtils} via JNI.
 *
 * The native library (libpdf_java) must be on java.library.path at test time.
 * Maven Surefire passes -Djava.library.path=../../target/release automatically.
 */
public class PdfDocumentTest {

    /** Load sample.pdf from test resources into a byte array. */
    private static byte[] samplePdfBytes() throws Exception {
        try (InputStream in = PdfDocumentTest.class.getResourceAsStream("/sample.pdf")) {
            assertNotNull(in, "sample.pdf not found on classpath");
            return in.readAllBytes();
        }
    }

    /** Open a PDF, verify page count is positive. */
    @Test
    void testOpenPdf() throws Exception {
        try (PdfDocument doc = PdfDocument.open(samplePdfBytes())) {
            assertTrue(doc.getPageCount() > 0,
                    "Expected at least one page, got " + doc.getPageCount());
        }
    }

    /** Extract text from page 0, verify it is non-empty. */
    @Test
    void testExtractText() throws Exception {
        try (PdfDocument doc = PdfDocument.open(samplePdfBytes())) {
            String text = doc.extractText(0);
            assertNotNull(text);
            assertFalse(text.isBlank(), "Expected non-empty text on page 0");
        }
    }

    /** getMetadata returns without throwing; null is acceptable for missing keys. */
    @Test
    void testMetadata() throws Exception {
        try (PdfDocument doc = PdfDocument.open(samplePdfBytes())) {
            // At least one standard key must not throw; value may be null
            String producer = doc.getMetadata("Producer");
            String creator  = doc.getMetadata("Creator");
            // We only assert that at least one is non-null for a real PDF
            assertTrue(producer != null || creator != null,
                    "Expected at least one metadata field (Producer or Creator) to be set");
        }
    }

    /** PDF/A compliance check runs and returns a ComplianceReport. */
    @Test
    void testComplianceCheck() throws Exception {
        // Write sample bytes to a temp file — PdfUtils.validatePdfA takes a path
        java.nio.file.Path tmp = java.nio.file.Files.createTempFile("xfa-test-", ".pdf");
        try {
            java.nio.file.Files.write(tmp, samplePdfBytes());
            ComplianceReport report = PdfUtils.validatePdfA(tmp.toString(), "2b");
            assertNotNull(report, "validatePdfA must not return null");
            // errorCount and warningCount must be non-negative
            assertTrue(report.errorCount   >= 0);
            assertTrue(report.warningCount >= 0);
        } finally {
            java.nio.file.Files.deleteIfExists(tmp);
        }
    }

    /** Opening garbage bytes must throw PdfException, not crash the JVM. */
    @Test
    void testInvalidPdf() {
        byte[] garbage = {0x00, 0x01, 0x02, 0x03, 0x04};
        assertThrows(PdfException.class, () -> PdfDocument.open(garbage),
                "Expected PdfException for invalid PDF bytes");
    }
}
