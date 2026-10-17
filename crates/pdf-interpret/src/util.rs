//! A number of utility methods.

use crate::{InterpreterWarning, WarningSinkFn};
use kurbo::{Affine, BezPath, PathEl, Rect};
use log::warn;
use pdf_syntax::object::Stream;
use pdf_syntax::object::stream::DecodeFailure;
use pdf_syntax::page::{Page, Rotation};
use siphasher::sip128::{Hasher128, SipHasher13};
use std::hash::Hash;
use std::ops::Sub;

pub(crate) fn decode_or_warn(stream: &Stream<'_>, sink: &WarningSinkFn) -> Option<Vec<u8>> {
    match stream.decoded() {
        Ok(data) => Some(data),
        Err(DecodeFailure::StreamTooLarge { observed, limit }) => {
            sink(InterpreterWarning::StreamTooLarge { observed, limit });
            None
        }
        Err(_) => None,
    }
}

pub(crate) trait OptionLog {
    fn warn_none(self, f: &str) -> Self;
}

impl<T> OptionLog for Option<T> {
    #[inline]
    fn warn_none(self, f: &str) -> Self {
        self.or_else(|| {
            warn!("{f}");

            None
        })
    }
}

const SCALAR_NEARLY_ZERO: f32 = 1.0 / (1 << 8) as f32;

/// A number of useful methods for f32 numbers.
pub trait Float32Ext: Sized + Sub<f32, Output = f32> + Copy + PartialOrd<f32> {
    /// Whether the number is approximately 0.
    fn is_nearly_zero(&self) -> bool {
        self.is_nearly_zero_within_tolerance(SCALAR_NEARLY_ZERO)
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_equal(&self, other: f32) -> bool {
        (*self - other).is_nearly_zero()
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_less_or_equal(&self, other: f32) -> bool {
        (*self - other).is_nearly_zero() || *self < other
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_greater_or_equal(&self, other: f32) -> bool {
        (*self - other).is_nearly_zero() || *self > other
    }

    /// Whether the number is approximately 0, with a given tolerance.
    fn is_nearly_zero_within_tolerance(&self, tolerance: f32) -> bool;
}

impl Float32Ext for f32 {
    fn is_nearly_zero_within_tolerance(&self, tolerance: f32) -> bool {
        debug_assert!(tolerance >= 0.0, "tolerance must be non-negative");

        self.abs() <= tolerance
    }
}

/// A number of useful methods for f64 numbers.
pub trait Float64Ext: Sized + Sub<f64, Output = f64> + Copy + PartialOrd<f64> {
    /// Whether the number is approximately 0.
    fn is_nearly_zero(&self) -> bool {
        self.is_nearly_zero_within_tolerance(SCALAR_NEARLY_ZERO as f64)
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_equal(&self, other: f64) -> bool {
        (*self - other).is_nearly_zero()
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_less_or_equal(&self, other: f64) -> bool {
        (*self - other).is_nearly_zero() || *self < other
    }

    /// Whether the number is nearly equal to another number.
    fn is_nearly_greater_or_equal(&self, other: f64) -> bool {
        (*self - other).is_nearly_zero() || *self > other
    }

    /// Whether the number is approximately 0, with a given tolerance.
    fn is_nearly_zero_within_tolerance(&self, tolerance: f64) -> bool;
}

impl Float64Ext for f64 {
    fn is_nearly_zero_within_tolerance(&self, tolerance: f64) -> bool {
        debug_assert!(tolerance >= 0.0, "tolerance must be non-negative");

        self.abs() <= tolerance
    }
}

pub(crate) trait PointExt: Sized {
    fn x(&self) -> f32;
    fn y(&self) -> f32;

    fn nearly_same(&self, other: Self) -> bool {
        self.x().is_nearly_equal(other.x()) && self.y().is_nearly_equal(other.y())
    }
}

impl PointExt for kurbo::Point {
    fn x(&self) -> f32 {
        self.x as f32
    }

    fn y(&self) -> f32 {
        self.y as f32
    }
}

/// Calculate a 128-bit siphash of a value.
pub(crate) fn hash128<T: Hash + ?Sized>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}

pub(crate) trait BezPathExt {
    fn fast_bounding_box(&self) -> Rect;
}

impl BezPathExt for BezPath {
    fn fast_bounding_box(&self) -> Rect {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        let mut include = |x: f64, y: f64| {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };

        for el in self.elements() {
            match *el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => include(p.x, p.y),
                PathEl::QuadTo(p1, p2) => {
                    include(p1.x, p1.y);
                    include(p2.x, p2.y);
                }
                PathEl::CurveTo(p1, p2, p3) => {
                    include(p1.x, p1.y);
                    include(p2.x, p2.y);
                    include(p3.x, p3.y);
                }
                PathEl::ClosePath => {}
            }
        }

        if min_x > max_x {
            Rect::ZERO
        } else {
            Rect::new(min_x, min_y, max_x, max_y)
        }
    }
}

/// Extension methods for rectangles.
pub trait RectExt {
    /// Convert the rectangle to a `kurbo` rectangle.
    fn to_kurbo(&self) -> Rect;
}

impl RectExt for pdf_syntax::object::Rect {
    fn to_kurbo(&self) -> Rect {
        Rect::new(self.x0, self.y0, self.x1, self.y1)
    }
}

// Note: Keep in sync with `pdf-interpret-write`.
/// Extension methods for PDF pages.
pub trait PageExt {
    /// Return the initial transform that should be applied when rendering. This accounts for a
    /// number of factors, such as the mismatch between PDF's y-up and most renderers' y-down
    /// coordinate system, the rotation of the page and the offset of the crop box.
    fn initial_transform(&self, invert_y: bool) -> Affine;
}

impl PageExt for Page<'_> {
    fn initial_transform(&self, invert_y: bool) -> Affine {
        // Use the raw CropBox origin for the coordinate-system translation.
        // MuPDF maps (CropBox.x0, CropBox.y0) → canvas (0, 0).  For normal
        // PDFs (CropBox ⊆ MediaBox) intersected_crop_box and crop_box share
        // the same origin, so there is no change.  For unusual documents where
        // CropBox extends beyond MediaBox (e.g. gen-802: CropBox=[0,0,684,864]
        // but MediaBox=[36,36,648,828]) the intersected origin was (36,36),
        // producing a 75-pixel content offset vs MuPDF. (#544 follow-up)
        let crop_box = self.crop_box();
        let (_, base_height) = self.base_dimensions();
        let (width, height) = self.render_dimensions();

        let horizontal_t =
            Affine::rotate(90.0_f64.to_radians()) * Affine::translate((0.0, -width as f64));
        let flipped_horizontal_t =
            Affine::translate((0.0, height as f64)) * Affine::rotate(-90.0_f64.to_radians());

        let rotation_transform = match self.rotation() {
            Rotation::None => Affine::IDENTITY,
            Rotation::Horizontal => {
                if invert_y {
                    horizontal_t
                } else {
                    flipped_horizontal_t
                }
            }
            Rotation::Flipped => {
                Affine::scale(-1.0) * Affine::translate((-width as f64, -height as f64))
            }
            Rotation::FlippedHorizontal => {
                if invert_y {
                    flipped_horizontal_t
                } else {
                    horizontal_t
                }
            }
        };

        let inversion_transform = if invert_y {
            Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, base_height as f64])
        } else {
            Affine::IDENTITY
        };

        rotation_transform * inversion_transform * Affine::translate((-crop_box.x0, -crop_box.y0))
    }
}

/// Decode a PDF text string to Unicode.
///
/// A text string is UTF-16BE behind a byte-order mark, UTF-8 behind its own,
/// or otherwise PDFDocEncoding: plain ASCII and Latin-1 throughout, except
/// for the two dozen codes below U+00A0 that name punctuation and spacing
/// marks instead. Undefined codes become the replacement character; decoding
/// never fails.
pub(crate) fn decode_text_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let mut units = Vec::with_capacity(bytes.len() / 2);
        let mut rest = &bytes[2..];
        while rest.len() >= 2 {
            units.push(u16::from_be_bytes([rest[0], rest[1]]));
            rest = &rest[2..];
        }
        if let Some(&last) = rest.first() {
            units.push(u16::from_be_bytes([last, 0]));
        }
        return Some(String::from_utf16_lossy(&units));
    }
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return Some(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let code = match byte {
            0x18 => 0x02D8,
            0x19 => 0x02C7,
            0x1A => 0x02C6,
            0x1B => 0x02D9,
            0x1C => 0x02DD,
            0x1D => 0x02DB,
            0x1E => 0x02DA,
            0x1F => 0x02DC,
            0x80 => 0x2022,
            0x81 => 0x2020,
            0x82 => 0x2021,
            0x83 => 0x2026,
            0x84 => 0x2014,
            0x85 => 0x2013,
            0x86 => 0x0192,
            0x87 => 0x2044,
            0x88 => 0x2039,
            0x89 => 0x203A,
            0x8A => 0x2212,
            0x8B => 0x2030,
            0x8C => 0x201E,
            0x8D => 0x201C,
            0x8E => 0x201D,
            0x8F => 0x2018,
            0x90 => 0x2019,
            0x91 => 0x201A,
            0x92 => 0x2122,
            0x93 => 0xFB01,
            0x94 => 0xFB02,
            0x95 => 0x0141,
            0x96 => 0x0152,
            0x97 => 0x0160,
            0x98 => 0x0178,
            0x99 => 0x017D,
            0x9A => 0x0131,
            0x9B => 0x0142,
            0x9C => 0x0153,
            0x9D => 0x0161,
            0x9E => 0x017E,
            0xA0 => 0x20AC,
            0x7F | 0x9F => 0xFFFD,
            _ => byte as u32,
        };
        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
    }
    Some(out)
}
