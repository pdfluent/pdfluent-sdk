/* QR-11 C-ABI runtime error-mapping smoke.
 * Triggers canonical error cases and asserts the typed PdfStatus mapping
 * (not a crash, not silent success). Build+run via the lane script. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include "pdfluent.h"

int main(void) {
    int fails = 0;
    PdfDocument *doc = NULL;

    /* 1. malformed bytes -> non-OK typed status, *out left NULL, no crash. */
    const uint8_t garbage[] = {0xDE,0xAD,0xBE,0xEF,0x00,0x42};
    PdfStatus s = pdf_document_open_from_bytes(garbage, sizeof(garbage), &doc);
    if (s == PDF_STATUS_OK) { fprintf(stderr,"FAIL malformed returned OK\n"); fails++; }
    if (doc != NULL)        { fprintf(stderr,"FAIL malformed left non-NULL out\n"); fails++; }
    printf("malformed -> status=%d (expect non-zero typed)\n", (int)s);

    /* 2. NULL data -> invalid-arg typed status, no crash. */
    doc = NULL;
    s = pdf_document_open_from_bytes(NULL, 0, &doc);
    if (s == PDF_STATUS_OK) { fprintf(stderr,"FAIL null-data returned OK\n"); fails++; }
    printf("null-data -> status=%d (expect non-zero typed)\n", (int)s);

    /* 3. NULL out pointer -> invalid-arg typed status, no crash. */
    s = pdf_document_open_from_bytes(garbage, sizeof(garbage), NULL);
    if (s == PDF_STATUS_OK) { fprintf(stderr,"FAIL null-out returned OK\n"); fails++; }
    printf("null-out -> status=%d (expect non-zero typed)\n", (int)s);

    if (fails == 0) { printf("QR-11 C-ABI runtime error-mapping: OK\n"); return 0; }
    fprintf(stderr,"QR-11 C-ABI: %d failure(s)\n", fails); return 1;
}
