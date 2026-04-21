//! Document metadata (Info dict + XMP).

use crate::error::Result;

/// Read-only document metadata.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Metadata {
    /// Document title.
    pub title: Option<String>,
    /// Document author.
    pub author: Option<String>,
    /// Document subject.
    pub subject: Option<String>,
    /// Document keywords.
    pub keywords: Vec<String>,
    /// Producer string.
    pub producer: Option<String>,
    /// Creator string.
    pub creator: Option<String>,
    /// Creation date (ISO 8601).
    pub creation_date: Option<String>,
    /// Last modification date (ISO 8601).
    pub modification_date: Option<String>,
}

/// Mutable metadata handle.
///
/// Obtained via [`crate::PdfDocument::metadata_mut`]. Returned
/// unconditionally; a document always has a metadata dictionary (created
/// lazily if absent). Changes are flushed on [`commit`](MetadataMut::commit)
/// or when the handle is dropped.
pub struct MetadataMut<'a> {
    _doc: std::marker::PhantomData<&'a mut crate::PdfDocument>,
}

impl<'a> MetadataMut<'a> {
    /// Set document title.
    pub fn set_title(&mut self, _title: impl Into<String>) -> &mut Self {
        unimplemented!("Epic 2 #1245");
    }

    /// Set document author.
    pub fn set_author(&mut self, _author: impl Into<String>) -> &mut Self {
        unimplemented!("Epic 2 #1245");
    }

    /// Set document subject.
    pub fn set_subject(&mut self, _subject: impl Into<String>) -> &mut Self {
        unimplemented!("Epic 2 #1245");
    }

    /// Set document keywords.
    pub fn set_keywords(&mut self, _keywords: &[&str]) -> &mut Self {
        unimplemented!("Epic 2 #1245");
    }

    /// Apply pending changes to the document.
    ///
    /// Takes `&mut self` (not `self`) so `commit` can be used at the end of
    /// a setter-chain without moving out of a `&mut` reference. The handle
    /// remains valid after `commit` and may be reused for additional
    /// mutations. Auto-commits on drop; calling `commit` explicitly surfaces
    /// errors that would otherwise be silently swallowed.
    ///
    /// ```no_run
    /// # use pdfluent::prelude::*;
    /// # fn run(mut doc: PdfDocument) -> Result<()> {
    /// doc.metadata_mut()
    ///     .set_title("Invoice")
    ///     .set_author("Acme")
    ///     .commit()?;
    /// # Ok(()) }
    /// ```
    pub fn commit(&mut self) -> Result<()> {
        unimplemented!("Epic 2 #1245");
    }
}
