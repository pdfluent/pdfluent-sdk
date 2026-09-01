use crate::bit_reader::BitWriter;
use crate::filter::FilterResult;
use crate::math::round_f32;
use crate::object::stream::{ImageColorSpace, ImageData, ImageDecodeParams};
use alloc::vec;
use alloc::vec::Vec;
use hayro_jpeg2000::{ColorSpace, DecodeSettings};

impl ImageColorSpace {
    fn num_components(&self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb | Self::RgbFromYCbCr => 3,
            Self::Cmyk => 4,
            Self::Unknown(num) => *num,
        }
    }
}

pub(crate) fn decode(data: &[u8], params: &ImageDecodeParams) -> Option<FilterResult> {
    use crate::object::stream::ImageColorSpace;

    let settings = DecodeSettings {
        resolve_palette_indices: false,
        strict: false,
        target_resolution: params.target_dimension,
    };

    let image = hayro_jpeg2000::Image::new(data, &settings).ok()?;

    let width = image.width();
    let height = image.height();
    let bpc = params.bpc.unwrap_or(image.original_bit_depth());
    let cs = match image.color_space() {
        ColorSpace::Gray => ImageColorSpace::Gray,
        ColorSpace::RGB => ImageColorSpace::Rgb,
        ColorSpace::CMYK => ImageColorSpace::Cmyk,
        ColorSpace::Unknown { num_channels } => ImageColorSpace::Unknown(*num_channels),
        ColorSpace::Icc {
            num_channels: num_components,
            ..
        } => match num_components {
            1 => ImageColorSpace::Gray,
            3 => ImageColorSpace::Rgb,
            4 => ImageColorSpace::Cmyk,
            _ => return None,
        },
    };
    let has_alpha = image.has_alpha();
    // Upstream 0.4.0 moved the scratch buffers into a caller-owned DecoderContext
    // so they can be reused across images; a fresh one per call keeps the old
    // one-shot behaviour.
    let mut decoder_context = hayro_jpeg2000::DecoderContext::default();
    let bitmap = image.decode(&mut decoder_context).ok()?.data_u8();

    let (mut data, mut alpha) = if !has_alpha {
        (bitmap, None)
    } else {
        // Extract the alpha channel.
        // Use checked_add to prevent u8 wrap-to-zero (JPX-01): Unknown(255) + 1
        // would produce total_channels = 0, causing a division-by-zero panic.
        let total_channels = cs.num_components().checked_add(1)?;
        let pixels = bitmap.len() / total_channels as usize;
        let mut color_channels =
            Vec::with_capacity(pixels.checked_mul(cs.num_components() as usize)?);
        let mut alpha_channel = Vec::with_capacity(pixels);

        for sample in bitmap.chunks_exact(total_channels as usize) {
            let (alpha, color) = sample.split_last()?;
            alpha_channel.push(*alpha);
            color_channels.extend_from_slice(color);
        }

        (color_channels, Some(alpha_channel))
    };

    // The decoded image is always 8-bit, so if necessary we have to rescale
    // ourselves.
    if bpc != 8 {
        data = scale(&data, bpc, cs.num_components(), width, height)?;
        alpha = alpha.and_then(|alpha| scale(&alpha, bpc, cs.num_components(), width, height));
    }

    Some(FilterResult {
        data,
        image_data: Some(ImageData {
            alpha,
            color_space: Some(cs),
            bits_per_component: bpc,
            width,
            height,
        }),
    })
}

// Hard cap on scale() output allocation: prevents DoS on pathological images
// that decode successfully but still request huge re-encoding. Upstream #1355
// removed the codec's hardcoded 60000-pixel dimension cap in favour of checked
// allocation arithmetic, so this crate must not assume any dimension bound.
// 512 MiB is generous for any real-world PDF image rescaling use case.
const MAX_SCALE_BYTES: usize = 512 * 1024 * 1024;

fn scale(
    data: &[u8],
    bit_per_component: u8,
    num_components: u8,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    // Guard against shift overflow (JPX-02): 1_u32 << bpc panics for bpc >= 32,
    // and bpc=0 produces mul_factor=0 which is a valid but degenerate case.
    // PDF supports BitsPerComponent 1/2/4/8/16; reject anything outside 1..=31.
    if bit_per_component == 0 || bit_per_component > 31 {
        return None;
    }

    let div_factor = ((1 << 8) - 1) as f32;
    let mul_factor = ((1_u32 << bit_per_component) - 1) as f32;

    // Compute allocation size with overflow checks (JPX-03): on 32-bit / WASM,
    // row_bytes * height overflows usize for large-but-valid dimensions. On 64-bit,
    // (u32::MAX)^2 fits in u64 but cannot be allocated. The explicit MAX_SCALE_BYTES
    // cap catches both cases without relying on the allocator to panic gracefully.
    let row_bytes = (width as u64)
        .checked_mul(num_components as u64)?
        .checked_mul(bit_per_component as u64)?
        .div_ceil(8);
    let total_bytes = row_bytes
        .checked_mul(height as u64)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n <= MAX_SCALE_BYTES)?;

    let mut input = vec![0u8; total_bytes];
    let mut writer = BitWriter::new(&mut input, bit_per_component)?;

    for bytes in data.chunks_exact(num_components as usize * width as usize) {
        for byte in bytes {
            let scaled = round_f32((*byte as f32 / div_factor) * mul_factor) as u32;
            writer.write(scaled)?;
        }

        writer.align();
    }

    let final_pos = writer.cur_pos();
    input.truncate(final_pos);

    Some(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: JPX-02 — scale() with bpc=0 must return None, not produce garbage.
    #[test]
    fn scale_bpc_zero_returns_none() {
        let data = vec![128u8; 4];
        assert!(scale(&data, 0, 1, 2, 2).is_none());
    }

    // Regression: JPX-02 — scale() with bpc>=32 must return None, not panic.
    #[test]
    fn scale_bpc_32_returns_none() {
        let data = vec![128u8; 4];
        assert!(scale(&data, 32, 1, 2, 2).is_none());
    }

    #[test]
    fn scale_bpc_255_returns_none() {
        let data = vec![128u8; 4];
        assert!(scale(&data, 255, 1, 2, 2).is_none());
    }

    // Regression: JPX-03 — scale() with dimensions that overflow usize on 32-bit
    // must return None, not panic or corrupt memory.
    #[test]
    fn scale_overflow_dimensions_returns_none() {
        // row_bytes = u32::MAX * 1 * 8 / 8 = u32::MAX bytes/row
        // total_bytes = u32::MAX * u32::MAX — overflows usize on both 32-bit and 64-bit
        let data: Vec<u8> = Vec::new();
        assert!(scale(&data, 8, 1, u32::MAX, u32::MAX).is_none());
    }

    // Regression: JPX-03 — width × height overflow specifically.
    #[test]
    fn scale_width_height_overflow_returns_none() {
        let data: Vec<u8> = Vec::new();
        // 65535 * 65535 * 8 bytes/component * 16bpc / 8 = ~8GB on 32-bit: overflows
        assert!(scale(&data, 16, 8, 65535, 65535).is_none());
    }

    // Sanity: valid small scale call must succeed.
    #[test]
    fn scale_valid_small_image_returns_some() {
        // 2×2 image, 1 component, decode from 8-bit to 4-bit (bpc=4)
        // Each 8-bit value 255 should scale to the max 4-bit value (15)
        let data = vec![255u8, 255u8, 255u8, 255u8]; // 4 pixels, 1 component
        let result = scale(&data, 4, 1, 2, 2);
        assert!(result.is_some());
        let out = result.unwrap();
        assert!(!out.is_empty());
    }

    // Sanity: 1-bit scale with known values.
    #[test]
    fn scale_1bit_known_values() {
        // bpc=1: div_factor=255, mul_factor=1. 255/255*1 = 1. 0/255*1 = 0.
        let data = vec![255u8, 0u8]; // 2 pixels, 1 component, 1-bit target
        let result = scale(&data, 1, 1, 2, 1);
        assert!(result.is_some());
    }
}
