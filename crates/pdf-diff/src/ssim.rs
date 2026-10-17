//! SSIM (Structural Similarity Index) computation and visual diff generation.
//!
//! SSIM accounts for structural patterns, luminance and contrast.
//! Score: 1.0 = identical, 0.0 = completely different.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

const K1: f64 = 0.01;
const K2: f64 = 0.03;
const L: f64 = 255.0;
const WINDOW_SIZE: usize = 8;

/// Convert RGBA pixels to grayscale luminance values.
fn to_grayscale(rgba: &[u8], width: u32, height: u32) -> Vec<f64> {
    let len = (width * height) as usize;
    let mut gray = Vec::with_capacity(len);
    for i in 0..len {
        let idx = i * 4;
        let r = rgba[idx] as f64;
        let g = rgba[idx + 1] as f64;
        let b = rgba[idx + 2] as f64;
        gray.push(0.299 * r + 0.587 * g + 0.114 * b);
    }
    gray
}

/// Mean and (co)variance over the rectangle `(x0, y0, w, h)` of two
/// grayscale images.
fn region_stats(
    a: &[f64],
    stride_a: usize,
    b: &[f64],
    stride_b: usize,
    rect: (usize, usize, usize, usize),
) -> (f64, f64, f64, f64, f64) {
    let (x0, y0, w, h) = rect;
    let n = (w * h) as f64;
    let mut sum_a = 0.0;
    let mut sum_b = 0.0;
    let mut sum_a2 = 0.0;
    let mut sum_b2 = 0.0;
    let mut sum_ab = 0.0;

    for dy in 0..h {
        for dx in 0..w {
            let va = a[(y0 + dy) * stride_a + (x0 + dx)];
            let vb = b[(y0 + dy) * stride_b + (x0 + dx)];
            sum_a += va;
            sum_b += vb;
            sum_a2 += va * va;
            sum_b2 += vb * vb;
            sum_ab += va * vb;
        }
    }

    let mean_a = sum_a / n;
    let mean_b = sum_b / n;
    let var_a = sum_a2 / n - mean_a * mean_a;
    let var_b = sum_b2 / n - mean_b * mean_b;
    let covar = sum_ab / n - mean_a * mean_b;

    (mean_a, mean_b, var_a, var_b, covar)
}

/// Compute mean and (co)variance for a window at position (x, y).
fn window_stats(
    a: &[f64],
    stride_a: usize,
    b: &[f64],
    stride_b: usize,
    x: usize,
    y: usize,
) -> (f64, f64, f64, f64, f64) {
    region_stats(a, stride_a, b, stride_b, (x, y, WINDOW_SIZE, WINDOW_SIZE))
}

/// The SSIM score for one set of window statistics (the standard formula).
fn ssim_from_stats(mean_a: f64, mean_b: f64, var_a: f64, var_b: f64, covar: f64) -> f64 {
    let c1 = (K1 * L).powi(2);
    let c2 = (K2 * L).powi(2);
    let numerator = (2.0 * mean_a * mean_b + c1) * (2.0 * covar + c2);
    let denominator = (mean_a.powi(2) + mean_b.powi(2) + c1) * (var_a + var_b + c2);
    numerator / denominator
}

/// Compute SSIM between two RGBA images (potentially different dimensions).
///
/// Compares the overlapping region using 8x8 windows with 50% overlap.
/// Returns a score between 0.0 and 1.0.
pub fn compute_ssim(
    img_a: &[u8],
    width_a: u32,
    height_a: u32,
    img_b: &[u8],
    width_b: u32,
    height_b: u32,
) -> f64 {
    let w = width_a.min(width_b) as usize;
    let h = height_a.min(height_b) as usize;

    if w < WINDOW_SIZE || h < WINDOW_SIZE {
        return 1.0;
    }

    let gray_a = to_grayscale(img_a, width_a, height_a);
    let gray_b = to_grayscale(img_b, width_b, height_b);

    let mut total_ssim = 0.0;
    let mut window_count = 0usize;
    let step = WINDOW_SIZE / 2;

    let mut y = 0;
    while y + WINDOW_SIZE <= h {
        let mut x = 0;
        while x + WINDOW_SIZE <= w {
            let (mean_a, mean_b, var_a, var_b, covar) =
                window_stats(&gray_a, width_a as usize, &gray_b, width_b as usize, x, y);

            total_ssim += ssim_from_stats(mean_a, mean_b, var_a, var_b, covar);
            window_count += 1;

            x += step;
        }
        y += step;
    }

    if window_count == 0 {
        return 1.0;
    }
    total_ssim / window_count as f64
}

/// SSIM of one tile: the standard windowed mean restricted to the tile
/// rectangle. A tile smaller than one 8x8 window scores as a single region
/// instead, so edge strips still gate instead of defaulting to 1.0.
fn tile_ssim(
    a: &[f64],
    stride_a: usize,
    b: &[f64],
    stride_b: usize,
    rect: (usize, usize, usize, usize),
) -> f64 {
    let (x0, y0, w, h) = rect;
    let step = WINDOW_SIZE / 2;
    let mut total_ssim = 0.0;
    let mut window_count = 0usize;

    let mut y = y0;
    while y + WINDOW_SIZE <= y0 + h {
        let mut x = x0;
        while x + WINDOW_SIZE <= x0 + w {
            let (mean_a, mean_b, var_a, var_b, covar) =
                window_stats(a, stride_a, b, stride_b, x, y);
            total_ssim += ssim_from_stats(mean_a, mean_b, var_a, var_b, covar);
            window_count += 1;
            x += step;
        }
        y += step;
    }

    if window_count == 0 {
        let (mean_a, mean_b, var_a, var_b, covar) = region_stats(a, stride_a, b, stride_b, rect);
        return ssim_from_stats(mean_a, mean_b, var_a, var_b, covar);
    }
    total_ssim / window_count as f64
}

/// Minimum per-tile SSIM ("tile floor") over `tile_size` px tiles.
///
/// Splits the overlapping region into `tile_size` squares (edge tiles
/// smaller), scores each with the standard 8x8-window SSIM, and returns the
/// minimum. A single localised defect tanks one tile while barely moving the
/// global mean, which is what the floor catches.
pub fn compute_tile_ssim_min(
    img_a: &[u8],
    width_a: u32,
    height_a: u32,
    img_b: &[u8],
    width_b: u32,
    height_b: u32,
    tile_size: u32,
) -> f64 {
    let w = width_a.min(width_b) as usize;
    let h = height_a.min(height_b) as usize;
    if w == 0 || h == 0 {
        return 1.0;
    }

    let gray_a = to_grayscale(img_a, width_a, height_a);
    let gray_b = to_grayscale(img_b, width_b, height_b);

    let tile = tile_size.max(1) as usize;
    let mut floor = 1.0f64;
    let mut ty = 0;
    while ty < h {
        let th = tile.min(h - ty);
        let mut tx = 0;
        while tx < w {
            let tw = tile.min(w - tx);
            let score = tile_ssim(
                &gray_a,
                width_a as usize,
                &gray_b,
                width_b as usize,
                (tx, ty, tw, th),
            );
            floor = floor.min(score);
            tx += tile;
        }
        ty += tile;
    }
    floor
}

/// Generate a visual diff image (RGBA) highlighting per-pixel differences in red.
///
/// Takes two RGBA images with potentially different strides, outputs the
/// overlapping region with differences amplified 5x.
pub fn generate_diff(
    img_a: &[u8],
    width_a: u32,
    img_b: &[u8],
    width_b: u32,
    out_width: u32,
    out_height: u32,
) -> Vec<u8> {
    let mut diff = Vec::with_capacity((out_width * out_height * 4) as usize);

    for y in 0..out_height {
        for x in 0..out_width {
            let idx_a = ((y * width_a + x) * 4) as usize;
            let idx_b = ((y * width_b + x) * 4) as usize;

            let dr = (img_a[idx_a] as i16 - img_b[idx_b] as i16).unsigned_abs();
            let dg = (img_a[idx_a + 1] as i16 - img_b[idx_b + 1] as i16).unsigned_abs();
            let db = (img_a[idx_a + 2] as i16 - img_b[idx_b + 2] as i16).unsigned_abs();

            let avg_diff = ((dr + dg + db) / 3).min(255) as u8;
            let amplified = (avg_diff as u16 * 5).min(255) as u8;

            // Blend: show original with red overlay on differences.
            let base_r = img_a[idx_a];
            let base_g = img_a[idx_a + 1];
            let base_b = img_a[idx_a + 2];

            if amplified > 10 {
                // Changed pixel: red-tinted.
                diff.push(base_r.saturating_add(amplified));
                diff.push(base_g / 2);
                diff.push(base_b / 2);
            } else {
                // Unchanged: slightly dimmed original.
                diff.push(base_r / 2 + 128);
                diff.push(base_g / 2 + 128);
                diff.push(base_b / 2 + 128);
            }
            diff.push(255);
        }
    }

    diff
}
