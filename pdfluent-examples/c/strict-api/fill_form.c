/*
 * fill_form.c — form field inspection and filling (G3 AcroForm, G4 XFA).
 *
 * Without PDFLUENT_TEXT_EDITING: read-only — lists all AcroForm field
 *   names via pdf_form_field_name().
 *
 * With PDFLUENT_TEXT_EDITING:
 *   - G3: fills an AcroForm field by name via pdf_form_set_field_value().
 *   - G4: sets an XFA field by SOM path via pdf_xfa_set_field_value().
 *   The result is written to <output.pdf>.
 *
 * Build:
 *   make fill_form                               # read-only list
 *   make fill_form CFLAGS=-DPDFLUENT_TEXT_EDITING # G3+G4 fill
 *
 * Run (read-only):
 *   ./fill_form <pdf-file>
 *
 * Run (G3 AcroForm fill):
 *   ./fill_form <pdf-file> acroform <field-name> <value> <output.pdf>
 *
 * Run (G4 XFA fill):
 *   ./fill_form <pdf-file> xfa <som.path> <value> <output.pdf>
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "pdfluent.h"

static void list_fields(PdfDocument *doc)
{
    int32_t count = pdf_form_field_count(doc);
    if (count < 0) {
        fprintf(stderr, "pdf_form_field_count failed\n");
        return;
    }
    if (count == 0) {
        printf("(no AcroForm fields)\n");
        return;
    }
    printf("%d AcroForm field(s):\n", count);
    for (int32_t i = 0; i < count; i++) {
        char *name = pdf_form_field_name(doc, i);
        printf("  [%d] %s\n", i, name ? name : "(unnamed)");
        pdf_string_free(name);
    }
}

#ifdef PDFLUENT_TEXT_EDITING
static int fill_acroform(PdfDocument *doc,
                         const char *field_name,
                         const char *value,
                         const char *output_path)
{
    PdfDocument *out = NULL;
    PdfStatus status = pdf_form_set_field_value(doc, field_name, value, &out);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "pdf_form_set_field_value: %s\n",
                err ? err : "(unknown)");
        return 1;
    }
    /* Note: pdf_document_save is not yet in the stable API.
     * Save placeholder: caller should use pdf_document_free after
     * copying internal bytes via a future pdf_document_get_bytes(). */
    printf("Field '%s' set to '%s' (output: %s — save API pending)\n",
           field_name, value, output_path);
    pdf_document_free(out);
    return 0;
}

static int fill_xfa(PdfDocument *doc,
                    const char *som_path,
                    const char *value,
                    const char *output_path)
{
    PdfDocument *out = NULL;
    PdfStatus status = pdf_xfa_set_field_value(doc, som_path, value, &out);
    if (status != PDF_STATUS_OK) {
        const char *err = pdf_get_last_error();
        fprintf(stderr, "pdf_xfa_set_field_value: %s\n",
                err ? err : "(unknown)");
        return 1;
    }
    printf("XFA path '%s' set to '%s' (output: %s — save API pending)\n",
           som_path, value, output_path);
    pdf_document_free(out);
    return 0;
}
#endif /* PDFLUENT_TEXT_EDITING */

int main(int argc, char *argv[])
{
    if (argc < 2) {
        fprintf(stderr,
                "Usage: %s <pdf-file> [acroform|xfa <name> <value> <out>]\n",
                argv[0]);
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

    int rc = 0;

    if (argc == 2) {
        list_fields(doc);
    }
#ifdef PDFLUENT_TEXT_EDITING
    else if (argc == 6 && strcmp(argv[2], "acroform") == 0) {
        rc = fill_acroform(doc, argv[3], argv[4], argv[5]);
    } else if (argc == 6 && strcmp(argv[2], "xfa") == 0) {
        rc = fill_xfa(doc, argv[3], argv[4], argv[5]);
    }
#endif
    else {
        fprintf(stderr, "Unexpected arguments\n");
        rc = 1;
    }

    pdf_document_free(doc);
    pdf_destroy();
    return rc;
}
