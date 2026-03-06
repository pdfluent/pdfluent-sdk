//! AcroForm engine for PDF interactive forms.
//!
//! Provides parsing, field manipulation, appearance generation, and
//! flattening of AcroForm (non-XFA) form fields.

pub mod actions;
pub mod appearance;
pub mod button;
pub mod choice;
pub mod flags;
pub mod flatten;
pub mod parse;
pub mod text;
pub mod tree;

pub use flags::FieldFlags;
pub use parse::parse_acroform;
pub use tree::{FieldId, FieldNode, FieldTree, FieldType, FieldValue, Quadding};
