package com.xfa.pdf;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * A PDF document handle backed by the native XFA PDF engine.
 *
 * <p>Usage:
 * <pre>{@code
 * try (PdfDocument doc = PdfDocument.open(Path.of("input.pdf"))) {
 *     System.out.println("Pages: " + doc.getPageCount());
 *     String text = doc.extractText(0);
 *     RenderedImage img = doc.renderPage(0, 150.0);
 * }
 * }</pre>
 *
 * <p>The document must be closed after use. Implements {@link AutoCloseable}
 * for use with try-with-resources.
 */
public class PdfDocument implements AutoCloseable {

    static {
        NativeLoader.load();
    }

    private long handle;

    private PdfDocument(long handle) {
        this.handle = handle;
    }

    // --- Static factory methods ---

    /**
     * Open a PDF document from a file path.
     *
     * @param path path to the PDF file
     * @return a new PdfDocument
     * @throws PdfException if the file cannot be read or parsed
     */
    public static PdfDocument open(Path path) throws PdfException {
        try {
            byte[] data = Files.readAllBytes(path);
            return open(data);
        } catch (IOException e) {
            throw new PdfException("failed to read file: " + e.getMessage(), e);
        }
    }

    /**
     * Open a PDF document from raw bytes.
     *
     * @param data PDF file contents
     * @return a new PdfDocument
     * @throws PdfException if the data cannot be parsed
     */
    public static PdfDocument open(byte[] data) throws PdfException {
        long h = nativeOpen(data);
        if (h == 0) {
            throw new PdfException("failed to open PDF");
        }
        return new PdfDocument(h);
    }

    /**
     * Open a password-protected PDF document.
     *
     * @param data PDF file contents
     * @param password the document password
     * @return a new PdfDocument
     * @throws PdfException if the password is wrong or data cannot be parsed
     */
    public static PdfDocument openWithPassword(byte[] data, String password) throws PdfException {
        long h = nativeOpenWithPassword(data, password);
        if (h == 0) {
            throw new PdfException("failed to open PDF");
        }
        return new PdfDocument(h);
    }

    // --- Instance methods ---

    /** Number of pages in the document. */
    public int getPageCount() {
        ensureOpen();
        return nativePageCount(handle);
    }

    /** Width of a page in PDF points (1/72 inch). */
    public double getPageWidth(int pageIndex) {
        ensureOpen();
        return nativePageWidth(handle, pageIndex);
    }

    /** Height of a page in PDF points (1/72 inch). */
    public double getPageHeight(int pageIndex) {
        ensureOpen();
        return nativePageHeight(handle, pageIndex);
    }

    /** Rotation of a page in degrees (0, 90, 180, 270). */
    public int getPageRotation(int pageIndex) {
        ensureOpen();
        return nativePageRotation(handle, pageIndex);
    }

    /**
     * Extract text from a page.
     *
     * @param pageIndex zero-based page index
     * @return the extracted text, or empty string if no text
     * @throws PdfException if the page index is out of range
     */
    public String extractText(int pageIndex) throws PdfException {
        ensureOpen();
        String text = nativeExtractText(handle, pageIndex);
        return text != null ? text : "";
    }

    /**
     * Render a page to RGBA pixels at the specified DPI.
     *
     * @param pageIndex zero-based page index
     * @param dpi dots per inch (72 = 1:1 with PDF points, 150 = standard, 300 = high quality)
     * @return rendered image with RGBA pixel data
     * @throws PdfException if rendering fails
     */
    public RenderedImage renderPage(int pageIndex, double dpi) throws PdfException {
        ensureOpen();
        byte[] raw = nativeRenderPage(handle, pageIndex, dpi);
        if (raw == null) {
            throw new PdfException("render returned null");
        }
        return decodeRenderedImage(raw);
    }

    /**
     * Render a thumbnail of a page, constrained to a maximum dimension.
     *
     * @param pageIndex zero-based page index
     * @param maxDimension maximum width or height in pixels
     * @return rendered thumbnail image
     * @throws PdfException if rendering fails
     */
    public RenderedImage renderThumbnail(int pageIndex, int maxDimension) throws PdfException {
        ensureOpen();
        byte[] raw = nativeRenderThumbnail(handle, pageIndex, maxDimension);
        if (raw == null) {
            throw new PdfException("thumbnail render returned null");
        }
        return decodeRenderedImage(raw);
    }

    /**
     * Get a metadata value.
     *
     * @param key one of: "Title", "Author", "Subject", "Keywords", "Creator", "Producer"
     * @return the metadata value, or null if not set
     */
    public String getMetadata(String key) {
        ensureOpen();
        return nativeGetMetadata(handle, key);
    }

    /** Number of top-level bookmarks. */
    public int getBookmarkCount() {
        ensureOpen();
        return nativeBookmarkCount(handle);
    }

    /**
     * Search for text across all pages.
     *
     * @param query search string (case-insensitive)
     * @return array of zero-based page indices containing the query
     */
    public int[] searchText(String query) {
        ensureOpen();
        int[] result = (int[]) nativeSearchText(handle, query);
        return result != null ? result : new int[0];
    }

    /** Close the document and free native resources. */
    @Override
    public void close() {
        if (handle != 0) {
            nativeClose(handle);
            handle = 0;
        }
    }

    /** Check whether the document is still open. */
    public boolean isOpen() {
        return handle != 0;
    }

    // --- Private helpers ---

    private void ensureOpen() {
        if (handle == 0) {
            throw new IllegalStateException("PdfDocument is closed");
        }
    }

    private static RenderedImage decodeRenderedImage(byte[] raw) {
        if (raw.length < 8) {
            throw new PdfException("invalid render result (too short)");
        }
        ByteBuffer bb = ByteBuffer.wrap(raw, 0, 8);
        int width = bb.getInt();
        int height = bb.getInt();
        byte[] pixels = new byte[raw.length - 8];
        System.arraycopy(raw, 8, pixels, 0, pixels.length);
        return new RenderedImage(width, height, pixels);
    }

    // --- Native methods ---

    private static native long nativeOpen(byte[] data);
    private static native long nativeOpenWithPassword(byte[] data, String password);
    private static native void nativeClose(long handle);
    private static native int nativePageCount(long handle);
    private static native double nativePageWidth(long handle, int pageIndex);
    private static native double nativePageHeight(long handle, int pageIndex);
    private static native int nativePageRotation(long handle, int pageIndex);
    private static native String nativeExtractText(long handle, int pageIndex);
    private static native byte[] nativeRenderPage(long handle, int pageIndex, double dpi);
    private static native byte[] nativeRenderThumbnail(long handle, int pageIndex, int maxDimension);
    private static native String nativeGetMetadata(long handle, String key);
    private static native int nativeBookmarkCount(long handle);
    private static native Object nativeSearchText(long handle, String query);
}
