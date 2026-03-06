//! Pluggable OCR engine trait and built-in implementations.
//!
//! Provides the `OcrEngine` trait for integrating different OCR backends,
//! and a `NoOpEngine` for testing.

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
}
