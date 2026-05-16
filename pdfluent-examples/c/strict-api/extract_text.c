/*
 * extract_text.c — structured text extraction (G1).
 *
 * Without PDFLUENT_TEXT_EDITING: plain-text extraction via
 *   pdf_page_extract_text().
 *
 * With PDFLUENT_TEXT_EDITING: per-run metadata (font_name, is_bold,
 *   is_italic, color) via pdf_page_extract_text_spans() (G1 API).
 *
 * Build:
 *   make extract_text                            # plain text
 *   make extract_text CFLAGS=-DPDFLUENT_TEXT_EDITING  # with G1 metadata
 *
 * Run:
 *   ./extract_text <pdf-file> [page-index]
 */

#include <stdio.h>
#include <stdlib.h>

#include "pdfluent.h"

#ifndef PDFLUENT_TEXT_EDITING
static void extract_plain(PdfDocument *doc, int32_t page)
{
    char *text = pdf_page_extract_text(doc, page);
    if (!text) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "extract_text page %d: %s\n",
                page, err ? err : "(null)");
        return;
    }
    printf("--- page %d (plain text) ---\n%.2000s\n", page, text);
    pdf_string_free(text);
}
#endif /* !PDFLUENT_TEXT_EDITING */

#ifdef PDFLUENT_TEXT_EDITING
static void extract_spans(PdfDocument *doc, int32_t page)
{
    PdfTextSpanArray arr = {NULL, 0};
    PdfStatus status = pdf_page_extract_text_spans(doc, page, &arr);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "extract_text_spans page %d: %s\n",
                page, err ? err : "(unknown)");
        return;
    }

    printf("--- page %d (%zu spans) ---\n", page, arr.count);
    for (size_t i = 0; i < arr.count && i < 20; i++) {
        const PdfTextSpan *s = &arr.spans[i];
        printf("  [%zu] \"%s\"  font=%s bold=%d italic=%d color=#%06X "
               "x=%.1f y=%.1f\n",
               i, s->text, s->font_name, s->is_bold, s->is_italic,
               s->color & 0x00FFFFFFu,
               s->x, s->y);
    }
    if (arr.count > 20) {
        printf("  ... (%zu more spans)\n", arr.count - 20);
    }

    pdf_text_span_array_free(&arr);
}
#endif /* PDFLUENT_TEXT_EDITING */

int main(int argc, char *argv[])
{
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <pdf-file> [page-index]\n", argv[0]);
        return 1;
    }

    int32_t page = 0;
    if (argc >= 3) {
        page = (int32_t)atoi(argv[2]);
        if (page < 0) {
            fprintf(stderr, "page-index must be >= 0\n");
            return 1;
        }
    }

    if (pdf_init() != PDF_STATUS_OK) {
        fprintf(stderr, "pdf_init failed\n");
        return 1;
    }

    PdfDocument *doc = NULL;
    PdfStatus status = pdf_document_open(argv[1], NULL, &doc);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "pdf_document_open: %s\n", err ? err : "(unknown)");
        pdf_destroy();
        return 1;
    }

    int32_t pages = pdf_document_page_count(doc);
    if (page >= pages) {
        fprintf(stderr, "page %d out of range (document has %d pages)\n",
                page, pages);
        pdf_document_free(doc);
        pdf_destroy();
        return 1;
    }

#ifdef PDFLUENT_TEXT_EDITING
    extract_spans(doc, page);
#else
    extract_plain(doc, page);
#endif

    pdf_document_free(doc);
    pdf_destroy();
    return 0;
}
