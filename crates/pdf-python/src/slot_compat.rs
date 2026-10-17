//! Slot P1: the document-compatible API, built by another team.
//!
//! Native registration slot for the compatibility package. Only that team
//! edits this file; door lanes leave it alone.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pyo3::prelude::*;

/// Register this package's native items on the module.
///
/// Compatible API: each lane's new module-level names, in fixed lane
/// order. Lane `#[pymethods]` blocks on Document/Page self-register.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    crate::compat::register(m)
}
