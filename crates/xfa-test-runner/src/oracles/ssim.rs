//! SSIM (Structural Similarity Index) for visual oracle comparisons.
//!
//! Re-exported from the canonical `pdf-diff` implementation so the test runner
//! and the `pdf-diff` library share a single SSIM definition (and a single
//! differential gate). SSIM accounts for structural patterns, luminance and
//! contrast. Score: 1.0 = identical, 0.0 = completely different.

pub use pdf_diff::ssim::compute_ssim;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_return_ssim_1() {
        let w = 16u32;
        let h = 16u32;
        let pixels: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let v = (i % 256) as u8;
                [v, v, v, 255]
            })
            .collect();

        let score = compute_ssim(&pixels, w, h, &pixels, w, h);
        assert!(
            (score - 1.0).abs() < 1e-6,
            "Expected SSIM ~1.0, got {score}"
        );
    }

    #[test]
    fn different_images_return_low_ssim() {
        let w = 16u32;
        let h = 16u32;
        let white: Vec<u8> = [255, 255, 255, 255].repeat((w * h) as usize);
        let black: Vec<u8> = [0, 0, 0, 255].repeat((w * h) as usize);

        let score = compute_ssim(&white, w, h, &black, w, h);
        assert!(score < 0.1, "Expected low SSIM, got {score}");
    }

    #[test]
    fn tiny_images_return_1() {
        let pixels = vec![128u8, 128, 128, 255, 0, 0, 0, 255];
        let score = compute_ssim(&pixels, 2, 1, &pixels, 2, 1);
        assert!((score - 1.0).abs() < 1e-6);
    }
}
