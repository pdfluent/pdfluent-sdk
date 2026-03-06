/*
 * pdf_engine.h — C API for the PDF rendering engine.
 *
 * Stable C ABI for embedding the PDF engine in non-Rust applications.
 * Mirrors PDFium-style patterns: opaque handles, status codes, free functions.
 *
 * Thread safety: each PdfDocument handle must be used from one thread at a
 * time. Error state (pdf_get_last_error) is per-thread.
 */

#ifndef PDF_ENGINE_H
#define PDF_ENGINE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Status codes -------------------------------------------------------- */

typedef enum {
    PDF_STATUS_OK                   = 0,
    PDF_STATUS_ERROR_INVALID_ARG    = 1,
    PDF_STATUS_ERROR_FILE_NOT_FOUND = 2,
    PDF_STATUS_ERROR_INVALID_PASS   = 3,
    PDF_STATUS_ERROR_CORRUPT_PDF    = 4,
    PDF_STATUS_ERROR_PAGE_RANGE     = 5,
    PDF_STATUS_ERROR_RENDER         = 6,
    PDF_STATUS_ERROR_UNKNOWN        = 99
} PdfStatus;

/* ---- Opaque handles ------------------------------------------------------ */

typedef struct PdfDocument PdfDocument;

/* ---- Library lifecycle --------------------------------------------------- */

PdfStatus       pdf_init(void);
void            pdf_destroy(void);
const char*     pdf_version(void);

/* ---- Error handling (per-thread) ----------------------------------------- */

const char*     pdf_get_last_error(void);
void            pdf_clear_error(void);

/* ---- Document lifecycle -------------------------------------------------- */

PdfStatus       pdf_document_open(
                    const char*     path,
                    const char*     password,   /* NULL for no password */
                    PdfDocument**   out);

PdfStatus       pdf_document_open_from_bytes(
                    const uint8_t*  data,
                    size_t          len,
                    PdfDocument**   out);

void            pdf_document_free(PdfDocument* doc);

/* ---- Document queries ---------------------------------------------------- */

int32_t         pdf_document_page_count(const PdfDocument* doc);

/* ---- Page geometry ------------------------------------------------------- */

double          pdf_page_width(const PdfDocument* doc, int32_t page_index);
double          pdf_page_height(const PdfDocument* doc, int32_t page_index);
int32_t         pdf_page_rotation(const PdfDocument* doc, int32_t page_index);

PdfStatus       pdf_page_media_box(
                    const PdfDocument* doc, int32_t page_index,
                    double* x0, double* y0, double* x1, double* y1);

PdfStatus       pdf_page_crop_box(
                    const PdfDocument* doc, int32_t page_index,
                    double* x0, double* y0, double* x1, double* y1);

/* ---- Rendering ----------------------------------------------------------- */

PdfStatus       pdf_page_render(
                    const PdfDocument*  doc,
                    int32_t             page_index,
                    double              dpi,
                    uint32_t*           out_width,
                    uint32_t*           out_height,
                    uint8_t**           out_pixels);

PdfStatus       pdf_page_render_thumbnail(
                    const PdfDocument*  doc,
                    int32_t             page_index,
                    uint32_t            max_dimension,
                    uint32_t*           out_width,
                    uint32_t*           out_height,
                    uint8_t**           out_pixels);

void            pdf_pixels_free(uint8_t* pixels, size_t len);

/* ---- Text extraction ----------------------------------------------------- */

char*           pdf_page_extract_text(
                    const PdfDocument* doc, int32_t page_index);

void            pdf_string_free(char* s);

/* ---- Metadata ------------------------------------------------------------ */

char*           pdf_document_get_meta(
                    const PdfDocument* doc, const char* key);

/* ---- Bookmarks ----------------------------------------------------------- */

int32_t         pdf_bookmark_count(const PdfDocument* doc);

#ifdef __cplusplus
}
#endif

#endif /* PDF_ENGINE_H */
