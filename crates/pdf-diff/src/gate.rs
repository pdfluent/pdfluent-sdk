//! Differential rendering gate.
//!
//! Classifies the SSIM score from a cross-engine comparison (our renderer vs a
//! reference renderer such as mutool, Poppler, or Adobe/pdfRest) into a
//! pass/fail verdict.
//!
//! This is the cross-*engine* match model — our render vs a *different* engine
//! — and is deliberately distinct from same-engine regression tracking (our
//! render vs our own frozen baseline, handled by
//! [`crate::DiffOptions::similarity_threshold`] and the corpus
//! `CI_BASELINE.json`). The threshold here is the cross-engine tolerance, which
//! is looser because different rasterisers legitimately differ at the pixel
//! level.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::ssim;
use crate::PageImage;

/// SSIM at or above this value counts as a visual match against a reference
/// renderer; below it is a regression.
///
/// Calibrated from the measured mutool-vs-pdftoppm SSIM distribution on the
/// corpus (the value the render oracles already use). Cross-engine
/// rasterisation differences keep correctly-rendered pages above this floor.
pub const GATE_SSIM_THRESHOLD: f64 = 0.75;

/// The outcome of a single differential comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DifferentialVerdict {
    /// SSIM >= [`GATE_SSIM_THRESHOLD`]: our render matches the reference.
    Match,
    /// SSIM < [`GATE_SSIM_THRESHOLD`]: our render diverges (a regression).
    Regression,
    /// The reference renderer was unavailable, so no comparison was possible.
    /// The gate treats this as a skip — never a pass and never a fail — so a
    /// missing oracle binary or API key can never mask a real regression.
    OracleUnavailable,
}

/// Classify an SSIM score against [`GATE_SSIM_THRESHOLD`].
pub fn classify(ssim_score: f64) -> DifferentialVerdict {
    if ssim_score >= GATE_SSIM_THRESHOLD {
        DifferentialVerdict::Match
    } else {
        DifferentialVerdict::Regression
    }
}

/// Compare our render against a reference render, returning the SSIM score and
/// the gate verdict. Both images must be RGBA.
pub fn compare(ours: &PageImage, reference: &PageImage) -> (f64, DifferentialVerdict) {
    let score = ssim::compute_ssim(
        &ours.pixels,
        ours.width,
        ours.height,
        &reference.pixels,
        reference.width,
        reference.height,
    );
    (score, classify(score))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> PageImage {
        let pixels = (0..(w * h)).flat_map(|_| rgba).collect();
        PageImage::new(w, h, pixels).unwrap()
    }

    #[test]
    fn identical_renders_match() {
        let img = solid(64, 64, [120, 120, 120, 255]);
        let (score, verdict) = compare(&img, &img);
        assert!(score >= 0.999, "identity ssim should be ~1.0, got {score}");
        assert_eq!(verdict, DifferentialVerdict::Match);
    }

    #[test]
    fn divergent_render_is_a_regression() {
        // Planted regression: a maximally different render (solid black vs
        // solid white) must fall below the gate threshold. This proves the
        // gate *fails* when our output diverges from the reference — with no
        // dependency on any external renderer, so it runs deterministically in
        // any environment.
        let black = solid(64, 64, [0, 0, 0, 255]);
        let white = solid(64, 64, [255, 255, 255, 255]);
        let (score, verdict) = compare(&black, &white);
        assert!(
            score < GATE_SSIM_THRESHOLD,
            "black-vs-white ssim {score} must be below the gate {GATE_SSIM_THRESHOLD}"
        );
        assert_eq!(verdict, DifferentialVerdict::Regression);
    }

    #[test]
    fn classify_respects_the_threshold_boundary() {
        assert_eq!(classify(1.0), DifferentialVerdict::Match);
        assert_eq!(classify(GATE_SSIM_THRESHOLD), DifferentialVerdict::Match);
        assert_eq!(
            classify(GATE_SSIM_THRESHOLD - 0.001),
            DifferentialVerdict::Regression
        );
    }
}
