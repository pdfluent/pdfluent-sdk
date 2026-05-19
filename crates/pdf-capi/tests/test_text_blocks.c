/**
 * C smoke test for pdf_page_extract_text_blocks + pdf_text_blocks_free.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests test_text_blocks
 *
 * Run:
 *   cd <repo_root>
 *   DYLD_LIBRARY_PATH=target/release crates/pdf-capi/tests/test_text_blocks
 *   (Linux: LD_LIBRARY_PATH=target/release …)
 *
 * Build options: -Wall -Wextra -std=c11 -pedantic (-Werror via Makefile).
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../pdf_capi.h"

static const char *SAMPLE_PDF = "fixtures/sample.pdf";

static int tests_run = 0;
static int tests_passed = 0;

#define RUN(name) do { \
    tests_run++; \
    fprintf(stderr, "  [%s] ", #name); \
    if (name() == 0) { tests_passed++; fprintf(stderr, "PASS\n"); } \
    else            { fprintf(stderr, "FAIL\n"); } \
} while (0)

#define CHECK(cond) do { \
    if (!(cond)) { \
        fprintf(stderr, "    CHECK failed at line %d: %s\n", __LINE__, #cond); \
        return 1; \
    } \
} while (0)

/* Open the shared fixture; caller must free with pdf_document_free. */
static PdfDocument *open_sample(void) {
    FILE *fp = fopen(SAMPLE_PDF, "rb");
    if (!fp) { return NULL; }
    fseek(fp, 0, SEEK_END);
    long size = ftell(fp);
    fseek(fp, 0, SEEK_SET);
    if (size <= 0) { fclose(fp); return NULL; }
    unsigned char *buf = (unsigned char *) malloc((size_t) size);
    if (!buf) { fclose(fp); return NULL; }
    if (fread(buf, 1, (size_t) size, fp) != (size_t) size) {
        free(buf); fclose(fp); return NULL;
    }
    fclose(fp);

    PdfDocument *doc = NULL;
    PdfStatus rc = pdf_document_open_from_bytes(buf, (size_t) size, &doc);
    free(buf);
    if (rc != PDF_STATUS_OK) { return NULL; }
    return doc;
}

static int test_null_doc(void) {
    PdfTextBlock *blocks = NULL;
    size_t count = 999;
    PdfStatus rc = pdf_page_extract_text_blocks(NULL, 0, &blocks, &count);
    CHECK(rc == PDF_STATUS_ERROR_INVALID_ARG);
    CHECK(blocks == NULL);
    CHECK(count == 0);
    return 0;
}

static int test_null_out_blocks(void) {
    PdfDocument *doc = open_sample();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    size_t count = 999;
    PdfStatus rc = pdf_page_extract_text_blocks(doc, 0, NULL, &count);
    CHECK(rc == PDF_STATUS_ERROR_INVALID_ARG);
    pdf_document_free(doc);
    return 0;
}

static int test_negative_page(void) {
    PdfDocument *doc = open_sample();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    PdfTextBlock *blocks = NULL;
    size_t count = 999;
    PdfStatus rc = pdf_page_extract_text_blocks(doc, -1, &blocks, &count);
    CHECK(rc == PDF_STATUS_ERROR_INVALID_ARG);
    CHECK(blocks == NULL);
    CHECK(count == 0);
    pdf_document_free(doc);
    return 0;
}

static int test_out_of_range_page(void) {
    PdfDocument *doc = open_sample();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    PdfTextBlock *blocks = NULL;
    size_t count = 999;
    PdfStatus rc = pdf_page_extract_text_blocks(doc, 9999, &blocks, &count);
    CHECK(rc == PDF_STATUS_ERROR_PAGE_RANGE);
    CHECK(blocks == NULL);
    CHECK(count == 0);
    pdf_document_free(doc);
    return 0;
}

static int test_valid_page(void) {
    PdfDocument *doc = open_sample();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    PdfTextBlock *blocks = NULL;
    size_t count = 0;
    PdfStatus rc = pdf_page_extract_text_blocks(doc, 0, &blocks, &count);
    CHECK(rc == PDF_STATUS_OK);
    /* Sample fixture is expected to have at least one block. */
    CHECK(count > 0);
    CHECK(blocks != NULL);
    for (size_t i = 0; i < count; i++) {
        CHECK(blocks[i].width  >= 0.0);
        CHECK(blocks[i].height >= 0.0);
        CHECK(blocks[i].text   != NULL);
        /* Ensure the text is a valid C string (find terminator). */
        size_t tlen = strlen(blocks[i].text);
        (void) tlen;
    }
    pdf_text_blocks_free(blocks, count);
    pdf_document_free(doc);
    return 0;
}

static int test_free_null(void) {
    pdf_text_blocks_free(NULL, 0);
    pdf_text_blocks_free(NULL, 42);
    return 0;
}

int main(void) {
    fprintf(stderr, "pdf_capi text-block smoke tests\n");
    fprintf(stderr, "================================\n");
    RUN(test_null_doc);
    RUN(test_null_out_blocks);
    RUN(test_negative_page);
    RUN(test_out_of_range_page);
    RUN(test_valid_page);
    RUN(test_free_null);
    fprintf(stderr, "\n%d/%d tests passed\n", tests_passed, tests_run);
    return tests_passed == tests_run ? 0 : 1;
}
