//! PDF/A font embedding and subsetting.
//!
//! Detects non-embedded fonts and embeds them for PDF/A conformity.
//! Uses the subsetter crate (via font_subset module) for size reduction.

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// Report from font embedding pass.
#[derive(Debug, Clone)]
pub struct FontEmbedReport {
    /// Number of fonts inspected.
    pub fonts_inspected: usize,
    /// Number of non-embedded fonts found.
    pub non_embedded_found: usize,
    /// Number of fonts successfully embedded.
    pub fonts_embedded: usize,
    /// Fonts that could not be embedded (name, reason).
    pub failed: Vec<(String, String)>,
}

/// Standard 14 font names that must be embedded for PDF/A.
const STANDARD_14: &[&str] = &[
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

/// Detect all non-embedded fonts in the document.
pub fn find_non_embedded_fonts(doc: &Document) -> Vec<(ObjectId, String)> {
    let mut result = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };

        // Must be a Font dictionary.
        if get_name(dict, b"Type").as_deref() != Some("Font") {
            continue;
        }

        let font_name = get_name(dict, b"BaseFont").unwrap_or_default();
        if font_name.is_empty() {
            continue;
        }

        // Check if the font has a FontDescriptor with embedded data.
        let has_embedded = match dict.get(b"FontDescriptor").ok() {
            Some(Object::Reference(fd_id)) => {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                    fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")
                } else {
                    false
                }
            }
            _ => false,
        };

        // Type0 fonts: check DescendantFonts for embedding.
        let is_type0 = get_name(dict, b"Subtype").as_deref() == Some("Type0");
        let has_embedded_descendant = if is_type0 {
            check_descendant_embedded(doc, dict)
        } else {
            false
        };

        if !has_embedded && !has_embedded_descendant {
            result.push((*id, font_name));
        }
    }

    result
}

/// Embed fonts from system font files into the document.
///
/// Searches common system font directories for matching font files.
/// For PDF/A, all fonts including Standard 14 must be embedded.
pub fn embed_fonts(doc: &mut Document) -> Result<FontEmbedReport> {
    let mut report = FontEmbedReport {
        fonts_inspected: 0,
        non_embedded_found: 0,
        fonts_embedded: 0,
        failed: Vec::new(),
    };

    let non_embedded = find_non_embedded_fonts(doc);
    report.fonts_inspected = count_all_fonts(doc);
    report.non_embedded_found = non_embedded.len();

    for (font_id, font_name) in &non_embedded {
        // Try to find a system font file.
        let font_path = find_system_font(font_name);

        match font_path {
            Some(path) => match embed_font_file(doc, *font_id, &path) {
                Ok(()) => report.fonts_embedded += 1,
                Err(e) => report.failed.push((font_name.clone(), format!("{e}"))),
            },
            None => {
                report
                    .failed
                    .push((font_name.clone(), "font file not found on system".into()));
            }
        }
    }

    Ok(report)
}

/// Check if this is a Standard 14 font.
pub fn is_standard_14(name: &str) -> bool {
    // Strip subset prefix (ABCDEF+FontName).
    let clean = if name.len() > 7 && name.as_bytes()[6] == b'+' {
        &name[7..]
    } else {
        name
    };
    STANDARD_14.contains(&clean)
}

/// Embed a font file into the document.
fn embed_font_file(doc: &mut Document, font_id: ObjectId, font_path: &str) -> Result<()> {
    let font_data = std::fs::read(font_path)
        .map_err(|e| ManipError::Other(format!("failed to read font file: {e}")))?;

    // Determine font type from file extension or data.
    let is_truetype = font_path.ends_with(".ttf")
        || font_path.ends_with(".ttc")
        || (font_data.len() >= 4
            && (&font_data[0..4] == b"\x00\x01\x00\x00" || &font_data[0..4] == b"true"));

    let is_otf =
        font_path.ends_with(".otf") || (font_data.len() >= 4 && &font_data[0..4] == b"OTTO");

    // Create font stream.
    let font_file_key = if is_truetype {
        "FontFile2"
    } else if is_otf {
        "FontFile3"
    } else {
        "FontFile"
    };

    let mut stream_dict = dictionary! {
        "Length" => Object::Integer(font_data.len() as i64),
    };
    if is_truetype {
        stream_dict.set("Length1", Object::Integer(font_data.len() as i64));
    }
    if is_otf {
        stream_dict.set("Subtype", Object::Name(b"OpenType".to_vec()));
    }

    let font_stream = Stream::new(stream_dict, font_data);
    let stream_id = doc.add_object(Object::Stream(font_stream));

    // Get or create FontDescriptor.
    let fd_id = get_or_create_font_descriptor(doc, font_id)?;

    // Set the font file reference.
    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
        fd.set(font_file_key, Object::Reference(stream_id));
    }

    Ok(())
}

/// Get the FontDescriptor reference from a Font dictionary, or create one.
fn get_or_create_font_descriptor(doc: &mut Document, font_id: ObjectId) -> Result<ObjectId> {
    let existing = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        match font.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(fd_id) = existing {
        return Ok(fd_id);
    }

    // Create a minimal FontDescriptor.
    let font_name = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        get_name(font, b"BaseFont").unwrap_or_else(|| "Unknown".into())
    };

    let fd = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(font_name.into_bytes()),
        "Flags" => Object::Integer(32), // Non-symbolic
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(-200),
            Object::Integer(1000), Object::Integer(800),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
    };
    let fd_id = doc.add_object(Object::Dictionary(fd));

    // Link FontDescriptor to Font.
    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        font.set("FontDescriptor", Object::Reference(fd_id));
    }

    Ok(fd_id)
}

/// Search common system font directories for a font file.
fn find_system_font(font_name: &str) -> Option<String> {
    let clean_name = if font_name.len() > 7 && font_name.as_bytes()[6] == b'+' {
        &font_name[7..]
    } else {
        font_name
    };

    // Build candidate file names.
    let candidates: Vec<String> = vec![
        format!("{clean_name}.ttf"),
        format!("{clean_name}.otf"),
        format!("{clean_name}.TTF"),
        format!("{clean_name}.OTF"),
        // Common variations.
        format!("{}Regular.ttf", clean_name.replace('-', "")),
        format!("{}-Regular.ttf", clean_name),
    ];

    // System font directories by platform.
    let dirs = if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/",
            "/Library/Fonts/",
            "~/Library/Fonts/",
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/share/fonts/",
            "/usr/local/share/fonts/",
            "~/.fonts/",
            "~/.local/share/fonts/",
        ]
    } else {
        vec!["C:\\Windows\\Fonts\\"]
    };

    for dir in &dirs {
        for candidate in &candidates {
            let path = format!("{dir}{candidate}");
            let expanded = path.replace('~', &std::env::var("HOME").unwrap_or_default());
            if std::path::Path::new(&expanded).exists() {
                return Some(expanded);
            }
        }
    }

    None
}

fn check_descendant_embedded(doc: &Document, font_dict: &lopdf::Dictionary) -> bool {
    let descendants = match font_dict.get(b"DescendantFonts").ok() {
        Some(Object::Array(arr)) => arr,
        _ => return false,
    };

    for item in descendants {
        let desc_id = match item {
            Object::Reference(id) => id,
            _ => continue,
        };
        let Some(Object::Dictionary(desc)) = doc.objects.get(desc_id) else {
            continue;
        };
        match desc.get(b"FontDescriptor").ok() {
            Some(Object::Reference(fd_id)) => {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                    if fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3") {
                        return true;
                    }
                }
            }
            _ => continue,
        }
    }
    false
}

fn count_all_fonts(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter(|obj| {
            if let Object::Dictionary(dict) = obj {
                get_name(dict, b"Type").as_deref() == Some("Font")
            } else {
                false
            }
        })
        .count()
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc_with_unembedded_font() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Font WITHOUT FontDescriptor (non-embedded).
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let mut font_res = lopdf::Dictionary::new();
        font_res.set("F1", Object::Reference(font_id));
        let mut res = lopdf::Dictionary::new();
        res.set("Font", Object::Dictionary(font_res));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(res),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_find_non_embedded() {
        let doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        assert_eq!(non_embedded.len(), 1);
        assert_eq!(non_embedded[0].1, "Helvetica");
    }

    #[test]
    fn test_is_standard_14() {
        assert!(is_standard_14("Helvetica"));
        assert!(is_standard_14("ABCDEF+Helvetica"));
        assert!(is_standard_14("Times-Roman"));
        assert!(!is_standard_14("ArialMT"));
    }

    #[test]
    fn test_embedded_font_not_detected() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Font stream.
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(10) },
            vec![0u8; 10],
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));

        // FontDescriptor WITH embedded font file.
        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestFont",
            "FontFile2" => Object::Reference(stream_id),
            "Flags" => Object::Integer(32),
            "FontBBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(1000), Object::Integer(1000),
            ]),
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "TestFont",
            "FontDescriptor" => Object::Reference(fd_id),
        };
        doc.add_object(Object::Dictionary(font));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let non_embedded = find_non_embedded_fonts(&doc);
        assert!(
            non_embedded.is_empty(),
            "embedded font should not be detected"
        );
    }

    #[test]
    fn test_embed_report_structure() {
        let mut doc = make_doc_with_unembedded_font();
        let report = embed_fonts(&mut doc).unwrap();
        assert_eq!(report.fonts_inspected, 1);
        assert_eq!(report.non_embedded_found, 1);
        // Helvetica likely won't be found as a system file,
        // so fonts_embedded may be 0 and failed may have 1 entry.
    }

    #[test]
    fn test_get_or_create_font_descriptor() {
        let mut doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        let font_id = non_embedded[0].0;

        // Should create a FontDescriptor.
        let fd_id = get_or_create_font_descriptor(&mut doc, font_id).unwrap();
        assert!(doc.objects.contains_key(&fd_id));

        // FontDescriptor should be linked.
        if let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) {
            assert!(font.has(b"FontDescriptor"));
        }
    }
}
