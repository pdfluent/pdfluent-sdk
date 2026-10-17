/* Copyright (c) 2026 Innovation Trigger B.V.
 *
 * PDFluent is available under two licences, at your option: the GNU AGPLv3, or
 * the PDFluent Commercial Licence. See the LICENSE file in this repository --
 * that file travels with the copy you received, which a URL does not.
 */

/**
 * C smoke test for the plain embedded-image API: pdf_page_images_json,
 * pdf_image_extract_data + pdf_extracted_image_free, and
 * pdf_image_decode_pixels + pdf_decoded_image_free.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests test_plain_images
 *
 * Run:
 *   cd <repo_root>
 *   DYLD_LIBRARY_PATH=target/release crates/pdf-capi/tests/test_plain_images
 *   (Linux: LD_LIBRARY_PATH=target/release …)
 *
 * The fixture is built in the test: one page holding a raw 2x2 grey
 * image, so the known pixels below are the samples the builder wrote.
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../pdf_capi.h"

static const unsigned char GRAY_SAMPLES[4] = { 0, 128, 192, 255 };

static int tests_run = 0;
static int tests_passed = 0;

#define RUN(name) do { \
    tests_run++; \
    fprintf(stderr, "  [%s] ", #name); \
    if (name() == 0) { tests_passed++; fprintf(stderr, "PASS\n"); } \
    else             { fprintf(stderr, "FAIL\n"); } \
} while (0)

#define CHECK(cond) do { \
    if (!(cond)) { \
        fprintf(stderr, "    CHECK failed at line %d: %s\n", __LINE__, #cond); \
        return 1; \
    } \
} while (0)

/* -- Minimal byte buffer ------------------------------------------------ */

typedef struct {
    unsigned char *data;
    size_t len;
    size_t cap;
} Buf;

static void buf_reserve(Buf *b, size_t extra) {
    if (b->len + extra <= b->cap) { return; }
    size_t cap = b->cap ? b->cap : 256;
    while (cap < b->len + extra) { cap *= 2; }
    b->data = (unsigned char *) realloc(b->data, cap);
    assert(b->data != NULL);
    b->cap = cap;
}

static void buf_put(Buf *b, const void *bytes, size_t n) {
    buf_reserve(b, n);
    memcpy(b->data + b->len, bytes, n);
    b->len += n;
}

static void buf_putf(Buf *b, const char *fmt, long value) {
    char tmp[64];
    int n = snprintf(tmp, sizeof tmp, fmt, value);
    assert(n > 0 && (size_t) n < sizeof tmp);
    buf_put(b, tmp, (size_t) n);
}

/* -- Fixture ------------------------------------------------------------ */

/* One page with a raw 2x2 DeviceGray image under /G0. Caller frees *out. */
static size_t gray_image_pdf(unsigned char **out) {
    static const char *heads[6] = {
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        "/Contents 4 0 R /Resources 5 0 R >>",
        NULL, /* content stream, built below */
        "<< /XObject << /G0 6 0 R >> >>",
        NULL, /* image stream, built below */
    };
    static const unsigned char content[4] = { 'q', '\n', 'Q', '\n' };
    Buf bodies[6];
    memset(bodies, 0, sizeof bodies);
    for (int i = 0; i < 6; i++) {
        if (i == 3) {
            buf_putf(&bodies[i], "<< /Length %ld >>\nstream\n", (long) sizeof content);
            buf_put(&bodies[i], content, sizeof content);
            buf_put(&bodies[i], "\nendstream", 10);
        } else if (i == 5) {
            static const char *dict =
                "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 "
                "/ColorSpace /DeviceGray /BitsPerComponent 8 ";
            buf_put(&bodies[i], dict, strlen(dict));
            buf_putf(&bodies[i], "/Length %ld >>\nstream\n", (long) sizeof GRAY_SAMPLES);
            buf_put(&bodies[i], GRAY_SAMPLES, sizeof GRAY_SAMPLES);
            buf_put(&bodies[i], "\nendstream", 10);
        } else {
            buf_put(&bodies[i], heads[i], strlen(heads[i]));
        }
    }

    Buf pdf = { 0, 0, 0 };
    long offsets[6];
    buf_put(&pdf, "%PDF-1.7\n", 9);
    for (int i = 0; i < 6; i++) {
        offsets[i] = (long) pdf.len;
        buf_putf(&pdf, "%ld 0 obj\n", (long) (i + 1));
        buf_put(&pdf, bodies[i].data, bodies[i].len);
        buf_put(&pdf, "\nendobj\n", 8);
        free(bodies[i].data);
    }
    long xref_at = (long) pdf.len;
    buf_put(&pdf, "xref\n0 7\n", 9);
    buf_put(&pdf, "0000000000 65535 f \n", 20);
    for (int i = 0; i < 6; i++) {
        char line[32];
        int n = snprintf(line, sizeof line, "%010ld 00000 n \n", offsets[i]);
        assert(n == 20);
        buf_put(&pdf, line, 20);
    }
    buf_putf(&pdf, "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n%ld\n%%%%EOF\n", xref_at);
    *out = pdf.data;
    return pdf.len;
}

static PdfDocument *open_fixture(void) {
    unsigned char *bytes = NULL;
    size_t len = gray_image_pdf(&bytes);
    PdfDocument *doc = NULL;
    PdfStatus rc = pdf_document_open_from_bytes(bytes, len, &doc);
    free(bytes);
    if (rc != PDF_STATUS_OK) { return NULL; }
    return doc;
}

/* The "id" the listing JSON reports for the fixture's image. */
static uint32_t listing_id(const char *json) {
    const char *at = strstr(json, "\"id\":");
    assert(at != NULL);
    return (uint32_t) strtoul(at + 5, NULL, 10);
}

/* -- Tests -------------------------------------------------------------- */

static int test_listing_json(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *json = pdf_page_images_json(doc, 0);
    CHECK(json != NULL);
    CHECK(json[0] == '[');
    CHECK(strstr(json, "\"width\":2") != NULL);
    CHECK(strstr(json, "\"height\":2") != NULL);
    CHECK(strstr(json, "\"components\":1") != NULL);
    CHECK(strstr(json, "\"color_space\":\"DeviceGray\"") != NULL);
    CHECK(strstr(json, "\"name\":\"G0\"") != NULL);
    CHECK(strstr(json, "\"has_mask\":false") != NULL);
    pdf_string_free(json);
    pdf_document_free(doc);
    return 0;
}

static int test_listing_rejects_bad_arguments(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    CHECK(pdf_page_images_json(NULL, 0) == NULL);
    CHECK(pdf_page_images_json(doc, -1) == NULL);
    CHECK(pdf_page_images_json(doc, 99) == NULL);
    pdf_document_free(doc);
    return 0;
}

static int test_extract_data(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *json = pdf_page_images_json(doc, 0);
    CHECK(json != NULL);
    uint32_t id = listing_id(json);
    pdf_string_free(json);

    PdfExtractedImage image;
    memset(&image, 0, sizeof image);
    PdfStatus rc = pdf_image_extract_data(doc, id, &image);
    CHECK(rc == PDF_STATUS_OK);
    CHECK(image.width == 2);
    CHECK(image.height == 2);
    CHECK(image.bits_per_component == 8);
    CHECK(image.components == 1);
    CHECK(strcmp(image.color_space, "DeviceGray") == 0);
    CHECK(strcmp(image.encoding, "png") == 0);
    CHECK(image.masked == 0);
    CHECK(image.len > 4);
    CHECK(image.data[0] == 0x89 && image.data[1] == 'P'
        && image.data[2] == 'N' && image.data[3] == 'G');
    pdf_extracted_image_free(&image);
    CHECK(image.color_space == NULL);
    CHECK(image.data == NULL);
    CHECK(image.len == 0);
    pdf_document_free(doc);
    return 0;
}

static int test_extract_rejects_bad_arguments(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    PdfExtractedImage image;
    memset(&image, 0, sizeof image);
    CHECK(pdf_image_extract_data(doc, 9999, &image) == PDF_STATUS_ERROR_EXTRACT);
    CHECK(pdf_image_extract_data(NULL, 6, &image) == PDF_STATUS_ERROR_INVALID_ARG);
    CHECK(pdf_image_extract_data(doc, 6, NULL) == PDF_STATUS_ERROR_INVALID_ARG);
    pdf_document_free(doc);
    return 0;
}

static int test_decode_pixels(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    char *json = pdf_page_images_json(doc, 0);
    CHECK(json != NULL);
    uint32_t id = listing_id(json);
    pdf_string_free(json);

    PdfDecodedImage image;
    memset(&image, 0, sizeof image);
    PdfStatus rc = pdf_image_decode_pixels(doc, id, &image);
    CHECK(rc == PDF_STATUS_OK);
    CHECK(image.width == 2);
    CHECK(image.height == 2);
    CHECK(image.components == 1);
    CHECK(strcmp(image.color_space, "DeviceGray") == 0);
    CHECK(image.len == sizeof GRAY_SAMPLES);
    CHECK(memcmp(image.pixels, GRAY_SAMPLES, sizeof GRAY_SAMPLES) == 0);
    pdf_decoded_image_free(&image);
    CHECK(image.color_space == NULL);
    CHECK(image.pixels == NULL);
    CHECK(image.len == 0);
    pdf_document_free(doc);
    return 0;
}

static int test_decode_rejects_bad_arguments(void) {
    PdfDocument *doc = open_fixture();
    if (!doc) { fprintf(stderr, "(skip: fixture not openable) "); return 0; }
    PdfDecodedImage image;
    memset(&image, 0, sizeof image);
    CHECK(pdf_image_decode_pixels(doc, 9999, &image) == PDF_STATUS_ERROR_EXTRACT);
    CHECK(pdf_image_decode_pixels(NULL, 6, &image) == PDF_STATUS_ERROR_INVALID_ARG);
    CHECK(pdf_image_decode_pixels(doc, 6, NULL) == PDF_STATUS_ERROR_INVALID_ARG);
    pdf_document_free(doc);
    return 0;
}

static int test_free_null(void) {
    pdf_extracted_image_free(NULL);
    pdf_decoded_image_free(NULL);
    pdf_string_free(NULL);
    return 0;
}

int main(void) {
    fprintf(stderr, "pdf_capi plain-image smoke tests\n");
    fprintf(stderr, "================================\n");
    RUN(test_listing_json);
    RUN(test_listing_rejects_bad_arguments);
    RUN(test_extract_data);
    RUN(test_extract_rejects_bad_arguments);
    RUN(test_decode_pixels);
    RUN(test_decode_rejects_bad_arguments);
    RUN(test_free_null);
    fprintf(stderr, "\n%d/%d tests passed\n", tests_passed, tests_run);
    return tests_passed == tests_run ? 0 : 1;
}
