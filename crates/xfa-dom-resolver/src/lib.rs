#![warn(missing_docs)]
//! XFA DOM Resolver — SOM path resolution and DOM management.
//!
//! Implements the Scripting Object Model (SOM) from XFA 3.3 §3,
//! including Template DOM, Data DOM, and Form DOM construction.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, DomError>`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod data_dom;
pub mod error;
pub mod som;
pub mod template_dom;
pub mod xfa_dom;
