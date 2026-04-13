//! Image embedding for PDF XObjects.
//!
//! Converts XFA image data (JPEG/PNG) into PDF Image XObject dictionaries
//! and provides PDF content stream operators for rendering.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::GenericImageView;
use lopdf::{dictionary, Object, ObjectId, Stream};
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
}

#[derive(Debug, Clone)]
pub struct ImageXObjectResult {
    pub object_id: ObjectId,
    pub width: u32,
    pub height: u32,
}

pub fn detect_image_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        Some(ImageFormat::Jpeg)
    } else if data.len() >= 8 && data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        Some(ImageFormat::Png)
    } else {
        None
    }
}

pub fn embed_jpeg(
    doc: &mut lopdf::Document,
    jpeg_data: &[u8],
) -> Result<ImageXObjectResult, String> {
    let (width, height, components) = parse_jpeg_dimensions(jpeg_data)
        .map_err(|e| format!("failed to parse JPEG dimensions: {}", e))?;

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
    let object_id = doc.add_object(Object::Stream(stream));

    Ok(ImageXObjectResult {
        object_id,
        width,
        height,
    })
}

pub fn embed_png(doc: &mut lopdf::Document, png_data: &[u8]) -> Result<ImageXObjectResult, String> {
    let img = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
        .map_err(|e| format!("failed to decode PNG: {}", e))?;

    let (width, height) = img.dimensions();
    let has_alpha = img.color().has_alpha();

    let (raw_rgb, alpha_channel) = if has_alpha {
        let rgba = img.to_rgba8();
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        let mut alpha = Vec::with_capacity((width * height) as usize);
        for pixel in rgba.pixels() {
            rgb.extend_from_slice(&pixel.0[..3]);
            alpha.push(pixel.0[3]);
        }
        (rgb, Some(alpha))
    } else {
        (img.to_rgb8().into_raw(), None)
    };

    let compressed_rgb =
        flate_compress(&raw_rgb).map_err(|e| format!("compression failed: {}", e))?;

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

    if let Some(alpha) = alpha_channel {
        let compressed_alpha =
            flate_compress(&alpha).map_err(|e| format!("alpha compression failed: {}", e))?;
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
    let object_id = doc.add_object(Object::Stream(stream));

    Ok(ImageXObjectResult {
        object_id,
        width,
        height,
    })
}

pub fn embed_image(
    doc: &mut lopdf::Document,
    data: &[u8],
    mime_type: &str,
) -> Result<ImageXObjectResult, String> {
    let format = detect_image_format(data).or(match mime_type {
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/png" => Some(ImageFormat::Png),
        _ => None,
    });

    match format {
        Some(ImageFormat::Jpeg) => embed_jpeg(doc, data),
        Some(ImageFormat::Png) => embed_png(doc, data),
        // XFA 3.3 §20.2 allows JPEG, PNG, GIF, BMP, TIFF. For anything the
        // native embedders don't handle (GIF/BMP/TIFF/etc.), let the `image`
        // crate decode the bytes and re-encode as PNG before embedding —
        // this preserves the image at the cost of one decode/encode pass
        // instead of dropping it entirely (see 01de9ce4's Finance Corp logo
        // which ships as image/tif).
        None => embed_via_reencode(doc, data, mime_type),
    }
}

fn embed_via_reencode(
    doc: &mut lopdf::Document,
    data: &[u8],
    mime_type: &str,
) -> Result<ImageXObjectResult, String> {
    let img = image::load_from_memory(data).map_err(|e| {
        format!("unsupported image format (mime={mime_type}); decode failed: {e}")
    })?;
    let mut png_buf: Vec<u8> = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_buf),
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("re-encode to PNG failed: {e}"))?;
    embed_png(doc, &png_buf)
}

pub fn render_image_ops(name: &str, x: f64, y: f64, w: f64, h: f64) -> Vec<u8> {
    let mut ops = Vec::new();
    ops.extend_from_slice(b"q\n");
    ops.extend(format!("{:.2} 0 0 {:.2} {:.2} {:.2} cm\n", w, h, x, y).bytes());
    ops.extend(format!("/{name} Do\n",).bytes());
    ops.extend_from_slice(b"Q\n");
    ops
}

fn parse_jpeg_dimensions(data: &[u8]) -> Result<(u32, u32, u8), String> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err("not a valid JPEG".into());
    }

    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            return Err("invalid JPEG marker".into());
        }

        let marker = data[i + 1];

        if marker == 0xFF {
            i += 1;
            continue;
        }

        let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);

        if is_sof {
            if i + 9 >= data.len() {
                return Err("truncated JPEG SOF".into());
            }
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            let components = data[i + 9];
            return Ok((width, height, components));
        }

        if i + 3 >= data.len() {
            break;
        }
        let segment_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        i += 2 + segment_len;
    }

    Err("no SOF marker found in JPEG".into())
}

fn flate_compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| format!("compression failed: {}", e))?;
    encoder
        .finish()
        .map_err(|e| format!("compression finalize failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_jpeg() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&[0xFF, 0xD8]);
        data.extend_from_slice(&[0xFF, 0xC0]);
        data.extend_from_slice(&[0x00, 0x11]);
        data.push(0x08);
        data.extend_from_slice(&[0x00, 0x02]);
        data.extend_from_slice(&[0x00, 0x02]);
        data.push(0x03);
        for id in 1..=3u8 {
            data.push(id);
            data.push(0x11);
            data.push(0x00);
        }
        data.extend_from_slice(&[0xFF, 0xD9]);
        data
    }

    fn minimal_png() -> Vec<u8> {
        use std::io::Cursor;
        let mut buf = Cursor::new(Vec::new());
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128]));
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn test_detect_format_jpeg() {
        let jpeg = minimal_jpeg();
        assert_eq!(detect_image_format(&jpeg), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn test_detect_format_png() {
        let png = minimal_png();
        assert_eq!(detect_image_format(&png), Some(ImageFormat::Png));
    }

    #[test]
    fn test_detect_format_unknown() {
        assert_eq!(detect_image_format(&[0x00, 0x01, 0x02]), None);
    }

    #[test]
    fn test_embed_jpeg() {
        let mut doc = lopdf::Document::with_version("1.7");
        let jpeg = minimal_jpeg();
        let result = embed_jpeg(&mut doc, &jpeg).unwrap();
        assert_eq!(result.width, 2);
        assert_eq!(result.height, 2);
    }

    #[test]
    fn test_embed_png() {
        let mut doc = lopdf::Document::with_version("1.7");
        let png = minimal_png();
        let result = embed_png(&mut doc, &png).unwrap();
        assert_eq!(result.width, 2);
        assert_eq!(result.height, 2);
    }

    #[test]
    fn test_embed_image_tiff_via_reencode() {
        // XFA templates sometimes ship image/tif data (e.g. 01de9ce4's
        // Finance Corp logo). Before the re-encode fallback these would be
        // dropped with "unsupported image format". Now they should be
        // decoded by the `image` crate and re-embedded as PNG.
        use std::io::Cursor;
        let img = image::RgbaImage::from_pixel(3, 4, image::Rgba([32, 64, 96, 255]));
        let mut tiff_buf = Cursor::new(Vec::new());
        img.write_to(&mut tiff_buf, image::ImageFormat::Tiff).unwrap();
        let tiff_data = tiff_buf.into_inner();
        assert_eq!(detect_image_format(&tiff_data), None);

        let mut doc = lopdf::Document::with_version("1.7");
        let result = embed_image(&mut doc, &tiff_data, "image/tif")
            .expect("TIFF should be accepted via re-encode fallback");
        assert_eq!(result.width, 3);
        assert_eq!(result.height, 4);
    }

    #[test]
    fn test_embed_image_gif_via_reencode() {
        // GIF is also allowed by XFA 3.3 §20.2 but not natively supported
        // by embed_jpeg/embed_png. Verify it goes through the re-encode path.
        use std::io::Cursor;
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut gif_buf = Cursor::new(Vec::new());
        img.write_to(&mut gif_buf, image::ImageFormat::Gif).unwrap();
        let gif_data = gif_buf.into_inner();

        let mut doc = lopdf::Document::with_version("1.7");
        let result = embed_image(&mut doc, &gif_data, "image/gif")
            .expect("GIF should be accepted via re-encode fallback");
        assert_eq!(result.width, 2);
        assert_eq!(result.height, 2);
    }

    #[test]
    fn test_render_image_ops() {
        let ops = render_image_ops("Im1", 100.0, 200.0, 50.0, 75.0);
        let content = String::from_utf8_lossy(&ops);
        assert!(content.contains("q\n"));
        assert!(content.contains("50.00 0 0 75.00 100.00 200.00 cm\n"));
        assert!(content.contains("/Im1 Do\n"));
        assert!(content.contains("Q\n"));
    }
}
