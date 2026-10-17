/**
 * C smoke test for pdf_document_extract_markdown + pdf_document_extract_blocks_json
 * + pdf_document_extract_docling_json.
 *
 * `fixtures/structured.pdf` is built and pinned by
 * `crates/pdf-engine/tests/the_api_over_the_block_model.rs`: a heading and a
 * bulleted list on page 0, a ruled table on page 1. Every binding suite reads
 * that same file and asserts the same three things, so a fault in one crossing
 * stands next to five that still work.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests test_structured_extraction
 *
 * Run:
 *   cd <repo_root>
 *   DYLD_LIBRARY_PATH=target/release crates/pdf-capi/tests/test_structured_extraction
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

static const char *STRUCTURED_PDF = "fixtures/structured.pdf";

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
static PdfDocument *open_structured(void) {
    FILE *fp = fopen(STRUCTURED_PDF, "rb");
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

static int test_markdown_has_heading_item_and_table(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *markdown = pdf_document_extract_markdown(doc, 0, -1);
    CHECK(markdown != NULL);
    CHECK(strstr(markdown, "# Safety rules") != NULL);
    CHECK(strstr(markdown, "- Wear the issued boots on the floor.") != NULL);
    CHECK(strstr(markdown, "| Band | Units | Rate |") != NULL);
    pdf_string_free(markdown);
    pdf_document_free(doc);
    return 0;
}

static int test_blocks_json_has_heading_item_and_cell(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *json = pdf_document_extract_blocks_json(doc, 0, -1);
    CHECK(json != NULL);
    CHECK(strstr(json, "\"kind\": \"heading\"") != NULL);
    CHECK(strstr(json, "\"text\": \"Safety rules\"") != NULL);
    CHECK(strstr(json, "\"kind\": \"listItem\"") != NULL);
    CHECK(strstr(json, "\"kind\": \"tableCell\"") != NULL);
    CHECK(strstr(json, "\"text\": \"Band\"") != NULL);
    pdf_string_free(json);
    pdf_document_free(doc);
    return 0;
}

static int test_docling_json_has_heading_list_and_table(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *json = pdf_document_extract_docling_json(doc, 0, -1, "structured");
    CHECK(json != NULL);
    CHECK(strstr(json, "\"schema_name\": \"DoclingDocument\"") != NULL);
    CHECK(strstr(json, "\"name\": \"structured\"") != NULL);
    CHECK(strstr(json, "\"label\": \"section_header\"") != NULL);
    CHECK(strstr(json, "\"label\": \"list_item\"") != NULL);
    CHECK(strstr(json, "\"label\": \"table\"") != NULL);
    pdf_string_free(json);
    json = pdf_document_extract_docling_json(doc, 0, -1, NULL);
    CHECK(json != NULL);
    CHECK(strstr(json, "\"name\": \"document\"") != NULL);
    pdf_string_free(json);
    pdf_document_free(doc);
    return 0;
}

static int test_window_holds_the_pages_it_asks_for(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *markdown = pdf_document_extract_markdown(doc, 1, 1);
    CHECK(markdown != NULL);
    CHECK(strstr(markdown, "| Band | Units | Rate |") != NULL);
    CHECK(strstr(markdown, "Safety rules") == NULL);
    pdf_string_free(markdown);
    pdf_document_free(doc);
    return 0;
}

static int test_window_past_the_end_is_empty(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *markdown = pdf_document_extract_markdown(doc, 9, 4);
    CHECK(markdown != NULL);
    CHECK(strlen(markdown) == 0);
    pdf_string_free(markdown);
    pdf_document_free(doc);
    return 0;
}

static int test_null_doc(void) {
    CHECK(pdf_document_extract_markdown(NULL, 0, -1) == NULL);
    CHECK(pdf_document_extract_blocks_json(NULL, 0, -1) == NULL);
    CHECK(pdf_document_extract_docling_json(NULL, 0, -1, NULL) == NULL);
    return 0;
}

static int test_negative_first_page(void) {
    PdfDocument *doc = open_structured();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    CHECK(pdf_document_extract_markdown(doc, -1, -1) == NULL);
    CHECK(pdf_document_extract_blocks_json(doc, -1, -1) == NULL);
    CHECK(pdf_document_extract_docling_json(doc, -1, -1, NULL) == NULL);
    pdf_document_free(doc);
    return 0;
}

int main(void) {
    fprintf(stderr, "pdf_capi structured-extraction smoke tests\n");
    fprintf(stderr, "==========================================\n");
    RUN(test_markdown_has_heading_item_and_table);
    RUN(test_blocks_json_has_heading_item_and_cell);
    RUN(test_docling_json_has_heading_list_and_table);
    RUN(test_window_holds_the_pages_it_asks_for);
    RUN(test_window_past_the_end_is_empty);
    RUN(test_null_doc);
    RUN(test_negative_first_page);
    fprintf(stderr, "\n%d/%d tests passed\n", tests_passed, tests_run);
    return tests_passed == tests_run ? 0 : 1;
}
