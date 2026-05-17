# C ABI License Activation

This document describes the license activation surface exposed by the C ABI
(`include/pdfluent.h`).  All symbols are declared in that header and implemented
in `crates/pdf-capi/src/license.rs`.

## Types

### `PdfluentLicenseStatus`

A plain struct that represents a consistent snapshot of the license state.
All fields are `int` so the struct has a stable, padding-free layout on all
supported platforms.

```c
typedef struct {
    int tier;            /* 0=Trial 1=Developer 2=Team 3=Business 4=Enterprise */
    int source;          /* 0=Default 1=EnvVar 2=Explicit                      */
    int output_is_marked;/* 1 if /Producer marks output (Trial only), else 0   */
} PdfluentLicenseStatus;
```

Ownership: the caller allocates this struct (on the stack is fine) and passes
its address to `pdfluent_license_status`.  The struct does not need to be freed
with any free function.

## Functions

### `pdfluent_license_activate_key`

```c
PdfStatus pdfluent_license_activate_key(const char *key);
```

Activate the process-global license from a null-terminated UTF-8 key string.
The key is consumed immediately and never stored locally.

**Returns**

| Code | Meaning |
|------|---------|
| `PDF_STATUS_OK` | Activation succeeded |
| `PDF_STATUS_ERROR_INVALID_ARG` | `key` is NULL or not valid UTF-8 |
| `PDF_STATUS_ERROR_LICENSE_INVALID` (16) | Key is malformed or names an unknown tier |
| `PDF_STATUS_ERROR_LICENSE_ALREADY_SET` (17) | Process already activated to a different tier |

### `pdfluent_license_activate_file`

```c
PdfStatus pdfluent_license_activate_file(const char *path);
```

Activate by reading a license key from a UTF-8 text file (whitespace is
stripped).  Internally calls `pdfluent_license_activate_key`.

**Returns**

| Code | Meaning |
|------|---------|
| `PDF_STATUS_OK` | Activation succeeded |
| `PDF_STATUS_ERROR_INVALID_ARG` | `path` is NULL or not valid UTF-8 |
| `PDF_STATUS_ERROR_LICENSE_FILE` (18) | File cannot be opened or read |
| `PDF_STATUS_ERROR_LICENSE_INVALID` (16) | File contents are not a valid key |
| `PDF_STATUS_ERROR_LICENSE_ALREADY_SET` (17) | Process already activated to a different tier |

### `pdfluent_license_effective_tier`

```c
int pdfluent_license_effective_tier(void);
```

Return the effective tier as an integer (0–4).  Returns -1 for a future tier
variant not yet mapped by this binding version.  Equivalent to calling
`pdfluent_license_status` and reading the `tier` field.

### `pdfluent_license_status`

```c
PdfStatus pdfluent_license_status(PdfluentLicenseStatus *out);
```

Fill `*out` with a snapshot of the current license status.

**Returns**

| Code | Meaning |
|------|---------|
| `PDF_STATUS_OK` | `*out` was written successfully |
| `PDF_STATUS_ERROR_INVALID_ARG` | `out` is NULL |

## Status codes (license-specific)

These codes are part of the standard `PdfStatus` enum defined in `pdfluent.h`.

| Value | Name | Meaning |
|-------|------|---------|
| 16 | `PDF_STATUS_ERROR_LICENSE_INVALID` | Key malformed or tier unknown |
| 17 | `PDF_STATUS_ERROR_LICENSE_ALREADY_SET` | Tier already set; restart to change |
| 18 | `PDF_STATUS_ERROR_LICENSE_FILE` | License file could not be read |

## C8 Error Catalogue Mapping

| C8 code | C ABI status |
|---------|-------------|
| `ErrorInvalidLicense` | `PDF_STATUS_ERROR_LICENSE_INVALID = 16` |
| `ErrorLicenseAlreadySet` | `PDF_STATUS_ERROR_LICENSE_ALREADY_SET = 17` |
| `ErrorLicenseFile` | `PDF_STATUS_ERROR_LICENSE_FILE = 18` |

## Example

See `crates/pdf-capi/examples/license_activate.c` for a complete worked example.
The snippet below shows the core pattern:

```c
#include "pdfluent.h"
#include <stdio.h>

int main(void) {
    pdf_init();

    /* Query status before activation */
    PdfluentLicenseStatus s;
    pdfluent_license_status(&s);
    printf("tier=%d source=%d marked=%d\n",
           s.tier, s.source, s.output_is_marked);

    /* Activate */
    PdfStatus st = pdfluent_license_activate_key("tier:developer");
    if (st == PDF_STATUS_ERROR_LICENSE_INVALID) {
        fprintf(stderr, "bad key: %s\n", pdf_get_last_error());
        pdf_destroy();
        return 1;
    }

    /* Query again */
    pdfluent_license_status(&s);
    printf("tier=%d source=%d marked=%d\n",
           s.tier, s.source, s.output_is_marked);

    pdf_destroy();
    return 0;
}
```

## Build the example

```bash
# From the project root:
cargo build -p pdf-capi --release
make -C crates/pdf-capi/examples

# Run (macOS):
DYLD_LIBRARY_PATH=target/release crates/pdf-capi/examples/license_activate

# Run (Linux):
LD_LIBRARY_PATH=target/release crates/pdf-capi/examples/license_activate
```

The example Makefile compiles with `-Wall -Wextra -Werror`.

## Thread safety and process-global state

The license tier is stored in a Rust `OnceLock` — it is written exactly once
per process and then read-only.  `pdfluent_license_status` and
`pdfluent_license_effective_tier` are safe to call from any thread at any time.
`pdfluent_license_activate_key` / `pdfluent_license_activate_file` are safe to
call concurrently but only the first successful call takes effect; subsequent
calls with the same tier return `PDF_STATUS_OK` (idempotent) and calls with a
different tier return `PDF_STATUS_ERROR_LICENSE_ALREADY_SET`.

## Security notes

- The SDK never logs license keys.
- Error messages report parse failure modes only; the raw key string is not
  included.
- Tests use only synthetic evaluation keys (`tier:developer`, etc.).  No real
  signed key should be committed to source control.
