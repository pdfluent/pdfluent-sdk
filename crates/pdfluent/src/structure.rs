//! Document-structure read APIs: outlines, annotations, attachments.
//!
//! These close the three items the prior core-PDF closure left as
//! out-of-scope, per `PHASE1_PRODUCT_SCOPE_DECISIONS.md`:
//!
//! - **Outlines / bookmarks**: read + write (`PdfDocument::outlines` /
//!   `set_outlines`), backed by `pdf_manip::bookmarks`.
//! - **Annotations**: read/list (`PdfDocument::annotations`). Write/CRUD
//!   on the Rust facade is intentionally **not exposed for v1** — the v1
//!   annotation-authoring surface is the WASM binding / `pdf-annot`
//!   builder.
//! - **Attachments / embedded files**: read/list + extract
//!   (`PdfDocument::attachments` / `attachment_bytes`). Write (adding
//!   embedded files) is intentionally **not exposed for v1**.
//!
//! All reads operate on the in-memory `lopdf::Document` already held by
//! `PdfDocument`; no new crate dependency is introduced.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Document, Object, ObjectId};

// ---------------------------------------------------------------------------
// Outlines
// ---------------------------------------------------------------------------

/// A document outline (bookmark) entry.
///
/// This is an input+output type (constructed by callers of
/// [`PdfDocument::set_outlines`](crate::PdfDocument::set_outlines)), so
/// it is intentionally exhaustive.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// Display title.
    pub title: String,
    /// 0-based target page index, when the action is an in-document GoTo.
    /// `None` for URI / named / external-GoToR actions.
    pub page: Option<usize>,
    /// Nested child outlines.
    pub children: Vec<Outline>,
}

impl Outline {
    /// Construct a leaf outline targeting a 0-based page.
    pub fn new(title: impl Into<String>, page: usize) -> Self {
        Self {
            title: title.into(),
            page: Some(page),
            children: Vec::new(),
        }
    }
}

pub(crate) fn from_bookmark(b: &pdf_manip::bookmarks::Bookmark) -> Outline {
    use pdf_manip::bookmarks::BookmarkAction;
    let page = match &b.action {
        // pdf-manip pages are 1-based; the facade is 0-based.
        BookmarkAction::GoTo { page, .. } => Some((*page).saturating_sub(1) as usize),
        _ => None,
    };
    Outline {
        title: b.title.clone(),
        page,
        children: b.children.iter().map(from_bookmark).collect(),
    }
}

pub(crate) fn to_bookmark(o: &Outline) -> pdf_manip::bookmarks::Bookmark {
    // 0-based facade page -> 1-based pdf-manip page.
    let page = o.page.map(|p| p as u32 + 1).unwrap_or(1);
    pdf_manip::bookmarks::Bookmark::with_children(
        o.title.clone(),
        page,
        o.children.iter().map(to_bookmark).collect(),
    )
}

// ---------------------------------------------------------------------------
// Annotations (read/list)
// ---------------------------------------------------------------------------

/// Read-only view of a single page annotation.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AnnotationInfo {
    /// `/Subtype` (e.g. `"Highlight"`, `"Text"`, `"Link"`, `"FreeText"`).
    pub subtype: String,
    /// `/Rect` as `[x0, y0, x1, y1]` in PDF points, when present.
    pub rect: Option<[f64; 4]>,
    /// `/Contents` text, when present.
    pub contents: Option<String>,
}

fn obj_as_f64(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn pdf_string(o: &Object) -> Option<String> {
    o.as_str()
        .ok()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// List annotations on a 0-based page index from the loaded document.
pub(crate) fn read_annotations(doc: &Document, page_index: usize) -> Vec<AnnotationInfo> {
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let Some(&page_id) = page_ids.get(page_index) else {
        return Vec::new();
    };
    let Ok(page_dict) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let annots = match page_dict.get_deref(b"Annots", doc) {
        Ok(Object::Array(a)) => a.clone(),
        _ => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in &annots {
        // Each entry is usually a reference to an annotation dict.
        let dict = match entry {
            Object::Reference(id) => match doc.get_dictionary(*id) {
                Ok(d) => d,
                Err(_) => continue,
            },
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_else(|| "Unknown".to_string());
        let rect = dict
            .get(b"Rect")
            .ok()
            .and_then(|o| o.as_array().ok())
            .and_then(|a| {
                if a.len() == 4 {
                    Some([
                        obj_as_f64(&a[0])?,
                        obj_as_f64(&a[1])?,
                        obj_as_f64(&a[2])?,
                        obj_as_f64(&a[3])?,
                    ])
                } else {
                    None
                }
            });
        let contents = dict.get(b"Contents").ok().and_then(pdf_string);
        out.push(AnnotationInfo {
            subtype,
            rect,
            contents,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Attachments / embedded files (read/list + extract)
// ---------------------------------------------------------------------------

/// Read-only view of an embedded-file attachment.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Attachment {
    /// Attachment name (the name-tree key).
    pub name: String,
    /// Uncompressed byte length of the embedded file stream (best effort;
    /// falls back to `/Params /Size` then compressed length).
    pub size: usize,
}

fn catalog_names_dict(doc: &Document) -> Option<lopdf::Dictionary> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(root).ok()?;
    match catalog.get_deref(b"Names", doc).ok()? {
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    }
}

/// Walk a name tree node, collecting (name, value-object-id) leaf pairs.
fn walk_name_tree(
    doc: &Document,
    node: &lopdf::Dictionary,
    out: &mut Vec<(String, ObjectId)>,
    depth: usize,
) {
    if depth > 64 {
        return; // cycle / pathological depth guard
    }
    // Leaf: /Names = [name1, filespec1, name2, filespec2, ...]
    if let Ok(Object::Array(names)) = node.get_deref(b"Names", doc) {
        let mut i = 0;
        while i + 1 < names.len() {
            if let (Ok(name), Object::Reference(id)) = (names[i].as_str(), &names[i + 1]) {
                out.push((String::from_utf8_lossy(name).into_owned(), *id));
            } else if let Ok(name) = names[i].as_str() {
                // inline filespec (rare) — skip extraction, still list by name
                let _ = name;
            }
            i += 2;
        }
    }
    // Intermediate: /Kids = [node, node, ...]
    if let Ok(Object::Array(kids)) = node.get_deref(b"Kids", doc) {
        for kid in kids {
            if let Object::Reference(id) = kid {
                if let Ok(d) = doc.get_dictionary(*id) {
                    walk_name_tree(doc, d, out, depth + 1);
                }
            }
        }
    }
}

fn embedded_filespecs(doc: &Document) -> Vec<(String, ObjectId)> {
    let mut out = Vec::new();
    let Some(names) = catalog_names_dict(doc) else {
        return out;
    };
    if let Ok(Object::Dictionary(node)) = names.get_deref(b"EmbeddedFiles", doc) {
        walk_name_tree(doc, node, &mut out, 0);
    }
    out
}

/// Extract the embedded-file stream bytes for a filespec object.
fn filespec_bytes(doc: &Document, filespec_id: ObjectId) -> Option<(usize, Option<Vec<u8>>)> {
    let filespec = doc.get_dictionary(filespec_id).ok()?;
    let ef = match filespec.get_deref(b"EF", doc).ok()? {
        Object::Dictionary(d) => d.clone(),
        _ => return None,
    };
    // Prefer /F (most common), fall back to /UF.
    let stream_id = ["F", "UF"]
        .iter()
        .find_map(|k| match ef.get(k.as_bytes()) {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        })?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    // `get_plain_content` returns the raw bytes when the stream has no
    // filter and the decoded bytes otherwise — robust for both
    // uncompressed and FlateDecode'd embedded files. Size is reported
    // as the length of exactly the bytes `attachment_bytes` returns.
    let bytes = stream
        .get_plain_content()
        .ok()
        .unwrap_or_else(|| stream.content.clone());
    Some((bytes.len(), Some(bytes)))
}

/// List embedded-file attachments (name + size).
pub(crate) fn read_attachments(doc: &Document) -> Vec<Attachment> {
    embedded_filespecs(doc)
        .into_iter()
        .map(|(name, id)| {
            let size = filespec_bytes(doc, id).map(|(s, _)| s).unwrap_or(0);
            Attachment { name, size }
        })
        .collect()
}

/// Extract the bytes of the embedded file with the given name.
pub(crate) fn read_attachment_bytes(doc: &Document, name: &str) -> Option<Vec<u8>> {
    embedded_filespecs(doc)
        .into_iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, id)| filespec_bytes(doc, id))
        .and_then(|(_, bytes)| bytes)
}
