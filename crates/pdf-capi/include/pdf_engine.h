/**
 * @file pdf_engine.h
 * @brief Deprecated alias for pdfluent.h.
 *
 * This header was the original public interface.  It is superseded by
 * pdfluent.h and retained only for backward compatibility with pre-1.0
 * consumers.  It will be removed in the next major version.
 *
 * Migrate: replace @c #include "pdf_engine.h" with @c #include "pdfluent.h"
 * and update status-code names (e.g. @c PDF_STATUS_ERROR_INVALID_ARG →
 * @c PDF_STATUS_ERROR_INVALID_ARG is unchanged; but add codes 7-15 that were
 * absent in the old header).
 */

#ifndef PDF_ENGINE_H
#define PDF_ENGINE_H

#include "pdfluent.h"

#endif /* PDF_ENGINE_H */
