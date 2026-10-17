//! Image export for a user-facing tool: every image comes out right, or it is
//! reported as skipped with the reason.
//!
//! [`crate::images::encode_image_for_document`] was written for embedding in
//! office packages and reads only the stream's filter, first colour-space name
//! and bit depth. That is not enough to produce a faithful picture, and three
//! classes came out silently wrong (#616):
//!
//! - an `/Indexed` image at 8 bits per component was written as greyscale, so
//!   the palette *indices* came out as grey levels;
//! - `/SMask`, `/Mask` and `/Decode` were ignored, so transparency was lost and
//!   inverted images stayed inverted;
//! - an Adobe CMYK JPEG was passed through unchanged, which browsers show with
//!   inverted colours.
//!
//! A download button that hands out a wrong picture is worse than one that says
//! "this one we could not extract". So [`export_images`] expands palettes (small
//! and exact), and skips everything else it cannot reproduce faithfully, naming
//! why in [`SkipReason`] -- including CMYK and non-sRGB ICC images, whose
//! colours would shift without a colour-managed conversion.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::images::EncodedImage;

/// Cap on the decoded size of one image stream (64 MB), as in `images.rs`.
const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// An image that was exported as a faithful PNG or JPEG.
#[derive(Debug, Clone)]
pub struct ExportedImage {
    /// The PDF object ID of the image stream.
    pub object_id: ObjectId,
    /// First page (1-based) on which the image is used.
    pub page: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The encoded image.
    pub image: EncodedImage,
}

/// An image that was left out because it could not be exported faithfully.
#[derive(Debug, Clone)]
pub struct SkippedImage {
    /// The PDF object ID of the image stream.
    pub object_id: ObjectId,
    /// First page (1-based) on which the image is used.
    pub page: u32,
    /// Width in pixels, as declared (0 if absent).
    pub width: u32,
    /// Height in pixels, as declared (0 if absent).
    pub height: u32,
    /// Why it was skipped.
    pub reason: SkipReason,
}

/// Why an image was skipped rather than exported.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// A compression this exporter does not decode (JPEG 2000, JBIG2, CCITT
    /// fax, a TIFF predictor, or an unknown filter). Carries the filter name.
    UnsupportedFilter(String),
    /// The image has an `/SMask` or `/Mask`: its transparency would be lost.
    Transparency,
    /// The image has a non-default `/Decode` array (for example an inverted
    /// image), which a plain export would not apply.
    DecodeArray,
    /// A four-component (CMYK) JPEG. Browsers typically show Adobe CMYK
    /// JPEGs with inverted colours.
    CmykJpeg,
    /// A colour space this exporter does not reproduce: Lab, Separation,
    /// DeviceN, CMYK, an ICC profile other than sRGB. Carries the name.
    ColorSpace(String),
    /// A bit depth this exporter does not reproduce for this colour space.
    BitDepth(u32),
    /// A stencil mask (`/ImageMask true`): it has no colours of its own.
    ImageMask,
    /// The stream could not be decoded, or holds fewer samples than its
    /// declared geometry needs.
    Undecodable,
}

impl SkipReason {
    /// A stable machine-readable code for the reason.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedFilter(_) => "unsupported_filter",
            Self::Transparency => "transparency",
            Self::DecodeArray => "decode_array",
            Self::CmykJpeg => "cmyk_jpeg",
            Self::ColorSpace(_) => "color_space",
            Self::BitDepth(_) => "bit_depth",
            Self::ImageMask => "image_mask",
            Self::Undecodable => "undecodable",
        }
    }

    /// A human-readable explanation.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::UnsupportedFilter(f) => format!("compression {f} is not supported"),
            Self::Transparency => "image has a transparency mask (SMask/Mask)".into(),
            Self::DecodeArray => "image has a non-default Decode array".into(),
            Self::CmykJpeg => "CMYK JPEG would show with wrong colours".into(),
            Self::ColorSpace(cs) => format!("colour space {cs} is not supported"),
            Self::BitDepth(b) => format!("{b} bits per component is not supported here"),
            Self::ImageMask => "stencil mask has no colours of its own".into(),
            Self::Undecodable => "image data could not be decoded".into(),
        }
    }
}

/// The result of [`export_images`].
#[derive(Debug, Clone, Default)]
pub struct ImageExport {
    /// Images exported faithfully, in page order.
    pub images: Vec<ExportedImage>,
    /// Images left out, with the reason, in page order.
    pub skipped: Vec<SkippedImage>,
}

/// Export every image XObject on every page, each once, as PNG or JPEG.
///
/// An image used on several pages is reported once, on the first. An image
/// that cannot be reproduced faithfully lands in [`ImageExport::skipped`]
/// rather than being emitted wrong.
#[must_use]
pub fn export_images(doc: &Document) -> ImageExport {
    let mut out = ImageExport::default();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    for (page, page_id) in doc.get_pages() {
        let mut ids = Vec::new();
        if let Some(resources) = page_resources(doc, page_id) {
            collect_images(doc, resources, &mut ids, &mut HashSet::new(), 0);
        }
        for id in ids {
            if !seen.insert(id) {
                continue;
            }
            let Ok(Object::Stream(stream)) = doc.get_object(id) else {
                continue;
            };
            let width = int(doc, &stream.dict, b"Width").unwrap_or(0).max(0) as u32;
            let height = int(doc, &stream.dict, b"Height").unwrap_or(0).max(0) as u32;
            match export_one(doc, stream, width, height) {
                Ok(image) => out.images.push(ExportedImage {
                    object_id: id,
                    page,
                    width,
                    height,
                    image,
                }),
                Err(reason) => out.skipped.push(SkippedImage {
                    object_id: id,
                    page,
                    width,
                    height,
                    reason,
                }),
            }
        }
    }
    out
}

/// What the samples of an image mean.
#[derive(Debug, Clone, PartialEq)]
enum ColorModel {
    Gray,
    Rgb,
    Cmyk,
    Indexed {
        base: Box<ColorModel>,
        hival: usize,
        lookup: Vec<u8>,
    },
    /// An ICC space whose samples are known (so a Decode array can be judged)
    /// but whose colours cannot be written out unconverted.
    IccUnsupported(String),
    Unsupported(String),
}

impl ColorModel {
    fn components(&self) -> usize {
        match self {
            Self::Gray | Self::Indexed { .. } => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
            Self::Unsupported(_) | Self::IccUnsupported(_) => 0,
        }
    }
}

fn export_one(
    doc: &Document,
    stream: &Stream,
    width: u32,
    height: u32,
) -> Result<EncodedImage, SkipReason> {
    let dict = &stream.dict;
    if matches!(
        resolve(doc, dict.get(b"ImageMask").ok()),
        Some(Object::Boolean(true))
    ) {
        return Err(SkipReason::ImageMask);
    }
    if dict.has(b"SMask") || dict.has(b"Mask") {
        return Err(SkipReason::Transparency);
    }
    if width == 0 || height == 0 {
        return Err(SkipReason::Undecodable);
    }

    let filters = filter_names(doc, dict);
    for f in &filters {
        match f.as_str() {
            "FlateDecode" | "Fl" | "LZWDecode" | "LZW" | "ASCII85Decode" | "A85"
            | "ASCIIHexDecode" | "AHx" | "RunLengthDecode" | "RL" => {}
            "DCTDecode" | "DCT" => {}
            other => return Err(SkipReason::UnsupportedFilter(other.to_string())),
        }
    }
    let is_jpeg = filters
        .last()
        .is_some_and(|f| f == "DCTDecode" || f == "DCT");
    if filters[..filters.len().saturating_sub(1)]
        .iter()
        .any(|f| f == "DCTDecode" || f == "DCT")
    {
        return Err(SkipReason::UnsupportedFilter("DCTDecode".into()));
    }
    if let Some(reason) = unsupported_decode_parms(doc, dict) {
        return Err(reason);
    }

    let model = match resolve(doc, dict.get(b"ColorSpace").ok()) {
        Some(cs) => color_model(doc, cs),
        None => ColorModel::Unsupported("none".into()),
    };
    let bpc = int(doc, dict, b"BitsPerComponent").unwrap_or(0);

    if let Some(decode) = resolve(doc, dict.get(b"Decode").ok()) {
        if !is_default_decode(decode, &model, bpc) {
            return Err(SkipReason::DecodeArray);
        }
    }

    let data = stream
        .decompressed_content_with_limit(MAX_DECODED_BYTES)
        .map_err(|_| SkipReason::Undecodable)?;

    if is_jpeg {
        match &model {
            ColorModel::Gray | ColorModel::Rgb | ColorModel::Cmyk => {}
            ColorModel::Indexed { .. } => return Err(SkipReason::ColorSpace("Indexed".into())),
            ColorModel::IccUnsupported(name) => return Err(SkipReason::ColorSpace(name.clone())),
            ColorModel::Unsupported(name) => return Err(SkipReason::ColorSpace(name.clone())),
        }
        if !data.starts_with(&[0xFF, 0xD8]) {
            return Err(SkipReason::Undecodable);
        }
        return match jpeg_components(&data) {
            Some(4) => Err(SkipReason::CmykJpeg),
            Some(1 | 3) => Ok(EncodedImage {
                data,
                mime: "image/jpeg",
                ext: "jpeg",
            }),
            _ => Err(SkipReason::Undecodable),
        };
    }

    let (w, h) = (width as usize, height as usize);
    let (samples, color) = match &model {
        ColorModel::Unsupported(name) | ColorModel::IccUnsupported(name) => {
            return Err(SkipReason::ColorSpace(name.clone()))
        }
        // A naive CMYK-to-RGB conversion is visibly off without the press
        // profile; skipped like the CMYK JPEG rather than emitted wrong.
        ColorModel::Cmyk => return Err(SkipReason::ColorSpace("DeviceCMYK".into())),
        ColorModel::Indexed {
            base,
            hival,
            lookup,
        } => {
            let indices = unpack(&data, w, h, bpc).ok_or(SkipReason::Undecodable)?;
            expand_palette(&indices, base, *hival, lookup)?
        }
        ColorModel::Gray => {
            let gray = if bpc == 8 {
                let n = w.checked_mul(h).ok_or(SkipReason::Undecodable)?;
                data.get(..n).ok_or(SkipReason::Undecodable)?.to_vec()
            } else {
                let raw = unpack(&data, w, h, bpc).ok_or(SkipReason::BitDepth(bpc as u32))?;
                let max = (1u32 << bpc) - 1;
                raw.iter()
                    .map(|&v| (u32::from(v) * 255 / max) as u8)
                    .collect()
            };
            (gray, image::ExtendedColorType::L8)
        }
        ColorModel::Rgb => {
            if bpc != 8 {
                return Err(SkipReason::BitDepth(bpc.max(0) as u32));
            }
            let n = w
                .checked_mul(h)
                .and_then(|p| p.checked_mul(model.components()))
                .ok_or(SkipReason::Undecodable)?;
            let samples = data.get(..n).ok_or(SkipReason::Undecodable)?;
            (samples.to_vec(), image::ExtendedColorType::Rgb8)
        }
    };
    encode_png(&samples, width, height, color).ok_or(SkipReason::Undecodable)
}

fn resolve<'a>(doc: &'a Document, obj: Option<&'a Object>) -> Option<&'a Object> {
    match obj? {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

fn int(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<i64> {
    match resolve(doc, dict.get(key).ok())? {
        Object::Integer(i) => Some(*i),
        Object::Real(r) => Some(*r as i64),
        _ => None,
    }
}

fn number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

fn filter_names(doc: &Document, dict: &Dictionary) -> Vec<String> {
    match resolve(doc, dict.get(b"Filter").ok()) {
        Some(Object::Name(n)) => vec![String::from_utf8_lossy(n).into_owned()],
        Some(Object::Array(arr)) => arr
            .iter()
            .map(|o| match resolve(doc, Some(o)) {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
                _ => "unknown".into(),
            })
            .collect(),
        Some(_) => vec!["unknown".into()],
        None => Vec::new(),
    }
}

/// Decode parameters lopdf would not apply, leaving predictor bytes among the
/// samples and scrambling the picture.
///
/// lopdf reads `/DecodeParms` only as a dictionary, and within it undoes the
/// PNG predictors (10-15) but not the TIFF one (2). An array -- one entry per
/// filter in a chain such as `[/ASCII85Decode /FlateDecode]` -- is not read at
/// all, so a predictor in it would stay in the data (codex, #1963).
fn unsupported_decode_parms(doc: &Document, dict: &Dictionary) -> Option<SkipReason> {
    let predictor = |p: &Object| match resolve(doc, Some(p)) {
        Some(Object::Dictionary(d)) => int(doc, d, b"Predictor").unwrap_or(1),
        _ => 1,
    };
    match resolve(doc, dict.get(b"DecodeParms").ok())? {
        Object::Array(arr) => arr
            .iter()
            .any(|p| predictor(p) > 1)
            .then(|| SkipReason::UnsupportedFilter("predictor in a filter chain".into())),
        single => {
            (predictor(single) == 2).then(|| SkipReason::UnsupportedFilter("TIFF predictor".into()))
        }
    }
}

/// The resources a page draws with: its own, or the nearest ancestor's in the
/// page tree, since `/Resources` is inherited (ISO 32000-1 §7.7.3.4).
fn page_resources(doc: &Document, page_id: ObjectId) -> Option<&Dictionary> {
    let mut node_id = page_id;
    for _ in 0..64 {
        let node = doc.get_dictionary(node_id).ok()?;
        if let Some(Object::Dictionary(d)) = resolve(doc, node.get(b"Resources").ok()) {
            return Some(d);
        }
        node_id = node.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Image XObjects reachable from `resources`, including those drawn inside
/// Form XObjects -- where logos commonly sit (codex, #1963).
fn collect_images(
    doc: &Document,
    resources: &Dictionary,
    out: &mut Vec<ObjectId>,
    forms: &mut HashSet<ObjectId>,
    depth: usize,
) {
    let Some(Object::Dictionary(xobjects)) = resolve(doc, resources.get(b"XObject").ok()) else {
        return;
    };
    for (_, obj) in xobjects.iter() {
        let Object::Reference(id) = obj else {
            continue;
        };
        let Ok(Object::Stream(stream)) = doc.get_object(*id) else {
            continue;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => {
                if !out.contains(id) {
                    out.push(*id);
                }
            }
            // A form without resources of its own draws with the page's, which
            // are already being walked.
            Ok(b"Form") if depth < 32 && forms.insert(*id) => {
                if let Some(Object::Dictionary(inner)) =
                    resolve(doc, stream.dict.get(b"Resources").ok())
                {
                    collect_images(doc, inner, out, forms, depth + 1);
                }
            }
            _ => {}
        }
    }
}

fn color_model(doc: &Document, cs: &Object) -> ColorModel {
    match cs {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"G" | b"CalGray" => ColorModel::Gray,
            b"DeviceRGB" | b"RGB" | b"CalRGB" => ColorModel::Rgb,
            b"DeviceCMYK" | b"CMYK" => ColorModel::Cmyk,
            other => ColorModel::Unsupported(String::from_utf8_lossy(other).into_owned()),
        },
        Object::Array(arr) => {
            let family = match arr.first().and_then(|o| resolve(doc, Some(o))) {
                Some(Object::Name(n)) => n.clone(),
                _ => return ColorModel::Unsupported("unknown".into()),
            };
            match family.as_slice() {
                b"CalGray" => ColorModel::Gray,
                b"CalRGB" => ColorModel::Rgb,
                b"ICCBased" => match arr.get(1).and_then(|o| resolve(doc, Some(o))) {
                    Some(Object::Stream(s)) => icc_model(doc, s),
                    _ => ColorModel::Unsupported("ICCBased".into()),
                },
                b"Indexed" | b"I" => indexed_model(doc, arr),
                other => ColorModel::Unsupported(String::from_utf8_lossy(other).into_owned()),
            }
        }
        _ => ColorModel::Unsupported("unknown".into()),
    }
}

fn indexed_model(doc: &Document, arr: &[Object]) -> ColorModel {
    let unsupported = || ColorModel::Unsupported("Indexed".into());
    let Some(base) = arr.get(1).and_then(|o| resolve(doc, Some(o))) else {
        return unsupported();
    };
    let base = color_model(doc, base);
    if !matches!(base, ColorModel::Gray | ColorModel::Rgb) {
        return match base {
            ColorModel::Unsupported(name) | ColorModel::IccUnsupported(name) => {
                ColorModel::Unsupported(format!("Indexed/{name}"))
            }
            ColorModel::Cmyk => ColorModel::Unsupported("Indexed/DeviceCMYK".into()),
            _ => unsupported(),
        };
    }
    let Some(hival) = arr
        .get(2)
        .and_then(|o| resolve(doc, Some(o)))
        .and_then(number)
        .filter(|h| (0.0..=255.0).contains(h))
    else {
        return unsupported();
    };
    let lookup = match arr.get(3).and_then(|o| resolve(doc, Some(o))) {
        Some(Object::String(bytes, _)) => bytes.clone(),
        Some(Object::Stream(s)) => match s.decompressed_content_with_limit(MAX_DECODED_BYTES) {
            Ok(b) => b,
            Err(_) => return unsupported(),
        },
        _ => return unsupported(),
    };
    ColorModel::Indexed {
        base: Box::new(base),
        hival: hival as usize,
        lookup,
    }
}

/// Whether `/Decode` is the identity mapping, which changes nothing.
fn is_default_decode(decode: &Object, model: &ColorModel, bpc: i64) -> bool {
    let Ok(arr) = decode.as_array() else {
        return false;
    };
    let values: Option<Vec<f64>> = arr.iter().map(number).collect();
    let Some(values) = values else {
        return false;
    };
    let expected: Vec<f64> = match model {
        ColorModel::Indexed { .. } => {
            if !(1..=8).contains(&bpc) {
                return false;
            }
            vec![0.0, f64::from((1u32 << bpc) - 1)]
        }
        ColorModel::Unsupported(_) | ColorModel::IccUnsupported(_) => return false,
        m => [0.0, 1.0].repeat(m.components()),
    };
    values.len() == expected.len()
        && values
            .iter()
            .zip(&expected)
            .all(|(a, b)| (a - b).abs() < 1e-6)
}

/// Unpack `w * h` samples of `bpc` bits (1, 2, 4 or 8), one component per
/// pixel. Rows start on a byte boundary (ISO 32000-1 §8.9.3).
fn unpack(data: &[u8], w: usize, h: usize, bpc: i64) -> Option<Vec<u8>> {
    if !matches!(bpc, 1 | 2 | 4 | 8) {
        return None;
    }
    let bpc = bpc as usize;
    let row_bytes = w.checked_mul(bpc)?.div_ceil(8);
    if data.len() < row_bytes.checked_mul(h)? {
        return None;
    }
    let mut out = Vec::with_capacity(w.checked_mul(h)?);
    let mask = ((1u16 << bpc) - 1) as u8;
    for row in data.chunks(row_bytes).take(h) {
        for x in 0..w {
            let bit = x * bpc;
            let byte = row[bit / 8];
            let shift = 8 - bpc - (bit % 8);
            out.push((byte >> shift) & mask);
        }
    }
    Some(out)
}

fn expand_palette(
    indices: &[u8],
    base: &ColorModel,
    hival: usize,
    lookup: &[u8],
) -> Result<(Vec<u8>, image::ExtendedColorType), SkipReason> {
    let n = base.components();
    if n == 0 || lookup.len() < (hival + 1) * n {
        return Err(SkipReason::Undecodable);
    }
    let mut out = Vec::with_capacity(indices.len() * n);
    for &i in indices {
        // Out-of-range indices are clamped (ISO 32000-1 §8.6.6.3).
        let i = usize::from(i).min(hival);
        out.extend_from_slice(&lookup[i * n..i * n + n]);
    }
    Ok(match base {
        ColorModel::Gray => (out, image::ExtendedColorType::L8),
        _ => (out, image::ExtendedColorType::Rgb8),
    })
}

/// An ICC-based colour space, as far as it can be written out unconverted.
///
/// A PNG or JPEG without a profile is read as sRGB. So an RGB profile is kept
/// only when it IS sRGB, found by the name in its description; any other RGB
/// profile (Adobe RGB, ProPhoto, a camera's) would shift every colour, and
/// CMYK needs the press profile applied (codex, #1963). A grey profile differs
/// from the default only in its tone curve and is kept.
fn icc_model(doc: &Document, profile: &Stream) -> ColorModel {
    match int(doc, &profile.dict, b"N") {
        Some(1) => ColorModel::Gray,
        Some(3) => {
            let bytes = profile
                .decompressed_content_with_limit(MAX_DECODED_BYTES)
                .unwrap_or_default();
            if bytes.windows(4).any(|w| w == b"sRGB") {
                ColorModel::Rgb
            } else {
                ColorModel::IccUnsupported("ICCBased (non-sRGB RGB profile)".into())
            }
        }
        Some(4) => ColorModel::IccUnsupported("ICCBased (CMYK)".into()),
        _ => ColorModel::Unsupported("ICCBased".into()),
    }
}

/// The number of colour components in a JPEG's frame header, or `None` if no
/// frame header is found before the scan data.
fn jpeg_components(data: &[u8]) -> Option<u8> {
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            return None;
        }
        let marker = data[pos + 1];
        if marker == 0xFF {
            pos += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            pos += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([data[pos + 2], data[pos + 3]]));
        // SOF0..SOF15, less DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return data.get(pos + 9).copied();
        }
        if marker == 0xDA {
            return None;
        }
        pos += 2 + len;
    }
    None
}

fn encode_png(
    samples: &[u8],
    width: u32,
    height: u32,
    color: image::ExtendedColorType,
) -> Option<EncodedImage> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;

    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
        .write_image(samples, width, height, color)
        .ok()?;
    Some(EncodedImage {
        data: out,
        mime: "image/png",
        ext: "png",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, StringFormat};
    use std::io::Write;

    /// A one-page document whose resources name each image as /Im0, /Im1, ...
    fn doc_with(images: Vec<Stream>) -> Document {
        let mut doc = Document::with_version("1.7");
        let mut xobjects = Dictionary::new();
        for (i, img) in images.into_iter().enumerate() {
            let id = doc.add_object(Object::Stream(img));
            xobjects.set(format!("Im{i}"), Object::Reference(id));
        }
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => dictionary! { "XObject" => xobjects },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    fn image(w: i64, h: i64, cs: Object, bpc: i64, data: Vec<u8>) -> Stream {
        Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => w,
                "Height" => h,
                "ColorSpace" => cs,
                "BitsPerComponent" => bpc,
            },
            data,
        )
    }

    fn only_skip(doc: &Document) -> SkipReason {
        let out = export_images(doc);
        assert!(out.images.is_empty(), "expected a skip, got an export");
        assert_eq!(out.skipped.len(), 1);
        out.skipped[0].reason.clone()
    }

    fn only_png(doc: &Document) -> image::DynamicImage {
        let out = export_images(doc);
        assert!(out.skipped.is_empty(), "unexpected skip: {:?}", out.skipped);
        assert_eq!(out.images.len(), 1);
        assert_eq!(out.images[0].image.mime, "image/png");
        image::load_from_memory(&out.images[0].image.data).expect("a real PNG")
    }

    /// A header-only JPEG: SOI, then a SOF0 frame header with `n` components.
    fn jpeg_header(n: u8) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 8 + 3 * n, 8, 0, 1, 0, 1, n]);
        for c in 0..n {
            v.extend_from_slice(&[c + 1, 0x11, 0]);
        }
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn an_indexed_image_comes_out_in_its_palette_colours_not_grey() {
        // Two pixels: index 0 = pure red, index 1 = pure blue.
        let cs = Object::Array(vec![
            "Indexed".into(),
            "DeviceRGB".into(),
            1.into(),
            Object::String(vec![255, 0, 0, 0, 0, 255], StringFormat::Hexadecimal),
        ]);
        let doc = doc_with(vec![image(2, 1, cs, 8, vec![0, 1])]);
        let png = only_png(&doc).to_rgb8();
        assert_eq!(png.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(png.get_pixel(1, 0).0, [0, 0, 255]);
    }

    #[test]
    fn a_four_bit_indexed_image_unpacks_its_indices() {
        let cs = Object::Array(vec![
            "Indexed".into(),
            "DeviceGray".into(),
            1.into(),
            Object::String(vec![10, 200], StringFormat::Literal),
        ]);
        // 3 pixels at 4 bits: indices 1, 0, 1 packed as 0x10 0x10.
        let doc = doc_with(vec![image(3, 1, cs, 4, vec![0x10, 0x10])]);
        let png = only_png(&doc).to_luma8();
        let px: Vec<u8> = png.pixels().map(|p| p.0[0]).collect();
        assert_eq!(px, vec![200, 10, 200]);
    }

    #[test]
    fn a_soft_masked_image_is_skipped_not_flattened() {
        let mut img = image(1, 1, "DeviceRGB".into(), 8, vec![1, 2, 3]);
        img.dict.set("SMask", Object::Reference((99, 0)));
        assert_eq!(only_skip(&doc_with(vec![img])), SkipReason::Transparency);
    }

    #[test]
    fn an_inverting_decode_array_is_skipped() {
        let mut img = image(1, 1, "DeviceGray".into(), 8, vec![0]);
        img.dict.set("Decode", vec![1.into(), 0.into()]);
        assert_eq!(only_skip(&doc_with(vec![img])), SkipReason::DecodeArray);
    }

    #[test]
    fn an_identity_decode_array_changes_nothing() {
        let mut img = image(1, 1, "DeviceGray".into(), 8, vec![77]);
        img.dict.set("Decode", vec![0.into(), 1.into()]);
        assert_eq!(
            only_png(&doc_with(vec![img])).to_luma8().get_pixel(0, 0).0,
            [77]
        );
    }

    #[test]
    fn a_cmyk_jpeg_is_skipped_and_an_rgb_jpeg_passes_through() {
        let mut cmyk = image(1, 1, "DeviceCMYK".into(), 8, jpeg_header(4));
        cmyk.dict.set("Filter", "DCTDecode");
        assert_eq!(only_skip(&doc_with(vec![cmyk])), SkipReason::CmykJpeg);

        let jpeg = jpeg_header(3);
        let mut rgb = image(1, 1, "DeviceRGB".into(), 8, jpeg.clone());
        rgb.dict.set("Filter", "DCTDecode");
        let out = export_images(&doc_with(vec![rgb]));
        assert_eq!(out.images.len(), 1);
        assert_eq!(out.images[0].image.mime, "image/jpeg");
        assert_eq!(out.images[0].image.data, jpeg);
    }

    #[test]
    fn jpeg_2000_is_skipped_with_its_filter_name() {
        let mut img = image(1, 1, "DeviceRGB".into(), 8, vec![0; 16]);
        img.dict.set("Filter", "JPXDecode");
        assert_eq!(
            only_skip(&doc_with(vec![img])),
            SkipReason::UnsupportedFilter("JPXDecode".into())
        );
    }

    #[test]
    fn a_png_predicted_flate_image_is_unpredicted() {
        // 2x1 RGB, PNG "None" predictor on each row: a filter byte, then samples.
        let raw = [0u8, 255, 0, 0, 0, 255, 0];
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).unwrap();
        let mut img = image(2, 1, "DeviceRGB".into(), 8, z.finish().unwrap());
        img.dict.set("Filter", "FlateDecode");
        img.dict.set(
            "DecodeParms",
            dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => 2 },
        );
        let png = only_png(&doc_with(vec![img])).to_rgb8();
        assert_eq!(png.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(png.get_pixel(1, 0).0, [0, 255, 0]);
    }

    #[test]
    fn a_lab_image_is_skipped_by_colour_space() {
        let cs = Object::Array(vec!["Lab".into(), Object::Dictionary(Dictionary::new())]);
        let img = image(1, 1, cs, 8, vec![0, 0, 0]);
        assert_eq!(
            only_skip(&doc_with(vec![img])),
            SkipReason::ColorSpace("Lab".into())
        );
    }

    #[test]
    fn a_short_stream_is_undecodable_not_a_truncated_picture() {
        let img = image(4, 4, "DeviceRGB".into(), 8, vec![0; 5]);
        assert_eq!(only_skip(&doc_with(vec![img])), SkipReason::Undecodable);
    }

    #[test]
    fn a_stencil_mask_is_skipped() {
        let mut img = image(8, 1, "DeviceGray".into(), 1, vec![0xAA]);
        img.dict.remove(b"ColorSpace");
        img.dict.set("ImageMask", true);
        assert_eq!(only_skip(&doc_with(vec![img])), SkipReason::ImageMask);
    }

    /// A one-page document whose image is reached some other way than as a
    /// direct entry in the page's own /Resources.
    fn doc_with_resources_on(parent: bool, via_form: bool, img: Stream) -> Document {
        let mut doc = Document::with_version("1.7");
        let img_id = doc.add_object(Object::Stream(img));
        let mut xobjects = dictionary! { "Im0" => img_id };
        if via_form {
            let form = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                    "Resources" => dictionary! { "XObject" => xobjects },
                },
                b"/Im0 Do".to_vec(),
            );
            let form_id = doc.add_object(Object::Stream(form));
            xobjects = dictionary! { "Fm0" => form_id };
        }
        let resources = dictionary! { "XObject" => xobjects };
        let pages_id = doc.new_object_id();
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        };
        let mut pages = dictionary! { "Type" => "Pages", "Count" => 1 };
        if parent {
            pages.set("Resources", resources);
        } else {
            page.set("Resources", resources);
        }
        let page_id = doc.add_object(page);
        pages.set("Kids", vec![page_id.into()]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    #[test]
    fn an_image_in_inherited_resources_is_found() {
        let img = image(1, 1, "DeviceGray".into(), 8, vec![9]);
        assert_eq!(
            export_images(&doc_with_resources_on(true, false, img))
                .images
                .len(),
            1
        );
    }

    #[test]
    fn an_image_drawn_inside_a_form_xobject_is_found() {
        let img = image(1, 1, "DeviceGray".into(), 8, vec![9]);
        assert_eq!(
            export_images(&doc_with_resources_on(false, true, img))
                .images
                .len(),
            1
        );
    }

    #[test]
    fn a_predictor_in_array_decode_parms_is_skipped_not_left_in_the_pixels() {
        let mut img = image(2, 1, "DeviceRGB".into(), 8, vec![0; 7]);
        img.dict.set(
            "DecodeParms",
            vec![Object::Dictionary(
                dictionary! { "Predictor" => 15, "Colors" => 3, "Columns" => 2 },
            )],
        );
        assert_eq!(
            only_skip(&doc_with(vec![img])),
            SkipReason::UnsupportedFilter("predictor in a filter chain".into())
        );
    }

    #[test]
    fn cmyk_and_non_srgb_icc_images_are_skipped_but_srgb_is_kept() {
        let raw_cmyk = image(1, 1, "DeviceCMYK".into(), 8, vec![0, 0, 0, 0]);
        assert!(matches!(
            only_skip(&doc_with(vec![raw_cmyk])),
            SkipReason::ColorSpace(_)
        ));

        let with = |profile: &[u8]| {
            let mut doc = doc_with(vec![image(1, 1, "DeviceRGB".into(), 8, vec![1, 2, 3])]);
            let pid = doc.add_object(Object::Stream(Stream::new(
                dictionary! { "N" => 3 },
                profile.to_vec(),
            )));
            for obj in doc.objects.values_mut() {
                if let Object::Stream(st) = obj {
                    if st.dict.has(b"Width") {
                        st.dict
                            .set("ColorSpace", vec!["ICCBased".into(), pid.into()]);
                    }
                }
            }
            doc
        };
        assert!(matches!(
            only_skip(&with(b"....Adobe RGB (1998)")),
            SkipReason::ColorSpace(_)
        ));
        assert_eq!(
            only_png(&with(b"....sRGB IEC61966-2.1"))
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [1, 2, 3]
        );
    }
}
