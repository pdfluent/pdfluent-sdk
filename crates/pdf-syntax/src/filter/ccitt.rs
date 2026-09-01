use crate::object::Dict;
use crate::object::dict::keys::{
    BLACK_IS_1, COLUMNS, ENCODED_BYTE_ALIGN, END_OF_BLOCK, END_OF_LINE, K, ROWS,
};
use crate::object::stream::{FilterResult, ImageColorSpace, ImageData, ImageDecodeParams};
use alloc::vec::Vec;
use core::iter;
use hayro_ccitt::{DecodeSettings, Decoder, EncodingMode};

pub(crate) fn decode(
    data: &[u8],
    params: Dict<'_>,
    image_params: &ImageDecodeParams,
) -> Option<FilterResult> {
    let k = params.get::<i32>(K).unwrap_or(0);

    // /Rows 0 means "derive row count from end-of-block marker or image height".
    // Treat 0 as absent so the fallback to image_params.height is used.
    // Without this, the Group4 decoder exits immediately (decoded_rows=0 == rows=0).
    let rows = params
        .get::<u32>(ROWS)
        .filter(|&r| r > 0)
        .unwrap_or(image_params.height);
    let end_of_block = params.get::<bool>(END_OF_BLOCK).unwrap_or(true);

    let settings = DecodeSettings {
        columns: params.get::<usize>(COLUMNS).unwrap_or(1728) as u32,
        rows,
        end_of_block,
        end_of_line: params.get::<bool>(END_OF_LINE).unwrap_or(false),
        rows_are_byte_aligned: params.get::<bool>(ENCODED_BYTE_ALIGN).unwrap_or(false),
        encoding: if k < 0 {
            EncodingMode::Group4
        } else if k == 0 {
            EncodingMode::Group3_1D
        } else {
            EncodingMode::Group3_2D { k: k as u32 }
        },
        invert_black: params.get::<bool>(BLACK_IS_1).unwrap_or(false),
    };

    struct ByteDecoder {
        output: Vec<u8>,
        decoded_rows: u32,
        buffer: u8,
        bit_count: u8,
    }

    impl ByteDecoder {
        fn push_bit(&mut self, white: bool) {
            let bit = if white { 1 } else { 0 };
            self.buffer = (self.buffer << 1) | bit;
            self.bit_count += 1;

            if self.bit_count == 8 {
                self.output.push(self.buffer);
                self.buffer = 0;
                self.bit_count = 0;
            }
        }

        fn flush(&mut self) {
            if self.bit_count > 0 {
                let padded = self.buffer << (8 - self.bit_count);
                self.output.push(padded);
                self.buffer = 0;
                self.bit_count = 0;
            }
        }
    }

    impl Decoder for ByteDecoder {
        // Upstream replaced push_pixel/push_pixel_chunk with a single
        // push_pixels, which hands the whole run to the implementor instead of
        // deciding the chunking inside the decoder. The byte-wise fast path is
        // the same one we had; it simply lives on this side of the trait now,
        // where it can see the output buffer's bit alignment.
        fn push_pixels(&mut self, white: bool, count: u32) {
            let (prefix, whole_bytes, tail) =
                hayro_ccitt::split_run(u32::from(self.bit_count), count);

            for _ in 0..prefix {
                self.push_bit(white);
            }
            if whole_bytes > 0 {
                let byte = if white { 0xFF } else { 0x00 };
                self.output
                    .extend(iter::repeat_n(byte, whole_bytes as usize));
            }
            for _ in 0..tail {
                self.push_bit(white);
            }
        }

        fn next_line(&mut self) {
            self.decoded_rows += 1;
            // Flush any remaining bits and align to byte boundary.
            self.flush();
        }
    }

    let mut decoder = ByteDecoder {
        output: Vec::new(),
        decoded_rows: 0,
        buffer: 0,
        bit_count: 0,
    };
    // Upstream 0.3.0 takes a reusable DecoderContext instead of settings by
    // reference: the context is built once and reset per call rather than
    // allocated on every call. One decode here, so it is built inline.
    let mut ctx = hayro_ccitt::DecoderContext::new(settings);
    let result = hayro_ccitt::decode(data, &mut decoder, &mut ctx);

    // If we decoded at least one row, let's be lenient and return what we got.
    // See also 0001763.pdf.
    if result.is_err() && decoder.decoded_rows == 0 {
        return None;
    }
    if result.is_err() {
        crate::leniency::emit(crate::leniency::CCITT_PARTIAL_DECODE);
    }

    Some(FilterResult {
        data: decoder.output,
        image_data: Some(ImageData {
            alpha: None,
            color_space: Some(ImageColorSpace::Gray),
            bits_per_component: 1,
            width: settings.columns,
            // `rows`, not image_params.height. The two come from different
            // places in the same file and nothing makes them agree: the decoder
            // stops after `rows`, so reporting the declared /Height sends a
            // consumer computing height * stride past the end of the data.
            // Ported from hayro upstream (LaurenzV/hayro#1269).
            height: rows,
        }),
    })
}

#[cfg(test)]
mod upstream_hardening_tests {
    use super::*;
    use crate::object::FromBytes;

    /// One row of eight white pixels, Group 3 one-dimensional.
    ///
    /// Taken from upstream's own regression fixture for LaurenzV/hayro#1258.
    const ONE_ROW_G3: &[u8] = &[0x35, 0x14];

    fn params_with(height: u32) -> ImageDecodeParams {
        ImageDecodeParams {
            height,
            ..Default::default()
        }
    }

    /// Ported from LaurenzV/hayro#1269.
    ///
    /// `/Rows` and the image's `/Height` are two numbers from the same file and
    /// nothing makes them agree. The decoder stops after `/Rows` rows, so the
    /// buffer holds that many -- but the reported height was `/Height`, so a
    /// consumer computing `height * stride` walked off the end of the data.
    ///
    /// Upstream's other half of this fix -- sizing the output allocation with a
    /// `checked_mul` -- does not apply here: this decoder grows its output as it
    /// goes rather than preallocating `columns * height`, so there is no
    /// allocation to size wrongly.
    #[test]
    fn the_reported_height_is_the_number_of_rows_decoded_not_the_declared_one() {
        let params = Dict::from_bytes(b"<< /K 0 /Columns 8 /Rows 1 >>").unwrap();
        // The image claims four rows; the CCITT parameters say one.
        let decoded = decode(ONE_ROW_G3, params, &params_with(4)).unwrap();

        let image = decoded.image_data.unwrap();
        assert_eq!(
            image.height, 1,
            "the height must describe the data returned"
        );

        // The invariant the height exists to support: a consumer reading
        // height * stride bytes must not read past what was decoded.
        let stride = (image.width as usize).div_ceil(8);
        assert!(
            decoded.data.len() >= stride * image.height as usize,
            "reported {}x{} needs {} bytes, got {}",
            image.width,
            image.height,
            stride * image.height as usize,
            decoded.data.len()
        );
    }

    /// The usual case must keep working: with no `/Rows`, the image height is
    /// still what the decoder is told to produce and still what it reports.
    #[test]
    fn without_rows_the_image_height_is_still_used() {
        let params = Dict::from_bytes(b"<< /K 0 /Columns 8 >>").unwrap();
        let decoded = decode(ONE_ROW_G3, params, &params_with(1)).unwrap();

        assert_eq!(decoded.image_data.unwrap().height, 1);
    }
}
