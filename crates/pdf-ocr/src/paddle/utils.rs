//! Shared image utilities for PaddleOCR preprocessing.

/// Resize RGB image maintaining aspect ratio so the longest side equals `max_side`.
/// Returns (resized_rgb, new_width, new_height, scale_x, scale_y).
pub fn resize_with_aspect_ratio(
    rgb: &[u8],
    width: u32,
    height: u32,
    max_side: u32,
) -> (Vec<u8>, u32, u32, f32, f32) {
    let ratio = if width >= height {
        max_side as f32 / width as f32
    } else {
        max_side as f32 / height as f32
    };

    // Don't upscale
    let ratio = ratio.min(1.0);

    let new_w = (width as f32 * ratio).round() as u32;
    let new_h = (height as f32 * ratio).round() as u32;

    let resized = bilinear_resize_rgb(rgb, width, height, new_w, new_h);
    let scale_x = new_w as f32 / width as f32;
    let scale_y = new_h as f32 / height as f32;
    (resized, new_w, new_h, scale_x, scale_y)
}

/// Bilinear interpolation resize for RGB images.
pub fn bilinear_resize_rgb(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == dst_w && src_h == dst_h {
        return src.to_vec();
    }
    let mut dst = vec![0u8; (dst_w * dst_h * 3) as usize];
    let x_ratio = if dst_w > 1 {
        (src_w as f64 - 1.0) / (dst_w as f64 - 1.0)
    } else {
        0.0
    };
    let y_ratio = if dst_h > 1 {
        (src_h as f64 - 1.0) / (dst_h as f64 - 1.0)
    } else {
        0.0
    };

    for dy in 0..dst_h {
        let sy = y_ratio * dy as f64;
        let sy0 = sy.floor() as u32;
        let sy1 = (sy0 + 1).min(src_h - 1);
        let fy = (sy - sy0 as f64) as f32;

        for dx in 0..dst_w {
            let sx = x_ratio * dx as f64;
            let sx0 = sx.floor() as u32;
            let sx1 = (sx0 + 1).min(src_w - 1);
            let fx = (sx - sx0 as f64) as f32;

            let idx00 = ((sy0 * src_w + sx0) * 3) as usize;
            let idx10 = ((sy0 * src_w + sx1) * 3) as usize;
            let idx01 = ((sy1 * src_w + sx0) * 3) as usize;
            let idx11 = ((sy1 * src_w + sx1) * 3) as usize;
            let dst_idx = ((dy * dst_w + dx) * 3) as usize;

            for c in 0..3 {
                let v00 = src[idx00 + c] as f32;
                let v10 = src[idx10 + c] as f32;
                let v01 = src[idx01 + c] as f32;
                let v11 = src[idx11 + c] as f32;

                let top = v00 + fx * (v10 - v00);
                let bot = v01 + fx * (v11 - v01);
                let val = top + fy * (bot - top);
                dst[dst_idx + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    dst
}

/// Resize RGB image to exact target dimensions (no aspect ratio).
pub fn resize_rgb_exact(
    rgb: &[u8],
    width: u32,
    height: u32,
    target_w: u32,
    target_h: u32,
) -> Vec<u8> {
    bilinear_resize_rgb(rgb, width, height, target_w, target_h)
}

/// Crop a rectangular region from an RGB image.
/// Coordinates are clamped to image bounds.
pub fn crop_rgb(
    rgb: &[u8],
    img_w: u32,
    img_h: u32,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) -> (Vec<u8>, u32, u32) {
    let x0 = x0.min(img_w);
    let y0 = y0.min(img_h);
    let x1 = x1.min(img_w).max(x0);
    let y1 = y1.min(img_h).max(y0);

    let crop_w = x1 - x0;
    let crop_h = y1 - y0;

    if crop_w == 0 || crop_h == 0 {
        return (Vec::new(), 0, 0);
    }

    let mut out = vec![0u8; (crop_w * crop_h * 3) as usize];
    for row in 0..crop_h {
        let src_start = ((y0 + row) * img_w + x0) as usize * 3;
        let dst_start = (row * crop_w) as usize * 3;
        let len = crop_w as usize * 3;
        out[dst_start..dst_start + len].copy_from_slice(&rgb[src_start..src_start + len]);
    }
    (out, crop_w, crop_h)
}

/// Rotate an RGB image 180 degrees.
pub fn rotate_180_rgb(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut out = vec![0u8; pixel_count * 3];
    for i in 0..pixel_count {
        let j = pixel_count - 1 - i;
        out[j * 3..j * 3 + 3].copy_from_slice(&rgb[i * 3..i * 3 + 3]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_no_upscale() {
        let rgb = vec![128u8; 100 * 100 * 3];
        let (_, w, h, _, _) = resize_with_aspect_ratio(&rgb, 100, 100, 960);
        assert_eq!(w, 100);
        assert_eq!(h, 100);
    }

    #[test]
    fn resize_downscale() {
        let rgb = vec![128u8; 1920 * 1080 * 3];
        let (_, w, h, _, _) = resize_with_aspect_ratio(&rgb, 1920, 1080, 960);
        assert_eq!(w, 960);
        assert_eq!(h, 540);
    }

    #[test]
    fn crop_simple() {
        // 4x4 red image with 2x2 green center
        let mut img = vec![255u8, 0, 0].repeat(16);
        for y in 1..3u32 {
            for x in 1..3u32 {
                let idx = (y * 4 + x) as usize * 3;
                img[idx] = 0;
                img[idx + 1] = 255;
                img[idx + 2] = 0;
            }
        }
        let (crop, w, h) = crop_rgb(&img, 4, 4, 1, 1, 3, 3);
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(&crop[0..3], &[0, 255, 0]);
    }

    #[test]
    fn rotate_180() {
        // 2x1 image: red then green
        let rgb = vec![255, 0, 0, 0, 255, 0];
        let rotated = rotate_180_rgb(&rgb, 2, 1);
        assert_eq!(&rotated[0..3], &[0, 255, 0]); // green first
        assert_eq!(&rotated[3..6], &[255, 0, 0]); // red second
    }
}
