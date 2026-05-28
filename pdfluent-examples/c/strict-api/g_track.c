/*
 * g_track.c — all four G-track text-editing operations (G1–G4).
 *
 * This file exercises every G-track API in a single program.  All four
 * operations require PDFLUENT_TEXT_EDITING to compile and produce useful
 * output.  Without the flag the program still compiles and runs cleanly
 * but reports that the G-track API is not enabled.
 *
 * G1  pdf_page_extract_text_spans()  — structured text with font metadata
 * G2  pdf_document_search_count()    — text search (count occurrences)
 * G3  pdf_form_set_field_value()     — AcroForm field fill
 * G4  pdf_xfa_set_field_value()      — XFA field fill via SOM path
 *
 * Build:
 *   make g_track                                 # stubs only
 *   make g_track CFLAGS=-DPDFLUENT_TEXT_EDITING  # full G1-G4
 *
 * Run:
 *   ./g_track <pdf-file>
 */

#include <stdio.h>
#include <stdlib.h>

#include "pdfluent.h"

#ifdef PDFLUENT_TEXT_EDITING

static void run_g1(PdfDocument *doc)
{
    printf("\n[G1] Structured text extraction (page 0)\n");
    PdfTextSpanArray arr = {NULL, 0};
    PdfStatus s = pdf_page_extract_text_spans(doc, 0, &arr);
    if (s != PDF_STATUS_OK) {
        printf("  skipped: %s\n", pdf_get_last_error());
        return;
    }
    printf("  %zu span(s) found\n", arr.count);
    for (size_t i = 0; i < arr.count && i < 3; i++) {
        const PdfTextSpan *sp = &arr.spans[i];
        printf("  [%zu] font=%-20s bold=%d italic=%d color=#%06X  \"%s\"\n",
               i, sp->font_name, sp->is_bold, sp->is_italic,
               sp->color & 0x00FFFFFFu, sp->text);
    }
    pdf_text_span_array_free(&arr);
}

static void run_g2(PdfDocument *doc, const char *query)
{
    printf("\n[G2] Text search: \"%s\"\n", query);
    int32_t count = pdf_document_search_count(doc, query);
    if (count < 0) {
        printf("  error: %s\n", pdf_get_last_error());
    } else {
        printf("  %d occurrence(s) found\n", count);
    }
}

static void run_g3(PdfDocument *doc)
{
    printf("\n[G3] AcroForm field fill\n");
    int32_t n = pdf_form_field_count(doc);
    if (n <= 0) {
        printf("  skipped: no AcroForm fields (count=%d)\n", n);
        return;
    }
    char *first_name = pdf_form_field_name(doc, 0);
    printf("  filling first field: %s\n", first_name ? first_name : "(null)");

    if (first_name) {
        PdfDocument *out = NULL;
        PdfStatus s = pdf_form_set_field_value(doc, first_name, "G3-test", &out);
        if (s == PDF_STATUS_OK) {
            printf("  set OK (pages=%d)\n", pdf_document_page_count(out));
            pdf_document_free(out);
        } else {
            printf("  set failed: %s\n", pdf_get_last_error());
        }
        pdf_string_free(first_name);
    }
}

static void run_g4(PdfDocument *doc)
{
    printf("\n[G4] XFA field fill (SOM path)\n");
    PdfDocument *out = NULL;
    /* Attempt a synthetic SOM path; the document may not be XFA. */
    PdfStatus s = pdf_xfa_set_field_value(
        doc, "form1[0].subform1[0].field1[0]", "G4-test", &out);
    if (s == PDF_STATUS_OK) {
        printf("  set OK (pages=%d)\n", pdf_document_page_count(out));
        pdf_document_free(out);
    } else {
        printf("  skipped (not an XFA form or path not found): %s\n",
               pdf_get_last_error());
    }
}

#endif /* PDFLUENT_TEXT_EDITING */

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
        fprintf(stderr, "pdf_document_open: %s\n", err ? err : "(unknown)");
        pdf_destroy();
        return 1;
    }

    printf("File : %s  (%d pages)\n",
           argv[1], pdf_document_page_count(doc));

#ifdef PDFLUENT_TEXT_EDITING
    run_g1(doc);
    run_g2(doc, "the");
    run_g3(doc);
    run_g4(doc);
#else
    printf("\nG-track API not enabled.\n"
           "Recompile with -DPDFLUENT_TEXT_EDITING to activate G1-G4.\n");
#endif

    pdf_document_free(doc);
    pdf_destroy();
    return 0;
}
