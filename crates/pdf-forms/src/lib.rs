#![warn(missing_docs)]
//! AcroForm engine for PDF interactive forms.
//!
//! Parses AcroForm field dictionaries from a [`pdf_syntax::Pdf`] into an
//! in-memory [`FieldTree`] — an arena-based tree that mirrors the AcroForm
//! parent/child hierarchy.  Supports field value read/write, appearance
//! generation, and form flattening.
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//! use pdf_syntax::Pdf;
//! use pdf_forms::{parse_acroform, FieldType, FieldValue};
//!
//! let data = Arc::new(std::fs::read("form.pdf").unwrap());
//! let pdf = Pdf::new(data).unwrap();
//!
//! let tree = parse_acroform(&pdf).expect("no AcroForm found");
//!
//! for id in tree.terminal_fields() {
//!     let name  = tree.fully_qualified_name(id);
//!     let ftype = tree.effective_field_type(id);
//!     let value = tree.effective_value(id);
//!     println!("[{ftype:?}] {name} = {value:?}");
//! }
//!
//! // Find a field by name and inspect its properties.
//! if let Some(id) = tree.find_by_name("Address.Street") {
//!     let node = tree.get(id);
//!     println!("max_len = {:?}", node.max_len);
//! }
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |---|---|
//! | [`FieldTree`] | Arena of all AcroForm field nodes |
//! | [`FieldNode`] | A single field or group (partial name, type, value, …) |
//! | [`FieldId`] | Opaque index into the arena |
//! | [`FieldType`] | `Text`, `Button`, `Choice`, `Signature` |
//! | [`FieldValue`] | Text string or array of selected strings (choice fields) |
//! | [`FieldFlags`] | Bit-field from PDF `/Ff` — read-only, required, multiline, … |
//! | [`FormAccess`] | Trait used by language bindings for unified AcroForm / XFA access |
//!
//! # Entry Point
//!
//! [`parse_acroform`] is the single parser entry point. It returns `None` when
//! the document has no `/AcroForm` dictionary.

pub mod actions;
pub mod appearance;
pub mod button;
pub mod choice;
pub mod facade;
pub mod flags;
pub mod flatten;
pub mod parse;
pub mod text;
pub mod tree;

pub use facade::{DocumentOps, FormAccess, FormError, FormKind};
pub use flags::FieldFlags;
pub use parse::parse_acroform;
pub use tree::{FieldId, FieldNode, FieldTree, FieldType, FieldValue, Quadding};
