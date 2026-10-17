/**
 * C API tests for AcroForm field writing + document serialisation:
 * pdf_form_set_field_value, pdf_form_field_value, pdf_document_tobytes.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests test_form_write
 *
 * Run (from project root):
 *   make -C crates/pdf-capi/tests run
 */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* pdf_form_set_field_value is declared behind this feature macro. */
#define PDFLUENT_TEXT_EDITING 1
#include "../pdf_capi.h"

/* Path to shared fixture (run from project root) */
static const char *ACROFORM_PDF = "fixtures/acroform.pdf";

static int tests_run = 0;
static int tests_passed = 0;
static int tests_skipped = 0;

#define RUN_TEST(fn)                                     \
    do {                                                 \
        tests_run++;                                     \
        printf("  [%2d] %-40s ", tests_run, #fn);        \
        int _r = fn();                                   \
        if (_r == 0) { tests_passed++; printf("PASS\n"); } \
        else if (_r == 77) { tests_skipped++; printf("SKIP\n"); } \
        else { printf("FAIL\n"); }                       \
    } while (0)

/* ---------- Read back the fixture's fields ---------- */

static int test_field_names_and_values(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(ACROFORM_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 77; /* skip if fixture missing */

    int32_t count = pdf_form_field_count(doc);
    assert(count == 2);

    /* Both fixture fields by name, in either order. */
    char *n0 = pdf_form_field_name(doc, 0);
    char *n1 = pdf_form_field_name(doc, 1);
    assert(n0 != NULL && n1 != NULL);
    assert((strcmp(n0, "FullName") == 0 && strcmp(n1, "Email") == 0) ||
           (strcmp(n0, "Email") == 0 && strcmp(n1, "FullName") == 0));
    pdf_string_free(n0);
    pdf_string_free(n1);

    /* Recorded values: FullName is filled, Email is empty. */
    char *full = pdf_form_field_value(doc, "FullName");
    assert(full != NULL);
    assert(strcmp(full, "John Doe") == 0);
    pdf_string_free(full);

    char *missing = pdf_form_field_value(doc, "NoSuchField");
    assert(missing == NULL);

    /* Safety: null inputs must not crash. */
    assert(pdf_form_field_value(NULL, "FullName") == NULL);
    assert(pdf_form_field_value(doc, NULL) == NULL);

    pdf_document_free(doc);
    return 0;
}

/* ---------- Set a value, read it back, source untouched ---------- */

static int test_set_field_value(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(ACROFORM_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 77;

    PdfDocument *updated = NULL;
    s = pdf_form_set_field_value(doc, "Email", "a@b.c", &updated);
    assert(s == PDF_STATUS_OK);
    assert(updated != NULL);

    /* The new document carries the value … */
    char *v = pdf_form_field_value(updated, "Email");
    assert(v != NULL);
    assert(strcmp(v, "a@b.c") == 0);
    pdf_string_free(v);

    /* … the source document does not. */
    char *old = pdf_form_field_value(doc, "Email");
    assert(old == NULL || old[0] == '\0');
    if (old != NULL) pdf_string_free(old);

    /* Unknown field: invalid argument, out untouched. */
    PdfDocument *nil = NULL;
    s = pdf_form_set_field_value(doc, "NoSuchField", "x", &nil);
    assert(s == PDF_STATUS_ERROR_INVALID_ARG);
    assert(nil == NULL);

    /* Safety: null inputs. */
    assert(pdf_form_set_field_value(NULL, "Email", "x", &nil)
           == PDF_STATUS_ERROR_INVALID_ARG);
    assert(pdf_form_set_field_value(doc, NULL, "x", &nil)
           == PDF_STATUS_ERROR_INVALID_ARG);
    assert(pdf_form_set_field_value(doc, "Email", NULL, &nil)
           == PDF_STATUS_ERROR_INVALID_ARG);
    assert(pdf_form_set_field_value(doc, "Email", "x", NULL)
           == PDF_STATUS_ERROR_INVALID_ARG);

    pdf_document_free(updated);
    pdf_document_free(doc);
    return 0;
}

/* ---------- Serialise, reopen, value persists ---------- */

static int test_tobytes_roundtrip(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(ACROFORM_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 77;

    PdfDocument *updated = NULL;
    s = pdf_form_set_field_value(doc, "FullName", "Jane Doe", &updated);
    assert(s == PDF_STATUS_OK);

    uint8_t *data = NULL;
    size_t len = 0;
    s = pdf_document_tobytes(updated, &data, &len);
    assert(s == PDF_STATUS_OK);
    assert(data != NULL);
    assert(len > 5);
    assert(memcmp(data, "%PDF-", 5) == 0);

    PdfDocument *reopened = NULL;
    s = pdf_document_open_from_bytes(data, len, &reopened);
    pdf_bytes_free(data, len);
    assert(s == PDF_STATUS_OK);
    assert(reopened != NULL);
    assert(pdf_form_field_count(reopened) == 2);

    char *v = pdf_form_field_value(reopened, "FullName");
    assert(v != NULL);
    assert(strcmp(v, "Jane Doe") == 0);
    pdf_string_free(v);

    /* Safety: null inputs. */
    assert(pdf_document_tobytes(NULL, &data, &len)
           == PDF_STATUS_ERROR_INVALID_ARG);
    assert(pdf_document_tobytes(doc, NULL, &len)
           == PDF_STATUS_ERROR_INVALID_ARG);
    assert(pdf_document_tobytes(doc, &data, NULL)
           == PDF_STATUS_ERROR_INVALID_ARG);

    pdf_document_free(reopened);
    pdf_document_free(updated);
    pdf_document_free(doc);
    return 0;
}

int main(void) {
    printf("test_form_write\n");
    printf("===============\n");

    PdfStatus init = pdf_init();
    assert(init == PDF_STATUS_OK);

    RUN_TEST(test_field_names_and_values);
    RUN_TEST(test_set_field_value);
    RUN_TEST(test_tobytes_roundtrip);

    pdf_destroy();

    printf("\n===============\n");
    printf("Results: %d/%d passed, %d skipped, %d failed\n",
           tests_passed, tests_run, tests_skipped,
           tests_run - tests_passed - tests_skipped);

    return (tests_run - tests_passed - tests_skipped > 0) ? 1 : 0;
}
