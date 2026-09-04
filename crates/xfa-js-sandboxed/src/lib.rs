#![warn(missing_docs)]
//! Sandboxed QuickJS-backed JavaScript runtime for XFA calculate scripts.
//!
//! Exposes a minimal surface:
//! - `xfa.form.<fieldName>.rawValue` getter/setter
//! - `xfa.event.newText` (read-only)
//!
//! All host-capability accesses (`require`, `process`, `fetch`, …) return
//! [`XfaJsError::UnsupportedHostCapability`]. Every public method returns
//! `Result<T, XfaJsError>`; no panics.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
pub mod field_store;
pub mod runtime;
pub mod value;

pub use error::XfaJsError;
pub use field_store::{ExecCtx, FieldValues};
pub use runtime::XfaJsRuntime;
pub use value::JsValue;
