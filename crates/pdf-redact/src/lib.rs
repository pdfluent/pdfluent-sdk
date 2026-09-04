#![warn(missing_docs)]
//! GDPR-compliant PDF redaction: permanent content removal.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
pub mod redact;
pub mod search_redact;
mod tounicode;

pub use error::{RedactError, Result};
pub use redact::{RedactionArea, RedactionReport, Redactor};
pub use search_redact::{search_and_redact, RedactSearchOptions, SearchRedactReport};
