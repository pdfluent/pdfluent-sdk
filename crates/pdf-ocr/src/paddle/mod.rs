//! PaddleOCR engine: text detection, recognition, and angle classification.

pub mod classifier;
pub mod detection;
pub mod dictionary;
pub mod models;
pub mod recognition;
pub mod utils;

pub use models::{DetectionModel, Language, PaddleOcrConfig};

use std::cell::UnsafeCell;

use crate::{OcrEngine, OcrPageResult, OcrWord};
use models::ModelSessions;

/// PaddleOCR engine implementing the `OcrEngine` trait.
///
/// Combines DBNet detection, optional angle classification,
/// and SVTR recognition into a complete OCR pipeline.
///
/// Uses interior mutability for ONNX sessions because `ort::Session::run`
/// requires `&mut self`, but the `OcrEngine` trait takes `&self`.
pub struct PaddleOcrEngine {
    sessions: UnsafeCell<ModelSessions>,
    config: PaddleOcrConfig,
}

// Safety: ort::Session is internally thread-safe. The UnsafeCell is used solely
// because Session::run() takes &mut self as a Rust API requirement, not because
// it actually mutates shared state unsafely. Concurrent calls should still be
// serialized externally by the caller (which the pipeline does).
unsafe impl Send for PaddleOcrEngine {}
unsafe impl Sync for PaddleOcrEngine {}

impl PaddleOcrEngine {
    /// Create a new PaddleOCR engine with default config.
    ///
    /// Downloads models if not cached.
    pub fn new() -> Result<Self, String> {
        Self::with_config(PaddleOcrConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: PaddleOcrConfig) -> Result<Self, String> {
        if !models::models_available(&config) {
            models::download_models(&config).map_err(|e| e.to_string())?;
        }
        let sessions = models::load_sessions(&config).map_err(|e| e.to_string())?;
        Ok(Self {
            sessions: UnsafeCell::new(sessions),
            config,
        })
    }
}

impl OcrEngine for PaddleOcrEngine {
    fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        _dpi: u32,
    ) -> std::result::Result<OcrPageResult, String> {
        // Safety: see Send/Sync impl above. We need &mut for ort::Session::run().
        let sessions = unsafe { &mut *self.sessions.get() };

        // 1. Run detection
        let (det_tensor, sx, sy) = detection::preprocess_for_detection(
            image_data,
            width,
            height,
            self.config.max_side_len,
        );
        let prob_map = detection::detect_inference(&mut sessions.detection, &det_tensor)
            .map_err(|e| e.to_string())?;
        let regions = detection::postprocess_detection(
            &prob_map,
            sx,
            sy,
            self.config.det_threshold,
            self.config.box_threshold,
            3.0,
            1.5,
        );

        if regions.is_empty() {
            return Ok(OcrPageResult {
                words: Vec::new(),
                confidence: 0.0,
                image_width: width,
                image_height: height,
            });
        }

        // 2. Crop detected regions from original image
        let mut crops = crop_regions(image_data, width, height, &regions);

        // 3. Optional: classify angle and rotate
        if self.config.use_angle_classifier {
            if let Some(ref mut cls_session) = sessions.classifier {
                classifier::classify_and_rotate_batch(cls_session, &mut crops)
                    .map_err(|e| e.to_string())?;
            }
        }

        // 4. Run recognition in batches
        let rec_results = recognition::recognize_batch(
            &mut sessions.recognition,
            &sessions.dictionary,
            &crops,
            self.config.rec_batch_size,
        )
        .map_err(|e| e.to_string())?;

        // 5. Map to OcrWord results
        let words: Vec<OcrWord> = regions
            .iter()
            .zip(rec_results.iter())
            .filter(|(_, rec)| !rec.text.is_empty())
            .map(|(region, rec)| OcrWord {
                text: rec.text.clone(),
                bbox_px: region.bbox,
                confidence: rec.confidence,
            })
            .collect();

        let avg_confidence = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| w.confidence).sum::<f32>() / words.len() as f32
        };

        Ok(OcrPageResult {
            words,
            confidence: avg_confidence,
            image_width: width,
            image_height: height,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        self.config
            .languages
            .iter()
            .map(|l| l.code().to_string())
            .collect()
    }
}

/// Crop text regions from the source image.
fn crop_regions(
    image_data: &[u8],
    width: u32,
    height: u32,
    regions: &[detection::TextRegion],
) -> Vec<(Vec<u8>, u32, u32)> {
    regions
        .iter()
        .map(|r| {
            utils::crop_rgb(
                image_data, width, height, r.bbox[0], r.bbox[1], r.bbox[2], r.bbox[3],
            )
        })
        .filter(|(_, w, h)| *w > 0 && *h > 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_regions_extracts_correct_pixels() {
        // 4x4 RGB image, all red
        let mut img: Vec<u8> = vec![255, 0, 0].repeat(16);
        // Set center 2x2 to green
        for y in 1u32..3 {
            for x in 1u32..3 {
                let idx = (y * 4 + x) as usize * 3;
                img[idx] = 0;
                img[idx + 1] = 255;
                img[idx + 2] = 0;
            }
        }
        let regions = vec![detection::TextRegion {
            bbox: [1, 1, 3, 3],
            polygon: [[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]],
            score: 0.9,
        }];
        let crops = crop_regions(&img, 4, 4, &regions);
        assert_eq!(crops.len(), 1);
        assert_eq!(crops[0].1, 2); // width
        assert_eq!(crops[0].2, 2); // height
        assert_eq!(&crops[0].0[0..3], &[0, 255, 0]);
    }

    #[test]
    fn crop_regions_empty_for_zero_size() {
        let img = vec![128u8; 4 * 4 * 3];
        let regions = vec![detection::TextRegion {
            bbox: [2, 2, 2, 2], // zero-size
            polygon: [[2.0, 2.0]; 4],
            score: 0.9,
        }];
        let crops = crop_regions(&img, 4, 4, &regions);
        assert!(crops.is_empty());
    }
}
