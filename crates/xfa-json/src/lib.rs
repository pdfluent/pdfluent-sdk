#![warn(missing_docs)]
//! JSON-first API for XFA forms.
//!
//! Provides bidirectional conversion between XFA `FormTree` structures and JSON,
//! with automatic type coercion, repeating section support, and schema export.
//!
//! # Examples
//!
//! ```
//! use xfa_json::{form_tree_to_json, json_to_form_tree, export_schema};
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod coerce;
pub mod export;
pub mod import;
pub mod schema;
pub mod types;

pub use export::form_tree_to_json;
pub use export::form_tree_to_value;
pub use import::json_to_form_tree;
pub use schema::export_schema;
pub use types::{FieldValue, FormData, FormSchema};
