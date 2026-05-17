package com.pdfluent;

import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.PointerByReference;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * A PDF document backed by the native PDFluent engine.
 *
 * <p>Use with try-with-resources to ensure the native handle is always freed:
 * <pre>{@code
 * try (PdfDocument doc = PdfDocument.open(Path.of("invoice.pdf"))) {
 *     System.out.println("Pages: " + doc.getPageCount());
 * }
 * }</pre>
 */
public final class PdfDocument implements AutoCloseable {

    private static final PdfCapiLibrary LIB = NativeLoader.get();

    private volatile Pointer handle;

    private PdfDocument(Pointer handle) {
        this.handle = handle;
    }

    // --- Factory methods ---

    /**
     * Open a PDF from a file path.
     *
     * @param path path to the PDF file
     * @throws PdfluentException if the file cannot be read or is not a valid PDF
     */
    public static PdfDocument open(Path path) {
        byte[] bytes;
        try {
            bytes = Files.readAllBytes(path);
        } catch (IOException e) {
            throw new PdfluentException("cannot read file: " + e.getMessage(), e);
        }
        return open(bytes);
    }

    /**
     * Open a PDF from raw bytes.
     *
     * @param data PDF file contents
     * @throws PdfluentException if the data is not a valid PDF
     */
    public static PdfDocument open(byte[] data) {
        if (data == null) throw new IllegalArgumentException("data must not be null");
        PointerByReference out = new PointerByReference();
        int status = LIB.pdf_document_open_from_bytes(data, new NativeLong(data.length), out);
        if (status != 0) {
            throw new PdfluentException(lastError());
        }
        Pointer handle = out.getValue();
        if (handle == null || handle.equals(Pointer.NULL)) {
            throw new PdfluentException("open returned null handle");
        }
        return new PdfDocument(handle);
    }

    // --- Properties ---

    /** Number of pages in the document. */
    public int getPageCount() {
        return LIB.pdf_document_page_count(requireOpen());
    }

    // --- Content ---

    /**
     * Extract plain text from a page.
     *
     * @param pageIndex zero-based page index
     * @return extracted text, or {@code ""} if the page has no text
     * @throws PdfluentException if the page index is out of range
     */
    public String extractText(int pageIndex) {
        Pointer ptr = LIB.pdf_page_extract_text(requireOpen(), pageIndex);
        if (ptr == null || ptr.equals(Pointer.NULL)) {
            String err = lastError();
            if (!err.isEmpty()) throw new PdfluentException(err);
            return "";
        }
        try {
            return ptr.getString(0, StandardCharsets.UTF_8.name());
        } finally {
            LIB.pdf_string_free(ptr);
        }
    }

    // --- Lifecycle ---

    /** Close the document and free native resources. Safe to call multiple times. */
    @Override
    public void close() {
        Pointer h = handle;
        if (h != null) {
            handle = null;
            LIB.pdf_document_free(h);
        }
    }

    /** True if the document has not been closed yet. */
    public boolean isOpen() {
        return handle != null;
    }

    // --- Helpers ---

    private Pointer requireOpen() {
        Pointer h = handle;
        if (h == null) throw new IllegalStateException("PdfDocument is closed");
        return h;
    }

    private static String lastError() {
        String err = LIB.pdf_get_last_error();
        return (err != null && !err.isEmpty()) ? err : "native call failed (no error message)";
    }
}
