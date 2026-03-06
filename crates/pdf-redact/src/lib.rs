//! GDPR-compliant PDF redaction: permanent content removal.

pub mod error;
pub mod redact;

pub use error::{RedactError, Result};
pub use redact::{RedactionArea, RedactionReport, Redactor};
