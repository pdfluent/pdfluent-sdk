/**
 * @file pdfluent.h
 * @brief PDFluent C API — stable, opaque-handle interface to the PDF engine.
 *
 * @par Design contract
 * - Every function returns a @ref PdfStatus (or a sentinel value defined in its
 *   doc comment) to indicate success or failure.
 * - Detailed error text is available via @ref pdf_get_last_error().  The string
 *   is valid until the next API call on the same thread.
 * - All opaque handles are freed by the matching @c _free function.  Passing
 *   NULL to a free function is always safe (no-op).
 * - Thread safety: a single handle must be used from one thread at a time.
 *   The error-state slot (@ref pdf_get_last_error) is per-thread.
 *
 * @par ABI stability
 * See @c docs/c_abi_stability.md.  In brief: major version = incompatible
 * change; minor = additive; patch = fix only.  Opaque structs (forward
 * declarations only) guarantee binary compatibility across minor versions.
 *
 * @par Ownership legend used in this header
 * - **CALLER FREES** — the function heap-allocates the object and transfers
 *   ownership to the caller, who must free with the documented free function.
 * - **LIBRARY OWNS** — the pointer belongs to the library; the caller must not
 *   free it.  Validity is stated in the doc comment (typically "until the next
 *   API call on this thread").
 * - **BORROWED** — the caller passes a pointer it already owns; the library
 *   does not free it and will not retain it past the call.
 */

#ifndef PDFLUENT_H
#define PDFLUENT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* =========================================================================
 * Status codes
 * =========================================================================
 * Every function that can fail returns one of these values.
 * Map to Rust error variants in crates/pdf-capi/src/types.rs.
 */

/**
 * @brief Status codes returned by all PDF API functions.
 *
 * | Value | Name                           | Rust variant              |
 * |-------|--------------------------------|---------------------------|
 * |   0   | PDF_STATUS_OK                  | (success)                 |
 * |   1   | PDF_STATUS_ERROR_INVALID_ARG   | ErrorInvalidArgument      |
 * |   2   | PDF_STATUS_ERROR_FILE_NOT_FOUND| ErrorFileNotFound         |
 * |   3   | PDF_STATUS_ERROR_INVALID_PASS  | ErrorInvalidPassword      |
 * |   4   | PDF_STATUS_ERROR_CORRUPT_PDF   | ErrorCorruptPdf           |
 * |   5   | PDF_STATUS_ERROR_PAGE_RANGE    | ErrorPageRange            |
 * |   6   | PDF_STATUS_ERROR_RENDER        | ErrorRender               |
 * |   7   | PDF_STATUS_ERROR_CONVERT       | ErrorConvert              |
 * |   8   | PDF_STATUS_ERROR_REDACT             | ErrorRedact               |
 * |   9   | PDF_STATUS_ERROR_SIGN               | ErrorSign                 |
 * |  10   | PDF_STATUS_ERROR_ANNOTATION         | ErrorAnnotation           |
 * |  11   | PDF_STATUS_ERROR_MERGE              | ErrorMerge                |
 * |  12   | PDF_STATUS_ERROR_EXTRACT            | ErrorExtract              |
 * |  13   | PDF_STATUS_ERROR_SPLIT              | ErrorSplit                |
 * |  14   | PDF_STATUS_ERROR_WATERMARK          | ErrorWatermark            |
 * |  15   | PDF_STATUS_ERROR_COMPRESS           | ErrorCompress             |
 * | 16-20 | retired -- the licence-activation codes (#226)                             |
 * |  22   | retired -- PDF_STATUS_ERROR_CAPABILITY_NOT_LICENSED (#226)                 |
 * |  99   | PDF_STATUS_ERROR_UNKNOWN                    | ErrorUnknown                 |
 *
 * See @c docs/c_abi_stability.md §3 for the full error catalogue with
 * recovery hints and cross-binding mapping.
 */
typedef enum {
    /** Operation succeeded. */
    PDF_STATUS_OK                    = 0,

    /**
     * A null pointer, a negative index, or an otherwise structurally invalid
     * argument was passed.
     *
     * Recovery: inspect argument values before calling.
     */
    PDF_STATUS_ERROR_INVALID_ARG     = 1,

    /**
     * The file path does not exist or cannot be read (permission denied, I/O
     * error, or a path that resolves to a directory).
     *
     * Recovery: verify the path exists and the process has read permission.
     */
    PDF_STATUS_ERROR_FILE_NOT_FOUND  = 2,

    /**
     * A password-protected document was opened without supplying the correct
     * owner or user password.
     *
     * Recovery: supply the correct password via the @c password argument.
     */
    PDF_STATUS_ERROR_INVALID_PASS    = 3,

    /**
     * The PDF data is corrupt, truncated, or not a valid PDF at all.
     *
     * Recovery: verify the source file is a complete, undamaged PDF.
     */
    PDF_STATUS_ERROR_CORRUPT_PDF     = 4,

    /**
     * A page index is outside [0, page_count-1].
     *
     * Recovery: call @ref pdf_document_page_count first and clamp the index.
     */
    PDF_STATUS_ERROR_PAGE_RANGE      = 5,

    /**
     * The rendering pipeline failed (resource allocation, rasterisation error,
     * or unsupported content type on the page).
     *
     * Recovery: retry at a lower DPI; check @ref pdf_get_last_error for
     * details.
     */
    PDF_STATUS_ERROR_RENDER          = 6,

    /**
     * A PDF/A conversion step failed (incompatible content, lopdf round-trip
     * error, or an unsupported conformance level for the source document).
     *
     * Recovery: validate the source with @ref pdf_document_validate_pdfa
     * first; check errors before converting.
     */
    PDF_STATUS_ERROR_CONVERT         = 7,

    /**
     * Text or image redaction failed.
     *
     * Recovery: check @ref pdf_get_last_error; ensure the pattern is valid
     * UTF-8.
     */
    PDF_STATUS_ERROR_REDACT          = 8,

    /**
     * PKCS#12 loading, cryptographic signing, or the PDF write-back failed.
     *
     * Recovery: verify the .p12/.pfx file is readable and the password is
     * correct; confirm the document has no existing signature fields blocking
     * an incremental update.
     */
    PDF_STATUS_ERROR_SIGN            = 9,

    /**
     * An annotation read or write operation failed (page not found, corrupt
     * annotation dictionary, or a serialisation error during save).
     *
     * Recovery: check @ref pdf_get_last_error; verify the page index.
     */
    PDF_STATUS_ERROR_ANNOTATION      = 10,

    /**
     * A multi-document merge failed (incompatible PDF versions, cross-
     * reference rebuild error, or lopdf write-back failure).
     *
     * Recovery: check @ref pdf_get_last_error; try merging fewer documents at
     * a time.
     */
    PDF_STATUS_ERROR_MERGE           = 11,

    /**
     * Image or content extraction failed (unsupported image filter, out-of-
     * range index, or a lopdf traversal error).
     *
     * Recovery: confirm the page and index with @ref pdf_page_image_count
     * first.
     */
    PDF_STATUS_ERROR_EXTRACT         = 12,

    /**
     * A page-range split failed (invalid range, lopdf rewrite failure).
     *
     * Recovery: check @ref pdf_get_last_error; verify @c from_page ≤
     * @c to_page < @ref pdf_document_page_count.
     */
    PDF_STATUS_ERROR_SPLIT           = 13,

    /**
     * Watermark application failed (unsupported page geometry, font embed
     * failure, or lopdf write-back failure).
     *
     * Recovery: check @ref pdf_get_last_error.
     */
    PDF_STATUS_ERROR_WATERMARK       = 14,

    /**
     * Stream-compression optimisation failed (zlib error or lopdf write-back
     * failure).
     *
     * Recovery: check @ref pdf_get_last_error.
     */
    PDF_STATUS_ERROR_COMPRESS             = 15,

    /*
     * 16 to 20 were the licence-activation codes and 22 was
     * PDF_STATUS_ERROR_CAPABILITY_NOT_LICENSED. There is no licence key and
     * no tier, so nothing can return them. The numbers are retired rather
     * than reused: a caller compiled against an older header must never read
     * a new meaning out of an old constant. (#226)
     */

    /**
     * An internal error with no specific code.  Always accompanied by a
     * message from @ref pdf_get_last_error.
     */
    PDF_STATUS_ERROR_UNKNOWN              = 99,
} PdfStatus;

/* =========================================================================
 * PDF/A conformance levels
 * =========================================================================
 */

/**
 * @brief PDF/A conformance levels.
 *
 * Every value here can be VALIDATED with @ref pdf_document_validate_pdfa.
 * CONVERSION with @ref pdf_document_convert_pdfa delivers three of them —
 * @c PDF_A_LEVEL_1B, @c PDF_A_LEVEL_2B and @c PDF_A_LEVEL_3B — and REFUSES the
 * rest with @c PDF_STATUS_ERROR_INVALID_ARG.  Each value below says which it
 * is, and the two words are load-bearing: a level that is accepted and then
 * written as its part's B level would leave the caller archiving a file whose
 * metadata does not say what they asked for.
 *
 * The A levels need a tagged structure tree and the U levels a complete Unicode
 * mapping; neither can be manufactured from an untagged source, and part 4 has
 * no B level at all.  As a level gets a measured baseline it is enabled here.
 *
 * Values are stable across library versions (see @c docs/c_abi_stability.md).
 */
typedef enum {
    /** PDF/A-1B — ISO 19005-1 basic (most common archival target). Converts and validates. */
    PDF_A_LEVEL_1B = 0,
    /** PDF/A-1A — ISO 19005-1 accessible (requires tagged PDF). Validates only. */
    PDF_A_LEVEL_1A = 1,
    /** PDF/A-2B — ISO 19005-2 basic. Converts and validates. */
    PDF_A_LEVEL_2B = 2,
    /** PDF/A-2U — ISO 19005-2 with Unicode mapping. Validates only. */
    PDF_A_LEVEL_2U = 3,
    /** PDF/A-2A — ISO 19005-2 accessible. Validates only. */
    PDF_A_LEVEL_2A = 4,
    /** PDF/A-3B — ISO 19005-3 basic (allows embedded files). Converts and validates. */
    PDF_A_LEVEL_3B = 5,
    /** PDF/A-3U — ISO 19005-3 with Unicode mapping. Validates only. */
    PDF_A_LEVEL_3U = 6,
    /** PDF/A-3A — ISO 19005-3 accessible. Validates only. */
    PDF_A_LEVEL_3A = 7,
    /** PDF/A-4  — ISO 19005-4 base level. Validates only. */
    PDF_A_LEVEL_4  = 8,
    /** PDF/A-4F — allows file attachments. Validates only. */
    PDF_A_LEVEL_4F = 9,
    /** PDF/A-4E — allows engineering content (3D, rich media). Validates only. */
    PDF_A_LEVEL_4E = 10,
} PdfALevel;

/* =========================================================================
 * Opaque handle types
 * =========================================================================
 * These are forward declarations only.  The struct body is private to the
 * library; sizeof/offsetof on these types is undefined behaviour for callers.
 * This is the mechanism that ensures binary compatibility across minor versions.
 */

/** @brief Opaque handle to an open PDF document. */
typedef struct PdfDocument PdfDocument;

/** @brief Opaque handle to a PDF/A compliance validation report. */
typedef struct PdfComplianceReport PdfComplianceReport;

/* =========================================================================
 * Library lifecycle
 * =========================================================================
 */

/**
 * @brief Initialise the PDF library.
 *
 * Must be called once before any other function.  Currently a no-op but
 * reserved for future global initialisation (thread pool, font cache).
 *
 * @par Ownership
 * No pointers involved.
 *
 * @return PDF_STATUS_OK on success.
 */
PdfStatus pdf_init(void);

/**
 * @brief Shut down the PDF library and release global resources.
 *
 * Must be called once after all documents have been freed.
 *
 * @par Ownership
 * No pointers involved.
 */
void pdf_destroy(void);

/**
 * @brief Return the library version string.
 *
 * The library's package version (the pdf-capi crate version), baked in at
 * build time.  It tracks the crate, so no fixed number is promised here.
 *
 * @par Ownership: LIBRARY OWNS
 * The returned pointer is a static string; the caller must not free
 * it.  Valid for the lifetime of the process.
 *
 * @return Null-terminated version string.
 */
const char *pdf_version(void);

/* =========================================================================
 * Error state (per-thread)
 * =========================================================================
 */

/**
 * @brief Return the last error message recorded on this thread.
 *
 * The message includes the error code, a human-readable description, a fix
 * hint, and a documentation URL (format: "[CODE] msg — Fix: hint — Docs: url").
 *
 * @par Ownership: LIBRARY OWNS
 * The returned pointer is valid until the next API call on the same thread.
 * The caller must not free it.
 *
 * @return Null-terminated error string, or NULL if no error has been recorded.
 */
const char *pdf_get_last_error(void);

/**
 * @brief Clear the last error recorded on this thread.
 *
 * @par Ownership
 * No pointers involved.
 */
void pdf_clear_error(void);

/* =========================================================================
 * Document lifecycle
 * =========================================================================
 */

/**
 * @brief Open a PDF from a file path.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a heap-allocated handle.  The caller must free it
 * with @ref pdf_document_free when done.  On failure @c *out is set to NULL.
 *
 * @param path      Null-terminated UTF-8 path to the PDF file.  BORROWED.
 * @param password  Null-terminated UTF-8 password, or NULL for no password.
 *                  BORROWED.
 * @param out       Writable pointer to receive the document handle.
 *
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_FILE_NOT_FOUND,
 *         PDF_STATUS_ERROR_INVALID_PASS, or PDF_STATUS_ERROR_CORRUPT_PDF.
 */
PdfStatus pdf_document_open(
    const char *path,
    const char *password,
    PdfDocument **out);

/**
 * @brief Open a PDF from an in-memory byte buffer.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a heap-allocated handle.  The caller must free it
 * with @ref pdf_document_free when done.  The library copies the bytes; the
 * caller may free @c data immediately after this call returns.  On failure
 * @c *out is set to NULL.
 *
 * @param data  Pointer to @c len readable bytes.  BORROWED.
 * @param len   Number of bytes pointed to by @c data.
 * @param out   Writable pointer to receive the document handle.
 *
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_CORRUPT_PDF.
 */
PdfStatus pdf_document_open_from_bytes(
    const uint8_t *data,
    size_t len,
    PdfDocument **out);

/**
 * @brief Free a document handle.
 *
 * @par Ownership: CALLER FREES doc
 * After this call @c doc is invalid and must not be used.
 * Passing NULL is a safe no-op.
 *
 * @param doc  Handle returned by @ref pdf_document_open or
 *             @ref pdf_document_open_from_bytes, or NULL.
 */
void pdf_document_free(PdfDocument *doc);

/**
 * @brief Serialise the document to bytes.
 *
 * Mutation entry points (@ref pdf_documents_merge,
 * @ref pdf_annotation_add_highlight, @ref pdf_form_set_field_value, …)
 * return a new document re-opened from freshly written bytes, so the bytes
 * below are always the document's current state — never a stale copy of its
 * input.
 *
 * @par Ownership: CALLER FREES @c *out_data via @ref pdf_bytes_free.
 * On failure @c *out_data is left untouched.
 *
 * @param doc       Valid document handle.  BORROWED.
 * @param out_data  Writable pointer to receive the heap buffer.
 * @param out_len   Writable pointer to receive the buffer length in bytes.
 * @return PDF_STATUS_OK, or PDF_STATUS_ERROR_INVALID_ARG on a NULL argument.
 */
PdfStatus pdf_document_tobytes(
    const PdfDocument *doc,
    uint8_t **out_data,
    size_t *out_len);

/* =========================================================================
 * Document queries
 * =========================================================================
 */

/**
 * @brief Return the number of pages in the document.
 *
 * @par Ownership
 * @c doc is BORROWED; the caller retains ownership.
 *
 * @param doc  Valid document handle, or NULL.
 * @return Number of pages (≥ 1), or -1 if @c doc is NULL (sets error state).
 */
int32_t pdf_document_page_count(const PdfDocument *doc);

/**
 * @brief Return the width of a page in PDF user-space points (1/72 inch).
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Width in points (> 0), or 0.0 on error.
 */
double pdf_page_width(const PdfDocument *doc, int32_t page_index);

/**
 * @brief Return the height of a page in PDF user-space points (1/72 inch).
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Height in points (> 0), or 0.0 on error.
 */
double pdf_page_height(const PdfDocument *doc, int32_t page_index);

/**
 * @brief Return the rotation of a page in degrees.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Rotation in degrees: 0, 90, 180, or 270.  Returns 0 on error.
 */
int32_t pdf_page_rotation(const PdfDocument *doc, int32_t page_index);

/* =========================================================================
 * Page geometry boxes
 * =========================================================================
 */

/**
 * @brief Get the MediaBox of a page.
 *
 * Coordinates are in PDF user-space points (origin bottom-left, y upward).
 *
 * @par Ownership
 * @c doc is BORROWED.  All @c out_* pointers are BORROWED write targets.
 *
 * @param doc        Valid document handle.
 * @param page_index Zero-based page index.
 * @param out_x0     Left edge of the box.
 * @param out_y0     Bottom edge of the box.
 * @param out_x1     Right edge of the box.
 * @param out_y1     Top edge of the box.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_PAGE_RANGE.
 */
PdfStatus pdf_page_media_box(
    const PdfDocument *doc,
    int32_t page_index,
    double *out_x0, double *out_y0,
    double *out_x1, double *out_y1);

/**
 * @brief Get the CropBox of a page.
 *
 * If the page has no explicit CropBox the MediaBox values are returned.
 * Coordinates are in PDF user-space points (origin bottom-left, y upward).
 *
 * @par Ownership
 * @c doc is BORROWED.  All @c out_* pointers are BORROWED write targets.
 *
 * @param doc        Valid document handle.
 * @param page_index Zero-based page index.
 * @param out_x0     Left edge of the box.
 * @param out_y0     Bottom edge of the box.
 * @param out_x1     Right edge of the box.
 * @param out_y1     Top edge of the box.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_PAGE_RANGE.
 */
PdfStatus pdf_page_crop_box(
    const PdfDocument *doc,
    int32_t page_index,
    double *out_x0, double *out_y0,
    double *out_x1, double *out_y1);

/* =========================================================================
 * Rendering
 * =========================================================================
 */

/**
 * @brief Render a page to an RGBA pixel buffer at the requested DPI.
 *
 * The pixel buffer is laid out as @c width × height × 4 bytes, row-major,
 * top-to-bottom, in sRGB colour space (R8 G8 B8 A8 unpremultiplied).
 *
 * @par Ownership: CALLER FREES *out_pixels
 * On success, @c *out_pixels points to a heap-allocated RGBA buffer of
 * exactly @c (*out_width) × (*out_height) × 4 bytes.  The caller must free
 * it with <tt>pdf_pixels_free(*out_pixels, width * height * 4)</tt>.
 * On failure @c *out_pixels is set to NULL.
 *
 * @param doc        Valid document handle.  BORROWED.
 * @param page_index Zero-based page index.
 * @param dpi        Render resolution in dots per inch (e.g. 72.0, 150.0, 300.0).
 * @param out_width  Receives the pixel width of the rendered image.
 * @param out_height Receives the pixel height of the rendered image.
 * @param out_pixels Receives a pointer to the RGBA pixel data.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_PAGE_RANGE, or PDF_STATUS_ERROR_RENDER.
 */
PdfStatus pdf_page_render(
    const PdfDocument *doc,
    int32_t page_index,
    double dpi,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_pixels);

/**
 * @brief Render a thumbnail that fits within @c max_dimension × @c max_dimension.
 *
 * The longer side of the page is scaled to @c max_dimension; the shorter side
 * is scaled proportionally.  Output layout is the same as @ref pdf_page_render.
 *
 * @par Ownership: CALLER FREES *out_pixels
 * Same contract as @ref pdf_page_render.
 *
 * @param doc          Valid document handle.  BORROWED.
 * @param page_index   Zero-based page index.
 * @param max_dimension Maximum pixel size of the longest side.
 * @param out_width    Receives the pixel width of the thumbnail.
 * @param out_height   Receives the pixel height of the thumbnail.
 * @param out_pixels   Receives a pointer to the RGBA pixel data.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_PAGE_RANGE, or PDF_STATUS_ERROR_RENDER.
 */
PdfStatus pdf_page_render_thumbnail(
    const PdfDocument *doc,
    int32_t page_index,
    uint32_t max_dimension,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_pixels);

/**
 * @brief Free a pixel buffer returned by @ref pdf_page_render or
 *        @ref pdf_page_render_thumbnail.
 *
 * @par Ownership: CALLER FREES pixels
 * After this call @c pixels is invalid.  Passing NULL is a safe no-op.
 *
 * @param pixels  Pointer returned by a @c pdf_page_render* function, or NULL.
 * @param len     Must equal @c width × height × 4 as reported by the render call.
 */
void pdf_pixels_free(uint8_t *pixels, size_t len);

/* =========================================================================
 * Text extraction
 * =========================================================================
 */

/**
 * @brief Extract all text on a page as a null-terminated UTF-8 string.
 *
 * Text is extracted in reading order (left-to-right, top-to-bottom).
 * The returned string may be empty if the page contains no extractable text.
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL on error or if @c doc / @c page_index
 * is invalid.
 *
 * @param doc        Valid document handle.  BORROWED.
 * @param page_index Zero-based page index.
 * @return Null-terminated UTF-8 string (caller frees), or NULL on error.
 */
char *pdf_page_extract_text(const PdfDocument *doc, int32_t page_index);

/**
 * @brief Extract the document as markdown, in reading order.
 *
 * Preview: output quality is still improving and the API may change in a minor release.
 *
 * Headings become @c # to @c ###, list items @c - and @c 1., a table a pipe
 * table or an HTML one where markdown cannot hold its shape, and a change of
 * page an HTML comment.  The running heads a document repeats are not in it;
 * they are in @ref pdf_document_extract_blocks_json.
 *
 * @par Ownership: CALLER FREES (return value)
 * The caller must free the returned pointer with @ref pdf_string_free.
 * Returns NULL on error.
 *
 * @param doc         Valid document handle.  BORROWED.
 * @param first_page  Zero-based first page of the window.
 * @param page_count  Pages from @c first_page; negative means to the end of
 *                    the document.  A window past the last page is an empty
 *                    string, not an error.
 * @return Null-terminated UTF-8 markdown (caller frees), or NULL on error.
 */
char *pdf_document_extract_markdown(const PdfDocument *doc,
                                    int32_t first_page,
                                    int32_t page_count);

/**
 * @brief Extract the document's structured blocks as JSON, in reading order.
 *
 * Preview: output quality is still improving and the API may change in a minor release.
 *
 * @c {"blocks": [...]}, one entry per paragraph, heading, list item, table
 * cell and running head, each with its kind, page, box, text, spans and place
 * in the reading order.  @c docs/STRUCTURED_EXTRACTION.md is the key to every
 * field.
 *
 * JSON rather than a struct array, because a block carries a different set of
 * fields per kind and a C struct holding all of them would be a tagged union:
 * a second definition of the block model to keep in step with the engine's.
 *
 * @par Ownership: CALLER FREES (return value)
 * The caller must free the returned pointer with @ref pdf_string_free.
 * Returns NULL on error.
 *
 * @param doc         Valid document handle.  BORROWED.
 * @param first_page  Zero-based first page of the window.
 * @param page_count  Pages from @c first_page; negative means to the end of
 *                    the document.
 * @return Null-terminated UTF-8 JSON (caller frees), or NULL on error.
 */
char *pdf_document_extract_blocks_json(const PdfDocument *doc,
                                       int32_t first_page,
                                       int32_t page_count);

/**
 * @brief Extract the document as a Docling @c DoclingDocument, as JSON.
 *
 * Preview: output quality is still improving and the API may change in a minor release.
 *
 * The schema version the engine pins, which docling-core's
 * @c DoclingDocument.model_validate_json loads as it is.  Headings, lists,
 * tables and running heads map onto Docling's labels; what has no mapping is
 * listed in @c docs/STRUCTURED_EXTRACTION.md.
 *
 * @par Ownership: CALLER FREES (return value)
 * The caller must free the returned pointer with @ref pdf_string_free.
 * Returns NULL on error.
 *
 * @param doc         Valid document handle.  BORROWED.
 * @param first_page  Zero-based first page of the window.
 * @param page_count  Pages from @c first_page; negative means to the end of
 *                    the document.
 * @param name        @c DoclingDocument.name, UTF-8.  BORROWED.  NULL means
 *                    @c "document".
 * @return Null-terminated UTF-8 JSON (caller frees), or NULL on error.
 */
char *pdf_document_extract_docling_json(const PdfDocument *doc,
                                        int32_t first_page,
                                        int32_t page_count,
                                        const char *name);

/**
 * @brief Free a string returned by a text or metadata API function.
 *
 * @par Ownership: CALLER FREES s
 * After this call @c s is invalid.  Passing NULL is a safe no-op.
 *
 * @param s  String returned by @ref pdf_page_extract_text,
 *           @ref pdf_document_extract_markdown,
 *           @ref pdf_document_extract_blocks_json,
 *           @ref pdf_document_extract_docling_json,
 *           @ref pdf_document_get_meta, @ref pdf_form_field_name,
 *           @ref pdf_annotation_type, or NULL.
 */
void pdf_string_free(char *s);

/* =========================================================================
 * Metadata
 * =========================================================================
 */

/**
 * @brief Get a document metadata value by key.
 *
 * Supported keys: @c "Title", @c "Author", @c "Subject", @c "Keywords",
 * @c "Creator", @c "Producer".  Keys are case-sensitive.
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL if the key is absent, unknown, or
 * if @c doc is NULL.
 *
 * @param doc  Valid document handle.  BORROWED.
 * @param key  Null-terminated metadata key string.  BORROWED.
 * @return Null-terminated UTF-8 value (caller frees), or NULL.
 */
char *pdf_document_get_meta(const PdfDocument *doc, const char *key);

/* =========================================================================
 * Bookmarks
 * =========================================================================
 */

/**
 * @brief Return the number of top-level bookmarks (outline entries).
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc  Valid document handle, or NULL.
 * @return Count of top-level bookmarks (≥ 0), or 0 if @c doc is NULL.
 */
int32_t pdf_bookmark_count(const PdfDocument *doc);

/* =========================================================================
 * PDF/A compliance
 * =========================================================================
 */

/**
 * @brief Validate a document against a PDF/A conformance level.
 *
 * A @ref PDF_STATUS_OK return means the validation *ran successfully*, not
 * that the document is conformant.  Check the report with
 * @ref pdf_compliance_report_is_compliant.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a heap-allocated report.  The caller must free it
 * with @ref pdf_compliance_report_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc    Valid document handle.
 * @param level  The PDF/A conformance level to validate against.
 * @param out    Writable pointer to receive the compliance report handle.
 * @return PDF_STATUS_OK or PDF_STATUS_ERROR_INVALID_ARG.
 */
PdfStatus pdf_document_validate_pdfa(
    const PdfDocument *doc,
    PdfALevel level,
    PdfComplianceReport **out);

/**
 * @brief Return whether a compliance report indicates full conformance.
 *
 * @par Ownership
 * @c report is BORROWED.
 *
 * @param report  Compliance report handle, or NULL.
 * @return 1 if fully conformant, 0 otherwise (including NULL input).
 */
int32_t pdf_compliance_report_is_compliant(const PdfComplianceReport *report);

/**
 * @brief Return the number of conformance errors recorded in the report.
 *
 * @par Ownership
 * @c report is BORROWED.
 *
 * @param report  Compliance report handle, or NULL.
 * @return Error count (≥ 0), or -1 if @c report is NULL.
 */
int32_t pdf_compliance_report_error_count(const PdfComplianceReport *report);

/**
 * @brief Return the number of warnings recorded in the report.
 *
 * Warnings do not affect conformance: a report with warnings but no
 * errors still reads as conformant.
 *
 * @par Ownership
 * @c report is BORROWED.
 *
 * @param report  Compliance report handle, or NULL.
 * @return Warning count (≥ 0), or -1 if @c report is NULL.
 */
int32_t pdf_compliance_report_warning_count(const PdfComplianceReport *report);

/**
 * @brief Return the number of issues recorded in the report.
 *
 * Counts errors, warnings and informational notes together; see
 * @ref pdf_compliance_report_issue_summary for the per-issue text.
 *
 * @par Ownership
 * @c report is BORROWED.
 *
 * @param report  Compliance report handle, or NULL.
 * @return Issue count (≥ 0), or -1 if @c report is NULL.
 */
int32_t pdf_compliance_report_issue_count(const PdfComplianceReport *report);

/**
 * @brief Return a one-line summary of one issue in the report.
 *
 * The summary reads @c "[severity] rule: message", where severity is
 * @c "error", @c "warning" or @c "info".
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL for an out-of-range index or if
 * @c report is NULL.
 *
 * @param report  Compliance report handle.  BORROWED.
 * @param index   Zero-based issue index.
 * @return Null-terminated summary string (caller frees), or NULL.
 */
char *pdf_compliance_report_issue_summary(
    const PdfComplianceReport *report,
    int32_t index);

/**
 * @brief Free a compliance report handle.
 *
 * @par Ownership: CALLER FREES report
 * After this call @c report is invalid.  Passing NULL is a safe no-op.
 *
 * @param report  Report returned by @ref pdf_document_validate_pdfa, or NULL.
 */
void pdf_compliance_report_free(PdfComplianceReport *report);

/* =========================================================================
 * PDF/A conversion
 * =========================================================================
 */

/**
 * @brief Convert a document to the requested PDF/A conformance level.
 *
 * Produces a new document; the source document is not modified.
 *
 * Delivers @c PDF_A_LEVEL_1B, @c PDF_A_LEVEL_2B and @c PDF_A_LEVEL_3B.  Any
 * other level returns @c PDF_STATUS_ERROR_INVALID_ARG and a message naming
 * these three; it is not written as the B level of its part.  Validation
 * (@ref pdf_document_validate_pdfa) covers every level of @ref PdfALevel.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc    Source document handle.
 * @param level  Target PDF/A conformance level.
 * @param out    Writable pointer to receive the converted document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, PDF_STATUS_ERROR_CORRUPT_PDF,
 *         or PDF_STATUS_ERROR_CONVERT.
 */
PdfStatus pdf_document_convert_pdfa(
    const PdfDocument *doc,
    PdfALevel level,
    PdfDocument **out);

/* =========================================================================
 * Redaction
 * =========================================================================
 */

/**
 * @brief Redact all text occurrences of @c pattern in the document.
 *
 * Returns a new document with the redactions applied.  If @c pattern has no
 * matches the document is returned unchanged (not an error).
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc and @c pattern are BORROWED.
 *
 * @param doc      Source document handle.
 * @param pattern  Null-terminated UTF-8 text pattern to redact.  BORROWED.
 * @param out      Writable pointer to receive the redacted document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_REDACT.
 */
PdfStatus pdf_document_redact(
    const PdfDocument *doc,
    const char *pattern,
    PdfDocument **out);

/* =========================================================================
 * Signing
 * =========================================================================
 */

/**
 * @brief Sign a document using a PKCS#12 identity bundle.
 *
 * Produces a new signed document; the source is not modified.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * All input pointers are BORROWED.
 *
 * @param doc              Source document handle.
 * @param pkcs12_path      Null-terminated path to the .p12 / .pfx file.
 * @param pkcs12_password  Null-terminated password for the bundle, or NULL for
 *                         password-less bundles.
 * @param out              Writable pointer to receive the signed document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_FILE_NOT_FOUND, or PDF_STATUS_ERROR_SIGN.
 */
PdfStatus pdf_document_sign(
    const PdfDocument *doc,
    const char *pkcs12_path,
    const char *pkcs12_password,
    PdfDocument **out);

/* =========================================================================
 * Form fields (AcroForm)
 * =========================================================================
 */

/**
 * @brief Return the number of terminal (leaf) AcroForm fields.
 *
 * Terminal fields are the ones that carry a widget annotation (text boxes,
 * check boxes, radio buttons, etc.).
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc  Valid document handle, or NULL.
 * @return Field count (≥ 0), 0 if no AcroForm is present, or -1 if @c doc is NULL.
 */
int32_t pdf_form_field_count(const PdfDocument *doc);

/**
 * @brief Return the fully qualified name of the AcroForm field at @c index.
 *
 * The name uses dot-separated hierarchy (e.g. @c "parent.child.grandchild").
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL for an out-of-range index, if no
 * AcroForm is present, or if @c doc is NULL or @c index is negative.
 *
 * @param doc    Valid document handle.  BORROWED.
 * @param index  Zero-based field index.
 * @return Null-terminated field name (caller frees), or NULL.
 */
char *pdf_form_field_name(const PdfDocument *doc, int32_t index);

/**
 * @brief Return the field-type string of the AcroForm field at @c index.
 *
 * One of @c "text", @c "button", @c "choice", @c "signature", or
 * @c "unknown" when the type cannot be read.
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL for an out-of-range index, if no
 * AcroForm is present, or if @c doc is NULL or @c index is negative.
 *
 * @param doc    Valid document handle.  BORROWED.
 * @param index  Zero-based field index.
 * @return Null-terminated type string (caller frees), or NULL.
 */
char *pdf_form_field_type(const PdfDocument *doc, int32_t index);

/**
 * @brief Return the current value of the AcroForm field @c name.
 *
 * @c name is the fully qualified name (@c "parent.child" notation) as
 * returned by @ref pdf_form_field_name.  Choice fields with several selected
 * values come back comma-joined.
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL when the field does not exist, carries
 * no value, or the document has no AcroForm.
 *
 * @param doc   Valid document handle.  BORROWED.
 * @param name  Null-terminated fully-qualified field name.  BORROWED.
 * @return Null-terminated field value (caller frees), or NULL.
 */
char *pdf_form_field_value(const PdfDocument *doc, const char *name);

/* =========================================================================
 * Annotations
 * =========================================================================
 */

/**
 * @brief Return the number of annotations on a page.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Annotation count (≥ 0), or -1 on error (null doc, invalid page).
 */
int32_t pdf_annotation_count(const PdfDocument *doc, int32_t page_index);

/**
 * @brief Return the subtype string of an annotation.
 *
 * Examples: @c "Text", @c "Link", @c "Highlight", @c "Ink", @c "Widget".
 *
 * @par Ownership: CALLER FREES (return value)
 * The returned pointer is heap-allocated.  The caller must free it with
 * @ref pdf_string_free.  Returns NULL for an out-of-range index or on error.
 *
 * @param doc          Valid document handle.  BORROWED.
 * @param page_index   Zero-based page index.
 * @param annot_index  Zero-based annotation index on the page.
 * @return Null-terminated subtype string (caller frees), or NULL.
 */
char *pdf_annotation_type(
    const PdfDocument *doc,
    int32_t page_index,
    int32_t annot_index);

/**
 * @brief Return the bounding box of an annotation.
 *
 * Edges are in PDF user-space points (@c x0, @c y0, @c x1, @c y1 with y
 * growing upward).  Annotations without a usable @c /Rect read back as
 * zeros.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc          Valid document handle.
 * @param page_index   Zero-based page index.
 * @param annot_index  Zero-based annotation index on the page.
 * @param out_x0       Receives the left edge.  Must be non-null.
 * @param out_y0       Receives the bottom edge.  Must be non-null.
 * @param out_x1       Receives the right edge.  Must be non-null.
 * @param out_y1       Receives the top edge.  Must be non-null.
 * @return PDF_STATUS_OK on success, or an error code.
 */
PdfStatus pdf_annotation_rect(
    const PdfDocument *doc,
    int32_t page_index,
    int32_t annot_index,
    double *out_x0,
    double *out_y0,
    double *out_x1,
    double *out_y1);

/**
 * @brief Add a yellow highlight annotation to a page and return a new document.
 *
 * Coordinates are in PDF user-space points (origin bottom-left, y upward).
 * The source document is not modified.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc        Source document handle.
 * @param page_index Zero-based page index.
 * @param x          Left edge of the highlight rectangle (points).
 * @param y          Bottom edge of the highlight rectangle (points).
 * @param w          Width of the highlight rectangle (points).
 * @param h          Height of the highlight rectangle (points).
 * @param out        Writable pointer to receive the modified document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_ANNOTATION.
 */
PdfStatus pdf_annotation_add_highlight(
    const PdfDocument *doc,
    int32_t page_index,
    double x, double y,
    double w, double h,
    PdfDocument **out);

/**
 * @brief Add a yellow highlight annotation with @c /Contents text.
 *
 * Behaves like @ref pdf_annotation_add_highlight, except the highlight
 * carries @c contents as its @c /Contents string.  Null or empty
 * @c contents stores no @c /Contents entry.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc        Source document handle.
 * @param page_index Zero-based page index.
 * @param x          Left edge of the highlight rectangle (points).
 * @param y          Bottom edge of the highlight rectangle (points).
 * @param w          Width of the highlight rectangle (points).
 * @param h          Height of the highlight rectangle (points).
 * @param contents   Null-terminated contents text, or NULL.  BORROWED.
 * @param out        Writable pointer to receive the modified document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_ANNOTATION.
 */
PdfStatus pdf_annotation_add_highlight_with_contents(
    const PdfDocument *doc,
    int32_t page_index,
    double x, double y,
    double w, double h,
    const char *contents,
    PdfDocument **out);

/* =========================================================================
 * Document merge
 * =========================================================================
 */

/**
 * @brief Merge @c count documents into a single new document.
 *
 * Pages are appended in the order they appear in @c docs.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c docs and each element within it are BORROWED.
 *
 * @param docs   Array of @c count document pointers (all must be non-NULL).
 *               BORROWED.
 * @param count  Number of documents to merge (must be > 0).
 * @param out    Writable pointer to receive the merged document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_MERGE.
 */
PdfStatus pdf_documents_merge(
    const PdfDocument *const *docs,
    int32_t count,
    PdfDocument **out);

/* =========================================================================
 * Signature verification
 * =========================================================================
 */

/**
 * @brief Return the number of signature fields in the document.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc  Valid document handle, or NULL.
 * @return Signature count (≥ 0), or -1 if @c doc is NULL.
 */
int32_t pdf_signature_count(const PdfDocument *doc);

/**
 * @brief Validate the digital signature at zero-based @c index.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc    Valid document handle, or NULL.
 * @param index  Zero-based signature index.
 * @return 1 if the signature is cryptographically valid, 0 if invalid,
 *         -1 if the status is unknown or on error (null doc, out-of-range index).
 */
int32_t pdf_signature_is_valid(const PdfDocument *doc, int32_t index);

/* =========================================================================
 * Image extraction
 * =========================================================================
 */

/**
 * @brief Return the number of images on a page.
 *
 * @par Ownership
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Image count (≥ 0), or -1 on error.
 */
int32_t pdf_page_image_count(const PdfDocument *doc, int32_t page_index);

/**
 * @brief Extract the raw decoded pixels of an image on a page.
 *
 * The buffer format is image-dependent (may be raw RGB, RGBA, or grayscale
 * depending on the source image).  Check @c out_width / @c out_height for
 * dimensions.
 *
 * @par Ownership: CALLER FREES *out_data
 * On success, @c *out_data points to a heap-allocated buffer of @c *out_len
 * bytes.  The caller must free it with
 * <tt>pdf_bytes_free(*out_data, *out_len)</tt>.  On failure @c *out_data is
 * set to NULL and @c *out_len to 0.
 * @c doc is BORROWED.
 *
 * @param doc          Valid document handle.  BORROWED.
 * @param page_index   Zero-based page index.
 * @param image_index  Zero-based image index on the page.
 * @param out_width    Receives the image width in pixels.
 * @param out_height   Receives the image height in pixels.
 * @param out_data     Receives a pointer to the image byte data.
 * @param out_len      Receives the byte length of @c *out_data.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_EXTRACT.
 */
PdfStatus pdf_page_extract_image(
    const PdfDocument *doc,
    int32_t page_index,
    int32_t image_index,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_data,
    size_t *out_len);

/* =========================================================================
 * Office export
 * =========================================================================
 */

/**
 * @brief Convert the document to a Word (.docx) package.
 *
 * @par Ownership: CALLER FREES @c *out_data via @ref pdf_bytes_free.
 *
 * @param doc       Document handle.
 * @param out_data  Receives a pointer to the .docx bytes.
 * @param out_len   Receives the byte length of @c *out_data.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_CONVERT.
 */
PdfStatus pdf_document_to_docx(
    const PdfDocument *doc,
    uint8_t **out_data,
    size_t *out_len);

/**
 * @brief Convert the document to an Excel (.xlsx) package.
 *
 * Tables are detected from the page layout; a PDF without tabular structure
 * yields a workbook with the text it could place.
 *
 * @par Ownership: CALLER FREES @c *out_data via @ref pdf_bytes_free.
 * @see pdf_document_to_docx
 */
PdfStatus pdf_document_to_xlsx(
    const PdfDocument *doc,
    uint8_t **out_data,
    size_t *out_len);

/**
 * @brief Convert the document to a PowerPoint (.pptx) package, one slide per
 *        page.
 *
 * @par Ownership: CALLER FREES @c *out_data via @ref pdf_bytes_free.
 * @see pdf_document_to_docx
 */
PdfStatus pdf_document_to_pptx(
    const PdfDocument *doc,
    uint8_t **out_data,
    size_t *out_len);

/**
 * @brief Free a byte buffer returned by @ref pdf_page_extract_image.
 *
 * @par Ownership: CALLER FREES data
 * After this call @c data is invalid.  Passing NULL is a safe no-op.
 *
 * @param data  Pointer returned by @ref pdf_page_extract_image, or NULL.
 * @param len   Must equal @c *out_len as returned by the extract call.
 */
void pdf_bytes_free(uint8_t *data, size_t len);

/* =========================================================================
 * Plain embedded-image API (facade)
 * =========================================================================
 * Listing plus extraction and pixel decoding over the document's stored
 * images. Stable since 1.1.0; the struct layouts below are frozen for
 * the 1.x line.
 */

/**
 * @brief List the images stored on one page as a JSON array.
 *
 * <tt>[{"id":…,"width":…,"height":…,"bits_per_component":…,"components":…,
 * "color_space":…,"color_space_name":…,"filter":…,"name":…,"has_mask":…}]</tt>,
 * one entry per image XObject in the page's resources, in resource order.
 *
 * @par Ownership: CALLER FREES the returned string via @ref pdf_string_free.
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle, or NULL.
 * @param page_index Zero-based page index.
 * @return Null-terminated UTF-8 JSON array, or NULL on error.
 */
char *pdf_page_images_json(const PdfDocument *doc, int32_t page_index);

/**
 * @brief One extracted image: header facts plus heap-allocated fields.
 *
 * Memory ownership: after a successful @ref pdf_image_extract_data call
 * the @c color_space, @c color_space_name, @c encoding and @c data fields
 * are heap allocations the caller MUST release via
 * @ref pdf_extracted_image_free, which frees the fields but not the
 * struct itself. Do NOT call @c free() on the fields, and do NOT mix
 * allocators.
 */
typedef struct {
    /** Image width, in pixels. */
    uint32_t width;
    /** Image height, in pixels. */
    uint32_t height;
    /** Bits per component. */
    uint32_t bits_per_component;
    /** Components per pixel. */
    uint32_t components;
    /** Decoded pixel layout: "DeviceGray", "DeviceRGB" or "DeviceCMYK". */
    char *color_space;
    /** Colour-space name recorded in the image dictionary. */
    char *color_space_name;
    /** Encoding of @c data: "jpeg" or "png". */
    char *encoding;
    /** Encoded image bytes, @c len of them. */
    uint8_t *data;
    /** Length of @c data, in bytes. */
    size_t len;
    /** 1 when the image is masked, else 0. */
    uint8_t masked;
    /** Horizontal resolution, in dots per inch. */
    uint32_t xres;
    /** Vertical resolution, in dots per inch. */
    uint32_t yres;
} PdfExtractedImage;

/**
 * @brief One decoded image: raw samples plus heap-allocated fields.
 *
 * Memory ownership: after a successful @ref pdf_image_decode_pixels call
 * the @c color_space and @c pixels fields are heap allocations the caller
 * MUST release via @ref pdf_decoded_image_free, which frees the fields
 * but not the struct itself. Do NOT call @c free() on the fields, and do
 * NOT mix allocators.
 */
typedef struct {
    /** Image width, in pixels. */
    uint32_t width;
    /** Image height, in pixels. */
    uint32_t height;
    /** Components per pixel. */
    uint32_t components;
    /** Decoded pixel layout: "DeviceGray", "DeviceRGB" or "DeviceCMYK". */
    char *color_space;
    /** Raw samples, @c components bytes per pixel, top row first. */
    uint8_t *pixels;
    /** Length of @c pixels, in bytes. */
    size_t len;
} PdfDecodedImage;

/**
 * @brief Extract one stored image: header facts plus encoded bytes.
 *
 * JPEG payloads pass through untouched; everything else arrives decoded
 * and re-encoded as PNG.
 *
 * @par Ownership: CALLER FREES @c *out via @ref pdf_extracted_image_free.
 * @c doc is BORROWED.
 *
 * @param doc      Valid document handle.  BORROWED.
 * @param image_id The @c id a @ref pdf_page_images_json entry reported.
 * @param out      Receives the extracted image.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_EXTRACT.
 */
PdfStatus pdf_image_extract_data(
    const PdfDocument *doc,
    uint32_t image_id,
    PdfExtractedImage *out);

/**
 * @brief Free the heap fields of a @ref PdfExtractedImage.
 *
 * @par Ownership: CALLER FREES img
 * After this call the fields are NULL/0.  Passing NULL is a safe no-op.
 *
 * @param img  Struct previously filled by @ref pdf_image_extract_data,
 *             or NULL.
 */
void pdf_extracted_image_free(PdfExtractedImage *img);

/**
 * @brief Decode one stored image to raw samples, top row first.
 *
 * Palettes and stencils are expanded.
 *
 * @par Ownership: CALLER FREES @c *out via @ref pdf_decoded_image_free.
 * @c doc is BORROWED.
 *
 * @param doc      Valid document handle.  BORROWED.
 * @param image_id The @c id a @ref pdf_page_images_json entry reported.
 * @param out      Receives the decoded image.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_EXTRACT.
 */
PdfStatus pdf_image_decode_pixels(
    const PdfDocument *doc,
    uint32_t image_id,
    PdfDecodedImage *out);

/**
 * @brief Free the heap fields of a @ref PdfDecodedImage.
 *
 * @par Ownership: CALLER FREES img
 * After this call the fields are NULL/0.  Passing NULL is a safe no-op.
 *
 * @param img  Struct previously filled by @ref pdf_image_decode_pixels,
 *             or NULL.
 */
void pdf_decoded_image_free(PdfDecodedImage *img);

/* =========================================================================
 * Text search
 * =========================================================================
 */

/**
 * @brief Count total occurrences of @c query across all pages.
 *
 * The search is case-sensitive and plain-text (no regex).
 *
 * @par Ownership
 * @c doc and @c query are BORROWED.
 *
 * @param doc    Valid document handle, or NULL.
 * @param query  Null-terminated UTF-8 search string.  BORROWED.
 * @return Match count (≥ 0), or -1 on error.
 */
int32_t pdf_document_search_count(const PdfDocument *doc, const char *query);

/* =========================================================================
 * Document split
 * =========================================================================
 */

/**
 * @brief Extract a page range into a new document.
 *
 * Both @c from_page and @c to_page are zero-based and inclusive.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc       Source document handle.
 * @param from_page First page to include (0-based, inclusive).
 * @param to_page   Last page to include (0-based, inclusive, ≥ @c from_page).
 * @param out       Writable pointer to receive the extracted document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_SPLIT.
 */
PdfStatus pdf_document_split_range(
    const PdfDocument *doc,
    int32_t from_page,
    int32_t to_page,
    PdfDocument **out);

/* =========================================================================
 * Watermark
 * =========================================================================
 */

/**
 * @brief Apply a diagonal text watermark to all pages and return a new document.
 *
 * The watermark is rendered in semi-transparent grey at 45 degrees, centred on
 * each page.  The source document is not modified.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc and @c text are BORROWED.
 *
 * @param doc   Source document handle.
 * @param text  Null-terminated UTF-8 watermark text.  BORROWED.
 * @param out   Writable pointer to receive the watermarked document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_WATERMARK.
 */
PdfStatus pdf_document_add_watermark(
    const PdfDocument *doc,
    const char *text,
    PdfDocument **out);

/* =========================================================================
 * Compression
 * =========================================================================
 */

/**
 * @brief Compress stream objects and return a new (smaller) document.
 *
 * Applies zlib/Deflate compression to uncompressed streams.  The source
 * document is not modified.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  The caller must free
 * it with @ref pdf_document_free.  On failure @c *out is set to NULL.
 * @c doc is BORROWED.
 *
 * @param doc  Source document handle.
 * @param out  Writable pointer to receive the compressed document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CORRUPT_PDF, or PDF_STATUS_ERROR_COMPRESS.
 */
PdfStatus pdf_document_compress(
    const PdfDocument *doc,
    PdfDocument **out);

/* =========================================================================
 * Structured text-block extraction
 * =========================================================================
 * Returns the per-block bounding boxes + concatenated text for a page.
 * Stable since 1.x; the struct layout below is frozen for the 1.x line.
 */

/**
 * @brief A single text block with a bounding box and concatenated text.
 *
 * Memory ownership: every block pointer returned by
 * @ref pdf_page_extract_text_blocks is part of one heap allocation that
 * the caller MUST release via @ref pdf_text_blocks_free. The embedded
 * @c text pointer points into Rust-owned storage that is freed together
 * with the block array. Do NOT call @c free() on individual @c text
 * pointers, and do NOT mix allocators.
 */
typedef struct {
    /** PDF user-space X of the block's bottom-left corner (1/72 inch). */
    double x;
    /** PDF user-space Y of the block's bottom-left corner (1/72 inch). */
    double y;
    /** Block width in PDF points.  Always >= 0; 0 for empty blocks. */
    double width;
    /** Block height in PDF points. Always >= 0; 0 for empty blocks. */
    double height;
    /** UTF-8, null-terminated. Lifetime tied to the block array. */
    const char *text;
} PdfTextBlock;

/**
 * @brief Extract the structured text blocks of a single page.
 *
 * On success: <tt>*out_blocks</tt> is set to a heap-allocated array of
 * @ref PdfTextBlock (or @c NULL when the page is text-empty),
 * <tt>*out_count</tt> holds the number of elements. The caller MUST
 * release the array via <tt>pdf_text_blocks_free(*out_blocks,
 * *out_count)</tt> when finished.
 *
 * On failure: <tt>*out_blocks</tt> is set to @c NULL,
 * <tt>*out_count</tt> to @c 0, and the function returns a non-Ok
 * @ref PdfStatus. Inspect @ref pdf_get_last_error for the message.
 *
 * @param doc        Document handle. Must not be NULL.
 * @param page_index Zero-based page index.
 * @param out_blocks Pointer to a caller-owned @c PdfTextBlock* slot.
 *                   MUST not be NULL.
 * @param out_count  Pointer to a caller-owned size_t slot. MUST not be
 *                   NULL.
 *
 * @return @ref PDF_STATUS_OK on success (possibly with zero blocks).
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG when any argument is NULL
 *         or @c page_index is negative.
 * @return @ref PDF_STATUS_ERROR_PAGE_RANGE when @c page_index is
 *         outside @c [0, page_count).
 * @return @ref PDF_STATUS_ERROR_EXTRACT for engine-side extraction
 *         failures (message in @c pdf_get_last_error).
 */
PdfStatus pdf_page_extract_text_blocks(
    const PdfDocument *doc,
    int                page_index,
    PdfTextBlock     **out_blocks,
    size_t            *out_count
);

/**
 * @brief Release an array of @ref PdfTextBlock previously returned by
 *        @ref pdf_page_extract_text_blocks.
 *
 * <tt>pdf_text_blocks_free(NULL, 0)</tt> is a no-op. The @c count
 * argument MUST match the value written by
 * @ref pdf_page_extract_text_blocks into @c *out_count.
 *
 * @param blocks Pointer previously returned by
 *               @ref pdf_page_extract_text_blocks (or NULL).
 * @param count  Number of elements in @c blocks (or 0).
 */
void pdf_text_blocks_free(PdfTextBlock *blocks, size_t count);

/* =========================================================================
 * G-track text-editing extensions (future / opt-in)
 * =========================================================================
 * These symbols are only declared when PDFLUENT_TEXT_EDITING is defined.
 * They map to G1–G4 program branches and are not yet in stable releases.
 *
 * Bindings (Python / .NET / Java) must guard the corresponding generated
 * code behind the same capability flag.
 */

#ifdef PDFLUENT_TEXT_EDITING

/**
 * @brief A single text run with font and colour metadata (G1).
 *
 * Returned as part of a @ref PdfTextSpanArray from
 * @ref pdf_page_extract_text_spans.
 */
typedef struct {
    /** Null-terminated UTF-8 text content of the run. LIBRARY OWNS — do not free. */
    const char *text;
    /** Null-terminated font family name (e.g. "Helvetica").  LIBRARY OWNS. */
    const char *font_name;
    /** Font size in points. */
    float       font_size;
    /** 1 if the text is bold, 0 otherwise. */
    int         is_bold;
    /** 1 if the text is italic, 0 otherwise. */
    int         is_italic;
    /** Packed ARGB colour: 0xAARRGGBB. */
    uint32_t    color;
    /** Left edge in PDF user-space points. */
    double      x;
    /** Bottom edge in PDF user-space points. */
    double      y;
    /** Width in PDF user-space points. */
    double      width;
    /** Height in PDF user-space points. */
    double      height;
} PdfTextSpan;

/**
 * @brief An array of @ref PdfTextSpan returned by @ref pdf_page_extract_text_spans.
 */
typedef struct {
    /** Pointer to the first span. CALLER FREES via pdf_text_span_array_free. */
    PdfTextSpan *spans;
    /** Number of elements in @c spans. */
    size_t       count;
} PdfTextSpanArray;

/**
 * @brief Extract text runs with font and colour metadata from a page (G1).
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c out->spans is heap-allocated.  The caller must free the
 * entire array with @ref pdf_text_span_array_free.  The @c text and
 * @c font_name pointers inside each span are owned by the array and become
 * invalid after the free.
 * @c doc is BORROWED.
 *
 * @param doc        Valid document handle.  BORROWED.
 * @param page_index Zero-based page index.
 * @param out        Writable pointer to receive the span array.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_PAGE_RANGE, or PDF_STATUS_ERROR_EXTRACT.
 */
PdfStatus pdf_page_extract_text_spans(
    const PdfDocument *doc,
    int32_t page_index,
    PdfTextSpanArray *out);

/**
 * @brief Free a @ref PdfTextSpanArray returned by @ref pdf_page_extract_text_spans.
 *
 * @par Ownership: CALLER FREES array.spans
 * After this call @c array->spans and all interior string pointers are invalid.
 * Passing a zeroed struct is safe (no-op).
 *
 * @param array  Pointer to the span array to free.
 */
void pdf_text_span_array_free(PdfTextSpanArray *array);

/**
 * @brief Set the value of an AcroForm field by fully-qualified name (G3).
 *
 * Returns a new document with the updated field value.  The source document
 * is not modified.
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  Free with
 * @ref pdf_document_free.  @c doc and all strings are BORROWED.
 *
 * @param doc    Source document handle.
 * @param name   Null-terminated fully-qualified field name.  BORROWED.
 * @param value  Null-terminated UTF-8 value to set.  BORROWED.
 * @param out    Writable pointer to receive the updated document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_UNKNOWN.
 */
PdfStatus pdf_form_set_field_value(
    const PdfDocument *doc,
    const char *name,
    const char *value,
    PdfDocument **out);

/**
 * @brief Fill an XFA form field by SOM path and return a new document (G4).
 *
 * SOM paths use dot notation, e.g. @c "form1.subform1.field1".
 *
 * @par Ownership: CALLER FREES *out
 * On success, @c *out is a new heap-allocated document.  Free with
 * @ref pdf_document_free.  @c doc and all strings are BORROWED.
 *
 * @param doc      Source document handle.
 * @param som_path Null-terminated SOM path to the XFA field.  BORROWED.
 * @param value    Null-terminated UTF-8 value to set.  BORROWED.
 * @param out      Writable pointer to receive the updated document.
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG, or
 *         PDF_STATUS_ERROR_UNKNOWN.
 */
PdfStatus pdf_xfa_set_field_value(
    const PdfDocument *doc,
    const char *som_path,
    const char *value,
    PdfDocument **out);

#endif /* PDFLUENT_TEXT_EDITING */

/* =========================================================================
 * Compatible API (lane L5) — C projection of `pdfluent::compat`
 * =========================================================================
 * One `pdf_compat_*` function per bound spec member (T1/T2). Conventions:
 * - Every function returns PdfStatus, except `pdf_compat_last_error_kind`
 *   and the void `_free` functions (null-safe no-ops).
 * - Pages, annotations, and shapes travel by id (document, page, xref);
 *   only owned values have handles. A negative page index fails with
 *   PDF_STATUS_ERROR_PAGE_RANGE.
 * - Strings cross as null-terminated UTF-8; `char **` outs are freed with
 *   `pdf_string_free`. Byte and numeric arrays are freed with
 *   `pdf_bytes_free` and their byte length (element count x element size).
 * - Option structs with more than a few fields cross as a single nullable
 *   JSON object string (null = defaults); keys are the Rust field names.
 *   Smaller options cross as explicit parameters.
 * - Colour spaces cross as their channel count: 1 grey, 3 RGB, 4 CMYK.
 * - `pdf_compat_last_error_kind` reports the compat failure kind behind the
 *   last error: one of the PDF_COMPAT_KIND_* codes below, or 0.
 */

/* ---- Opaque handles ---- */

typedef struct PdfCompatDocument PdfCompatDocument;
typedef struct PdfCompatPixmap PdfCompatPixmap;
typedef struct PdfCompatFont PdfCompatFont;
typedef struct PdfCompatTextWriter PdfCompatTextWriter;
typedef struct PdfCompatWidget PdfCompatWidget;
typedef struct PdfCompatLink PdfCompatLink;
typedef struct PdfCompatTableFinder PdfCompatTableFinder;
typedef struct PdfCompatTextOutput PdfCompatTextOutput;
typedef struct PdfCompatSearchHits PdfCompatSearchHits;
typedef struct PdfCompatTocList PdfCompatTocList;
typedef struct PdfCompatFontInfoList PdfCompatFontInfoList;
typedef struct PdfCompatImageRefList PdfCompatImageRefList;
typedef struct PdfCompatPlacedImageList PdfCompatPlacedImageList;
typedef struct PdfCompatRectList PdfCompatRectList;
typedef struct PdfCompatDrawingList PdfCompatDrawingList;
typedef struct PdfCompatLinkList PdfCompatLinkList;
typedef struct PdfCompatWidgetList PdfCompatWidgetList;
typedef struct PdfCompatAnnotEntryList PdfCompatAnnotEntryList;
typedef struct PdfCompatTextTraceList PdfCompatTextTraceList;
typedef struct PdfCompatImageData PdfCompatImageData;
typedef struct PdfCompatFontData PdfCompatFontData;
typedef struct PdfCompatTocBuilder PdfCompatTocBuilder;

/* ---- Geometry values (fixed layout; the ABI's only non-opaque structs) ---- */

typedef struct { double x, y; } PdfCompatPoint;
typedef struct { double x0, y0, x1, y1; } PdfCompatRect;
typedef struct { int32_t x0, y0, x1, y1; } PdfCompatIRect;
typedef struct { double a, b, c, d, e, f; } PdfCompatMatrix;
typedef struct { PdfCompatPoint ul, ur, ll, lr; } PdfCompatQuad;

/* ---- Compat failure kinds (see pdf_compat_last_error_kind) ---- */

#define PDF_COMPAT_KIND_NONE 0
#define PDF_COMPAT_KIND_WRONG_TYPE 1
#define PDF_COMPAT_KIND_BAD_VALUE 2
#define PDF_COMPAT_KIND_UNKNOWN_KEY 3
#define PDF_COMPAT_KIND_INDEX_OUT_OF_RANGE 4
#define PDF_COMPAT_KIND_MISSING_FILE 5
#define PDF_COMPAT_KIND_UNREADABLE_FILE 6
#define PDF_COMPAT_KIND_EMPTY_FILE 7
#define PDF_COMPAT_KIND_ENGINE_FAULT 8
#define PDF_COMPAT_KIND_MISSING_COMPONENT 9
#define PDF_COMPAT_KIND_DEPRECATION 10

/* ---- Functions ---- */

/** Build a point from its coordinates. */
PdfStatus pdf_compat_point_new(double x, double y, PdfCompatPoint * out);

/** Build a rectangle from its four edges. */
PdfStatus pdf_compat_rect_new(double x0, double y0, double x1, double y1, PdfCompatRect * out);

/** Build an integer rectangle from its four edges. */
PdfStatus pdf_compat_irect_new(
    int32_t x0,
    int32_t y0,
    int32_t x1,
    int32_t y1,
    PdfCompatIRect * out);

/** Build a matrix from its six coefficients. */
PdfStatus pdf_compat_matrix_new(
    double a,
    double b,
    double c,
    double d,
    double e,
    double f,
    PdfCompatMatrix * out);

/** Whether a rectangle contains another: 1 yes, 0 no. */
PdfStatus pdf_compat_rect_contains(
    const PdfCompatRect * rect,
    const PdfCompatRect * other,
    int32_t * out);

/** Rectangle area, in square points. */
PdfStatus pdf_compat_rect_get_area(const PdfCompatRect * rect, double * out);

/** Rectangle height, in points (`y1 - y0`). */
PdfStatus pdf_compat_rect_get_height(const PdfCompatRect * rect, double * out);

/** Rectangle width, in points (`x1 - x0`). */
PdfStatus pdf_compat_rect_get_width(const PdfCompatRect * rect, double * out);

/** Grow a rectangle in place to include another; writes the new value back. */
PdfStatus pdf_compat_rect_include_rect(
    PdfCompatRect * rect,
    const PdfCompatRect * other,
    PdfCompatRect * out);

/** Intersect a rectangle in place with another; writes the new value back. */
PdfStatus pdf_compat_rect_intersect(
    PdfCompatRect * rect,
    const PdfCompatRect * other,
    PdfCompatRect * out);

/** Whether two rectangles overlap: 1 yes, 0 no. */
PdfStatus pdf_compat_rect_intersects(
    const PdfCompatRect * rect,
    const PdfCompatRect * other,
    int32_t * out);

/** Whether a rectangle is empty: 1 yes, 0 no. */
PdfStatus pdf_compat_rect_is_empty(const PdfCompatRect * rect, int32_t * out);

/** Whether a rectangle is infinite: 1 yes, 0 no. */
PdfStatus pdf_compat_rect_is_infinite(const PdfCompatRect * rect, int32_t * out);

/** Normalize a rectangle in place; writes the new value back. */
PdfStatus pdf_compat_rect_normalize(PdfCompatRect * rect, PdfCompatRect * out);

/** Rotate a matrix in place by a quarter-turn count; writes the new value. */
PdfStatus pdf_compat_matrix_prerotate(
    PdfCompatMatrix * matrix,
    double theta,
    PdfCompatMatrix * out);

/** Open a file, bytes, or a fresh shell. [CALLER FREES *out] */
PdfStatus pdf_compat_document_open(
    const char * filename,
    const uint8_t * data,
    size_t data_len,
    const char * filetype,
    const PdfCompatRect * rect,
    double width,
    double height,
    double fontsize,
    const char * archive,
    PdfCompatDocument ** out);

/** Unlock with a password. */
PdfStatus pdf_compat_document_authenticate(
    const PdfCompatDocument * doc,
    const char * password,
    int32_t * out);

/** Flatten annotations and widgets into content. */
PdfStatus pdf_compat_document_bake(PdfCompatDocument * doc, int32_t annots, int32_t widgets);

/** Close the document and free its handle. */
PdfStatus pdf_compat_document_close(PdfCompatDocument * doc);

/** Convert a page span to PDF content. [CALLER FREES *out_data] */
PdfStatus pdf_compat_document_convert_to_pdf(
    PdfCompatDocument * doc,
    int32_t from_page,
    int32_t to_page,
    int32_t rotate,
    uint8_t ** out_data,
    size_t * out_len);

/** Font bytes and facts. [CALLER FREES *out] */
PdfStatus pdf_compat_document_extract_font(
    const PdfCompatDocument * doc,
    int32_t xref,
    int32_t info_only,
    const char * name,
    PdfCompatFontData ** out);

/** Image bytes and facts for an object number. [CALLER FREES *out] */
PdfStatus pdf_compat_document_extract_image(
    const PdfCompatDocument * doc,
    int32_t xref,
    PdfCompatImageData ** out);

/** Reserve a fresh object number. */
PdfStatus pdf_compat_document_get_new_xref(PdfCompatDocument * doc, int32_t * out);

/** Fonts referenced by a page. [CALLER FREES *out] */
PdfStatus pdf_compat_document_get_page_fonts(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t full,
    PdfCompatFontInfoList ** out);

/** Images referenced by a page. [CALLER FREES *out] */
PdfStatus pdf_compat_document_get_page_images(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t full,
    PdfCompatImageRefList ** out);

/** Outline entries. [CALLER FREES *out] */
PdfStatus pdf_compat_document_get_toc(
    const PdfCompatDocument * doc,
    int32_t simple,
    PdfCompatTocList ** out);

/** Insert pages from another document. */
PdfStatus pdf_compat_document_insert_pdf(
    PdfCompatDocument * doc,
    const PdfCompatDocument * source,
    int32_t from_page,
    int32_t to_page,
    int32_t start_at,
    int32_t rotate,
    int32_t links,
    int32_t annots,
    int32_t widgets,
    int32_t join_duplicates,
    int32_t show_progress,
    int32_t finish);

/** Whether this is a form PDF. */
PdfStatus pdf_compat_document_is_form_pdf(const PdfCompatDocument * doc, int32_t * out);

/** Whether this is a PDF document. */
PdfStatus pdf_compat_document_is_pdf(const PdfCompatDocument * doc, int32_t * out);

/** Undo/redo availability of the compatible journal. */
PdfStatus pdf_compat_document_journal_can_do(const PdfCompatDocument * doc, int32_t * can_undo, int32_t * can_redo);

/** Enable the compatible journal. */
PdfStatus pdf_compat_document_journal_enable(PdfCompatDocument * doc);

/** Whether the compatible journal is enabled. */
PdfStatus pdf_compat_document_journal_is_enabled(const PdfCompatDocument * doc, int32_t * out);

/** Load a journal file over a never-enabled journal. */
PdfStatus pdf_compat_document_journal_load(PdfCompatDocument * doc, const char * path);

/** Journal step name. CALLER FREES with @ref pdf_string_free. */
PdfStatus pdf_compat_document_journal_op_name(const PdfCompatDocument * doc, int32_t step, char ** out);

/** Journal position: the applied prefix length. */
PdfStatus pdf_compat_document_journal_position(const PdfCompatDocument * doc, int32_t * out);

/** Redo a journal step. */
PdfStatus pdf_compat_document_journal_redo(PdfCompatDocument * doc);

/** Save the journal file. */
PdfStatus pdf_compat_document_journal_save(PdfCompatDocument * doc, const char * path);

/** Start a journal step; the name may be NULL for an anonymous step. */
PdfStatus pdf_compat_document_journal_start_op(PdfCompatDocument * doc, const char * name);

/** Stop the journal step. */
PdfStatus pdf_compat_document_journal_stop_op(PdfCompatDocument * doc);

/** Undo a journal step. */
PdfStatus pdf_compat_document_journal_undo(PdfCompatDocument * doc);

/** Validate a page number; returns it. */
PdfStatus pdf_compat_document_load_page(const PdfCompatDocument * doc, int32_t page, int32_t * out);

/** Whether the document needs a password. */
PdfStatus pdf_compat_document_needs_pass(const PdfCompatDocument * doc, int32_t * out);

/** Insert a fresh page; returns its number. */
PdfStatus pdf_compat_document_new_page(
    PdfCompatDocument * doc,
    int32_t at,
    double width,
    double height,
    int32_t * out);

/** Number of pages. Binds PdfDocument::len; the spec names no page_count (reported). */
PdfStatus pdf_compat_document_page_count(const PdfCompatDocument * doc, int32_t * out);

/** The document catalog. [CALLER FREES *out] */
PdfStatus pdf_compat_document_pdf_catalog(const PdfCompatDocument * doc, char ** out);

/** Save to a file. */
PdfStatus pdf_compat_document_save(
    const PdfCompatDocument * doc,
    const char * path,
    const char * settings);

/** Save incrementally. */
PdfStatus pdf_compat_document_save_incr(const PdfCompatDocument * doc);

/** Replace the document metadata. */
PdfStatus pdf_compat_document_set_metadata(
    PdfCompatDocument * doc,
    const char * title,
    const char * author,
    const char * subject,
    const char * keywords,
    const char * creator,
    const char * producer,
    const char * creation_date,
    const char * mod_date,
    const char * format,
    const char * encryption,
    const char * trapped);

/** Replace the outline; returns its size. */
PdfStatus pdf_compat_document_set_toc(
    PdfCompatDocument * doc,
    const PdfCompatTocBuilder * toc,
    int32_t collapse,
    int32_t * out);

/** Serialise the document to bytes. [CALLER FREES *out_data] */
PdfStatus pdf_compat_document_tobytes(
    const PdfCompatDocument * doc,
    const char * settings,
    uint8_t ** out_data,
    size_t * out_len);

/** Replace one raw object. */
PdfStatus pdf_compat_document_update_object(
    PdfCompatDocument * doc,
    int32_t xref,
    const char * text,
    int32_t page);

/** Replace one stream payload. */
PdfStatus pdf_compat_document_update_stream(
    PdfCompatDocument * doc,
    int32_t xref,
    const uint8_t * data,
    size_t data_len,
    int32_t is_new,
    int32_t compress);

/** Write the document to bytes. [CALLER FREES *out_data] */
PdfStatus pdf_compat_document_write(
    const PdfCompatDocument * doc,
    const char * settings,
    uint8_t ** out_data,
    size_t * out_len);

/** One object key: value and type. [CALLER FREES *out] [CALLER FREES *out2] */
PdfStatus pdf_compat_document_xref_get_key(
    const PdfCompatDocument * doc,
    int32_t xref,
    const char * key,
    char ** out,
    char ** out2);

/** Object count. */
PdfStatus pdf_compat_document_xref_length(const PdfCompatDocument * doc, int32_t * out);

/** One raw object. [CALLER FREES *out] */
PdfStatus pdf_compat_document_xref_object(
    const PdfCompatDocument * doc,
    int32_t xref,
    int32_t compressed,
    int32_t ascii,
    char ** out);

/** Set one object key. */
PdfStatus pdf_compat_document_xref_set_key(
    PdfCompatDocument * doc,
    int32_t xref,
    const char * key,
    const char * value);

/** One stream payload. [CALLER FREES *out_data] */
PdfStatus pdf_compat_document_xref_stream(
    const PdfCompatDocument * doc,
    int32_t xref,
    uint8_t ** out_data,
    size_t * out_len);

/** Highlight text; returns the annotation xref. */
PdfStatus pdf_compat_page_add_highlight_annot(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatQuad * quads,
    size_t quads_len,
    const PdfCompatPoint * start,
    const PdfCompatPoint * stop,
    const PdfCompatRect * clip,
    int32_t * out);

/** Add a rectangle annotation; returns its xref. */
PdfStatus pdf_compat_page_add_rect_annot(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    int32_t * out);

/** Mark a quadrilateral for redaction; returns its xref. */
PdfStatus pdf_compat_page_add_redact_annot(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatQuad * quad,
    const char * text,
    const char * fontname,
    double fontsize,
    int32_t align,
    const double * fill,
    size_t fill_len,
    const double * text_color,
    size_t text_color_len,
    int32_t cross_out,
    int32_t * out);

/** Add a text annotation; returns its xref. */
PdfStatus pdf_compat_page_add_text_annot(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * at,
    const char * text,
    const char * icon,
    int32_t * out);

/** Add a form widget; returns its xref. */
PdfStatus pdf_compat_page_add_widget(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatWidget * widget,
    int32_t * out);

/** Annotations on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_annots(
    PdfCompatDocument * doc,
    int32_t page,
    const int32_t * types,
    size_t types_len,
    PdfCompatAnnotEntryList ** out);

/** Burn pending redactions in. */
PdfStatus pdf_compat_page_apply_redactions(
    PdfCompatDocument * doc,
    int32_t page,
    int32_t images,
    int32_t graphics,
    int32_t text,
    int32_t * out);

/** Tidy the content stream. */
PdfStatus pdf_compat_page_clean_contents(PdfCompatDocument * doc, int32_t page, int32_t sanitize);

/** Delete an annotation. */
PdfStatus pdf_compat_page_delete_annot(
    PdfCompatDocument * doc,
    int32_t page,
    int32_t xref,
    int32_t type_code,
    const char * type_name,
    const PdfCompatRect * rect);

/** Stroke a Bezier curve. */
PdfStatus pdf_compat_page_draw_bezier(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * p1,
    const PdfCompatPoint * p2,
    const PdfCompatPoint * p3,
    const PdfCompatPoint * p4,
    const char * options,
    PdfCompatPoint * out);

/** Stroke a circle. */
PdfStatus pdf_compat_page_draw_circle(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * center,
    double radius,
    const char * options,
    PdfCompatPoint * out);

/** Stroke a line. */
PdfStatus pdf_compat_page_draw_line(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * p1,
    const PdfCompatPoint * p2,
    const char * options,
    PdfCompatPoint * out);

/** Stroke a rectangle. */
PdfStatus pdf_compat_page_draw_rect(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    const char * options,
    double radius,
    PdfCompatPoint * out);

/** Detect tables on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_find_tables(
    const PdfCompatDocument * doc,
    int32_t page,
    PdfCompatTableFinder ** out);

/** First annotation, if any. [CALLER FREES *out] */
PdfStatus pdf_compat_page_first_annot(
    PdfCompatDocument * doc,
    int32_t page,
    PdfCompatAnnotEntryList ** out);

/** Content-stream object numbers. [CALLER FREES *out_data] */
PdfStatus pdf_compat_page_get_contents(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t ** out_data,
    size_t * out_len);

/** Vector drawings on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_drawings(
    PdfCompatDocument * doc,
    int32_t page,
    int32_t extended,
    PdfCompatDrawingList ** out);

/** Fonts used on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_fonts(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t full,
    PdfCompatFontInfoList ** out);

/** Placed images with geometry. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_image_info(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t hashes,
    int32_t xrefs,
    PdfCompatPlacedImageList ** out);

/** Boxes where a named image appears. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_image_rects(
    const PdfCompatDocument * doc,
    int32_t page,
    const char * name,
    int32_t transform,
    PdfCompatRectList ** out);

/** Images on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_images(
    const PdfCompatDocument * doc,
    int32_t page,
    int32_t full,
    PdfCompatImageRefList ** out);

/** Links on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_links(
    PdfCompatDocument * doc,
    int32_t page,
    PdfCompatLinkList ** out);

/** Render the page to a pixel buffer. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_pixmap(
    const PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatMatrix * matrix,
    double dpi,
    int32_t colorspace,
    const PdfCompatRect * clip,
    int32_t alpha,
    int32_t annots,
    PdfCompatPixmap ** out);

/** Render the page to SVG text. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_svg_image(
    const PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatMatrix * matrix,
    int32_t text_as_path,
    char ** out);

/** Extract text in the named format. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_text(
    const PdfCompatDocument * doc,
    int32_t page,
    const char * format,
    const PdfCompatRect * clip,
    int32_t flags,
    int32_t sort,
    int32_t use_textpage,
    PdfCompatTextOutput ** out);

/** Text inside a rectangle. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_textbox(
    const PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    int32_t use_textpage,
    char ** out);

/** Typeset spans with glyph origins. [CALLER FREES *out] */
PdfStatus pdf_compat_page_get_texttrace(
    const PdfCompatDocument * doc,
    int32_t page,
    PdfCompatTextTraceList ** out);

/** Register a font for the page. */
PdfStatus pdf_compat_page_insert_font(
    PdfCompatDocument * doc,
    int32_t page,
    const char * fontname,
    const char * fontfile,
    const uint8_t * fontbuffer,
    size_t fontbuffer_len,
    int32_t set_simple,
    int32_t wmode,
    int32_t encoding,
    int32_t * out);

/** Place an image on the page. */
PdfStatus pdf_compat_page_insert_image(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    int32_t alpha,
    const char * filename,
    double height,
    int32_t keep_proportion,
    const char * mask,
    int32_t oc,
    int32_t overlay,
    const PdfCompatPixmap * pixmap,
    int32_t rotate,
    const uint8_t * stream,
    size_t stream_len,
    double width,
    int32_t xref,
    int32_t * out);

/** Add a link to the page. */
PdfStatus pdf_compat_page_insert_link(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatLink * link,
    int32_t mark);

/** Insert text at a point. */
PdfStatus pdf_compat_page_insert_text(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * at,
    const char * text,
    const char *const * lines,
    size_t lines_len,
    const char * options,
    int32_t * out);

/** Flow text into a rectangle. */
PdfStatus pdf_compat_page_insert_textbox(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    const char * text,
    const char *const * lines,
    size_t lines_len,
    const char * options,
    double * out);

/** Media box in points. */
PdfStatus pdf_compat_page_mediabox(
    const PdfCompatDocument * doc,
    int32_t page,
    PdfCompatRect * out);

/** Open a drawing canvas on the page. */
PdfStatus pdf_compat_page_new_shape(PdfCompatDocument * doc, int32_t page);

/** Page box in points. */
PdfStatus pdf_compat_page_rect(const PdfCompatDocument * doc, int32_t page, PdfCompatRect * out);

/** Page rotation in degrees. */
PdfStatus pdf_compat_page_rotation(const PdfCompatDocument * doc, int32_t page, int32_t * out);

/** Search for text; rectangles or quadrilaterals. [CALLER FREES *out] */
PdfStatus pdf_compat_page_search_for(
    const PdfCompatDocument * doc,
    int32_t page,
    const char * needle,
    const PdfCompatRect * clip,
    int32_t quads,
    int32_t flags,
    int32_t limit,
    int32_t use_textpage,
    PdfCompatSearchHits ** out);

/** Set the crop box. */
PdfStatus pdf_compat_page_set_cropbox(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect);

/** Set the rotation. */
PdfStatus pdf_compat_page_set_rotation(PdfCompatDocument * doc, int32_t page, int32_t rotation);

/** Stamp a source page onto this one. */
PdfStatus pdf_compat_page_show_pdf_page(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    const PdfCompatDocument * source,
    int32_t page_no,
    int32_t keep_proportion,
    int32_t overlay,
    int32_t oc,
    int32_t rotate,
    const PdfCompatRect * clip,
    int32_t * out);

/** Form widgets on the page. [CALLER FREES *out] */
PdfStatus pdf_compat_page_widgets(
    PdfCompatDocument * doc,
    int32_t page,
    const int32_t * types,
    size_t types_len,
    PdfCompatWidgetList ** out);

/** Page object number. */
PdfStatus pdf_compat_page_xref(const PdfCompatDocument * doc, int32_t page, int32_t * out);

/** Channel count, excluding alpha. */
PdfStatus pdf_compat_colorspace_n(int32_t colorspace, int32_t * out);

/** Build a font from a name, file, or buffer. [CALLER FREES *out] */
PdfStatus pdf_compat_font_new(
    const char * fontname,
    const char * fontfile,
    const uint8_t * fontbuffer,
    size_t fontbuffer_len,
    int32_t script,
    const char * language,
    int32_t ordering,
    int32_t is_bold,
    int32_t is_italic,
    int32_t is_serif,
    int32_t embed,
    PdfCompatFont ** out);

/** Typeset width of a string. */
PdfStatus pdf_compat_font_text_length(
    const PdfCompatFont * font,
    const char * text,
    double fontsize,
    const char * language,
    int32_t script,
    int32_t wmode,
    int32_t small_caps,
    int32_t fallback,
    double * out);

/** Allocate a pixel buffer. [CALLER FREES *out] */
PdfStatus pdf_compat_pixmap_new(
    int32_t colorspace,
    const PdfCompatIRect * bounds,
    int32_t alpha,
    PdfCompatPixmap ** out);

/** Whether an alpha channel is present. */
PdfStatus pdf_compat_pixmap_alpha(const PdfCompatPixmap * pixmap, int32_t * out);

/** Clear to a value, optionally clipped. */
PdfStatus pdf_compat_pixmap_clear_with(
    PdfCompatPixmap * pixmap,
    int32_t value,
    const PdfCompatIRect * bbox);

/** Colour-space code: 1 grey, 3 RGB, 4 CMYK. */
PdfStatus pdf_compat_pixmap_colorspace(const PdfCompatPixmap * pixmap, int32_t * out);

/** Height in pixels. */
PdfStatus pdf_compat_pixmap_h(const PdfCompatPixmap * pixmap, int32_t * out);

/** Height in pixels. */
PdfStatus pdf_compat_pixmap_height(const PdfCompatPixmap * pixmap, int32_t * out);

/** Pixel bounds. */
PdfStatus pdf_compat_pixmap_irect(const PdfCompatPixmap * pixmap, PdfCompatIRect * out);

/** Channel count, excluding alpha. */
PdfStatus pdf_compat_pixmap_n(const PdfCompatPixmap * pixmap, int32_t * out);

/** A copy of the raw samples. [CALLER FREES *out_data] */
PdfStatus pdf_compat_pixmap_samples(
    const PdfCompatPixmap * pixmap,
    uint8_t ** out_data,
    size_t * out_len);

/** Write the pixmap to a file. */
PdfStatus pdf_compat_pixmap_save(
    const PdfCompatPixmap * pixmap,
    const char * path,
    const char * output,
    int32_t jpg_quality);

/** Fill a box with a colour. */
PdfStatus pdf_compat_pixmap_set_rect(
    PdfCompatPixmap * pixmap,
    const PdfCompatIRect * bbox,
    const uint8_t * color,
    size_t color_len);

/** Row stride in bytes. */
PdfStatus pdf_compat_pixmap_stride(const PdfCompatPixmap * pixmap, int32_t * out);

/** Encode the pixmap. [CALLER FREES *out_data] */
PdfStatus pdf_compat_pixmap_tobytes(
    const PdfCompatPixmap * pixmap,
    const char * output,
    int32_t jpg_quality,
    uint8_t ** out_data,
    size_t * out_len);

/** Width in pixels. */
PdfStatus pdf_compat_pixmap_w(const PdfCompatPixmap * pixmap, int32_t * out);

/** Width in pixels. */
PdfStatus pdf_compat_pixmap_width(const PdfCompatPixmap * pixmap, int32_t * out);

/** Stage text for a page rectangle. [CALLER FREES *out] */
PdfStatus pdf_compat_text_writer_new(
    const PdfCompatRect * rect,
    double opacity,
    const double * color,
    size_t color_len,
    PdfCompatTextWriter ** out);

/** Append text; returns the used box and end point. */
PdfStatus pdf_compat_text_writer_append(
    PdfCompatTextWriter * writer,
    const PdfCompatPoint * pos,
    const char * text,
    const PdfCompatFont * font,
    double fontsize,
    const char * language,
    int32_t right_to_left,
    int32_t small_caps,
    PdfCompatRect * out,
    PdfCompatPoint * out2);

/** Flush staged text onto a page. */
PdfStatus pdf_compat_text_writer_write_text(
    const PdfCompatTextWriter * writer,
    PdfCompatDocument * doc,
    int32_t page,
    const double * color,
    size_t color_len,
    double opacity,
    int32_t overlay,
    const double * morph,
    size_t morph_len,
    const PdfCompatMatrix * matrix,
    int32_t render_mode,
    int32_t oc);

/** Build a blank widget. [CALLER FREES *out] */
PdfStatus pdf_compat_widget_new(PdfCompatWidget ** out);

/** Typeset width of a string in a base font. */
PdfStatus pdf_compat_get_text_length(
    const char * text,
    const char * fontname,
    double fontsize,
    int32_t encoding,
    double * out);

/** Named paper size in points. */
PdfStatus pdf_compat_paper_size(const char * name, double * out, double * out2);

/** Set stroke and fill colours. */
PdfStatus pdf_compat_annot_set_colors(
    PdfCompatDocument * doc,
    int32_t page,
    int32_t xref,
    const double * stroke,
    size_t stroke_len,
    const double * fill,
    size_t fill_len);

/** Push appearance changes. */
PdfStatus pdf_compat_annot_update(
    PdfCompatDocument * doc,
    int32_t page,
    int32_t xref,
    const char * blend_mode,
    double opacity,
    double fontsize,
    const char * fontname,
    const double * text_color,
    size_t text_color_len,
    const double * border_color,
    size_t border_color_len,
    const double * fill_color,
    size_t fill_color_len,
    int32_t cross_out,
    int32_t rotate);

/** Commit pending marks. */
PdfStatus pdf_compat_shape_commit(PdfCompatDocument * doc, int32_t page, int32_t overlay);

/** Queue a line. */
PdfStatus pdf_compat_shape_draw_line(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatPoint * p1,
    const PdfCompatPoint * p2,
    PdfCompatPoint * out);

/** Queue a rectangle. */
PdfStatus pdf_compat_shape_draw_rect(
    PdfCompatDocument * doc,
    int32_t page,
    const PdfCompatRect * rect,
    double radius,
    PdfCompatPoint * out);

/** Stroke and fill the queued path. */
PdfStatus pdf_compat_shape_finish(PdfCompatDocument * doc, int32_t page, const char * options);

/** Snapshot count. */
PdfStatus pdf_compat_toc_list_count(const PdfCompatTocList * list, int32_t * out);

/** Nesting level, from 1. */
PdfStatus pdf_compat_toc_list_level(const PdfCompatTocList * list, int32_t index, int32_t * out);

/** Entry title. [CALLER FREES *out] */
PdfStatus pdf_compat_toc_list_title(const PdfCompatTocList * list, int32_t index, char ** out);

/** 1-based destination page. */
PdfStatus pdf_compat_toc_list_page(const PdfCompatTocList * list, int32_t index, int32_t * out);

/** Whether a follow-on destination exists. */
PdfStatus pdf_compat_toc_list_has_dest(const PdfCompatTocList * list, int32_t index, int32_t * out);

/** Destination kind, or -1. */
PdfStatus pdf_compat_toc_list_dest_kind(
    const PdfCompatTocList * list,
    int32_t index,
    int32_t * out);

/** Destination page, or -1. */
PdfStatus pdf_compat_toc_list_dest_page(
    const PdfCompatTocList * list,
    int32_t index,
    int32_t * out);

/** Destination point, or the origin. */
PdfStatus pdf_compat_toc_list_dest_target(
    const PdfCompatTocList * list,
    int32_t index,
    PdfCompatPoint * out);

/** Destination zoom, or 0. */
PdfStatus pdf_compat_toc_list_dest_zoom(const PdfCompatTocList * list, int32_t index, double * out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_toc_list_free(PdfCompatTocList * list);

/** Width in pixels. */
PdfStatus pdf_compat_image_data_width(const PdfCompatImageData * handle, int32_t * out);

/** Height in pixels. */
PdfStatus pdf_compat_image_data_height(const PdfCompatImageData * handle, int32_t * out);

/** Bits per component. */
PdfStatus pdf_compat_image_data_bits(const PdfCompatImageData * handle, int32_t * out);

/** Colour-space code. */
PdfStatus pdf_compat_image_data_colorspace(const PdfCompatImageData * handle, int32_t * out);

/** Colour-space name. [CALLER FREES *out] */
PdfStatus pdf_compat_image_data_colorspace_name(const PdfCompatImageData * handle, char ** out);

/** File extension. [CALLER FREES *out] */
PdfStatus pdf_compat_image_data_ext(const PdfCompatImageData * handle, char ** out);

/** Payload size in bytes. */
PdfStatus pdf_compat_image_data_size(const PdfCompatImageData * handle, int32_t * out);

/** Whether masked. */
PdfStatus pdf_compat_image_data_masked(const PdfCompatImageData * handle, int32_t * out);

/** Horizontal resolution. */
PdfStatus pdf_compat_image_data_xres(const PdfCompatImageData * handle, int32_t * out);

/** Vertical resolution. */
PdfStatus pdf_compat_image_data_yres(const PdfCompatImageData * handle, int32_t * out);

/** Raw payload bytes. [CALLER FREES *out_data] */
PdfStatus pdf_compat_image_data_data(
    const PdfCompatImageData * handle,
    uint8_t ** out_data,
    size_t * out_len);

/** Free a snapshot list. Null is safe. */
void pdf_compat_image_data_free(PdfCompatImageData * list);

/** Font name. [CALLER FREES *out] */
PdfStatus pdf_compat_font_data_name(const PdfCompatFontData * handle, char ** out);

/** Font bytes. [CALLER FREES *out_data] */
PdfStatus pdf_compat_font_data_data(
    const PdfCompatFontData * handle,
    uint8_t ** out_data,
    size_t * out_len);

/** Free a snapshot list. Null is safe. */
void pdf_compat_font_data_free(PdfCompatFontData * list);

/** Snapshot count. */
PdfStatus pdf_compat_font_info_list_count(const PdfCompatFontInfoList * list, int32_t * out);

/** Font object number. */
PdfStatus pdf_compat_font_info_list_xref(
    const PdfCompatFontInfoList * list,
    int32_t index,
    int32_t * out);

/** Font name. [CALLER FREES *out] */
PdfStatus pdf_compat_font_info_list_name(
    const PdfCompatFontInfoList * list,
    int32_t index,
    char ** out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_font_info_list_free(PdfCompatFontInfoList * list);

/** Snapshot count. */
PdfStatus pdf_compat_image_ref_list_count(const PdfCompatImageRefList * list, int32_t * out);

/** Image object number. */
PdfStatus pdf_compat_image_ref_list_xref(
    const PdfCompatImageRefList * list,
    int32_t index,
    int32_t * out);

/** Mask object number. */
PdfStatus pdf_compat_image_ref_list_mask(
    const PdfCompatImageRefList * list,
    int32_t index,
    int32_t * out);

/** Width in pixels. */
PdfStatus pdf_compat_image_ref_list_width(
    const PdfCompatImageRefList * list,
    int32_t index,
    int32_t * out);

/** Height in pixels. */
PdfStatus pdf_compat_image_ref_list_height(
    const PdfCompatImageRefList * list,
    int32_t index,
    int32_t * out);

/** Bits per component. */
PdfStatus pdf_compat_image_ref_list_bits(
    const PdfCompatImageRefList * list,
    int32_t index,
    int32_t * out);

/** Colour-space name. [CALLER FREES *out] */
PdfStatus pdf_compat_image_ref_list_colorspace_name(
    const PdfCompatImageRefList * list,
    int32_t index,
    char ** out);

/** Alternate name. [CALLER FREES *out] */
PdfStatus pdf_compat_image_ref_list_alt(
    const PdfCompatImageRefList * list,
    int32_t index,
    char ** out);

/** Resource name. [CALLER FREES *out] */
PdfStatus pdf_compat_image_ref_list_name(
    const PdfCompatImageRefList * list,
    int32_t index,
    char ** out);

/** Filter name. [CALLER FREES *out] */
PdfStatus pdf_compat_image_ref_list_filter(
    const PdfCompatImageRefList * list,
    int32_t index,
    char ** out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_image_ref_list_free(PdfCompatImageRefList * list);

/** Snapshot count. */
PdfStatus pdf_compat_placed_image_list_count(const PdfCompatPlacedImageList * list, int32_t * out);

/** Placed box in points. */
PdfStatus pdf_compat_placed_image_list_bbox(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    PdfCompatRect * out);

/** Image-to-page matrix. */
PdfStatus pdf_compat_placed_image_list_transform(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    PdfCompatMatrix * out);

/** Width in pixels. */
PdfStatus pdf_compat_placed_image_list_width(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Height in pixels. */
PdfStatus pdf_compat_placed_image_list_height(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Bits per component. */
PdfStatus pdf_compat_placed_image_list_bits(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Payload size in bytes. */
PdfStatus pdf_compat_placed_image_list_size(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Sequence number. */
PdfStatus pdf_compat_placed_image_list_number(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Colour-space name. [CALLER FREES *out] */
PdfStatus pdf_compat_placed_image_list_colorspace_name(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    char ** out);

/** Whether masked. */
PdfStatus pdf_compat_placed_image_list_has_mask(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Horizontal resolution. */
PdfStatus pdf_compat_placed_image_list_xres(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Vertical resolution. */
PdfStatus pdf_compat_placed_image_list_yres(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Whether a digest is present. */
PdfStatus pdf_compat_placed_image_list_has_digest(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    int32_t * out);

/** Digest bytes, or empty. [CALLER FREES *out_data] */
PdfStatus pdf_compat_placed_image_list_digest(
    const PdfCompatPlacedImageList * list,
    int32_t index,
    uint8_t ** out_data,
    size_t * out_len);

/** Free a snapshot list. Null is safe. */
void pdf_compat_placed_image_list_free(PdfCompatPlacedImageList * list);

/** Snapshot count. */
PdfStatus pdf_compat_rect_list_count(const PdfCompatRectList * list, int32_t * out);

/** Rectangle at an index. */
PdfStatus pdf_compat_rect_list_at(
    const PdfCompatRectList * list,
    int32_t index,
    PdfCompatRect * out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_rect_list_free(PdfCompatRectList * list);

/** Snapshot count. */
PdfStatus pdf_compat_drawing_list_count(const PdfCompatDrawingList * list, int32_t * out);

/** Drawing box in points. */
PdfStatus pdf_compat_drawing_list_rect(
    const PdfCompatDrawingList * list,
    int32_t index,
    PdfCompatRect * out);

/** Sequence number. */
PdfStatus pdf_compat_drawing_list_seqno(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out);

/** Drawing operator name. [CALLER FREES *out] */
PdfStatus pdf_compat_drawing_list_drawing_type(
    const PdfCompatDrawingList * list,
    int32_t index,
    char ** out);

/** Stroke colour, or empty. [CALLER FREES *out_data] */
PdfStatus pdf_compat_drawing_list_color(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    double ** out_data,
    size_t * out_len);

/** Fill colour, or empty. [CALLER FREES *out_data] */
PdfStatus pdf_compat_drawing_list_fill(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    double ** out_data,
    size_t * out_len);

/** Stroke width, if any. */
PdfStatus pdf_compat_drawing_list_width(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    double * out);

/** Line-cap values, or empty. [CALLER FREES *out_data] */
PdfStatus pdf_compat_drawing_list_line_cap(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    int32_t ** out_data,
    size_t * out_len);

/** Line-join value, if any. */
PdfStatus pdf_compat_drawing_list_line_join(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    int32_t * out);

/** Dash pattern, if any. [CALLER FREES *out] */
PdfStatus pdf_compat_drawing_list_dashes(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    char ** out);

/** Close-path flag, if any. */
PdfStatus pdf_compat_drawing_list_close_path(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    int32_t * out);

/** Even-odd fill rule. */
PdfStatus pdf_compat_drawing_list_even_odd(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out);

/** Fill opacity, if any. */
PdfStatus pdf_compat_drawing_list_fill_opacity(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    double * out);

/** Stroke opacity, if any. */
PdfStatus pdf_compat_drawing_list_stroke_opacity(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    double * out);

/** Layer name. [CALLER FREES *out] */
PdfStatus pdf_compat_drawing_list_layer(
    const PdfCompatDrawingList * list,
    int32_t index,
    char ** out);

/** Nesting level, if any. */
PdfStatus pdf_compat_drawing_list_level(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out_has,
    int32_t * out);

/** Path-item count. */
PdfStatus pdf_compat_drawing_list_item_count(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t * out);

/** Path-item operator. [CALLER FREES *out] */
PdfStatus pdf_compat_drawing_list_item_op(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t sub,
    char ** out);

/** Path-item parameters as JSON. [CALLER FREES *out] */
PdfStatus pdf_compat_drawing_list_item_params(
    const PdfCompatDrawingList * list,
    int32_t index,
    int32_t sub,
    char ** out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_drawing_list_free(PdfCompatDrawingList * list);

/** Snapshot count. */
PdfStatus pdf_compat_link_list_count(const PdfCompatLinkList * list, int32_t * out);

/** Clone one entry into a fresh handle. [CALLER FREES *out] */
PdfStatus pdf_compat_link_list_at(
    const PdfCompatLinkList * list,
    int32_t index,
    PdfCompatLink ** out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_link_list_free(PdfCompatLinkList * list);

/** Snapshot count. */
PdfStatus pdf_compat_widget_list_count(const PdfCompatWidgetList * list, int32_t * out);

/** Clone one entry into a fresh handle. [CALLER FREES *out] */
PdfStatus pdf_compat_widget_list_at(
    const PdfCompatWidgetList * list,
    int32_t index,
    PdfCompatWidget ** out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_widget_list_free(PdfCompatWidgetList * list);

/** Snapshot count. */
PdfStatus pdf_compat_annot_entry_list_count(const PdfCompatAnnotEntryList * list, int32_t * out);

/** Annotation object number. */
PdfStatus pdf_compat_annot_entry_list_xref(
    const PdfCompatAnnotEntryList * list,
    int32_t index,
    int32_t * out);

/** Annotation type code. */
PdfStatus pdf_compat_annot_entry_list_type_code(
    const PdfCompatAnnotEntryList * list,
    int32_t index,
    int32_t * out);

/** Annotation type name. [CALLER FREES *out] */
PdfStatus pdf_compat_annot_entry_list_type_name(
    const PdfCompatAnnotEntryList * list,
    int32_t index,
    char ** out);

/** Annotation box in points. */
PdfStatus pdf_compat_annot_entry_list_rect(
    const PdfCompatAnnotEntryList * list,
    int32_t index,
    PdfCompatRect * out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_annot_entry_list_free(PdfCompatAnnotEntryList * list);

/** Snapshot count. */
PdfStatus pdf_compat_text_trace_list_count(const PdfCompatTextTraceList * list, int32_t * out);

/** Span box in points. */
PdfStatus pdf_compat_text_trace_list_bbox(
    const PdfCompatTextTraceList * list,
    int32_t index,
    PdfCompatRect * out);

/** Ascender height. */
PdfStatus pdf_compat_text_trace_list_ascender(
    const PdfCompatTextTraceList * list,
    int32_t index,
    double * out);

/** Writing direction. */
PdfStatus pdf_compat_text_trace_list_bidi_dir(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t * out);

/** Bidi level. */
PdfStatus pdf_compat_text_trace_list_bidi_level(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t * out);

/** Character count. */
PdfStatus pdf_compat_text_trace_list_char_count(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t * out);

/** Character codepoint. */
PdfStatus pdf_compat_text_trace_list_char_codepoint(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t sub,
    int32_t * out);

/** Glyph id. */
PdfStatus pdf_compat_text_trace_list_char_glyph(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t sub,
    int32_t * out);

/** Glyph origin. */
PdfStatus pdf_compat_text_trace_list_char_origin(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t sub,
    PdfCompatPoint * out);

/** Glyph box. */
PdfStatus pdf_compat_text_trace_list_char_bbox(
    const PdfCompatTextTraceList * list,
    int32_t index,
    int32_t sub,
    PdfCompatRect * out);

/** Free a snapshot list. Null is safe. */
void pdf_compat_text_trace_list_free(PdfCompatTextTraceList * list);

/** Free a pixel buffer. Null is safe. */
void pdf_compat_pixmap_free(PdfCompatPixmap * handle);

/** Free a font. Null is safe. */
void pdf_compat_font_free(PdfCompatFont * handle);

/** Free a staged-text writer. Null is safe. */
void pdf_compat_text_writer_free(PdfCompatTextWriter * handle);

/** Free a widget. Null is safe. */
void pdf_compat_widget_free(PdfCompatWidget * handle);

/** Free a link. Null is safe. */
void pdf_compat_link_free(PdfCompatLink * handle);

/** Free a table finder. Null is safe. */
void pdf_compat_table_finder_free(PdfCompatTableFinder * handle);

/** Free a text result. Null is safe. */
void pdf_compat_text_output_free(PdfCompatTextOutput * handle);

/** Free search hits. Null is safe. */
void pdf_compat_search_hits_free(PdfCompatSearchHits * handle);

/** Start an empty outline builder. [CALLER FREES *out] */
PdfStatus pdf_compat_toc_builder_new(PdfCompatTocBuilder ** out);

/** Push one outline entry: level, title, 1-based page, and destination. */
PdfStatus pdf_compat_toc_builder_push(
    PdfCompatTocBuilder * builder,
    int32_t level,
    const char * title,
    int32_t page,
    int32_t has_dest,
    int32_t dest_kind,
    int32_t dest_page,
    double dest_x,
    double dest_y,
    double dest_zoom);

/** Free an outline builder. Null is safe. */
void pdf_compat_toc_builder_free(PdfCompatTocBuilder * builder);

/** Widget field name. [CALLER FREES *out] */
PdfStatus pdf_compat_widget_field_name(const PdfCompatWidget * widget, char ** out);

/** Set the widget field name. */
PdfStatus pdf_compat_widget_set_field_name(PdfCompatWidget * widget, const char * value);

/** Widget field-type code. */
PdfStatus pdf_compat_widget_field_type(const PdfCompatWidget * widget, int32_t * out);

/** Set the widget field-type code. */
PdfStatus pdf_compat_widget_set_field_type(PdfCompatWidget * widget, int32_t value);

/** Widget field value text. [CALLER FREES *out] */
PdfStatus pdf_compat_widget_field_value(const PdfCompatWidget * widget, char ** out);

/** Set the widget field value text. */
PdfStatus pdf_compat_widget_set_field_value(PdfCompatWidget * widget, const char * value);

/** Widget box in points. */
PdfStatus pdf_compat_widget_rect(const PdfCompatWidget * widget, PdfCompatRect * out);

/** Set the widget box. */
PdfStatus pdf_compat_widget_set_rect(PdfCompatWidget * widget, const PdfCompatRect * value);

/** Build a blank link. [CALLER FREES *out] */
PdfStatus pdf_compat_link_new(PdfCompatLink ** out);

/** Link box in points. */
PdfStatus pdf_compat_link_rect(const PdfCompatLink * link, PdfCompatRect * out);

/** Set the link box. */
PdfStatus pdf_compat_link_set_rect(PdfCompatLink * link, const PdfCompatRect * value);

/** Link kind code. */
PdfStatus pdf_compat_link_kind(const PdfCompatLink * link, int32_t * out);

/** Set the link kind code. */
PdfStatus pdf_compat_link_set_kind(PdfCompatLink * link, int32_t value);

/** Link target URI. [CALLER FREES *out] */
PdfStatus pdf_compat_link_uri(const PdfCompatLink * link, char ** out);

/** Set the link target URI. */
PdfStatus pdf_compat_link_set_uri(PdfCompatLink * link, const char * value);

/** Link object number. */
PdfStatus pdf_compat_link_xref(const PdfCompatLink * link, int32_t * out);

/** Set the link object number. */
PdfStatus pdf_compat_link_set_xref(PdfCompatLink * link, int32_t value);

/** Link identifier. [CALLER FREES *out] */
PdfStatus pdf_compat_link_id(const PdfCompatLink * link, char ** out);

/** Set the link identifier. */
PdfStatus pdf_compat_link_set_id(PdfCompatLink * link, const char * value);

/** Link border width in points. */
PdfStatus pdf_compat_link_border_width(const PdfCompatLink * link, double * out);

/** Set the link border width. */
PdfStatus pdf_compat_link_set_border_width(PdfCompatLink * link, double value);

/** Link border style, if any: `has` says whether one exists. [CALLER FREES *out_style] */
PdfStatus pdf_compat_link_border_style(
    const PdfCompatLink * link,
    int32_t * out_has,
    char ** out_style);

/** Set the link border style; null clears it. */
PdfStatus pdf_compat_link_set_border_style(PdfCompatLink * link, const char * value);

/** Link border dashes: element count plus heap ints, freed with `pdf_bytes_free`. [CALLER FREES *out_data] */
PdfStatus pdf_compat_link_border_dashes(
    const PdfCompatLink * link,
    int32_t ** out_data,
    size_t * out_len);

/** Set the link border dashes. */
PdfStatus pdf_compat_link_set_border_dashes(
    PdfCompatLink * link,
    const int32_t * values,
    size_t len);

/** Link border cloud effect level. */
PdfStatus pdf_compat_link_border_clouds(const PdfCompatLink * link, int32_t * out);

/** Set the link border cloud effect level. */
PdfStatus pdf_compat_link_set_border_clouds(PdfCompatLink * link, int32_t value);

/** Link stroke colour: element count plus heap doubles, freed with `pdf_bytes_free`. [CALLER FREES *out_data] */
PdfStatus pdf_compat_link_stroke(const PdfCompatLink * link, double ** out_data, size_t * out_len);

/** Set the link stroke colour. */
PdfStatus pdf_compat_link_set_stroke(PdfCompatLink * link, const double * values, size_t len);

/** Link fill colour: element count plus heap doubles, freed with `pdf_bytes_free`. [CALLER FREES *out_data] */
PdfStatus pdf_compat_link_fill(const PdfCompatLink * link, double ** out_data, size_t * out_len);

/** Set the link fill colour. */
PdfStatus pdf_compat_link_set_fill(PdfCompatLink * link, const double * values, size_t len);

/** Link flags. */
PdfStatus pdf_compat_link_flags(const PdfCompatLink * link, int32_t * out);

/** Set the link flags. */
PdfStatus pdf_compat_link_set_flags(PdfCompatLink * link, int32_t value);

/** Whether the link carries a follow-on destination: 1 yes, 0 no. */
PdfStatus pdf_compat_link_has_dest(const PdfCompatLink * link, int32_t * out);

/** Follow-on destination page, or -1 when there is none. */
PdfStatus pdf_compat_link_dest_page(const PdfCompatLink * link, int32_t * out);

/** Follow-on destination point, or the origin when there is none. */
PdfStatus pdf_compat_link_dest_target(const PdfCompatLink * link, PdfCompatPoint * out);

/** Follow-on destination zoom, or 0 when there is none. */
PdfStatus pdf_compat_link_dest_zoom(const PdfCompatLink * link, double * out);

/** Clear the follow-on destination. */
PdfStatus pdf_compat_link_clear_dest(PdfCompatLink * link);

/** Whether the link chains to another: 1 yes, 0 no. */
PdfStatus pdf_compat_link_has_next(const PdfCompatLink * link, int32_t * out);

/** Clone the chained link into a fresh handle, freed with `pdf_compat_link_free`. [CALLER FREES *out] */
PdfStatus pdf_compat_link_next(const PdfCompatLink * link, PdfCompatLink ** out);

/** Chain another link; null clears the chain. */
PdfStatus pdf_compat_link_set_next(PdfCompatLink * link, const PdfCompatLink * next);

/** Result format tag: 0 text, 1 words, 2 blocks, 3 dict, 4 json, 5 raw json,. */
PdfStatus pdf_compat_text_output_format(const PdfCompatTextOutput * handle, int32_t * out);

/** Result text for the string formats (text, json, raw json, html, xhtml, xml). [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_text(const PdfCompatTextOutput * handle, char ** out);

/** Word count for the words format. */
PdfStatus pdf_compat_text_output_word_count(const PdfCompatTextOutput * handle, int32_t * out);

/** Word box for the words format. */
PdfStatus pdf_compat_text_output_word_rect(
    const PdfCompatTextOutput * handle,
    int32_t index,
    PdfCompatRect * out);

/** Word text for the words format. [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_word_text(
    const PdfCompatTextOutput * handle,
    int32_t index,
    char ** out);

/** Word block number for the words format. */
PdfStatus pdf_compat_text_output_word_block(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Word line number for the words format. */
PdfStatus pdf_compat_text_output_word_line(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Word number for the words format. */
PdfStatus pdf_compat_text_output_word_number(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Block count for the blocks format. */
PdfStatus pdf_compat_text_output_block_count(const PdfCompatTextOutput * handle, int32_t * out);

/** Block box for the blocks format. */
PdfStatus pdf_compat_text_output_block_rect(
    const PdfCompatTextOutput * handle,
    int32_t index,
    PdfCompatRect * out);

/** Block text for the blocks format. [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_block_text(
    const PdfCompatTextOutput * handle,
    int32_t index,
    char ** out);

/** Block number for the blocks format. */
PdfStatus pdf_compat_text_output_block_number(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Block type for the blocks format. */
PdfStatus pdf_compat_text_output_block_type(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Page width for the dict format. */
PdfStatus pdf_compat_text_output_dict_width(const PdfCompatTextOutput * handle, double * out);

/** Page height for the dict format. */
PdfStatus pdf_compat_text_output_dict_height(const PdfCompatTextOutput * handle, double * out);

/** Dict block count for the dict format. */
PdfStatus pdf_compat_text_output_dict_block_count(
    const PdfCompatTextOutput * handle,
    int32_t * out);

/** Dict block tag: 0 text, 1 image. */
PdfStatus pdf_compat_text_output_dict_block_tag(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Text dict block box. */
PdfStatus pdf_compat_text_output_dict_text_bbox(
    const PdfCompatTextOutput * handle,
    int32_t index,
    PdfCompatRect * out);

/** Text dict block number. */
PdfStatus pdf_compat_text_output_dict_text_number(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Text dict block flags. */
PdfStatus pdf_compat_text_output_dict_text_flags(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Text dict block line count. */
PdfStatus pdf_compat_text_output_dict_line_count(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Dict line box. */
PdfStatus pdf_compat_text_output_dict_line_bbox(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    PdfCompatRect * out);

/** Dict line writing mode. */
PdfStatus pdf_compat_text_output_dict_line_wmode(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t * out);

/** Dict line direction. */
PdfStatus pdf_compat_text_output_dict_line_direction(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    PdfCompatPoint * out);

/** Dict line span count. */
PdfStatus pdf_compat_text_output_dict_span_count(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t * out);

/** Dict span box. */
PdfStatus pdf_compat_text_output_dict_span_bbox(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    PdfCompatRect * out);

/** Dict span origin. */
PdfStatus pdf_compat_text_output_dict_span_origin(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    PdfCompatPoint * out);

/** Dict span font name. [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_dict_span_font(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    char ** out);

/** Dict span size in points. */
PdfStatus pdf_compat_text_output_dict_span_size(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    double * out);

/** Dict span colour. */
PdfStatus pdf_compat_text_output_dict_span_color(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict span alpha. */
PdfStatus pdf_compat_text_output_dict_span_alpha(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict span flags. */
PdfStatus pdf_compat_text_output_dict_span_flags(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict span bidi level. */
PdfStatus pdf_compat_text_output_dict_span_bidi(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict span character flags. */
PdfStatus pdf_compat_text_output_dict_span_char_flags(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict span ascender. */
PdfStatus pdf_compat_text_output_dict_span_ascender(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    double * out);

/** Dict span descender. */
PdfStatus pdf_compat_text_output_dict_span_descender(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    double * out);

/** Dict span text. [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_dict_span_text(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    char ** out);

/** Dict span character count (raw dicts only). */
PdfStatus pdf_compat_text_output_dict_span_char_count(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t * out);

/** Dict character box. */
PdfStatus pdf_compat_text_output_dict_char_bbox(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t ch,
    PdfCompatRect * out);

/** Dict character text (a single character). [CALLER FREES *out] */
PdfStatus pdf_compat_text_output_dict_char_text(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t ch,
    char ** out);

/** Dict character origin. */
PdfStatus pdf_compat_text_output_dict_char_origin(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t ch,
    PdfCompatPoint * out);

/** Whether the dict character is synthetic: 1 yes, 0 no. */
PdfStatus pdf_compat_text_output_dict_char_synthetic(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t line,
    int32_t span,
    int32_t ch,
    int32_t * out);

/** Image dict block box. */
PdfStatus pdf_compat_text_output_dict_image_bbox(
    const PdfCompatTextOutput * handle,
    int32_t index,
    PdfCompatRect * out);

/** Image dict block number. */
PdfStatus pdf_compat_text_output_dict_image_number(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Image dict block width in pixels. */
PdfStatus pdf_compat_text_output_dict_image_width(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Image dict block height in pixels. */
PdfStatus pdf_compat_text_output_dict_image_height(
    const PdfCompatTextOutput * handle,
    int32_t index,
    int32_t * out);

/** Whether hits are quadrilaterals (1) or rectangles (0). */
PdfStatus pdf_compat_search_hits_is_quads(const PdfCompatSearchHits * handle, int32_t * out);

/** Hit count. */
PdfStatus pdf_compat_search_hits_count(const PdfCompatSearchHits * handle, int32_t * out);

/** Hit rectangle; fails when the hits are quadrilaterals. */
PdfStatus pdf_compat_search_hits_rect_at(
    const PdfCompatSearchHits * handle,
    int32_t index,
    PdfCompatRect * out);

/** Hit quadrilateral; fails when the hits are rectangles. */
PdfStatus pdf_compat_search_hits_quad_at(
    const PdfCompatSearchHits * handle,
    int32_t index,
    PdfCompatQuad * out);

/** Table count. */
PdfStatus pdf_compat_table_finder_table_count(const PdfCompatTableFinder * handle, int32_t * out);

/** Table box in points. */
PdfStatus pdf_compat_table_finder_table_bbox(
    const PdfCompatTableFinder * handle,
    int32_t index,
    PdfCompatRect * out);

/** Table row count. */
PdfStatus pdf_compat_table_finder_table_row_count(
    const PdfCompatTableFinder * handle,
    int32_t index,
    int32_t * out);

/** Table column count. */
PdfStatus pdf_compat_table_finder_table_col_count(
    const PdfCompatTableFinder * handle,
    int32_t index,
    int32_t * out);

/** Table cell count. */
PdfStatus pdf_compat_table_finder_table_cell_count(
    const PdfCompatTableFinder * handle,
    int32_t index,
    int32_t * out);

/** Table cell box in points. */
PdfStatus pdf_compat_table_finder_table_cell_bbox(
    const PdfCompatTableFinder * handle,
    int32_t index,
    int32_t cell,
    PdfCompatRect * out);

/** Table cell text. [CALLER FREES *out] */
PdfStatus pdf_compat_table_finder_table_cell_text(
    const PdfCompatTableFinder * handle,
    int32_t index,
    int32_t cell,
    char ** out);

/* P1 port (#643): this export had no declaration in the ported lines;
 * declared here against the definition in src/compat.rs. */
/** Report the compat failure kind behind the last error: one of the
 * PDF_COMPAT_KIND_* codes above, or 0 when the last error carries none. */
int32_t pdf_compat_last_error_kind(void);

#ifdef __cplusplus
}
#endif

#endif /* PDFLUENT_H */
