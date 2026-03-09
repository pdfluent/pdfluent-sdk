package com.xfa.pdf;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit tests for PdfDocument JNI bindings.
 *
 * <p>These tests require the native library to be built and available.
 * Set {@code java.library.path} to the Rust target directory, or
 * set {@code PDF_NATIVE_LIB} to the full path of the shared library.
 *
 * <p>Build the native library first:
 * <pre>{@code
 * cargo build -p pdf-java --release
 * export PDF_NATIVE_LIB=target/release/libpdf_java.dylib  # macOS
 * mvn test -f bindings/java/pom.xml
 * }</pre>
 */
class PdfDocumentTest {

    /** Create a minimal PDF in memory for testing. */
    private static byte[] createTestPdf() {
        // Minimal valid PDF
        String pdf = "%PDF-1.4\n" +
            "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n" +
            "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n" +
            "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n" +
            "xref\n0 4\n" +
            "0000000000 65535 f \n" +
            "0000000009 00000 n \n" +
            "0000000058 00000 n \n" +
            "0000000115 00000 n \n" +
            "trailer<</Size 4/Root 1 0 R>>\n" +
            "startxref\n191\n%%EOF";
        return pdf.getBytes();
    }

    @Test
    void openAndClose() {
        PdfDocument doc = PdfDocument.open(createTestPdf());
        assertTrue(doc.isOpen());
        doc.close();
        assertFalse(doc.isOpen());
    }

    @Test
    void tryWithResources() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertTrue(doc.isOpen());
            assertTrue(doc.getPageCount() > 0);
        }
    }

    @Test
    void pageCount() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertEquals(1, doc.getPageCount());
        }
    }

    @Test
    void pageDimensions() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            double width = doc.getPageWidth(0);
            double height = doc.getPageHeight(0);
            // US Letter size
            assertEquals(612.0, width, 1.0);
            assertEquals(792.0, height, 1.0);
        }
    }

    @Test
    void pageRotation() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertEquals(0, doc.getPageRotation(0));
        }
    }

    @Test
    void extractTextFromEmptyPage() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            String text = doc.extractText(0);
            assertNotNull(text);
        }
    }

    @Test
    void invalidPageIndexThrows() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertThrows(PdfException.class, () -> doc.extractText(99));
        }
    }

    @Test
    void closedDocumentThrows() {
        PdfDocument doc = PdfDocument.open(createTestPdf());
        doc.close();
        assertThrows(IllegalStateException.class, () -> doc.getPageCount());
    }

    @Test
    void invalidPdfThrows() {
        assertThrows(PdfException.class, () -> PdfDocument.open(new byte[]{1, 2, 3}));
    }

    @Test
    void metadataReturnsNullForMissing() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertNull(doc.getMetadata("Title"));
        }
    }

    @Test
    void bookmarkCount() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            assertEquals(0, doc.getBookmarkCount());
        }
    }

    @Test
    void searchTextReturnsEmptyArray() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            int[] results = doc.searchText("nonexistent");
            assertNotNull(results);
            assertEquals(0, results.length);
        }
    }

    @Test
    void renderPage() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            RenderedImage img = doc.renderPage(0, 72.0);
            assertNotNull(img);
            assertTrue(img.getWidth() > 0);
            assertTrue(img.getHeight() > 0);
            assertEquals(img.getWidth() * img.getHeight() * 4, img.getPixels().length);
        }
    }

    @Test
    void renderThumbnail() {
        try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
            RenderedImage img = doc.renderThumbnail(0, 100);
            assertNotNull(img);
            assertTrue(img.getWidth() <= 100);
            assertTrue(img.getHeight() <= 100);
        }
    }
}
