//! Pluggable OCR engine trait and built-in implementations.
//!
//! Provides the `OcrEngine` trait for integrating different OCR backends,
//! and a `NoOpEngine` for testing.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// A single word recognized by OCR.
#[derive(Debug, Clone)]
pub struct OcrWord {
    /// The recognized text.
    pub text: String,
    /// Bounding box in pixel coordinates [x0, y0, x1, y1].
    pub bbox_px: [u32; 4],
    /// Recognition confidence (0.0 to 1.0).
    pub confidence: f32,
}

/// Result of OCR processing on a single page.
#[derive(Debug, Clone)]
pub struct OcrPageResult {
    /// Recognized words.
    pub words: Vec<OcrWord>,
    /// Overall confidence for the page.
    pub confidence: f32,
    /// Width of the source image in pixels.
    pub image_width: u32,
    /// Height of the source image in pixels.
    pub image_height: u32,
}

impl OcrPageResult {
    /// Get the full text of the page by joining all words with spaces.
    pub fn full_text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Trait for pluggable OCR engines.
///
/// Implementors provide character recognition on rasterized page images.
pub trait OcrEngine: Send + Sync {
    /// Recognize text in an image.
    ///
    /// # Arguments
    /// * `image_data` - Raw pixel data (RGB, row-major).
    /// * `width` - Image width in pixels.
    /// * `height` - Image height in pixels.
    /// * `dpi` - Resolution in dots per inch.
    fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        dpi: u32,
    ) -> std::result::Result<OcrPageResult, String>;

    /// Return the list of supported languages.
    fn supported_languages(&self) -> Vec<String>;
}

/// RGB row-major pixels from an RGB or RGBA buffer.
///
/// Backends accept both shapes and drop an alpha channel; anything else is a
/// caller error naming the expected sizes.
///
/// Hidden: an internal helper the Python binding also uses, not API.
#[doc(hidden)]
pub fn rgb_pixels(
    image_data: &[u8],
    width: u32,
    height: u32,
) -> std::result::Result<Vec<u8>, String> {
    let pixels = width as usize * height as usize;
    if image_data.len() == pixels * 3 {
        Ok(image_data.to_vec())
    } else if image_data.len() == pixels * 4 {
        Ok(image_data
            .chunks_exact(4)
            .flat_map(|px| px[..3].to_vec())
            .collect())
    } else {
        Err(format!(
            "unexpected image size: {} bytes for {}x{} (expected {} RGB or {} RGBA)",
            image_data.len(),
            width,
            height,
            pixels * 3,
            pixels * 4,
        ))
    }
}

/// A no-op OCR engine that always returns empty results.
///
/// Useful for testing the pipeline without a real OCR backend.
#[derive(Debug, Default)]
pub struct NoOpEngine;

impl OcrEngine for NoOpEngine {
    fn recognize(
        &self,
        _image_data: &[u8],
        width: u32,
        height: u32,
        _dpi: u32,
    ) -> std::result::Result<OcrPageResult, String> {
        Ok(OcrPageResult {
            words: Vec::new(),
            confidence: 0.0,
            image_width: width,
            image_height: height,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_engine_returns_empty() {
        let engine = NoOpEngine;
        let result = engine.recognize(&[], 100, 100, 300).unwrap();
        assert!(result.words.is_empty());
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.image_width, 100);
        assert_eq!(result.image_height, 100);
        assert!(engine.supported_languages().is_empty());
    }

    #[test]
    fn ocr_page_result_full_text() {
        let result = OcrPageResult {
            words: vec![
                OcrWord {
                    text: "Hello".to_string(),
                    bbox_px: [0, 0, 50, 20],
                    confidence: 0.95,
                },
                OcrWord {
                    text: "World".to_string(),
                    bbox_px: [60, 0, 110, 20],
                    confidence: 0.90,
                },
            ],
            confidence: 0.92,
            image_width: 200,
            image_height: 100,
        };
        assert_eq!(result.full_text(), "Hello World");
    }

    #[test]
    fn rgb_pixels_keeps_rgb_drops_alpha_and_rejects_the_rest() {
        assert_eq!(
            rgb_pixels(&[1, 2, 3, 4, 5, 6], 2, 1).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert_eq!(
            rgb_pixels(&[1, 2, 3, 255, 4, 5, 6, 0], 2, 1).unwrap(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(rgb_pixels(&[1, 2, 3, 4, 5], 2, 1).is_err());
    }
}
