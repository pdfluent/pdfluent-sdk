#![warn(missing_docs)]
//! GDPR-compliant PDF redaction: permanent content removal.

pub mod error;
pub mod redact;
pub mod search_redact;
mod tounicode;

pub use error::{RedactError, Result};
pub use redact::{RedactionArea, RedactionReport, Redactor};
pub use search_redact::{search_and_redact, RedactSearchOptions, SearchRedactReport};
