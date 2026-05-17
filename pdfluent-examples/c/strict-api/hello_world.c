/*
 * hello_world.c — open a PDF, print page count, close.
 *
 * Demonstrates the minimal PDFluent C API lifecycle:
 *   1. pdf_init()
 *   2. pdf_document_open()
 *   3. pdf_document_page_count()
 *   4. pdf_document_free()
 *   5. pdf_destroy()
 *
 * Build (see Makefile):
 *   make hello_world
 *
 * Run:
 *   ./hello_world <path/to/file.pdf>
 */

#include <stdio.h>
#include <stdlib.h>

#include "pdfluent.h"

int main(int argc, char *argv[])
{
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <pdf-file>\n", argv[0]);
        return 1;
    }

    if (pdf_init() != PDF_STATUS_OK) {
        fprintf(stderr, "pdf_init failed\n");
        return 1;
    }

    PdfDocument *doc = NULL;
    PdfStatus status = pdf_document_open(argv[1], NULL, &doc);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "pdf_document_open: %s\n", err ? err : "(unknown error)");
        pdf_destroy();
        return 1;
    }

    int32_t pages = pdf_document_page_count(doc);
    printf("File   : %s\n", argv[1]);
    printf("Pages  : %d\n", pages);
    printf("Version: %s\n", pdf_version());

    pdf_document_free(doc);
    pdf_destroy();
    return 0;
}
