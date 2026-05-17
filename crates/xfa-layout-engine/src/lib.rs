#![warn(missing_docs)]
//! XFA Layout Engine — Box Model and pagination.
//!
//! Implements the XFA layout algorithms from XFA 3.3 §4 and §8,
//! including positioned/flowed layout, content splitting, and pagination.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, LayoutError>`.

pub mod error;
pub mod form;
pub mod layout;
#[cfg(not(target_arch = "wasm32"))]
pub mod scripting;
pub mod text;
pub mod types;
