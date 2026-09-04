#![warn(missing_docs)]
//! XFA Layout Engine — Box Model and pagination.
//!
//! Implements the XFA layout algorithms from XFA 3.3 §4 and §8,
//! including positioned/flowed layout, content splitting, and pagination.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, LayoutError>`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
pub mod form;
pub mod layout;
#[cfg(not(target_arch = "wasm32"))]
pub mod scripting;
pub mod text;
pub mod types;
