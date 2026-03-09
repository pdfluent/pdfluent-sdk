/**
 * Basic example: open a PDF, print page info, extract text, render a page.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   cc -o basic examples/basic.c -L../../target/release -lpdf_capi -I.
 *
 * Run:
 *   ./basic input.pdf
 */

#include <stdio.h>
#include <stdlib.h>
#include "pdf_capi.h"

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <pdf-file>\n", argv[0]);
        return 1;
    }

    /* Initialize the library */
    pdf_init();

    /* Open the PDF */
    PdfDocument *doc = NULL;
    PdfStatus status = pdf_document_open(argv[1], NULL, &doc);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "Error: %s\n", err ? err : "unknown");
        return 1;
    }

    /* Print page info */
    int pages = pdf_document_page_count(doc);
    printf("Pages: %d\n", pages);

    for (int i = 0; i < pages && i < 5; i++) {
        double w = pdf_page_width(doc, i);
        double h = pdf_page_height(doc, i);
        int rot = pdf_page_rotation(doc, i);
        printf("  Page %d: %.0f x %.0f pt, rotation %d\n", i, w, h, rot);
    }

    /* Extract text from first page */
    char *text = pdf_page_extract_text(doc, 0);
    if (text) {
        printf("\nText (page 0, first 200 chars):\n%.200s\n", text);
        pdf_string_free(text);
    }

    /* Render first page at 72 DPI */
    uint32_t rw, rh;
    uint8_t *pixels;
    status = pdf_page_render(doc, 0, 72.0, &rw, &rh, &pixels);
    if (status == PDF_STATUS_OK) {
        printf("\nRendered: %u x %u pixels (%zu bytes)\n",
               rw, rh, (size_t)rw * rh * 4);
        pdf_pixels_free(pixels, (size_t)rw * rh * 4);
    }

    /* Metadata */
    char *title = pdf_document_get_meta(doc, "Title");
    if (title) {
        printf("Title: %s\n", title);
        pdf_string_free(title);
    }

    /* Bookmarks */
    printf("Bookmarks: %d\n", pdf_bookmark_count(doc));

    /* Clean up */
    pdf_document_free(doc);
    pdf_destroy();

    printf("\nLibrary version: %s\n", pdf_version());
    return 0;
}
