// C header for the pdf-capi native library.
// This is a manual mirror of the Rust C API for use in Swift via module maps.

#ifndef PDF_CAPI_H
#define PDF_CAPI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ---- Status codes ----

typedef enum {
    PDF_STATUS_OK = 0,
    PDF_STATUS_ERROR_INVALID_ARGUMENT = 1,
    PDF_STATUS_ERROR_FILE_NOT_FOUND = 2,
    PDF_STATUS_ERROR_INVALID_PASSWORD = 3,
    PDF_STATUS_ERROR_CORRUPT_PDF = 4,
    PDF_STATUS_ERROR_PAGE_RANGE = 5,
    PDF_STATUS_ERROR_RENDER = 6,
    PDF_STATUS_ERROR_CONVERT = 7,
    PDF_STATUS_ERROR_REDACT = 8,
    PDF_STATUS_ERROR_SIGN = 9,
    PDF_STATUS_ERROR_ANNOTATION = 10,
    PDF_STATUS_ERROR_MERGE = 11,
    PDF_STATUS_ERROR_EXTRACT = 12,
    PDF_STATUS_ERROR_SPLIT = 13,
    PDF_STATUS_ERROR_WATERMARK = 14,
    PDF_STATUS_ERROR_COMPRESS = 15,
    PDF_STATUS_ERROR_UNKNOWN = 99,
} PdfStatus;

// ---- PDF/A level ----

typedef enum {
    PDF_A_LEVEL_1B  = 0,
    PDF_A_LEVEL_1A  = 1,
    PDF_A_LEVEL_2B  = 2,
    PDF_A_LEVEL_2U  = 3,
    PDF_A_LEVEL_2A  = 4,
    PDF_A_LEVEL_3B  = 5,
    PDF_A_LEVEL_3U  = 6,
    PDF_A_LEVEL_3A  = 7,
    PDF_A_LEVEL_4   = 8,
    PDF_A_LEVEL_4F  = 9,
    PDF_A_LEVEL_4E  = 10,
} PdfALevel;

// ---- Opaque handles ----

typedef struct PdfDocument PdfDocument;
typedef struct PdfComplianceReport PdfComplianceReport;

// ---- Library lifecycle ----

PdfStatus pdf_init(void);
void pdf_destroy(void);
const char *pdf_version(void);

// ---- Document lifecycle ----

PdfStatus pdf_document_open_from_bytes(
    const uint8_t *data,
    size_t len,
    PdfDocument **out);

PdfStatus pdf_document_open(
    const char *path,
    const char *password,  // may be NULL
    PdfDocument **out);

void pdf_document_free(PdfDocument *doc);

// ---- Document queries ----

int32_t pdf_document_page_count(const PdfDocument *doc);
double pdf_page_width(const PdfDocument *doc, int32_t page_index);
double pdf_page_height(const PdfDocument *doc, int32_t page_index);
int32_t pdf_page_rotation(const PdfDocument *doc, int32_t page_index);

// ---- Rendering ----

PdfStatus pdf_page_render(
    const PdfDocument *doc,
    int32_t page_index,
    double dpi,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_pixels);

PdfStatus pdf_page_render_thumbnail(
    const PdfDocument *doc,
    int32_t page_index,
    uint32_t max_dimension,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_pixels);

void pdf_pixels_free(uint8_t *pixels, size_t len);

// ---- Text extraction ----

char *pdf_page_extract_text(const PdfDocument *doc, int32_t page_index);
void pdf_string_free(char *s);

// ---- Metadata ----

char *pdf_document_get_meta(const PdfDocument *doc, const char *key);
int32_t pdf_bookmark_count(const PdfDocument *doc);

// ---- Page geometry boxes ----

PdfStatus pdf_page_media_box(
    const PdfDocument *doc,
    int32_t page_index,
    double *out_x0, double *out_y0,
    double *out_x1, double *out_y1);

PdfStatus pdf_page_crop_box(
    const PdfDocument *doc,
    int32_t page_index,
    double *out_x0, double *out_y0,
    double *out_x1, double *out_y1);

// ---- PDF/A compliance ----

// Validate a document against a PDF/A conformance level.
// Writes an opaque PdfComplianceReport to *out on success.
// Returns PDF_STATUS_OK even when the document is non-compliant —
// check pdf_compliance_report_is_compliant() on the report.
PdfStatus pdf_document_validate_pdfa(
    const PdfDocument *doc,
    PdfALevel level,
    PdfComplianceReport **out);

int32_t pdf_compliance_report_is_compliant(const PdfComplianceReport *report);
int32_t pdf_compliance_report_error_count(const PdfComplianceReport *report);
void    pdf_compliance_report_free(PdfComplianceReport *report);

// ---- PDF/A conversion ----

// Convert a document to PDF/A. Returns a new document on success.
// The caller must free the output document with pdf_document_free.
PdfStatus pdf_document_convert_pdfa(
    const PdfDocument *doc,
    PdfALevel level,
    PdfDocument **out);

// ---- Redaction ----

// Redact all occurrences of pattern from the document.
// Returns a new document (caller must free with pdf_document_free).
// PDF_STATUS_OK is returned even if pattern has zero matches.
PdfStatus pdf_document_redact(
    const PdfDocument *doc,
    const char *pattern,
    PdfDocument **out);

// ---- Signing ----

// Sign a document using a PKCS#12 identity (.p12 / .pfx).
// pkcs12_password may be NULL for password-less bundles.
// Returns a new signed document (caller must free with pdf_document_free).
PdfStatus pdf_document_sign(
    const PdfDocument *doc,
    const char *pkcs12_path,
    const char *pkcs12_password,  // may be NULL
    PdfDocument **out);

// ---- Form fields ----

// Number of terminal AcroForm fields; 0 if no AcroForm; -1 on null doc.
int32_t pdf_form_field_count(const PdfDocument *doc);

// Fully qualified name of field at zero-based index.
// Returns NULL for out-of-range index or no AcroForm. Free with pdf_string_free.
char *pdf_form_field_name(const PdfDocument *doc, int32_t index);

// ---- Annotations ----

// Number of annotations on a page; 0 if none; -1 on error.
int32_t pdf_annotation_count(const PdfDocument *doc, int32_t page_index);

// Subtype string of annotation at zero-based annot_index on the page.
// Returns NULL for out-of-range or error. Free with pdf_string_free.
char *pdf_annotation_type(
    const PdfDocument *doc,
    int32_t page_index,
    int32_t annot_index);

// Add a yellow highlight annotation to a page and return a new document.
// x, y, w, h are in PDF user-space points (y increases upward).
// The caller must free the returned document with pdf_document_free.
PdfStatus pdf_annotation_add_highlight(
    const PdfDocument *doc,
    int32_t page_index,
    double x, double y,
    double w, double h,
    PdfDocument **out);

// ---- Document merge ----

// Merge count documents into one. docs is an array of count document pointers.
// The caller must free the returned document with pdf_document_free.
PdfStatus pdf_documents_merge(
    const PdfDocument *const *docs,
    int32_t count,
    PdfDocument **out);

// ---- Signature verification ----

// Number of signature fields in the document; -1 on null doc.
int32_t pdf_signature_count(const PdfDocument *doc);

// Validate the signature at zero-based index.
// Returns 1=valid, 0=invalid, -1=unknown/error.
int32_t pdf_signature_is_valid(const PdfDocument *doc, int32_t index);

// ---- Image extraction ----

// Number of images on a page; -1 on error.
int32_t pdf_page_image_count(const PdfDocument *doc, int32_t page_index);

// Extract raw image bytes at image_index on page_index.
// On success writes width/height and a heap buffer to *out_data / *out_len.
// Free with pdf_bytes_free(*out_data, *out_len).
PdfStatus pdf_page_extract_image(
    const PdfDocument *doc,
    int32_t page_index,
    int32_t image_index,
    uint32_t *out_width,
    uint32_t *out_height,
    uint8_t **out_data,
    size_t *out_len);

// Free a byte buffer returned by pdf_page_extract_image. Null is safe.
void pdf_bytes_free(uint8_t *data, size_t len);

// ---- Text search ----

// Count total occurrences of query across all pages. Returns -1 on error.
int32_t pdf_document_search_count(const PdfDocument *doc, const char *query);

// ---- Document split ----

// Extract pages from_page..=to_page (0-based, inclusive) into a new document.
// The caller must free the returned document with pdf_document_free.
PdfStatus pdf_document_split_range(
    const PdfDocument *doc,
    int32_t from_page,
    int32_t to_page,
    PdfDocument **out);

// ---- Watermark ----

// Apply a diagonal text watermark to all pages. Returns a new document.
// The caller must free the returned document with pdf_document_free.
PdfStatus pdf_document_add_watermark(
    const PdfDocument *doc,
    const char *text,
    PdfDocument **out);

// ---- Compression ----

// Compress stream objects and return a new (smaller) document.
// The caller must free the returned document with pdf_document_free.
PdfStatus pdf_document_compress(
    const PdfDocument *doc,
    PdfDocument **out);

// ---- Error state ----

const char *pdf_get_last_error(void);
void pdf_clear_error(void);

#ifdef __cplusplus
}
#endif

#endif // PDF_CAPI_H
