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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod embed;
pub mod error;
pub mod fdf;
pub mod iso_codes;
pub mod validation;
pub mod xfdf;
pub mod xml_form;
pub mod zugferd;

pub use error::{InvoiceError, Result};
pub use validation::{
    validate_en16931, validate_invoice, En16931ValidationResult, Severity, ValidationIssue,
    ValidationReport,
};
