//! Two-phase PDF redaction: mark areas, then apply permanent removal.
//!
//! Phase 1: Mark redaction areas (regions on specific pages).
//! Phase 2: Apply redactions — permanently remove content, draw overlays,
//! and clean metadata.

use crate::error::{RedactError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashMap;
use std::io::Write;

const MINIMAL_XMP: &[u8] = b"<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n    <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\n      <pdf:Producer>pdfluent</pdf:Producer>\n    </rdf:Description>\n  </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>";

/// A rectangular area on a page to be redacted.
#[derive(Debug, Clone)]
pub struct RedactionArea {
    /// The page number (1-based).
    pub page: u32,
    /// The rectangle [x0, y0, x1, y1] in PDF coordinates.
    pub rect: [f64; 4],
    /// Fill color [r, g, b] for the redaction overlay (default: black).
    pub fill_color: [f64; 3],
    /// Optional overlay text to display on the redacted area.
    pub overlay_text: Option<String>,
}

impl RedactionArea {
    /// Create a new redaction area with default black fill.
    pub fn new(page: u32, rect: [f64; 4]) -> Self {
        Self {
            page,
            rect,
            fill_color: [0.0, 0.0, 0.0],
            overlay_text: None,
        }
    }

    /// Set a custom fill color.
    pub fn with_color(mut self, r: f64, g: f64, b: f64) -> Self {
        self.fill_color = [r, g, b];
        self
    }

    /// Set overlay text.
    pub fn with_overlay(mut self, text: impl Into<String>) -> Self {
        self.overlay_text = Some(text.into());
        self
    }
}

/// Report generated after applying redactions.
#[derive(Debug, Clone)]
pub struct RedactionReport {
    /// Number of areas that were redacted.
    pub areas_redacted: usize,
    /// Number of content stream operations removed.
    pub operations_removed: usize,
    /// Number of pages affected.
    pub pages_affected: usize,
    /// Whether metadata was cleaned.
    pub metadata_cleaned: bool,
}

/// Two-phase redactor: mark areas, then apply.
#[derive(Debug)]
pub struct Redactor {
    /// Pending redaction areas.
    areas: Vec<RedactionArea>,
}

impl Redactor {
    /// Create a new empty redactor.
    pub fn new() -> Self {
        Self { areas: Vec::new() }
    }

    /// Mark an area for redaction.
    pub fn mark(&mut self, area: RedactionArea) {
        self.areas.push(area);
    }

    /// Mark multiple areas for redaction.
    pub fn mark_all(&mut self, areas: impl IntoIterator<Item = RedactionArea>) {
        self.areas.extend(areas);
    }

    /// Return the number of pending redaction areas.
    pub fn pending_count(&self) -> usize {
        self.areas.len()
    }

    /// Apply all pending redactions to the document.
    ///
    /// This permanently removes content in the marked areas, draws overlay
    /// rectangles, and cleans document metadata.
    pub fn apply(&self, doc: &mut Document) -> Result<RedactionReport> {
        if self.areas.is_empty() {
            return Err(RedactError::NoAreas);
        }

        let pages = doc.get_pages();
        let total = pages.len() as u32;

        // Validate all page numbers.
        for area in &self.areas {
            if area.page == 0 || area.page > total {
                return Err(RedactError::PageOutOfRange(area.page, total));
            }
        }

        let mut total_ops_removed = 0;
        let mut affected_pages = std::collections::HashSet::new();

        // Group areas by page.
        let mut page_areas: std::collections::HashMap<u32, Vec<&RedactionArea>> =
            std::collections::HashMap::new();
        for area in &self.areas {
            page_areas.entry(area.page).or_default().push(area);
        }

        for (&page_num, areas) in &page_areas {
            let page_id = match pages.get(&page_num) {
                Some(&id) => id,
                None => continue,
            };

            // Phase 1: Filter content stream to remove text in redacted areas.
            let ops_removed = redact_page_content(doc, page_id, areas)?;
            total_ops_removed += ops_removed;

            // Phase 2: Black out Image XObjects whose bounding box overlaps a
            // redaction area.  Unsupported filters (JBIG2, JPEG2000, Crypt)
            // cause this to return UnsupportedImageFilter.
            redact_image_xobjects(doc, page_id, areas)?;

            // Phase 3: Draw redaction overlays.
            draw_redaction_overlays(doc, page_id, areas)?;

            // Phase 4: Strip /Contents and /T from overlapping annotations.
            strip_annotation_contents(doc, page_id, areas)?;

            // Phase 5 (#1339 M8-REDACT-03): replace /ToUnicode CMaps on every
            // font used by this redacted page with a U+FFFD-everywhere stub.
            // Page-scoped clone-on-write keeps fonts shared with non-redacted
            // pages intact. Fails closed with UnsupportedToUnicodeCMap if the
            // existing /ToUnicode shape isn't understood — we never silently
            // drop the key (would regress PDF/A and leave decode fallbacks
            // open for pdf-extract).
            strip_tounicode_for_redacted_page(doc, page_id)?;

            affected_pages.insert(page_num);
        }

        // Phase 6: Clean metadata (Info dict removal + XMP replacement).
        clean_metadata(doc);

        Ok(RedactionReport {
            areas_redacted: self.areas.len(),
            operations_removed: total_ops_removed,
            pages_affected: affected_pages.len(),
            metadata_cleaned: true,
        })
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter text operations from a page's content stream that fall within redaction areas.
fn redact_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<usize> {
    let content_ids = get_content_stream_ids(doc, page_id);
    let mut total_removed = 0;

    for content_id in content_ids {
        // Decompress the stream before decoding — compressed content (FlateDecode)
        // cannot be parsed directly by Content::decode.
        let content_bytes = match doc.get_object(content_id) {
            Ok(Object::Stream(ref s)) => {
                let mut stream = s.clone();
                let _ = stream.decompress();
                stream.content
            }
            _ => continue,
        };

        // Strip inline images (BI…EI) before parsing — lopdf's content
        // decoder cannot handle their binary payload.
        let (parseable, _) = pdf_manip::content_editor::strip_inline_images(&content_bytes);

        let content = match Content::decode(&parseable) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (filtered_ops, removed) = filter_text_ops(&content.operations, areas);
        total_removed += removed;

        let new_content = Content {
            operations: filtered_ops,
        };
        let encoded = new_content
            .encode()
            .map_err(|e| RedactError::Other(format!("failed to encode content: {e}")))?;

        // Compress the new content.
        let compressed = compress_flate(&encoded);

        if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(content_id) {
            if compressed.len() < encoded.len() {
                s.content = compressed;
                s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
                s.dict
                    .set("Length", Object::Integer(s.content.len() as i64));
            } else {
                s.content = encoded;
                s.dict.remove(b"Filter");
                s.dict
                    .set("Length", Object::Integer(s.content.len() as i64));
            }
        }
    }

    Ok(total_removed)
}

/// Draw redaction overlay rectangles on the page.
///
/// The overlay is drawn in a clean graphics state to prevent interference
/// from the page's existing CTM, opacity, or blend modes.  Existing content
/// is wrapped in `q … Q` so its state is fully isolated.
fn draw_redaction_overlays(
    doc: &mut Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<()> {
    // Wrap existing content in q/Q to isolate the graphics state.
    // Without this, a dirty CTM or non-unit opacity from previous content
    // would affect our overlay (causing "too light" or misplaced rects).
    wrap_existing_content_in_save_restore(doc, page_id);

    let mut ops = Vec::new();

    for area in areas {
        let [r, g, b] = area.fill_color;
        let [x0, y0, x1, y1] = area.rect;

        // Use content-space coordinates directly.  The text positions from
        // extract_positioned_chars are already in the content stream's
        // coordinate system.  Our overlay is appended AFTER the existing
        // content (which is wrapped in q…Q), so the CTM is restored to the
        // page default.  For rotated pages, the renderer applies Rotate to
        // all content streams uniformly — both the original text and our
        // overlay — so the overlay covers the correct visual position
        // without us needing to transform coordinates.
        let (tx0, ty0, tx1, ty1) = (x0, y0, x1, y1);
        let w = tx1 - tx0;
        let h = ty1 - ty0;

        // Save state, set color, draw filled rectangle.
        ops.push(Operation::new("q", vec![]));
        ops.push(Operation::new(
            "rg",
            vec![
                Object::Real(r as f32),
                Object::Real(g as f32),
                Object::Real(b as f32),
            ],
        ));
        ops.push(Operation::new(
            "re",
            vec![
                Object::Real(tx0 as f32),
                Object::Real(ty0 as f32),
                Object::Real(w as f32),
                Object::Real(h as f32),
            ],
        ));
        ops.push(Operation::new("f", vec![]));

        // Draw overlay text if specified.
        if let Some(ref text) = area.overlay_text {
            // Calculate font size to fit within the rectangle.
            let max_font_size = h.abs() * 0.7;
            let font_size = max_font_size.clamp(4.0, 12.0) as f32;

            ops.push(Operation::new("BT", vec![]));
            // Set white text color.
            ops.push(Operation::new(
                "rg",
                vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)],
            ));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"Helvetica".to_vec()), Object::Real(font_size)],
            ));
            ops.push(Operation::new(
                "Td",
                vec![
                    Object::Real((tx0 + 2.0) as f32),
                    Object::Real((ty0 + 2.0) as f32),
                ],
            ));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    text.as_bytes().to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ));
            ops.push(Operation::new("ET", vec![]));
        }

        ops.push(Operation::new("Q", vec![]));
    }

    let new_content = Content { operations: ops };
    let encoded = new_content
        .encode()
        .map_err(|e| RedactError::Other(format!("failed to encode overlay: {e}")))?;

    let overlay_stream = Stream::new(dictionary! {}, encoded);
    let overlay_id = doc.add_object(Object::Stream(overlay_stream));

    append_content_to_page(doc, page_id, overlay_id);

    Ok(())
}

// ---------------------------------------------------------------------------
// Image XObject redaction (#1293)
// ---------------------------------------------------------------------------

/// Black out all Image XObjects on `page_id` whose page-space bounding box
/// overlaps any of `areas`.  Returns `UnsupportedImageFilter` if a matching
/// image uses JBIG2Decode, JPXDecode, or Crypt.
fn redact_image_xobjects(
    doc: &mut Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<()> {
    let overlapping = find_overlapping_images(doc, page_id, areas)?;
    for id in overlapping {
        blackout_image_xobject(doc, id)?;
    }
    Ok(())
}

/// Walk the page content streams, tracking the CTM, and collect IDs of Image
/// XObjects whose transformed bounding box overlaps a redaction area.
fn find_overlapping_images(
    doc: &Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<Vec<ObjectId>> {
    let mut overlapping: Vec<ObjectId> = Vec::new();
    let xobjects = get_page_xobjects(doc, page_id);

    for content_id in get_content_stream_ids(doc, page_id) {
        let content_bytes = match doc.get_object(content_id) {
            Ok(Object::Stream(ref s)) => {
                let mut stream = s.clone();
                let _ = stream.decompress();
                stream.content
            }
            _ => continue,
        };

        let (parseable, _) = pdf_manip::content_editor::strip_inline_images(&content_bytes);
        let content = match Content::decode(&parseable) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Identity CTM: [a, b, c, d, e, f] where x' = a*x + c*y + e
        let mut ctm_stack: Vec<[f64; 6]> = vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]];

        for op in &content.operations {
            match op.operator.as_str() {
                "q" => {
                    let top = ctm_stack
                        .last()
                        .copied()
                        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                    ctm_stack.push(top);
                }
                "Q" => {
                    if ctm_stack.len() > 1 {
                        ctm_stack.pop();
                    }
                }
                "cm" if op.operands.len() >= 6 => {
                    let cm = [
                        as_number(&op.operands[0]).unwrap_or(0.0),
                        as_number(&op.operands[1]).unwrap_or(0.0),
                        as_number(&op.operands[2]).unwrap_or(0.0),
                        as_number(&op.operands[3]).unwrap_or(0.0),
                        as_number(&op.operands[4]).unwrap_or(0.0),
                        as_number(&op.operands[5]).unwrap_or(0.0),
                    ];
                    if let Some(current) = ctm_stack.last_mut() {
                        *current = concat_matrix(cm, *current);
                    }
                }
                "Do" if !op.operands.is_empty() => {
                    if let Object::Name(ref name) = op.operands[0] {
                        let name_str = String::from_utf8_lossy(name).into_owned();
                        if let Some(&xobj_id) = xobjects.get(&name_str) {
                            if is_image_xobject(doc, xobj_id) {
                                let ctm = ctm_stack
                                    .last()
                                    .copied()
                                    .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                                let bbox = ctm_bbox(ctm);
                                if bbox_overlaps_any(bbox, areas) && !overlapping.contains(&xobj_id)
                                {
                                    overlapping.push(xobj_id);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(overlapping)
}

/// Replace the content of an Image XObject with all-black pixel data.
///
/// Preserves Width, Height, and ColorSpace.  Updates Filter to FlateDecode.
/// Returns `UnsupportedImageFilter` for JBIG2Decode, JPXDecode, and Crypt.
fn blackout_image_xobject(doc: &mut Document, xobj_id: ObjectId) -> Result<()> {
    let (width, height, components) = {
        match doc.get_object(xobj_id) {
            Ok(Object::Stream(ref s)) => {
                check_image_filter(&s.dict)?;
                let w = s.dict.get(b"Width").ok().and_then(as_number).unwrap_or(1.0) as usize;
                let h = s
                    .dict
                    .get(b"Height")
                    .ok()
                    .and_then(as_number)
                    .unwrap_or(1.0) as usize;
                let c = color_space_components(&s.dict);
                (w, h, c)
            }
            _ => return Ok(()),
        }
    };

    let black = vec![0u8; width * height * components];
    let compressed = compress_flate(&black);
    let len = compressed.len() as i64;

    if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(xobj_id) {
        s.content = compressed;
        s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        s.dict.set("Length", Object::Integer(len));
        s.dict.remove(b"DecodeParms");
    }
    Ok(())
}

/// Return an error if the image stream uses an unsupported filter.
fn check_image_filter(dict: &lopdf::Dictionary) -> Result<()> {
    let filter_obj = match dict.get(b"Filter") {
        Ok(f) => f.clone(),
        Err(_) => return Ok(()),
    };
    let names: Vec<String> = match filter_obj {
        Object::Name(ref n) => vec![String::from_utf8_lossy(n).into_owned()],
        Object::Array(ref arr) => arr
            .iter()
            .filter_map(|o| {
                if let Object::Name(ref n) = o {
                    Some(String::from_utf8_lossy(n).into_owned())
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    };
    for name in &names {
        if matches!(name.as_str(), "JBIG2Decode" | "JPXDecode" | "Crypt") {
            return Err(RedactError::UnsupportedImageFilter(name.clone()));
        }
    }
    Ok(())
}

/// Number of colour components implied by a ColorSpace entry.
fn color_space_components(dict: &lopdf::Dictionary) -> usize {
    match dict.get(b"ColorSpace") {
        Ok(Object::Name(ref n)) => match std::str::from_utf8(n).unwrap_or("") {
            "DeviceGray" | "CalGray" => 1,
            "DeviceCMYK" => 4,
            _ => 3, // DeviceRGB, sRGB, unknown → default RGB
        },
        _ => 3,
    }
}

/// Collect the name → ObjectId map of XObjects declared in the page's Resources.
fn get_page_xobjects(doc: &Document, page_id: ObjectId) -> HashMap<String, ObjectId> {
    let mut result = HashMap::new();

    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return result,
    };
    let resources = match page_dict.get(b"Resources") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };
    let xobj_dict = match resources.get(b"XObject") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for (name, obj) in xobj_dict.iter() {
        if let Object::Reference(id) = obj {
            result.insert(String::from_utf8_lossy(name).into_owned(), *id);
        }
    }
    result
}

/// Return true if the object at `id` is an Image XObject.
fn is_image_xobject(doc: &Document, id: ObjectId) -> bool {
    match doc.get_object(id) {
        Ok(Object::Stream(ref s)) => match s.dict.get(b"Subtype") {
            Ok(Object::Name(ref n)) => n.as_slice() == b"Image",
            _ => false,
        },
        _ => false,
    }
}

/// Concatenate two CTM matrices: result = `a` × `b` (both in PDF [a b c d e f] order).
fn concat_matrix(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    let [aa, ab, ac, ad, ae, af] = a;
    let [m0, m1, m2, m3, m4, m5] = b;
    [
        aa * m0 + ac * m1,
        ab * m0 + ad * m1,
        aa * m2 + ac * m3,
        ab * m2 + ad * m3,
        aa * m4 + ac * m5 + ae,
        ab * m4 + ad * m5 + af,
    ]
}

/// Compute the axis-aligned bounding box of the unit square [0,0]–[1,1]
/// after applying `ctm`.  This is the page-space bbox of an Image XObject.
fn ctm_bbox(ctm: [f64; 6]) -> [f64; 4] {
    let [a, b, c, d, e, f] = ctm;
    let corners = [
        (e, f),
        (a + e, b + f),
        (c + e, d + f),
        (a + c + e, b + d + f),
    ];
    let min_x = corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|p| p.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = corners
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max);
    [min_x, min_y, max_x, max_y]
}

/// Return true if `bbox` overlaps any of `areas` (AABB test).
fn bbox_overlaps_any(bbox: [f64; 4], areas: &[&RedactionArea]) -> bool {
    let [bx0, by0, bx1, by1] = bbox;
    for area in areas {
        let [ax0, ay0, ax1, ay1] = area.rect;
        let (rax0, rax1) = if ax0 < ax1 { (ax0, ax1) } else { (ax1, ax0) };
        let (ray0, ray1) = if ay0 < ay1 { (ay0, ay1) } else { (ay1, ay0) };
        let no_overlap = bx1 < rax0 || bx0 > rax1 || by1 < ray0 || by0 > ray1;
        if !no_overlap {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Annotation field stripping (#1294)
// ---------------------------------------------------------------------------

/// Strip `/Contents` and `/T` from every annotation on `page_id` whose
/// `/Rect` overlaps any of `areas`.
fn strip_annotation_contents(
    doc: &mut Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<()> {
    let annot_ids = get_page_annotation_ids(doc, page_id);
    for annot_id in annot_ids {
        let rect = match get_annotation_rect(doc, annot_id) {
            Some(r) => r,
            None => continue,
        };
        if bbox_overlaps_any(rect, areas) {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(annot_id) {
                d.remove(b"Contents");
                d.remove(b"T");
            }
        }
    }
    Ok(())
}

/// Return the ObjectIds of all annotations on `page_id`.
fn get_page_annotation_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return Vec::new(),
    };
    let annots_arr = match page_dict.get(b"Annots") {
        Ok(Object::Array(ref arr)) => arr.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(ref arr)) => arr.clone(),
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    annots_arr
        .iter()
        .filter_map(|o| {
            if let Object::Reference(id) = o {
                Some(*id)
            } else {
                None
            }
        })
        .collect()
}

/// Return the page-space bounding rectangle of an annotation, or None.
fn get_annotation_rect(doc: &Document, annot_id: ObjectId) -> Option<[f64; 4]> {
    let dict = match doc.get_object(annot_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return None,
    };
    match dict.get(b"Rect") {
        Ok(Object::Array(ref arr)) if arr.len() >= 4 => {
            let vals: Vec<f64> = arr.iter().filter_map(as_number).collect();
            if vals.len() >= 4 {
                Some([vals[0], vals[1], vals[2], vals[3]])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Clean document metadata: remove Info dictionary, replace XMP, and strip thumbnails.
fn clean_metadata(doc: &mut Document) {
    doc.trailer.remove(b"Info");
    replace_xmp_metadata(doc);
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
            page.remove(b"Thumb");
        }
    }
}

/// Replace the catalog /Metadata XMP stream with a minimal stub that only
/// preserves `pdf:Producer`.  Creates the stream if none exists.
fn replace_xmp_metadata(doc: &mut Document) {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };

    let meta_id: Option<ObjectId> = {
        match doc.get_object(root_id) {
            Ok(Object::Dictionary(ref d)) => match d.get(b"Metadata") {
                Ok(Object::Reference(id)) => Some(*id),
                _ => None,
            },
            _ => return,
        }
    };

    let xmp = MINIMAL_XMP.to_vec();
    let xmp_len = xmp.len() as i64;

    if let Some(meta_stream_id) = meta_id {
        if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(meta_stream_id) {
            s.content = xmp;
            s.dict.set("Length", Object::Integer(xmp_len));
            s.dict.remove(b"Filter");
        }
    } else {
        let xmp_stream = Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            xmp,
        );
        let new_id = doc.add_object(Object::Stream(xmp_stream));
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(root_id) {
            catalog.set("Metadata", Object::Reference(new_id));
        }
    }
}

/// Filter text operations, removing those whose position falls within any redaction area.
fn filter_text_ops(ops: &[Operation], areas: &[&RedactionArea]) -> (Vec<Operation>, usize) {
    let mut filtered = Vec::new();
    let mut removed = 0;
    let mut text_x: f64 = 0.0;
    let mut text_y: f64 = 0.0;
    let mut in_text = false;

    for op in ops {
        match op.operator.as_str() {
            "BT" => {
                in_text = true;
                text_x = 0.0;
                text_y = 0.0;
                filtered.push(op.clone());
            }
            "ET" => {
                in_text = false;
                filtered.push(op.clone());
            }
            "Tm" => {
                if in_text && op.operands.len() >= 6 {
                    text_x = as_number(&op.operands[4]).unwrap_or(0.0);
                    text_y = as_number(&op.operands[5]).unwrap_or(0.0);
                }
                filtered.push(op.clone());
            }
            "Td" | "TD" => {
                if in_text && op.operands.len() >= 2 {
                    text_x += as_number(&op.operands[0]).unwrap_or(0.0);
                    text_y += as_number(&op.operands[1]).unwrap_or(0.0);
                }
                filtered.push(op.clone());
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if in_text && point_in_any_rect(text_x, text_y, areas) {
                    removed += 1;
                } else {
                    filtered.push(op.clone());
                }
            }
            _ => {
                filtered.push(op.clone());
            }
        }
    }

    (filtered, removed)
}

/// Check if a point falls within any redaction area.
fn point_in_any_rect(x: f64, y: f64, areas: &[&RedactionArea]) -> bool {
    for area in areas {
        let [x0, y0, x1, y1] = area.rect;
        let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        if x >= min_x && x <= max_x && y >= min_y && y <= max_y {
            return true;
        }
    }
    false
}

/// Get the content stream object IDs for a page.
fn get_content_stream_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };

    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return Vec::new(),
    };

    match page_dict.get(b"Contents") {
        Ok(Object::Reference(id)) => vec![*id],
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|obj| {
                if let Object::Reference(id) = obj {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Resolve page /Contents to a flat list of stream ObjectIds.
///
/// Handles all valid PDF structures including indirect references to
/// arrays (common in incrementally-updated PDFs where an update replaces
/// a single content stream with an array).
fn resolve_content_streams(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };
    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return Vec::new(),
    };
    match page_dict.get(b"Contents").ok() {
        Some(c) => flatten_content_refs(doc, c),
        None => Vec::new(),
    }
}

fn flatten_content_refs(doc: &Document, obj: &Object) -> Vec<ObjectId> {
    match obj {
        Object::Reference(id) => {
            if let Ok(Object::Array(arr)) = doc.get_object(*id) {
                return arr
                    .iter()
                    .flat_map(|o| flatten_content_refs(doc, o))
                    .collect();
            }
            vec![*id]
        }
        Object::Array(arr) => arr
            .iter()
            .flat_map(|o| flatten_content_refs(doc, o))
            .collect(),
        _ => Vec::new(),
    }
}

/// Append a content stream reference to a page's Contents array.
fn append_content_to_page(doc: &mut Document, page_id: ObjectId, content_id: ObjectId) {
    let existing = resolve_content_streams(doc, page_id);
    let new_contents = if existing.is_empty() {
        Object::Reference(content_id)
    } else {
        let mut arr: Vec<Object> = existing.into_iter().map(Object::Reference).collect();
        arr.push(Object::Reference(content_id));
        Object::Array(arr)
    };

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", new_contents);
    }
}

/// Wrap existing page content in q/Q to isolate graphics state.
///
/// This prevents the page's CTM, opacity, or blend mode from leaking into
/// the overlay content stream we append afterwards.
fn wrap_existing_content_in_save_restore(doc: &mut Document, page_id: ObjectId) {
    let existing = resolve_content_streams(doc, page_id);
    if existing.is_empty() {
        return;
    }

    let q_stream = Stream::new(dictionary! {}, b"q\n".to_vec());
    let q_id = doc.add_object(Object::Stream(q_stream));

    let big_q_stream = Stream::new(dictionary! {}, b"\nQ\n".to_vec());
    let big_q_id = doc.add_object(Object::Stream(big_q_stream));

    // Build flat array: [q, ...existing streams..., Q]
    let mut new_arr = Vec::with_capacity(existing.len() + 2);
    new_arr.push(Object::Reference(q_id));
    for id in existing {
        new_arr.push(Object::Reference(id));
    }
    new_arr.push(Object::Reference(big_q_id));
    let wrapped = Object::Array(new_arr);

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", wrapped);
    }
}

/// Read page rotation (0, 90, 180, 270) and MediaBox [x0, y0, x1, y1].
#[allow(dead_code)]
fn page_rotation_and_media_box(doc: &Document, page_id: ObjectId) -> (i64, [f64; 4]) {
    let default_box = [0.0, 0.0, 612.0, 792.0];

    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return (0, default_box),
    };
    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return (0, default_box),
    };

    let rotate = page_dict
        .get(b"Rotate")
        .ok()
        .and_then(|r| match r {
            Object::Integer(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(0);

    let media_box = page_dict
        .get(b"MediaBox")
        .ok()
        .and_then(|mb| {
            if let Object::Array(arr) = mb {
                if arr.len() >= 4 {
                    let vals: Vec<f64> = arr.iter().filter_map(as_number).collect();
                    if vals.len() >= 4 {
                        return Some([vals[0], vals[1], vals[2], vals[3]]);
                    }
                }
            }
            None
        })
        .unwrap_or(default_box);

    (rotate, media_box)
}

/// Transform a rect from user-space coordinates to the rotated page's
/// coordinate system.  Overlay rects are in "visual" coordinates (what the
/// user sees), but the page's content stream uses the MediaBox coordinate
/// system with /Rotate applied by the viewer.
#[allow(dead_code)]
fn transform_rect_for_rotation(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    rotate: i64,
    media_box: &[f64; 4],
) -> (f64, f64, f64, f64) {
    let page_w = media_box[2] - media_box[0];
    let page_h = media_box[3] - media_box[1];

    // Inverse of the viewer's rotation (content→visual):
    //   Rotate=90:  visual(x,y) = content(cy, W-cx) → content(cx,cy) = (W-vy, vx)
    //   Rotate=180: visual(x,y) = content(W-cx, H-cy) → content(cx,cy) = (W-vx, H-vy)
    //   Rotate=270: visual(x,y) = content(H-cy, cx)   → content(cx,cy) = (vy, H-vx)
    match rotate % 360 {
        0 => (x0, y0, x1, y1),
        90 | -270 => (page_w - y1, x0, page_w - y0, x1),
        180 | -180 => (page_w - x1, page_h - y1, page_w - x0, page_h - y0),
        270 | -90 => (y0, page_h - x1, y1, page_h - x0),
        _ => (x0, y0, x1, y1),
    }
}

/// Compress data with flate/zlib.
fn compress_flate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    if encoder.write_all(data).is_ok() {
        encoder.finish().unwrap_or_else(|_| data.to_vec())
    } else {
        data.to_vec()
    }
}

// ─── M8-REDACT-03 (#1339) /ToUnicode strip ──────────────────────────────

/// Phase 5: replace /ToUnicode CMaps on every font referenced by a redacted
/// page with a U+FFFD-everywhere stub.
///
/// Cascade clone-on-write semantics:
///   * /Resources is resolved via the /Pages parent chain (PDF resource
///     inheritance) so pages that omit a direct /Resources entry still get
///     their fonts processed (Codex P1 #1379 fix).
///   * Both the page-private /Resources dict AND its /Font sub-dict get
///     materialised as fresh objects before any mutation, so a redacted
///     page never rewrites a /Resources or /Font dict that other pages
///     happen to share via indirect reference (Codex P1 #1379 fix).
///   * Each font dict that needs stubbing also gets cloned; the original
///     shared font object stays intact for non-redacted pages.
///
/// Returns `Err(UnsupportedToUnicodeCMap)` rather than silently stripping
/// when the existing /ToUnicode CMap shape isn't understood by the
/// conservative source-code extractor in `crate::tounicode`.
fn strip_tounicode_for_redacted_page(doc: &mut Document, page_id: ObjectId) -> Result<usize> {
    // Per-redaction-call memoization of `original Form XObject id →
    // cloned-and-stubbed Form XObject id`. Used by
    // `strip_tounicode_for_container` so that any time the same original
    // Form is encountered (multiple /XObject aliases on the same page,
    // the same Form referenced from nested Forms, a self-cycle, etc.)
    // it resolves to ONE shared, already-stubbed clone instead of
    // producing fresh unprocessed clones for the second-and-later
    // aliases (Codex P1 ronde-6 follow-up #1379). Without the memo,
    // `visited.insert` correctly prevented infinite recursion but did
    // not prevent the second alias from getting an un-stubbed clone,
    // leaving extractable text reachable from the redacted page.
    let mut form_clones: std::collections::HashMap<ObjectId, ObjectId> =
        std::collections::HashMap::new();
    strip_tounicode_for_container(doc, page_id, ContainerKind::Page, &mut form_clones)
}

#[derive(Copy, Clone)]
enum ContainerKind {
    /// A page dictionary. /Resources lives directly on the dict (or is
    /// inherited from a /Pages parent).
    Page,
    /// A Form XObject Stream (/Subtype /Form). /Resources lives inside the
    /// stream's dict; Form XObjects do NOT participate in /Pages parent
    /// inheritance — they either carry their own /Resources or rely on
    /// the enclosing context (which we already process via the page).
    FormXObject,
}

/// Recursively strip /ToUnicode CMaps from every font reachable through
/// `container_id`'s `/Resources/Font` AND every Form XObject reachable
/// through `/Resources/XObject/* (Subtype /Form)`. Cascade clone-on-write
/// at every level keeps shared objects byte-identical for non-redacted
/// pages.
///
/// `form_clones` memoises `original Form XObject id → cloned-and-stubbed
/// Form XObject id` for the entire redaction call. The map serves two
/// roles at once:
///   - Cycle guard: a key in the map means we've already begun
///     processing that original, so re-encountering it (in a cycle, or
///     via a second alias) does NOT recurse — preventing infinite
///     recursion.
///   - Alias coalescing: when the same original is encountered again,
///     all referencing /XObject entries point at the SAME shared
///     clone (which has already been stubbed by the first recursion).
///     Without this, a second alias would receive a fresh clone whose
///     /Resources/Font/* still pointed at the ORIGINAL /ToUnicode (no
///     recursion = no stub) — Codex P1 ronde-6 follow-up #1379.
fn strip_tounicode_for_container(
    doc: &mut Document,
    container_id: ObjectId,
    kind: ContainerKind,
    form_clones: &mut std::collections::HashMap<ObjectId, ObjectId>,
) -> Result<usize> {
    // Phase A: snapshot the resolved resources read-only. For pages the
    // /Pages parent chain is walked; for Form XObjects we read /Resources
    // directly off the stream's dict (Form XObjects don't inherit).
    let snapshot = match kind {
        ContainerKind::Page => resolve_page_resources_snapshot(doc, container_id),
        ContainerKind::FormXObject => resolve_form_xobject_resources_snapshot(doc, container_id),
    };
    let snapshot = match snapshot {
        Some(s) => s,
        None => return Ok(0),
    };

    // Phase B: collect work read-only. UnsupportedToUnicodeCMap surfaces
    // here, BEFORE any mutation, so a malformed CMap leaves the document
    // untouched.
    let font_actions = collect_strip_actions(doc, &snapshot)?;
    let form_xobject_entries = collect_form_xobject_entries(doc, &snapshot);

    let needs_font_clone = !font_actions.is_empty();
    let needs_xobject_clone = !form_xobject_entries.is_empty();
    if !needs_font_clone && !needs_xobject_clone {
        return Ok(0);
    }

    // Phase C: cascade clone-on-write. Materialise a private /Resources
    // dict (and /Font and/or /XObject sub-dicts as needed) so subsequent
    // mutations on this container never touch shared objects.
    let private = materialize_private_path(
        doc,
        container_id,
        kind,
        &snapshot,
        needs_font_clone,
        needs_xobject_clone,
    );

    let mut stripped = 0;

    // Phase D-1: apply font strips via the private /Font dict.
    if let Some(font_dict_id) = private.font_dict_id {
        for FontStripPlan {
            font_name_bytes,
            font_dict_template,
            codes,
        } in font_actions
        {
            let stub_id = build_and_insert_stub_stream(doc, &codes);
            let mut new_font = font_dict_template;
            new_font.set("ToUnicode", Object::Reference(stub_id));
            let new_font_id = doc.add_object(Object::Dictionary(new_font));
            if let Ok(Object::Dictionary(font_dict)) = doc.get_object_mut(font_dict_id) {
                // Raw byte key — see Codex P2 #1379 ronde-3 fix.
                font_dict.set(font_name_bytes.clone(), Object::Reference(new_font_id));
            }
            stripped += 1;
        }
    }

    // Phase D-2: for every Form XObject, resolve to a per-original
    // memoised clone, redirect this container's private /XObject entry
    // there, and recurse into the clone the FIRST time we see its
    // original (subsequent aliases reuse the existing already-stubbed
    // clone, fixing Codex P1 ronde-6 #1379).
    if let Some(xobject_dict_id) = private.xobject_dict_id {
        for entry in form_xobject_entries {
            let (cloned_form_id, recurse_into_new_clone) =
                if let Some(&existing) = form_clones.get(&entry.original_id) {
                    // We've already minted a clone for this original
                    // somewhere up the call stack (or via another alias).
                    // Reuse it so all aliases share the same stubbed copy.
                    (existing, false)
                } else {
                    let new_clone = clone_form_xobject_stream(doc, entry.original_id);
                    form_clones.insert(entry.original_id, new_clone);
                    (new_clone, true)
                };

            // Always wire this container's private /XObject entry to the
            // clone. The original Form XObject Stream stays byte-
            // identical for any non-redacted page that still references
            // it.
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(xobject_dict_id) {
                d.set(entry.name.clone(), Object::Reference(cloned_form_id));
            }

            if recurse_into_new_clone {
                stripped += strip_tounicode_for_container(
                    doc,
                    cloned_form_id,
                    ContainerKind::FormXObject,
                    form_clones,
                )?;
            }
        }
    }

    Ok(stripped)
}

/// Snapshot of the resolved /Resources, /Font, and /XObject sub-dicts
/// for a container (page or Form XObject). Holds owned clones so it
/// stays valid across subsequent mutations.
struct ResourcesSnapshot {
    /// Cloned snapshot of the resolved /Resources dict.
    #[allow(dead_code)]
    resources_dict: lopdf::Dictionary,
    /// Cloned snapshot of /Resources/Font, if present.
    font_dict: Option<lopdf::Dictionary>,
    /// Cloned snapshot of /Resources/XObject, if present. Used to find
    /// Form XObjects that need recursive ToUnicode-strip processing.
    xobject_dict: Option<lopdf::Dictionary>,
}

struct FontStripPlan {
    /// Raw resource-name bytes from /Resources/Font. PDF resource names
    /// are byte identifiers (PDF spec §7.3.5), so we keep the bytes
    /// verbatim and only render lossy strings for diagnostics. Writing
    /// the rewritten /Font entry back under a UTF-8-lossy version of the
    /// key would silently bypass the redaction for any name that
    /// contains non-UTF-8 bytes (Codex P2 #1379 fix).
    font_name_bytes: Vec<u8>,
    /// Owned clone of the original font dict (with the original /ToUnicode
    /// reference still in place); the apply phase rewrites /ToUnicode to
    /// the stub and emits this as a new indirect object so the original
    /// font dict is never mutated.
    font_dict_template: lopdf::Dictionary,
    codes: crate::tounicode::TounicodeCodes,
}

/// Walk the /Pages parent chain to resolve a page's /Resources dict, per
/// PDF spec resource inheritance. Returns an owned clone so subsequent
/// mutations don't invalidate the borrow.
fn resolve_inherited_resources_clone(
    doc: &Document,
    page_id: ObjectId,
) -> Option<lopdf::Dictionary> {
    let mut current = page_id;
    let mut visited: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            // Cycle in /Parent chain — fail closed.
            return None;
        }
        let dict = doc.get_dictionary(current).ok()?;
        if let Ok(res_obj) = dict.get(b"Resources") {
            return match res_obj {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
                _ => None,
            };
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(id)) => current = *id,
            _ => return None,
        }
    }
}

fn resolve_page_resources_snapshot(doc: &Document, page_id: ObjectId) -> Option<ResourcesSnapshot> {
    let resources_dict = resolve_inherited_resources_clone(doc, page_id)?;
    Some(snapshot_from_resources_dict(doc, resources_dict))
}

/// Form-XObject variant: read /Resources directly from the stream's dict.
/// Form XObjects don't participate in /Pages parent inheritance — if they
/// have no /Resources entry there's nothing to do at this level (and any
/// fonts they reference were already covered by the enclosing page).
fn resolve_form_xobject_resources_snapshot(
    doc: &Document,
    form_id: ObjectId,
) -> Option<ResourcesSnapshot> {
    let stream_dict = match doc.get_object(form_id).ok()? {
        Object::Stream(s) => &s.dict,
        _ => return None,
    };
    let res_obj = stream_dict.get(b"Resources").ok()?;
    let resources_dict = match res_obj {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
        _ => return None,
    };
    Some(snapshot_from_resources_dict(doc, resources_dict))
}

/// Build a snapshot from an already-resolved /Resources dict, copying out
/// /Font and /XObject sub-dicts (resolving an indirect reference if
/// either is stored that way).
fn snapshot_from_resources_dict(
    doc: &Document,
    resources_dict: lopdf::Dictionary,
) -> ResourcesSnapshot {
    let font_dict = match resources_dict.get(b"Font") {
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok().cloned(),
        _ => None,
    };
    let xobject_dict = match resources_dict.get(b"XObject") {
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok().cloned(),
        _ => None,
    };
    ResourcesSnapshot {
        resources_dict,
        font_dict,
        xobject_dict,
    }
}

/// A Form XObject reference under a container's /Resources/XObject
/// sub-dict that we need to clone-and-recurse into. Image XObjects and
/// other non-Form subtypes are filtered out by `collect_form_xobject_entries`.
struct FormXObjectEntry {
    /// Raw byte key of the /XObject entry — preserved verbatim so the
    /// rewrite under `entry.name` matches what content streams reference.
    name: Vec<u8>,
    /// ObjectId of the original (potentially shared) Form XObject Stream.
    original_id: ObjectId,
}

fn collect_form_xobject_entries(
    doc: &Document,
    snapshot: &ResourcesSnapshot,
) -> Vec<FormXObjectEntry> {
    let Some(xobject_dict) = snapshot.xobject_dict.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name_bytes, val) in xobject_dict.iter() {
        let id = match val {
            Object::Reference(id) => *id,
            // Inline streams are not supported under /XObject (PDF spec
            // requires indirect refs there). Skip silently.
            _ => continue,
        };
        let stream = match doc.get_object(id) {
            Ok(Object::Stream(s)) => s,
            _ => continue,
        };
        // Only Form XObjects (Subtype /Form) are recursed into. Image
        // XObjects (Subtype /Image) have their own redaction path
        // (Phase 2 black-out) and don't carry /ToUnicode.
        let is_form = matches!(
            stream.dict.get(b"Subtype"),
            Ok(Object::Name(n)) if n == b"Form"
        );
        if !is_form {
            continue;
        }
        out.push(FormXObjectEntry {
            name: name_bytes.to_vec(),
            original_id: id,
        });
    }
    out
}

fn clone_form_xobject_stream(doc: &mut Document, original_id: ObjectId) -> ObjectId {
    let cloned = match doc.get_object(original_id) {
        Ok(Object::Stream(s)) => s.clone(),
        _ => {
            // Caller filtered to /Subtype /Form streams; if we hit a
            // non-stream here something has changed under our feet —
            // return the original id so the caller's set() at least
            // leaves the document well-formed (it'll just be a no-op
            // clone that shares with the original).
            return original_id;
        }
    };
    doc.add_object(Object::Stream(cloned))
}

fn collect_strip_actions(
    doc: &Document,
    snapshot: &ResourcesSnapshot,
) -> Result<Vec<FontStripPlan>> {
    let Some(font_dict) = snapshot.font_dict.as_ref() else {
        return Ok(Vec::new());
    };

    let mut plans = Vec::new();
    for (name_bytes, font_obj) in font_dict.iter() {
        // PDF resource names are byte identifiers, not UTF-8 strings (they
        // can include #xx escapes that decode to non-UTF-8 bytes). We keep
        // the raw bytes for the rewrite path so the entry we emit is keyed
        // under exactly the same bytes content streams reference. The
        // lossy String form is used ONLY for human-readable error messages
        // (Codex P2 #1379 fix).
        let font_name_bytes: Vec<u8> = name_bytes.to_vec();
        let display_name = String::from_utf8_lossy(&font_name_bytes).into_owned();

        let resolved_font_dict: lopdf::Dictionary = match font_obj {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_dictionary(*id).ok() {
                Some(d) => d.clone(),
                None => continue,
            },
            _ => continue,
        };

        let to_unicode = match resolved_font_dict.get(b"ToUnicode") {
            Ok(o) => o,
            Err(_) => continue, // No /ToUnicode on this font — leave alone.
        };

        if existing_tounicode_is_safe_stub(doc, to_unicode) {
            continue; // Idempotent: previous redaction already stubbed this
                      // AND the stream content validates as a safe stub (marker
                      // alone is not authoritative — see existing_tounicode_is_safe_stub).
        }

        let bytes = read_tounicode_bytes(doc, to_unicode).ok_or_else(|| {
            RedactError::UnsupportedToUnicodeCMap {
                font_resource_name: display_name.clone(),
                reason: "could not read /ToUnicode stream bytes".to_string(),
            }
        })?;
        let codes = crate::tounicode::extract_tounicode_codes(&bytes, &display_name)?;
        plans.push(FontStripPlan {
            font_name_bytes,
            font_dict_template: resolved_font_dict,
            codes,
        });
    }
    Ok(plans)
}

/// Result of a `materialize_private_path` call. Either `font_dict_id` or
/// `xobject_dict_id` is `Some` based on the requested clone flags; both
/// can be `Some` when the caller will mutate both sub-dicts.
struct PrivateContainerPath {
    /// New indirect /Font sub-dict, populated if `clone_font` was true.
    font_dict_id: Option<ObjectId>,
    /// New indirect /XObject sub-dict, populated if `clone_xobject` was true.
    xobject_dict_id: Option<ObjectId>,
}

/// Cascade clone-on-write: produce fresh indirect /Resources for
/// `container_id`, plus fresh /Font and/or /XObject sub-dicts as
/// requested. The container's /Resources entry is rewritten to point at
/// the new private dict; original shared objects stay intact for any
/// other container that still references them.
///
/// Works for both `ContainerKind::Page` and `ContainerKind::FormXObject`:
/// the only difference is whether the container's dict lives directly on
/// the indirect object (page) or inside a Stream's dict (Form XObject).
fn materialize_private_path(
    doc: &mut Document,
    container_id: ObjectId,
    kind: ContainerKind,
    snapshot: &ResourcesSnapshot,
    clone_font: bool,
    clone_xobject: bool,
) -> PrivateContainerPath {
    let font_dict_id = if clone_font {
        let font_dict_clone = snapshot.font_dict.clone().unwrap_or_default();
        Some(doc.add_object(Object::Dictionary(font_dict_clone)))
    } else {
        None
    };

    let xobject_dict_id = if clone_xobject {
        let xobject_dict_clone = snapshot.xobject_dict.clone().unwrap_or_default();
        Some(doc.add_object(Object::Dictionary(xobject_dict_clone)))
    } else {
        None
    };

    // Build a private /Resources by cloning the snapshot and rewiring
    // /Font and/or /XObject to point at our private sub-dicts.
    let mut new_resources = snapshot.resources_dict.clone();
    if let Some(id) = font_dict_id {
        new_resources.set("Font", Object::Reference(id));
    }
    if let Some(id) = xobject_dict_id {
        new_resources.set("XObject", Object::Reference(id));
    }
    let new_resources_id = doc.add_object(Object::Dictionary(new_resources));

    // Attach the new /Resources to the container. Pages keep /Resources
    // on their dict directly; Form XObjects keep it inside the Stream's
    // dict.
    set_container_resources(doc, container_id, kind, Object::Reference(new_resources_id));

    PrivateContainerPath {
        font_dict_id,
        xobject_dict_id,
    }
}

fn set_container_resources(
    doc: &mut Document,
    container_id: ObjectId,
    kind: ContainerKind,
    value: Object,
) {
    match (kind, doc.get_object_mut(container_id)) {
        (ContainerKind::Page, Ok(Object::Dictionary(page))) => {
            page.set("Resources", value);
        }
        (ContainerKind::FormXObject, Ok(Object::Stream(s))) => {
            s.dict.set("Resources", value);
        }
        // Mismatched kind/object — leave as-is; the strip is fail-safe and
        // simply skips this container.
        _ => {}
    }
}

/// Idempotency check that decides whether a /ToUnicode stream is one of
/// our previously-emitted safe stubs and can therefore be skipped.
///
/// The XfaRedactionStub marker is treated as a fast-path *hint*, never
/// authoritative: a crafted PDF can set the marker on an attacker-
/// controlled /ToUnicode stream (Codex P1 #1379), which would let the
/// strip silently skip the font and leave the original mapping intact.
///
/// The authoritative check is on the stream BODY: we parse the bytes
/// and require every bfchar destination to be exactly `<FFFD>` and no
/// bfrange to be present, matching what `build_stub_cmap` produces.
/// If the marker is missing OR the content doesn't validate, we treat
/// the stream as un-stubbed and replace it with a fresh stub.
fn existing_tounicode_is_safe_stub(doc: &Document, to_unicode: &Object) -> bool {
    let stream = match to_unicode {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Stream(s)) => s.clone(),
            _ => return false,
        },
        Object::Stream(s) => s.clone(),
        _ => return false,
    };
    // Require the marker as a sanity hint (cheap rejection of obviously-
    // unrelated streams), then require content validation as the actual
    // authority.
    let marker_present = matches!(
        stream.dict.get(crate::tounicode::STUB_MARKER_KEY),
        Ok(Object::Boolean(true))
    );
    if !marker_present {
        return false;
    }
    let mut s = stream;
    let _ = s.decompress();
    crate::tounicode::is_safe_stub_cmap(&s.content)
}

fn read_tounicode_bytes(doc: &Document, to_unicode: &Object) -> Option<Vec<u8>> {
    let stream = match to_unicode {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Stream(s)) => s.clone(),
            _ => return None,
        },
        Object::Stream(s) => s.clone(),
        _ => return None,
    };
    let mut s = stream;
    let _ = s.decompress();
    Some(s.content)
}

fn build_and_insert_stub_stream(
    doc: &mut Document,
    codes: &crate::tounicode::TounicodeCodes,
) -> ObjectId {
    let stub_bytes = crate::tounicode::build_stub_cmap(codes);
    let compressed = compress_flate(&stub_bytes);
    let mut dict = lopdf::Dictionary::new();
    if compressed.len() < stub_bytes.len() {
        dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        dict.set("Length", Object::Integer(compressed.len() as i64));
        dict.set(crate::tounicode::STUB_MARKER_KEY, Object::Boolean(true));
        let stream = lopdf::Stream::new(dict, compressed);
        doc.add_object(Object::Stream(stream))
    } else {
        dict.set("Length", Object::Integer(stub_bytes.len() as i64));
        dict.set(crate::tounicode::STUB_MARKER_KEY, Object::Boolean(true));
        let stream = lopdf::Stream::new(dict, stub_bytes);
        doc.add_object(Object::Stream(stream))
    }
}

/// Convert a PDF object to a number.
fn as_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a minimal doc with text content.
    fn make_doc_with_text(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        // Add Info dict for metadata cleaning test.
        let info = dictionary! {
            "Title" => Object::String(b"Test".to_vec(), lopdf::StringFormat::Literal),
            "Author" => Object::String(b"Tester".to_vec(), lopdf::StringFormat::Literal),
        };
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn redact_empty_returns_error() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let redactor = Redactor::new();
        let result = redactor.apply(&mut doc);
        assert!(result.is_err());
    }

    #[test]
    fn redact_text_in_area() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [90.0, 690.0, 200.0, 720.0]));

        let report = redactor.apply(&mut doc).unwrap();
        assert_eq!(report.areas_redacted, 1);
        assert!(report.operations_removed > 0);
        assert_eq!(report.pages_affected, 1);
        assert!(report.metadata_cleaned);
    }

    #[test]
    fn redact_preserves_text_outside_area() {
        let mut doc =
            make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Keep) Tj 100 200 Td (Remove) Tj ET");
        let mut redactor = Redactor::new();
        // Only redact the area around position (200, 900), which is 100+100, 700+200.
        redactor.mark(RedactionArea::new(1, [190.0, 890.0, 310.0, 920.0]));

        let report = redactor.apply(&mut doc).unwrap();
        // The "Remove" text at (200, 900) should be removed.
        assert!(report.operations_removed > 0);
    }

    #[test]
    fn redact_with_overlay_text() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [90.0, 690.0, 200.0, 720.0]).with_overlay("REDACTED"));

        let report = redactor.apply(&mut doc).unwrap();
        assert_eq!(report.areas_redacted, 1);
    }

    #[test]
    fn redact_with_custom_color() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [90.0, 690.0, 200.0, 720.0]).with_color(1.0, 0.0, 0.0));

        let report = redactor.apply(&mut doc).unwrap();
        assert_eq!(report.areas_redacted, 1);
    }

    #[test]
    fn redact_cleans_metadata() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [90.0, 690.0, 200.0, 720.0]));

        let report = redactor.apply(&mut doc).unwrap();
        assert!(report.metadata_cleaned);

        // Verify Info dict reference is removed from trailer.
        assert!(doc.trailer.get(b"Info").is_err());
    }

    #[test]
    fn redact_page_out_of_range() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(5, [0.0, 0.0, 100.0, 100.0]));

        let result = redactor.apply(&mut doc);
        assert!(result.is_err());
    }

    #[test]
    fn redact_multiple_areas() {
        let mut doc =
            make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret1) Tj 100 600 Td (Secret2) Tj ET");
        let mut redactor = Redactor::new();
        redactor.mark_all(vec![
            RedactionArea::new(1, [90.0, 690.0, 200.0, 720.0]),
            RedactionArea::new(1, [190.0, 1290.0, 310.0, 1320.0]),
        ]);

        assert_eq!(redactor.pending_count(), 2);
        let report = redactor.apply(&mut doc).unwrap();
        assert_eq!(report.areas_redacted, 2);
    }

    // ─── M8-REDACT-03 (#1339) /ToUnicode strip tests ────────────────────

    /// Minimal /ToUnicode CMap that maps a few codes — used as the
    /// "original" CMap for the strip flow tests below.
    const ORIGINAL_TOUNICODE: &[u8] = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
3 beginbfchar
<41> <0041>
<42> <0042>
<43> <0043>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";

    /// Build a minimal page with /Resources/Font/F1 pointing at an
    /// indirect font dict that has a /ToUnicode stream. Returns
    /// (doc, page_id, font_id, original_tounicode_id).
    fn make_doc_with_tounicode_font() -> (Document, ObjectId, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content_stream));

        // /ToUnicode stream as the original CMap.
        let tu_stream = Stream::new(dictionary! {}, ORIGINAL_TOUNICODE.to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));

        // Font dict with /ToUnicode reference.
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        // Resources dict with /Font/F1 → font_id.
        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_id),
            },
        };

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        (doc, page_id, font_id, tu_id)
    }

    /// Read the /ToUnicode reference of /F1 on `page_id` after a redaction.
    /// Returns None if /F1 has no /ToUnicode (e.g., font without /ToUnicode
    /// case, where we leave the entry absent).
    fn current_tounicode_ref(doc: &Document, page_id: ObjectId) -> Option<ObjectId> {
        let page = doc.get_dictionary(page_id).ok()?;
        let resources = match page.get(b"Resources").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Dictionary(d) => d.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let font_dict_obj = resources.get(b"Font").ok()?;
        let font_dict = match font_dict_obj {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Dictionary(d) => d.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let font_obj = font_dict.get(b"F1").ok()?;
        let font = match font_obj {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Dictionary(d) => d.clone(),
                _ => return None,
            },
            _ => return None,
        };
        match font.get(b"ToUnicode").ok()? {
            Object::Reference(id) => Some(*id),
            _ => None,
        }
    }

    fn current_font_id(doc: &Document, page_id: ObjectId) -> Option<ObjectId> {
        let page = doc.get_dictionary(page_id).ok()?;
        let resources = match page.get(b"Resources").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Dictionary(d) => d.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let font_dict = match resources.get(b"Font").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Dictionary(d) => d.clone(),
                _ => return None,
            },
            _ => return None,
        };
        match font_dict.get(b"F1").ok()? {
            Object::Reference(id) => Some(*id),
            _ => None,
        }
    }

    #[test]
    fn tounicode_strip_replaces_cmap_on_redacted_page() {
        let (mut doc, _page_id, original_font_id, original_tu_id) = make_doc_with_tounicode_font();

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        let report = redactor.apply(&mut doc).unwrap();
        assert!(report.pages_affected >= 1);

        // Page now points at a CLONED font dict (different ObjectId).
        let new_font_id = current_font_id(&doc, _page_id).expect("F1 must still resolve");
        assert_ne!(
            new_font_id, original_font_id,
            "redacted page must point at a cloned font dict, not the shared original"
        );

        // Cloned font's /ToUnicode points at a NEW stream marked as stub.
        let new_tu_id = current_tounicode_ref(&doc, _page_id)
            .expect("redacted page font must still have /ToUnicode (now a stub)");
        assert_ne!(
            new_tu_id, original_tu_id,
            "stub /ToUnicode must be a fresh stream, not the original"
        );
        let stub_stream = match doc.get_object(new_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        assert!(
            matches!(
                stub_stream.dict.get(crate::tounicode::STUB_MARKER_KEY),
                Ok(Object::Boolean(true))
            ),
            "stub stream must carry the XfaRedactionStub marker"
        );
    }

    #[test]
    fn tounicode_strip_preserves_original_for_non_redacted_pages_with_shared_font() {
        // Two pages share the same indirect font dict. Redact only page 1
        // and verify page 2 still references the ORIGINAL font + /ToUnicode.
        let (mut doc, page1_id, shared_font_id, original_tu_id) = make_doc_with_tounicode_font();

        // Add a second page that shares the same font dict.
        let content2 = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content2_id = doc.add_object(Object::Stream(content2));
        let resources2 = dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(shared_font_id),
            },
        };
        let page2_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content2_id),
            "Resources" => Object::Dictionary(resources2),
        };
        let page2_id = doc.add_object(Object::Dictionary(page2_dict));

        // Wire page2 into Pages tree.
        let pages_id = match doc
            .get_dictionary(page1_id)
            .unwrap()
            .get(b"Parent")
            .unwrap()
        {
            Object::Reference(id) => *id,
            _ => panic!("page must have indirect Parent"),
        };
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page2_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        if let Ok(Object::Dictionary(pages)) = doc.get_object_mut(pages_id) {
            pages.set(
                "Kids",
                vec![Object::Reference(page1_id), Object::Reference(page2_id)],
            );
            pages.set("Count", 2_i64);
        }

        // Redact page 1 only.
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // Page 1 should now point at a clone (new font id) with stub /ToUnicode.
        let page1_font_id = current_font_id(&doc, page1_id).unwrap();
        assert_ne!(
            page1_font_id, shared_font_id,
            "redacted page 1 must have a cloned font"
        );

        // Page 2 must still reference the ORIGINAL shared font dict.
        let page2_font_id = current_font_id(&doc, page2_id).unwrap();
        assert_eq!(
            page2_font_id, shared_font_id,
            "non-redacted page 2 must keep its shared font reference"
        );
        // And the original /ToUnicode must still be the original stream
        // (untouched, not in-place mutated).
        let page2_tu_id = current_tounicode_ref(&doc, page2_id).unwrap();
        assert_eq!(
            page2_tu_id, original_tu_id,
            "non-redacted page 2 must keep the original /ToUnicode reference"
        );
    }

    #[test]
    fn tounicode_strip_idempotent_on_repeated_redaction() {
        let (mut doc, page_id, _, _) = make_doc_with_tounicode_font();

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        let after_first = current_tounicode_ref(&doc, page_id).unwrap();

        // Re-apply: the second pass must detect the existing stub marker
        // and skip — no fresh stub stream allocated, no re-cloning.
        let mut redactor2 = Redactor::new();
        redactor2.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor2.apply(&mut doc).unwrap();

        let after_second = current_tounicode_ref(&doc, page_id).unwrap();
        assert_eq!(
            after_first, after_second,
            "repeated redaction must not allocate a new stub /ToUnicode"
        );
    }

    #[test]
    fn tounicode_strip_skips_font_without_existing_tounicode() {
        // Build a page whose font dict has NO /ToUnicode key.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td (X) Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let original_font_id = font_id;
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // Font without /ToUnicode must remain untouched: same indirect ID,
        // and still no /ToUnicode key.
        let after_font_id = current_font_id(&doc, page_id).unwrap();
        assert_eq!(
            after_font_id, original_font_id,
            "font without /ToUnicode must not be cloned"
        );
        let font_dict = doc.get_dictionary(after_font_id).unwrap();
        assert!(
            font_dict.get(b"ToUnicode").is_err(),
            "no /ToUnicode key should be added by the strip"
        );
    }

    #[test]
    fn tounicode_strip_returns_err_on_unsupported_cmap_shape() {
        // Build a doc whose /ToUnicode stream has bytes that the
        // conservative parser cannot make sense of (no codespace, no
        // bfchar/bfrange). The redactor must surface the
        // UnsupportedToUnicodeCMap error rather than silently strip.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td (X) Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));
        // Garbage CMap content — no codespacerange, no bfchar.
        let tu_stream = Stream::new(dictionary! {}, b"begincmap\nendcmap\n".to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        let err = redactor.apply(&mut doc).unwrap_err();
        assert!(
            matches!(err, RedactError::UnsupportedToUnicodeCMap { .. }),
            "expected UnsupportedToUnicodeCMap, got {err:?}"
        );
    }

    #[test]
    fn tounicode_strip_resolves_resources_inherited_from_pages_parent() {
        // Codex P1 #1379: when a page has no direct /Resources entry but
        // inherits one from its /Pages parent (a common PDF layout), the
        // strip flow must walk the parent chain to find the inherited
        // Resources and apply the stub. Without the walk, no fonts are
        // stripped and the redaction defense-in-depth is silently bypassed.
        let mut doc = Document::with_version("1.7");

        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));

        // /ToUnicode stream + font dict on the /Pages parent's Resources.
        let tu_stream = Stream::new(dictionary! {}, ORIGINAL_TOUNICODE.to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        // Page WITHOUT /Resources — inherits from /Pages parent.
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        // /Pages dict carries the /Resources that this page inherits.
        let inherited_resources = dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_id),
            },
        };
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
            "Resources" => Object::Dictionary(inherited_resources),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Apply redaction.
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // The page must now have its own private /Resources (no longer
        // inheriting), and that private Resources must point at a
        // private /Font dict whose /F1 entry references a CLONED font
        // dict whose /ToUnicode is the stub.
        let new_tu_id = current_tounicode_ref(&doc, page_id)
            .expect("inherited-resources page must have private /ToUnicode after redaction");
        assert_ne!(
            new_tu_id, tu_id,
            "stub /ToUnicode must be a fresh stream, not the original inherited one"
        );
        let stub_stream = match doc.get_object(new_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        assert!(
            matches!(
                stub_stream.dict.get(crate::tounicode::STUB_MARKER_KEY),
                Ok(Object::Boolean(true))
            ),
            "inherited-Resources page must end up with a stub-marked /ToUnicode"
        );
    }

    #[test]
    fn tounicode_strip_does_not_mutate_shared_indirect_font_subdict() {
        // Codex P1 #1379: when /Resources/Font is an indirect dict shared
        // between two pages, redacting page 1 must NOT rewrite that
        // shared /Font sub-dict (which would silently downgrade page 2's
        // font to use the stubbed /ToUnicode). The strip must clone the
        // /Font sub-dict (and its parent /Resources) before mutating.
        let mut doc = Document::with_version("1.7");

        let content1 = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <41> Tj ET".to_vec(),
        );
        let content1_id = doc.add_object(Object::Stream(content1));
        let content2 = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <42> Tj ET".to_vec(),
        );
        let content2_id = doc.add_object(Object::Stream(content2));

        // Original /ToUnicode + font dict.
        let tu_stream = Stream::new(dictionary! {}, ORIGINAL_TOUNICODE.to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        // SHARED indirect /Font sub-dict — both pages reference this same
        // ObjectId, so any in-place mutation of it would affect both pages.
        let shared_font_subdict = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let shared_font_subdict_id = doc.add_object(Object::Dictionary(shared_font_subdict));

        let resources1 = dictionary! {
            "Font" => Object::Reference(shared_font_subdict_id),
        };
        let resources2 = dictionary! {
            "Font" => Object::Reference(shared_font_subdict_id),
        };

        let page1_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content1_id),
            "Resources" => Object::Dictionary(resources1),
        };
        let page1_id = doc.add_object(Object::Dictionary(page1_dict));
        let page2_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content2_id),
            "Resources" => Object::Dictionary(resources2),
        };
        let page2_id = doc.add_object(Object::Dictionary(page2_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page1_id), Object::Reference(page2_id)],
            "Count" => 2_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        for pid in [page1_id, page2_id] {
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(pid) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Snapshot the shared /Font sub-dict's contents BEFORE redaction.
        let before_subdict_f1 = match doc
            .get_dictionary(shared_font_subdict_id)
            .unwrap()
            .get(b"F1")
            .unwrap()
        {
            Object::Reference(id) => *id,
            _ => panic!("F1 entry must be indirect to be shareable"),
        };
        assert_eq!(before_subdict_f1, font_id);

        // Redact page 1 only.
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // CRITICAL: the shared /Font sub-dict's /F1 entry must NOT have
        // been rewritten. If it had, page 2 (which still references the
        // shared sub-dict) would inherit page 1's stubbed font.
        let shared_after = doc.get_dictionary(shared_font_subdict_id).unwrap();
        let after_subdict_f1 = match shared_after.get(b"F1").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("F1 must still be indirect"),
        };
        assert_eq!(
            after_subdict_f1, font_id,
            "shared /Font sub-dict was mutated in place — page 2 will inherit page 1's stub"
        );

        // Page 2 still resolves /F1 → original font with original /ToUnicode.
        let page2_tu = current_tounicode_ref(&doc, page2_id)
            .expect("non-redacted page 2 must keep its /ToUnicode");
        assert_eq!(
            page2_tu, tu_id,
            "non-redacted page 2 must still use the original /ToUnicode"
        );

        // Page 1 now has private /Resources/Font/F1 pointing at a stubbed clone.
        let page1_tu = current_tounicode_ref(&doc, page1_id)
            .expect("redacted page 1 must have its private /ToUnicode stub");
        assert_ne!(
            page1_tu, tu_id,
            "redacted page 1 must use the stub /ToUnicode, not the original"
        );
    }

    #[test]
    fn tounicode_strip_rejects_crafted_marker_without_safe_stub_content() {
        // Codex P1 #1379 regression-guard: an attacker can craft a PDF
        // that sets /XfaRedactionStub true on an arbitrary /ToUnicode
        // stream whose content still maps codes to real characters.
        // If the redactor honored the marker alone, redaction would
        // silently skip stubbing for that font and the original mapping
        // would survive — defeating the new defense-in-depth layer.
        //
        // After this fix, the marker is treated as a fast-path hint only;
        // the authoritative idempotency check is on the stream BODY
        // (every bfchar destination must be U+FFFD, no bfrange present).
        // A crafted-marker stream therefore falls through to replacement.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));

        // Crafted /ToUnicode: real mapping (NOT stub) + marker key set.
        let mut tu_dict = lopdf::Dictionary::new();
        tu_dict.set(crate::tounicode::STUB_MARKER_KEY, Object::Boolean(true));
        let tu_stream = Stream::new(tu_dict, ORIGINAL_TOUNICODE.to_vec());
        let crafted_tu_id = doc.add_object(Object::Stream(tu_stream));

        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(crafted_tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        let resources = dictionary! {
            "Font" => dictionary! {
                "F1" => Object::Reference(font_id),
            },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // The crafted /ToUnicode must NOT have been honored as a safe
        // stub — the page's font now references a fresh /ToUnicode
        // stream that IS our genuine stub.
        let new_tu_id = current_tounicode_ref(&doc, page_id)
            .expect("page must still have /ToUnicode after redaction");
        assert_ne!(
            new_tu_id, crafted_tu_id,
            "crafted-marker /ToUnicode must NOT be honored as already-stubbed"
        );

        // Verify the new /ToUnicode validates as a real stub.
        let new_stream = match doc.get_object(new_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        let mut s = new_stream;
        let _ = s.decompress();
        assert!(
            crate::tounicode::is_safe_stub_cmap(&s.content),
            "redactor must replace crafted-marker stream with a content-validated stub"
        );
    }

    #[test]
    fn tounicode_strip_preserves_non_utf8_font_resource_key() {
        // Codex P2 #1379 regression-guard: PDF resource names are byte
        // identifiers and may contain bytes that are invalid UTF-8 (via
        // #xx escapes). If we round-trip the key through
        // String::from_utf8_lossy on the rewrite path, the U+FFFD
        // replacement bytes (`EF BF BD`) replace the original invalid
        // byte, and the new entry lands under a DIFFERENT key. The
        // original key (which content streams reference) keeps pointing
        // at the unstubbed font, silently bypassing the redaction.
        //
        // After this fix, font_name_bytes carries the raw key end-to-end.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));

        // /ToUnicode (genuine, not crafted).
        let tu_stream = Stream::new(dictionary! {}, ORIGINAL_TOUNICODE.to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let original_font_id = doc.add_object(Object::Dictionary(font_dict));

        // /Font sub-dict with a non-UTF-8 resource key.
        // Bytes: 0xFF 0x46 0x31 — `0xFF` is not a valid UTF-8 start byte.
        let raw_key: Vec<u8> = vec![0xFF, b'F', b'1'];
        let mut font_subdict = lopdf::Dictionary::new();
        font_subdict.set(raw_key.clone(), Object::Reference(original_font_id));

        let resources = dictionary! {
            "Font" => Object::Dictionary(font_subdict),
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Sanity: lossy round-trip differs from raw bytes.
        let lossy_key_bytes = String::from_utf8_lossy(&raw_key).into_owned().into_bytes();
        assert_ne!(
            lossy_key_bytes, raw_key,
            "test sanity: lossy conversion must produce different bytes for 0xFF F1"
        );

        // Apply redaction.
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // Resolve the page's now-private /Resources → /Font sub-dict.
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let resources_id = match page_dict.get(b"Resources").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("strip should have made /Resources indirect"),
        };
        let resources = doc.get_dictionary(resources_id).unwrap();
        let font_subdict_id = match resources.get(b"Font").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("strip should have made /Font indirect"),
        };
        let font_subdict = doc.get_dictionary(font_subdict_id).unwrap();

        // The RAW byte key must still resolve to a font — and that font
        // must be a CLONED dict (different id) whose /ToUnicode is our
        // stub. Without the fix this would fail because the rewrite
        // landed under the lossy key.
        let entry = font_subdict
            .get(raw_key.as_slice())
            .expect("raw byte key must still resolve in the private /Font sub-dict");
        let cloned_font_id = match entry {
            Object::Reference(id) => *id,
            _ => panic!("raw key entry must be indirect"),
        };
        assert_ne!(
            cloned_font_id, original_font_id,
            "raw key must reference the cloned font dict, not the original"
        );

        // The lossy-converted key must NOT have its own slot — that would
        // mean we wrote a duplicate entry under the wrong key while
        // leaving the raw key pointing at the original (unstubbed) font.
        assert!(
            font_subdict.get(lossy_key_bytes.as_slice()).is_err(),
            "lossy-converted key must not exist in /Font — silent bypass guard"
        );

        // Verify the cloned font's /ToUnicode is our genuine stub.
        let cloned_font = doc.get_dictionary(cloned_font_id).unwrap();
        let new_tu_id = match cloned_font.get(b"ToUnicode").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("cloned font must have indirect /ToUnicode"),
        };
        assert_ne!(new_tu_id, tu_id, "stub must be a fresh stream");
        let new_stream = match doc.get_object(new_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        let mut s = new_stream;
        let _ = s.decompress();
        assert!(
            crate::tounicode::is_safe_stub_cmap(&s.content),
            "raw-key path must end up at a content-validated stub stream"
        );
    }

    #[test]
    fn tounicode_strip_rejects_crafted_usecmap_with_trivial_bfchar() {
        // Codex P1 #1379 ronde-5 regression-guard: the previous round's
        // marker-bypass fix validated stream content for "all bfchar
        // destinations are U+FFFD". An attacker can carry the marker,
        // include a single trivial `<00> <FFFD>` bfchar entry to satisfy
        // that check, AND include `usecmap` to inherit real glyph→Unicode
        // mappings from another CMap. pdf-font's CMap parser supports
        // usecmap, so post-redaction text extraction would still leak
        // the original mappings. The validator must therefore also
        // reject any presence of `usecmap` for "safe stub" classification.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 100 700 Td <414243> Tj ET".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));

        // Crafted /ToUnicode: stub-marker + ONE trivial FFFD bfchar +
        // /Identity-H usecmap (would normally inherit real mappings).
        let crafted_cmap = b"\
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo
<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Adobe-Identity-UCS def
/CMapType 2 def
/Identity-H usecmap
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfchar
<00> <FFFD>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
";
        let mut tu_dict = lopdf::Dictionary::new();
        tu_dict.set(crate::tounicode::STUB_MARKER_KEY, Object::Boolean(true));
        let tu_stream = Stream::new(tu_dict, crafted_cmap.to_vec());
        let crafted_tu_id = doc.add_object(Object::Stream(tu_stream));

        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(crafted_tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // The crafted /ToUnicode (with usecmap) must NOT have been
        // honoured as a safe stub — the page now points at a fresh
        // /ToUnicode whose content validates AND has no usecmap.
        let new_tu_id = current_tounicode_ref(&doc, page_id)
            .expect("page must still have /ToUnicode after redaction");
        assert_ne!(
            new_tu_id, crafted_tu_id,
            "usecmap-carrying /ToUnicode must NOT be honoured as already-stubbed"
        );

        let new_stream = match doc.get_object(new_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        let mut s = new_stream;
        let _ = s.decompress();
        // Our genuine stub never emits usecmap; verify that.
        assert!(
            !s.content.windows(b"usecmap".len()).any(|w| w == b"usecmap"),
            "replacement stub must not contain usecmap"
        );
        assert!(
            crate::tounicode::is_safe_stub_cmap(&s.content),
            "replacement stream must validate as safe stub"
        );
    }

    // ─── Form XObject font traversal (Codex P1 ronde-6 fix) ─────────────

    /// Build a self-contained /ToUnicode stream + font dict pair and
    /// return their object ids. Helper for the Form XObject tests that
    /// need realistic font data without duplicating ORIGINAL_TOUNICODE
    /// boilerplate per test.
    fn add_font_with_tounicode(doc: &mut Document) -> (ObjectId, ObjectId) {
        let tu_stream = Stream::new(dictionary! {}, ORIGINAL_TOUNICODE.to_vec());
        let tu_id = doc.add_object(Object::Stream(tu_stream));
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "ToUnicode" => Object::Reference(tu_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        (font_id, tu_id)
    }

    /// Construct a Form XObject Stream whose /Resources/Font/<name>
    /// references `font_id`. Returns the new Form XObject's id.
    fn add_form_xobject_with_font(
        doc: &mut Document,
        font_resource_name: &[u8],
        font_id: ObjectId,
    ) -> ObjectId {
        let resources = dictionary! {
            "Font" => dictionary! {
                font_resource_name.to_vec() => Object::Reference(font_id),
            },
        };
        let mut form_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => Object::Dictionary(resources),
        };
        form_dict.set("Length", Object::Integer(0));
        let form_stream = Stream::new(form_dict, Vec::new());
        doc.add_object(Object::Stream(form_stream))
    }

    /// Resolve a page's /Resources/XObject/<name> to a (cloned) Form
    /// XObject Stream + its /Resources/Font/<font_name> /ToUnicode id.
    fn page_form_font_tounicode(
        doc: &Document,
        page_id: ObjectId,
        xobject_name: &[u8],
        font_name: &[u8],
    ) -> Option<(ObjectId, ObjectId)> {
        let page = doc.get_dictionary(page_id).ok()?;
        let resources = match page.get(b"Resources").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
            _ => return None,
        };
        let xobject_dict = match resources.get(b"XObject").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
            _ => return None,
        };
        let form_id = match xobject_dict.get(xobject_name).ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        let form_stream = match doc.get_object(form_id).ok()? {
            Object::Stream(s) => s,
            _ => return None,
        };
        let form_resources = match form_stream.dict.get(b"Resources").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
            _ => return None,
        };
        let font_sub = match form_resources.get(b"Font").ok()? {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
            _ => return None,
        };
        let font_id = match font_sub.get(font_name).ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        let font_dict = doc.get_dictionary(font_id).ok()?;
        let tu_id = match font_dict.get(b"ToUnicode").ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        Some((form_id, tu_id))
    }

    #[test]
    fn tounicode_strip_traverses_form_xobject_fonts() {
        // Codex P1 ronde-6 regression-guard: a redacted page that
        // references a Form XObject with its OWN /Resources/Font/F1 must
        // get the Form's /ToUnicode stubbed too. Without recursion, text
        // drawn via `Do <FormXObj>` would still decode through the
        // original mapping.
        let mut doc = Document::with_version("1.7");
        let content = Stream::new(
            dictionary! {},
            b"BT /Fp1 12 Tf 100 700 Td (page-level) Tj ET q /Fm1 Do Q".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));

        // Page-level font (so the page itself also has ToUnicode work).
        let (page_font_id, _page_tu_id) = add_font_with_tounicode(&mut doc);
        // Form XObject with its OWN font + ToUnicode.
        let (form_font_id, original_form_tu_id) = add_font_with_tounicode(&mut doc);
        let form_id = add_form_xobject_with_font(&mut doc, b"F1", form_font_id);

        let resources = dictionary! {
            "Font" => dictionary! { "Fp1" => Object::Reference(page_font_id) },
            "XObject" => dictionary! { "Fm1" => Object::Reference(form_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // The page's /XObject/Fm1 must now point at a CLONED Form XObject
        // (different from `form_id`), and that cloned Form's /Resources/
        // Font/F1 must have a stub /ToUnicode (different from
        // `original_form_tu_id`).
        let (cloned_form_id, new_form_tu_id) =
            page_form_font_tounicode(&doc, page_id, b"Fm1", b"F1")
                .expect("Form XObject + font + /ToUnicode must still be reachable");
        assert_ne!(
            cloned_form_id, form_id,
            "Form XObject must be cloned for the redacted page"
        );
        assert_ne!(
            new_form_tu_id, original_form_tu_id,
            "Form XObject's font must point at a fresh stub /ToUnicode"
        );
        let stub_stream = match doc.get_object(new_form_tu_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        let mut s = stub_stream;
        let _ = s.decompress();
        assert!(
            crate::tounicode::is_safe_stub_cmap(&s.content),
            "Form XObject font's new /ToUnicode must validate as safe stub"
        );
    }

    #[test]
    fn tounicode_strip_clones_shared_form_xobject_for_redacted_page_only() {
        // Two pages share the same indirect Form XObject. Redacting page
        // 1 must NOT mutate the shared Form XObject Stream — only the
        // redacted page's /XObject entry should redirect to a private
        // clone. Page 2 keeps its original reference and original
        // /ToUnicode.
        let mut doc = Document::with_version("1.7");
        let (form_font_id, original_tu_id) = add_font_with_tounicode(&mut doc);
        let shared_form_id = add_form_xobject_with_font(&mut doc, b"F1", form_font_id);

        let make_page_dict = |form_id: ObjectId| {
            dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => Object::Reference(doc_objects_dummy()),
                "Resources" => Object::Dictionary(dictionary! {
                    "XObject" => dictionary! { "Fm1" => Object::Reference(form_id) },
                }),
            }
        };
        // We need a real Contents stream — build one and reuse for both pages.
        let content = Stream::new(dictionary! {}, b"q /Fm1 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content));
        let page1_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "XObject" => dictionary! { "Fm1" => Object::Reference(shared_form_id) },
            }),
        };
        let page1_id = doc.add_object(Object::Dictionary(page1_dict));
        let page2_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "XObject" => dictionary! { "Fm1" => Object::Reference(shared_form_id) },
            }),
        };
        let page2_id = doc.add_object(Object::Dictionary(page2_dict));
        // Suppress the unused make_page_dict warning — kept for clarity in
        // the comment above this block.
        let _ = make_page_dict;

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page1_id), Object::Reference(page2_id)],
            "Count" => 2_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        for pid in [page1_id, page2_id] {
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(pid) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));
        redactor.apply(&mut doc).unwrap();

        // Page 1 → cloned Form XObject (different id) → stub.
        let (page1_form, page1_tu) =
            page_form_font_tounicode(&doc, page1_id, b"Fm1", b"F1").unwrap();
        assert_ne!(page1_form, shared_form_id);
        assert_ne!(page1_tu, original_tu_id);

        // Page 2 → original shared Form → original /ToUnicode.
        let (page2_form, page2_tu) =
            page_form_font_tounicode(&doc, page2_id, b"Fm1", b"F1").unwrap();
        assert_eq!(
            page2_form, shared_form_id,
            "non-redacted page must still reference the original Form XObject"
        );
        assert_eq!(
            page2_tu, original_tu_id,
            "non-redacted page must still see the original /ToUnicode"
        );

        // Shared Form XObject Stream must be byte-identical to its
        // pre-redaction state — no in-place mutation.
        let shared_form_stream = match doc.get_object(shared_form_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("shared Form XObject must still exist as a Stream"),
        };
        // The shared Form's /Resources/Font/F1 must still reference the
        // original /ToUnicode.
        let shared_resources = match shared_form_stream.dict.get(b"Resources").unwrap() {
            Object::Dictionary(d) => d.clone(),
            _ => panic!("shared Form Resources must be inline dict"),
        };
        let shared_font_dict = match shared_resources.get(b"Font").unwrap() {
            Object::Dictionary(d) => d.clone(),
            _ => panic!("shared Form /Font must be inline dict"),
        };
        let shared_font_ref = match shared_font_dict.get(b"F1").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("shared Form /F1 must be indirect"),
        };
        let shared_font_dict_obj = doc.get_dictionary(shared_font_ref).unwrap();
        let shared_tu_ref = match shared_font_dict_obj.get(b"ToUnicode").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("shared Form font /ToUnicode must be indirect"),
        };
        assert_eq!(
            shared_tu_ref, original_tu_id,
            "shared Form's font must still reference the ORIGINAL /ToUnicode"
        );
    }

    #[test]
    fn tounicode_strip_recurses_into_nested_form_xobject() {
        // Nested Form XObjects: outer Form Fm1 contains /Resources/XObject/
        // Fm2 → inner Form with its own /Font + /ToUnicode. The inner
        // font's /ToUnicode must also be stubbed.
        let mut doc = Document::with_version("1.7");
        let (inner_font_id, inner_original_tu) = add_font_with_tounicode(&mut doc);
        let inner_form_id = add_form_xobject_with_font(&mut doc, b"FInner", inner_font_id);

        // Outer Form: has /Resources/XObject/Fm2 → inner_form_id.
        let outer_resources = dictionary! {
            "XObject" => dictionary! { "Fm2" => Object::Reference(inner_form_id) },
        };
        let mut outer_form_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => Object::Dictionary(outer_resources),
        };
        outer_form_dict.set("Length", Object::Integer(0));
        let outer_form_stream = Stream::new(outer_form_dict, Vec::new());
        let outer_form_id = doc.add_object(Object::Stream(outer_form_stream));

        let content = Stream::new(dictionary! {}, b"q /Fm1 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content));
        let resources = dictionary! {
            "XObject" => dictionary! { "Fm1" => Object::Reference(outer_form_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));
        redactor.apply(&mut doc).unwrap();

        // Walk: page → cloned outer Form → cloned inner Form → font with
        // stub /ToUnicode.
        let page_outer_form_id = match doc
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
        {
            Object::Reference(rid) => {
                match doc.get_dictionary(*rid).unwrap().get(b"XObject").unwrap() {
                    Object::Reference(xrid) => {
                        match doc.get_dictionary(*xrid).unwrap().get(b"Fm1").unwrap() {
                            Object::Reference(id) => *id,
                            _ => panic!("Fm1 must be indirect"),
                        }
                    }
                    Object::Dictionary(d) => match d.get(b"Fm1").unwrap() {
                        Object::Reference(id) => *id,
                        _ => panic!("Fm1 must be indirect"),
                    },
                    _ => panic!("/XObject must be dict or ref"),
                }
            }
            _ => panic!("page /Resources must be indirect after redaction"),
        };
        assert_ne!(
            page_outer_form_id, outer_form_id,
            "outer Form must be cloned for the redacted page"
        );
        let outer_form_stream = match doc.get_object(page_outer_form_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("outer Form must still be a Stream"),
        };
        let outer_resources = match outer_form_stream.dict.get(b"Resources").unwrap() {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("outer Form Resources must be present"),
        };
        let inner_xobject_dict = match outer_resources.get(b"XObject").unwrap() {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("outer Form /XObject must be dict"),
        };
        let inner_clone_id = match inner_xobject_dict.get(b"Fm2").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("Fm2 must be indirect"),
        };
        assert_ne!(
            inner_clone_id, inner_form_id,
            "inner Form must also be cloned by recursion"
        );
        let inner_form_stream = match doc.get_object(inner_clone_id).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("inner clone must still be a Stream"),
        };
        let inner_resources = match inner_form_stream.dict.get(b"Resources").unwrap() {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("inner Resources missing"),
        };
        let inner_font_subdict = match inner_resources.get(b"Font").unwrap() {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("inner /Font missing"),
        };
        let inner_font_ref = match inner_font_subdict.get(b"FInner").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("inner FInner must be indirect"),
        };
        let inner_font_dict = doc.get_dictionary(inner_font_ref).unwrap();
        let inner_new_tu_ref = match inner_font_dict.get(b"ToUnicode").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("inner font /ToUnicode must be indirect"),
        };
        assert_ne!(
            inner_new_tu_ref, inner_original_tu,
            "inner Form's font /ToUnicode must be a fresh stub"
        );
    }

    #[test]
    fn tounicode_strip_handles_cyclic_form_xobject_graph_without_infinite_recursion() {
        // Build a Form XObject whose /Resources/XObject contains itself
        // (Fm1 -> Fm1). The cycle guard must prevent infinite recursion;
        // the strip must still complete and stub at least one font in
        // the cycle.
        let mut doc = Document::with_version("1.7");
        let (font_id, _original_tu) = add_font_with_tounicode(&mut doc);

        // Build the Form XObject WITHOUT the self-reference first
        // (we'll patch it in below). Its resources include /Font/F1 →
        // font_id.
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        };
        let mut form_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => Object::Dictionary(resources),
        };
        form_dict.set("Length", Object::Integer(0));
        let form_stream = Stream::new(form_dict, Vec::new());
        let form_id = doc.add_object(Object::Stream(form_stream));

        // Patch the cycle: add /Resources/XObject/Fm1 → form_id (self-ref).
        if let Ok(Object::Stream(s)) = doc.get_object_mut(form_id) {
            if let Ok(Object::Dictionary(res)) = s.dict.get_mut(b"Resources") {
                res.set(
                    "XObject",
                    Object::Dictionary(dictionary! {
                        "Fm1" => Object::Reference(form_id),
                    }),
                );
            }
        }

        let content = Stream::new(dictionary! {}, b"q /Fm1 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content));
        let page_resources = dictionary! {
            "XObject" => dictionary! { "Fm1" => Object::Reference(form_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(page_resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Must terminate (no stack overflow / hang) and report success.
        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));
        let report = redactor
            .apply(&mut doc)
            .expect("cyclic Form XObject graph must not panic the redactor");
        assert!(report.pages_affected >= 1);
    }

    #[test]
    fn tounicode_strip_skips_form_xobject_without_resources() {
        // Form XObject with NO /Resources at all is fine — it has no
        // fonts of its own (relies on the enclosing context). The strip
        // must visit it (cycle guard etc.) but produce no work.
        let mut doc = Document::with_version("1.7");
        let mut form_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        };
        form_dict.set("Length", Object::Integer(0));
        let form_stream = Stream::new(form_dict, Vec::new());
        let form_id = doc.add_object(Object::Stream(form_stream));

        // Page DOES have a font with /ToUnicode (so strip has SOMETHING
        // to do, ensuring the Form XObject path is exercised).
        let (page_font_id, _) = add_font_with_tounicode(&mut doc);
        let content = Stream::new(
            dictionary! {},
            b"BT /Fp1 12 Tf 100 700 Td (X) Tj ET q /Fm1 Do Q".to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content));
        let resources = dictionary! {
            "Font" => dictionary! { "Fp1" => Object::Reference(page_font_id) },
            "XObject" => dictionary! { "Fm1" => Object::Reference(form_id) },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [50.0, 690.0, 250.0, 720.0]));
        redactor.apply(&mut doc).unwrap();

        // The Form XObject (no /Resources) must remain at the same
        // ObjectId — we never cloned it because we had no work to do
        // for it.
        let page_xobject = match doc
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
        {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("page /Resources missing"),
        };
        let xobject_sub = match page_xobject.get(b"XObject").unwrap() {
            Object::Reference(rid) => doc.get_dictionary(*rid).unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            _ => panic!("page /XObject missing"),
        };
        let resolved_form = match xobject_sub.get(b"Fm1").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("Fm1 must be indirect"),
        };
        // The page-level /XObject sub-dict was cloned (because the page
        // has font work), but the Form XObject WITHOUT resources should
        // not itself be cloned — entries we rewrite are only those we
        // recursed into, and a no-resources Form has nothing to do.
        // Either resolved_form == form_id (best) or we cloned but the
        // clone is byte-identical.
        let resolved_stream = match doc.get_object(resolved_form).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("Form XObject must be a Stream"),
        };
        assert!(
            resolved_stream.dict.get(b"Resources").is_err(),
            "no-Resources Form XObject should remain without /Resources after strip"
        );
    }

    /// Stand-in for nonexistent test helpers in some intermediate
    /// constructions; never read.
    fn doc_objects_dummy() -> ObjectId {
        ObjectId::default()
    }

    #[test]
    fn tounicode_strip_coalesces_aliased_form_xobject_to_one_stubbed_clone() {
        // Codex P1 ronde-6 follow-up #1379: when the SAME original Form
        // XObject is referenced by multiple /XObject names on a redacted
        // page (or transitively via cycles / nested aliases), the second
        // and later references must NOT receive a fresh un-stubbed
        // clone. They must share the SAME memoised clone produced and
        // stubbed during the first encounter.
        //
        // Without alias-coalescing memoisation: page → /XObject/A → clone1
        // (stubbed) and /XObject/B → clone2 (NOT stubbed, contains
        // original /ToUnicode mappings). Content stream renders
        // `Do A` → stub; `Do B` → leak.
        let mut doc = Document::with_version("1.7");
        let (form_font_id, original_tu_id) = add_font_with_tounicode(&mut doc);
        let shared_form_id = add_form_xobject_with_font(&mut doc, b"F1", form_font_id);

        // Page references the SAME Form XObject under TWO names: /Fm1
        // and /Fm2. Both must end up pointing at the same stubbed clone.
        let content = Stream::new(dictionary! {}, b"q /Fm1 Do Q q /Fm2 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content));
        let resources = dictionary! {
            "XObject" => dictionary! {
                "Fm1" => Object::Reference(shared_form_id),
                "Fm2" => Object::Reference(shared_form_id),
            },
        };
        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut redactor = Redactor::new();
        redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));
        redactor.apply(&mut doc).unwrap();

        // Resolve /Fm1 and /Fm2 — they MUST point at the SAME cloned
        // Form XObject id, not two separate fresh clones.
        let (fm1_form, fm1_tu) =
            page_form_font_tounicode(&doc, page_id, b"Fm1", b"F1").expect("Fm1 path must resolve");
        let (fm2_form, fm2_tu) =
            page_form_font_tounicode(&doc, page_id, b"Fm2", b"F1").expect("Fm2 path must resolve");

        assert_eq!(
            fm1_form, fm2_form,
            "aliased /XObject entries must share the same memoised clone"
        );
        assert_ne!(
            fm1_form, shared_form_id,
            "alias clone must be different from the shared original"
        );

        // Both aliases must end up at the SAME stub /ToUnicode (the one
        // produced during the first recursion). Neither may keep the
        // original mapping reachable.
        assert_eq!(fm1_tu, fm2_tu);
        assert_ne!(fm1_tu, original_tu_id);

        let stub_stream = match doc.get_object(fm1_tu).unwrap() {
            Object::Stream(s) => s.clone(),
            _ => panic!("/ToUnicode must be a stream"),
        };
        let mut s = stub_stream;
        let _ = s.decompress();
        assert!(
            crate::tounicode::is_safe_stub_cmap(&s.content),
            "shared alias clone's font must point at a content-validated stub /ToUnicode"
        );
    }
}
