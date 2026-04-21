//! [`PdfMerger`] — factory builder for merging multiple PDF documents.

use crate::document::PdfDocument;
use crate::error::Result;

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
    pub fn build(self) -> Result<PdfDocument> {
        unimplemented!("Epic 2 #1243 wires this against pdf_manip::pages::merge_docs");
    }
}
