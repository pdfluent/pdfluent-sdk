//! Redaction options.

/// Options for redaction operations.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RedactOptions {
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
}

impl RedactOptions {
    /// New default options (case-insensitive, literal matching).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable case-sensitive matching.
    pub fn case_sensitive(mut self, v: bool) -> Self {
        self.case_sensitive = v;
        self
    }

    /// Treat the target as a regex pattern.
    pub fn regex(mut self, v: bool) -> Self {
        self.regex = v;
        self
    }
}
