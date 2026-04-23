//! Insert JPEG/PNG images into PDF pages as Image XObjects.
//!
//! - JPEG: passthrough (raw bytes as DCTDecode stream, no re-encoding)
//! - PNG: decode to raw pixels, FlateDecode stream + SMask for alpha channel

use crate::error::{ManipError, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::GenericImageView;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::io::Write;

/// Conservative ceiling that still admits practical large images while rejecting absurd headers.
pub(crate) const MAX_IMAGE_DIMENSION: u32 = 65_535;
/// 256 MiB covers roughly an 8K RGBA working buffer without allowing unbounded allocations.
pub(crate) const MAX_IMAGE_ALLOCATION_BYTES: usize = 256 * 1024 * 1024;

/// Image format for insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// JPEG image (passthrough, no re-encoding).
    Jpeg,
    /// PNG image (decoded to raw pixels + optional alpha SMask).
    Png,
}

/// Configuration for inserting an image into a PDF page.
#[derive(Debug, Clone)]
pub struct ImageInsert {
    /// Raw image file data (JPEG or PNG bytes).
    pub image_data: Vec<u8>,
    /// Image format.
    pub format: ImageFormat,
    /// X position in PDF points from bottom-left.
    pub x: f64,
    /// Y position in PDF points from bottom-left.
    pub y: f64,
    /// Display width in PDF points.
    pub width: f64,
    /// Display height in PDF points.
    pub height: f64,
    /// Target page index (1-based).
    pub page_index: u32,
    /// Optional opacity (0.0 = invisible, 1.0 = fully opaque).
    pub opacity: Option<f64>,
}

/// Result of an image insertion.
#[derive(Debug, Clone)]
pub struct ImageInsertResult {
    /// Object ID of the created Image XObject.
    pub image_object_id: ObjectId,
    /// Resource name used in the content stream (e.g. "Im1").
    pub resource_name: String,
    /// Pixel width of the image.
    pub pixel_width: u32,
    /// Pixel height of the image.
    pub pixel_height: u32,
}

/// Insert an image into a PDF page.
pub fn insert_image(doc: &mut Document, insert: &ImageInsert) -> Result<ImageInsertResult> {
    if insert.page_index == 0 {
        return Err(ManipError::PageOutOfRange(0, doc.get_pages().len()));
    }

    let pages = doc.get_pages();
    let page_id = *pages
        .get(&insert.page_index)
        .ok_or(ManipError::PageOutOfRange(
            insert.page_index as usize,
            pages.len(),
        ))?;

    let (image_id, pixel_width, pixel_height) = match insert.format {
        ImageFormat::Jpeg => create_jpeg_xobject(doc, &insert.image_data)?,
        ImageFormat::Png => create_png_xobject(doc, &insert.image_data)?,
    };

    // Generate a unique resource name.
    let resource_name = generate_unique_image_name(doc, page_id);

    // Create ExtGState for opacity if needed.
    let gs_name = if let Some(opacity) = insert.opacity {
        if (opacity - 1.0).abs() > f64::EPSILON {
            let gs_dict = dictionary! {
                "Type" => "ExtGState",
                "ca" => Object::Real(opacity as f32),
                "CA" => Object::Real(opacity as f32),
            };
            let gs_id = doc.add_object(Object::Dictionary(gs_dict));
            let gs_name = format!("GS_{resource_name}");
            ensure_page_resource(doc, page_id, "ExtGState", &gs_name, gs_id);
            Some(gs_name)
        } else {
            None
        }
    } else {
        None
    };

    // Register the image XObject in page resources.
    ensure_page_resource(doc, page_id, "XObject", &resource_name, image_id);

    // Build content stream: q + optional gs + CTM + Do + Q
    let ops = build_image_ops(
        &resource_name,
        gs_name.as_deref(),
        insert.x,
        insert.y,
        insert.width,
        insert.height,
    );

    let content_data = Content { operations: ops }
        .encode()
        .map_err(|e| ManipError::Image(format!("failed to encode image content: {e}")))?;

    let stream = Stream::new(dictionary! {}, content_data);
    let stream_id = doc.add_object(Object::Stream(stream));

    // Append content stream to page (foreground).
    append_content_to_page(doc, page_id, stream_id);

    Ok(ImageInsertResult {
        image_object_id: image_id,
        resource_name,
        pixel_width,
        pixel_height,
    })
}

/// Detect image format from magic bytes.
pub fn detect_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        Some(ImageFormat::Jpeg)
    } else if data.len() >= 8 && data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        Some(ImageFormat::Png)
    } else {
        None
    }
}

/// Create a JPEG Image XObject (passthrough — raw bytes, no re-encoding).
fn create_jpeg_xobject(doc: &mut Document, jpeg_data: &[u8]) -> Result<(ObjectId, u32, u32)> {
    let (width, height, components) = parse_jpeg_dimensions(jpeg_data)?;

    let color_space = match components {
        1 => Object::Name(b"DeviceGray".to_vec()),
        3 => Object::Name(b"DeviceRGB".to_vec()),
        4 => Object::Name(b"DeviceCMYK".to_vec()),
        _ => Object::Name(b"DeviceRGB".to_vec()),
    };

    let stream_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => Object::Integer(width as i64),
        "Height" => Object::Integer(height as i64),
        "BitsPerComponent" => Object::Integer(8),
        "ColorSpace" => color_space,
        "Filter" => "DCTDecode",
        "Length" => Object::Integer(jpeg_data.len() as i64),
    };

    let stream = Stream::new(stream_dict, jpeg_data.to_vec());
    let id = doc.add_object(Object::Stream(stream));

    Ok((id, width, height))
}

/// Create a PNG Image XObject (decode to raw pixels, FlateDecode, optional SMask).
fn create_png_xobject(doc: &mut Document, png_data: &[u8]) -> Result<(ObjectId, u32, u32)> {
    let (width, height, png_bytes_per_pixel) = parse_png_header(png_data)?;
    checked_image_allocation_len(width, height, png_bytes_per_pixel)?;

    let img = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
        .map_err(|e| ManipError::Image(format!("failed to decode PNG: {e}")))?;

    let (width, height) = img.dimensions();
    let has_alpha = img.color().has_alpha();

    let (raw_rgb, alpha_channel) = if has_alpha {
        checked_image_allocation_len(width, height, 4)?;
        let rgba = img.to_rgba8();
        let pixel_count = checked_image_allocation_len(width, height, 1)?;
        let mut rgb = Vec::with_capacity(pixel_count.checked_mul(3).ok_or_else(|| {
            ManipError::ImageTooLarge {
                width,
                height,
                bytes_per_pixel: 4,
                max_dimension: MAX_IMAGE_DIMENSION,
                max_allocation_bytes: MAX_IMAGE_ALLOCATION_BYTES,
            }
        })?);
        let mut alpha = Vec::with_capacity(pixel_count);
        for pixel in rgba.pixels() {
            rgb.extend_from_slice(&pixel.0[..3]);
            alpha.push(pixel.0[3]);
        }
        (rgb, Some(alpha))
    } else {
        checked_image_allocation_len(width, height, 3)?;
        (img.to_rgb8().into_raw(), None)
    };

    // Compress raw RGB data.
    let compressed_rgb = flate_compress(&raw_rgb)?;

    let mut stream_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => Object::Integer(width as i64),
        "Height" => Object::Integer(height as i64),
        "BitsPerComponent" => Object::Integer(8),
        "ColorSpace" => "DeviceRGB",
        "Filter" => "FlateDecode",
        "Length" => Object::Integer(compressed_rgb.len() as i64),
    };

    // Create SMask for alpha channel if present.
    if let Some(alpha) = alpha_channel {
        let compressed_alpha = flate_compress(&alpha)?;
        let smask_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "BitsPerComponent" => Object::Integer(8),
            "ColorSpace" => "DeviceGray",
            "Filter" => "FlateDecode",
            "Length" => Object::Integer(compressed_alpha.len() as i64),
        };
        let smask_stream = Stream::new(smask_dict, compressed_alpha);
        let smask_id = doc.add_object(Object::Stream(smask_stream));
        stream_dict.set("SMask", Object::Reference(smask_id));
    }

    let stream = Stream::new(stream_dict, compressed_rgb);
    let id = doc.add_object(Object::Stream(stream));

    Ok((id, width, height))
}

fn parse_png_header(data: &[u8]) -> Result<(u32, u32, u32)> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    if data.len() < 29 || data[..8] != PNG_SIGNATURE {
        return Err(ManipError::Image("not a valid PNG".into()));
    }

    let ihdr_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    if ihdr_len != 13 || &data[12..16] != b"IHDR" {
        return Err(ManipError::Image("invalid PNG IHDR".into()));
    }

    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    let color_type = data[25];
    let bytes_per_pixel = match color_type {
        0 | 2 => 3,
        _ => 4,
    };

    Ok((width, height, bytes_per_pixel))
}

pub(crate) fn checked_image_allocation_len(
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
) -> Result<usize> {
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(ManipError::ImageTooLarge {
            width,
            height,
            bytes_per_pixel,
            max_dimension: MAX_IMAGE_DIMENSION,
            max_allocation_bytes: MAX_IMAGE_ALLOCATION_BYTES,
        });
    }

    let allocation_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or_else(|| ManipError::ImageTooLarge {
            width,
            height,
            bytes_per_pixel,
            max_dimension: MAX_IMAGE_DIMENSION,
            max_allocation_bytes: MAX_IMAGE_ALLOCATION_BYTES,
        })? as usize;

    if allocation_len > MAX_IMAGE_ALLOCATION_BYTES {
        return Err(ManipError::ImageTooLarge {
            width,
            height,
            bytes_per_pixel,
            max_dimension: MAX_IMAGE_DIMENSION,
            max_allocation_bytes: MAX_IMAGE_ALLOCATION_BYTES,
        });
    }

    Ok(allocation_len)
}

/// Parse JPEG dimensions from SOF marker.
fn parse_jpeg_dimensions(data: &[u8]) -> Result<(u32, u32, u8)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(ManipError::Image("not a valid JPEG".into()));
    }

    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            return Err(ManipError::Image("invalid JPEG marker".into()));
        }

        let marker = data[i + 1];

        // Skip padding bytes.
        if marker == 0xFF {
            i += 1;
            continue;
        }

        // SOF markers (SOF0..SOF15, excluding DHT/DAC/RST/SOI/EOI/SOS).
        let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);

        if is_sof {
            if i + 9 >= data.len() {
                return Err(ManipError::Image("truncated JPEG SOF".into()));
            }
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            let components = data[i + 9];
            return Ok((width, height, components));
        }

        // Skip this marker segment.
        if i + 3 >= data.len() {
            break;
        }
        let segment_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        i += 2 + segment_len;
    }

    Err(ManipError::Image("no SOF marker found in JPEG".into()))
}

/// FlateDecode compress data.
fn flate_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| ManipError::Image(format!("compression failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| ManipError::Image(format!("compression finalize failed: {e}")))
}

/// Generate a unique image resource name for a page (Im1, Im2, ...).
fn generate_unique_image_name(doc: &Document, page_id: ObjectId) -> String {
    let mut n = 1u32;
    if let Some(Object::Dictionary(page_dict)) = doc.objects.get(&page_id) {
        if let Ok(Object::Dictionary(res)) = page_dict.get(b"Resources") {
            if let Ok(Object::Dictionary(xobjects)) = res.get(b"XObject") {
                loop {
                    let name = format!("Im{n}");
                    if !xobjects.has(name.as_bytes()) {
                        return name;
                    }
                    n += 1;
                }
            }
        }
    }
    format!("Im{n}")
}

/// Ensure a page has a named resource entry in the given sub-dictionary.
fn ensure_page_resource(
    doc: &mut Document,
    page_id: ObjectId,
    category: &str,
    name: &str,
    obj_id: ObjectId,
) {
    if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
        let has_resources = page_dict.get(b"Resources").ok().is_some();
        if !has_resources {
            let mut cat_dict = lopdf::Dictionary::new();
            cat_dict.set(name, Object::Reference(obj_id));
            let mut res_dict = lopdf::Dictionary::new();
            res_dict.set(category, Object::Dictionary(cat_dict));
            page_dict.set("Resources", Object::Dictionary(res_dict));
            return;
        }
        if let Ok(Object::Dictionary(ref mut res)) = page_dict.get_mut(b"Resources") {
            if let Ok(Object::Dictionary(ref mut cat_d)) = res.get_mut(category.as_bytes()) {
                cat_d.set(name, Object::Reference(obj_id));
            } else {
                let mut cat_dict = lopdf::Dictionary::new();
                cat_dict.set(name, Object::Reference(obj_id));
                res.set(category, Object::Dictionary(cat_dict));
            }
        }
    }
}

/// Build content stream operations for placing an image.
fn build_image_ops(
    image_name: &str,
    gs_name: Option<&str>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Vec<Operation> {
    let mut ops = Vec::new();

    // Save graphics state.
    ops.push(Operation::new("q", vec![]));

    // Set opacity via ExtGState if provided.
    if let Some(gs) = gs_name {
        ops.push(Operation::new(
            "gs",
            vec![Object::Name(gs.as_bytes().to_vec())],
        ));
    }

    // CTM: [width 0 0 height x y] — scales 1x1 image unit to desired size and position.
    ops.push(Operation::new(
        "cm",
        vec![
            Object::Real(width as f32),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(height as f32),
            Object::Real(x as f32),
            Object::Real(y as f32),
        ],
    ));

    // Paint the image XObject.
    ops.push(Operation::new(
        "Do",
        vec![Object::Name(image_name.as_bytes().to_vec())],
    ));

    // Restore graphics state.
    ops.push(Operation::new("Q", vec![]));

    ops
}

/// Append a content stream to a page (foreground).
fn append_content_to_page(doc: &mut Document, page_id: ObjectId, stream_id: ObjectId) {
    if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
        let existing = page_dict.get(b"Contents").ok().cloned();
        let new_contents = match existing {
            Some(Object::Reference(existing_id)) => Object::Array(vec![
                Object::Reference(existing_id),
                Object::Reference(stream_id),
            ]),
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Reference(stream_id));
                Object::Array(arr)
            }
            _ => Object::Reference(stream_id),
        };
        page_dict.set("Contents", new_contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(lopdf::Dictionary::new()),
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

    // Minimal valid JPEG: SOI + SOF0 marker with 2x2 RGB + EOI.
    fn minimal_jpeg() -> Vec<u8> {
        // A minimal JPEG with SOI, APP0, SOF0 (2x2, 3 components), SOS, scan data, EOI.
        // We only need it to parse dimensions; lopdf won't decode it.
        let mut data = Vec::new();
        // SOI
        data.extend_from_slice(&[0xFF, 0xD8]);
        // SOF0 marker
        data.extend_from_slice(&[0xFF, 0xC0]);
        // Length = 17 (for 3 components)
        data.extend_from_slice(&[0x00, 0x11]);
        // Precision = 8
        data.push(0x08);
        // Height = 2
        data.extend_from_slice(&[0x00, 0x02]);
        // Width = 2
        data.extend_from_slice(&[0x00, 0x02]);
        // Components = 3
        data.push(0x03);
        // Component specs (3x3 bytes)
        for id in 1..=3u8 {
            data.push(id); // component id
            data.push(0x11); // sampling factors
            data.push(0x00); // quant table
        }
        // EOI
        data.extend_from_slice(&[0xFF, 0xD9]);
        data
    }

    // Minimal valid PNG: 1x1 red pixel.
    fn minimal_png() -> Vec<u8> {
        use std::io::Cursor;
        let mut buf = Cursor::new(Vec::new());
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128]));
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn oversized_png_header(width: u32, height: u32, color_type: u8) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        data.extend_from_slice(&13u32.to_be_bytes());
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.push(8);
        data.push(color_type);
        data.push(0);
        data.push(0);
        data.push(0);
        data.extend_from_slice(&0u32.to_be_bytes());
        data
    }

    #[test]
    fn test_detect_format() {
        let jpeg = minimal_jpeg();
        assert_eq!(detect_format(&jpeg), Some(ImageFormat::Jpeg));

        let png = minimal_png();
        assert_eq!(detect_format(&png), Some(ImageFormat::Png));

        assert_eq!(detect_format(&[0x00, 0x01, 0x02]), None);
    }

    #[test]
    fn test_parse_jpeg_dimensions() {
        let jpeg = minimal_jpeg();
        let (w, h, c) = parse_jpeg_dimensions(&jpeg).unwrap();
        assert_eq!((w, h, c), (2, 2, 3));
    }

    #[test]
    fn test_insert_jpeg() {
        let mut doc = make_test_doc();
        let jpeg = minimal_jpeg();

        let result = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: jpeg,
                format: ImageFormat::Jpeg,
                x: 100.0,
                y: 200.0,
                width: 200.0,
                height: 150.0,
                page_index: 1,
                opacity: None,
            },
        )
        .unwrap();

        assert_eq!(result.resource_name, "Im1");
        assert_eq!(result.pixel_width, 2);
        assert_eq!(result.pixel_height, 2);

        // Verify image XObject was created.
        let obj = doc.objects.get(&result.image_object_id).unwrap();
        if let Object::Stream(stream) = obj {
            assert_eq!(
                stream.dict.get(b"Subtype").unwrap(),
                &Object::Name(b"Image".to_vec())
            );
            assert_eq!(
                stream.dict.get(b"Filter").unwrap(),
                &Object::Name(b"DCTDecode".to_vec())
            );
        } else {
            panic!("expected stream object");
        }
    }

    #[test]
    fn test_insert_png_with_alpha() {
        let mut doc = make_test_doc();
        let png = minimal_png();

        let result = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: png,
                format: ImageFormat::Png,
                x: 50.0,
                y: 50.0,
                width: 100.0,
                height: 100.0,
                page_index: 1,
                opacity: Some(0.5),
            },
        )
        .unwrap();

        assert_eq!(result.resource_name, "Im1");
        assert_eq!(result.pixel_width, 2);
        assert_eq!(result.pixel_height, 2);

        // Verify SMask was created (PNG with alpha).
        let obj = doc.objects.get(&result.image_object_id).unwrap();
        if let Object::Stream(stream) = obj {
            assert!(
                stream.dict.has(b"SMask"),
                "PNG with alpha should have SMask"
            );
        } else {
            panic!("expected stream object");
        }
    }

    #[test]
    fn test_create_png_xobject_rejects_oversized_header() {
        let mut doc = make_test_doc();
        let png = oversized_png_header(MAX_IMAGE_DIMENSION, MAX_IMAGE_DIMENSION, 6);

        let err = create_png_xobject(&mut doc, &png).unwrap_err();
        assert!(matches!(
            err,
            ManipError::ImageTooLarge {
                width: MAX_IMAGE_DIMENSION,
                height: MAX_IMAGE_DIMENSION,
                bytes_per_pixel: 4,
                ..
            }
        ));
    }

    #[test]
    fn test_create_png_xobject_accepts_small_valid_png() {
        let mut doc = make_test_doc();
        let result = create_png_xobject(&mut doc, &minimal_png());

        assert!(result.is_ok());
    }

    #[test]
    fn test_insert_with_opacity() {
        let mut doc = make_test_doc();
        let jpeg = minimal_jpeg();

        let result = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: jpeg,
                format: ImageFormat::Jpeg,
                x: 0.0,
                y: 0.0,
                width: 612.0,
                height: 792.0,
                page_index: 1,
                opacity: Some(0.5),
            },
        )
        .unwrap();

        // Verify ExtGState was created for opacity.
        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get(&page_id) {
            let res = page_dict.get(b"Resources").unwrap();
            if let Object::Dictionary(res_dict) = res {
                assert!(res_dict.has(b"ExtGState"));
                let gs = res_dict.get(b"ExtGState").unwrap();
                if let Object::Dictionary(gs_dict) = gs {
                    let gs_name = format!("GS_{}", result.resource_name);
                    assert!(gs_dict.has(gs_name.as_bytes()));
                }
            }
        }
    }

    #[test]
    fn test_page_out_of_range() {
        let mut doc = make_test_doc();
        let jpeg = minimal_jpeg();

        let err = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: jpeg,
                format: ImageFormat::Jpeg,
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                page_index: 5,
                opacity: None,
            },
        );

        assert!(err.is_err());
    }

    #[test]
    fn test_multiple_inserts_unique_names() {
        let mut doc = make_test_doc();

        let jpeg1 = minimal_jpeg();
        let r1 = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: jpeg1,
                format: ImageFormat::Jpeg,
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                page_index: 1,
                opacity: None,
            },
        )
        .unwrap();

        let jpeg2 = minimal_jpeg();
        let r2 = insert_image(
            &mut doc,
            &ImageInsert {
                image_data: jpeg2,
                format: ImageFormat::Jpeg,
                x: 200.0,
                y: 200.0,
                width: 100.0,
                height: 100.0,
                page_index: 1,
                opacity: None,
            },
        )
        .unwrap();

        assert_ne!(r1.resource_name, r2.resource_name);
    }
}
