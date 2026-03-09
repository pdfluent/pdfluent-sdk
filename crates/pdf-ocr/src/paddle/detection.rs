//! DBNet text detection pipeline for PaddleOCR.
//!
//! Preprocesses images, runs inference via ONNX Runtime, and extracts
//! text region bounding boxes from the probability map.

use ndarray::Array4;
use ort::session::Session;

use super::utils;

/// ImageNet normalization constants used by DBNet.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Error type for detection operations.
#[derive(Debug)]
pub enum DetectionError {
    Inference(String),
    InvalidShape(String),
}

impl std::fmt::Display for DetectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference(msg) => write!(f, "detection inference error: {msg}"),
            Self::InvalidShape(msg) => write!(f, "invalid tensor shape: {msg}"),
        }
    }
}

impl std::error::Error for DetectionError {}

/// A detected text region with its bounding polygon.
#[derive(Debug, Clone)]
pub struct TextRegion {
    /// Bounding box in original image coordinates [x0, y0, x1, y1].
    pub bbox: [u32; 4],
    /// Rotated bounding box points (4 corners) for oriented text.
    pub polygon: [[f32; 2]; 4],
    /// Detection confidence score.
    pub score: f32,
}

/// Preprocess an image for DBNet inference.
///
/// Resizes to fit within `max_side_len`, pads to multiples of 32,
/// normalizes with ImageNet stats, and transposes HWC → NCHW.
///
/// Returns (tensor, scale_x, scale_y) where scales map back to original coords.
pub fn preprocess_for_detection(
    rgb_data: &[u8],
    width: u32,
    height: u32,
    max_side_len: u32,
) -> (Array4<f32>, f32, f32) {
    // 1. Resize with aspect ratio
    let (resized, new_w, new_h, scale_x, scale_y) =
        utils::resize_with_aspect_ratio(rgb_data, width, height, max_side_len);

    // 2. Pad to multiples of 32
    let pad_w = new_w.div_ceil(32) * 32;
    let pad_h = new_h.div_ceil(32) * 32;

    // 3. Create NCHW tensor with normalization
    let mut tensor = Array4::<f32>::zeros((1, 3, pad_h as usize, pad_w as usize));

    for y in 0..new_h as usize {
        for x in 0..new_w as usize {
            let idx = (y * new_w as usize + x) * 3;
            for c in 0..3 {
                let pixel = resized[idx + c] as f32 / 255.0;
                tensor[[0, c, y, x]] = (pixel - MEAN[c]) / STD[c];
            }
        }
    }
    // Padded area stays at 0.0 (which is approximately the mean-normalized zero)

    (tensor, scale_x, scale_y)
}

/// Run DBNet inference on preprocessed tensor.
///
/// Returns the probability map with shape [1, 1, H, W].
pub fn detect_inference(
    session: &mut Session,
    input: &Array4<f32>,
) -> Result<Array4<f32>, DetectionError> {
    let input_tensor = ort::value::Tensor::from_array(input.view().into_dyn().to_owned())
        .map_err(|e| DetectionError::Inference(e.to_string()))?;

    let outputs = session
        .run(ort::inputs!["x" => input_tensor])
        .map_err(|e| DetectionError::Inference(e.to_string()))?;

    let output = &outputs[0];

    let (shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| DetectionError::InvalidShape(e.to_string()))?;

    if shape.len() != 4 || shape[0] != 1 || shape[1] != 1 {
        return Err(DetectionError::InvalidShape(format!(
            "expected [1,1,H,W], got {shape:?}"
        )));
    }

    let h = shape[2] as usize;
    let w = shape[3] as usize;
    Array4::from_shape_vec((1, 1, h, w), data.to_vec())
        .map_err(|e| DetectionError::InvalidShape(e.to_string()))
}

/// Post-process DBNet probability map into text regions.
///
/// Thresholds the probability map, finds connected components, computes
/// bounding boxes, and scales back to original image coordinates.
pub fn postprocess_detection(
    prob_map: &Array4<f32>,
    scale_x: f32,
    scale_y: f32,
    threshold: f32,
    box_threshold: f32,
    min_area: f32,
    unclip_ratio: f32,
) -> Vec<TextRegion> {
    let h = prob_map.shape()[2];
    let w = prob_map.shape()[3];

    // 1. Threshold → binary mask
    let mut mask = vec![false; h * w];
    for y in 0..h {
        for x in 0..w {
            mask[y * w + x] = prob_map[[0, 0, y, x]] > threshold;
        }
    }

    // 2. Connected component labeling
    let labels = connected_components(&mask, w, h);
    let max_label = labels.iter().copied().max().unwrap_or(0);

    let mut regions = Vec::new();

    for label in 1..=max_label {
        // Collect pixels for this component
        let mut pixels: Vec<(usize, usize)> = Vec::new();
        let mut score_sum = 0.0f32;

        for y in 0..h {
            for x in 0..w {
                if labels[y * w + x] == label {
                    pixels.push((x, y));
                    score_sum += prob_map[[0, 0, y, x]];
                }
            }
        }

        if pixels.is_empty() {
            continue;
        }

        let area = pixels.len() as f32;
        if area < min_area {
            continue;
        }

        // Mean score for this component
        let mean_score = score_sum / area;
        if mean_score < box_threshold {
            continue;
        }

        // Compute bounding box in map coordinates
        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        for &(x, y) in &pixels {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        // Unclip: expand bounding box
        let box_w = (max_x - min_x + 1) as f32;
        let box_h = (max_y - min_y + 1) as f32;
        let perimeter = 2.0 * (box_w + box_h);
        let box_area = box_w * box_h;
        let offset = box_area * unclip_ratio / perimeter;

        let exp_min_x = (min_x as f32 - offset).max(0.0);
        let exp_min_y = (min_y as f32 - offset).max(0.0);
        let exp_max_x = (max_x as f32 + offset).min((w - 1) as f32);
        let exp_max_y = (max_y as f32 + offset).min((h - 1) as f32);

        // Scale back to original image coordinates
        let orig_x0 = (exp_min_x / scale_x).round().max(0.0) as u32;
        let orig_y0 = (exp_min_y / scale_y).round().max(0.0) as u32;
        let orig_x1 = (exp_max_x / scale_x).round() as u32;
        let orig_y1 = (exp_max_y / scale_y).round() as u32;

        let polygon = [
            [orig_x0 as f32, orig_y0 as f32],
            [orig_x1 as f32, orig_y0 as f32],
            [orig_x1 as f32, orig_y1 as f32],
            [orig_x0 as f32, orig_y1 as f32],
        ];

        regions.push(TextRegion {
            bbox: [orig_x0, orig_y0, orig_x1, orig_y1],
            polygon,
            score: mean_score,
        });
    }

    // Sort: top→bottom, then left→right
    regions.sort_by(|a, b| {
        let y_cmp = a.bbox[1].cmp(&b.bbox[1]);
        if y_cmp == std::cmp::Ordering::Equal {
            a.bbox[0].cmp(&b.bbox[0])
        } else {
            y_cmp
        }
    });

    regions
}

/// Simple connected component labeling using union-find (two-pass).
fn connected_components(mask: &[bool], w: usize, h: usize) -> Vec<u32> {
    let mut labels = vec![0u32; w * h];
    let mut parent: Vec<u32> = Vec::new();
    let mut next_label = 1u32;

    // Helper: find root with path compression
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }

    fn union(parent: &mut [u32], a: u32, b: u32) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra as usize] = rb;
        }
    }

    // Dummy entry for label 0
    parent.push(0);

    // First pass
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !mask[idx] {
                continue;
            }

            let above = if y > 0 { labels[(y - 1) * w + x] } else { 0 };
            let left = if x > 0 { labels[y * w + x - 1] } else { 0 };

            match (above > 0, left > 0) {
                (false, false) => {
                    labels[idx] = next_label;
                    parent.push(next_label);
                    next_label += 1;
                }
                (true, false) => {
                    labels[idx] = above;
                }
                (false, true) => {
                    labels[idx] = left;
                }
                (true, true) => {
                    labels[idx] = above;
                    if above != left {
                        union(&mut parent, above, left);
                    }
                }
            }
        }
    }

    // Second pass: resolve labels
    for label in labels.iter_mut() {
        if *label > 0 {
            *label = find(&mut parent, *label);
        }
    }

    // Renumber labels to be contiguous
    let mut remap = std::collections::HashMap::new();
    let mut new_label = 0u32;
    for label in labels.iter_mut() {
        if *label > 0 {
            let entry = remap.entry(*label).or_insert_with(|| {
                new_label += 1;
                new_label
            });
            *label = *entry;
        }
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_dimensions_correct() {
        let rgb = vec![128u8; 640 * 480 * 3];
        let (tensor, _sx, _sy) = preprocess_for_detection(&rgb, 640, 480, 960);
        assert_eq!(tensor.shape()[0], 1); // batch
        assert_eq!(tensor.shape()[1], 3); // channels
        assert!(tensor.shape()[2] % 32 == 0); // padded height
        assert!(tensor.shape()[3] % 32 == 0); // padded width
    }

    #[test]
    fn preprocess_large_image_resizes() {
        let rgb = vec![128u8; 1920 * 1080 * 3];
        let (tensor, sx, sy) = preprocess_for_detection(&rgb, 1920, 1080, 960);
        assert_eq!(tensor.shape()[0], 1);
        assert_eq!(tensor.shape()[1], 3);
        // Should be resized to ~960x540, padded to 960x544
        assert!(tensor.shape()[3] <= 960);
        assert!(tensor.shape()[2] <= 960);
        assert!(sx < 1.0);
        assert!(sy < 1.0);
    }

    #[test]
    fn preprocess_small_image_no_upscale() {
        let rgb = vec![128u8; 100 * 100 * 3];
        let (tensor, sx, sy) = preprocess_for_detection(&rgb, 100, 100, 960);
        assert_eq!(tensor.shape()[0], 1);
        assert_eq!(tensor.shape()[1], 3);
        // 100 padded to 128 (next multiple of 32)
        assert_eq!(tensor.shape()[2], 128);
        assert_eq!(tensor.shape()[3], 128);
        assert!((sx - 1.0).abs() < 1e-5);
        assert!((sy - 1.0).abs() < 1e-5);
    }

    #[test]
    fn preprocess_normalization_range() {
        let rgb = vec![255u8; 100 * 100 * 3];
        let (tensor, _, _) = preprocess_for_detection(&rgb, 100, 100, 960);
        let max_val = tensor.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(max_val < 5.0);
        assert!(max_val > 0.0);
    }

    #[test]
    fn postprocess_empty_map_returns_empty() {
        let prob_map = Array4::<f32>::zeros((1, 1, 32, 32));
        let regions = postprocess_detection(&prob_map, 1.0, 1.0, 0.3, 0.6, 3.0, 1.5);
        assert!(regions.is_empty());
    }

    #[test]
    fn postprocess_finds_bright_region() {
        let mut prob_map = Array4::<f32>::zeros((1, 1, 64, 64));
        for y in 20..40 {
            for x in 10..50 {
                prob_map[[0, 0, y, x]] = 0.9;
            }
        }
        let regions = postprocess_detection(&prob_map, 1.0, 1.0, 0.3, 0.6, 3.0, 1.5);
        assert!(!regions.is_empty());
        assert!(regions[0].score > 0.6);
    }

    #[test]
    fn postprocess_filters_low_score() {
        let mut prob_map = Array4::<f32>::zeros((1, 1, 64, 64));
        // Score just above threshold but below box_threshold
        for y in 20..25 {
            for x in 10..15 {
                prob_map[[0, 0, y, x]] = 0.35;
            }
        }
        let regions = postprocess_detection(&prob_map, 1.0, 1.0, 0.3, 0.6, 3.0, 1.5);
        assert!(regions.is_empty());
    }

    #[test]
    fn postprocess_filters_small_area() {
        let mut prob_map = Array4::<f32>::zeros((1, 1, 64, 64));
        // Only 2 pixels — below min_area of 3
        prob_map[[0, 0, 30, 30]] = 0.9;
        prob_map[[0, 0, 30, 31]] = 0.9;
        let regions = postprocess_detection(&prob_map, 1.0, 1.0, 0.3, 0.6, 3.0, 1.5);
        assert!(regions.is_empty());
    }

    #[test]
    fn postprocess_multiple_regions_sorted() {
        let mut prob_map = Array4::<f32>::zeros((1, 1, 128, 128));
        // Region A: bottom-left
        for y in 80..90 {
            for x in 10..30 {
                prob_map[[0, 0, y, x]] = 0.9;
            }
        }
        // Region B: top-right
        for y in 10..20 {
            for x in 80..100 {
                prob_map[[0, 0, y, x]] = 0.9;
            }
        }
        let regions = postprocess_detection(&prob_map, 1.0, 1.0, 0.3, 0.6, 3.0, 1.5);
        assert_eq!(regions.len(), 2);
        // Region B (top) should come first
        assert!(regions[0].bbox[1] < regions[1].bbox[1]);
    }

    #[test]
    fn postprocess_scales_back_to_original() {
        let mut prob_map = Array4::<f32>::zeros((1, 1, 64, 64));
        for y in 10..20 {
            for x in 10..20 {
                prob_map[[0, 0, y, x]] = 0.9;
            }
        }
        // scale_x=0.5 means map coords are half of original
        let regions = postprocess_detection(&prob_map, 0.5, 0.5, 0.3, 0.6, 3.0, 1.5);
        assert!(!regions.is_empty());
        // Bbox should be roughly doubled from map coords
        assert!(regions[0].bbox[0] > 10);
        assert!(regions[0].bbox[1] > 10);
    }

    #[test]
    fn connected_components_empty() {
        let mask = vec![false; 16];
        let labels = connected_components(&mask, 4, 4);
        assert!(labels.iter().all(|&l| l == 0));
    }

    #[test]
    fn connected_components_single_blob() {
        let mut mask = vec![false; 16];
        // 2x2 block in top-left
        mask[0] = true;
        mask[1] = true;
        mask[4] = true;
        mask[5] = true;
        let labels = connected_components(&mask, 4, 4);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[0], labels[4]);
        assert_eq!(labels[0], labels[5]);
        assert_ne!(labels[0], 0);
    }

    #[test]
    fn connected_components_two_blobs() {
        let mut mask = vec![false; 25]; // 5x5
                                        // Blob 1: top-left
        mask[0] = true;
        mask[1] = true;
        // Blob 2: bottom-right
        mask[23] = true;
        mask[24] = true;
        let labels = connected_components(&mask, 5, 5);
        assert_ne!(labels[0], 0);
        assert_ne!(labels[23], 0);
        assert_ne!(labels[0], labels[23]);
    }
}
