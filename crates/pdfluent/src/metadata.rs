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
/// Obtained via [`crate::PdfDocument::metadata_mut`]. Changes are applied on
/// [`MetadataMut::commit`] or on drop.
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

    /// Apply the pending changes to the document.
    pub fn commit(self) -> Result<()> {
        unimplemented!("Epic 2 #1245");
    }
}
