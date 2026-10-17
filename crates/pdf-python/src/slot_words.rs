//! Slot W1: word boxes, built by another team.
//!
//! Native registration slot for the word-box package. Only that team edits
//! this file; door lanes leave it alone.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{ExtractOptions, PdfDocument, Word};
use pyo3::prelude::*;

/// One page's words, with the box each one was drawn in.
///
/// The body behind `Page.words`: the page's own window, so blocks are
/// numbered within the page. The method itself stays in the class's single
/// `#[pymethods]` table in `lib.rs`, which calls here and maps the engine
/// words to the Python type -- pyo3 allows one such table per class.
pub(crate) fn page_words(
    doc: &PdfDocument,
    index: usize,
) -> Result<Vec<Word>, pdf_engine::EngineError> {
    let mut options = ExtractOptions::default();
    options.pages = Some(vec![index]);
    doc.extract_words(&options)
}

/// Register this package's native items on the module (nothing yet).
pub(crate) fn register(_m: &Bound<PyModule>) -> PyResult<()> {
    Ok(())
}
