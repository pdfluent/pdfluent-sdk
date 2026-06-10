package com.pdfluent;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * A PDF document backed by the native PDFluent engine.
 *
 * <h2>Opening a document</h2>
 * <pre>{@code
 * // From a file path
 * try (PdfluentDocument doc = PdfluentDocument.open(Path.of("report.pdf"))) {
 *     System.out.println("Pages: " + doc.getPageCount());
 * }
 *
 * // From bytes
 * byte[] pdfBytes = Files.readAllBytes(path);
 * try (PdfluentDocument doc = PdfluentDocument.open(pdfBytes)) {
 *     String text = doc.extractText(0);
 * }
 *
 * // Password-protected
 * try (PdfluentDocument doc = PdfluentDocument.openWithPassword(pdfBytes, "secret")) {
 *     RenderedImage img = doc.renderPage(0, 150.0);
 * }
 * }</pre>
 *
 * <h2>Resource management</h2>
 * <p>{@code PdfluentDocument} implements {@link AutoCloseable} for use with
 * try-with-resources. The document must be closed after use to release native memory.
 * Calling {@link #close()} more than once is safe — subsequent calls are no-ops.
 *
 * <h2>Thread safety</h2>
 * <p><strong>{@code PdfluentDocument} is NOT thread-safe.</strong><br>
 * The underlying native handle is not protected by any synchronisation primitive.
 * Concurrent access from multiple threads — reads or writes — has undefined behaviour.
 * Applications that need concurrent PDF processing must either:
 * <ul>
 *   <li>Open a separate {@code PdfluentDocument} per thread, or</li>
 *   <li>Guard all access with an external lock (e.g. {@code synchronized} block).</li>
 * </ul>
 * {@link NativeLoader} is thread-safe; native library loading is idempotent and
 * protected by a {@code synchronized} guard.
 * {@link RenderedImage} is immutable and therefore thread-safe after construction.
 *
 * <h2>Exception hierarchy</h2>
 * <p>All methods that can fail throw a subclass of {@link PdfluentException}:
 * <ul>
 *   <li>{@link PdfluentIoException} — file not found, permission denied</li>
 *   <li>{@link PdfluentParseException} — corrupt or non-PDF data</li>
 *   <li>{@link PdfluentEncryptedDocumentException} — wrong or missing password</li>
 *   <li>{@link PdfluentPageRangeException} — page index out of range</li>
 *   <li>{@link PdfluentRenderException} — rendering failure</li>
 * </ul>
 * See {@link PdfluentException} for the full hierarchy and the rationale for using
 * unchecked exceptions.
 *
 * <h2>JNI error handling</h2>
 * <p>Every native method that returns a handle ({@code long}) or an object
 * ({@code byte[]}, {@code String}) uses a null / zero sentinel to signal failure.
 * The Java layer maps each sentinel to the most specific {@link PdfluentException}
 * subclass that can be inferred from the call context. A future milestone will
 * upgrade the native layer to call {@code ThrowNew} with the exact typed exception,
 * removing contextual inference.
 */
public class PdfluentDocument implements AutoCloseable {

    static {
        NativeLoader.load();
    }

    /** Native handle. Zero means the document has been closed. */
    private long handle;

    private PdfluentDocument(long handle) {
        this.handle = handle;
    }

    // =========================================================================
    // Static factory methods
    // =========================================================================

    /**
     * Open a PDF document from a file path.
     *
     * <p>This method reads the entire file into memory and then parses it.
     * For very large files, consider using {@link #open(byte[])} with a
     * pre-allocated buffer.
     *
     * @param path path to the PDF file; must not be {@code null}
     * @return a new, open {@code PdfluentDocument}
     * @throws PdfluentIoException    if the file cannot be read (not found, permission denied)
     * @throws PdfluentParseException if the file contents are not a valid PDF
     */
    public static PdfluentDocument open(Path path) {
        try {
            byte[] data = Files.readAllBytes(path);
            return open(data);
        } catch (IOException e) {
            throw new PdfluentIoException("failed to read file '" + path + "': " + e.getMessage(), e);
        }
    }

    /**
     * Open a PDF document from raw bytes.
     *
     * @param data PDF file contents; must not be {@code null}
     * @return a new, open {@code PdfluentDocument}
     * @throws PdfluentParseException if the data is not a valid PDF
     */
    public static PdfluentDocument open(byte[] data) {
        long h = nativeOpen(data);
        if (h == 0) {
            throw new PdfluentParseException("failed to parse PDF: data is corrupt or not a valid PDF");
        }
        return new PdfluentDocument(h);
    }

    /**
     * Open a password-protected PDF document.
     *
     * @param data     PDF file contents; must not be {@code null}
     * @param password the document password; must not be {@code null}
     * @return a new, open {@code PdfluentDocument}
     * @throws PdfluentEncryptedDocumentException if the password is incorrect or the
     *                                            document cannot be decrypted
     * @throws PdfluentParseException             if the (decrypted) data is not a valid PDF
     */
    public static PdfluentDocument openWithPassword(byte[] data, String password) {
        long h = nativeOpenWithPassword(data, password);
        if (h == 0) {
            throw new PdfluentEncryptedDocumentException(
                "failed to open encrypted PDF: wrong password or unsupported encryption");
        }
        return new PdfluentDocument(h);
    }

    // =========================================================================
    // Page information
    // =========================================================================

    /**
     * Returns the number of pages in the document.
     *
     * @return page count; always {@code >= 1} for a valid PDF
     * @throws IllegalStateException if the document has been closed
     */
    public int getPageCount() {
        ensureOpen();
        return nativePageCount(handle);
    }

    /**
     * Returns the width of the specified page in PDF points (1/72 inch).
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return page width in PDF points
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     */
    public double getPageWidth(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        return nativePageWidth(handle, pageIndex);
    }

    /**
     * Returns the height of the specified page in PDF points (1/72 inch).
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return page height in PDF points
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     */
    public double getPageHeight(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        return nativePageHeight(handle, pageIndex);
    }

    /**
     * Returns the rotation of the specified page in degrees.
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return rotation in degrees; one of {@code 0}, {@code 90}, {@code 180}, {@code 270}
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     */
    public int getPageRotation(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        return nativePageRotation(handle, pageIndex);
    }

    // =========================================================================
    // Text extraction
    // =========================================================================

    /**
     * Extract plain text from the specified page.
     *
     * <p>Text order follows the PDF content stream order, which may not match
     * reading order for complex layouts. For structured extraction, a future
     * milestone will expose a block/span API.
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return extracted text; empty string if the page contains no extractable text
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     */
    public String extractText(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        String text = nativeExtractText(handle, pageIndex);
        if (text == null) {
            throw new PdfluentPageRangeException(
                "page index " + pageIndex + " is out of range [0, " + nativePageCount(handle) + ")");
        }
        return text;
    }

    /**
     * Extract the structured text blocks of a page.
     *
     * <p>Returns an ordered list of {@link TextBlock}s for the given page.
     * Each block carries its bounding box in PDF user-space points (origin
     * = bottom-left) plus the concatenated UTF-8 text. The native array is
     * freed inside this method; the returned objects are pure Java values.
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return an ordered list (may be empty) of {@link TextBlock}
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     * @throws PdfluentException          for engine extraction failures
     */
    public java.util.List<TextBlock> extractTextBlocks(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        String[] flat = nativeExtractTextBlocks(handle, pageIndex);
        if (flat == null || flat.length == 0) {
            return java.util.Collections.emptyList();
        }
        java.util.List<TextBlock> result = new java.util.ArrayList<>(flat.length / 5);
        for (int i = 0; i + 4 < flat.length; i += 5) {
            double x = Double.parseDouble(flat[i]);
            double y = Double.parseDouble(flat[i + 1]);
            double w = Double.parseDouble(flat[i + 2]);
            double h = Double.parseDouble(flat[i + 3]);
            String text = flat[i + 4];
            result.add(new TextBlock(x, y, w, h, text));
        }
        return java.util.Collections.unmodifiableList(result);
    }

    // =========================================================================
    // Rendering
    // =========================================================================

    /**
     * Render the specified page to an RGBA pixel buffer at the given DPI.
     *
     * <p>Common DPI values:
     * <ul>
     *   <li>{@code 72} — 1:1 with PDF points (screen preview)</li>
     *   <li>{@code 150} — standard quality</li>
     *   <li>{@code 300} — print quality</li>
     * </ul>
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @param dpi       dots per inch; must be {@code > 0}
     * @return a {@link RenderedImage} containing RGBA pixel data
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     * @throws PdfluentRenderException    if the rendering engine fails
     */
    public RenderedImage renderPage(int pageIndex, double dpi) {
        ensureOpen();
        checkPageIndex(pageIndex);
        byte[] raw = nativeRenderPage(handle, pageIndex, dpi);
        if (raw == null) {
            throw new PdfluentRenderException(
                "render failed for page " + pageIndex + " at " + dpi + " DPI");
        }
        return decodeRenderedImage(raw);
    }

    /**
     * Render a thumbnail of the specified page, constrained to a maximum dimension.
     *
     * <p>The returned image preserves the page aspect ratio; neither width nor
     * height will exceed {@code maxDimension} pixels.
     *
     * @param pageIndex    zero-based page index; must be in {@code [0, getPageCount())}
     * @param maxDimension maximum width or height in pixels; must be {@code > 0}
     * @return a {@link RenderedImage} with {@code width <= maxDimension} and
     *         {@code height <= maxDimension}
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     * @throws PdfluentRenderException    if the rendering engine fails
     */
    public RenderedImage renderThumbnail(int pageIndex, int maxDimension) {
        ensureOpen();
        checkPageIndex(pageIndex);
        byte[] raw = nativeRenderThumbnail(handle, pageIndex, maxDimension);
        if (raw == null) {
            throw new PdfluentRenderException(
                "thumbnail render failed for page " + pageIndex);
        }
        return decodeRenderedImage(raw);
    }

    // =========================================================================
    // Metadata and document information
    // =========================================================================

    /**
     * Retrieve a metadata value from the PDF document information dictionary.
     *
     * <p>Standard keys: {@code "Title"}, {@code "Author"}, {@code "Subject"},
     * {@code "Keywords"}, {@code "Creator"}, {@code "Producer"},
     * {@code "CreationDate"}, {@code "ModDate"}.
     *
     * @param key the metadata key; case-sensitive
     * @return the metadata value, or {@code null} if the key is not set in the document
     * @throws IllegalStateException if the document has been closed
     */
    public String getMetadata(String key) {
        ensureOpen();
        return nativeGetMetadata(handle, key);
    }

    /**
     * Returns the number of top-level bookmarks (outline entries).
     *
     * @return bookmark count; {@code 0} if the document has no outline
     * @throws IllegalStateException if the document has been closed
     */
    public int getBookmarkCount() {
        ensureOpen();
        return nativeBookmarkCount(handle);
    }

    // =========================================================================
    // Search
    // =========================================================================

    /**
     * Search for text across all pages (case-insensitive).
     *
     * @param query the search string; must not be {@code null}
     * @return array of zero-based page indices where the query was found;
     *         empty array if no matches
     * @throws IllegalStateException if the document has been closed
     */
    public int[] searchText(String query) {
        ensureOpen();
        int[] result = (int[]) nativeSearchText(handle, query);
        return result != null ? result : new int[0];
    }

    // =========================================================================
    // AutoCloseable
    // =========================================================================

    /**
     * Close the document and release native memory.
     *
     * <p>This method is idempotent: calling it more than once has no effect.
     * After {@code close()} returns, {@link #isOpen()} will return {@code false}
     * and any further calls to document methods will throw
     * {@link IllegalStateException}.
     *
     * <p>Prefer try-with-resources over manual {@code close()} calls:
     * <pre>{@code
     * try (PdfluentDocument doc = PdfluentDocument.open(path)) {
     *     // use doc
     * } // close() called automatically, even on exception
     * }</pre>
     */
    @Override
    public void close() {
        if (handle != 0) {
            nativeClose(handle);
            handle = 0;
        }
    }

    /**
     * Returns {@code true} if the document is still open and its native resources
     * have not been freed.
     *
     * @return {@code true} if open; {@code false} if {@link #close()} has been called
     */
    public boolean isOpen() {
        return handle != 0;
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    private void ensureOpen() {
        if (handle == 0) {
            throw new IllegalStateException("PdfluentDocument is closed");
        }
    }

    private void checkPageIndex(int pageIndex) {
        int count = nativePageCount(handle);
        if (pageIndex < 0 || pageIndex >= count) {
            throw new PdfluentPageRangeException(
                "page index " + pageIndex + " is out of range [0, " + count + ")");
        }
    }

    private static RenderedImage decodeRenderedImage(byte[] raw) {
        if (raw.length < 8) {
            throw new PdfluentRenderException(
                "native render result is malformed: expected at least 8 header bytes, got " + raw.length);
        }
        ByteBuffer bb = ByteBuffer.wrap(raw, 0, 8);
        int width = bb.getInt();
        int height = bb.getInt();
        byte[] pixels = new byte[raw.length - 8];
        System.arraycopy(raw, 8, pixels, 0, pixels.length);
        return new RenderedImage(width, height, pixels);
    }

    // =========================================================================
    // Native method declarations
    // =========================================================================

    /*
     * JNI error handling contract:
     *   - nativeOpen / nativeOpenWithPassword return 0 on failure.
     *   - nativeExtractText / nativeRenderPage / nativeRenderThumbnail return null on failure.
     *   - nativePageCount / nativePageWidth / nativePageHeight / nativePageRotation /
     *     nativeBookmarkCount are only called after checkPageIndex() and cannot fail
     *     in a well-formed native implementation.
     *   - nativeSearchText returns null on empty result (treated as empty array).
     *
     * If the native layer calls ThrowNew before returning, the JVM will propagate the
     * pending exception after the native method returns, overriding the null/0 check.
     * Both paths are safe.
     */

    private static native long nativeOpen(byte[] data);
    private static native long nativeOpenWithPassword(byte[] data, String password);
    private static native void nativeClose(long handle);
    private static native int nativePageCount(long handle);
    private static native double nativePageWidth(long handle, int pageIndex);
    private static native double nativePageHeight(long handle, int pageIndex);
    private static native int nativePageRotation(long handle, int pageIndex);
    private static native String nativeExtractText(long handle, int pageIndex);
    private static native String[] nativeExtractTextBlocks(long handle, int pageIndex);
    private static native byte[] nativeRenderPage(long handle, int pageIndex, double dpi);
    private static native byte[] nativeRenderThumbnail(long handle, int pageIndex, int maxDimension);
    private static native String nativeGetMetadata(long handle, String key);
    private static native int nativeBookmarkCount(long handle);
    private static native Object nativeSearchText(long handle, String query);
}
