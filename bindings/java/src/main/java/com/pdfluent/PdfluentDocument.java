package com.pdfluent;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

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
    // Embedded images
    // =========================================================================

    /**
     * Lists the images stored on one page.
     *
     * <p>One {@link ImageInfo} per image XObject in the page's resources, in
     * resource order.
     *
     * @param pageIndex zero-based page index; must be in {@code [0, getPageCount())}
     * @return an ordered list (may be empty) of {@link ImageInfo}
     * @throws IllegalStateException      if the document has been closed
     * @throws PdfluentPageRangeException if {@code pageIndex} is out of range
     * @throws PdfluentException          for engine extraction failures
     */
    public java.util.List<ImageInfo> pageImages(int pageIndex) {
        ensureOpen();
        checkPageIndex(pageIndex);
        String[] flat = nativePageImages(handle, pageIndex);
        if (flat == null || flat.length == 0) {
            return java.util.Collections.emptyList();
        }
        java.util.List<ImageInfo> result = new java.util.ArrayList<>(flat.length / 10);
        for (int i = 0; i + 9 < flat.length; i += 10) {
            result.add(
                new ImageInfo(
                    Integer.parseInt(flat[i]),
                    Integer.parseInt(flat[i + 1]),
                    Integer.parseInt(flat[i + 2]),
                    Integer.parseInt(flat[i + 3]),
                    Integer.parseInt(flat[i + 4]),
                    flat[i + 5],
                    flat[i + 6],
                    flat[i + 7],
                    flat[i + 8],
                    Boolean.parseBoolean(flat[i + 9])));
        }
        return java.util.Collections.unmodifiableList(result);
    }

    /**
     * Extracts one stored image: header facts plus encoded bytes.
     *
     * <p>JPEG payloads pass through untouched; everything else arrives decoded
     * and re-encoded as PNG.
     *
     * @param imageId the {@link ImageInfo#getId() id} a {@link #pageImages(int)}
     *                entry reported
     * @return the extracted image
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for unknown ids and engine extraction failures
     */
    public ExtractedImage extractImageData(int imageId) {
        ensureOpen();
        byte[] raw = nativeExtractImageData(handle, imageId);
        if (raw == null) {
            throw new PdfluentException("failed to extract image " + imageId);
        }
        return decodeExtractedImage(raw);
    }

    /**
     * Decodes one stored image to raw samples, top row first.
     *
     * <p>Palettes and stencils are expanded.
     *
     * @param imageId the {@link ImageInfo#getId() id} a {@link #pageImages(int)}
     *                entry reported
     * @return the decoded image
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for unknown ids and engine extraction failures
     */
    public DecodedImage decodeImagePixels(int imageId) {
        ensureOpen();
        byte[] raw = nativeDecodeImagePixels(handle, imageId);
        if (raw == null) {
            throw new PdfluentException("failed to decode image " + imageId);
        }
        return decodeDecodedImage(raw);
    }

    /**
     * The document as markdown, in reading order (#573).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * <p>Headings become {@code #} to {@code ###}, list items {@code -} and
     * {@code 1.}, a table a pipe table or an HTML one where markdown cannot
     * hold its shape, and a change of page an HTML comment. The running heads
     * a document repeats are not in it; they are in {@link #extractBlocks()}.
     *
     * @return the whole document as markdown
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public String extractMarkdown() {
        return extractMarkdown(0, -1);
    }

    /**
     * The document as markdown, for a window of pages (#573).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * <p>The document is read whole whatever the window is, because what a
     * block IS is decided against the rest of the document: a size is a
     * heading against what the body is set in, and a line is a running head
     * because three pages repeat it.
     *
     * @param firstPage zero-based first page of the window
     * @param pageCount pages from {@code firstPage}; negative for the rest of
     *                  the document. A window past the last page is an empty
     *                  string, not an error.
     * @return the window as markdown
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public String extractMarkdown(int firstPage, int pageCount) {
        ensureOpen();
        String markdown = nativeExtractMarkdown(handle, Math.max(firstPage, 0), pageCount);
        return markdown == null ? "" : markdown;
    }

    /**
     * The document as a Docling {@code DoclingDocument}, as JSON (#629).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * <p>The schema version the engine pins, which docling-core's
     * {@code DoclingDocument.model_validate_json} loads as it is, so a pipeline
     * built on Docling reads this document unchanged. What has no mapping is
     * listed in {@code docs/STRUCTURED_EXTRACTION.md}.
     *
     * @return the whole document as {@code DoclingDocument} JSON, named
     *         {@code "document"}
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public String extractDocling() {
        return extractDocling(0, -1, "document");
    }

    /**
     * The document as a Docling {@code DoclingDocument}, for a window of pages
     * (#629).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * @param firstPage zero-based first page of the window
     * @param pageCount pages from {@code firstPage}; negative for the rest of
     *                  the document
     * @param name      {@code DoclingDocument.name}, which Docling fills with
     *                  the file's stem; {@code null} means {@code "document"}
     * @return the window as {@code DoclingDocument} JSON
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public String extractDocling(int firstPage, int pageCount, String name) {
        ensureOpen();
        return nativeExtractDocling(
            handle, Math.max(firstPage, 0), pageCount, name == null ? "document" : name);
    }

    /**
     * The document's blocks, in reading order (#573).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * <p>One {@link Block} per paragraph, heading, list item, table cell and
     * running head, each with the page and the box it was drawn in — the unit
     * a caller chunks, cites or embeds.
     *
     * @return an ordered list (may be empty) of {@link Block}
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public java.util.List<Block> extractBlocks() {
        return extractBlocks(0, -1);
    }

    /**
     * The document's blocks, for a window of pages (#573).
     *
     * <p><b>Preview: output quality is still improving and the API may change in a minor release.</b>
     *
     * @param firstPage zero-based first page of the window
     * @param pageCount pages from {@code firstPage}; negative for the rest of
     *                  the document. A window past the last page is an empty
     *                  list, not an error.
     * @return an ordered list (may be empty) of {@link Block}
     * @throws IllegalStateException if the document has been closed
     * @throws PdfluentException     for engine extraction failures
     */
    public java.util.List<Block> extractBlocks(int firstPage, int pageCount) {
        ensureOpen();
        String[] flat =
            nativeExtractBlocksWithConfidence(handle, Math.max(firstPage, 0), pageCount);
        if (flat == null || flat.length == 0) {
            return java.util.Collections.emptyList();
        }
        java.util.List<Block> result = new java.util.ArrayList<>(flat.length / BLOCK_STRIDE);
        for (int i = 0; i + BLOCK_STRIDE - 1 < flat.length; i += BLOCK_STRIDE) {
            double[] bbox = new double[] {
                parseDouble(flat[i + 3]), parseDouble(flat[i + 4]),
                parseDouble(flat[i + 5]), parseDouble(flat[i + 6]),
            };
            result.add(new Block(
                Block.Kind.of(flat[i]),
                flat[i + 1],
                (int) parseDouble(flat[i + 2]),
                bbox,
                (int) parseDouble(flat[i + 7]),
                Boolean.parseBoolean(flat[i + 8]),
                parseInteger(flat[i + 9]),
                parseInteger(flat[i + 10]),
                parseInteger(flat[i + 11]),
                parseInteger(flat[i + 12]),
                parseInteger(flat[i + 13]),
                parseInteger(flat[i + 14]),
                parseBoolean(flat[i + 15]),
                parseDoubleOrNull(flat[i + 16]),
                gridOf(flat, i)));
        }
        return java.util.Collections.unmodifiableList(result);
    }

    /**
     * How many strings the native layer writes per block. The order is
     * {@code [kind, text, page, x0, y0, x1, y1, index, continuesPrevious,
     * level, ordinal, row, column, rowSpan, columnSpan, header, confidence,
     * gridThinRows, gridFill, gridRetriedFrom, gridFullRows, gridColumns]}, with
     * an empty string where the field does not belong to the block's kind.
     *
     * <p>This is the stride of {@code nativeExtractBlocksWithConfidence}, which is
     * a method of its own rather than a wider {@code nativeExtractBlocks}. The
     * library is loaded at run time, so an older jar can be paired with a newer
     * one: that jar decodes by the stride it was compiled with, and widening the
     * old method's record would shift every field it reads. A jar that calls this
     * name is one that knows the record is seventeen long.
     */
    private static final int BLOCK_STRIDE = 22;

    /** A number the native layer wrote, or zero where it wrote nothing. */
    private static double parseDouble(String value) {
        if (value == null || value.isEmpty()) return 0.0;
        try {
            return Double.parseDouble(value);
        } catch (NumberFormatException e) {
            return 0.0;
        }
    }

    /** A number the native layer wrote, or {@code null} for a field this kind has not got. */
    private static Integer parseInteger(String value) {
        if (value == null || value.isEmpty()) return null;
        try {
            return Integer.valueOf(value);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    /**
     * The builder's grid evidence for one record, or {@code null} where the block
     * is not a table cell (#574).
     */
    private static Block.GridDoubt gridOf(String[] flat, int i) {
        Integer thinRows = parseInteger(flat[i + 17]);
        if (thinRows == null) return null;
        return new Block.GridDoubt(
            thinRows,
            parseDouble(flat[i + 18]),
            parseInteger(flat[i + 19]),
            parseInteger(flat[i + 20]),
            parseInteger(flat[i + 21]));
    }

    /**
     * A number the native layer wrote, or {@code null} where it wrote nothing --
     * which for a confidence means no model spoke, not a confidence of zero.
     */
    private static Double parseDoubleOrNull(String value) {
        if (value == null || value.isEmpty()) return null;
        try {
            return Double.valueOf(value);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    /** A flag the native layer wrote, or {@code null} for a field this kind has not got. */
    private static Boolean parseBoolean(String value) {
        if (value == null || value.isEmpty()) return null;
        return Boolean.valueOf(value);
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
    // Persistence
    // =========================================================================

    /**
     * Write the document (including any form-field mutations) to a file.
     *
     * <p>When the document has been mutated (e.g. via {@link #setFormField})
     * the updated state is serialised; otherwise the original bytes are
     * written verbatim.
     *
     * @param path destination file path; must not be {@code null}
     * @throws PdfluentException if serialisation or the file write fails
     * @throws IllegalStateException if the document has been closed
     */
    public void save(Path path) {
        ensureOpen();
        nativeSave(handle, path.toString());
    }

    // =========================================================================
    // AcroForm
    // =========================================================================

    /**
     * Enumerate the document's interactive AcroForm fields.
     *
     * <p>Returns one {@link FormField} per terminal field, with its
     * fully-qualified name, type ({@code text}/{@code button}/{@code choice}/
     * {@code signature}), current value, and 0-based page index (or -1 when
     * unknown). XFA-only documents and documents without an AcroForm return
     * an empty list.
     *
     * @return the form fields; never {@code null}
     * @throws IllegalStateException if the document has been closed
     */
    public List<FormField> getFormFields() {
        ensureOpen();
        String[] flat = nativeGetFormFields(handle);
        if (flat == null || flat.length == 0) {
            return new ArrayList<>();
        }
        // Stride 4: [name, type, value, page] per field.
        List<FormField> fields = new ArrayList<>(flat.length / 4);
        for (int i = 0; i + 3 < flat.length; i += 4) {
            int page;
            try {
                page = Integer.parseInt(flat[i + 3]);
            } catch (NumberFormatException e) {
                page = -1;
            }
            fields.add(new FormField(flat[i], flat[i + 1], flat[i + 2], page));
        }
        return fields;
    }

    /**
     * Fill a single AcroForm field by its fully-qualified name.
     *
     * <p>Routes through the single SDK writeback chain
     * ({@code pdf_forms::apply_field_value}): the field type is detected from
     * {@code /FT} and the value is applied as text, a radio export name, a
     * choice option, or a bool-ish checkbox state
     * ({@code "true"}/{@code "false"}/{@code "Off"}/{@code "0"}). The chain
     * keeps {@code /V}, per-widget {@code /AS}, and {@code /AP} consistent and
     * rejects read-only fields. The mutation is persisted to the in-memory
     * document; a subsequent {@code save} writes it out.
     *
     * @param name  fully-qualified field name (e.g. {@code "Address.Street"});
     *              must not be {@code null}
     * @param value the value to apply; must not be {@code null}
     * @return {@code true} if the field was found and updated; {@code false}
     *         when the document has no form or no field matches {@code name}
     * @throws PdfluentException if the field is read-only or the value is not
     *                           valid for the field
     * @throws IllegalStateException if the document has been closed
     */
    public boolean setFormField(String name, String value) {
        ensureOpen();
        return nativeSetFormField(handle, name, value);
    }

    /**
     * Select multiple options on a multi-select list box.
     *
     * <p>Routes through {@code pdf_forms::apply_choice_multi}: writes
     * {@code /V} as an array of text strings and rebuilds {@code /I} (the
     * sorted selected-index cache) to match what Adobe Acrobat produces. Pass
     * an empty array to clear the selection. For a non-editable list box every
     * value must be one of the field's {@code /Opt} options.
     *
     * @param name   fully-qualified field name of a multi-select list box;
     *               must not be {@code null}
     * @param values export (or display) values of the options to select;
     *               must not be {@code null}
     * @return {@code true} if the field was found and updated; {@code false}
     *         when the document has no form or no field matches {@code name}
     * @throws PdfluentException if the field is not a multi-select list box,
     *                           is read-only, or a value is not a valid option
     * @throws IllegalStateException if the document has been closed
     */
    public boolean setMultiSelect(String name, String[] values) {
        ensureOpen();
        return nativeSetMultiSelect(handle, name, values);
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

    private static byte[] readLengthPrefixedBytes(ByteBuffer bb) {
        if (bb.remaining() < 4) {
            throw new PdfluentException(
                "native image result is malformed: truncated length prefix");
        }
        int len = bb.getInt();
        if (len < 0 || len > bb.remaining()) {
            throw new PdfluentException(
                "native image result is malformed: bad length " + len);
        }
        byte[] bytes = new byte[len];
        bb.get(bytes);
        return bytes;
    }

    private static ExtractedImage decodeExtractedImage(byte[] raw) {
        if (raw.length < 28) {
            throw new PdfluentException(
                "native image result is malformed: expected at least 28 header bytes, got "
                    + raw.length);
        }
        ByteBuffer bb = ByteBuffer.wrap(raw);
        int width = bb.getInt();
        int height = bb.getInt();
        int bitsPerComponent = bb.getInt();
        int components = bb.getInt();
        boolean masked = bb.getInt() != 0;
        int xres = bb.getInt();
        int yres = bb.getInt();
        String colorSpace = new String(readLengthPrefixedBytes(bb), StandardCharsets.UTF_8);
        String colorSpaceName = new String(readLengthPrefixedBytes(bb), StandardCharsets.UTF_8);
        String encoding = new String(readLengthPrefixedBytes(bb), StandardCharsets.UTF_8);
        byte[] data = readLengthPrefixedBytes(bb);
        return new ExtractedImage(
            width,
            height,
            bitsPerComponent,
            components,
            colorSpace,
            colorSpaceName,
            encoding,
            data,
            masked,
            xres,
            yres);
    }

    private static DecodedImage decodeDecodedImage(byte[] raw) {
        if (raw.length < 12) {
            throw new PdfluentException(
                "native image result is malformed: expected at least 12 header bytes, got "
                    + raw.length);
        }
        ByteBuffer bb = ByteBuffer.wrap(raw);
        int width = bb.getInt();
        int height = bb.getInt();
        int components = bb.getInt();
        String colorSpace = new String(readLengthPrefixedBytes(bb), StandardCharsets.UTF_8);
        byte[] pixels = readLengthPrefixedBytes(bb);
        return new DecodedImage(width, height, components, colorSpace, pixels);
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
    private static native String[] nativePageImages(long handle, int pageIndex);
    private static native byte[] nativeExtractImageData(long handle, int imageId);
    private static native byte[] nativeDecodeImagePixels(long handle, int imageId);
    private static native String nativeExtractMarkdown(long handle, int firstPage, int pageCount);
    private static native String[] nativeExtractBlocks(long handle, int firstPage, int pageCount);
    private static native String nativeExtractDocling(
        long handle, int firstPage, int pageCount, String name);

    /**
     * The same records with the learned model's confidence appended (#624), so
     * stride 17. Declared beside the sixteen-field method rather than replacing
     * it: see {@link #BLOCK_STRIDE}.
     */
    private static native String[] nativeExtractBlocksWithConfidence(
        long handle, int firstPage, int pageCount);
    private static native byte[] nativeRenderPage(long handle, int pageIndex, double dpi);
    private static native byte[] nativeRenderThumbnail(long handle, int pageIndex, int maxDimension);
    private static native String nativeGetMetadata(long handle, String key);
    private static native int nativeBookmarkCount(long handle);
    private static native Object nativeSearchText(long handle, String query);
    private static native void nativeSave(long handle, String path);
    private static native String[] nativeGetFormFields(long handle);
    private static native boolean nativeSetFormField(long handle, String name, String value);
    private static native boolean nativeSetMultiSelect(long handle, String name, String[] values);
}
