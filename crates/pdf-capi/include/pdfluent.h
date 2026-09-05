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
 * |  16   | PDF_STATUS_ERROR_LICENSE_INVALID            | ErrorInvalidLicense          |
 * |  17   | PDF_STATUS_ERROR_LICENSE_ALREADY_SET        | ErrorLicenseAlreadySet       |
 * |  18   | PDF_STATUS_ERROR_LICENSE_FILE               | ErrorLicenseFile             |
 * |  19   | PDF_STATUS_ERROR_LICENSE_EXPIRED            | ErrorLicenseExpired          |
 * |  20   | PDF_STATUS_ERROR_LICENSE_INVALID_SIGNATURE  | ErrorLicenseInvalidSignature |
 * |  22   | PDF_STATUS_ERROR_CAPABILITY_NOT_LICENSED    | ErrorCapabilityNotLicensed   |
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

    /**
     * The license key string is malformed or names an unrecognised tier.
     *
     * Recovery: verify the key string matches the documented format
     * (@c "tier:<name>").  See @c docs/licensing.md for the accepted values.
     */
    PDF_STATUS_ERROR_LICENSE_INVALID      = 16,

    /**
     * The process-global license has already been set to a different tier
     * in this run.  Restart the process to switch tiers.
     *
     * Recovery: restart the process, then activate with the desired tier
     * before calling any other API.
     */
    PDF_STATUS_ERROR_LICENSE_ALREADY_SET  = 17,

    /**
     * The license file could not be opened or read from disk (permission
     * denied, path not found, or I/O error).
     *
     * Recovery: verify the path exists and is readable.
     */
    PDF_STATUS_ERROR_LICENSE_FILE         = 18,

    /**
     * A signed license payload has expired — its @c expires_at field is
     * a unix timestamp in the past.
     *
     * Recovery: renew the license, then call
     * @ref pdfluent_license_activate_payload again with the refreshed JSON.
     */
    PDF_STATUS_ERROR_LICENSE_EXPIRED            = 19,

    /**
     * A signed license payload's Ed25519 signature does not verify against
     * the public key set via @ref pdfluent_license_set_public_key.  Either
     * the payload was tampered, or it was signed with a different private
     * key than this build expects.
     *
     * Recovery: re-download the license file and try again; if the issue
     * persists, contact PDFluent support.
     */
    PDF_STATUS_ERROR_LICENSE_INVALID_SIGNATURE  = 20,

    /**
     * The licence is valid but its tier does not include the requested
     * capability — Office export below Business, for instance.
     *
     * Deliberately distinct from @c PDF_STATUS_ERROR_INVALID_LICENSE: "your
     * key is bad" and "your plan does not cover this" send a caller to
     * different places.
     */
    PDF_STATUS_ERROR_CAPABILITY_NOT_LICENSED = 22,
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
 * @brief PDF/A conformance levels for validation and conversion.
 *
 * Values are stable across library versions (see @c docs/c_abi_stability.md).
 */
typedef enum {
    /** PDF/A-1B — ISO 19005-1 basic (most common archival target). */
    PDF_A_LEVEL_1B = 0,
    /** PDF/A-1A — ISO 19005-1 accessible (requires tagged PDF). */
    PDF_A_LEVEL_1A = 1,
    /** PDF/A-2B — ISO 19005-2 basic. */
    PDF_A_LEVEL_2B = 2,
    /** PDF/A-2U — ISO 19005-2 with Unicode mapping. */
    PDF_A_LEVEL_2U = 3,
    /** PDF/A-2A — ISO 19005-2 accessible. */
    PDF_A_LEVEL_2A = 4,
    /** PDF/A-3B — ISO 19005-3 basic (allows embedded files). */
    PDF_A_LEVEL_3B = 5,
    /** PDF/A-3U — ISO 19005-3 with Unicode mapping. */
    PDF_A_LEVEL_3U = 6,
    /** PDF/A-3A — ISO 19005-3 accessible. */
    PDF_A_LEVEL_3A = 7,
    /** PDF/A-4  — ISO 19005-4 base level. */
    PDF_A_LEVEL_4  = 8,
    /** PDF/A-4F — allows file attachments. */
    PDF_A_LEVEL_4F = 9,
    /** PDF/A-4E — allows engineering content (3D, rich media). */
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
 * @par Ownership: LIBRARY OWNS
 * The returned pointer is a static string literal; the caller must not free
 * it.  Valid for the lifetime of the process.
 *
 * @return Null-terminated version string (e.g. @c "1.0.0-beta.1").
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
 * @brief Free a string returned by a text or metadata API function.
 *
 * @par Ownership: CALLER FREES s
 * After this call @c s is invalid.  Passing NULL is a safe no-op.
 *
 * @param s  String returned by @ref pdf_page_extract_text,
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
 * @return PDF_STATUS_OK, PDF_STATUS_ERROR_INVALID_ARG,
 *         PDF_STATUS_ERROR_CAPABILITY_NOT_LICENSED, or
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
 * License activation
 * =========================================================================
 * Process-global, set-once.  Re-activating with the same tier is idempotent.
 * Re-activating with a different tier returns
 * @ref PDF_STATUS_ERROR_LICENSE_ALREADY_SET — restart the process to switch.
 *
 * Key format (1.0): @c "tier:<name>" where @c <name> is one of
 * @c trial, @c developer, @c team, @c business, @c enterprise.
 * Cryptographically-signed payloads (Ed25519) are accepted by the same
 * functions from release 1.1 onward.
 *
 * Error messages (available via @ref pdf_get_last_error) report parse
 * failure modes only; the raw key string is never logged.
 */

/**
 * @brief Current license status snapshot.
 *
 * Populated by @ref pdfluent_license_status.  All three fields are plain
 * integers so the struct has a stable, padding-free layout on all supported
 * platforms.
 *
 * @par Tier values
 * | Value | Tier       |
 * |-------|-----------|
 * |   0   | Trial      |
 * |   1   | Developer  |
 * |   2   | Team       |
 * |   3   | Business   |
 * |   4   | Enterprise |
 * |  -1   | Unknown (future variant not yet mapped by this binding) |
 *
 * @par Source values
 * | Value | Source   | Meaning                                            |
 * |-------|----------|----------------------------------------------------|
 * |   0   | Default  | No key supplied; active tier is Trial              |
 * |   1   | EnvVar   | Key resolved from @c PDFLUENT_LICENSE_KEY env var  |
 * |   2   | Explicit | Key set via @ref pdfluent_license_activate_key or  |
 * |       |          | @ref pdfluent_license_activate_file                |
 */
typedef struct {
    /** Effective tier (see table above). */
    int tier;
    /** Activation source (see table above). */
    int source;
    /**
     * 1 if the current tier marks PDF output via the @c /Producer metadata
     * field, 0 otherwise.  Only @c Trial sets this flag.
     */
    int output_is_marked;
} PdfluentLicenseStatus;

/**
 * @brief Activate the process-global license from a key string.
 *
 * The key is consumed immediately and never stored locally.  On success the
 * process-global tier is updated to the tier encoded in @c key.
 *
 * @par Ownership
 * @c key is BORROWED — the library does not retain it after the call returns.
 *
 * @param key  Null-terminated UTF-8 license key string.  Must not be NULL.
 *             BORROWED.
 *
 * @return @ref PDF_STATUS_OK on success.
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG if @c key is NULL or not valid
 *         UTF-8.
 * @return @ref PDF_STATUS_ERROR_LICENSE_INVALID if the key is malformed or
 *         names an unknown tier.
 * @return @ref PDF_STATUS_ERROR_LICENSE_ALREADY_SET if the process has already
 *         been activated to a different tier this run.
 *
 * @note Use @ref pdf_get_last_error for a human-readable failure message.
 */
PdfStatus pdfluent_license_activate_key(const char *key);

/**
 * @brief Activate the process-global license by reading a key from a file.
 *
 * The file must contain a single UTF-8 license key string (leading/trailing
 * whitespace is stripped).  Internally calls @ref pdfluent_license_activate_key.
 *
 * @par Ownership
 * @c path is BORROWED — the library does not retain it after the call returns.
 *
 * @param path  Null-terminated UTF-8 file system path to the license key file.
 *              Must not be NULL.  BORROWED.
 *
 * @return @ref PDF_STATUS_OK on success.
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG if @c path is NULL or not valid
 *         UTF-8.
 * @return @ref PDF_STATUS_ERROR_LICENSE_FILE if the file cannot be opened or
 *         read.
 * @return @ref PDF_STATUS_ERROR_LICENSE_INVALID if the file contents are not a
 *         valid key.
 * @return @ref PDF_STATUS_ERROR_LICENSE_ALREADY_SET if the process has already
 *         been activated to a different tier this run.
 *
 * @note Use @ref pdf_get_last_error for a human-readable failure message.
 */
PdfStatus pdfluent_license_activate_file(const char *path);

/**
 * @brief Return the effective tier as a plain integer.
 *
 * Convenience shorthand for callers that only need the tier number.  Equivalent
 * to calling @ref pdfluent_license_status and reading the @c tier field.
 *
 * @par Ownership
 * No pointers involved.
 *
 * @return Tier value in [0, 4] (see @ref PdfluentLicenseStatus).
 *         Returns -1 for a future tier variant not yet mapped by this binding.
 */
int pdfluent_license_effective_tier(void);

/**
 * @brief Fill @c out with a snapshot of the current license status.
 *
 * The snapshot is consistent within the call but may be superseded by a
 * concurrent activation on another thread.
 *
 * @par Ownership
 * @c out is a BORROWED write-target.  The caller allocates the struct (on the
 * stack or heap) and the library fills it.  The struct does not need to be
 * freed with any free function.
 *
 * @param out  Pointer to a caller-allocated @ref PdfluentLicenseStatus to
 *             receive the current status.  Must not be NULL.  BORROWED.
 *
 * @return @ref PDF_STATUS_OK on success.
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG if @c out is NULL.
 */
PdfStatus pdfluent_license_status(PdfluentLicenseStatus *out);

/**
 * @brief Inject the public Ed25519 verification key.
 *
 * Must be called once at process startup before any
 * @ref pdfluent_license_activate_payload call.  Calling twice with the
 * SAME key is idempotent; calling with a DIFFERENT key returns
 * @ref PDF_STATUS_ERROR_LICENSE_INVALID.
 *
 * @param public_key  Pointer to a 32-byte buffer holding the raw Ed25519
 *                    verifying key.  BORROWED.
 * @param key_len     Length of @c public_key in bytes.  Must equal 32.
 *
 * @return @ref PDF_STATUS_OK on success.
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG if @c public_key is NULL or
 *         @c key_len is not 32.
 * @return @ref PDF_STATUS_ERROR_LICENSE_INVALID if a different key was
 *         already injected this process.
 */
PdfStatus pdfluent_license_set_public_key(const unsigned char *public_key, size_t key_len);

/**
 * @brief Activate the process-global license from a signed JSON payload.
 *
 * The payload must be a null-terminated UTF-8 string containing the full
 * signed license JSON: @c {"payload": {...}, "signature": "..."}, where
 * @c signature is the base64-encoded Ed25519 signature over the
 * canonical payload JSON.
 *
 * @ref pdfluent_license_set_public_key must have been called first.
 *
 * @param payload_json  Null-terminated UTF-8 string containing the
 *                      signed payload.  BORROWED.
 *
 * @return @ref PDF_STATUS_OK on success.
 * @return @ref PDF_STATUS_ERROR_INVALID_ARG if @c payload_json is NULL.
 * @return @ref PDF_STATUS_ERROR_LICENSE_INVALID_SIGNATURE if the
 *         Ed25519 signature does not verify (tampered or wrong-key
 *         payload).
 * @return @ref PDF_STATUS_ERROR_LICENSE_EXPIRED if @c expires_at is in
 *         the past.
 * @return @ref PDF_STATUS_ERROR_LICENSE_ALREADY_SET if the process is
 *         already activated to a different tier.
 * @return @ref PDF_STATUS_ERROR_LICENSE_INVALID for malformed JSON,
 *         unknown tier names, or missing public key.
 *
 * On any error the process-global tier is NOT modified.
 */
PdfStatus pdfluent_license_activate_payload(const char *payload_json);

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

#ifdef __cplusplus
}
#endif

#endif /* PDFLUENT_H */
