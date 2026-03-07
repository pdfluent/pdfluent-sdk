//! Tesseract OCR backend via leptess.
//!
//! Only available with the `tesseract` feature flag.
//! Requires Tesseract and Leptonica system libraries.

use crate::engine::{OcrEngine, OcrPageResult, OcrWord};

/// Tesseract-based OCR engine.
///
/// Wraps the Tesseract 4/5 engine via leptess FFI bindings.
/// Requires `tesseract-ocr` and `libleptonica` installed on the system.
#[derive(Debug)]
pub struct TesseractEngine {
    language: String,
    tessdata_path: Option<String>,
}

impl TesseractEngine {
    /// Create a new Tesseract engine.
    ///
    /// # Arguments
    /// * `language` - Tesseract language code (e.g. "eng", "nld", "deu")
    /// * `tessdata_path` - Optional path to tessdata directory. If `None`, uses system default.
    pub fn new(language: &str, tessdata_path: Option<&str>) -> Result<Self, String> {
        // Validate by attempting to create a LepTess instance.
        let tess = leptess::LepTess::new(tessdata_path, language)
            .map_err(|e| format!("Tesseract init failed: {e}"))?;
        drop(tess);

        Ok(Self {
            language: language.to_string(),
            tessdata_path: tessdata_path.map(String::from),
        })
    }
}

impl OcrEngine for TesseractEngine {
    fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        dpi: u32,
    ) -> Result<OcrPageResult, String> {
        let mut tess = leptess::LepTess::new(self.tessdata_path.as_deref(), &self.language)
            .map_err(|e| format!("Tesseract init: {e}"))?;

        // Set DPI before image so Tesseract uses it during recognition.
        tess.set_variable(leptess::Variable::UserDefinedDpi, &dpi.to_string())
            .map_err(|e| format!("set DPI: {e}"))?;

        // Determine pixel format from buffer length.
        let expected_rgb = (width as usize) * (height as usize) * 3;
        let expected_rgba = (width as usize) * (height as usize) * 4;

        let rgb_data = if image_data.len() == expected_rgba {
            // Convert RGBA to RGB by dropping alpha channel.
            image_data
                .chunks_exact(4)
                .flat_map(|px| &px[..3])
                .copied()
                .collect::<Vec<u8>>()
        } else if image_data.len() == expected_rgb {
            image_data.to_vec()
        } else {
            return Err(format!(
                "unexpected image size: {} bytes for {}x{} (expected {} RGB or {} RGBA)",
                image_data.len(),
                width,
                height,
                expected_rgb,
                expected_rgba,
            ));
        };

        // leptess set_image_from_mem expects an encoded image (PNG/TIFF/BMP), not
        // raw pixels. Encode the raw RGB data as a minimal BMP in memory.
        let bmp = encode_rgb_as_bmp(&rgb_data, width, height);
        tess.set_image_from_mem(&bmp)
            .map_err(|e| format!("set image: {e}"))?;

        tess.set_source_resolution(dpi as i32);

        // Run recognition and get HOCR output for word-level bounding boxes.
        let hocr = tess
            .get_hocr_text(0)
            .map_err(|e| format!("get hocr: {e}"))?;

        let words = parse_hocr_words(&hocr);

        let confidence = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| w.confidence).sum::<f32>() / words.len() as f32
        };

        Ok(OcrPageResult {
            words,
            confidence,
            image_width: width,
            image_height: height,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        self.language.split('+').map(|s| s.to_string()).collect()
    }
}

/// Encode raw RGB pixel data as a BMP file in memory.
///
/// BMP rows are stored bottom-up with each row padded to a 4-byte boundary.
fn encode_rgb_as_bmp(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row_stride = (width * 3 + 3) & !3; // Pad each row to 4-byte boundary.
    let pixel_data_size = row_stride * height;
    let file_size = 54 + pixel_data_size; // 14 (file header) + 40 (info header) + pixels.

    let mut buf = Vec::with_capacity(file_size as usize);

    // --- BMP file header (14 bytes) ---
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // Reserved.
    buf.extend_from_slice(&54u32.to_le_bytes()); // Pixel data offset.

    // --- DIB header (BITMAPINFOHEADER, 40 bytes) ---
    buf.extend_from_slice(&40u32.to_le_bytes()); // Header size.
    buf.extend_from_slice(&(width as i32).to_le_bytes());
    buf.extend_from_slice(&(height as i32).to_le_bytes()); // Positive = bottom-up.
    buf.extend_from_slice(&1u16.to_le_bytes()); // Color planes.
    buf.extend_from_slice(&24u16.to_le_bytes()); // Bits per pixel.
    buf.extend_from_slice(&0u32.to_le_bytes()); // Compression (none).
    buf.extend_from_slice(&pixel_data_size.to_le_bytes());
    buf.extend_from_slice(&2835u32.to_le_bytes()); // H resolution (pixels/meter, ~72 DPI).
    buf.extend_from_slice(&2835u32.to_le_bytes()); // V resolution.
    buf.extend_from_slice(&0u32.to_le_bytes()); // Colors in palette.
    buf.extend_from_slice(&0u32.to_le_bytes()); // Important colors.

    // --- Pixel data (bottom-up, BGR) ---
    let src_stride = width as usize * 3;
    let pad_bytes = (row_stride as usize) - src_stride;
    for y in (0..height as usize).rev() {
        let row_start = y * src_stride;
        let row = &rgb[row_start..row_start + src_stride];
        // BMP stores pixels as BGR, convert from RGB.
        for pixel in row.chunks_exact(3) {
            buf.push(pixel[2]); // B
            buf.push(pixel[1]); // G
            buf.push(pixel[0]); // R
        }
        // Pad row to 4-byte boundary.
        buf.extend(std::iter::repeat_n(0u8, pad_bytes));
    }

    buf
}

/// Parse HOCR XML output to extract word bounding boxes and confidence scores.
fn parse_hocr_words(hocr: &str) -> Vec<OcrWord> {
    let mut words = Vec::new();

    // HOCR format: <span class='ocrx_word' ... title='bbox x0 y0 x1 y1; x_wconf 95'>text</span>
    // Split on "ocrx_word" — the first segment is before any word span
    // and may contain page/line-level bboxes that we must skip.
    // All subsequent segments belong to actual word spans.
    for segment in hocr.split("ocrx_word").skip(1) {
        let bbox = extract_hocr_bbox(segment);
        let conf = extract_hocr_confidence(segment);
        let text = extract_hocr_text(segment);

        if let (Some(bbox), Some(text)) = (bbox, text) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                words.push(OcrWord {
                    text: trimmed,
                    bbox_px: bbox,
                    confidence: conf.unwrap_or(0.0) / 100.0,
                });
            }
        }
    }

    words
}

/// Extract bounding box coordinates from an HOCR title attribute fragment.
fn extract_hocr_bbox(s: &str) -> Option<[u32; 4]> {
    let bbox_start = s.find("bbox ")?;
    let after = &s[bbox_start + 5..];
    let end = after
        .find(';')
        .or_else(|| after.find('"'))
        .or_else(|| after.find('\''))
        .unwrap_or(after.len());
    let coords: Vec<u32> = after[..end]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    if coords.len() >= 4 {
        Some([coords[0], coords[1], coords[2], coords[3]])
    } else {
        None
    }
}

/// Extract word confidence from an HOCR title attribute fragment.
fn extract_hocr_confidence(s: &str) -> Option<f32> {
    let conf_start = s.find("x_wconf ")?;
    let after = &s[conf_start + 8..];
    let end = after
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

/// Extract the text content from an HOCR word span fragment.
fn extract_hocr_text(s: &str) -> Option<String> {
    // The text appears after the closing '>' of the opening tag.
    let tag_end = s.find('>')?;
    let after = &s[tag_end + 1..];
    let text_end = after.find("</").unwrap_or(after.len());
    // Strip common inline HTML tags and decode HTML entities.
    let text = after[..text_end]
        .replace("<em>", "")
        .replace("</em>", "")
        .replace("<strong>", "")
        .replace("</strong>", "");
    Some(decode_html_entities(&text))
}

/// Decode common HTML entities in HOCR text output.
fn decode_html_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hocr_parse_extracts_words() {
        let hocr = r#"<span class='ocrx_word' id='word_1' title='bbox 10 20 100 50; x_wconf 95'>Hello</span>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].bbox_px, [10, 20, 100, 50]);
        assert!((words[0].confidence - 0.95).abs() < 0.01);
    }

    #[test]
    fn hocr_parse_multiple_words() {
        let hocr = r#"
        <span class='ocrx_word' title='bbox 10 20 50 40; x_wconf 90'>Hello</span>
        <span class='ocrx_word' title='bbox 60 20 120 40; x_wconf 85'>World</span>
        "#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[1].text, "World");
    }

    #[test]
    fn hocr_parse_empty() {
        let words = parse_hocr_words("");
        assert!(words.is_empty());
    }

    #[test]
    fn hocr_parse_with_inline_html() {
        let hocr =
            r#"<span class='ocrx_word' title='bbox 5 10 80 30; x_wconf 88'><em>Bold</em></span>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Bold");
        assert_eq!(words[0].bbox_px, [5, 10, 80, 30]);
        assert!((words[0].confidence - 0.88).abs() < 0.01);
    }

    #[test]
    fn hocr_parse_skips_non_word_segments() {
        // A page-level bbox should NOT produce a word entry.
        let hocr = r#"<div class='ocr_page' title='bbox 0 0 600 800; ppageno 0'>
        <span class='ocrx_word' title='bbox 10 20 100 50; x_wconf 95'>Hello</span>
        </div>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hello");
    }

    #[test]
    fn hocr_parse_decodes_html_entities() {
        let hocr =
            r#"<span class='ocrx_word' title='bbox 10 20 100 50; x_wconf 90'>A &amp; B</span>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "A & B");
    }

    #[test]
    fn hocr_entity_decode_no_cascade() {
        // &amp;lt; should decode to "&lt;", not "<".
        let hocr = r#"<span class='ocrx_word' title='bbox 0 0 80 30; x_wconf 85'>&amp;lt;tag&amp;gt;</span>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "&lt;tag&gt;");
    }

    #[test]
    fn bmp_encoding_produces_valid_header() {
        let width = 4u32;
        let height = 2u32;
        let rgb = vec![0u8; (width * height * 3) as usize];
        let bmp = encode_rgb_as_bmp(&rgb, width, height);

        // Check BMP magic.
        assert_eq!(&bmp[0..2], b"BM");

        // Check pixel data offset = 54.
        let offset = u32::from_le_bytes([bmp[10], bmp[11], bmp[12], bmp[13]]);
        assert_eq!(offset, 54);

        // Check width and height in DIB header.
        let w = i32::from_le_bytes([bmp[18], bmp[19], bmp[20], bmp[21]]);
        let h = i32::from_le_bytes([bmp[22], bmp[23], bmp[24], bmp[25]]);
        assert_eq!(w, 4);
        assert_eq!(h, 2);

        // Check bits per pixel = 24.
        let bpp = u16::from_le_bytes([bmp[28], bmp[29]]);
        assert_eq!(bpp, 24);
    }

    #[test]
    fn bmp_encoding_row_padding() {
        // Width 5 pixels: 5*3=15 bytes, padded to 16.
        let width = 5u32;
        let height = 1u32;
        let rgb = vec![128u8; (width * height * 3) as usize];
        let bmp = encode_rgb_as_bmp(&rgb, width, height);

        let row_stride = ((width * 3 + 3) & !3) as usize;
        assert_eq!(row_stride, 16);
        // Total size = 54 header + row_stride * height.
        assert_eq!(bmp.len(), 54 + row_stride);
    }

    #[cfg(feature = "tesseract")]
    #[test]
    fn tesseract_engine_new_with_eng() {
        // This test requires tesseract-ocr installed with eng data.
        let result = TesseractEngine::new("eng", None);
        if result.is_err() {
            eprintln!("Tesseract not available: {}", result.unwrap_err());
            return;
        }
        let engine = result.unwrap();
        assert!(engine.supported_languages().contains(&"eng".to_string()));
    }

    #[cfg(feature = "tesseract")]
    #[test]
    fn tesseract_recognize_white_image() {
        use crate::engine::OcrEngine;

        let result = TesseractEngine::new("eng", None);
        if result.is_err() {
            eprintln!("Tesseract not available, skipping");
            return;
        }
        let engine = result.unwrap();
        // Create a 100x50 white RGB image (should produce no/empty text).
        let white = vec![255u8; 100 * 50 * 3];
        let ocr = engine.recognize(&white, 100, 50, 300).unwrap();
        // A blank white image should yield zero or very few words.
        assert!(ocr.words.len() < 5);
    }
}
