//! Native side of the compatible Python API: one file per lane.
//!
//! Each lane adds its methods to the EXISTING `Document`/`Page` classes with
//! its own `#[pymethods]` blocks inside its own file (enabled by PyO3's
//! `multiple-pymethods` feature); those blocks self-register and need no call
//! here. What each lane's `register` adds is only NEW module-level names —
//! new classes and free functions — in the fixed order below, so two lanes
//! never edit the same code. Every name a lane registers must also be listed
//! in its `python/pdfluent/_compat_<lane>.py`, which is what `pdfluent/__init__.py`
//! re-exports.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pyo3::prelude::*;

pub mod document;
pub mod edit;
pub mod errors;
pub mod functions;
pub mod geometry;
pub mod pixmap;
pub mod text;

/// The error for a value of a `#[non_exhaustive]` engine enum that this
/// binding does not know yet.
///
/// The engine's 1.x enums are `#[non_exhaustive]`, so a later minor release
/// may add a variant this build was not compiled against. Mapping it onto a
/// neighbouring Python value would misreport it; raising says what happened.
pub(crate) fn unknown_variant(what: &str) -> PyErr {
    pyo3::exceptions::PyNotImplementedError::new_err(format!(
        "{what}: this value is not supported by this version of the Python binding"
    ))
}

/// Register every lane's new module-level names, in fixed lane order.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(m)?;
    geometry::register(m)?;
    text::register(m)?;
    document::register(m)?;
    pixmap::register(m)?;
    edit::register(m)?;
    functions::register(m)?;
    Ok(())
}
