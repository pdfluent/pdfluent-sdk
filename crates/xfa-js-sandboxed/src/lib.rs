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

pub mod error;
pub mod field_store;
pub mod runtime;
pub mod value;

pub use error::XfaJsError;
pub use field_store::{ExecCtx, FieldValues};
pub use runtime::XfaJsRuntime;
pub use value::JsValue;
