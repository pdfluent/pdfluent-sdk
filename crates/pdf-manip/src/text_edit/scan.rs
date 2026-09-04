//! Container-aware scanning (design §2.1).
//!
//! Unlike the legacy path, page content streams are parsed **per stream** and
//! walked as one logical operator sequence with per-operator provenance, so
//! edits can later rewrite only the touched stream. Form XObjects are scanned
//! as their own containers (detect-only in Phase 1B). Marked-content nesting
//! is tracked so matches know their enclosing `/ActualText`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::Operation;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::content_editor::{get_content_stream_ids, ContentEditor, GraphicsStateTracker};
use crate::error::ManipError;
use crate::text_run::{extract_text_runs, FontMap, TextRun};

use super::signatures::{resolve, resolve_dict};
use super::TextEditError;

/// Maximum Form XObject nesting depth scanned.
const MAX_XOBJECT_DEPTH: usize = 4;

/// Scan of one container's operator sequence.
pub(crate) struct ContainerScan {
    /// Operator sequence (logical, concatenated for page streams).
    pub ops: Vec<Operation>,
    /// For page containers: per-op (stream index, local op index).
    /// Empty for XObject containers (single stream, identity mapping).
    pub op_src: Vec<(usize, usize)>,
    /// Text runs over `ops`.
    pub runs: Vec<TextRun>,
    /// Concatenated visual text of all runs.
    pub combined: String,
    /// Byte offset of each run's start within `combined` (+ end sentinel).
    pub run_bounds: Vec<usize>,
    /// Innermost enclosing /ActualText per operator index.
    pub actual: Vec<Option<String>>,
    /// Graphics state before each operator.
    pub tracker: GraphicsStateTracker,
}

/// Scan of a Form XObject container.
pub(crate) struct XobjScan {
    /// Resource-name path from the page (e.g. `["Fm0", "Inner"]`).
    pub name_path: Vec<String>,
    /// Stream object of the XObject.
    pub stream_id: ObjectId,
    /// Number of pages whose resources reference this XObject.
    pub shared_by: u32,
    /// The scanned content.
    pub scan: ContainerScan,
}

/// Full scan of one page: its logical stream sequence plus every reachable
/// Form XObject.
pub(crate) struct PageScan {
    /// 1-based page number.
    pub page: u32,
    /// Content stream objects of the page, in `/Contents` order.
    pub stream_ids: Vec<ObjectId>,
    /// True when per-stream parsing failed and the page was scanned from the
    /// concatenated bytes instead (operators straddle stream boundaries).
    pub fused: bool,
    /// True when any of this page's content streams is also referenced by
    /// another page.
    pub shared_stream: bool,
    /// Fonts visible to the page's own content.
    pub fonts: FontMap,
    /// The page's logical content scan.
    pub content: ContainerScan,
    /// Form XObjects reachable from the page.
    pub xobjects: Vec<XobjScan>,
}

impl PageScan {
    /// Map a global op index of the page scan to (stream index, local index).
    pub fn source_of(&self, global_op: usize) -> Option<(usize, usize)> {
        self.content.op_src.get(global_op).copied()
    }
}

/// Scan one page of the document.
pub(crate) fn scan_page(doc: &Document, page: u32) -> Result<PageScan, TextEditError> {
    let pages = doc.get_pages();
    let total = pages.len();
    let &page_id = pages
        .get(&page)
        .ok_or(TextEditError::Document(ManipError::PageOutOfRange(
            page as usize,
            total,
        )))?;

    let stream_ids = get_content_stream_ids(doc, page_id);
    let fonts = FontMap::from_page(doc, page).unwrap_or_else(|_| FontMap::empty());

    // Parse each stream separately; fall back to a fused scan when a stream
    // fails on its own but the concatenation parses (tokenization straddles).
    let mut per_stream_ops: Vec<Vec<Operation>> = Vec::with_capacity(stream_ids.len());
    let mut fused = false;
    for &id in &stream_ids {
        match stream_bytes(doc, id).and_then(|b| ContentEditor::from_stream(&b).ok()) {
            Some(editor) => per_stream_ops.push(editor.operations().to_vec()),
            None => {
                fused = true;
                break;
            }
        }
    }

    let (ops, op_src) = if fused {
        let combined_bytes = concat_streams(doc, &stream_ids);
        let editor =
            ContentEditor::from_stream(&combined_bytes).map_err(TextEditError::Document)?;
        let ops = editor.operations().to_vec();
        // Provenance is unknowable for fused pages; every op maps to stream 0.
        let src = vec![(0usize, 0usize); ops.len()];
        (ops, src)
    } else {
        let mut ops = Vec::new();
        let mut src = Vec::new();
        for (si, stream_ops) in per_stream_ops.into_iter().enumerate() {
            for (li, op) in stream_ops.into_iter().enumerate() {
                ops.push(op);
                src.push((si, li));
            }
        }
        (ops, src)
    };

    let shared_stream = page_streams_shared(doc, page_id, &stream_ids);
    let page_resources = page_resources(doc, page_id);
    let content = build_container_scan(ops, op_src, &fonts, doc, page_resources.as_ref());

    // Discover Form XObjects reachable from the page content.
    let mut xobjects = Vec::new();
    let mut visited = Vec::new();
    collect_xobjects(
        doc,
        &content.ops,
        page_resources.as_ref(),
        &fonts,
        Vec::new(),
        &mut visited,
        &mut xobjects,
        0,
    );

    Ok(PageScan {
        page,
        stream_ids,
        fused,
        shared_stream,
        fonts,
        content,
        xobjects,
    })
}

fn build_container_scan(
    ops: Vec<Operation>,
    op_src: Vec<(usize, usize)>,
    fonts: &FontMap,
    doc: &Document,
    resources: Option<&Dictionary>,
) -> ContainerScan {
    let editor = ContentEditor::from_operations(ops);
    let runs = extract_text_runs(&editor, fonts);
    let tracker = editor.track_state();
    let ops = editor.operations().to_vec();

    let mut combined = String::new();
    let mut run_bounds = Vec::with_capacity(runs.len() + 1);
    for run in &runs {
        run_bounds.push(combined.len());
        combined.push_str(&run.text);
    }
    run_bounds.push(combined.len());

    let actual = actual_text_per_op(&ops, doc, resources);

    ContainerScan {
        ops,
        op_src,
        runs,
        combined,
        run_bounds,
        actual,
        tracker,
    }
}

// ---------------------------------------------------------------------------
// Marked content (/ActualText) tracking
// ---------------------------------------------------------------------------

/// For every operator index, the innermost enclosing `/ActualText`, if any.
fn actual_text_per_op(
    ops: &[Operation],
    doc: &Document,
    resources: Option<&Dictionary>,
) -> Vec<Option<String>> {
    let mut result = Vec::with_capacity(ops.len());
    // Stack of Option<actual text> for each open BMC/BDC scope.
    let mut stack: Vec<Option<String>> = Vec::new();

    for op in ops {
        let innermost = stack.iter().rev().find_map(|s| s.clone());
        result.push(innermost);

        match op.operator.as_str() {
            "BMC" => stack.push(None),
            "BDC" => {
                let props = op.operands.get(1).and_then(|o| match o {
                    Object::Dictionary(d) => Some(d.clone()),
                    Object::Name(name) => lookup_properties(doc, resources, name),
                    _ => None,
                });
                let actual = props.and_then(|d| match d.get(b"ActualText") {
                    Ok(Object::String(s, _)) => Some(pdf_text_string(s)),
                    _ => None,
                });
                stack.push(actual);
            }
            "EMC" => {
                stack.pop();
            }
            _ => {}
        }
    }
    result
}

/// Resolve a BDC `/Properties` name through the container resources.
fn lookup_properties(
    doc: &Document,
    resources: Option<&Dictionary>,
    name: &[u8],
) -> Option<Dictionary> {
    let res = resources?;
    let props = resolve_dict(doc, res.get(b"Properties").ok()?)?;
    resolve_dict(doc, props.get(name).ok()?).cloned()
}

/// Decode a PDF text string: UTF-16BE with BOM, else PDFDocEncoding
/// (approximated as Latin-1, which covers the printable overlap).
fn pdf_text_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        char::decode_utf16(units)
            .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

// ---------------------------------------------------------------------------
// Form XObject discovery
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn collect_xobjects(
    doc: &Document,
    ops: &[Operation],
    resources: Option<&Dictionary>,
    parent_fonts: &FontMap,
    path: Vec<String>,
    visited: &mut Vec<ObjectId>,
    out: &mut Vec<XobjScan>,
    depth: usize,
) {
    if depth >= MAX_XOBJECT_DEPTH {
        return;
    }
    let Some(res) = resources else { return };
    let Some(xobjects) = res.get(b"XObject").ok().and_then(|o| resolve_dict(doc, o)) else {
        return;
    };

    for op in ops {
        if op.operator != "Do" {
            continue;
        }
        let Some(Object::Name(name)) = op.operands.first() else {
            continue;
        };
        let Ok(entry) = xobjects.get(name) else {
            continue;
        };
        let Object::Reference(stream_id) = entry else {
            continue;
        };
        if visited.contains(stream_id) {
            continue;
        }
        let Ok(Object::Stream(stream)) = doc.get_object(*stream_id) else {
            continue;
        };
        let is_form = matches!(
            stream.dict.get(b"Subtype").map(|o| resolve(doc, o)),
            Ok(Object::Name(st)) if st == b"Form"
        );
        if !is_form {
            continue;
        }
        visited.push(*stream_id);

        let mut sub_path = path.clone();
        sub_path.push(String::from_utf8_lossy(name).to_string());

        let xobj_fonts = FontMap::from_xobject_stream(doc, *stream_id, parent_fonts);
        let xobj_resources = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|o| resolve_dict(doc, o))
            .cloned();

        let mut decoded = stream.clone();
        let _ = decoded.decompress();
        if let Ok(editor) = ContentEditor::from_stream(&decoded.content) {
            let ops_vec = editor.operations().to_vec();
            let scan = build_container_scan(
                ops_vec,
                Vec::new(),
                &xobj_fonts,
                doc,
                xobj_resources.as_ref(),
            );
            // Recurse before moving `scan`.
            collect_xobjects(
                doc,
                &scan.ops,
                xobj_resources.as_ref(),
                &xobj_fonts,
                sub_path.clone(),
                visited,
                out,
                depth + 1,
            );
            out.push(XobjScan {
                name_path: sub_path,
                stream_id: *stream_id,
                shared_by: count_xobject_referencing_pages(doc, *stream_id),
                scan,
            });
        }
    }
}

/// How many pages reference `xobj_id` from their `/Resources/XObject` dict.
fn count_xobject_referencing_pages(doc: &Document, xobj_id: ObjectId) -> u32 {
    let mut count = 0;
    for (_, page_id) in doc.get_pages() {
        let Some(res) = page_resources(doc, page_id) else {
            continue;
        };
        let Some(xobjs) = res.get(b"XObject").ok().and_then(|o| resolve_dict(doc, o)) else {
            continue;
        };
        let referenced = xobjs
            .iter()
            .any(|(_, v)| matches!(v, Object::Reference(id) if *id == xobj_id));
        if referenced {
            count += 1;
        }
    }
    count.max(1)
}

// ---------------------------------------------------------------------------
// Stream helpers
// ---------------------------------------------------------------------------

fn stream_bytes(doc: &Document, id: ObjectId) -> Option<Vec<u8>> {
    match doc.get_object(id) {
        Ok(Object::Stream(s)) => {
            let mut s = s.clone();
            let _ = s.decompress();
            Some(s.content)
        }
        _ => None,
    }
}

fn concat_streams(doc: &Document, ids: &[ObjectId]) -> Vec<u8> {
    let mut combined = Vec::new();
    for &id in ids {
        if let Some(bytes) = stream_bytes(doc, id) {
            if !combined.is_empty() {
                combined.push(b'\n');
            }
            combined.extend_from_slice(&bytes);
        }
    }
    combined
}

fn page_resources(doc: &Document, page_id: ObjectId) -> Option<Dictionary> {
    let page = doc.get_dictionary(page_id).ok()?;
    match page.get(b"Resources") {
        Ok(obj) => resolve_dict(doc, obj).cloned(),
        Err(_) => {
            // Walk up the page tree for inherited resources.
            let mut current = page.clone();
            loop {
                let parent = match current.get(b"Parent") {
                    Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok()?.clone(),
                    _ => return None,
                };
                if let Ok(obj) = parent.get(b"Resources") {
                    return resolve_dict(doc, obj).cloned();
                }
                current = parent;
            }
        }
    }
}

/// Whether any of `stream_ids` is also a content stream of another page.
fn page_streams_shared(doc: &Document, page_id: ObjectId, stream_ids: &[ObjectId]) -> bool {
    if stream_ids.is_empty() {
        return false;
    }
    for (_, other_id) in doc.get_pages() {
        if other_id == page_id {
            continue;
        }
        let other = get_content_stream_ids(doc, other_id);
        if other.iter().any(|id| stream_ids.contains(id)) {
            return true;
        }
    }
    false
}
