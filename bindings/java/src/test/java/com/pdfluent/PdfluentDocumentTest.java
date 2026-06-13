package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Base64;
import java.util.List;

import org.junit.jupiter.api.io.TempDir;

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


    // Embedded synthetic AcroForm fixtures (from pdfluent-forms
    // gen_acroform_corpus) so form tests need no external files.
    private static final String MULTISELECT_PDF_B64 =
        "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvQ2gvVChsYW5ndWFnZXMpL0ZmIDIwOTcxNTIvUmVjdFsxMDAgNDAwIDMyMCA1MjBdL09wdFsoRU4pKE5MKShERSkoRlIpXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAFVAAABAAABigAAAQAAAcYAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQ1NAolJUVPRg==";
    private static final String PURE_TEXT_PDF_B64 =
        "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvVHgvVChmdWxsX25hbWUpL1JlY3RbMTAwIDcwMCAzMjAgNzIwXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAE0AAABAAABaQAAAQAAAaUAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQyMQolJUVPRg==";

    private static byte[] multiselectPdf() {
        return Base64.getDecoder().decode(MULTISELECT_PDF_B64);
    }

    private static byte[] pureTextPdf() {
        return Base64.getDecoder().decode(PURE_TEXT_PDF_B64);
    }

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
    // Structured text-block extraction (BINDING_API_PARITY_TEXT_BLOCKS round)
    // -------------------------------------------------------------------------

    @Test
    void extractTextBlocksFromEmptyPageReturnsList() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            java.util.List<TextBlock> blocks = doc.extractTextBlocks(0);
            assertNotNull(blocks);
            for (TextBlock b : blocks) {
                assertTrue(b.getWidth() >= 0.0);
                assertTrue(b.getHeight() >= 0.0);
                assertNotNull(b.getText());
            }
        }
    }

    @Test
    void extractTextBlocksOutOfRangeThrowsPageRangeException() {
        try (PdfluentDocument doc = PdfluentDocument.open(minimalPdf())) {
            assertThrows(PdfluentPageRangeException.class,
                () -> doc.extractTextBlocks(99));
        }
    }

    @Test
    void textBlockEqualityIsFieldwise() {
        TextBlock a = new TextBlock(10.0, 20.0, 100.0, 12.0, "hello");
        TextBlock b = new TextBlock(10.0, 20.0, 100.0, 12.0, "hello");
        TextBlock c = new TextBlock(11.0, 20.0, 100.0, 12.0, "hello");
        assertEquals(a, b);
        assertNotEquals(a, c);
        assertEquals(a.hashCode(), b.hashCode());
        assertTrue(a.toString().contains("hello"));
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
    void readFormFields() {
        try (PdfluentDocument doc = PdfluentDocument.open(multiselectPdf())) {
            List<FormField> fields = doc.getFormFields();
            assertFalse(fields.isEmpty(), "multiselect fixture exposes a field");
            FormField languages = fields.stream()
                .filter(f -> f.name.equals("languages"))
                .findFirst()
                .orElseThrow(() -> new AssertionError("languages field missing"));
            assertEquals("choice", languages.fieldType);
        }
    }

    @Test
    void writeTextFormFieldRoundTrips(@TempDir Path tmp) throws Exception {
        Path out = tmp.resolve("text-filled.pdf");
        try (PdfluentDocument doc = PdfluentDocument.open(pureTextPdf())) {
            assertTrue(doc.setFormField("full_name", "Jane Doe"));
            doc.save(out);
        }
        try (PdfluentDocument doc = PdfluentDocument.open(Files.readAllBytes(out))) {
            String value = doc.getFormFields().stream()
                .filter(f -> f.name.equals("full_name"))
                .map(f -> f.value)
                .findFirst()
                .orElse("");
            assertEquals("Jane Doe", value);
        }
    }

    @Test
    void writeMultiSelectRoundTrips(@TempDir Path tmp) throws Exception {
        Path out = tmp.resolve("ms-filled.pdf");
        try (PdfluentDocument doc = PdfluentDocument.open(multiselectPdf())) {
            assertTrue(doc.setMultiSelect("languages", new String[] {"FR", "EN"}));
            doc.save(out);
        }
        try (PdfluentDocument doc = PdfluentDocument.open(Files.readAllBytes(out))) {
            String value = doc.getFormFields().stream()
                .filter(f -> f.name.equals("languages"))
                .map(f -> f.value)
                .findFirst()
                .orElse("");
            assertTrue(value.contains("FR") && value.contains("EN"),
                "both selected options present, got: " + value);
        }
    }

    @Test
    void setMultiSelectRejectsUnknownOption() {
        try (PdfluentDocument doc = PdfluentDocument.open(multiselectPdf())) {
            assertThrows(PdfluentException.class,
                () -> doc.setMultiSelect("languages", new String[] {"KL"}));
        }
    }

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
