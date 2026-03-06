//! Form data exchange and ZUGFeRD/Factur-X e-invoicing.
//!
//! This crate provides three capabilities:
//!
//! 1. **FDF/XFDF** — Import and export form field data using the Forms Data
//!    Format (binary) and XML Forms Data Format.
//! 2. **AcroForm XML** — Lightweight XML representation of form field data
//!    plus XDP (XML Data Package) generation for XFA payloads.
//! 3. **ZUGFeRD / Factur-X** — Generate and parse CII (Cross-Industry Invoice)
//!    XML conforming to ZUGFeRD 2.3 / Factur-X 1.0, and embed it into PDF/A-3
//!    documents as required by EU e-invoicing regulations.

pub mod embed;
pub mod error;
pub mod fdf;
pub mod xfdf;
pub mod xml_form;
pub mod zugferd;

pub use error::{InvoiceError, Result};
