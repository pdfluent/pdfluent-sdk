//! Angle classifier for detecting 180° rotated text regions.

use ndarray::Array4;
use ort::session::Session;

use super::utils;

/// Error type for angle classification.
#[derive(Debug)]
pub enum ClassifierError {
    Inference(String),
    InvalidShape(String),
}

impl std::fmt::Display for ClassifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference(msg) => write!(f, "classifier inference error: {msg}"),
            Self::InvalidShape(msg) => write!(f, "invalid tensor shape: {msg}"),
        }
    }
}

impl std::error::Error for ClassifierError {}

/// Result of angle classification.
#[derive(Debug, Clone)]
pub struct AngleResult {
    /// 0 = normal, 1 = 180° rotated.
    pub label: u32,
    /// Classification confidence.
    pub confidence: f32,
}

/// Fixed input dimensions for the angle classifier model.
const CLS_HEIGHT: u32 = 48;
const CLS_WIDTH: u32 = 192;

/// Preprocess a crop for angle classification.
///
/// Resizes to 48x192 fixed size, normalizes to [-1, 1], transposes to NCHW.
fn preprocess_for_classifier(rgb_crop: &[u8], width: u32, height: u32) -> Array4<f32> {
    let resized = utils::resize_rgb_exact(rgb_crop, width, height, CLS_WIDTH, CLS_HEIGHT);

    let mut tensor = Array4::<f32>::zeros((1, 3, CLS_HEIGHT as usize, CLS_WIDTH as usize));
    for y in 0..CLS_HEIGHT as usize {
        for x in 0..CLS_WIDTH as usize {
            let idx = (y * CLS_WIDTH as usize + x) * 3;
            for c in 0..3 {
                tensor[[0, c, y, x]] = (resized[idx + c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }
    tensor
}

/// Classify text orientation (0° or 180°) for a single crop.
pub fn classify_angle(
    session: &mut Session,
    crop: &[u8],
    width: u32,
    height: u32,
) -> Result<AngleResult, ClassifierError> {
    let input = preprocess_for_classifier(crop, width, height);
    let input_tensor = ort::value::Tensor::from_array(input.view().into_dyn().to_owned())
        .map_err(|e| ClassifierError::Inference(e.to_string()))?;

    let outputs = session
        .run(ort::inputs!["x" => input_tensor])
        .map_err(|e| ClassifierError::Inference(e.to_string()))?;

    let output = &outputs[0];
    let (shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| ClassifierError::InvalidShape(e.to_string()))?;

    // Output shape: [1, 2] — logits for [normal, rotated_180]
    if shape.len() != 2 || shape[1] != 2 {
        return Err(ClassifierError::InvalidShape(format!(
            "expected [1, 2], got {shape:?}"
        )));
    }

    let logit_0 = data[0];
    let logit_1 = data[1];

    // Softmax
    let max_logit = logit_0.max(logit_1);
    let exp_0 = (logit_0 - max_logit).exp();
    let exp_1 = (logit_1 - max_logit).exp();
    let sum = exp_0 + exp_1;

    let (label, confidence) = if logit_1 > logit_0 {
        (1, exp_1 / sum)
    } else {
        (0, exp_0 / sum)
    };

    Ok(AngleResult { label, confidence })
}

/// Classify and optionally rotate a batch of text crops.
///
/// Crops detected as 180° rotated (label=1) are rotated in-place.
pub fn classify_and_rotate_batch(
    session: &mut Session,
    crops: &mut [(Vec<u8>, u32, u32)],
) -> Result<(), ClassifierError> {
    for crop in crops.iter_mut() {
        let result = classify_angle(session, &crop.0, crop.1, crop.2)?;
        if result.label == 1 {
            crop.0 = utils::rotate_180_rgb(&crop.0, crop.1, crop.2);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_classifier_dimensions() {
        let rgb = vec![128u8; 200 * 30 * 3];
        let tensor = preprocess_for_classifier(&rgb, 200, 30);
        assert_eq!(tensor.shape(), &[1, 3, 48, 192]);
    }

    #[test]
    fn preprocess_classifier_normalization() {
        let rgb = vec![0u8; 200 * 30 * 3];
        let tensor = preprocess_for_classifier(&rgb, 200, 30);
        assert!((tensor[[0, 0, 0, 0]] - (-1.0)).abs() < 1e-5);
    }
}
