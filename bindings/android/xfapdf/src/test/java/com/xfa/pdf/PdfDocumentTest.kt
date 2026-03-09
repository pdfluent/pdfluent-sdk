package com.xfa.pdf

import org.junit.Assert.*
import org.junit.Test

/**
 * Unit tests for PdfDocument Kotlin/Android bindings.
 *
 * These tests require the native library to be available.
 * Build the native library first:
 *   cargo build -p pdf-java --release
 *   export LD_LIBRARY_PATH=target/release  # Linux
 *   export DYLD_LIBRARY_PATH=target/release  # macOS
 *
 * Then run via Gradle:
 *   cd bindings/android && ./gradlew :xfapdf:test
 */
class PdfDocumentTest {

    private fun createTestPdf(): ByteArray {
        val pdf = "%PDF-1.4\n" +
            "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n" +
            "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n" +
            "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n" +
            "xref\n0 4\n" +
            "0000000000 65535 f \n" +
            "0000000009 00000 n \n" +
            "0000000058 00000 n \n" +
            "0000000115 00000 n \n" +
            "trailer<</Size 4/Root 1 0 R>>\n" +
            "startxref\n191\n%%EOF"
        return pdf.toByteArray(Charsets.US_ASCII)
    }

    @Test
    fun openAndClose() {
        val doc = PdfDocument.open(createTestPdf())
        assertTrue(doc.isOpen)
        doc.close()
        assertFalse(doc.isOpen)
    }

    @Test
    fun usePattern() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertTrue(doc.isOpen)
            assertTrue(doc.pageCount > 0)
        }
    }

    @Test
    fun pageCount() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertEquals(1, doc.pageCount)
        }
    }

    @Test
    fun pageDimensions() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertEquals(612.0, doc.getPageWidth(0), 1.0)
            assertEquals(792.0, doc.getPageHeight(0), 1.0)
        }
    }

    @Test
    fun pageRotation() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertEquals(0, doc.getPageRotation(0))
        }
    }

    @Test
    fun extractTextFromEmptyPage() {
        PdfDocument.open(createTestPdf()).use { doc ->
            val text = doc.extractText(0)
            assertNotNull(text)
        }
    }

    @Test(expected = PdfException::class)
    fun invalidPdfThrows() {
        PdfDocument.open(byteArrayOf(1, 2, 3))
    }

    @Test(expected = IllegalStateException::class)
    fun closedDocumentThrows() {
        val doc = PdfDocument.open(createTestPdf())
        doc.close()
        doc.pageCount // should throw
    }

    @Test
    fun metadataReturnsNullForMissing() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertNull(doc.getMetadata("Title"))
        }
    }

    @Test
    fun bookmarkCount() {
        PdfDocument.open(createTestPdf()).use { doc ->
            assertEquals(0, doc.bookmarkCount)
        }
    }

    @Test
    fun searchTextReturnsEmptyArray() {
        PdfDocument.open(createTestPdf()).use { doc ->
            val results = doc.searchText("nonexistent")
            assertNotNull(results)
            assertEquals(0, results.size)
        }
    }

    @Test
    fun renderPage() {
        PdfDocument.open(createTestPdf()).use { doc ->
            val img = doc.renderPage(0, 72.0)
            assertTrue(img.width > 0)
            assertTrue(img.height > 0)
            assertEquals(img.width * img.height * 4, img.pixels.size)
        }
    }

    @Test
    fun renderThumbnail() {
        PdfDocument.open(createTestPdf()).use { doc ->
            val img = doc.renderThumbnail(0, 100)
            assertTrue(img.width <= 100)
            assertTrue(img.height <= 100)
        }
    }
}
