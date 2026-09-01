//! Streams.

use crate::crypto::DecryptionTarget;
use crate::filter::Filter;
use crate::object;
use crate::object::Dict;
use crate::object::Name;
use crate::object::dict::keys::{DECODE_PARMS, DP, F, FILTER, LENGTH, TYPE};
use crate::object::{Array, ObjectIdentifier};
use crate::object::{Object, ObjectLike};
use crate::reader::Reader;
use crate::reader::{Readable, ReaderContext, ReaderExt, Skippable};
use crate::sync::Arc;
use crate::util::OptionLog;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter};
use log::warn;
use smallvec::SmallVec;

#[derive(Clone)]
struct StreamInner<'a> {
    dict: Dict<'a>,
    filters: SmallVec<[Filter; 2]>,
    filter_params: SmallVec<[Dict<'a>; 2]>,
    data: &'a [u8],
    /// Maximum decoded size enforced in `decoded()` / `decoded_image()`.
    /// Comes from [`PdfLoadLimits::stream_byte_limit`] at parse time.
    /// `u64::MAX` means no limit (same sentinel as [`PdfLoadLimits::max_stream_bytes`]).
    stream_byte_limit: u64,
}

/// A stream of arbitrary data.
#[derive(Clone)]
pub struct Stream<'a>(Arc<StreamInner<'a>>);

impl PartialEq for Stream<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.dict == other.0.dict && self.0.data == other.0.data
    }
}

/// Additional parameters for decoding images.
#[derive(Clone, PartialEq, Default)]
pub struct ImageDecodeParams {
    /// Whether the color space of the image is an indexed color space.
    pub is_indexed: bool,
    /// The bits per component of the image, if that information is available.
    pub bpc: Option<u8>,
    /// The components per channel of the image, if that information is available.
    pub num_components: Option<u8>,
    /// A target resolution for the image. Note that this is only a hint so that
    /// in case it's possible, a version of the image will be extracted that
    /// is as close as possible to the hinted dimension.
    pub target_dimension: Option<(u32, u32)>,
    /// The width of the image as indicated by the image dictionary.
    pub width: u32,
    /// The height of the image as indicated by the image dictionary.
    pub height: u32,
    /// The image pixel limit in force, if one is configured.
    ///
    /// Set by [`Stream::decoded_image`] from the stream's own context, not by
    /// the caller. A filter cannot read it for itself: the only dictionary it
    /// receives is `/DecodeParms`, which is `Dict::default()` when the stream
    /// has none -- and that carries a context with no limits at all. The CCITT
    /// decoder's own limit check was reading exactly that, so a stream without
    /// `/DecodeParms` skipped it entirely. (Codex, #1609.)
    pub pixel_limit: Option<u32>,
}

impl<'a> Stream<'a> {
    pub(crate) fn new(data: &'a [u8], dict: Dict<'a>, stream_byte_limit: u64) -> Self {
        let mut collected_filters = SmallVec::new();
        let mut collected_params = SmallVec::new();

        if let Some(filter) = dict
            .get::<Name>(F)
            .or_else(|| dict.get::<Name>(FILTER))
            .and_then(Filter::from_name)
        {
            let params = dict
                .get::<Dict<'_>>(DP)
                .or_else(|| dict.get::<Dict<'_>>(DECODE_PARMS))
                .unwrap_or_default();

            collected_filters.push(filter);
            collected_params.push(params);
        } else if let Some(filters) = dict
            .get::<Array<'_>>(F)
            .or_else(|| dict.get::<Array<'_>>(FILTER))
        {
            let filters = filters.iter::<Name>().map(Filter::from_name);
            let mut params = dict
                .get::<Array<'_>>(DP)
                .or_else(|| dict.get::<Array<'_>>(DECODE_PARMS))
                .map(|a| a.iter::<Object<'_>>());

            for filter in filters {
                let params = params
                    .as_mut()
                    .and_then(|p| p.next())
                    .and_then(|p| p.into_dict())
                    .unwrap_or_default();

                if let Some(filter) = filter {
                    collected_filters.push(filter);
                    collected_params.push(params);
                }
            }
        }

        Self(Arc::new(StreamInner {
            dict,
            filters: collected_filters,
            filter_params: collected_params,
            data,
            stream_byte_limit,
        }))
    }

    /// Return the raw, decrypted data of the stream.
    ///
    /// Stream filters will not be applied.
    pub fn raw_data(&self) -> Cow<'a, [u8]> {
        let ctx = self.0.dict.ctx();

        if ctx.xref().needs_decryption(ctx)
            && self
                .0
                .dict
                .get::<object::String>(TYPE)
                .map(|t| t.as_ref() != b"XRef")
                .unwrap_or(true)
        {
            // Streams are always indirect objects and therefore always have an obj_id.
            // If somehow absent (corrupt PDF), fall back to raw data.
            if let Some(obj_id) = self.0.dict.obj_id() {
                Cow::Owned(
                    ctx.xref()
                        .decrypt(obj_id, self.0.data, DecryptionTarget::Stream)
                        .unwrap_or_default(),
                )
            } else {
                Cow::Borrowed(self.0.data)
            }
        } else {
            Cow::Borrowed(self.0.data)
        }
    }

    /// Return the raw, underlying dictionary of the stream.
    pub fn dict(&self) -> &Dict<'a> {
        &self.0.dict
    }

    /// Return the object identifier of the stream, if available.
    ///
    /// Returns `None` if the stream is corrupt and lacks an object ID.
    pub fn obj_id(&self) -> Option<ObjectIdentifier> {
        self.0.dict.obj_id()
    }

    /// Return the filters that are applied to the stream.
    pub fn filters(&self) -> &[Filter] {
        &self.0.filters
    }

    /// Return the decoded data of the stream.
    ///
    /// Note that the result of this method will not be cached, so calling it multiple
    /// times is expensive.
    pub fn decoded(&self) -> Result<Vec<u8>, DecodeFailure> {
        self.decoded_image(&ImageDecodeParams::default())
            .map(|r| r.data)
    }

    /// Return the decoded data of the stream, and return image metadata
    /// if available.
    pub fn decoded_image(
        &self,
        image_params: &ImageDecodeParams,
    ) -> Result<FilterResult, DecodeFailure> {
        // Carried down to the filters, which have no other way to reach it.
        let limit_in_force = self.0.dict.ctx().load_limits().image_pixel_limit();
        let image_params = &ImageDecodeParams {
            pixel_limit: limit_in_force,
            ..image_params.clone()
        };

        if let Some(limit) = limit_in_force
            && image_params.width > 0
            && image_params.height > 0
        {
            let pixels =
                u64::from(image_params.width).saturating_mul(u64::from(image_params.height));
            if pixels > u64::from(limit) {
                warn!("image pixel count {pixels} exceeds limit {limit}, stopping image decode");
                return Err(DecodeFailure::ImageDecode);
            }
        }

        let data = self.raw_data();

        let mut current: Option<FilterResult> = None;

        for (filter, params) in self.0.filters.iter().zip(self.0.filter_params.iter()) {
            let new = filter.apply(
                current.as_ref().map(|c| c.data.as_ref()).unwrap_or(&data),
                params.clone(),
                image_params,
            )?;
            current = Some(new);
        }

        let result = current.unwrap_or(FilterResult {
            data: data.to_vec(),
            image_data: None,
        });

        let limit = self.0.stream_byte_limit;
        if limit != u64::MAX {
            let observed = result.data.len() as u64;
            if observed > limit {
                warn!("decoded stream size {observed} exceeds limit {limit}, stopping decode");
                return Err(DecodeFailure::StreamTooLarge { observed, limit });
            }
        }

        Ok(result)
    }
}

impl Debug for Stream<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "Stream (len: {:?})", self.0.data.len())
    }
}

impl Skippable for Stream<'_> {
    fn skip(_: &mut Reader<'_>, _: bool) -> Option<()> {
        // A stream can never appear in a dict/array, so it should never be skipped.
        warn!("attempted to skip a stream object");

        None
    }
}

impl<'a> Readable<'a> for Stream<'a> {
    fn read(r: &mut Reader<'a>, ctx: &ReaderContext<'a>) -> Option<Self> {
        let dict = r.read_with_context::<Dict<'_>>(ctx)?;

        if dict.contains_key(F) {
            warn!("encountered stream referencing external file, which is unsupported");

            return None;
        }

        let stream_byte_limit = ctx.load_limits().stream_byte_limit().unwrap_or(u64::MAX);
        let offset = r.offset();
        parse_proper(r, &dict, stream_byte_limit)
            .or_else(|| {
                warn!("failed to parse stream, trying to parse it manually");
                crate::leniency::emit(crate::leniency::STREAM_PARSE_FALLBACK);
                r.jump(offset);
                parse_fallback(r, &dict, stream_byte_limit)
            })
            .error_none("was unable to manually parse the stream")
    }
}

#[derive(Debug, Copy, Clone)]
/// A failure that can occur during decoding a data stream.
pub enum DecodeFailure {
    /// An image stream failed to decode.
    ImageDecode,
    /// A data stream failed to decode.
    StreamDecode,
    /// A failure occurred while decrypting a file.
    Decryption,
    /// An unknown failure occurred.
    Unknown,
    /// The decoded stream exceeds the configured
    /// [`PdfLoadLimits::max_stream_bytes`](crate::pdf::PdfLoadLimits) limit.
    ///
    /// Unlike the other variants, this is a **hard stop** — callers must not
    /// silently discard it with `.ok()`. Propagate it as
    /// `LimitError::StreamTooLarge` / `Error::ResourceLimitExceeded`.
    StreamTooLarge {
        /// Decoded byte count that triggered the limit.
        observed: u64,
        /// The configured limit.
        limit: u64,
    },
}

/// An image color space.
#[derive(Debug, Copy, Clone)]
pub enum ImageColorSpace {
    /// Grayscale color space.
    Gray,
    /// RGB color space.
    Rgb,
    /// RGB produced by JPEG YCbCr→RGB decoding.
    ///
    /// JPEG images stored with YCbCr encoding (Adobe APP14 transform=1 or
    /// JFIF default) are converted to sRGB by the JPEG decoder using the
    /// standard BT.601 matrix. The resulting RGB values are already in sRGB
    /// colorimetry. Any PDF `/ColorSpace` entry that is not device-RGB (e.g.
    /// an [`ICCBased`] printer profile) should be ignored for these images;
    /// the JPEG decoder's own colour model takes precedence, matching MuPDF
    /// and Acrobat behaviour.
    RgbFromYCbCr,
    /// CMYK color space.
    Cmyk,
    /// An unknown color space.
    Unknown(u8),
}

/// Additional data that is extracted from some image streams.
pub struct ImageData {
    /// An optional alpha channel of the image.
    pub alpha: Option<Vec<u8>>,
    /// The color space of the image.
    pub color_space: Option<ImageColorSpace>,
    /// The bits per component of the image.
    pub bits_per_component: u8,
    /// The width of the image.
    pub width: u32,
    /// The height of the image.
    pub height: u32,
}

/// The result of applying a filter.
pub struct FilterResult {
    /// The decoded data.
    pub data: Vec<u8>,
    /// Additional data that is extracted from JPX image streams.
    pub image_data: Option<ImageData>,
}

impl FilterResult {
    pub(crate) fn from_data(data: Vec<u8>) -> Self {
        Self {
            data,
            image_data: None,
        }
    }
}

fn parse_proper<'a>(
    r: &mut Reader<'a>,
    dict: &Dict<'a>,
    stream_byte_limit: u64,
) -> Option<Stream<'a>> {
    let length = dict.get::<u32>(LENGTH)?;

    r.skip_white_spaces_and_comments();
    r.forward_tag(b"stream")?;
    // Skip horizontal whitespace (spaces/tabs) between "stream" keyword and EOL.
    // Some producers write "stream \r\n" (with a trailing space) which is technically
    // non-conforming but tolerated by Acrobat and MuPDF.
    while r.peek_byte().is_some_and(|b| b == b' ' || b == b'\t') {
        r.forward();
    }
    r.forward_tag(b"\n")
        .or_else(|| r.forward_tag(b"\r\n"))
        .or_else(|| r.forward_tag(b"\r"))?;
    let data = r.read_bytes(length as usize)?;
    r.skip_white_spaces();
    r.forward_tag(b"endstream")?;

    Some(Stream::new(data, dict.clone(), stream_byte_limit))
}

fn parse_fallback<'a>(
    r: &mut Reader<'a>,
    dict: &Dict<'a>,
    stream_byte_limit: u64,
) -> Option<Stream<'a>> {
    while r.forward_tag(b"stream").is_none() {
        r.read_byte()?;
    }

    // Skip any horizontal whitespace between "stream" keyword and EOL (same lenience as
    // parse_proper — some producers write "stream \r\n").
    while r.peek_byte().is_some_and(|b| b == b' ' || b == b'\t') {
        r.forward();
    }
    r.forward_tag(b"\n")
        .or_else(|| r.forward_tag(b"\r\n"))
        // Technically not allowed, but no reason to not try it.
        .or_else(|| r.forward_tag(b"\r"))?;

    let data_start = r.tail()?;
    let start = r.offset();

    loop {
        if r.peek_byte()?.is_ascii_whitespace() || r.peek_tag(b"endstream").is_some() {
            let length = r.offset() - start;
            let data = data_start.get(..length)?;

            r.skip_white_spaces();

            // This was just a whitespace in the data stream but not actually marking the end
            // of the stream, so continue searching.
            if r.forward_tag(b"endstream").is_none() {
                continue;
            }

            let stream = Stream::new(data, dict.clone(), stream_byte_limit);

            // Seems like we found the end!
            return Some(stream);
        } else {
            r.read_byte()?;
        }
    }
}

impl<'a> TryFrom<Object<'a>> for Stream<'a> {
    type Error = ();

    fn try_from(value: Object<'a>) -> Result<Self, Self::Error> {
        match value {
            Object::Stream(s) => Ok(s),
            _ => Err(()),
        }
    }
}

impl<'a> ObjectLike<'a> for Stream<'a> {}

#[cfg(test)]
mod tests {
    use crate::object::Stream;
    use crate::pdf::PdfLoadLimits;
    use crate::reader::Reader;
    use crate::reader::{ReaderContext, ReaderExt};

    use super::DecodeFailure;

    #[test]
    fn stream() {
        let data = b"<< /Length 10 >> stream\nabcdefghij\nendstream";
        let mut r = Reader::new(data);
        let stream = r
            .read_with_context::<Stream<'_>>(&ReaderContext::dummy())
            .unwrap();

        assert_eq!(stream.0.data, b"abcdefghij");
    }

    /// `decoded()` on an unfiltered stream succeeds when no byte limit is set.
    #[test]
    fn decoded_no_limit() {
        let data = b"<< /Length 5 >> stream\nhello\nendstream";
        let mut r = Reader::new(data);
        let stream = r
            .read_with_context::<Stream<'_>>(&ReaderContext::dummy())
            .unwrap();

        let decoded = stream.decoded().unwrap();
        assert_eq!(decoded, b"hello");
    }

    /// `decoded()` returns `StreamTooLarge` when the decoded size exceeds the configured limit.
    #[test]
    fn decoded_exceeds_byte_limit() {
        // 10-byte payload, limit of 5 bytes → should fail.
        let data = b"<< /Length 10 >> stream\nabcdefghij\nendstream";
        let limits = PdfLoadLimits::new().max_stream_bytes(5);
        let ctx = ReaderContext::dummy_with_limits(limits);
        let mut r = Reader::new(data);
        let stream = r.read_with_context::<Stream<'_>>(&ctx).unwrap();

        match stream.decoded() {
            Err(DecodeFailure::StreamTooLarge { observed, limit }) => {
                assert_eq!(observed, 10);
                assert_eq!(limit, 5);
            }
            other => panic!("expected StreamTooLarge, got {other:?}"),
        }
    }

    /// `decoded()` succeeds when the payload exactly equals the byte limit.
    #[test]
    fn decoded_at_byte_limit_succeeds() {
        let data = b"<< /Length 10 >> stream\nabcdefghij\nendstream";
        let limits = PdfLoadLimits::new().max_stream_bytes(10);
        let ctx = ReaderContext::dummy_with_limits(limits);
        let mut r = Reader::new(data);
        let stream = r.read_with_context::<Stream<'_>>(&ctx).unwrap();

        let decoded = stream.decoded().unwrap();
        assert_eq!(decoded, b"abcdefghij");
    }

    /// FlateDecode: a 12-byte compressed payload that expands to 50 bytes must
    /// be rejected when the configured limit is below the expanded size.
    ///
    /// Enforcement happens in decoded_image() after the filter chain: the
    /// decompressor runs to completion (up to MAX_DECODE_SIZE) and the
    /// resulting size is compared against the configured limit.
    #[test]
    fn flate_decode_exceeds_limit() {
        // zlib-compressed 50 zero bytes (12-byte compressed form).
        let compressed: &[u8] = &[
            0x78, 0x9c, 0x63, 0x60, 0x20, 0x15, 0x00, 0x00, 0x00, 0x32, 0x00, 0x01,
        ];
        let length = compressed.len();
        let mut pdf = format!("<< /Length {length} /Filter /FlateDecode >> stream\n").into_bytes();
        pdf.extend_from_slice(compressed);
        pdf.extend_from_slice(b"\nendstream");

        let limits = PdfLoadLimits::new().max_stream_bytes(20); // 50 > 20 → must fail
        let ctx = ReaderContext::dummy_with_limits(limits);
        let mut r = Reader::new(&pdf);
        let stream = r.read_with_context::<Stream<'_>>(&ctx).unwrap();

        match stream.decoded() {
            Err(DecodeFailure::StreamTooLarge { .. }) => {}
            other => panic!("expected StreamTooLarge, got {other:?}"),
        }
    }

    /// FlateDecode: same stream succeeds when the limit is at or above the
    /// expanded size (50 bytes).
    #[test]
    fn flate_decode_within_limit() {
        let compressed: &[u8] = &[
            0x78, 0x9c, 0x63, 0x60, 0x20, 0x15, 0x00, 0x00, 0x00, 0x32, 0x00, 0x01,
        ];
        let length = compressed.len();
        let mut pdf = format!("<< /Length {length} /Filter /FlateDecode >> stream\n").into_bytes();
        pdf.extend_from_slice(compressed);
        pdf.extend_from_slice(b"\nendstream");

        let limits = PdfLoadLimits::new().max_stream_bytes(50);
        let ctx = ReaderContext::dummy_with_limits(limits);
        let mut r = Reader::new(&pdf);
        let stream = r.read_with_context::<Stream<'_>>(&ctx).unwrap();

        let decoded = stream.decoded().unwrap();
        assert_eq!(decoded.len(), 50);
        assert!(decoded.iter().all(|&b| b == 0));
    }

    /// FlateDecode: limit exactly one byte below the expanded size must fail.
    #[test]
    fn flate_decode_limit_one_below() {
        let compressed: &[u8] = &[
            0x78, 0x9c, 0x63, 0x60, 0x20, 0x15, 0x00, 0x00, 0x00, 0x32, 0x00, 0x01,
        ];
        let length = compressed.len();
        let mut pdf = format!("<< /Length {length} /Filter /FlateDecode >> stream\n").into_bytes();
        pdf.extend_from_slice(compressed);
        pdf.extend_from_slice(b"\nendstream");

        let limits = PdfLoadLimits::new().max_stream_bytes(49); // 50 > 49 → must fail
        let ctx = ReaderContext::dummy_with_limits(limits);
        let mut r = Reader::new(&pdf);
        let stream = r.read_with_context::<Stream<'_>>(&ctx).unwrap();

        match stream.decoded() {
            Err(DecodeFailure::StreamTooLarge { .. }) => {}
            other => panic!("expected StreamTooLarge, got {other:?}"),
        }
    }

    /// The wiring, not the check: does the limit reach the filter at all?
    ///
    /// The CCITT decoder used to read the limit from its own `/DecodeParms`
    /// dictionary. When a stream has none that is `Dict::default()`, whose
    /// context carries no limits -- so the check existed, its unit tests passed,
    /// and it was unreachable. `Stream::decoded_image` now puts the limit on
    /// `ImageDecodeParams`, where every filter can see it. (Codex, #1609.)
    ///
    /// `/DecodeParms` is present here on purpose. The absent case cannot
    /// discriminate: `/Columns` then defaults to 1728 and two bytes of data
    /// never complete a row, so nothing is produced either way. With
    /// `/Columns 8` the row completes, and the limit either stops it or does not.
    #[test]
    fn the_pixel_limit_reaches_the_filter_that_needs_it() {
        use super::ImageDecodeParams;

        // One row of eight white pixels, Group 3 one-dimensional.
        let data = b"<< /Length 2 /Filter /CCITTFaxDecode /DecodeParms << /K 0 /Columns 8 /Rows 1 >> >> stream\n\x35\x14\nendstream";
        let limits = PdfLoadLimits::new().max_image_pixels(4);
        let mut r = Reader::new(data);
        let stream = r
            .read_with_context::<Stream<'_>>(&ReaderContext::dummy_with_limits(limits))
            .unwrap();

        // Declared 1x1, so the check before decoding is satisfied; the CCITT
        // parameters ask for 8 pixels against a limit of 4.
        let params = ImageDecodeParams {
            width: 1,
            height: 1,
            ..Default::default()
        };

        let produced = stream
            .decoded_image(&params)
            .ok()
            .and_then(|res| res.image_data)
            .map(|i| u64::from(i.width) * u64::from(i.height));
        assert!(
            produced.is_none_or(|pixels| pixels <= 4),
            "produced {produced:?} pixels under a limit of 4"
        );
    }
}
