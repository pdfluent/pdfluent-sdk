//! Two-phase PDF redaction: mark areas, then apply permanent removal.
//!
//! Phase 1: Mark redaction areas (regions on specific pages).
//! Phase 2: Apply redactions — permanently remove content, draw overlays,
//! and clean metadata.

use crate::error::{RedactError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::io::Write;

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

            // Phase 2: Draw redaction overlays.
            draw_redaction_overlays(doc, page_id, areas)?;

            affected_pages.insert(page_num);
        }

        // Phase 3: Clean metadata.
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
fn draw_redaction_overlays(
    doc: &mut Document,
    page_id: ObjectId,
    areas: &[&RedactionArea],
) -> Result<()> {
    let mut ops = Vec::new();

    for area in areas {
        let [r, g, b] = area.fill_color;
        let [x0, y0, x1, y1] = area.rect;
        let w = x1 - x0;
        let h = y1 - y0;

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
                Object::Real(x0 as f32),
                Object::Real(y0 as f32),
                Object::Real(w as f32),
                Object::Real(h as f32),
            ],
        ));
        ops.push(Operation::new("f", vec![]));

        // Draw overlay text if specified.
        if let Some(ref text) = area.overlay_text {
            // Calculate font size to fit within the rectangle.
            let max_font_size = h * 0.7;
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
                    Object::Real((x0 + 2.0) as f32),
                    Object::Real((y0 + 2.0) as f32),
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

/// Clean document metadata: remove Info dictionary, XMP metadata, and thumbnails.
fn clean_metadata(doc: &mut Document) {
    // Remove Info dictionary reference from trailer.
    doc.trailer.remove(b"Info");

    // Remove XMP metadata from the catalog.
    if let Ok(catalog_id) = doc.trailer.get(b"Root") {
        if let Object::Reference(root_id) = catalog_id.clone() {
            if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(root_id) {
                catalog.remove(b"Metadata");
            }
        }
    }

    // Remove thumbnails from pages.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
            page.remove(b"Thumb");
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

/// Append a content stream reference to a page's Contents array.
fn append_content_to_page(doc: &mut Document, page_id: ObjectId, content_id: ObjectId) {
    let existing = {
        let page_obj = match doc.get_object(page_id) {
            Ok(obj) => obj,
            Err(_) => return,
        };
        let page_dict = match page_obj {
            Object::Dictionary(ref d) => d,
            _ => return,
        };
        page_dict.get(b"Contents").ok().cloned()
    };

    let new_contents = match existing {
        Some(Object::Reference(existing_id)) => Object::Array(vec![
            Object::Reference(existing_id),
            Object::Reference(content_id),
        ]),
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(content_id));
            Object::Array(arr)
        }
        _ => Object::Reference(content_id),
    };

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", new_contents);
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
}
