//! Image downsampling for PDF optimization.
//!
//! Scans embedded images and downsamples those above a target DPI
//! to reduce file size. Re-encodes as JPEG via DCTDecode.

use crate::error::{ManipError, Result};
use flate2::read::ZlibDecoder;
use image::imageops::FilterType;
use image::{DynamicImage, RgbImage};
use lopdf::content::Content;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashMap;
use std::io::Read;

/// Configuration for image downsampling.
#[derive(Debug, Clone)]
pub struct DownsampleConfig {
    /// Target DPI threshold — images above this will be downsampled.
    pub target_dpi: u32,
    /// JPEG output quality (1–100).
    pub jpeg_quality: u8,
    /// Skip images smaller than this width in pixels.
    pub min_width: u32,
}

impl Default for DownsampleConfig {
    fn default() -> Self {
        Self {
            target_dpi: 150,
            jpeg_quality: 80,
            min_width: 64,
        }
    }
}

/// Report from a downsampling pass.
#[derive(Debug, Clone)]
pub struct DownsampleReport {
    /// Number of images inspected.
    pub images_inspected: usize,
    /// Number of images downsampled.
    pub images_downsampled: usize,
    /// Total bytes saved (approximate).
    pub bytes_saved: i64,
}

/// Downsample all embedded images above `target_dpi` in the document.
pub fn downsample_images(
    doc: &mut Document,
    config: &DownsampleConfig,
) -> Result<DownsampleReport> {
    let mut report = DownsampleReport {
        images_inspected: 0,
        images_downsampled: 0,
        bytes_saved: 0,
    };

    let display_sizes = collect_image_display_sizes(doc);

    let image_ids: Vec<ObjectId> = doc
        .objects
        .keys()
        .copied()
        .filter(|id| is_image_xobject(doc, *id))
        .collect();

    for id in image_ids {
        report.images_inspected += 1;

        let (pixel_w, pixel_h, components, filter, content_len) = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            let w = get_int(&stream.dict, b"Width").unwrap_or(0) as u32;
            let h = get_int(&stream.dict, b"Height").unwrap_or(0) as u32;
            let bpc = get_int(&stream.dict, b"BitsPerComponent").unwrap_or(8);
            if bpc != 8 || w == 0 || h == 0 {
                continue;
            }
            let cs = get_name(&stream.dict, b"ColorSpace");
            let comp = match cs.as_deref() {
                Some("DeviceGray") => 1u32,
                Some("DeviceCMYK") => 4,
                _ => 3,
            };
            let f = get_name(&stream.dict, b"Filter");
            (w, h, comp, f, stream.content.len())
        };

        if pixel_w < config.min_width {
            continue;
        }

        let current_dpi = if let Some((dw, dh)) = display_sizes.get(&id).copied() {
            if dw > 0.0 && dh > 0.0 {
                let dpi_x = (pixel_w as f64) / (dw / 72.0);
                let dpi_y = (pixel_h as f64) / (dh / 72.0);
                dpi_x.max(dpi_y)
            } else {
                continue;
            }
        } else {
            300.0
        };

        if current_dpi <= config.target_dpi as f64 {
            continue;
        }

        let scale = config.target_dpi as f64 / current_dpi;
        let new_w = ((pixel_w as f64 * scale).round() as u32).max(1);
        let new_h = ((pixel_h as f64 * scale).round() as u32).max(1);

        let raw_pixels = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            match decode_image_data(
                &stream.content,
                filter.as_deref(),
                pixel_w,
                pixel_h,
                components,
            ) {
                Ok(data) => data,
                Err(_) => continue,
            }
        };

        let img = match components {
            1 => image::GrayImage::from_raw(pixel_w, pixel_h, raw_pixels)
                .map(DynamicImage::ImageLuma8),
            3 => RgbImage::from_raw(pixel_w, pixel_h, raw_pixels).map(DynamicImage::ImageRgb8),
            4 => {
                let mut rgb_data = Vec::with_capacity((pixel_w * pixel_h * 3) as usize);
                for chunk in raw_pixels.chunks(4) {
                    let c = chunk[0] as f32 / 255.0;
                    let m = chunk[1] as f32 / 255.0;
                    let y = chunk[2] as f32 / 255.0;
                    let k = chunk[3] as f32 / 255.0;
                    rgb_data.push(((1.0 - c) * (1.0 - k) * 255.0) as u8);
                    rgb_data.push(((1.0 - m) * (1.0 - k) * 255.0) as u8);
                    rgb_data.push(((1.0 - y) * (1.0 - k) * 255.0) as u8);
                }
                RgbImage::from_raw(pixel_w, pixel_h, rgb_data).map(DynamicImage::ImageRgb8)
            }
            _ => None,
        };

        let Some(img) = img else { continue };

        let resized = img.resize_exact(new_w, new_h, FilterType::Lanczos3);
        let rgb = resized.to_rgb8();

        let jpeg_data = match encode_jpeg(&rgb) {
            Ok(data) => data,
            Err(_) => continue,
        };

        let bytes_before = content_len as i64;
        let bytes_after = jpeg_data.len() as i64;

        if bytes_after >= bytes_before {
            continue;
        }

        let new_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => Object::Integer(new_w as i64),
            "Height" => Object::Integer(new_h as i64),
            "BitsPerComponent" => Object::Integer(8),
            "ColorSpace" => "DeviceRGB",
            "Filter" => "DCTDecode",
            "Length" => Object::Integer(jpeg_data.len() as i64),
        };
        doc.objects
            .insert(id, Object::Stream(Stream::new(new_dict, jpeg_data)));

        report.images_downsampled += 1;
        report.bytes_saved += bytes_before - bytes_after;
    }

    Ok(report)
}

fn is_image_xobject(doc: &Document, id: ObjectId) -> bool {
    if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
        get_name(&stream.dict, b"Subtype").as_deref() == Some("Image")
    } else {
        false
    }
}

fn get_int(dict: &lopdf::Dictionary, key: &[u8]) -> Option<i64> {
    match dict.get(key).ok()? {
        Object::Integer(n) => Some(*n),
        _ => None,
    }
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

/// Collect display sizes of images from page content streams (CTM analysis).
fn collect_image_display_sizes(doc: &Document) -> HashMap<ObjectId, (f64, f64)> {
    let mut sizes = HashMap::new();

    for (_page_num, page_id) in doc.get_pages() {
        let Some(Object::Dictionary(page_dict)) = doc.objects.get(&page_id) else {
            continue;
        };

        let xobject_map = get_page_xobjects(doc, page_dict);
        let Some(content_data) = get_page_content_data(doc, page_dict) else {
            continue;
        };
        let Ok(content) = Content::decode(&content_data) else {
            continue;
        };

        let mut ctm_stack: Vec<[f64; 6]> = vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]];

        for op in &content.operations {
            match op.operator.as_str() {
                "q" => {
                    let current = *ctm_stack.last().unwrap_or(&[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                    ctm_stack.push(current);
                }
                "Q" => {
                    if ctm_stack.len() > 1 {
                        ctm_stack.pop();
                    }
                }
                "cm" if op.operands.len() >= 6 => {
                    let vals: Vec<f64> = op.operands.iter().filter_map(obj_to_f64).collect();
                    if vals.len() == 6 {
                        if let Some(ctm) = ctm_stack.last_mut() {
                            *ctm = multiply_ctm(ctm, &vals);
                        }
                    }
                }
                "Do" if op.operands.len() == 1 => {
                    if let Object::Name(ref name) = op.operands[0] {
                        let name_str = String::from_utf8_lossy(name);
                        if let Some(&obj_id) = xobject_map.get(name_str.as_ref()) {
                            let ctm = ctm_stack
                                .last()
                                .copied()
                                .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                            let dw = (ctm[0] * ctm[0] + ctm[2] * ctm[2]).sqrt();
                            let dh = (ctm[1] * ctm[1] + ctm[3] * ctm[3]).sqrt();
                            if dw > 0.0 && dh > 0.0 {
                                sizes.insert(obj_id, (dw, dh));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    sizes
}

fn multiply_ctm(current: &[f64; 6], new: &[f64]) -> [f64; 6] {
    [
        new[0] * current[0] + new[1] * current[2],
        new[0] * current[1] + new[1] * current[3],
        new[2] * current[0] + new[3] * current[2],
        new[2] * current[1] + new[3] * current[3],
        new[4] * current[0] + new[5] * current[2] + current[4],
        new[4] * current[1] + new[5] * current[3] + current[5],
    ]
}

fn obj_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(n) => Some(*n as f64),
        Object::Real(n) => Some(*n as f64),
        _ => None,
    }
}

fn get_page_xobjects(doc: &Document, page_dict: &lopdf::Dictionary) -> HashMap<String, ObjectId> {
    let mut map = HashMap::new();
    let res = match page_dict.get(b"Resources").ok() {
        Some(Object::Dictionary(d)) => d,
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => d,
            _ => return map,
        },
        _ => return map,
    };
    let xobjects = match res.get(b"XObject").ok() {
        Some(Object::Dictionary(d)) => d,
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => d,
            _ => return map,
        },
        _ => return map,
    };
    for (key, val) in xobjects.iter() {
        if let Object::Reference(id) = val {
            map.insert(String::from_utf8_lossy(key).to_string(), *id);
        }
    }
    map
}

fn get_page_content_data(doc: &Document, page_dict: &lopdf::Dictionary) -> Option<Vec<u8>> {
    match page_dict.get(b"Contents").ok()? {
        Object::Reference(id) => {
            if let Some(Object::Stream(stream)) = doc.objects.get(id) {
                Some(stream.content.clone())
            } else {
                None
            }
        }
        Object::Array(arr) => {
            let mut data = Vec::new();
            for item in arr {
                if let Object::Reference(id) = item {
                    if let Some(Object::Stream(stream)) = doc.objects.get(id) {
                        data.extend_from_slice(&stream.content);
                        data.push(b'\n');
                    }
                }
            }
            Some(data)
        }
        _ => None,
    }
}

fn decode_image_data(
    data: &[u8],
    filter: Option<&str>,
    _width: u32,
    _height: u32,
    _components: u32,
) -> Result<Vec<u8>> {
    match filter {
        Some("FlateDecode") => {
            let mut decoder = ZlibDecoder::new(data);
            let mut buf = Vec::new();
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| ManipError::Image(format!("FlateDecode failed: {e}")))?;
            Ok(buf)
        }
        Some("DCTDecode") => {
            let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                .map_err(|e| ManipError::Image(format!("JPEG decode failed: {e}")))?;
            Ok(img.to_rgb8().into_raw())
        }
        None => Ok(data.to_vec()),
        Some(other) => Err(ManipError::Image(format!(
            "unsupported image filter for downsampling: {other}"
        ))),
    }
}

fn encode_jpeg(img: &RgbImage) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| ManipError::Image(format!("JPEG encoding failed: {e}")))?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn make_doc_with_image() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let raw_pixels: Vec<u8> = (0..4 * 4 * 3).map(|i| (i % 256) as u8).collect();
        let compressed = {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(&raw_pixels).unwrap();
            enc.finish().unwrap()
        };

        let img_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => Object::Integer(4),
            "Height" => Object::Integer(4),
            "BitsPerComponent" => Object::Integer(8),
            "ColorSpace" => "DeviceRGB",
            "Filter" => "FlateDecode",
            "Length" => Object::Integer(compressed.len() as i64),
        };
        let img_id = doc.add_object(Object::Stream(Stream::new(img_dict, compressed)));

        let content_ops = vec![
            lopdf::content::Operation::new("q", vec![]),
            lopdf::content::Operation::new(
                "cm",
                vec![
                    Object::Real(72.0),
                    Object::Real(0.0),
                    Object::Real(0.0),
                    Object::Real(72.0),
                    Object::Real(0.0),
                    Object::Real(0.0),
                ],
            ),
            lopdf::content::Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
            lopdf::content::Operation::new("Q", vec![]),
        ];
        let content_data = lopdf::content::Content {
            operations: content_ops,
        }
        .encode()
        .unwrap();
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content_data)));

        let mut xobject_dict = lopdf::Dictionary::new();
        xobject_dict.set("Im1", Object::Reference(img_id));
        let mut res_dict = lopdf::Dictionary::new();
        res_dict.set("XObject", Object::Dictionary(xobject_dict));

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
    fn test_downsample_default_config() {
        let mut doc = make_doc_with_image();
        let config = DownsampleConfig {
            target_dpi: 2,
            jpeg_quality: 80,
            min_width: 1,
        };
        let report = downsample_images(&mut doc, &config).unwrap();
        assert_eq!(report.images_inspected, 1);
    }

    #[test]
    fn test_no_downsample_below_target() {
        let mut doc = make_doc_with_image();
        let config = DownsampleConfig {
            target_dpi: 300,
            jpeg_quality: 80,
            min_width: 1,
        };
        let report = downsample_images(&mut doc, &config).unwrap();
        assert_eq!(report.images_inspected, 1);
        assert_eq!(report.images_downsampled, 0);
    }

    #[test]
    fn test_skip_small_images() {
        let mut doc = make_doc_with_image();
        let config = DownsampleConfig {
            target_dpi: 2,
            jpeg_quality: 80,
            min_width: 100,
        };
        let report = downsample_images(&mut doc, &config).unwrap();
        assert_eq!(report.images_downsampled, 0);
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

        let report = downsample_images(&mut doc, &DownsampleConfig::default()).unwrap();
        assert_eq!(report.images_inspected, 0);
    }
}
