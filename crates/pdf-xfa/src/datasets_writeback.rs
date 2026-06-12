//! Faithful XFA packet writeback.
//!
//! Strategy (Phase 1 SDK foundation):
//!
//! 1. **Surgical splice** — resolve each changed data node in the *original*
//!    datasets packet text via roxmltree byte ranges and replace only the
//!    element's text content (or insert a new leaf element). Everything else
//!    — attributes, namespaces, comments, the `dataDescription` sub-packet,
//!    whitespace — is preserved byte-for-byte.
//! 2. **Fallback regeneration** — when a splice target cannot be resolved,
//!    regenerate only the `<xfa:data>` section from the [`DataDom`] while
//!    keeping the original `<xfa:datasets>` wrapper (and its other children,
//!    e.g. `dataDescription`) intact.
//!
//! The PDF-level swap supports both `/XFA` layouts: the array of
//! `(name, stream)` pairs and the single consolidated XDP stream.

use lopdf::{Document, Object, ObjectId};
use xfa_dom_resolver::data_dom::DataDom;

use crate::error::{Result, XfaError};

/// One datasets mutation, expressed as a path of
/// `(element-local-name, index-among-same-name-element-siblings)` segments
/// starting at the data root (the element the [`DataDom`] unwrapped to,
/// usually `<xfa:data>`).
#[derive(Debug, Clone)]
pub(crate) enum DatasetsEdit {
    /// Replace the text content of an existing leaf element.
    SetValue {
        /// Path from the data root to the leaf (root segment included).
        path: Vec<(String, usize)>,
        /// New (unescaped) text value.
        value: String,
    },
    /// Insert a new leaf element under an existing group element.
    CreateValue {
        /// Path from the data root to the parent group (root included).
        parent_path: Vec<(String, usize)>,
        /// New element's local name.
        name: String,
        /// New (unescaped) text value.
        value: String,
    },
}

/// One saved-form-packet value sync: the form-tree path of named segments
/// (root subform first) and the new value.
#[derive(Debug, Clone)]
pub(crate) struct FormPacketEdit {
    /// `(name, index-among-same-name-siblings)` from the root subform down
    /// to the field.
    pub segments: Vec<(String, usize)>,
    /// New (unescaped) value.
    pub value: String,
    /// When `false`, only update an existing `<value>` element and skip
    /// nodes that have none (used to clear deselected radio members and to
    /// refresh group-level values without changing the producer's shape).
    pub insert_if_missing: bool,
}

/// Escape text content for XML.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// A pending byte-range replacement on the original text.
struct Patch {
    start: usize,
    end: usize,
    replacement: String,
}

fn apply_patches(original: &str, mut patches: Vec<Patch>) -> Option<String> {
    patches.sort_by(|a, b| b.start.cmp(&a.start));
    // Reject overlaps: after sorting descending by start, every patch must
    // end at or before the previous (higher) patch's start.
    for w in patches.windows(2) {
        if w[1].end > w[0].start {
            return None;
        }
    }
    let mut out = original.to_string();
    for p in patches {
        if p.start > p.end || p.end > out.len() {
            return None;
        }
        out.replace_range(p.start..p.end, &p.replacement);
    }
    Some(out)
}

/// Find the end of the open tag (`>`) within `slice`, skipping quoted
/// attribute values. Returns `(offset_of_gt, self_closing)`.
fn open_tag_end(slice: &str) -> Option<(usize, bool)> {
    let bytes = slice.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => {
                    let self_closing = i > 0 && bytes[i - 1] == b'/';
                    return Some((i, self_closing));
                }
                _ => {}
            },
        }
    }
    None
}

/// Extract the qualified tag name as written in the source open tag.
fn source_tag_name(slice: &str) -> Option<&str> {
    let rest = slice.strip_prefix('<')?;
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Compute a patch that sets the text content of a leaf element.
fn leaf_set_text_patch(xml: &str, el: roxmltree::Node<'_, '_>, value: &str) -> Option<Patch> {
    if el.children().any(|c| c.is_element()) {
        return None; // not a leaf in the source — bail to fallback
    }
    let range = el.range();
    let slice = &xml[range.clone()];
    let (gt, self_closing) = open_tag_end(slice)?;
    let escaped = escape_xml(value);
    if self_closing {
        let qname = source_tag_name(slice)?.to_string();
        let head = slice[..gt].trim_end_matches('/').trim_end();
        Some(Patch {
            start: range.start,
            end: range.end,
            replacement: format!("{head}>{escaped}</{qname}>"),
        })
    } else {
        let close = slice.rfind("</")?;
        if close < gt + 1 {
            return None;
        }
        Some(Patch {
            start: range.start + gt + 1,
            end: range.start + close,
            replacement: escaped,
        })
    }
}

/// Compute a patch that inserts a new leaf child element just before the
/// parent's closing tag (rebuilding self-closing parents).
fn insert_child_patch(
    xml: &str,
    parent: roxmltree::Node<'_, '_>,
    name: &str,
    value: &str,
) -> Option<Patch> {
    let range = parent.range();
    let slice = &xml[range.clone()];
    let (gt, self_closing) = open_tag_end(slice)?;
    let escaped = escape_xml(value);
    let child = format!("<{name}>{escaped}</{name}>");
    if self_closing {
        let qname = source_tag_name(slice)?.to_string();
        let head = slice[..gt].trim_end_matches('/').trim_end();
        Some(Patch {
            start: range.start,
            end: range.end,
            replacement: format!("{head}>{child}</{qname}>"),
        })
    } else {
        let close = slice.rfind("</")?;
        if close < gt + 1 {
            return None;
        }
        Some(Patch {
            start: range.start + close,
            end: range.start + close,
            replacement: child,
        })
    }
}

/// Mirror of `DataDom::unwrap_datasets_root` on the parsed XML: descend
/// through `datasets`/`data` wrappers to the element the data paths are
/// rooted at.
fn data_root<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
) -> Option<roxmltree::Node<'a, 'input>> {
    let mut current = doc.root().children().find(|c| c.is_element())?;
    loop {
        let name = current.tag_name().name();
        if name == "datasets" || name == "data" {
            let data_child = current
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "data");
            if let Some(child) = data_child {
                current = child;
                continue;
            }
        }
        break;
    }
    Some(current)
}

/// Resolve a `(name, index)` path (root segment first) against the XML.
fn resolve_path<'a, 'input>(
    root: roxmltree::Node<'a, 'input>,
    path: &[(String, usize)],
) -> Option<roxmltree::Node<'a, 'input>> {
    let (first, rest) = path.split_first()?;
    if root.tag_name().name() != first.0 {
        return None;
    }
    let mut current = root;
    for (name, index) in rest {
        let mut k = 0usize;
        let mut next = None;
        for c in current.children() {
            if !c.is_element() || c.tag_name().name() != *name {
                continue;
            }
            if k == *index {
                next = Some(c);
                break;
            }
            k += 1;
        }
        current = next?;
    }
    Some(current)
}

/// Surgically apply the edits to the original datasets packet text.
/// Returns `None` when any edit cannot be resolved — the caller falls back
/// to [`regenerate_datasets`].
pub(crate) fn splice_datasets(original: &str, edits: &[DatasetsEdit]) -> Option<String> {
    if edits.is_empty() {
        return Some(original.to_string());
    }
    let doc = roxmltree::Document::parse(original).ok()?;
    let root = data_root(&doc)?;

    let mut patches = Vec::new();
    for edit in edits {
        match edit {
            DatasetsEdit::SetValue { path, value } => {
                let el = resolve_path(root, path)?;
                patches.push(leaf_set_text_patch(original, el, value)?);
            }
            DatasetsEdit::CreateValue {
                parent_path,
                name,
                value,
            } => {
                let parent = resolve_path(root, parent_path)?;
                patches.push(insert_child_patch(original, parent, name, value)?);
            }
        }
    }
    apply_patches(original, patches)
}

/// Strip the outer element from a serialized XML fragment, returning the
/// inner content (`<data>inner</data>` → `inner`).
fn strip_outer_element(xml: &str) -> &str {
    let Some((gt, self_closing)) = open_tag_end(xml) else {
        return xml;
    };
    if self_closing {
        return "";
    }
    let Some(close) = xml.rfind("</") else {
        return xml;
    };
    if close <= gt {
        return xml;
    }
    &xml[gt + 1..close]
}

/// Fallback: regenerate the `<xfa:data>` section from the [`DataDom`],
/// preserving the original `<xfa:datasets>` wrapper and its other children
/// when the original packet is parseable; otherwise produce a fresh
/// canonical packet.
pub(crate) fn regenerate_datasets(original: &str, data_dom: &DataDom) -> String {
    // `DataDom::to_xml` serializes the effective root element (usually
    // `<data>`); the inner content is what belongs inside `<xfa:data>`.
    let serialized = data_dom.to_xml();
    let root_is_data = data_dom
        .root()
        .and_then(|r| data_dom.get(r))
        .map(|n| n.name() == "data")
        .unwrap_or(false);
    let inner = if root_is_data {
        strip_outer_element(&serialized).to_string()
    } else {
        serialized
    };

    if !original.is_empty() {
        if let Ok(doc) = roxmltree::Document::parse(original) {
            if let Some(datasets_el) = doc.root().children().find(|c| c.is_element()) {
                if datasets_el.tag_name().name() == "datasets" {
                    if let Some(data_el) = datasets_el
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "data")
                    {
                        let range = data_el.range();
                        let slice = &original[range.clone()];
                        if let Some((gt, self_closing)) = open_tag_end(slice) {
                            let mut out = original.to_string();
                            if self_closing {
                                let head = slice[..gt].trim_end_matches('/').trim_end().to_string();
                                let qname =
                                    source_tag_name(slice).unwrap_or("xfa:data").to_string();
                                out.replace_range(range, &format!("{head}>{inner}</{qname}>"));
                            } else if let Some(close) = slice.rfind("</") {
                                out.replace_range(
                                    range.start + gt + 1..range.start + close,
                                    &inner,
                                );
                            }
                            return out;
                        }
                    }
                }
            }
        }
    }

    format!(
        "<xfa:datasets xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\"><xfa:data>{inner}</xfa:data></xfa:datasets>"
    )
}

/// Apply saved-form-packet value syncs. Returns the (possibly modified)
/// packet text and the number of edits that resolved.
///
/// Resolution walks `subform`/`field`/`exclGroup` elements by `name`
/// attribute, mirroring the form-tree path. Unresolvable edits are skipped —
/// Adobe re-merges those values from datasets on open.
pub(crate) fn splice_form_packet(original: &str, edits: &[FormPacketEdit]) -> (String, usize) {
    let Ok(doc) = roxmltree::Document::parse(original) else {
        return (original.to_string(), 0);
    };
    let Some(form_root) = doc.root().children().find(|c| c.is_element()) else {
        return (original.to_string(), 0);
    };

    fn named_children<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
    ) -> Vec<roxmltree::Node<'a, 'input>> {
        node.children()
            .filter(|c| {
                c.is_element() && matches!(c.tag_name().name(), "subform" | "field" | "exclGroup")
            })
            .collect()
    }

    fn resolve_segments<'a, 'input>(
        from: roxmltree::Node<'a, 'input>,
        segments: &[(String, usize)],
    ) -> Option<roxmltree::Node<'a, 'input>> {
        let mut current = from;
        for (name, index) in segments {
            let mut k = 0usize;
            let mut next = None;
            for c in named_children(current) {
                if c.attribute("name").unwrap_or("") != name {
                    continue;
                }
                if k == *index {
                    next = Some(c);
                    break;
                }
                k += 1;
            }
            current = next?;
        }
        Some(current)
    }

    let mut patches = Vec::new();
    let mut resolved = 0usize;
    for edit in edits {
        let Some(field_el) = resolve_segments(form_root, &edit.segments) else {
            continue;
        };
        let value_el = field_el
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "value");
        let patch = match value_el {
            Some(v) => {
                // Replace the inner element's text (<value><text>…</text></value>);
                // when <value> is empty, insert a <text> child.
                match v.children().find(|c| c.is_element()) {
                    Some(inner) => leaf_set_text_patch(original, inner, &edit.value),
                    None if edit.value.is_empty() => None, // already empty
                    None => insert_child_patch(original, v, "text", &edit.value),
                }
            }
            None if !edit.insert_if_missing => None,
            None => {
                let child = format!("<value><text>{}</text></value>", escape_xml(&edit.value));
                // Insert a whole <value> block into the field element.
                let range = field_el.range();
                let slice = &original[range.clone()];
                open_tag_end(slice).and_then(|(gt, self_closing)| {
                    if self_closing {
                        let qname = source_tag_name(slice)?.to_string();
                        let head = slice[..gt].trim_end_matches('/').trim_end();
                        Some(Patch {
                            start: range.start,
                            end: range.end,
                            replacement: format!("{head}>{child}</{qname}>"),
                        })
                    } else {
                        let close = slice.rfind("</")?;
                        Some(Patch {
                            start: range.start + close,
                            end: range.start + close,
                            replacement: child,
                        })
                    }
                })
            }
        };
        if let Some(p) = patch {
            patches.push(p);
            resolved += 1;
        }
    }

    match apply_patches(original, patches) {
        Some(out) => (out, resolved),
        None => (original.to_string(), 0),
    }
}

/// Replace a whole packet section (e.g. `<xfa:datasets>…</xfa:datasets>`)
/// inside a consolidated XDP document.
fn replace_packet_section(xdp: &str, packet_local_name: &str, new_packet: &str) -> Option<String> {
    // Find the open tag: `<datasets` or `<prefix:datasets` followed by a
    // delimiter, scanning all candidates.
    let bytes = xdp.as_bytes();
    let mut search = 0usize;
    while let Some(rel) = xdp[search..].find('<') {
        let at = search + rel;
        let rest = &xdp[at + 1..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let qname = &rest[..name_end];
        let local = qname.rsplit(':').next().unwrap_or(qname);
        if local == packet_local_name && !qname.starts_with('/') && !qname.starts_with('!') {
            // Find the matching close tag for this qualified name.
            let close_pat = format!("</{qname}>");
            if let Some(close_rel) = xdp[at..].find(&close_pat) {
                let end = at + close_rel + close_pat.len();
                let mut out = String::with_capacity(xdp.len());
                out.push_str(&xdp[..at]);
                out.push_str(new_packet);
                out.push_str(&xdp[end..]);
                return Some(out);
            }
        }
        search = at + 1;
        if search >= bytes.len() {
            break;
        }
    }
    None
}

/// Swap the datasets (and optionally the form) packet streams in the PDF's
/// `/AcroForm /XFA` entry. Handles the array-of-pairs layout and the single
/// consolidated XDP stream.
pub(crate) fn write_packets_into_pdf(
    doc: &mut Document,
    new_datasets: &str,
    new_form: Option<&str>,
) -> Result<()> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| XfaError::WritebackFailed(format!("no /Root: {e}")))?;
    let catalog = doc
        .get_object(catalog_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| XfaError::WritebackFailed(format!("catalog: {e}")))?;

    let acro_obj = catalog
        .get(b"AcroForm")
        .map_err(|_| XfaError::WritebackFailed("no /AcroForm in catalog".to_string()))?;
    let acro_ref = match acro_obj {
        Object::Reference(r) => Some(*r),
        Object::Dictionary(_) => None,
        _ => {
            return Err(XfaError::WritebackFailed(
                "unsupported /AcroForm object type".to_string(),
            ))
        }
    };
    let acro_dict = match acro_ref {
        Some(r) => doc
            .get_object(r)
            .and_then(|o| o.as_dict())
            .map_err(|e| XfaError::WritebackFailed(format!("AcroForm deref: {e}")))?,
        None => match catalog.get(b"AcroForm") {
            Ok(Object::Dictionary(d)) => d,
            _ => unreachable!("checked above"),
        },
    };
    let xfa_obj = acro_dict
        .get(b"XFA")
        .map_err(|_| XfaError::WritebackFailed("no /XFA in AcroForm".to_string()))?
        .clone();

    match xfa_obj {
        Object::Array(arr) => {
            let mut replacements: Vec<(ObjectId, Vec<u8>)> = Vec::new();
            let mut i = 0;
            let mut found_datasets = false;
            while i + 1 < arr.len() {
                let name: Option<String> = match &arr[i] {
                    Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
                    Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                    _ => None,
                };
                if let Some(name) = name {
                    let target: Option<&str> = match name.as_str() {
                        "datasets" => {
                            found_datasets = true;
                            Some(new_datasets)
                        }
                        "form" => new_form,
                        _ => None,
                    };
                    if let (Some(content), Ok(id)) = (target, arr[i + 1].as_reference()) {
                        replacements.push((id, content.as_bytes().to_vec()));
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            for (id, content) in replacements {
                doc.objects
                    .insert(id, Object::Stream(make_packet_stream(content)));
            }
            if !found_datasets {
                // The source form shipped without a datasets packet (values
                // never saved). Append a `("datasets", stream)` pair so the
                // filled values have a home — the layout Adobe writes on the
                // first save of such a form.
                let stream_id =
                    doc.add_object(Object::Stream(make_packet_stream(new_datasets.into())));
                let mut new_arr = arr.clone();
                new_arr.push(Object::String(
                    b"datasets".to_vec(),
                    lopdf::StringFormat::Literal,
                ));
                new_arr.push(Object::Reference(stream_id));
                set_xfa_entry(doc, catalog_id, acro_ref, Object::Array(new_arr))?;
            }
            Ok(())
        }
        Object::Reference(r) => {
            let existing_xml = match doc.get_object(r) {
                Ok(Object::Stream(stream)) => stream
                    .get_plain_content()
                    .ok()
                    .and_then(|c| String::from_utf8(c).ok())
                    .ok_or_else(|| {
                        XfaError::WritebackFailed("XFA stream not valid UTF-8".to_string())
                    })?,
                _ => {
                    return Err(XfaError::WritebackFailed(
                        "/XFA reference is not a stream".to_string(),
                    ))
                }
            };
            let mut updated = match replace_packet_section(&existing_xml, "datasets", new_datasets)
            {
                Some(u) => u,
                // No datasets section yet: insert one before the closing
                // </xdp:xdp> wrapper tag.
                None => match existing_xml.rfind("</") {
                    Some(close) => {
                        let mut u = existing_xml.clone();
                        u.insert_str(close, new_datasets);
                        u
                    }
                    None => {
                        return Err(XfaError::WritebackFailed(
                            "consolidated XDP has no closing tag to anchor a datasets section"
                                .to_string(),
                        ))
                    }
                },
            };
            if let Some(form) = new_form {
                if let Some(u) = replace_packet_section(&updated, "form", form) {
                    updated = u;
                }
            }
            doc.objects
                .insert(r, Object::Stream(make_packet_stream(updated.into_bytes())));
            Ok(())
        }
        _ => Err(XfaError::WritebackFailed(
            "unsupported /XFA object type".to_string(),
        )),
    }
}

fn make_packet_stream(content: Vec<u8>) -> lopdf::Stream {
    let mut stream = lopdf::Stream::new(lopdf::dictionary! {}, content);
    // Smaller files; ignore failures (raw stream is also valid).
    let _ = stream.compress();
    stream
}

/// Write a new `/XFA` value into the AcroForm dictionary, whether the
/// dictionary is an indirect object or inlined in the catalog.
fn set_xfa_entry(
    doc: &mut Document,
    catalog_id: ObjectId,
    acro_ref: Option<ObjectId>,
    value: Object,
) -> Result<()> {
    match acro_ref {
        Some(r) => {
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(r) {
                d.set(b"XFA".to_vec(), value);
                return Ok(());
            }
            Err(XfaError::WritebackFailed(
                "AcroForm dictionary not mutable".to_string(),
            ))
        }
        None => {
            if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(catalog_id) {
                if let Ok(Object::Dictionary(acro)) = catalog.get_mut(b"AcroForm") {
                    acro.set(b"XFA".to_vec(), value);
                    return Ok(());
                }
            }
            Err(XfaError::WritebackFailed(
                "inline AcroForm dictionary not mutable".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DS: &str = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><form1><name>old</name><addr><street/></addr><row><a>1</a></row><row><a>2</a></row></form1></xfa:data><dd:dataDescription xmlns:dd="http://ns.adobe.com/data-description/"><form1/></dd:dataDescription></xfa:datasets>"#;

    fn p(parts: &[(&str, usize)]) -> Vec<(String, usize)> {
        parts.iter().map(|(n, i)| (n.to_string(), *i)).collect()
    }

    #[test]
    fn splice_replaces_leaf_text() {
        let edits = vec![DatasetsEdit::SetValue {
            path: p(&[("data", 0), ("form1", 0), ("name", 0)]),
            value: "Jasper & Co".to_string(),
        }];
        let out = splice_datasets(DS, &edits).expect("splice");
        assert!(out.contains("<name>Jasper &amp; Co</name>"));
        // Wrapper, sibling rows, and dataDescription preserved verbatim.
        assert!(out.contains("dataDescription"));
        assert!(out.contains("<row><a>1</a></row><row><a>2</a></row>"));
    }

    #[test]
    fn splice_rebuilds_self_closing_leaf() {
        let edits = vec![DatasetsEdit::SetValue {
            path: p(&[("data", 0), ("form1", 0), ("addr", 0), ("street", 0)]),
            value: "Main St 1".to_string(),
        }];
        let out = splice_datasets(DS, &edits).expect("splice");
        assert!(out.contains("<street>Main St 1</street>"));
    }

    #[test]
    fn splice_indexes_same_name_siblings() {
        let edits = vec![DatasetsEdit::SetValue {
            path: p(&[("data", 0), ("form1", 0), ("row", 1), ("a", 0)]),
            value: "two".to_string(),
        }];
        let out = splice_datasets(DS, &edits).expect("splice");
        assert!(out.contains("<row><a>1</a></row><row><a>two</a></row>"));
    }

    #[test]
    fn splice_creates_missing_leaf() {
        let edits = vec![DatasetsEdit::CreateValue {
            parent_path: p(&[("data", 0), ("form1", 0), ("addr", 0)]),
            name: "city".to_string(),
            value: "Amsterdam".to_string(),
        }];
        let out = splice_datasets(DS, &edits).expect("splice");
        assert!(out.contains("<city>Amsterdam</city></addr>"));
    }

    #[test]
    fn splice_unresolvable_path_returns_none() {
        let edits = vec![DatasetsEdit::SetValue {
            path: p(&[("data", 0), ("nope", 0)]),
            value: "x".to_string(),
        }];
        assert!(splice_datasets(DS, &edits).is_none());
    }

    #[test]
    fn regenerate_preserves_wrapper_and_datadescription() {
        let dom = DataDom::from_xml(DS).expect("parse");
        let out = regenerate_datasets(DS, &dom);
        assert!(out.starts_with("<xfa:datasets"));
        assert!(out.contains("dataDescription"));
        assert!(out.contains("<name>old</name>"));
    }

    #[test]
    fn form_packet_value_sync() {
        let form = r#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform name="form1"><field name="name"><value><text>old</text></value></field><field name="empty"/></subform></form>"#;
        let edits = vec![
            FormPacketEdit {
                segments: p(&[("form1", 0), ("name", 0)]),
                value: "new".to_string(),
                insert_if_missing: true,
            },
            FormPacketEdit {
                segments: p(&[("form1", 0), ("empty", 0)]),
                value: "filled".to_string(),
                insert_if_missing: true,
            },
        ];
        let (out, n) = splice_form_packet(form, &edits);
        assert_eq!(n, 2);
        assert!(out.contains("<value><text>new</text></value>"));
        assert!(out.contains("<field name=\"empty\"><value><text>filled</text></value></field>"));
    }

    #[test]
    fn form_packet_radio_member_semantics() {
        // Mirrors the Adobe shape: selected member holds the on-value,
        // deselected member is cleared; absent <value> on a member with
        // insert_if_missing=false stays untouched.
        let form = r#"<form><subform name="form1"><exclGroup name="g"><field name="Ja"><value override="1"><text>2</text></value></field><field name="Nee"><value override="1"/></field></exclGroup></subform></form>"#;
        let edits = vec![
            FormPacketEdit {
                segments: p(&[("form1", 0), ("g", 0), ("Nee", 0)]),
                value: "1".to_string(),
                insert_if_missing: true,
            },
            FormPacketEdit {
                segments: p(&[("form1", 0), ("g", 0), ("Ja", 0)]),
                value: String::new(),
                insert_if_missing: false,
            },
            FormPacketEdit {
                segments: p(&[("form1", 0), ("g", 0)]),
                value: "1".to_string(),
                insert_if_missing: false, // group has no <value> → skipped
            },
        ];
        let (out, n) = splice_form_packet(form, &edits);
        assert_eq!(n, 2, "member edits resolve; group update-only is skipped");
        assert!(
            out.contains(r#"<field name="Nee"><value override="1"><text>1</text></value></field>"#)
        );
        assert!(
            out.contains(r#"<field name="Ja"><value override="1"><text></text></value></field>"#)
        );
        assert!(
            !out.contains("<exclGroup name=\"g\"><value>"),
            "no group value injected"
        );
    }

    #[test]
    fn replace_packet_section_consolidated_xdp() {
        let xdp = r#"<?xml version="1.0"?><xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template>t</template><xfa:datasets xmlns:xfa="x"><xfa:data><f/></xfa:data></xfa:datasets><config>c</config></xdp:xdp>"#;
        let out = replace_packet_section(xdp, "datasets", "<xfa:datasets>NEW</xfa:datasets>")
            .expect("replace");
        assert!(out.contains("<xfa:datasets>NEW</xfa:datasets>"));
        assert!(out.contains("<template>t</template>"));
        assert!(out.contains("<config>c</config>"));
    }
}
