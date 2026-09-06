package com.pdfluent;

import com.sun.jna.Library;
import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import com.sun.jna.ptr.PointerByReference;

import java.util.Arrays;
import java.util.List;

/**
 * JNA mapping for the PDFluent C ABI ({@code libpdf_capi}).
 *
 * <p>Functions documented here mirror declarations in {@code pdf_capi.h}.
 * Memory ownership: pointers returned from this library are owned by the
 * library and must be freed with the appropriate {@code *_free} call.
 */
interface PdfCapiLibrary extends Library {

    // ---- Library lifecycle ---------------------------------------------------

    /** Library version string (static; do not free). */
    String pdf_version();

    /** Initialise the library; returns 0 (OK). Currently a no-op. */
    int pdf_init();

    /** Shut down the library. Currently a no-op. */
    void pdf_destroy();

    // ---- Document lifecycle -------------------------------------------------

    /**
     * Open a PDF from raw bytes.
     * On success writes a handle to {@code out}; returns 0 (OK) or non-zero status.
     */
    int pdf_document_open_from_bytes(byte[] data, NativeLong len, PointerByReference out);

    /**
     * Open a PDF from a null-terminated UTF-8 file path.
     * {@code password} may be null.
     */
    int pdf_document_open(byte[] path, Pointer password, PointerByReference out);

    /** Free a document handle. Null is safe. */
    void pdf_document_free(Pointer doc);

    // ---- Document queries ---------------------------------------------------

    /** Number of pages; -1 if doc is null. */
    int pdf_document_page_count(Pointer doc);

    // ---- Digital signing & signature verification ---------------------------

    /**
     * Sign a document with a PKCS#12 identity bundle, producing a new document.
     *
     * <p>C ABI: {@code PdfStatus pdf_document_sign(const PdfDocument *doc,
     * const char *pkcs12_path, const char *pkcs12_password, PdfDocument
     * **out)}. The source {@code doc} is BORROWED and not modified. On success
     * {@code out} receives a NEW heap document handle the caller must free with
     * {@link #pdf_document_free}. {@code pkcs12Password} may be {@code null} for
     * password-less bundles.
     *
     * @return 0 OK · 1 ErrorInvalidArgument · 2 ErrorFileNotFound ·
     *         9 ErrorSign.
     */
    int pdf_document_sign(Pointer doc, String pkcs12Path, String pkcs12Password, PointerByReference out);

    /** Number of signature fields in the document; -1 if {@code doc} is null. */
    int pdf_signature_count(Pointer doc);

    /**
     * Validate the digital signature at zero-based {@code index}.
     *
     * @return 1 valid · 0 invalid · -1 unknown / error (null doc or
     *         out-of-range index).
     */
    int pdf_signature_is_valid(Pointer doc, int index);

    // ---- Content ------------------------------------------------------------

    /**
     * Extract text from a page as a heap-allocated C string.
     * Caller must free with {@link #pdf_string_free(Pointer)}.
     */
    Pointer pdf_page_extract_text(Pointer doc, int pageIndex);

    /** Free a C string returned by the library. Null is safe. */
    void pdf_string_free(Pointer s);

    // ---- Error state --------------------------------------------------------

    /** Thread-local error message (static, do not free). Returns "" if no error. */
    String pdf_get_last_error();

    void pdf_clear_error();

    // ---- Structured text-block extraction ----------------------------------

    /**
     * Native layout of {@code PdfTextBlock} — must match the C struct in
     * {@code include/pdfluent.h}. Five fields: {@code (double, double,
     * double, double, const char*)}.
     *
     * <p>The {@code text} pointer points into Rust-owned memory that is
     * released together with the parent array via
     * {@link #pdf_text_blocks_free}; do <b>not</b> free it individually.
     */
    class PdfTextBlock extends Structure {
        public double x;
        public double y;
        public double width;
        public double height;
        public Pointer text;

        public PdfTextBlock() {}

        public PdfTextBlock(Pointer p) {
            super(p);
            read();
        }

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("x", "y", "width", "height", "text");
        }

        public static class ByReference extends PdfTextBlock implements Structure.ByReference {}
    }

    /**
     * Extract structured text blocks for a page.
     *
     * <p>On success: writes a heap-allocated PdfTextBlock array pointer
     * to {@code outBlocks} and the element count to {@code outCount}.
     * The caller MUST release the array via {@link #pdf_text_blocks_free}
     * passing the same pointer + count.
     *
     * @return 0 OK · 1 ErrorInvalidArgument · 5 ErrorPageRange · 12 ErrorExtract.
     */
    int pdf_page_extract_text_blocks(
        Pointer doc,
        int pageIndex,
        PointerByReference outBlocks,
        com.sun.jna.ptr.NativeLongByReference outCount);

    /**
     * Free an array previously returned by
     * {@link #pdf_page_extract_text_blocks}.
     *
     * <p>{@code pdf_text_blocks_free(null, 0)} is a no-op.
     */
    void pdf_text_blocks_free(Pointer blocks, NativeLong count);
}
