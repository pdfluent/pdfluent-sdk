/* c_abi_perf_loop.c — C-ABI binding overhead micro-benchmark.
 *
 * Measures the full FFI round-trip of opening a valid PDF from bytes,
 * reading page_count, and freeing the document, over N iterations.
 * Reports per-iteration nanosecond percentiles as a JSON object on stdout.
 *
 * Build:
 *   cc -O2 -I crates/pdf-capi/include -o /tmp/c_abi_perf_loop \
 *      scripts/perf/c_abi_perf_loop.c -L target/release -lpdf_capi
 * Run:
 *   c_abi_perf_loop <valid.pdf> <iterations>
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>
#include "pdfluent.h"

static int cmp_u64(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return (x > y) - (x < y);
}

static uint64_t pct(uint64_t *s, size_t n, double q) {
    if (n == 0) return 0;
    double idx = q * (double)(n - 1);
    size_t lo = (size_t)idx;
    size_t hi = lo + 1 < n ? lo + 1 : n - 1;
    double frac = idx - (double)lo;
    return (uint64_t)((double)s[lo] + ((double)s[hi] - (double)s[lo]) * frac);
}

int main(int argc, char **argv) {
    if (argc < 3) { fprintf(stderr, "usage: %s <pdf> <iters>\n", argv[0]); return 2; }
    const char *path = argv[1];
    long iters = strtol(argv[2], NULL, 10);
    if (iters <= 0) iters = 200;

    FILE *f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "cannot open %s\n", path); return 2; }
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *buf = (uint8_t *)malloc((size_t)sz);
    if (!buf || fread(buf, 1, (size_t)sz, f) != (size_t)sz) { fprintf(stderr, "read fail\n"); return 2; }
    fclose(f);

    /* warmup */
    for (int w = 0; w < 20; w++) {
        PdfDocument *d = NULL;
        if (pdf_document_open_from_bytes(buf, (size_t)sz, &d) == PDF_STATUS_OK && d) {
            (void)pdf_document_page_count(d);
            pdf_document_free(d);
        }
    }

    uint64_t *ns = (uint64_t *)malloc(sizeof(uint64_t) * (size_t)iters);
    long ok = 0;
    int pages = 0;
    for (long i = 0; i < iters; i++) {
        struct timespec t0, t1;
        clock_gettime(CLOCK_MONOTONIC, &t0);
        PdfDocument *d = NULL;
        PdfStatus s = pdf_document_open_from_bytes(buf, (size_t)sz, &d);
        if (s == PDF_STATUS_OK && d) {
            pages = pdf_document_page_count(d);
            pdf_document_free(d);
        }
        clock_gettime(CLOCK_MONOTONIC, &t1);
        ns[ok++] = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                 + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    }
    free(buf);
    qsort(ns, (size_t)ok, sizeof(uint64_t), cmp_u64);
    uint64_t mn = ok ? ns[0] : 0, mx = ok ? ns[ok - 1] : 0;
    printf("{\"op\":\"open+page_count+free\",\"samples\":%ld,\"pages\":%d,"
           "\"min_ns\":%llu,\"p50_ns\":%llu,\"p95_ns\":%llu,\"p99_ns\":%llu,\"max_ns\":%llu}\n",
           ok, pages,
           (unsigned long long)mn,
           (unsigned long long)pct(ns, (size_t)ok, 0.50),
           (unsigned long long)pct(ns, (size_t)ok, 0.95),
           (unsigned long long)pct(ns, (size_t)ok, 0.99),
           (unsigned long long)mx);
    free(ns);
    return ok > 0 ? 0 : 1;
}
