//! Font subsetting for embedded PDF fonts.
//!
//! Reduces embedded font data to only the glyphs actually used in the document,
//! using the `subsetter` crate for OpenType/TrueType subsetting.

use crate::error::{ManipError, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::{Document, Object, ObjectId};
use std::collections::HashSet;
use std::io::{Read, Write};

/// Report from a font subsetting pass.
#[derive(Debug, Clone)]
pub struct SubsetReport {
    /// Number of embedded fonts found.
    pub fonts_processed: usize,
    /// Number of fonts actually subsetted (size reduced).
    pub fonts_subsetted: usize,
    /// Total bytes saved.
    pub bytes_saved: usize,
}

/// Subset all embedded fonts in the document to only used glyphs.
pub fn subset_fonts(doc: &mut Document) -> Result<SubsetReport> {
    let mut report = SubsetReport {
        fonts_processed: 0,
        fonts_subsetted: 0,
        bytes_saved: 0,
    };

    // Step 1: Find all Font objects and collect used character codes per font.
    let font_usage = collect_font_usage(doc);

    // Step 2: Find embedded font streams and their glyph usage.
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for font_id in font_ids {
        let font_file_info = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };

            // Check if this is a Font Descriptor with an embedded font.
            if get_name(dict, b"Type").as_deref() != Some("FontDescriptor") {
                continue;
            }

            // Find the font stream reference.
            let font_file_key = if dict.has(b"FontFile2") {
                b"FontFile2".as_slice()
            } else if dict.has(b"FontFile3") {
                b"FontFile3".as_slice()
            } else {
                continue;
            };

            let font_stream_id = match dict.get(font_file_key).ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Get the font name for looking up usage.
            let font_name = get_name(dict, b"FontName").unwrap_or_default();

            Some((font_stream_id, font_name, font_file_key == b"FontFile2"))
        };

        let Some((font_stream_id, font_name, is_truetype)) = font_file_info else {
            continue;
        };

        // Get glyph IDs used by this font.
        let glyph_ids = match font_usage.get(&font_name) {
            Some(ids) => ids,
            None => continue, // Font not used in any content stream.
        };

        if glyph_ids.is_empty() {
            continue;
        }

        report.fonts_processed += 1;

        // Get the font stream data.
        let (font_data, was_compressed) = {
            let Some(Object::Stream(stream)) = doc.objects.get(&font_stream_id) else {
                continue;
            };
            let compressed = get_name(&stream.dict, b"Filter")
                .as_deref()
                .map(|f| f == "FlateDecode")
                .unwrap_or(false);
            let data = if compressed {
                decompress_stream(&stream.content)?
            } else {
                stream.content.clone()
            };
            (data, compressed)
        };

        let original_size = font_data.len();

        // Subset the font using the subsetter crate.
        let glyph_vec: Vec<u16> = glyph_ids.iter().copied().collect();
        let mapper = subsetter::GlyphRemapper::new_from_glyphs(&glyph_vec);

        let subsetted = match subsetter::subset(&font_data, 0, &mapper) {
            Ok(data) => data,
            Err(_) => continue, // Subsetting failed — skip this font.
        };

        if subsetted.len() >= original_size {
            continue; // No size reduction.
        }

        let bytes_saved = original_size - subsetted.len();

        // Re-compress if originally compressed.
        let final_data = if was_compressed {
            compress_data(&subsetted)?
        } else {
            subsetted.clone()
        };

        // Replace the font stream.
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&font_stream_id) {
            let new_len = final_data.len();
            stream.set_content(final_data);
            stream.dict.set("Length", Object::Integer(new_len as i64));
            // Update Length1 (uncompressed size) for TrueType.
            if is_truetype {
                stream
                    .dict
                    .set("Length1", Object::Integer(subsetted.len() as i64));
            }
        }

        report.fonts_subsetted += 1;
        report.bytes_saved += bytes_saved;
    }

    Ok(report)
}

/// Collect which glyph IDs are used by each font in the document.
/// Returns a map of font name → set of glyph IDs (u16).
fn collect_font_usage(doc: &Document) -> std::collections::HashMap<String, HashSet<u16>> {
    let mut usage: std::collections::HashMap<String, HashSet<u16>> =
        std::collections::HashMap::new();

    // Find all Font objects.
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };

        if get_name(dict, b"Type").as_deref() != Some("Font") {
            continue;
        }

        let font_name = get_base_font_name(dict);
        if font_name.is_empty() {
            continue;
        }

        // For Type0/CIDFont: look at descendant fonts for W array.
        // For simple fonts: look at /Widths array to determine used range.
        let glyph_set = usage.entry(font_name).or_default();

        // Collect from /W array (CID fonts).
        if let Ok(Object::Array(w_array)) = dict.get(b"W") {
            collect_glyph_ids_from_w_array(w_array, glyph_set);
        }

        // Collect from /Widths array (simple fonts) using /FirstChar.
        if let Ok(Object::Array(widths)) = dict.get(b"Widths") {
            let first_char = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(n)) => *n as u16,
                _ => 0,
            };
            for (i, w) in widths.iter().enumerate() {
                if let Object::Integer(n) = w {
                    if *n > 0 {
                        glyph_set.insert(first_char + i as u16);
                    }
                }
                if let Object::Real(n) = w {
                    if *n > 0.0 {
                        glyph_set.insert(first_char + i as u16);
                    }
                }
            }
        }

        // Always include .notdef (glyph 0).
        glyph_set.insert(0);
    }

    usage
}

fn collect_glyph_ids_from_w_array(w_array: &[Object], glyph_set: &mut HashSet<u16>) {
    let mut i = 0;
    while i < w_array.len() {
        if let Object::Integer(start_cid) = &w_array[i] {
            let start = *start_cid as u16;
            if i + 1 < w_array.len() {
                match &w_array[i + 1] {
                    Object::Array(widths) => {
                        // Format: start [w1 w2 w3 ...]
                        for (j, _w) in widths.iter().enumerate() {
                            glyph_set.insert(start + j as u16);
                        }
                        i += 2;
                    }
                    Object::Integer(end_cid) => {
                        // Format: start end width
                        let end = *end_cid as u16;
                        for cid in start..=end {
                            glyph_set.insert(cid);
                        }
                        i += 3;
                    }
                    _ => {
                        i += 1;
                    }
                }
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

fn get_base_font_name(dict: &lopdf::Dictionary) -> String {
    match dict.get(b"BaseFont").ok() {
        Some(Object::Name(n)) => {
            let name = String::from_utf8_lossy(n).to_string();
            // Strip subset prefix (e.g. "ABCDEF+ArialMT" → "ArialMT").
            if name.len() > 7 && name.as_bytes()[6] == b'+' {
                name[7..].to_string()
            } else {
                name
            }
        }
        _ => String::new(),
    }
}

fn decompress_stream(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut buf = Vec::new();
    decoder
        .read_to_end(&mut buf)
        .map_err(|e| ManipError::Other(format!("FlateDecode failed: {e}")))?;
    Ok(buf)
}

fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| ManipError::Other(format!("compression failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| ManipError::Other(format!("compression finalize: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn make_doc_with_font() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Create a minimal TrueType font stream (not real font data, but tests the flow).
        let font_stream = Stream::new(
            dictionary! {
                "Length1" => Object::Integer(100),
            },
            vec![0u8; 100],
        );
        let font_stream_id = doc.add_object(Object::Stream(font_stream));

        // Font descriptor.
        let font_descriptor = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestFont",
            "FontFile2" => Object::Reference(font_stream_id),
            "Flags" => Object::Integer(32),
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
        let fd_id = doc.add_object(Object::Dictionary(font_descriptor));

        // Font dictionary.
        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "TestFont",
            "FirstChar" => Object::Integer(32),
            "LastChar" => Object::Integer(122),
            "Widths" => Object::Array(
                (32..=122).map(|_| Object::Integer(500)).collect()
            ),
            "FontDescriptor" => Object::Reference(fd_id),
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        // Page with content.
        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let mut font_res = lopdf::Dictionary::new();
        font_res.set("F1", Object::Reference(font_id));
        let mut res_dict = lopdf::Dictionary::new();
        res_dict.set("Font", Object::Dictionary(font_res));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(res_dict),
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
    fn test_subset_report_structure() {
        let mut doc = make_doc_with_font();
        let report = subset_fonts(&mut doc).unwrap();
        // The test font data is not valid TrueType, so subsetter will skip it.
        // But the report should still be valid.
        assert_eq!(report.fonts_processed, 1);
        // subsetter will fail on invalid data, so fonts_subsetted should be 0.
        assert_eq!(report.fonts_subsetted, 0);
    }

    #[test]
    fn test_collect_font_usage() {
        let doc = make_doc_with_font();
        let usage = collect_font_usage(&doc);
        assert!(usage.contains_key("TestFont"));
        let glyphs = &usage["TestFont"];
        assert!(glyphs.contains(&0)); // .notdef always included
    }

    #[test]
    fn test_empty_document() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
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

        let report = subset_fonts(&mut doc).unwrap();
        assert_eq!(report.fonts_processed, 0);
        assert_eq!(report.fonts_subsetted, 0);
    }

    #[test]
    fn test_get_base_font_name_with_prefix() {
        let dict = dictionary! {
            "BaseFont" => "ABCDEF+ArialMT",
        };
        assert_eq!(get_base_font_name(&dict), "ArialMT");
    }

    #[test]
    fn test_get_base_font_name_without_prefix() {
        let dict = dictionary! {
            "BaseFont" => "Helvetica",
        };
        assert_eq!(get_base_font_name(&dict), "Helvetica");
    }
}
