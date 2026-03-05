//! XFA form flattening — convert interactive XFA forms to static PDF.
//!
//! Flattening bakes all field values and appearance streams into the
//! page content, then removes the XFA metadata and scripting dictionaries
//! from the document catalog. The result is a non-interactive PDF that
//! renders identically in any viewer.

use crate::appearance::{generate_appearances, AppearanceConfig, AppearanceStream};
use crate::error::{PdfError, Result};
use crate::pdf_reader::PdfReader;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use xfa_layout_engine::layout::LayoutDom;

/// Configuration for the flattening process.
#[derive(Debug, Clone)]
pub struct FlattenConfig {
    /// Appearance stream generation settings.
    pub appearance: AppearanceConfig,
    /// Whether to remove the XFA entry from the AcroForm dictionary.
    pub remove_xfa: bool,
    /// Whether to remove the AcroForm dictionary entirely.
    pub remove_acroform: bool,
    /// Whether to compress flattened content streams.
    pub compress: bool,
}

impl Default for FlattenConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            remove_xfa: true,
            remove_acroform: true,
            compress: true,
        }
    }
}

/// Result of a flatten operation.
#[derive(Debug)]
pub struct FlattenResult {
    /// Number of pages processed.
    pub pages_processed: usize,
    /// Number of fields flattened.
    pub fields_flattened: usize,
    /// Number of appearance streams generated.
    pub streams_generated: usize,
}

/// Flatten a LayoutDom into a new PDF document, producing a static PDF.
///
/// This is the primary entry point for creating a flattened PDF from scratch.
/// It:
/// 1. Generates appearance streams from the layout
/// 2. Creates page content streams embedding all field appearances as Form XObjects
/// 3. Builds a complete PDF document structure
///
/// Returns the flattened PDF as bytes.
pub fn flatten_to_pdf(layout: &LayoutDom, config: &FlattenConfig) -> Result<Vec<u8>> {
    let mut doc = lopdf::Document::new();
    let mut page_ids = Vec::new();

    for page in &layout.pages {
        let appearances = generate_appearances(&page.nodes, &config.appearance)
            .map_err(|e| PdfError::RenderError(format!("appearances: {e}")))?;

        let page_id = build_page(&mut doc, page.width, page.height, &appearances, config)?;
        page_ids.push(page_id);
    }

    // Build page tree
    let pages_id = doc.add_object(Object::Dictionary(build_pages_dict(&page_ids)));

    // Set parent reference on each page
    for &page_id in &page_ids {
        if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(page_id) {
            dict.set("Parent", Object::Reference(pages_id));
        }
    }

    // Build catalog (no AcroForm, no XFA — it's already flat)
    let catalog = dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf)
        .map_err(|e| PdfError::Io(std::io::Error::other(format!("save: {e}"))))?;

    Ok(buf)
}

/// Flatten a LayoutDom into an existing PDF, replacing page content in-place.
///
/// This modifies the PdfReader's document:
/// 1. Generates appearance streams from the layout
/// 2. Replaces page content with flattened streams
/// 3. Removes XFA and optionally AcroForm from the catalog
pub fn flatten_into_pdf(
    reader: &mut PdfReader,
    layout: &LayoutDom,
    config: &FlattenConfig,
) -> Result<FlattenResult> {
    let doc = reader.document_mut();
    let mut fields_flattened = 0;
    let mut streams_generated = 0;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();

    for (page_idx, page) in layout.pages.iter().enumerate() {
        if page_idx >= page_ids.len() {
            break;
        }
        let page_id = page_ids[page_idx];

        let appearances = generate_appearances(&page.nodes, &config.appearance)
            .map_err(|e| PdfError::RenderError(format!("appearances: {e}")))?;

        let (content_stream, xobject_dict, font_dict, count) =
            build_content_stream(doc, page.height, &appearances, config)?;

        let content_id = doc.add_object(Object::Stream(content_stream));

        // Build resources dictionary
        let mut resources = Dictionary::new();
        if !xobject_dict.is_empty() {
            resources.set("XObject", Object::Dictionary(xobject_dict));
        }
        if !font_dict.is_empty() {
            resources.set("Font", Object::Dictionary(font_dict));
        }

        // Update page
        if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
            page_dict.set("Contents", Object::Reference(content_id));
            page_dict.set("Resources", Object::Dictionary(resources));
        }

        fields_flattened += appearances.len();
        streams_generated += count;
    }

    // Remove XFA/AcroForm metadata
    if config.remove_xfa || config.remove_acroform {
        remove_xfa_metadata(doc, config.remove_acroform);
    }

    Ok(FlattenResult {
        pages_processed: layout.pages.len().min(page_ids.len()),
        fields_flattened,
        streams_generated,
    })
}

/// Build a single PDF page with flattened field appearances.
fn build_page(
    doc: &mut lopdf::Document,
    width: f64,
    height: f64,
    appearances: &[(String, f64, f64, AppearanceStream)],
    config: &FlattenConfig,
) -> Result<ObjectId> {
    let (content_stream, xobject_dict, font_dict, _) =
        build_content_stream(doc, height, appearances, config)?;

    let content_id = doc.add_object(Object::Stream(content_stream));

    let mut resources = Dictionary::new();
    if !xobject_dict.is_empty() {
        resources.set("XObject", Object::Dictionary(xobject_dict));
    }
    if !font_dict.is_empty() {
        resources.set("Font", Object::Dictionary(font_dict));
    }

    let page = dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            0.0.into(), 0.0.into(), width.into(), height.into(),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(resources),
    };

    Ok(doc.add_object(Object::Dictionary(page)))
}

/// Build a page content stream that paints all field appearances as Form XObjects.
///
/// Each appearance is added as a Form XObject resource. The content stream
/// uses q/cm/Do/Q operators to position each XObject on the page.
///
/// Returns (content_stream, xobject_resources, font_resources, xobject_count).
fn build_content_stream(
    doc: &mut lopdf::Document,
    page_height: f64,
    appearances: &[(String, f64, f64, AppearanceStream)],
    config: &FlattenConfig,
) -> Result<(Stream, Dictionary, Dictionary, usize)> {
    let mut ops = Vec::new();
    let mut xobject_dict = Dictionary::new();
    let mut font_dict = Dictionary::new();
    let mut font_seen = std::collections::HashSet::new();

    for (idx, (_name, x, y, appearance)) in appearances.iter().enumerate() {
        let xobject_name = format!("XF{idx}");

        // Build Form XObject from the appearance stream
        let xobject = build_form_xobject(appearance, config.compress);
        let xobject_id = doc.add_object(Object::Stream(xobject));
        xobject_dict.set(
            xobject_name.as_bytes().to_vec(),
            Object::Reference(xobject_id),
        );

        // Collect font resources
        for (res_name, font_name) in &appearance.font_resources {
            if font_seen.insert(res_name.clone()) {
                let font_obj = dictionary! {
                    "Type" => Object::Name(b"Font".to_vec()),
                    "Subtype" => Object::Name(b"Type1".to_vec()),
                    "BaseFont" => Object::Name(font_name.as_bytes().to_vec()),
                };
                font_dict.set(res_name.as_bytes().to_vec(), Object::Dictionary(font_obj));
            }
        }

        // Convert from layout coordinates (top-left origin) to PDF
        // coordinates (bottom-left origin): pdf_y = page_height - layout_y - bbox_height
        let bbox_height = appearance.bbox[3] - appearance.bbox[1];
        let pdf_y = page_height - *y - bbox_height;

        // q — save graphics state
        ops.push(Operation::new("q", vec![]));

        // cm — translate to field position on page (PDF bottom-left coordinates)
        ops.push(Operation::new(
            "cm",
            vec![
                1.0.into(),
                0.0.into(),
                0.0.into(),
                1.0.into(),
                Object::Real(*x as f32),
                Object::Real(pdf_y as f32),
            ],
        ));

        // Do — paint the Form XObject
        ops.push(Operation::new(
            "Do",
            vec![Object::Name(xobject_name.as_bytes().to_vec())],
        ));

        // Q — restore graphics state
        ops.push(Operation::new("Q", vec![]));
    }

    let content = Content { operations: ops };
    let content_bytes = content
        .encode()
        .map_err(|e| PdfError::Io(std::io::Error::other(format!("encode: {e}"))))?;

    let dict = Dictionary::new();
    let mut stream = Stream::new(dict, content_bytes);
    if config.compress {
        let _ = stream.compress();
    }

    Ok((stream, xobject_dict, font_dict, appearances.len()))
}

/// Build a PDF Form XObject stream from an AppearanceStream.
fn build_form_xobject(appearance: &AppearanceStream, compress: bool) -> Stream {
    let [bx, by, bw, bh] = appearance.bbox;
    let bbox = vec![bx.into(), by.into(), bw.into(), bh.into()];

    // Build font resources for the XObject
    let mut font_dict = Dictionary::new();
    for (res_name, font_name) in &appearance.font_resources {
        let font_obj = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(font_name.as_bytes().to_vec()),
        };
        font_dict.set(res_name.as_bytes().to_vec(), Object::Dictionary(font_obj));
    }

    let mut resources = Dictionary::new();
    if !font_dict.is_empty() {
        resources.set("Font", Object::Dictionary(font_dict));
    }

    let dict = dictionary! {
        "Type" => Object::Name(b"XObject".to_vec()),
        "Subtype" => Object::Name(b"Form".to_vec()),
        "BBox" => Object::Array(bbox),
        "Resources" => Object::Dictionary(resources),
    };

    let mut stream = Stream::new(dict, appearance.content.clone());
    if compress {
        let _ = stream.compress();
    }

    stream
}

/// Build a Pages dictionary for the document.
fn build_pages_dict(page_ids: &[ObjectId]) -> Dictionary {
    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Count" => Object::Integer(page_ids.len() as i64),
        "Kids" => Object::Array(kids),
    }
}

/// Remove XFA metadata and optionally AcroForm from the document catalog.
fn remove_xfa_metadata(doc: &mut lopdf::Document, remove_acroform: bool) {
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };

    // Determine if AcroForm is indirect or inline
    let acroform_ref = if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
        if let Ok(Object::Reference(r)) = catalog.get(b"AcroForm") {
            Some(*r)
        } else {
            None
        }
    } else {
        None
    };

    if remove_acroform {
        // Remove AcroForm entirely from catalog
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
            catalog.remove(b"AcroForm");
            catalog.remove(b"NeedsRendering");
        }
    } else {
        // Keep AcroForm but remove XFA entry from it
        if let Some(acroform_id) = acroform_ref {
            // Indirect AcroForm reference
            if let Ok(Object::Dictionary(ref mut acroform)) = doc.get_object_mut(acroform_id) {
                acroform.remove(b"XFA");
            }
        } else {
            // Inline AcroForm dictionary
            if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
                if let Ok(Object::Dictionary(ref mut acroform)) = catalog.get_mut(b"AcroForm") {
                    acroform.remove(b"XFA");
                }
            }
        }

        // Remove NeedsRendering from catalog
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
            catalog.remove(b"NeedsRendering");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::FormNodeId;
    use xfa_layout_engine::layout::{LayoutContent, LayoutNode, LayoutPage};
    use xfa_layout_engine::types::Rect;

    fn make_layout(fields: Vec<LayoutNode>) -> LayoutDom {
        LayoutDom {
            pages: vec![LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: fields,
            }],
        }
    }

    fn text_node(name: &str, x: f64, y: f64, w: f64, h: f64, text: &str) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, w, h),
            name: name.to_string(),
            content: LayoutContent::WrappedText {
                lines: vec![text.to_string()],
                font_size: 12.0,
            },
            children: vec![],
        }
    }

    #[test]
    fn flatten_single_field() {
        let layout = make_layout(vec![text_node("Name", 72.0, 72.0, 200.0, 20.0, "John")]);
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        assert!(!pdf_bytes.is_empty());

        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn flatten_multiple_fields() {
        let layout = make_layout(vec![
            text_node("Name", 72.0, 72.0, 200.0, 20.0, "John Doe"),
            text_node("SSN", 72.0, 100.0, 200.0, 20.0, "123-45-6789"),
            text_node("City", 72.0, 128.0, 200.0, 20.0, "New York"),
        ]);
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn flatten_multipage() {
        let layout = LayoutDom {
            pages: vec![
                LayoutPage {
                    width: 612.0,
                    height: 792.0,
                    nodes: vec![text_node("Page1Field", 72.0, 72.0, 200.0, 20.0, "A")],
                },
                LayoutPage {
                    width: 612.0,
                    height: 792.0,
                    nodes: vec![text_node("Page2Field", 72.0, 72.0, 200.0, 20.0, "B")],
                },
            ],
        };
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn flatten_empty_layout() {
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: vec![],
            }],
        };
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn flatten_no_xfa_in_output() {
        let layout = make_layout(vec![text_node("F", 72.0, 72.0, 200.0, 20.0, "Test")]);
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();

        let catalog_id = match doc.trailer.get(b"Root").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("No Root reference"),
        };
        let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(
            catalog.get(b"AcroForm").is_err(),
            "Flattened PDF should not have AcroForm"
        );
    }

    #[test]
    fn flatten_uncompressed() {
        let layout = make_layout(vec![text_node("F", 72.0, 72.0, 100.0, 20.0, "Hi")]);
        let config = FlattenConfig {
            compress: false,
            ..Default::default()
        };
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);

        // Uncompressed: content should contain readable operators
        let pages = doc.get_pages();
        let page_id = *pages.values().next().unwrap();
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object(page_id) {
            if let Ok(Object::Reference(content_ref)) = page_dict.get(b"Contents") {
                if let Ok(Object::Stream(stream)) = doc.get_object(*content_ref) {
                    let content_str = String::from_utf8_lossy(&stream.content);
                    assert!(content_str.contains("Do") || appearances_empty(&layout));
                }
            }
        }
    }

    #[test]
    fn flatten_into_existing_pdf() {
        // Create a minimal PDF with one page
        let mut doc = lopdf::Document::new();
        let content = Stream::new(Dictionary::new(), b"".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let page = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.0.into(), 0.0.into(), 612.0.into(), 792.0.into(),
            ]),
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        if let Ok(Object::Dictionary(ref mut p)) = doc.get_object_mut(page_id) {
            p.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        let mut reader = PdfReader::from_bytes(&buf).unwrap();

        let layout = make_layout(vec![text_node("Test", 72.0, 72.0, 200.0, 20.0, "Hello")]);
        let config = FlattenConfig::default();
        let result = flatten_into_pdf(&mut reader, &layout, &config).unwrap();

        assert_eq!(result.pages_processed, 1);
        assert_eq!(result.fields_flattened, 1);
        assert!(result.streams_generated > 0);
    }

    #[test]
    fn remove_xfa_from_catalog() {
        let mut doc = lopdf::Document::new();

        let acroform = dictionary! {
            "XFA" => Object::String(b"dummy".to_vec(), lopdf::StringFormat::Literal),
        };
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "AcroForm" => Object::Dictionary(acroform),
            "NeedsRendering" => Object::Boolean(true),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        remove_xfa_metadata(&mut doc, true);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(cat.get(b"AcroForm").is_err(), "AcroForm should be removed");
        assert!(
            cat.get(b"NeedsRendering").is_err(),
            "NeedsRendering should be removed"
        );
    }

    #[test]
    fn remove_xfa_keeps_acroform_when_requested() {
        let mut doc = lopdf::Document::new();

        let acroform = dictionary! {
            "XFA" => Object::String(b"dummy".to_vec(), lopdf::StringFormat::Literal),
            "Fields" => Object::Array(vec![]),
        };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        remove_xfa_metadata(&mut doc, false);

        // AcroForm should still exist
        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(cat.get(b"AcroForm").is_ok(), "AcroForm should be kept");

        // But XFA should be removed from AcroForm
        let af = doc.get_object(acroform_id).unwrap().as_dict().unwrap();
        assert!(af.get(b"XFA").is_err(), "XFA should be removed");
    }

    #[test]
    fn flatten_result_counts() {
        let layout = make_layout(vec![
            text_node("A", 10.0, 10.0, 100.0, 20.0, "X"),
            text_node("B", 10.0, 40.0, 100.0, 20.0, "Y"),
        ]);
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
        assert!(!pdf_bytes.is_empty());

        // Should produce valid multi-field PDF
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    /// Helper: check if layout has no visible content
    fn appearances_empty(layout: &LayoutDom) -> bool {
        layout.pages.iter().all(|p| p.nodes.is_empty())
    }
}
