//! Door lane L6: inputs, batch and OCR selection (SPEC sections 15, 16, 17, 21).
//!
//! Native registration slot for the inputs lane of `pdfluent_extract`.
//! The lane adds its own bindings inside `register`; every other lane owns
//! a different file and leaves this one alone.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pyo3::prelude::*;

/// Whether an OCR engine is available to this build.
///
/// True when the `tesseract` binary answers; false when no binary is found
/// and on builds without OCR support compiled in. The language is not
/// checked: a binary without the requested language data still fails the
/// page that asks for it.
#[pyfunction]
fn ocr_engine_available() -> bool {
    #[cfg(feature = "ocr-cli")]
    {
        pdf_ocr::TesseractCliEngine::is_available()
    }
    #[cfg(not(feature = "ocr-cli"))]
    {
        false
    }
}

/// Register this lane's native items on the module.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(ocr_engine_available, m)?)?;
    Ok(())
}
