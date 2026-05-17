#![warn(missing_docs)]
//! XFA DOM Resolver — SOM path resolution and DOM management.
//!
//! Implements the Scripting Object Model (SOM) from XFA 3.3 §3,
//! including Template DOM, Data DOM, and Form DOM construction.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, DomError>`.

pub mod data_dom;
pub mod error;
pub mod som;
pub mod template_dom;
pub mod xfa_dom;
