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

    // How many rows to decode. Three sources disagree and all three come from
    // the file, so the rule is the maximum of what /Rows asks for and what the
    // image dictionary declares:
    //
    // * /Rows 0 means "derive it from the end-of-block marker or the image
    //   height". Treated as absent; without this the Group4 decoder exits
    //   immediately, because decoded_rows == rows == 0.
    // * /Rows *below* the declared height means the file is malformed, and
    //   upstream decodes to the height anyway rather than truncating the image
    //   -- Chromium does the same. Ported from LaurenzV/hayro#1339; before it,
    //   a page whose /Rows undercounted rendered as a sliver.
    //
    // The upper bound is safe: image_params.height is what the pixel-limit
    // check in Stream::decoded_image was applied to, and the count actually
    // produced is checked against that limit again below.
    let rows = params
        .get::<u32>(ROWS)
        .unwrap_or(0)
        .max(image_params.height);
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

    // The pixel limit in Stream::decoded_image was checked against the DECLARED
    // /Height, before any of this ran. Reporting decoded_rows -- which is the
    // honest count, and the point of the fix above -- lets a file declare a
    // small /Height to get past that check and then hand a much larger row count
    // to get_components, which allocates at least a u16 per pixel. The fix for
    // one hole opened another. (Codex, #1609.)
    //
    // So the limit is applied again, to what was actually produced.
    if let Some(limit) = params.ctx().load_limits().image_pixel_limit() {
        let pixels = u64::from(settings.columns).saturating_mul(u64::from(decoder.decoded_rows));
        if pixels > u64::from(limit) {
            log::warn!(
                "CCITT decoded {} rows of {} columns = {pixels} pixels, over the limit {limit}",
                decoder.decoded_rows,
                settings.columns
            );
            return None;
        }
    }

    Some(FilterResult {
        data: decoder.output,
        image_data: Some(ImageData {
            alpha: None,
            color_space: Some(ImageColorSpace::Gray),
            bits_per_component: 1,
            width: settings.columns,
            // The rows actually decoded -- not `image_params.height`, and not
            // `rows` either.
            //
            // Upstream (LaurenzV/hayro#1269) moved from /Height to /Rows, which
            // fixes the common case. It is still an upper bound: a stream that
            // ends early, or carries an end-of-block before /Rows, decodes fewer.
            // The leniency branch above exists precisely because that happens.
            //
            // The gap is reachable on purpose. A file declaring a small /Height
            // (so the pixel-limit check passes) and a huge /Rows, carrying one
            // encoded row, would have get_components allocate and zero-pad
            // rows * width samples. decoded_rows is what the buffer holds.
            // (Codex, #1609.)
            height: decoder.decoded_rows,
        }),
    })
}

#[cfg(test)]
mod upstream_hardening_tests {
    use super::*;
    use crate::object::FromBytes;
    use crate::reader::{Reader, ReaderContext, ReaderExt};

    /// One row of eight white pixels, Group 3 one-dimensional.
    ///
    /// Taken from upstream's own regression fixture for LaurenzV/hayro#1258.
    const ONE_ROW_G3: &[u8] = &[0x35, 0x14];

    /// The same dictionary, but read through a context that carries a pixel
    /// limit -- which `Dict::from_bytes` cannot give us, since it uses a dummy
    /// context with the defaults.
    fn params_with_pixel_limit(src: &[u8], limit: u32) -> Dict<'_> {
        let limits = crate::pdf::PdfLoadLimits::new().max_image_pixels(u64::from(limit));
        Reader::new(src)
            .read_with_context::<Dict<'_>>(&ReaderContext::dummy_with_limits(limits))
            .expect("the test's own dictionary must parse")
    }

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

    /// The gap between "asked for" and "produced", which `/Rows` alone does not
    /// close (Codex, #1609).
    ///
    /// A small `/Height` passes the pixel-limit check upstream of here; a huge
    /// `/Rows` then sets the reported height, while the data encodes one row.
    /// A consumer sizing a buffer from the reported height allocates and
    /// zero-pads a thousand rows for eight bytes of data.
    #[test]
    fn a_stream_that_stops_early_reports_what_it_produced_not_what_it_promised() {
        let params = Dict::from_bytes(b"<< /K 0 /Columns 8 /Rows 1000 >>").unwrap();
        let decoded = decode(ONE_ROW_G3, params, &params_with(1)).unwrap();

        let image = decoded.image_data.unwrap();
        assert_eq!(
            image.height, 1,
            "one row was encoded, whatever /Rows claims"
        );

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

    /// The hole the `decoded_rows` fix opened, and the second check that closes it.
    ///
    /// `Stream::decoded_image` applies the pixel limit to the DECLARED
    /// `/Height`, before any decoding. Reporting the decoded row count is the
    /// honest answer, and it also means a file can declare a tiny `/Height` to
    /// slip past that check and then hand a far larger count to
    /// `get_components`, which allocates at least a `u16` per pixel.
    /// (Codex, #1609.)
    #[test]
    fn decoded_rows_are_checked_against_the_pixel_limit_too() {
        // Declared height 1, so the limit upstream of here is satisfied. The
        // data decodes one row of eight columns: 8 pixels against a limit of 4.
        let params = params_with_pixel_limit(b"<< /K 0 /Columns 8 /Rows 1 >>", 4);
        assert!(
            decode(ONE_ROW_G3, params, &params_with(1)).is_none(),
            "8 decoded pixels must not pass a 4-pixel limit"
        );
    }

    /// And the limit must not fire on an image that fits, or every CCITT image
    /// in a document with a limit set would vanish.
    #[test]
    fn an_image_inside_the_pixel_limit_still_decodes() {
        let params = params_with_pixel_limit(b"<< /K 0 /Columns 8 /Rows 1 >>", 64);
        assert!(decode(ONE_ROW_G3, params, &params_with(1)).is_some());
    }

    /// Ported from LaurenzV/hayro#1339: `/Rows` below the declared height is a
    /// malformed file, and the image is decoded to the height rather than
    /// truncated. Chromium does the same.
    ///
    /// Before the port this rendered as a single row out of four -- a sliver
    /// where the page has a picture.
    #[test]
    fn rows_below_the_declared_height_decodes_the_whole_image_anyway() {
        let data: Vec<u8> = ONE_ROW_G3.iter().copied().cycle().take(4).collect();
        let params = Dict::from_bytes(b"<< /K 0 /Columns 8 /Rows 1 >>").unwrap();
        let decoded = decode(&data, params, &params_with(4)).unwrap();

        assert_eq!(
            decoded.image_data.unwrap().height,
            4,
            "/Rows 1 against a declared height of 4 must not truncate the image"
        );
    }
}
