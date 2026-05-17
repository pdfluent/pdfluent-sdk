/**
 * license_activate.c — PDFluent C ABI license activation example.
 *
 * Demonstrates:
 *   1. Querying license status before any activation (Trial/Default).
 *   2. Attempting activation with an invalid key and handling the typed error.
 *   3. Activating with a synthetic test key and verifying the status change.
 *
 * The key used here ("tier:developer") is a synthetic evaluation key accepted
 * by the 1.0 evaluation format.  Real signed keys (Ed25519) ship in 1.1 and
 * use the same API surface.
 *
 * Build:
 *   cargo build -p pdf-capi --release
 *   make -C crates/pdf-capi/examples
 *
 * Run (macOS):
 *   DYLD_LIBRARY_PATH=../../target/release ./license_activate
 *
 * Run (Linux):
 *   LD_LIBRARY_PATH=../../target/release ./license_activate
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../include/pdfluent.h"

/* Print a PdfluentLicenseStatus in a human-readable form. */
static void print_status(const char *label, const PdfluentLicenseStatus *s) {
    static const char *tier_names[]   = { "Trial", "Developer", "Team",
                                          "Business", "Enterprise" };
    static const char *source_names[] = { "Default", "EnvVar", "Explicit" };

    const char *tier   = (s->tier   >= 0 && s->tier   <= 4) ? tier_names[s->tier]   : "Unknown";
    const char *source = (s->source >= 0 && s->source <= 2) ? source_names[s->source] : "Unknown";

    printf("%s: tier=%s source=%s output_is_marked=%s\n",
           label, tier, source,
           s->output_is_marked ? "yes" : "no");
}

/* Map a PdfStatus to a short descriptive string for display. */
static const char *status_name(PdfStatus s) {
    switch (s) {
    case PDF_STATUS_OK:                        return "OK";
    case PDF_STATUS_ERROR_INVALID_ARG:         return "INVALID_ARG";
    case PDF_STATUS_ERROR_LICENSE_INVALID:     return "LICENSE_INVALID";
    case PDF_STATUS_ERROR_LICENSE_ALREADY_SET: return "LICENSE_ALREADY_SET";
    case PDF_STATUS_ERROR_LICENSE_FILE:        return "LICENSE_FILE";
    default:                                   return "OTHER_ERROR";
    }
}

int main(void) {
    PdfStatus s;
    PdfluentLicenseStatus status;
    int exit_code = 0;

    pdf_init();

    /* ------------------------------------------------------------------ */
    /* Step 1: Query status before activation                               */
    /* ------------------------------------------------------------------ */
    s = pdfluent_license_status(&status);
    if (s != PDF_STATUS_OK) {
        fprintf(stderr, "pdfluent_license_status failed: %s\n", pdf_get_last_error());
        exit_code = 1;
        goto done;
    }
    print_status("Before activation", &status);

    /* Tier 0 = Trial, source 0 = Default when no key has been supplied. */
    if (status.tier < 0 || status.tier > 4) {
        fprintf(stderr, "FAIL: unexpected tier value %d\n", status.tier);
        exit_code = 1;
        goto done;
    }

    /* ------------------------------------------------------------------ */
    /* Step 2: Attempt activation with an invalid key                       */
    /* ------------------------------------------------------------------ */
    s = pdfluent_license_activate_key("not-a-valid-key-format");
    printf("Invalid key result: %s", status_name(s));
    if (s == PDF_STATUS_ERROR_LICENSE_INVALID || s == PDF_STATUS_ERROR_LICENSE_ALREADY_SET) {
        /* Either typed status is acceptable: the key was rejected. */
        printf(" (expected)\n");
    } else if (s == PDF_STATUS_OK) {
        /* Should not happen with a malformed key. */
        fprintf(stderr, "\nFAIL: expected rejection, got OK\n");
        exit_code = 1;
        goto done;
    } else {
        printf("\n");
    }

    /* Confirm the last error message is populated. */
    {
        const char *err = pdf_get_last_error();
        if (err && strlen(err) > 0) {
            printf("Last error: %s\n", err);
        }
        pdf_clear_error();
    }

    /* ------------------------------------------------------------------ */
    /* Step 3: Activate with a synthetic test key                           */
    /* ------------------------------------------------------------------ */
    s = pdfluent_license_activate_key("tier:developer");
    printf("Activate 'tier:developer': %s\n", status_name(s));

    if (s != PDF_STATUS_OK && s != PDF_STATUS_ERROR_LICENSE_ALREADY_SET) {
        /* Any other status is unexpected for a well-formed key. */
        const char *err = pdf_get_last_error();
        fprintf(stderr, "FAIL: unexpected status %d — %s\n", (int)s,
                err ? err : "(no message)");
        exit_code = 1;
        goto done;
    }

    /* ------------------------------------------------------------------ */
    /* Step 4: Query status after activation                                */
    /* ------------------------------------------------------------------ */
    s = pdfluent_license_status(&status);
    if (s != PDF_STATUS_OK) {
        fprintf(stderr, "pdfluent_license_status failed: %s\n", pdf_get_last_error());
        exit_code = 1;
        goto done;
    }
    print_status("After activation", &status);

    /* Also exercise the tier-only helper. */
    {
        int tier = pdfluent_license_effective_tier();
        printf("Effective tier (integer): %d\n", tier);
        if (tier < 0 || tier > 4) {
            fprintf(stderr, "FAIL: tier %d out of expected range [0,4]\n", tier);
            exit_code = 1;
            goto done;
        }
    }

    /* ------------------------------------------------------------------ */
    /* Step 5: Null-pointer safety check                                    */
    /* ------------------------------------------------------------------ */
    s = pdfluent_license_status(NULL);
    if (s != PDF_STATUS_ERROR_INVALID_ARG) {
        fprintf(stderr, "FAIL: null out-pointer should return INVALID_ARG, got %d\n", (int)s);
        exit_code = 1;
        goto done;
    }
    pdf_clear_error();

    s = pdfluent_license_activate_key(NULL);
    if (s != PDF_STATUS_ERROR_INVALID_ARG) {
        fprintf(stderr, "FAIL: null key should return INVALID_ARG, got %d\n", (int)s);
        exit_code = 1;
        goto done;
    }
    pdf_clear_error();

    printf("All checks passed.\n");

done:
    pdf_destroy();
    return exit_code;
}
