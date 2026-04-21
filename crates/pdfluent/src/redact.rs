//! Redaction options.

/// Options for redaction operations.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RedactOptions {
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
    pub(crate) on_pages: Option<Vec<usize>>,
}

impl RedactOptions {
    /// New default options (case-insensitive, literal matching, all pages).
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

    /// Limit redaction to the specified pages (1-based). Without this
    /// option, all pages are scanned.
    pub fn on_pages(mut self, pages: &[usize]) -> Self {
        self.on_pages = Some(pages.to_vec());
        self
    }
}
