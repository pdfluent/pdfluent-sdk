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

    // ---------- Scenario 5: Read AcroForm fields ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: AcroForm field reading not yet exposed in Java binding")
    void readFormFields() {
        // TODO: try (PdfDocument doc = PdfDocument.open(loadFixture("acroform.pdf"))) {
        //     var fields = doc.getFormFields();
        //     assertFalse(fields.isEmpty());
        // }
    }

    // ---------- Scenario 6: Fill text field, save ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: Form field writing not yet exposed in Java binding")
    void writeFormField() {
        // TODO: try (PdfDocument doc = PdfDocument.open(loadFixture("acroform.pdf"))) {
        //     doc.setFieldValue("name", "test");
        //     byte[] saved = doc.save();
        //     try (PdfDocument doc2 = PdfDocument.open(saved)) {
        //         assertEquals("test", doc2.getFieldValue("name"));
        //     }
        // }
    }

    // ---------- Scenario 7: Read annotations ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: Annotation reading not yet exposed in Java binding")
    void readAnnotations() {
        // TODO: try (PdfDocument doc = PdfDocument.open(loadFixture("sample.pdf"))) {
        //     var annots = doc.getAnnotations(0);
        //     assertNotNull(annots);
        // }
    }

    // ---------- Scenario 8: Add highlight, save ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: Annotation creation not yet exposed in Java binding")
    void addHighlightAnnotation() {
        // TODO: Create highlight annotation, save, reopen, verify
    }

    // ---------- Scenario 9: Validate PDF/A ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: PDF/A validation not yet exposed in Java binding")
    void validatePdfA() {
        // TODO: try (PdfDocument doc = PdfDocument.open(createTestPdf())) {
        //     var report = doc.validatePdfA("2b");
        //     assertNotNull(report);
        // }
    }

    // ---------- Scenario 10: Merge 2 PDFs ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: PDF merge not yet exposed in Java binding")
    void mergePdfs() {
        // TODO: byte[] merged = PdfDocument.merge(pdf1, pdf2);
        //     try (PdfDocument doc = PdfDocument.open(merged)) {
        //         assertEquals(2, doc.getPageCount());
        //     }
    }

    // ---------- Scenario 11: Verify signature ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: Signature verification not yet exposed in Java binding")
    void verifySignature() {
        // TODO: try (PdfDocument doc = PdfDocument.open(loadFixture("signed.pdf"))) {
        //     var sigs = doc.verifySignatures();
        //     assertFalse(sigs.isEmpty());
        // }
    }

    // ---------- Scenario 12: Extract images ----------

    @Test
    @org.junit.jupiter.api.Disabled("TODO: Image extraction not yet exposed in Java binding")
    void extractImages() {
        // TODO: try (PdfDocument doc = PdfDocument.open(loadFixture("sample.pdf"))) {
        //     var images = doc.extractImages(0);
        //     assertNotNull(images);
        // }
    }
}
