//! Image extraction from PDF documents.
//!
//! Iterates over all XObject Image streams and decodes them based on their filter.

use crate::error::{ExtractError, Result};
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::io::Read;

/// The compression filter applied to an image stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFilter {
    /// DCT (JPEG) compression.
    Jpeg,
    /// Flate (zlib/deflate) compression.
    Flate,
    /// JBIG2 compression.
    Jbig2,
    /// JPX (JPEG 2000) compression.
    Jpx,
    /// CCITT fax compression.
    CcittFax,
    /// No compression (raw).
    Raw,
    /// Unknown or unsupported filter.
    Unknown(String),
}

/// An image extracted from a PDF document.
#[derive(Debug, Clone)]
pub struct ExtractedImage {
    /// The PDF object ID of the image stream.
    pub object_id: ObjectId,
    /// The page number (1-based) where this image appears.
    pub page: u32,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Bits per color component.
    pub bits_per_component: u32,
    /// Color space name (e.g., "DeviceRGB", "DeviceGray").
    pub color_space: String,
    /// The compression filter used.
    pub filter: ImageFilter,
    /// The decoded (or raw) image data.
    pub data: Vec<u8>,
}

/// Extract all images from all pages of a PDF document.
pub fn extract_all_images(doc: &Document) -> Result<Vec<ExtractedImage>> {
    let page_map = build_page_image_map(doc);
    let mut images = Vec::new();

    for (page_num, obj_ids) in &page_map {
        for &obj_id in obj_ids {
            if let Ok(img) = decode_image(doc, obj_id, *page_num) {
                images.push(img);
            }
        }
    }

    Ok(images)
}

/// Extract images from a specific page (1-based page number).
pub fn extract_page_images(doc: &Document, page_num: u32) -> Result<Vec<ExtractedImage>> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    if page_num == 0 || page_num > total {
        return Err(ExtractError::PageOutOfRange(page_num, total));
    }

    let page_id = *pages
        .get(&page_num)
        .ok_or(ExtractError::PageOutOfRange(page_num, total))?;

    let obj_ids = collect_page_xobject_ids(doc, page_id);
    let mut images = Vec::new();

    for obj_id in obj_ids {
        if let Ok(img) = decode_image(doc, obj_id, page_num) {
            images.push(img);
        }
    }

    Ok(images)
}

/// Check if a stream object is an image XObject.
fn is_image_stream(dict: &lopdf::Dictionary) -> bool {
    if let Ok(subtype) = dict.get(b"Subtype") {
        if let Ok(name) = subtype.as_name() {
            return name == b"Image";
        }
    }
    false
}

/// Get the filter from a stream dictionary.
fn get_filter(dict: &lopdf::Dictionary) -> ImageFilter {
    if let Ok(filter_obj) = dict.get(b"Filter") {
        match filter_obj {
            Object::Name(name) => filter_from_name(name),
            Object::Array(arr) => {
                // Use the first filter in the array.
                if let Some(Object::Name(name)) = arr.first() {
                    filter_from_name(name)
                } else {
                    ImageFilter::Raw
                }
            }
            _ => ImageFilter::Raw,
        }
    } else {
        ImageFilter::Raw
    }
}

/// Convert a filter name to an `ImageFilter` variant.
fn filter_from_name(name: &[u8]) -> ImageFilter {
    match name {
        b"DCTDecode" => ImageFilter::Jpeg,
        b"FlateDecode" => ImageFilter::Flate,
        b"JBIG2Decode" => ImageFilter::Jbig2,
        b"JPXDecode" => ImageFilter::Jpx,
        b"CCITTFaxDecode" => ImageFilter::CcittFax,
        _ => ImageFilter::Unknown(String::from_utf8_lossy(name).to_string()),
    }
}

/// Get color space from an image stream dictionary.
fn get_color_space(dict: &lopdf::Dictionary) -> String {
    if let Ok(cs) = dict.get(b"ColorSpace") {
        match cs {
            Object::Name(name) => String::from_utf8_lossy(name).to_string(),
            Object::Array(arr) => {
                if let Some(Object::Name(name)) = arr.first() {
                    String::from_utf8_lossy(name).to_string()
                } else {
                    "Unknown".to_string()
                }
            }
            _ => "Unknown".to_string(),
        }
    } else {
        "Unknown".to_string()
    }
}

/// Get an integer value from a dictionary key.
fn get_int(dict: &lopdf::Dictionary, key: &[u8]) -> u32 {
    dict.get(key)
        .ok()
        .and_then(|v| match v {
            Object::Integer(i) => Some(*i as u32),
            _ => None,
        })
        .unwrap_or(0)
}

/// Decode an image from a PDF object.
fn decode_image(doc: &Document, obj_id: ObjectId, page: u32) -> Result<ExtractedImage> {
    let obj = doc
        .get_object(obj_id)
        .map_err(|e| ExtractError::Other(format!("object not found: {e}")))?;

    let stream = match obj {
        Object::Stream(ref s) => s,
        _ => return Err(ExtractError::Other("not a stream object".into())),
    };

    let dict = &stream.dict;
    if !is_image_stream(dict) {
        return Err(ExtractError::Other("not an image stream".into()));
    }

    let width = get_int(dict, b"Width");
    let height = get_int(dict, b"Height");
    let bits_per_component = get_int(dict, b"BitsPerComponent");
    let color_space = get_color_space(dict);
    let filter = get_filter(dict);

    let data = match filter {
        ImageFilter::Jpeg | ImageFilter::Jbig2 | ImageFilter::Jpx | ImageFilter::CcittFax => {
            // For these formats, return the raw compressed bytes.
            get_raw_stream_bytes(stream)
        }
        ImageFilter::Flate => {
            let raw = get_raw_stream_bytes(stream);
            decompress_flate(&raw).unwrap_or(raw)
        }
        ImageFilter::Raw => get_raw_stream_bytes(stream),
        ImageFilter::Unknown(_) => get_raw_stream_bytes(stream),
    };

    Ok(ExtractedImage {
        object_id: obj_id,
        page,
        width,
        height,
        bits_per_component,
        color_space,
        filter,
        data,
    })
}

/// Get raw bytes from a stream (without decompression).
fn get_raw_stream_bytes(stream: &lopdf::Stream) -> Vec<u8> {
    stream.content.clone()
}

/// Get decompressed stream bytes using lopdf's built-in decompression.
#[allow(dead_code)]
fn get_stream_bytes(stream: &lopdf::Stream, _doc: &Document) -> Vec<u8> {
    let mut s = stream.clone();
    if s.decompress().is_ok() {
        s.content.clone()
    } else {
        stream.content.clone()
    }
}

/// Maximum bytes to decompress from a single image stream (64 MB).
/// Prevents zip-bomb hangs on pathological PDFs.
const FLATE_MAX_DECOMPRESS_BYTES: u64 = 64 * 1024 * 1024;

/// Decompress flate-encoded data, capped at `FLATE_MAX_DECOMPRESS_BYTES`.
fn decompress_flate(data: &[u8]) -> std::result::Result<Vec<u8>, std::io::Error> {
    let decoder = flate2::read::ZlibDecoder::new(data);
    let mut decoded = Vec::new();
    decoder
        .take(FLATE_MAX_DECOMPRESS_BYTES)
        .read_to_end(&mut decoded)?;
    Ok(decoded)
}

/// Build a map of page number -> list of image object IDs.
fn build_page_image_map(doc: &Document) -> BTreeMap<u32, Vec<ObjectId>> {
    let mut map = BTreeMap::new();
    let pages = doc.get_pages();

    for (&page_num, &page_id) in &pages {
        let ids = collect_page_xobject_ids(doc, page_id);
        if !ids.is_empty() {
            map.insert(page_num, ids);
        }
    }

    map
}

/// Collect all XObject IDs referenced by a page that are image streams.
fn collect_page_xobject_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut ids = Vec::new();

    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return ids,
    };

    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return ids,
    };

    // Get Resources dict.
    let resources = match page_dict.get(b"Resources") {
        Ok(res_obj) => match res_obj {
            Object::Dictionary(ref d) => d.clone(),
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => return ids,
            },
            _ => return ids,
        },
        Err(_) => return ids,
    };

    // Get XObject dict from Resources.
    let xobjects = match resources.get(b"XObject") {
        Ok(xo) => match xo {
            Object::Dictionary(ref d) => d.clone(),
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => return ids,
            },
            _ => return ids,
        },
        Err(_) => return ids,
    };

    for (_name, obj) in xobjects.iter() {
        if let Object::Reference(obj_id) = obj {
            if let Ok(Object::Stream(ref stream)) = doc.get_object(*obj_id) {
                if is_image_stream(&stream.dict) {
                    ids.push(*obj_id);
                }
            }
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    /// Helper: create a minimal PDF document with a JPEG image on page 1.
    fn make_doc_with_jpeg_image() -> Document {
        let mut doc = Document::with_version("1.7");

        let img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 100_i64,
            "Height" => 50_i64,
            "BitsPerComponent" => 8_i64,
            "ColorSpace" => "DeviceRGB",
            "Filter" => "DCTDecode",
        };
        let img_stream = Stream::new(img_dict, vec![0xFF, 0xD8, 0xFF, 0xE0]);
        let img_id = doc.add_object(Object::Stream(img_stream));

        let xobject_dict = dictionary! {
            "Im0" => Object::Reference(img_id),
        };
        let resources_dict = dictionary! {
            "XObject" => Object::Dictionary(xobject_dict),
        };
        let content_data = b"q 100 0 0 50 0 0 cm /Im0 Do Q".to_vec();
        let content_stream = Stream::new(dictionary! {}, content_data);
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => Object::Dictionary(resources_dict),
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

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Helper: create a doc with a raw (uncompressed) image.
    fn make_doc_with_raw_image() -> Document {
        let mut doc = Document::with_version("1.7");

        let img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 2_i64,
            "Height" => 2_i64,
            "BitsPerComponent" => 8_i64,
            "ColorSpace" => "DeviceRGB",
        };
        // 2x2 RGB image = 12 bytes.
        let img_stream = Stream::new(img_dict, vec![255; 12]);
        let img_id = doc.add_object(Object::Stream(img_stream));

        let xobject_dict = dictionary! {
            "Im0" => Object::Reference(img_id),
        };
        let resources_dict = dictionary! {
            "XObject" => Object::Dictionary(xobject_dict),
        };
        let content_data = b"q 2 0 0 2 0 0 cm /Im0 Do Q".to_vec();
        let content_stream = Stream::new(dictionary! {}, content_data);
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => Object::Dictionary(resources_dict),
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

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Helper: create a doc with a flate-compressed image.
    fn make_doc_with_flate_image() -> Document {
        let mut doc = Document::with_version("1.7");

        // Compress raw data with flate.
        let raw_data = vec![128u8; 12]; // 2x2 RGB
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &raw_data).unwrap();
        let compressed = encoder.finish().unwrap();

        let img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 2_i64,
            "Height" => 2_i64,
            "BitsPerComponent" => 8_i64,
            "ColorSpace" => "DeviceRGB",
            "Filter" => "FlateDecode",
        };
        let img_stream = Stream::new(img_dict, compressed);
        let img_id = doc.add_object(Object::Stream(img_stream));

        let xobject_dict = dictionary! {
            "Im0" => Object::Reference(img_id),
        };
        let resources_dict = dictionary! {
            "XObject" => Object::Dictionary(xobject_dict),
        };
        let content_data = b"q 2 0 0 2 0 0 cm /Im0 Do Q".to_vec();
        let content_stream = Stream::new(dictionary! {}, content_data);
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => Object::Dictionary(resources_dict),
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

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn extract_jpeg_image() {
        let doc = make_doc_with_jpeg_image();
        let images = extract_all_images(&doc).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width, 100);
        assert_eq!(images[0].height, 50);
        assert_eq!(images[0].bits_per_component, 8);
        assert_eq!(images[0].color_space, "DeviceRGB");
        assert_eq!(images[0].filter, ImageFilter::Jpeg);
        assert_eq!(images[0].page, 1);
    }

    #[test]
    fn extract_from_specific_page() {
        let doc = make_doc_with_jpeg_image();
        let images = extract_page_images(&doc, 1).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filter, ImageFilter::Jpeg);
    }

    #[test]
    fn extract_page_out_of_range() {
        let doc = make_doc_with_jpeg_image();
        let result = extract_page_images(&doc, 5);
        assert!(result.is_err());
    }

    #[test]
    fn extract_raw_image() {
        let doc = make_doc_with_raw_image();
        let images = extract_all_images(&doc).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filter, ImageFilter::Raw);
        assert_eq!(images[0].data.len(), 12);
    }

    #[test]
    fn extract_flate_compressed_image() {
        let doc = make_doc_with_flate_image();
        let images = extract_all_images(&doc).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filter, ImageFilter::Flate);
        // After decompression, we should have 12 bytes (2x2 RGB).
        assert_eq!(images[0].data.len(), 12);
    }

    #[test]
    fn no_images_returns_empty() {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
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

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let images = extract_all_images(&doc).unwrap();
        assert!(images.is_empty());
    }
}
