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

// ─── P1 render gate (V1) ────────────────────────────────────────────────────
//
// Same-width-and-height renders compare on two bars: the global SSIM over the
// whole page, and the tile floor (the minimum per-tile SSIM over 32x32 px
// tiles). The global mean alone lets a missing word, line or table rule
// through; the floor catches localised defects. Whole-page geometric changes
// (shift, blur, rescale) are not equivalences under this rule — they fail.

/// Global SSIM at or above this value passes the render gate's first bar.
pub const RENDER_GLOBAL_SSIM: f64 = 0.98;
/// Minimum per-tile SSIM at or above this value passes the gate's second bar.
pub const RENDER_TILE_MIN_SSIM: f64 = 0.90;
/// Tile edge length in pixels for the tile floor (edge tiles smaller).
pub const RENDER_TILE_SIZE: u32 = 32;

/// Thresholds for [`compare_render`]. The defaults are the calibrated P1
/// values, matching the `[render]` section of `tests/compat/tolerance.toml`.
#[derive(Debug, Clone, Copy)]
pub struct RenderGateConfig {
    /// Global SSIM bar (default: [`RENDER_GLOBAL_SSIM`]).
    pub global: f64,
    /// Tile-floor bar (default: [`RENDER_TILE_MIN_SSIM`]).
    pub tile_min: f64,
    /// Tile edge in pixels (default: [`RENDER_TILE_SIZE`]).
    pub tile: u32,
}

impl Default for RenderGateConfig {
    fn default() -> Self {
        Self {
            global: RENDER_GLOBAL_SSIM,
            tile_min: RENDER_TILE_MIN_SSIM,
            tile: RENDER_TILE_SIZE,
        }
    }
}

/// The two scores behind a render-gate verdict.
#[derive(Debug, Clone, Copy)]
pub struct RenderScores {
    /// SSIM over the whole overlapping region.
    pub global_ssim: f64,
    /// Minimum per-tile SSIM (see [`ssim::compute_tile_ssim_min`]).
    pub tile_min_ssim: f64,
}

/// Outcome of the P1 render gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RenderVerdict {
    /// Both bars hold (scores at or above the thresholds).
    Pass,
    /// Dimensions differ, or either bar fails.
    Fail,
}

/// Classify two render scores against the gate thresholds. Scores exactly at
/// the thresholds pass.
pub fn classify_render(
    global_ssim: f64,
    tile_min_ssim: f64,
    config: &RenderGateConfig,
) -> RenderVerdict {
    if global_ssim >= config.global && tile_min_ssim >= config.tile_min {
        RenderVerdict::Pass
    } else {
        RenderVerdict::Fail
    }
}

/// Compare two renders under the P1 rule, returning the scores and the gate
/// verdict. Both images must be RGBA. Dimensions must match exactly, else the
/// verdict is [`RenderVerdict::Fail`] (the scores still describe the overlap).
pub fn compare_render(
    ours: &PageImage,
    reference: &PageImage,
    config: &RenderGateConfig,
) -> (RenderScores, RenderVerdict) {
    let global_ssim = ssim::compute_ssim(
        &ours.pixels,
        ours.width,
        ours.height,
        &reference.pixels,
        reference.width,
        reference.height,
    );
    let tile_min_ssim = ssim::compute_tile_ssim_min(
        &ours.pixels,
        ours.width,
        ours.height,
        &reference.pixels,
        reference.width,
        reference.height,
        config.tile,
    );
    let scores = RenderScores {
        global_ssim,
        tile_min_ssim,
    };
    if ours.width != reference.width || ours.height != reference.height {
        return (scores, RenderVerdict::Fail);
    }
    let verdict = classify_render(global_ssim, tile_min_ssim, config);
    (scores, verdict)
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

    fn white_128() -> PageImage {
        solid(128, 128, [255, 255, 255, 255])
    }

    fn with_black_box() -> PageImage {
        let mut img = white_128();
        // 8x8 black box fully inside the (32..64, 32..64) tile.
        for y in 40..48 {
            for x in 40..48 {
                let idx = ((y * 128 + x) * 4) as usize;
                img.pixels[idx] = 0;
                img.pixels[idx + 1] = 0;
                img.pixels[idx + 2] = 0;
            }
        }
        img
    }

    #[test]
    fn tile_floor_catches_small_box_that_global_misses() {
        let config = RenderGateConfig::default();
        let (scores, verdict) = compare_render(&white_128(), &with_black_box(), &config);
        assert!(
            scores.global_ssim >= config.global,
            "global SSIM {} must clear {} for one 8x8 box",
            scores.global_ssim,
            config.global
        );
        assert!(
            scores.tile_min_ssim < config.tile_min,
            "tile floor {} must trip {} for one 8x8 box",
            scores.tile_min_ssim,
            config.tile_min
        );
        assert_eq!(verdict, RenderVerdict::Fail);
    }

    #[test]
    fn one_level_brightness_shift_passes_both_bars() {
        let config = RenderGateConfig::default();
        let dimmed = solid(128, 128, [254, 254, 254, 255]);
        let (scores, verdict) = compare_render(&white_128(), &dimmed, &config);
        assert!(
            scores.global_ssim >= config.global,
            "global SSIM {} must clear {}",
            scores.global_ssim,
            config.global
        );
        assert!(
            scores.tile_min_ssim >= config.tile_min,
            "tile floor {} must clear {}",
            scores.tile_min_ssim,
            config.tile_min
        );
        assert_eq!(verdict, RenderVerdict::Pass);
    }

    #[test]
    fn render_boundary_scores_pass() {
        let config = RenderGateConfig::default();
        assert_eq!(
            classify_render(config.global, config.tile_min, &config),
            RenderVerdict::Pass
        );
        assert_eq!(
            classify_render(config.global - 0.001, config.tile_min, &config),
            RenderVerdict::Fail
        );
        assert_eq!(
            classify_render(config.global, config.tile_min - 0.001, &config),
            RenderVerdict::Fail
        );
    }

    #[test]
    fn render_dimension_mismatch_fails() {
        let config = RenderGateConfig::default();
        let tall = solid(128, 129, [255, 255, 255, 255]);
        let (_, verdict) = compare_render(&white_128(), &tall, &config);
        assert_eq!(verdict, RenderVerdict::Fail);
    }
}
