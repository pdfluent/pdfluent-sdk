//! Document metadata (Info dict + XMP).
//!
//! Read-side combines [`pdf_engine::DocumentInfo`] (title/author/subject/
//! keywords/creator/producer) with the Info-dict dates extracted directly
//! from [`lopdf::Document`]. XMP-stream metadata beyond these core fields
//! is tracked for a post-1.0 extension.

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
    /// Producer string (the software that produced the PDF).
    pub producer: Option<String>,
    /// Creator string (the authoring application).
    pub creator: Option<String>,
    /// Creation date as stored in the Info dict (`/CreationDate`). Typically
    /// PDF D-format: `D:YYYYMMDDHHmmSSOHH'mm'`.
    pub creation_date: Option<String>,
    /// Last modification date (`/ModDate`).
    pub modification_date: Option<String>,
}

/// Pending changes buffered by [`MetadataMut`].
#[derive(Default)]
struct PendingChanges {
    // `Option<Option<String>>` lets us distinguish "not set (no pending
    // change)" from "explicitly cleared" (Some(None))) — though for 1.0
    // we only expose setters, not clearers, so Some(Some) is used.
    set_title: Option<String>,
    set_author: Option<String>,
    set_subject: Option<String>,
    set_keywords: Option<Vec<String>>,
}

/// Mutable metadata handle.
///
/// Obtained via [`crate::PdfDocument::metadata_mut`]. Returned
/// unconditionally; a document always has a metadata dictionary (created
/// lazily in [`MetadataMut::commit`] if absent). Changes are flushed on
/// [`commit`](MetadataMut::commit) or when the handle is dropped.
///
/// This is the pending-changes pattern: setters buffer edits on the handle,
/// and `commit()` is the explicit flush that surfaces write errors.
pub struct MetadataMut<'a> {
    doc: &'a mut crate::PdfDocument,
    pending: PendingChanges,
}

impl<'a> MetadataMut<'a> {
    /// Internal constructor — called from `PdfDocument::metadata_mut`.
    pub(crate) fn new(doc: &'a mut crate::PdfDocument) -> Self {
        Self {
            doc,
            pending: PendingChanges::default(),
        }
    }

    /// Set document title.
    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.pending.set_title = Some(title.into());
        self
    }

    /// Set document author.
    pub fn set_author(&mut self, author: impl Into<String>) -> &mut Self {
        self.pending.set_author = Some(author.into());
        self
    }

    /// Set document subject.
    pub fn set_subject(&mut self, subject: impl Into<String>) -> &mut Self {
        self.pending.set_subject = Some(subject.into());
        self
    }

    /// Set document keywords.
    ///
    /// The PDF Info dict stores keywords as a single comma-separated string;
    /// this method accepts a slice and joins with ", " when writing.
    pub fn set_keywords(&mut self, keywords: &[&str]) -> &mut Self {
        self.pending.set_keywords = Some(keywords.iter().map(|k| (*k).to_owned()).collect());
        self
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
        let pending = std::mem::take(&mut self.pending);
        flush_pending_to_lopdf(self.doc.lopdf_mut(), &pending)
    }
}

impl Drop for MetadataMut<'_> {
    fn drop(&mut self) {
        // Flush any pending changes that commit() wasn't called for.
        // Errors during drop are intentionally swallowed (documented
        // behaviour); users who care call commit() explicitly.
        let pending = std::mem::take(&mut self.pending);
        if pending.has_any() {
            let _ = flush_pending_to_lopdf(self.doc.lopdf_mut(), &pending);
        }
    }
}

impl PendingChanges {
    fn has_any(&self) -> bool {
        self.set_title.is_some()
            || self.set_author.is_some()
            || self.set_subject.is_some()
            || self.set_keywords.is_some()
    }
}

/// Write the buffered metadata changes into the lopdf `/Info` dictionary,
/// creating it if needed.
fn flush_pending_to_lopdf(doc: &mut lopdf::Document, pending: &PendingChanges) -> Result<()> {
    use lopdf::{Dictionary, Object};

    if !pending.has_any() {
        return Ok(());
    }

    // Locate-or-create the Info dict.
    let info_id = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => *id,
        _ => {
            // No Info dict yet — create an empty one and link via trailer.
            let id = doc.add_object(Object::Dictionary(Dictionary::new()));
            doc.trailer.set("Info", Object::Reference(id));
            id
        }
    };

    let info_obj = doc.objects.get_mut(&info_id).ok_or_else(|| {
        crate::error::internal_error(format!(
            "Info reference {info_id:?} in trailer does not resolve",
        ))
    })?;

    let info_dict = info_obj.as_dict_mut().map_err(|e| {
        crate::error::internal_error(format!("Info object is not a dictionary: {e:?}"))
    })?;

    if let Some(title) = &pending.set_title {
        info_dict.set("Title", Object::string_literal(title.as_str()));
    }
    if let Some(author) = &pending.set_author {
        info_dict.set("Author", Object::string_literal(author.as_str()));
    }
    if let Some(subject) = &pending.set_subject {
        info_dict.set("Subject", Object::string_literal(subject.as_str()));
    }
    if let Some(keywords) = &pending.set_keywords {
        let joined = keywords.join(", ");
        info_dict.set("Keywords", Object::string_literal(joined.as_str()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Read helpers (pub(crate) — called from PdfDocument::metadata())
// ---------------------------------------------------------------------------

/// Read the Info-dict date strings that `pdf_engine::DocumentInfo` does not
/// expose.
pub(crate) fn read_info_dates(doc: &lopdf::Document) -> (Option<String>, Option<String>) {
    fn read_string(doc: &lopdf::Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
        let obj = dict.get(key).ok()?;
        let resolved = match obj {
            lopdf::Object::Reference(id) => doc.get_object(*id).ok()?,
            other => other,
        };
        lopdf::decode_text_string(resolved).ok()
    }

    let info_dict = match doc.trailer.get(b"Info") {
        Ok(lopdf::Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return (None, None),
        },
        Ok(lopdf::Object::Dictionary(d)) => d,
        _ => return (None, None),
    };

    let creation = read_string(doc, info_dict, b"CreationDate");
    let modification = read_string(doc, info_dict, b"ModDate");
    (creation, modification)
}

/// Parse the comma-separated `/Keywords` string into a vector.
pub(crate) fn parse_keywords(raw: Option<String>) -> Vec<String> {
    match raw {
        None => Vec::new(),
        Some(s) => s
            .split(',')
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use lopdf::Object;

    fn attach_invalid_info_object(doc: &mut crate::PdfDocument) {
        let info_id = doc.lopdf_mut().add_object(Object::Integer(7));
        doc.lopdf_mut()
            .trailer
            .set("Info", Object::Reference(info_id));
    }

    #[test]
    fn commit_surfaces_flush_errors_that_drop_would_swallow() {
        let mut explicit = crate::PdfDocument::create();
        attach_invalid_info_object(&mut explicit);

        let err = explicit
            .metadata_mut()
            .set_title("Broken title")
            .commit()
            .unwrap_err();
        assert!(
            matches!(err, crate::Error::Internal { .. }),
            "expected explicit commit to surface the flush failure, got {err:?}",
        );

        let mut best_effort = crate::PdfDocument::create();
        attach_invalid_info_object(&mut best_effort);
        {
            let mut metadata = best_effort.metadata_mut();
            metadata.set_title("Dropped title");
        }

        let bytes = best_effort
            .to_bytes()
            .expect("drop path should swallow metadata flush errors");
        let reopened = crate::PdfDocument::from_bytes(&bytes).expect("reparse after drop");
        assert_eq!(
            reopened.metadata().title.as_deref(),
            None,
            "drop auto-commit should remain best-effort when flushing metadata fails",
        );
    }
}
