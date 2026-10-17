//! Door lane L5: tables and plain text (SPEC sections 11, 13).
//!
//! Native registration slot for the tables lane of `pdfluent_extract`.
//! The lane adds its own bindings inside `register`; every other lane owns
//! a different file and leaves this one alone.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pyo3::prelude::*;

/// Register this lane's native items on the module (nothing yet).
pub(crate) fn register(_m: &Bound<PyModule>) -> PyResult<()> {
    Ok(())
}
