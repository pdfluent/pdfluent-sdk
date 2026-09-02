package com.xfa.pdf;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;

/**
 * High-level wrapper around the XFA native PDF engine.
 *
 * <p>Call {@link #open(String)} or {@link #open(byte[])} to load a document,
 * then use it as an {@link AutoCloseable} with try-with-resources to ensure
 * the native handle is released.
 *
 * <pre>{@code
 * try (PdfDocument doc = PdfDocument.open("/path/to/file.pdf")) {
 *     System.out.println("Pages: " + doc.getPageCount());
 * }
 * }</pre>
 */
public class PdfDocument implements AutoCloseable {

    static {
        // Load the native library. The name must match `[lib] name` in
        // crates/pdf-java/Cargo.toml (`pdfluent_java`); the platform-specific
        // file is:
        //   macOS   → libpdfluent_java.dylib
        //   Linux   → libpdfluent_java.so
        //   Windows → pdfluent_java.dll
        // The two were out of step for months ("pdf_java" here, "pdfluent_java"
        // in Cargo.toml), so every test errored with UnsatisfiedLinkError on
        // every platform -- and nothing ran them, so nobody saw it.
        //
        // Loading is as far as this class gets. Its natives resolve to
        // Java_com_xfa_pdf_PdfDocument_*, and the library has exported
        // Java_com_pdfluent_PdfluentDocument_* instead since 18742a72. This
        // pre-rebrand wrapper is kept so it still compiles (see pom.xml); the
        // wrapper that binds is com.pdfluent.PdfluentDocument in bindings/java.
        System.loadLibrary("pdfluent_java");
    }

    /** Opaque pointer to the Rust JniDocument struct. */
    private long handle;

    // -----------------------------------------------------------------------
    // Constructors / factory methods
    // -----------------------------------------------------------------------

    private PdfDocument(long handle) {
        this.handle = handle;
    }

    /**
     * Open a PDF from a file path.
     *
     * @param path absolute or relative path to the PDF file
     * @return opened document
     * @throws PdfException if the file cannot be read or the PDF is invalid
     * @throws IOException  if the file cannot be read
     */
    public static PdfDocument open(String path) throws PdfException, IOException {
        byte[] bytes = Files.readAllBytes(Paths.get(path));
        return open(bytes);
    }

    /**
     * Open a PDF from raw bytes.
     *
     * @param data PDF bytes
     * @return opened document
     * @throws PdfException if the data is not a valid PDF
     */
    public static PdfDocument open(byte[] data) throws PdfException {
        long h = nativeOpen(data);
        if (h == 0) {
            throw new PdfException("failed to open PDF");
        }
        return new PdfDocument(h);
    }

    /**
     * Open an encrypted (password-protected) PDF from a file path.
     *
     * @param path     path to the PDF file
     * @param password user or owner password
     * @return opened document
     * @throws PdfException if the password is incorrect or the PDF is invalid
     * @throws IOException  if the file cannot be read
     */
    public static PdfDocument openWithPassword(String path, String password)
            throws PdfException, IOException {
        byte[] bytes = Files.readAllBytes(Paths.get(path));
        return openWithPassword(bytes, password);
    }

    /**
     * Open an encrypted PDF from raw bytes.
     *
     * @param data     PDF bytes
     * @param password user or owner password
     * @return opened document
     * @throws PdfException if the password is incorrect or the PDF is invalid
     */
    public static PdfDocument openWithPassword(byte[] data, String password)
            throws PdfException {
        long h = nativeOpenWithPassword(data, password);
        if (h == 0) {
            throw new PdfException("failed to open encrypted PDF");
        }
        return new PdfDocument(h);
    }

    // -----------------------------------------------------------------------
    // AutoCloseable
    // -----------------------------------------------------------------------

    @Override
    public void close() {
        if (handle != 0) {
            nativeClose(handle);
            handle = 0;
        }
    }

    // -----------------------------------------------------------------------
    // Page geometry
    // -----------------------------------------------------------------------

    /** Returns the total number of pages. */
    public int getPageCount() {
        checkOpen();
        return nativePageCount(handle);
    }

    /**
     * Returns the width of a page in PDF user-space points (1/72 inch).
     *
     * @param pageIndex 0-based page index
     */
    public double getPageWidth(int pageIndex) throws PdfException {
        checkOpen();
        return nativePageWidth(handle, pageIndex);
    }

    /**
     * Returns the height of a page in PDF user-space points (1/72 inch).
     *
     * @param pageIndex 0-based page index
     */
    public double getPageHeight(int pageIndex) throws PdfException {
        checkOpen();
        return nativePageHeight(handle, pageIndex);
    }

    /**
     * Returns the clockwise rotation of a page in degrees (0, 90, 180, 270).
     *
     * @param pageIndex 0-based page index
     */
    public int getPageRotation(int pageIndex) throws PdfException {
        checkOpen();
        return nativePageRotation(handle, pageIndex);
    }

    // -----------------------------------------------------------------------
    // Text
    // -----------------------------------------------------------------------

    /**
     * Extracts all text from the given page.
     *
     * @param pageIndex 0-based page index
     * @return extracted text, or an empty string if the page has no text
     */
    public String extractText(int pageIndex) throws PdfException {
        checkOpen();
        String text = nativeExtractText(handle, pageIndex);
        return text != null ? text : "";
    }

    /**
     * Searches all pages for the given query string.
     *
     * @param query text to search for
     * @return 0-based indices of pages that contain at least one match
     */
    public int[] searchText(String query) throws PdfException {
        checkOpen();
        int[] result = nativeSearchText(handle, query);
        return result != null ? result : new int[0];
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    /**
     * Renders a page to RGBA pixels at the given DPI.
     *
     * <p>The returned byte array is formatted as:
     * {@code [width:4 bytes BE][height:4 bytes BE][RGBA pixels…]}.
     * Parse with {@link #parsePixelBuffer(byte[])}.
     *
     * @param pageIndex 0-based page index
     * @param dpi       resolution (72 = 1:1 mapping of PDF points to pixels)
     * @return packed pixel buffer
     */
    public byte[] renderPage(int pageIndex, double dpi) throws PdfException {
        checkOpen();
        return nativeRenderPage(handle, pageIndex, dpi);
    }

    /**
     * Renders a thumbnail for a page.
     *
     * @param pageIndex    0-based page index
     * @param maxDimension maximum width or height in pixels
     * @return packed pixel buffer (same format as {@link #renderPage})
     */
    public byte[] renderThumbnail(int pageIndex, int maxDimension) throws PdfException {
        checkOpen();
        return nativeRenderThumbnail(handle, pageIndex, maxDimension);
    }

    /**
     * Extracts width and height from a packed pixel buffer returned by
     * {@link #renderPage} or {@link #renderThumbnail}.
     *
     * @return {@code int[]{width, height}}
     */
    public static int[] parsePixelBuffer(byte[] buf) {
        int w = ((buf[0] & 0xff) << 24) | ((buf[1] & 0xff) << 16)
                | ((buf[2] & 0xff) << 8) | (buf[3] & 0xff);
        int h = ((buf[4] & 0xff) << 24) | ((buf[5] & 0xff) << 16)
                | ((buf[6] & 0xff) << 8) | (buf[7] & 0xff);
        return new int[]{w, h};
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    /**
     * Returns a metadata value.
     *
     * @param key one of {@code "Title"}, {@code "Author"}, {@code "Subject"},
     *            {@code "Keywords"}, {@code "Creator"}, {@code "Producer"}
     * @return the value, or {@code null} if not set
     */
    public String getMetadata(String key) throws PdfException {
        checkOpen();
        return nativeGetMetadata(handle, key);
    }

    /** Returns the number of top-level bookmarks (outline entries). */
    public int getBookmarkCount() {
        checkOpen();
        return nativeBookmarkCount(handle);
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /**
     * Saves the document to the given path.
     *
     * <p>If form fields were written or annotations were added, the mutated
     * document is saved; otherwise the original bytes are written unchanged.
     *
     * @param path destination file path
     */
    public void save(String path) throws PdfException {
        checkOpen();
        nativeSave(handle, path);
    }

    // -----------------------------------------------------------------------
    // Forms
    // -----------------------------------------------------------------------

    /**
     * Returns all interactive form fields in the document.
     *
     * @return list of fields, empty if the document has no AcroForm
     */
    public List<FormField> getFormFields() throws PdfException {
        checkOpen();
        String[] raw = nativeGetFormFields(handle);
        List<FormField> fields = new ArrayList<>();
        if (raw == null) return fields;
        // Stride 4: name, type, value, page
        for (int i = 0; i + 3 < raw.length; i += 4) {
            int page = Integer.parseInt(raw[i + 3]);
            fields.add(new FormField(raw[i], raw[i + 1], raw[i + 2], page));
        }
        return fields;
    }

    /**
     * Sets the value of a form field.
     *
     * @param name  fully-qualified field name (e.g. {@code "Address.Street"})
     * @param value new text value
     * @return {@code true} if the field was found and updated
     */
    public boolean setFormField(String name, String value) throws PdfException {
        checkOpen();
        return nativeSetFormField(handle, name, value);
    }

    // -----------------------------------------------------------------------
    // Annotations
    // -----------------------------------------------------------------------

    /**
     * Returns all annotations on the given page.
     *
     * @param pageIndex 0-based page index
     * @return list of annotations
     */
    public List<Annotation> getAnnotations(int pageIndex) throws PdfException {
        checkOpen();
        String[] raw = nativeGetAnnotations(handle, pageIndex);
        List<Annotation> annots = new ArrayList<>();
        if (raw == null) return annots;
        // Stride 7: type, x0, y0, x1, y1, contents, author
        for (int i = 0; i + 6 < raw.length; i += 7) {
            annots.add(new Annotation(
                    raw[i],
                    pageIndex,
                    Double.parseDouble(raw[i + 1]),
                    Double.parseDouble(raw[i + 2]),
                    Double.parseDouble(raw[i + 3]),
                    Double.parseDouble(raw[i + 4]),
                    raw[i + 5],
                    raw[i + 6]
            ));
        }
        return annots;
    }

    /**
     * Adds an annotation to a page.
     *
     * @param pageIndex  0-based page index
     * @param type       {@code "highlight"} or {@code "freetext"}
     * @param x0         left edge of bounding rect (PDF user-space points)
     * @param y0         bottom edge of bounding rect
     * @param x1         right edge of bounding rect
     * @param y1         top edge of bounding rect
     * @param content    text content; may be empty
     */
    public void addAnnotation(int pageIndex, String type,
                              double x0, double y0, double x1, double y1,
                              String content) throws PdfException {
        checkOpen();
        nativeAddAnnotation(handle, pageIndex, type, x0, y0, x1, y1,
                content != null ? content : "");
    }

    // -----------------------------------------------------------------------
    // Redaction
    // -----------------------------------------------------------------------

    /**
     * Searches for text and redacts all occurrences.
     *
     * @param pageIndex 0-based page index, or {@code -1} to search all pages
     * @param term      text to search for (literal, case-insensitive)
     * @return summary of the redaction
     */
    public RedactReport redactText(int pageIndex, String term) throws PdfException {
        checkOpen();
        int[] result = nativeRedactText(handle, pageIndex, term);
        if (result == null || result.length < 3) {
            return new RedactReport(0, 0, 0);
        }
        return new RedactReport(result[0], result[1], result[2]);
    }

    // -----------------------------------------------------------------------
    // Encryption
    // -----------------------------------------------------------------------

    /**
     * Saves a password-protected copy of the document (RC4-128).
     *
     * @param outputPath destination file path
     * @param password   user and owner password
     */
    public void encrypt(String outputPath, String password) throws PdfException {
        checkOpen();
        nativeEncrypt(handle, outputPath, password);
    }

    /**
     * Saves a decrypted (plain) copy of an encrypted document.
     *
     * @param outputPath destination file path
     * @param password   user or owner password
     */
    public void decrypt(String outputPath, String password) throws PdfException {
        checkOpen();
        nativeDecrypt(handle, outputPath, password);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    private void checkOpen() {
        if (handle == 0) {
            throw new IllegalStateException("PdfDocument is already closed");
        }
    }

    // -----------------------------------------------------------------------
    // Native method declarations
    // -----------------------------------------------------------------------

    private static native long   nativeOpen(byte[] data);
    private static native long   nativeOpenWithPassword(byte[] data, String password);
    private static native void   nativeClose(long handle);
    private static native int    nativePageCount(long handle);
    private static native double nativePageWidth(long handle, int pageIndex);
    private static native double nativePageHeight(long handle, int pageIndex);
    private static native int    nativePageRotation(long handle, int pageIndex);
    private static native String nativeExtractText(long handle, int pageIndex);
    private static native byte[] nativeRenderPage(long handle, int pageIndex, double dpi);
    private static native byte[] nativeRenderThumbnail(long handle, int pageIndex, int maxDimension);
    private static native String nativeGetMetadata(long handle, String key);
    private static native int    nativeBookmarkCount(long handle);
    private static native int[]  nativeSearchText(long handle, String query);
    private static native void   nativeSave(long handle, String path);
    private static native String[] nativeGetFormFields(long handle);
    private static native boolean  nativeSetFormField(long handle, String name, String value);
    private static native String[] nativeGetAnnotations(long handle, int page);
    private static native void     nativeAddAnnotation(long handle, int page, String type,
                                                        double x0, double y0, double x1, double y1,
                                                        String content);
    private static native int[]    nativeRedactText(long handle, int page, String term);
    private static native void     nativeEncrypt(long handle, String outputPath, String password);
    private static native void     nativeDecrypt(long handle, String outputPath, String password);
}
