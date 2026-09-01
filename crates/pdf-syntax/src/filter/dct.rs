use crate::object::Dict;
use crate::object::dict::keys::COLOR_TRANSFORM;
use crate::object::stream::{FilterResult, ImageColorSpace, ImageData, ImageDecodeParams};
use alloc::borrow::Cow;
use core::num::NonZeroU32;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::colorspace::ColorSpace::CMYK;
use zune_jpeg::zune_core::options::DecoderOptions;

pub(crate) fn decode(
    data: &[u8],
    params: Dict<'_>,
    image_params: &ImageDecodeParams,
) -> Option<FilterResult> {
    if image_params.width > u16::MAX as u32 || image_params.height > u16::MAX as u32 {
        return None;
    }

    // Some PDFs have weird JPEGs where the JPEG metadata is completely wrong
    // (for example indicating that one of the dimensions is u16::MAX), but the
    // metadata in the PDF image dictionary is correct. Therefore, we first
    // validate the JPEG metadata and patch the data if any of the dimensions
    // are too large (if they are too small, they will just be padded later on).
    let data = maybe_patch_jpeg_dimensions(data, image_params)?;

    let options = DecoderOptions::default()
        .set_max_width(u16::MAX as usize)
        .set_max_height(u16::MAX as usize);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(&*data), options);
    decoder.decode_headers().ok()?;

    let color_transform = params.get::<u8>(COLOR_TRANSFORM);
    let input_color_space = decoder.input_colorspace()?;

    // Track whether the JPEG decoder will apply a YCbCr→RGB colour transform.
    // When it does, the decoded bytes are already in sRGB colorimetry (BT.601
    // matrix) and any PDF ICCBased profile must NOT be applied on top.
    let mut jpeg_ycbcr_to_rgb = false;

    let mut out_colorspace = if let Some(num_components) = image_params.num_components
        && !matches!(num_components, 1 | 3 | 4)
    {
        ColorSpace::MultiBand(NonZeroU32::new(num_components as u32)?)
    } else {
        match input_color_space {
            ColorSpace::YCbCr => {
                if color_transform.is_none_or(|c| c == 1) {
                    jpeg_ycbcr_to_rgb = true;
                    ColorSpace::RGB
                } else {
                    ColorSpace::YCbCr
                }
            }
            ColorSpace::RGB | ColorSpace::RGBA => ColorSpace::RGB,
            ColorSpace::Luma | ColorSpace::LumaA => ColorSpace::Luma,
            // TODO: Find test case with color transform on cmyk
            CMYK => CMYK,
            ColorSpace::YCCK => ColorSpace::YCCK,
            _ => ColorSpace::RGB,
        }
    };

    // In case image had APP14 marker, we might have to override the colorspace.
    if input_color_space == CMYK && decoder.info()?.components == 3 {
        out_colorspace = ColorSpace::RGB;
    }

    decoder.set_options(DecoderOptions::default().jpeg_set_out_colorspace(out_colorspace));
    let mut decoded = decoder.decode().ok()?;

    if out_colorspace == ColorSpace::YCCK {
        // YCCK JPEG: channels 0-2 are YCbCr, channel 3 is K (JPEG-inverted: 255=no ink).
        // Convert YCbCr to CMY (ink-density form: 0=no ink, 255=full ink), then invert K
        // so all four channels are in PDF DeviceCMYK convention (0=no ink, 255=full ink).
        // Downstream DeviceCMYK ICC profile expects this convention.
        //
        // Conversion formula adapted from:
        // <https://github.com/mozilla/pdf.js/blob/69595a29192b7704733404a42a2ebb537601117b/src/core/jpg.js#L1331>
        // Values are clamped to [0, 255] to avoid wrapping artefacts from the as-cast.
        for c in decoded.chunks_mut(4) {
            let y = c[0] as f32;
            let cb = c[1] as f32;
            let cr = c[2] as f32;
            c[0] = (434.456 - y - 1.402 * cr).clamp(0.0, 255.0) as u8;
            c[1] = (119.541 - y + 0.344 * cb + 0.714 * cr).clamp(0.0, 255.0) as u8;
            c[2] = (481.816 - y - 1.772 * cb).clamp(0.0, 255.0) as u8;
            // Invert K: JPEG stores K as 255=no ink; DeviceCMYK expects 0=no ink.
            c[3] = 255 - c[3];
        }
    }

    // JPEG CMYK (including YCCK after the conversion above): JPEG encodes CMYK as
    // inverted ink density (255 = no ink, 0 = full ink), i.e. the complement of the
    // PDF DeviceCMYK convention (0 = no ink).  Invert all channels so that the data
    // can be fed directly into the DeviceCMYK ICC profile which expects standard
    // DeviceCMYK values.
    if out_colorspace == CMYK {
        for byte in &mut decoded {
            *byte = 255 - *byte;
        }
    }

    let (w, h) = decoder.dimensions()?;
    let width = w as u32;
    let height = h as u32;

    let image_data = ImageData {
        alpha: None,
        color_space: match out_colorspace {
            ColorSpace::RGB | ColorSpace::YCbCr => {
                if jpeg_ycbcr_to_rgb {
                    // Signal that the bytes are already in sRGB (BT.601 matrix
                    // applied by the JPEG decoder). x_object.rs uses this to
                    // skip any PDF ICCBased/CalRGB conversion.
                    Some(ImageColorSpace::RgbFromYCbCr)
                } else {
                    Some(ImageColorSpace::Rgb)
                }
            }
            ColorSpace::Luma => Some(ImageColorSpace::Gray),
            ColorSpace::YCCK | CMYK => Some(ImageColorSpace::Cmyk),
            ColorSpace::MultiBand(_) => None,
            _ => None,
        },
        bits_per_component: 8,
        width,
        height,
    };

    Some(FilterResult {
        data: decoded,
        image_data: Some(image_data),
    })
}

fn maybe_patch_jpeg_dimensions<'a>(
    data: &'a [u8],
    image_params: &ImageDecodeParams,
) -> Option<Cow<'a, [u8]>> {
    let sof_offset = find_sof_marker(data)?;

    // Every one of these is derived from file bytes: the marker offset, the two
    // dimensions and both areas. Ported from hayro upstream
    // (LaurenzV/hayro#1194).
    let height_offset = sof_offset.checked_add(5)?;
    let width_offset = sof_offset.checked_add(7)?;

    let jpeg_height = u16::from_be_bytes([
        *data.get(height_offset)?,
        *data.get(height_offset.checked_add(1)?)?,
    ]);
    let jpeg_width = u16::from_be_bytes([
        *data.get(width_offset)?,
        *data.get(width_offset.checked_add(1)?)?,
    ]);

    let jpeg_area = (jpeg_width as usize).checked_mul(jpeg_height as usize)?;
    let image_area = (image_params.width as usize).checked_mul(image_params.height as usize)?;
    let need_patch = jpeg_area > image_area;

    if !need_patch {
        return Some(Cow::Borrowed(data));
    }

    let target_w = (image_params.width as u16).to_be_bytes();
    let target_h = (image_params.height as u16).to_be_bytes();

    let mut patched = data.to_vec();
    patched[height_offset..height_offset.checked_add(2)?].copy_from_slice(&target_h);
    patched[width_offset..width_offset.checked_add(2)?].copy_from_slice(&target_w);

    Some(Cow::Owned(patched))
}

fn find_sof_marker(data: &[u8]) -> Option<usize> {
    let mut i = 0_usize;

    while i.checked_add(1).is_some_and(|next| next < data.len()) {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }

        let marker = data[i + 1];

        // Note: Not sure if 100% correct/robust, is AI-generated.
        match marker {
            // All SOF markers carry dimensions: SOF0–SOF15, excluding
            // 0xC4 (DHT), 0xC8 (JPG), 0xCC (DAC) which are not frame markers.
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                return Some(i);
            }
            // Skip padding bytes (0xFF followed by 0xFF).
            0xFF => {
                i += 1;

                continue;
            }
            // SOI (0xD8), EOI (0xD9), TEM (0x01) and stuffed byte (0x00)
            // are standalone markers with no payload.
            0xD8 | 0xD9 | 0x01 | 0x00 => {
                i += 2;

                continue;
            }
            // All other markers have a 2-byte length field — skip over them.
            _ => {
                let len_start = i.checked_add(2)?;
                let len_end = i.checked_add(3)?;
                let seg_len =
                    u16::from_be_bytes([*data.get(len_start)?, *data.get(len_end)?]) as usize;

                i = i.checked_add(2)?.checked_add(seg_len)?;
            }
        }
    }

    None
}

/// Regression tests for fixes ported from hayro upstream (LaurenzV/hayro#1194).
#[cfg(test)]
mod upstream_hardening_tests {
    use super::find_sof_marker;

    /// The segment length is two file bytes and used to be added to the cursor
    /// unchecked: `i += 2 + seg_len` walks past `usize::MAX` on a marker near
    /// the end of a long buffer.
    #[test]
    fn find_sof_marker_survives_a_maximal_segment_length() {
        // APP0 marker declaring a 0xFFFF-byte segment, with nothing behind it.
        let data = [0xFF, 0xE0, 0xFF, 0xFF];
        assert_eq!(find_sof_marker(&data), None);
    }

    #[test]
    fn find_sof_marker_survives_a_truncated_length_field() {
        let data = [0xFF, 0xE0, 0x00];
        assert_eq!(find_sof_marker(&data), None);
    }

    /// A real SOF0 must still be found, so the guards cannot be satisfied by
    /// bailing out early on everything.
    #[test]
    fn find_sof_marker_still_finds_sof0() {
        let data = [0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        assert_eq!(find_sof_marker(&data), Some(2));
    }
}
