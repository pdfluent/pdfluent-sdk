/**
 * @file pdf_capi.h
 * @brief Backward-compatible alias for pdfluent.h.
 *
 * This header is retained for Swift module maps and downstream consumers that
 * already include it by this name.  All declarations now live in
 * include/pdfluent.h, which is the canonical single source of truth.
 *
 * New code should include <pdfluent.h> directly.
 */

#ifndef PDF_CAPI_H
#define PDF_CAPI_H

#include "include/pdfluent.h"

#endif /* PDF_CAPI_H */
