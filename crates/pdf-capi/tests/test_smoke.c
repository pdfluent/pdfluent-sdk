/**
 * C API smoke tests — 12 core scenarios.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests
 *
 * Run:
 *   make -C crates/pdf-capi/tests run
 */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../pdf_capi.h"

/* Path to shared fixture (run from project root) */
static const char *SAMPLE_PDF = "fixtures/sample.pdf";
static const char *MULTI_PDF  = "fixtures/multi-page.pdf";

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

/* ---------- Scenario 1: Open PDF, count pages ---------- */

static int test_open_and_page_count(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;
    assert(doc != NULL);

    int pages = pdf_document_page_count(doc);
    assert(pages >= 1);

    /* Page dimensions should be positive */
    double w = pdf_page_width(doc, 0);
    double h = pdf_page_height(doc, 0);
    assert(w > 0.0);
    assert(h > 0.0);

    pdf_document_free(doc);
    return 0;
}

/* ---------- Scenario 2: Render page 1 ---------- */

static int test_render_page(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    uint32_t w = 0, h = 0;
    uint8_t *pixels = NULL;
    s = pdf_page_render(doc, 0, 72.0, &w, &h, &pixels);
    if (s != PDF_STATUS_OK) {
        pdf_document_free(doc);
        return 1;
    }

    assert(w > 0);
    assert(h > 0);
    assert(pixels != NULL);

    /* Check that we got RGBA data (at least some non-zero bytes) */
    size_t len = (size_t)w * h * 4;
    int has_data = 0;
    for (size_t i = 0; i < len && i < 1000; i++) {
        if (pixels[i] != 0) { has_data = 1; break; }
    }
    assert(has_data);

    pdf_pixels_free(pixels, len);
    pdf_document_free(doc);
    return 0;
}

/* ---------- Scenario 3: Extract text ---------- */

static int test_extract_text(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    char *text = pdf_page_extract_text(doc, 0);
    /* simple.pdf may or may not have text; just verify API doesn't crash */
    if (text != NULL) {
        assert(strlen(text) >= 0);
        pdf_string_free(text);
    }

    pdf_document_free(doc);
    return 0;
}

/* ---------- Scenario 4: Read metadata ---------- */

static int test_metadata(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    /* Query all standard keys — may return NULL if not set */
    const char *keys[] = {"Title", "Author", "Subject", "Keywords", "Creator", "Producer"};
    for (int i = 0; i < 6; i++) {
        char *val = pdf_document_get_meta(doc, keys[i]);
        if (val != NULL) {
            pdf_string_free(val);
        }
    }

    /* Bookmark count should not crash */
    int bm = pdf_bookmark_count(doc);
    assert(bm >= 0);

    pdf_document_free(doc);
    return 0;
}

/* ---------- Scenario 5: Read AcroForm fields ---------- */

static int test_form_fields_read(void) {
    /* TODO: C API does not yet expose form field reading.
     * Requires: pdf_document_form_fields() or similar.
     * Depends on: pdf-capi extension with forms API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 6: Fill text field, save ---------- */

static int test_form_field_write(void) {
    /* TODO: C API does not yet expose form field writing.
     * Requires: pdf_form_set_field_value() + pdf_document_save().
     * Depends on: pdf-capi extension with forms write API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 7: Read annotations ---------- */

static int test_annotations_read(void) {
    /* TODO: C API does not yet expose annotation reading.
     * Requires: pdf_page_annotations() or similar.
     * Depends on: pdf-capi extension with annotations API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 8: Add highlight, save ---------- */

static int test_annotation_highlight(void) {
    /* TODO: C API does not yet expose annotation creation.
     * Requires: pdf_page_add_highlight() + pdf_document_save().
     * Depends on: pdf-capi extension with annotation write API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 9: Validate PDF/A ---------- */

static int test_pdfa_validation(void) {
    /* TODO: C API does not yet expose PDF/A validation.
     * Requires: pdf_validate_pdfa() or similar.
     * Depends on: pdf-capi extension with compliance API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 10: Merge 2 PDFs ---------- */

static int test_merge_pdfs(void) {
    /* TODO: C API does not yet expose PDF merging.
     * Requires: pdf_document_merge() or similar.
     * Depends on: pdf-capi extension with manipulation API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 11: Verify signature ---------- */

static int test_verify_signature(void) {
    /* TODO: C API does not yet expose signature verification.
     * Requires: pdf_document_verify_signatures() or similar.
     * Depends on: pdf-capi extension with signing API.
     */
    return 77; /* skip */
}

/* ---------- Scenario 12: Extract images ---------- */

static int test_extract_images(void) {
    /* TODO: C API does not yet expose image extraction.
     * Requires: pdf_page_extract_images() or similar.
     * Depends on: pdf-capi extension with image API.
     */
    return 77; /* skip */
}

/* ---------- Extra: page geometry boxes ---------- */

static int test_page_geometry(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    double x0, y0, x1, y1;
    s = pdf_page_media_box(doc, 0, &x0, &y0, &x1, &y1);
    assert(s == PDF_STATUS_OK);
    assert((x1 - x0) > 0.0);
    assert((y1 - y0) > 0.0);

    s = pdf_page_crop_box(doc, 0, &x0, &y0, &x1, &y1);
    assert(s == PDF_STATUS_OK);

    pdf_document_free(doc);
    return 0;
}

/* ---------- Extra: thumbnail rendering ---------- */

static int test_render_thumbnail(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(SAMPLE_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    uint32_t w = 0, h = 0;
    uint8_t *pixels = NULL;
    s = pdf_page_render_thumbnail(doc, 0, 200, &w, &h, &pixels);
    if (s != PDF_STATUS_OK) {
        pdf_document_free(doc);
        return 1;
    }

    assert(w > 0 && w <= 200);
    assert(h > 0 && h <= 200);
    assert(pixels != NULL);

    pdf_pixels_free(pixels, (size_t)w * h * 4);
    pdf_document_free(doc);
    return 0;
}

/* ---------- Extra: error handling ---------- */

static int test_error_handling(void) {
    pdf_clear_error();
    assert(pdf_get_last_error() == NULL);

    /* File not found */
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open("/nonexistent.pdf", NULL, &doc);
    assert(s == PDF_STATUS_ERROR_FILE_NOT_FOUND);
    assert(doc == NULL);
    const char *err = pdf_get_last_error();
    assert(err != NULL);

    /* Null document */
    assert(pdf_document_page_count(NULL) == -1);
    assert(pdf_page_width(NULL, 0) == 0.0);

    /* Negative page index */
    assert(pdf_page_rotation(NULL, -1) == 0);

    /* Free null is safe */
    pdf_document_free(NULL);
    pdf_string_free(NULL);
    pdf_pixels_free(NULL, 0);

    pdf_clear_error();
    return 0;
}

/* ---------- Extra: multi-page document ---------- */

static int test_multi_page(void) {
    PdfDocument *doc = NULL;
    PdfStatus s = pdf_document_open(MULTI_PDF, NULL, &doc);
    if (s != PDF_STATUS_OK) return 1;

    int pages = pdf_document_page_count(doc);
    assert(pages > 1);

    /* All pages should have positive dimensions */
    for (int i = 0; i < pages; i++) {
        assert(pdf_page_width(doc, i) > 0.0);
        assert(pdf_page_height(doc, i) > 0.0);
    }

    /* Out-of-range page should return 0 */
    assert(pdf_page_width(doc, pages) == 0.0);

    pdf_document_free(doc);
    return 0;
}

/* ---------- main ---------- */

int main(void) {
    printf("pdf-capi smoke tests\n");
    printf("====================\n");

    PdfStatus init = pdf_init();
    assert(init == PDF_STATUS_OK);

    printf("\nCore scenarios (12):\n");
    RUN_TEST(test_open_and_page_count);
    RUN_TEST(test_render_page);
    RUN_TEST(test_extract_text);
    RUN_TEST(test_metadata);
    RUN_TEST(test_form_fields_read);
    RUN_TEST(test_form_field_write);
    RUN_TEST(test_annotations_read);
    RUN_TEST(test_annotation_highlight);
    RUN_TEST(test_pdfa_validation);
    RUN_TEST(test_merge_pdfs);
    RUN_TEST(test_verify_signature);
    RUN_TEST(test_extract_images);

    printf("\nAdditional tests:\n");
    RUN_TEST(test_page_geometry);
    RUN_TEST(test_render_thumbnail);
    RUN_TEST(test_error_handling);
    RUN_TEST(test_multi_page);

    pdf_destroy();

    printf("\n====================\n");
    printf("Results: %d/%d passed, %d skipped, %d failed\n",
           tests_passed, tests_run, tests_skipped,
           tests_run - tests_passed - tests_skipped);

    const char *version = pdf_version();
    printf("Library version: %s\n", version);

    return (tests_run - tests_passed - tests_skipped > 0) ? 1 : 0;
}
