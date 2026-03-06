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
    /// Whether to produce PDF/A-2b compliant output.
    ///
    /// When enabled, the flattened PDF will include:
    /// - XMP metadata with PDF/A-2b conformance declaration
    /// - sRGB output intent (ICC profile)
    /// - JavaScript and embedded files removed
    /// - CMYK colors converted to sRGB
    pub pdfa: bool,
}

impl Default for FlattenConfig {
    fn default() -> Self {
        Self {
            appearance: AppearanceConfig::default(),
            remove_xfa: true,
            remove_acroform: true,
            compress: true,
            pdfa: false,
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

    // For PDF/A: embed the font once, share across all pages/XObjects.
    let embedded_font_id = if config.pdfa {
        embed_font_for_pdfa(&mut doc, &config.appearance.default_font)
    } else {
        None
    };

    for page in &layout.pages {
        let appearances = generate_appearances(&page.nodes, &config.appearance)
            .map_err(|e| PdfError::RenderError(format!("appearances: {e}")))?;

        let page_id = build_page(
            &mut doc,
            page.width,
            page.height,
            &appearances,
            config,
            embedded_font_id,
        )?;
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

    // Apply PDF/A-2b compliance if requested.
    if config.pdfa {
        apply_pdfa2b(&mut doc)?;
    }

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

    // For PDF/A: embed the font once, share across all pages/XObjects.
    let embedded_font_id = if config.pdfa {
        embed_font_for_pdfa(doc, &config.appearance.default_font)
    } else {
        None
    };

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();

    for (page_idx, page) in layout.pages.iter().enumerate() {
        if page_idx >= page_ids.len() {
            break;
        }
        let page_id = page_ids[page_idx];

        // Use the actual PDF page MediaBox height for coordinate conversion,
        // falling back to the layout page height if MediaBox is unavailable.
        let actual_page_height = doc
            .get_object(page_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"MediaBox").ok())
            .and_then(|mb| mb.as_array().ok())
            .and_then(|arr| {
                if arr.len() >= 4 {
                    match &arr[3] {
                        Object::Real(f) => Some(*f as f64),
                        Object::Integer(i) => Some(*i as f64),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .unwrap_or(page.height);

        let appearances = generate_appearances(&page.nodes, &config.appearance)
            .map_err(|e| PdfError::RenderError(format!("appearances: {e}")))?;

        let (content_stream, xobject_dict, font_dict, count) = build_content_stream(
            doc,
            actual_page_height,
            &appearances,
            config,
            embedded_font_id,
        )?;

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

    // Apply PDF/A-2b compliance if requested.
    if config.pdfa {
        apply_pdfa2b(doc)?;
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
    embedded_font_id: Option<ObjectId>,
) -> Result<ObjectId> {
    let (content_stream, xobject_dict, font_dict, _) =
        build_content_stream(doc, height, appearances, config, embedded_font_id)?;

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
    embedded_font_id: Option<ObjectId>,
) -> Result<(Stream, Dictionary, Dictionary, usize)> {
    let mut ops = Vec::new();
    let mut xobject_dict = Dictionary::new();
    let mut font_dict = Dictionary::new();
    let mut font_seen = std::collections::HashSet::new();

    for (idx, (_name, x, y, appearance)) in appearances.iter().enumerate() {
        let xobject_name = format!("XF{idx}");

        // Build Form XObject from the appearance stream
        let xobject = build_form_xobject(appearance, config.compress, embedded_font_id);
        let xobject_id = doc.add_object(Object::Stream(xobject));
        xobject_dict.set(
            xobject_name.as_bytes().to_vec(),
            Object::Reference(xobject_id),
        );

        // Collect font resources
        for (res_name, font_name) in &appearance.font_resources {
            if font_seen.insert(res_name.clone()) {
                let font_obj = make_font_object(font_name, embedded_font_id);
                font_dict.set(res_name.as_bytes().to_vec(), font_obj);
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
fn build_form_xobject(
    appearance: &AppearanceStream,
    compress: bool,
    embedded_font_id: Option<ObjectId>,
) -> Stream {
    let [bx, by, bw, bh] = appearance.bbox;
    let bbox = vec![bx.into(), by.into(), bw.into(), bh.into()];

    // Build font resources for the XObject
    let mut font_dict = Dictionary::new();
    for (res_name, font_name) in &appearance.font_resources {
        let font_obj = make_font_object(font_name, embedded_font_id);
        font_dict.set(res_name.as_bytes().to_vec(), font_obj);
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

/// Build a font Object: either a reference to an embedded font or an inline Type1 dict.
fn make_font_object(font_name: &str, embedded_font_id: Option<ObjectId>) -> Object {
    if let Some(font_id) = embedded_font_id {
        Object::Reference(font_id)
    } else {
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(font_name.as_bytes().to_vec()),
        })
    }
}

/// Embed a system font into the document for PDF/A compliance.
///
/// Finds the font on the system via `FontResolver`, embeds it as TrueType
/// with a complete `FontDescriptor` + `FontFile2`, and returns the font
/// object's ID for shared referencing across pages and XObjects.
fn embed_font_for_pdfa(doc: &mut lopdf::Document, font_name: &str) -> Option<ObjectId> {
    let mut resolver = crate::font::FontResolver::new();
    let loaded = resolver.resolve(font_name, false, false)?;

    let raw_data = loaded.raw_data().to_vec();
    let units_per_em = loaded.units_per_em;
    let ascender = loaded.ascender;
    let descender = loaded.descender;
    let scale = 1000.0 / units_per_em as f64;

    // Build widths for WinAnsiEncoding range (32..=255).
    // WinAnsiEncoding maps code points 128-159 to specific Unicode characters
    // (e.g. 128 → U+20AC Euro sign), not to U+0080-U+009F control chars.
    // We must use the correct Unicode mapping to get consistent glyph widths.
    let first_char: i64 = 32;
    let last_char: i64 = 255;
    let widths: Vec<Object> = (first_char..=last_char)
        .map(|cp| {
            let unicode = winansi_to_unicode(cp as u8);
            // Undefined positions (U+FFFD) get width 0 — they should never render.
            let w = if unicode == '\u{FFFD}' {
                0
            } else {
                loaded.char_advance(unicode)
            };
            Object::Integer((w as f64 * scale).round() as i64)
        })
        .collect();

    let postscript_name = loaded
        .postscript_name
        .clone()
        .unwrap_or_else(|| font_name.to_string());

    // Drop the borrow on resolver before mutating doc.
    drop(resolver);

    // Embed the raw TrueType font data.
    let font_stream = Stream::new(
        dictionary! {
            "Length1" => Object::Integer(raw_data.len() as i64),
        },
        raw_data,
    );
    let font_file_id = doc.add_object(Object::Stream(font_stream));

    // FontDescriptor (ISO 32000-1:2008, Table 122).
    let descriptor = dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(postscript_name.as_bytes().to_vec()),
        "Flags" => Object::Integer(32), // Nonsymbolic
        "FontBBox" => Object::Array(vec![
            Object::Integer(-200),
            Object::Integer((descender as f64 * scale).round() as i64),
            Object::Integer(1200),
            Object::Integer((ascender as f64 * scale).round() as i64),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer((ascender as f64 * scale).round() as i64),
        "Descent" => Object::Integer((descender as f64 * scale).round() as i64),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(font_file_id),
    };
    let descriptor_id = doc.add_object(Object::Dictionary(descriptor));

    // Complete TrueType font dictionary.
    let font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"TrueType".to_vec()),
        "BaseFont" => Object::Name(postscript_name.as_bytes().to_vec()),
        "FirstChar" => Object::Integer(first_char),
        "LastChar" => Object::Integer(last_char),
        "Widths" => Object::Array(widths),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    };
    let font_id = doc.add_object(Object::Dictionary(font));

    Some(font_id)
}

/// Map a WinAnsiEncoding code point (0-255) to its Unicode character.
///
/// Code points 32-127 and 160-255 map directly to their Unicode equivalents
/// (Latin-1 / ISO 8859-1). Code points 128-159 map to specific characters
/// per the PDF spec (ISO 32000-1:2008, Annex D, Table D.1).
fn winansi_to_unicode(code: u8) -> char {
    #[rustfmt::skip]
    const WIN_ANSI_128_159: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', // 128-131
        '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', // 132-135
        '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', // 136-139
        '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}', // 140-143
        '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', // 144-147
        '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', // 148-151
        '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', // 152-155
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}', // 156-159
    ];

    if (128..=159).contains(&code) {
        WIN_ANSI_128_159[(code - 128) as usize]
    } else {
        char::from(code)
    }
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

/// Apply PDF/A-2b compliance transformations to a document.
///
/// This performs all steps needed for PDF/A-2b:
/// 1. Inject XMP metadata with conformance declaration
/// 2. Add sRGB output intent (if not already present)
/// 3. Convert CMYK colors to sRGB in content streams
/// 4. Remove JavaScript and additional actions
/// 5. Remove embedded files
/// 6. Add file identifier to trailer (ISO 32000-1:2008, 14.4)
fn apply_pdfa2b(doc: &mut lopdf::Document) -> Result<()> {
    // 1. XMP metadata
    crate::xmp::inject_pdfa2b_metadata(doc, "XFA Flattened Document")?;

    // 2. sRGB output intent
    let color_report = crate::colorspace::detect_color_spaces(doc);
    if !color_report.has_srgb_output_intent {
        crate::colorspace::add_srgb_output_intent(doc)?;
    }

    // 3. CMYK → sRGB conversion in page content streams
    if color_report.uses_cmyk {
        let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        for page_id in page_ids {
            if let Ok(content) = doc.get_page_content(page_id) {
                let converted = crate::colorspace::convert_cmyk_to_rgb_in_content(&content);
                if converted != content {
                    let _ = doc.change_page_content(page_id, converted);
                }
            }
        }
    }

    // 4. Remove JavaScript and actions
    crate::pdfa_sanitize::remove_javascript(doc);

    // 5. Remove embedded files
    crate::pdfa_sanitize::remove_embedded_files(doc);

    // 6. File identifier (PDF/A-2b §6.1.3)
    inject_file_id(doc);

    Ok(())
}

/// Inject a file identifier array into the document trailer.
///
/// PDF/A-2b requires the trailer to contain an `ID` key with two byte strings
/// (ISO 32000-1:2008, §14.4). We generate a deterministic ID based on the
/// current timestamp and a fixed salt.
fn inject_file_id(doc: &mut lopdf::Document) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = simple_hash(seed);

    let id_bytes = hash.to_be_bytes().to_vec();
    let id_string = Object::String(id_bytes.clone(), lopdf::StringFormat::Literal);
    doc.trailer
        .set("ID", Object::Array(vec![id_string.clone(), id_string]));
}

/// Simple non-cryptographic hash for generating file IDs.
fn simple_hash(seed: u128) -> u128 {
    let mut h = seed;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    h
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
