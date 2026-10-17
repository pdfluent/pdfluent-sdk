/**
 * C API journal smoke test — the compatible undo log over the C-ABI.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/tests test_journal
 *
 * Run (from project root):
 *   make -C crates/pdf-capi/tests run
 */

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../pdf_capi.h"

/* Path to shared fixture (run from project root) */
static const char *SAMPLE_PDF = "fixtures/sample.pdf";

static int tests_run = 0;
static int tests_passed = 0;

#define RUN_TEST(fn)                                     \
    do {                                                 \
        tests_run++;                                     \
        printf("  [%2d] %-40s ", tests_run, #fn);        \
        int _r = fn();                                   \
        if (_r == 0) { tests_passed++; printf("PASS\n"); } \
        else { printf("FAIL\n"); }                       \
    } while (0)

#define CHECK(s) do { if ((s) != PDF_STATUS_OK) return 1; } while (0)

static PdfCompatDocument *open_sample(void) {
    PdfCompatDocument *doc = NULL;
    PdfStatus s = pdf_compat_document_open(
        SAMPLE_PDF, NULL, 0, NULL, NULL, 0.0, 0.0, 0.0, NULL, &doc);
    if (s != PDF_STATUS_OK || doc == NULL) return NULL;
    return doc;
}

static int rotation_of(PdfCompatDocument *doc, int32_t *out) {
    return pdf_compat_page_rotation(doc, 0, out) == PDF_STATUS_OK ? 0 : 1;
}

/* ---------- Scenario 1: enable + availability ---------- */

static int test_enable_and_can_do(void) {
    PdfCompatDocument *doc = open_sample();
    if (doc == NULL) return 1;

    int32_t enabled = -1, undo = -1, redo = -1, pos = -1;
    CHECK(pdf_compat_document_journal_is_enabled(doc, &enabled));
    if (enabled != 0) { pdf_compat_document_close(doc); return 1; }
    CHECK(pdf_compat_document_journal_enable(doc));
    CHECK(pdf_compat_document_journal_is_enabled(doc, &enabled));
    if (enabled != 1) { pdf_compat_document_close(doc); return 1; }
    CHECK(pdf_compat_document_journal_can_do(doc, &undo, &redo));
    if (undo != 0 || redo != 0) { pdf_compat_document_close(doc); return 1; }
    CHECK(pdf_compat_document_journal_position(doc, &pos));
    if (pos != 0) { pdf_compat_document_close(doc); return 1; }

    CHECK(pdf_compat_document_close(doc));
    return 0;
}

/* ---------- Scenario 2: op records, undo/redo restore ---------- */

static int test_op_undo_redo(void) {
    PdfCompatDocument *doc = open_sample();
    if (doc == NULL) return 1;
    int32_t rot = -1;
    if (rotation_of(doc, &rot) != 0 || rot != 0) {
        pdf_compat_document_close(doc);
        return 1;
    }

    CHECK(pdf_compat_document_journal_enable(doc));
    CHECK(pdf_compat_document_journal_start_op(doc, "op1"));
    CHECK(pdf_compat_page_set_rotation(doc, 0, 90));
    CHECK(pdf_compat_document_journal_stop_op(doc));

    int32_t pos = -1, undo = -1, redo = -1;
    CHECK(pdf_compat_document_journal_position(doc, &pos));
    if (pos != 1) { pdf_compat_document_close(doc); return 1; }
    char *name = NULL;
    CHECK(pdf_compat_document_journal_op_name(doc, 0, &name));
    if (name == NULL || strcmp(name, "op1") != 0) {
        if (name != NULL) pdf_string_free(name);
        pdf_compat_document_close(doc);
        return 1;
    }
    pdf_string_free(name);
    if (rotation_of(doc, &rot) != 0 || rot != 90) {
        pdf_compat_document_close(doc);
        return 1;
    }

    CHECK(pdf_compat_document_journal_undo(doc));
    if (rotation_of(doc, &rot) != 0 || rot != 0) {
        pdf_compat_document_close(doc);
        return 1;
    }
    CHECK(pdf_compat_document_journal_can_do(doc, &undo, &redo));
    if (undo != 0 || redo != 1) { pdf_compat_document_close(doc); return 1; }

    CHECK(pdf_compat_document_journal_redo(doc));
    if (rotation_of(doc, &rot) != 0 || rot != 90) {
        pdf_compat_document_close(doc);
        return 1;
    }

    CHECK(pdf_compat_document_close(doc));
    return 0;
}

/* ---------- Scenario 3: save/load round-trip ---------- */

static int test_save_load_round_trip(void) {
    char path[256];
    /* P_tmpdir is undefined under glibc strict -std=c99/-std=c11. */
    const char *tmpdir = getenv("TMPDIR");
    if (tmpdir == NULL || tmpdir[0] == '\0') tmpdir = "/tmp";
    snprintf(path, sizeof(path), "%s/pdfluent-capi-journal-%d.bin",
             tmpdir, (int)rand());

    PdfCompatDocument *doc = open_sample();
    if (doc == NULL) return 1;
    CHECK(pdf_compat_document_journal_enable(doc));
    CHECK(pdf_compat_document_journal_start_op(doc, "op1"));
    CHECK(pdf_compat_page_set_rotation(doc, 0, 90));
    CHECK(pdf_compat_document_journal_stop_op(doc));
    CHECK(pdf_compat_document_journal_save(doc, path));
    CHECK(pdf_compat_document_close(doc));

    PdfCompatDocument *loaded = open_sample();
    if (loaded == NULL) { remove(path); return 1; }
    CHECK(pdf_compat_document_journal_load(loaded, path));
    int32_t pos = -1;
    CHECK(pdf_compat_document_journal_position(loaded, &pos));
    if (pos != 1) { pdf_compat_document_close(loaded); remove(path); return 1; }
    CHECK(pdf_compat_document_journal_undo(loaded));
    int32_t rot = -1;
    if (rotation_of(loaded, &rot) != 0 || rot != 0) {
        pdf_compat_document_close(loaded);
        remove(path);
        return 1;
    }
    CHECK(pdf_compat_document_close(loaded));
    remove(path);
    return 0;
}

int main(void) {
    printf("journal smoke tests\n");
    RUN_TEST(test_enable_and_can_do);
    RUN_TEST(test_op_undo_redo);
    RUN_TEST(test_save_load_round_trip);
    printf("journal: %d/%d passed\n", tests_passed, tests_run);
    return tests_passed == tests_run ? 0 : 1;
}
