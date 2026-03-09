//! SVTR text recognition pipeline with CTC greedy decoding.
//!
//! Preprocesses cropped text regions, runs SVTR inference via ONNX Runtime,
//! and decodes the output logits into text using CTC greedy decode.

use ndarray::{Array2, Array4};
use ort::session::Session;

use super::dictionary::Dictionary;
use super::utils;

/// Error type for recognition operations.
#[derive(Debug)]
pub enum RecognitionError {
    Inference(String),
    InvalidShape(String),
}

impl std::fmt::Display for RecognitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference(msg) => write!(f, "recognition inference error: {msg}"),
            Self::InvalidShape(msg) => write!(f, "invalid tensor shape: {msg}"),
        }
    }
}

impl std::error::Error for RecognitionError {}

/// Result of recognizing text in a single cropped region.
#[derive(Debug, Clone)]
pub struct RecognizedText {
    pub text: String,
    pub confidence: f32,
}

/// Preprocess a cropped text region for SVTR recognition.
///
/// Resizes to `target_height` (default 48) while maintaining aspect ratio.
/// Normalizes to [-1, 1] range and transposes HWC → NCHW [1, 3, H, W'].
pub fn preprocess_for_recognition(
    rgb_crop: &[u8],
    width: u32,
    height: u32,
    target_height: u32,
) -> Array4<f32> {
    let new_w = if height == target_height {
        width
    } else {
        let ratio = target_height as f32 / height as f32;
        (width as f32 * ratio).round().max(1.0) as u32
    };

    let resized = utils::resize_rgb_exact(rgb_crop, width, height, new_w, target_height);

    let mut tensor = Array4::<f32>::zeros((1, 3, target_height as usize, new_w as usize));
    for y in 0..target_height as usize {
        for x in 0..new_w as usize {
            let idx = (y * new_w as usize + x) * 3;
            for c in 0..3 {
                tensor[[0, c, y, x]] = (resized[idx + c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }
    tensor
}

/// Preprocess multiple crops into batched tensors.
///
/// Sorts crops by width for efficient padding, pads shorter crops to match
/// the widest in each batch, and splits into batches of `max_batch_size`.
pub fn preprocess_batch(
    crops: &[(Vec<u8>, u32, u32)],
    target_height: u32,
    max_batch_size: usize,
) -> Vec<Array4<f32>> {
    if crops.is_empty() {
        return Vec::new();
    }

    // Preprocess each crop individually to get their natural widths
    let mut singles: Vec<(usize, Array4<f32>)> = crops
        .iter()
        .enumerate()
        .map(|(i, (rgb, w, h))| {
            let t = preprocess_for_recognition(rgb, *w, *h, target_height);
            (i, t)
        })
        .collect();

    // Sort by width (descending) for efficient padding
    singles.sort_by(|a, b| b.1.shape()[3].cmp(&a.1.shape()[3]));

    let mut batches = Vec::new();
    for chunk in singles.chunks(max_batch_size) {
        let max_w = chunk.iter().map(|(_, t)| t.shape()[3]).max().unwrap_or(1);
        let batch_size = chunk.len();

        let mut batch = Array4::<f32>::zeros((batch_size, 3, target_height as usize, max_w));
        for (b_idx, (_, tensor)) in chunk.iter().enumerate() {
            let w = tensor.shape()[3];
            for c in 0..3 {
                for y in 0..target_height as usize {
                    for x in 0..w {
                        batch[[b_idx, c, y, x]] = tensor[[0, c, y, x]];
                    }
                }
            }
        }
        batches.push(batch);
    }
    batches
}

/// Run SVTR recognition inference on a preprocessed batch tensor.
///
/// Returns logits with shape [batch, seq_len, vocab_size].
pub fn recognize_inference(
    session: &mut Session,
    input: &Array4<f32>,
) -> Result<ndarray::Array3<f32>, RecognitionError> {
    let input_tensor = ort::value::Tensor::from_array(input.view().into_dyn().to_owned())
        .map_err(|e| RecognitionError::Inference(e.to_string()))?;

    let outputs = session
        .run(ort::inputs!["x" => input_tensor])
        .map_err(|e| RecognitionError::Inference(e.to_string()))?;

    let output = &outputs[0];
    let (shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| RecognitionError::InvalidShape(e.to_string()))?;

    if shape.len() != 3 {
        return Err(RecognitionError::InvalidShape(format!(
            "expected [batch, seq_len, vocab_size], got shape with {} dims",
            shape.len()
        )));
    }

    let batch = shape[0] as usize;
    let seq_len = shape[1] as usize;
    let vocab = shape[2] as usize;
    ndarray::Array3::from_shape_vec((batch, seq_len, vocab), data.to_vec())
        .map_err(|e| RecognitionError::InvalidShape(e.to_string()))
}

/// CTC greedy decode: convert logits for one sample to text.
///
/// Steps:
/// 1. Argmax per timestep
/// 2. Softmax per timestep for confidence
/// 3. Remove consecutive duplicates
/// 4. Remove blank tokens (index 0)
/// 5. Map to characters via dictionary
/// 6. Geometric mean of per-character confidences
pub fn ctc_greedy_decode(logits: &Array2<f32>, dict: &Dictionary) -> RecognizedText {
    let mut text = String::new();
    let mut confidences = Vec::new();
    let mut prev_index = 0usize;

    for t in 0..logits.shape()[0] {
        let row = logits.row(t);

        // Find argmax and max logit
        let (best_idx, &max_logit) = row
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        // Softmax confidence for the best index
        let exp_sum: f32 = row.iter().map(|&x| (x - max_logit).exp()).sum();
        let conf = 1.0 / exp_sum; // exp(max_logit - max_logit) / exp_sum = 1.0 / exp_sum

        // CTC: skip blanks (index 0) and duplicate consecutive indices
        if best_idx != 0 && best_idx != prev_index {
            if let Some(ch) = dict.get(best_idx) {
                text.push_str(ch);
                confidences.push(conf);
            }
        }
        prev_index = best_idx;
    }

    let avg_conf = if confidences.is_empty() {
        0.0
    } else {
        // Geometric mean via log space
        let log_sum: f32 = confidences.iter().map(|c| c.ln()).sum();
        (log_sum / confidences.len() as f32).exp()
    };

    RecognizedText {
        text,
        confidence: avg_conf,
    }
}

/// Decode a full batch of recognition results.
pub fn ctc_decode_batch(
    logits: &ndarray::Array3<f32>,
    dictionary: &Dictionary,
) -> Vec<RecognizedText> {
    let batch_size = logits.shape()[0];
    (0..batch_size)
        .map(|i| {
            let sample = logits.slice(ndarray::s![i, .., ..]).to_owned();
            ctc_greedy_decode(&sample, dictionary)
        })
        .collect()
}

/// Run recognition on a batch of crops end-to-end.
///
/// Preprocesses, runs inference in batches, and decodes results.
pub fn recognize_batch(
    session: &mut Session,
    dictionary: &Dictionary,
    crops: &[(Vec<u8>, u32, u32)],
    batch_size: usize,
) -> Result<Vec<RecognizedText>, RecognitionError> {
    if crops.is_empty() {
        return Ok(Vec::new());
    }

    // Preprocess each crop individually to preserve original order
    let mut results = vec![
        RecognizedText {
            text: String::new(),
            confidence: 0.0,
        };
        crops.len()
    ];

    // Process in batches
    for chunk_start in (0..crops.len()).step_by(batch_size) {
        let chunk_end = (chunk_start + batch_size).min(crops.len());
        let chunk = &crops[chunk_start..chunk_end];

        let batches = preprocess_batch(chunk, 48, chunk.len());
        if let Some(batch_tensor) = batches.into_iter().next() {
            let logits = recognize_inference(session, &batch_tensor)?;
            let decoded = ctc_decode_batch(&logits, dictionary);
            for (i, result) in decoded.into_iter().enumerate() {
                results[chunk_start + i] = result;
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_dimensions_correct() {
        let rgb = vec![128u8; 200 * 30 * 3];
        let tensor = preprocess_for_recognition(&rgb, 200, 30, 48);
        assert_eq!(tensor.shape()[0], 1); // batch
        assert_eq!(tensor.shape()[1], 3); // channels
        assert_eq!(tensor.shape()[2], 48); // target height
        assert_eq!(tensor.shape()[3], 320); // 200 * 48/30 = 320
    }

    #[test]
    fn preprocess_already_correct_height() {
        let rgb = vec![128u8; 100 * 48 * 3];
        let tensor = preprocess_for_recognition(&rgb, 100, 48, 48);
        assert_eq!(tensor.shape()[2], 48);
        assert_eq!(tensor.shape()[3], 100);
    }

    #[test]
    fn preprocess_normalization_range() {
        let rgb = vec![0u8; 100 * 48 * 3];
        let tensor = preprocess_for_recognition(&rgb, 100, 48, 48);
        // All zeros: (0/255 - 0.5) / 0.5 = -1.0
        assert!((tensor[[0, 0, 0, 0]] - (-1.0)).abs() < 1e-5);

        let rgb_white = vec![255u8; 100 * 48 * 3];
        let tensor_white = preprocess_for_recognition(&rgb_white, 100, 48, 48);
        // All 255: (1.0 - 0.5) / 0.5 = 1.0
        assert!((tensor_white[[0, 0, 0, 0]] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn ctc_decode_simple() {
        let dict = Dictionary::from_bytes(b"H\ne\nl\no\n").unwrap();
        // Pattern: blank, H, H, blank, e, l, l, o
        let mut logits = Array2::<f32>::from_elem((8, 5), -10.0);
        logits[[0, 0]] = 10.0; // blank
        logits[[1, 1]] = 10.0; // H
        logits[[2, 1]] = 10.0; // H (duplicate)
        logits[[3, 0]] = 10.0; // blank
        logits[[4, 2]] = 10.0; // e
        logits[[5, 3]] = 10.0; // l
        logits[[6, 3]] = 10.0; // l (duplicate)
        logits[[7, 4]] = 10.0; // o

        let result = ctc_greedy_decode(&logits, &dict);
        assert_eq!(result.text, "Helo"); // CTC removes duplicates
        assert!(result.confidence > 0.9);
    }

    #[test]
    fn ctc_decode_with_real_duplicate_chars() {
        let dict = Dictionary::from_bytes(b"H\ne\nl\no\n").unwrap();
        // "Hello" requires blank between the two l's
        // Pattern: H, blank, e, l, blank, l, o
        let mut logits = Array2::<f32>::from_elem((7, 5), -10.0);
        logits[[0, 1]] = 10.0; // H
        logits[[1, 0]] = 10.0; // blank
        logits[[2, 2]] = 10.0; // e
        logits[[3, 3]] = 10.0; // l
        logits[[4, 0]] = 10.0; // blank
        logits[[5, 3]] = 10.0; // l
        logits[[6, 4]] = 10.0; // o

        let result = ctc_greedy_decode(&logits, &dict);
        assert_eq!(result.text, "Hello");
    }

    #[test]
    fn ctc_decode_empty_logits() {
        let dict = Dictionary::from_bytes(b"a\n").unwrap();
        let logits = Array2::<f32>::zeros((0, 2));
        let result = ctc_greedy_decode(&logits, &dict);
        assert!(result.text.is_empty());
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn ctc_decode_all_blanks() {
        let dict = Dictionary::from_bytes(b"a\nb\n").unwrap();
        let mut logits = Array2::<f32>::from_elem((5, 3), -10.0);
        for t in 0..5 {
            logits[[t, 0]] = 10.0; // all blank
        }
        let result = ctc_greedy_decode(&logits, &dict);
        assert!(result.text.is_empty());
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn batch_preprocess_pads_correctly() {
        let crops = vec![
            (vec![128u8; 100 * 30 * 3], 100u32, 30u32),
            (vec![128u8; 200 * 30 * 3], 200u32, 30u32),
        ];
        let batches = preprocess_batch(&crops, 48, 8);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].shape()[0], 2); // 2 crops
        assert_eq!(batches[0].shape()[2], 48); // target height
                                               // Width should be the max of the two scaled widths
        let w1 = (100.0_f32 * 48.0 / 30.0).round() as usize; // 160
        let w2 = (200.0_f32 * 48.0 / 30.0).round() as usize; // 320
        assert_eq!(batches[0].shape()[3], w1.max(w2));
    }

    #[test]
    fn batch_preprocess_splits_large_batch() {
        let crops: Vec<_> = (0..5)
            .map(|_| (vec![128u8; 50 * 48 * 3], 50u32, 48u32))
            .collect();
        let batches = preprocess_batch(&crops, 48, 2);
        assert_eq!(batches.len(), 3); // 2 + 2 + 1
        assert_eq!(batches[0].shape()[0], 2);
        assert_eq!(batches[1].shape()[0], 2);
        assert_eq!(batches[2].shape()[0], 1);
    }

    #[test]
    fn ctc_decode_batch_multiple() {
        let dict = Dictionary::from_bytes(b"a\nb\n").unwrap();
        let mut logits = ndarray::Array3::<f32>::from_elem((2, 3, 3), -10.0);
        // Sample 0: "a"
        logits[[0, 0, 1]] = 10.0;
        logits[[0, 1, 0]] = 10.0;
        logits[[0, 2, 0]] = 10.0;
        // Sample 1: "b"
        logits[[1, 0, 2]] = 10.0;
        logits[[1, 1, 0]] = 10.0;
        logits[[1, 2, 0]] = 10.0;

        let results = ctc_decode_batch(&logits, &dict);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text, "a");
        assert_eq!(results[1].text, "b");
    }

    #[test]
    fn preprocess_batch_empty() {
        let batches = preprocess_batch(&[], 48, 8);
        assert!(batches.is_empty());
    }
}
