//! [`PdfMerger`] — factory builder for merging multiple PDF documents.

use crate::document::PdfDocument;
use crate::error::Result;

/// Strategy for combining bookmarks when merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BookmarkMergeStrategy {
    /// Concatenate each source document's bookmarks under a top-level entry.
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
    pub(crate) bookmarks: Option<BookmarkMergeStrategy>,
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

    /// Add a document from raw bytes.
    pub fn add_bytes(self, _bytes: &[u8]) -> Result<Self> {
        unimplemented!("Epic 2 #1243 wires this against PdfDocument::from_bytes");
    }

    /// Choose how to combine bookmarks.
    pub fn with_bookmarks(mut self, strategy: BookmarkMergeStrategy) -> Self {
        self.opts.bookmarks = Some(strategy);
        self
    }

    /// Preserve page-label sequences from the input documents.
    pub fn with_page_labels(mut self, v: bool) -> Self {
        self.opts.page_labels = v;
        self
    }

    /// Build the merged document.
    ///
    /// # Errors
    ///
    /// - [`crate::Error::InvalidPdf`] if any input is unreadable after merge setup.
    /// - [`crate::Error::Internal`] on unexpected failures (please report).
    pub fn build(self) -> Result<PdfDocument> {
        unimplemented!("Epic 2 #1243 wires this against pdf_manip::pages::merge_docs");
    }
}
