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
