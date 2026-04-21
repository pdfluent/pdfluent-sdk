//! [`PdfMerger`] — factory builder for merging multiple PDF documents.

use crate::capability::Capability;
use crate::document::PdfDocument;
use crate::error::{internal_error, Error, Result};
use crate::license;

/// Strategy for combining bookmarks when merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BookmarkMergeStrategy {
    /// Concatenate each source document's bookmarks under a top-level entry.
    /// **Default** — matches the most common expectation.
    #[default]
    Concat,
    /// Flatten all bookmarks into a single top-level sequence.
    FlattenAll,
    /// Discard all bookmarks.
    Discard,
}

/// Options for the [`PdfMerger`] build step.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct MergeOptions {
    pub(crate) bookmarks: BookmarkMergeStrategy,
    pub(crate) page_labels: bool,
}

/// Combine multiple PDF documents into a new document.
///
/// # Example
///
/// ```no_run
/// use pdfluent::prelude::*;
///
/// # fn run() -> Result<()> {
/// let merged = PdfMerger::new()
///     .add(PdfDocument::open("cover.pdf")?)
///     .add(PdfDocument::open("body.pdf")?)
///     .with_bookmarks(BookmarkMergeStrategy::Concat)
///     .build()?;
/// merged.save("combined.pdf")?;
/// # Ok(()) }
/// ```
#[derive(Debug, Default)]
pub struct PdfMerger {
    inputs: Vec<PdfDocument>,
    opts: MergeOptions,
}

impl PdfMerger {
    /// Create a new empty merger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document to merge.
    ///
    /// `add(doc)` is intentionally the builder-style name and is **not**
    /// meant as an implementation of [`std::ops::Add`]. `PdfMerger` is a
    /// one-way accumulator; the signature `(self, doc) -> Self` matches
    /// other fluent builders throughout the crate (`SignOptions::reason`
    /// etc.). See RFC 0001 §3.3.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, doc: PdfDocument) -> Self {
        self.inputs.push(doc);
        self
    }

    /// Choose how to combine bookmarks. Default is
    /// [`BookmarkMergeStrategy::Concat`].
    pub fn with_bookmarks(mut self, strategy: BookmarkMergeStrategy) -> Self {
        self.opts.bookmarks = strategy;
        self
    }

    /// Preserve page-label sequences from the input documents.
    pub fn with_page_labels(mut self, v: bool) -> Self {
        self.opts.page_labels = v;
        self
    }

    /// Build the merged document.
    ///
    /// # 1.0 behaviour
    ///
    /// - Page trees are concatenated in `add()` order via
    ///   `pdf_manip::pages::merge_documents`.
    /// - [`BookmarkMergeStrategy::Concat`] (the default) is the only
    ///   strategy that receives dedicated treatment in 1.0. `FlattenAll`
    ///   and `Discard` are accepted and fall back to the Concat-like
    ///   behaviour provided by the underlying merger, with bookmarks
    ///   treated on a best-effort basis. RFC §14 v1.5 documents this
    ///   truth-gap; full strategy support lands in 1.1.
    /// - `with_page_labels(true)` is accepted but a no-op in 1.0.
    ///
    /// # Errors
    ///
    /// - [`Error::Internal`] if called with zero inputs.
    /// - [`Error::InvalidPdf`] wrapping the underlying merge error.
    pub fn build(self) -> Result<PdfDocument> {
        license::require_capability(Capability::PageOps)?;
        if self.inputs.is_empty() {
            return Err(internal_error(
                "PdfMerger::build() called with no inputs; add at least one PdfDocument first",
            ));
        }
        // `pdf_manip::pages::merge_documents` takes `&[lopdf::Document]`.
        // Extract references to each input's lopdf representation.
        let lopdf_docs: Vec<lopdf::Document> =
            self.inputs.iter().map(|d| d.lopdf().clone()).collect();
        let merged =
            pdf_manip::pages::merge_documents(&lopdf_docs).map_err(|e| Error::InvalidPdf {
                byte_offset: None,
                reason: e.to_string(),
            })?;
        PdfDocument::from_lopdf(merged)
    }
}
