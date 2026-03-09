package com.xfa.pdf

import java.io.Closeable
import java.io.File
import java.nio.ByteBuffer

/**
 * A PDF document backed by the native XFA PDF engine.
 *
 * Usage:
 * ```kotlin
 * PdfDocument.open(pdfBytes).use { doc ->
 *     println("Pages: ${doc.pageCount}")
 *     val text = doc.extractText(0)
 *     val image = doc.renderPage(0, dpi = 150.0)
 *     val bitmap = image.toBitmap()
 * }
 * ```
 *
 * Implements [Closeable] for use with `.use {}`.
 */
class PdfDocument private constructor(
    private var handle: Long
) : Closeable {

    companion object {
        init {
            NativeLoader.load()
        }

        /**
         * Open a PDF from raw bytes.
         *
         * @param data PDF file contents.
         * @return A new PdfDocument.
         * @throws PdfException if the data cannot be parsed.
         */
        @JvmStatic
        fun open(data: ByteArray): PdfDocument {
            val h = nativeOpen(data)
            if (h == 0L) throw PdfException("failed to open PDF")
            return PdfDocument(h)
        }

        /**
         * Open a PDF from a file.
         *
         * @param file The PDF file.
         * @return A new PdfDocument.
         * @throws PdfException if the file cannot be read or parsed.
         */
        @JvmStatic
        fun open(file: File): PdfDocument {
            return open(file.readBytes())
        }

        /**
         * Open a password-protected PDF.
         *
         * @param data PDF file contents.
         * @param password The document password.
         * @return A new PdfDocument.
         * @throws PdfException if the password is wrong or data cannot be parsed.
         */
        @JvmStatic
        fun openWithPassword(data: ByteArray, password: String): PdfDocument {
            val h = nativeOpenWithPassword(data, password)
            if (h == 0L) throw PdfException("failed to open PDF")
            return PdfDocument(h)
        }

        // ---- Native methods ----

        @JvmStatic private external fun nativeOpen(data: ByteArray): Long
        @JvmStatic private external fun nativeOpenWithPassword(data: ByteArray, password: String): Long
        @JvmStatic private external fun nativeClose(handle: Long)
        @JvmStatic private external fun nativePageCount(handle: Long): Int
        @JvmStatic private external fun nativePageWidth(handle: Long, pageIndex: Int): Double
        @JvmStatic private external fun nativePageHeight(handle: Long, pageIndex: Int): Double
        @JvmStatic private external fun nativePageRotation(handle: Long, pageIndex: Int): Int
        @JvmStatic private external fun nativeExtractText(handle: Long, pageIndex: Int): String?
        @JvmStatic private external fun nativeRenderPage(handle: Long, pageIndex: Int, dpi: Double): ByteArray?
        @JvmStatic private external fun nativeRenderThumbnail(handle: Long, pageIndex: Int, maxDimension: Int): ByteArray?
        @JvmStatic private external fun nativeGetMetadata(handle: Long, key: String): String?
        @JvmStatic private external fun nativeBookmarkCount(handle: Long): Int
        @JvmStatic private external fun nativeSearchText(handle: Long, query: String): Any?
    }

    /** Whether the document is still open. */
    val isOpen: Boolean get() = handle != 0L

    /** Number of pages in the document. */
    val pageCount: Int
        get() {
            ensureOpen()
            return nativePageCount(handle)
        }

    /** Number of top-level bookmarks. */
    val bookmarkCount: Int
        get() {
            ensureOpen()
            return nativeBookmarkCount(handle)
        }

    /**
     * Width of a page in PDF points (1/72 inch).
     */
    fun getPageWidth(pageIndex: Int): Double {
        ensureOpen()
        return nativePageWidth(handle, pageIndex)
    }

    /**
     * Height of a page in PDF points (1/72 inch).
     */
    fun getPageHeight(pageIndex: Int): Double {
        ensureOpen()
        return nativePageHeight(handle, pageIndex)
    }

    /**
     * Rotation of a page in degrees (0, 90, 180, 270).
     */
    fun getPageRotation(pageIndex: Int): Int {
        ensureOpen()
        return nativePageRotation(handle, pageIndex)
    }

    /**
     * Extract text from a page.
     *
     * @param pageIndex Zero-based page index.
     * @return The extracted text, or empty string if no text.
     * @throws PdfException if the page index is out of range.
     */
    fun extractText(pageIndex: Int): String {
        ensureOpen()
        return nativeExtractText(handle, pageIndex) ?: ""
    }

    /**
     * Render a page to RGBA pixels at the specified DPI.
     *
     * @param pageIndex Zero-based page index.
     * @param dpi Dots per inch (72 = 1:1, 150 = standard, 300 = high quality).
     * @return Rendered image with RGBA pixel data.
     * @throws PdfException if rendering fails.
     */
    fun renderPage(pageIndex: Int, dpi: Double = 150.0): RenderedImage {
        ensureOpen()
        val raw = nativeRenderPage(handle, pageIndex, dpi)
            ?: throw PdfException("render returned null")
        return decodeRenderedImage(raw)
    }

    /**
     * Render a thumbnail constrained to a maximum dimension.
     *
     * @param pageIndex Zero-based page index.
     * @param maxDimension Maximum width or height in pixels.
     */
    fun renderThumbnail(pageIndex: Int, maxDimension: Int = 200): RenderedImage {
        ensureOpen()
        val raw = nativeRenderThumbnail(handle, pageIndex, maxDimension)
            ?: throw PdfException("thumbnail render returned null")
        return decodeRenderedImage(raw)
    }

    /**
     * Get a metadata value.
     *
     * @param key One of: "Title", "Author", "Subject", "Keywords", "Creator", "Producer".
     * @return The metadata value, or null if not set.
     */
    fun getMetadata(key: String): String? {
        ensureOpen()
        return nativeGetMetadata(handle, key)
    }

    /**
     * Search for text across all pages.
     *
     * @param query Search string (case-insensitive).
     * @return Array of zero-based page indices containing the query.
     */
    fun searchText(query: String): IntArray {
        ensureOpen()
        val result = nativeSearchText(handle, query) as? IntArray
        return result ?: IntArray(0)
    }

    /** Close the document and free native resources. */
    override fun close() {
        if (handle != 0L) {
            nativeClose(handle)
            handle = 0L
        }
    }

    // ---- Private helpers ----

    private fun ensureOpen() {
        if (handle == 0L) throw IllegalStateException("PdfDocument is closed")
    }

    private fun decodeRenderedImage(raw: ByteArray): RenderedImage {
        require(raw.size >= 8) { "invalid render result (too short)" }
        val bb = ByteBuffer.wrap(raw, 0, 8)
        val width = bb.int
        val height = bb.int
        val pixels = raw.copyOfRange(8, raw.size)
        return RenderedImage(width, height, pixels)
    }
}
