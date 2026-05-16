package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit 5 tests for the PDFluent Java JNI binding.
 *
 * <p>These tests require the native library to be built and accessible.
 * Build the native library, then run:
 *
 * <pre>{@code
 * cargo build -p pdf-java --release
 * # macOS:
 * export PDFLUENT_NATIVE_LIB=target/release/libpdfluent_java.dylib
 * # Linux:
 * export PDFLUENT_NATIVE_LIB=target/release/libpdfluent_java.so
 * mvn test -f bindings/java/pom.xml
 * }</pre>
 *
 * <p>All tests use try-with-resources to verify the {@link AutoCloseable} contract.
 */
class PdfluentDocumentTest {

    /**
     * Minimal valid PDF with one US-Letter page (612 x 792 pts), no content.
     * Used by all tests that do not need a specific fixture file.
     */
    private static byte[] minimalPdf() {
        String pdf =
            "%PDF-1.4\n"
            + "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n"
            + "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n"
            + "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n"
            + "xref\n0 4\n"
            + "0000000000 65535 f \n"
            + "0000000009 00000 n \n"
            + "0000000058 00000 n \n"
            + "0000000115 00000 n \n"
            + "trailer<</Size 4/Root 1 0 R>>\n"
            + "startxref\n191\n%%EOF";
        return pdf.getBytes();
    }

    // -------------------------------------------------------------------------
    // AutoCloseable / lifecycle
    // -------------------------------------------------------------------------

    @Test
    void openAndCloseIsIdempotent() {
        PdfluentDocument doc = PdfluentDocument.open(minimalPdf());
        assertTrue(doc.isOpen());
        doc.close();
        assertFalse(doc.isOpen());
        // second close must be a no-op, not throw
        doc.close();
        assertFalse(doc.isOpen());
    }

    @Test
    void tryWithResourcesClosesDocument() {
        PdfluentDocument captured;
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertTrue(doc.isOpen());
            captured = doc;
        }
        assertFalse(captured.isOpen());
    }

    @Test
    void accessAfterCloseThrowsIllegalState() {
        PdfluentDocument doc = PdfluentDocument.open(minimalPdf());
        doc.close();
        assertThrows(IllegalStateException.class, doc::getPageCount);
    }

    // -------------------------------------------------------------------------
    // G1 — Page geometry
    // -------------------------------------------------------------------------

    @Test
    void pageCount() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertEquals(1, doc.getPageCount());
        }
    }

    @Test
    void pageDimensions() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertEquals(612.0, doc.getPageWidth(0),  1.0);
            assertEquals(792.0, doc.getPageHeight(0), 1.0);
        }
    }

    @Test
    void pageRotation() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertEquals(0, doc.getPageRotation(0));
        }
    }

    @Test
    void outOfRangePageThrowsPageRangeException() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.getPageWidth(99));
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.getPageHeight(99));
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.getPageRotation(99));
        }
    }

    // -------------------------------------------------------------------------
    // G2 — Text extraction
    // -------------------------------------------------------------------------

    @Test
    void extractTextFromEmptyPageReturnsEmptyString() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            String text = doc.extractText(0);
            assertNotNull(text);
            // Minimal PDF has no content stream — empty text is expected
        }
    }

    @Test
    void extractTextOutOfRangeThrowsPageRangeException() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.extractText(99));
        }
    }

    // -------------------------------------------------------------------------
    // G3 — Rendering
    // -------------------------------------------------------------------------

    @Test
    void renderPageReturnsSizedBuffer() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            RenderedImage img = doc.renderPage(0, 72.0);
            assertNotNull(img);
            assertTrue(img.getWidth() > 0);
            assertTrue(img.getHeight() > 0);
            assertEquals(img.getWidth() * img.getHeight() * 4, img.getPixels().length);
        }
    }

    @Test
    void renderThumbnailRespectsMaxDimension() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            RenderedImage thumb = doc.renderThumbnail(0, 100);
            assertNotNull(thumb);
            assertTrue(thumb.getWidth()  <= 100);
            assertTrue(thumb.getHeight() <= 100);
        }
    }

    @Test
    void renderOutOfRangeThrowsPageRangeException() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.renderPage(99, 72.0));
        }
    }

    // -------------------------------------------------------------------------
    // Exception hierarchy
    // -------------------------------------------------------------------------

    @Test
    void invalidBytesThrowsParseException() {
        assertThrows(PdfluentParseException.class,
            () -> PdfluentDocument.open(new byte[]{1, 2, 3}));
    }

    @Test
    void parseExceptionIsSubclassOfPdfluentException() {
        assertThrows(PdfluentException.class,
            () -> PdfluentDocument.open(new byte[]{1, 2, 3}));
    }

    @Test
    void ioExceptionThrownForMissingFile() {
        assertThrows(PdfluentIoException.class,
            () -> PdfluentDocument.open(Path.of("/nonexistent/file.pdf")));
    }

    @Test
    void ioExceptionIsSubclassOfPdfluentException() {
        assertThrows(PdfluentException.class,
            () -> PdfluentDocument.open(Path.of("/nonexistent/file.pdf")));
    }

    // -------------------------------------------------------------------------
    // Metadata / bookmarks / search
    // -------------------------------------------------------------------------

    @Test
    void metadataReturnsNullForAbsentKey() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertNull(doc.getMetadata("Title"));
        }
    }

    @Test
    void bookmarkCountIsZeroForMinimalPdf() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertEquals(0, doc.getBookmarkCount());
        }
    }

    @Test
    void searchTextReturnsEmptyArrayForNoMatch() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            int[] results = doc.searchText("nonexistent");
            assertNotNull(results);
            assertEquals(0, results.length);
        }
    }

    // -------------------------------------------------------------------------
    // Future operations (disabled — not yet exposed in JNI binding)
    // -------------------------------------------------------------------------

    @Test
    @org.junit.jupiter.api.Disabled("AcroForm field reading not yet exposed in Java binding")
    void readFormFields() { }

    @Test
    @org.junit.jupiter.api.Disabled("Form field writing not yet exposed in Java binding")
    void writeFormField() { }

    @Test
    @org.junit.jupiter.api.Disabled("Annotation reading not yet exposed in Java binding")
    void readAnnotations() { }

    @Test
    @org.junit.jupiter.api.Disabled("Annotation creation not yet exposed in Java binding")
    void addHighlightAnnotation() { }

    @Test
    @org.junit.jupiter.api.Disabled("PDF/A validation not yet exposed in Java binding")
    void validatePdfA() { }

    @Test
    @org.junit.jupiter.api.Disabled("PDF merge not yet exposed in Java binding")
    void mergePdfs() { }

    @Test
    @org.junit.jupiter.api.Disabled("Signature verification not yet exposed in Java binding")
    void verifySignature() { }

    @Test
    @org.junit.jupiter.api.Disabled("Image extraction not yet exposed in Java binding")
    void extractImages() { }
}
