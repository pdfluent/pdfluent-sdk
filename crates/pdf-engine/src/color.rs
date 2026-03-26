//! Color helpers for render output conversion.

/// Preserve the original DeviceCMYK components as 8-bit output.
pub fn preserve_device_cmyk(c: f32, m: f32, y: f32, k: f32) -> [u8; 4] {
    [
        (c.clamp(0.0, 1.0) * 255.0) as u8,
        (m.clamp(0.0, 1.0) * 255.0) as u8,
        (y.clamp(0.0, 1.0) * 255.0) as u8,
        (k.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

pub(crate) fn rgba_to_cmyk_pixel(rgba: &[u8]) -> [u8; 4] {
    let alpha = rgba[3] as f32 / 255.0;
    let bg = 1.0 - alpha;
    let r = (rgba[0] as f32 / 255.0) * alpha + bg;
    let g = (rgba[1] as f32 / 255.0) * alpha + bg;
    let b = (rgba[2] as f32 / 255.0) * alpha + bg;
    let k = 1.0 - r.max(g).max(b);
    let (c, m, y) = if k >= 1.0 - f32::EPSILON {
        (0.0_f32, 0.0_f32, 0.0_f32)
    } else {
        let inv = 1.0 - k;
        ((inv - r) / inv, (inv - g) / inv, (inv - b) / inv)
    };

    [
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (m.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (y.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (k.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

pub(crate) fn rgba_to_cmyk_buffer(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&rgba_to_cmyk_pixel(px));
    }
    out
}

pub(crate) fn blend_cmyk(bottom: [u8; 4], top: [u8; 4], alpha: u8) -> [u8; 4] {
    let alpha = alpha as f32 / 255.0;
    let inv_alpha = 1.0 - alpha;
    let mut out = [0; 4];
    for (idx, component) in out.iter_mut().enumerate() {
        let blended = bottom[idx] as f32 * inv_alpha + top[idx] as f32 * alpha;
        *component = blended.clamp(0.0, 255.0).round() as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserve_device_cmyk_truncates_to_u8() {
        assert_eq!(
            preserve_device_cmyk(0.25, 0.5, 0.75, 1.0),
            [63, 127, 191, 255]
        );
    }

    #[test]
    fn blend_cmyk_half_alpha() {
        assert_eq!(
            blend_cmyk([0, 0, 0, 0], [255, 128, 64, 32], 128),
            [128, 64, 32, 16]
        );
    }
}
