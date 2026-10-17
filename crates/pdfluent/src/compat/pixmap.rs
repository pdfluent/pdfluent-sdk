//! Images for the compatible API (lane L3: pixmaps and image queries).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Owned pixel buffers plus the page and document image listings. [`Pixmap`]
//! owns its samples; the info structs are plain snapshots of recorded
//! shapes. Rendering knobs live in [`RenderOptions`], shared by every call
//! that produces a pixmap.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document as LopdfDocument, Object, ObjectId};

use super::geometry::{CompareOp, IRect, Matrix, Quad, Rect};
use super::{ErrorKind, Page, Value};
use crate::document::PdfDocument;
use crate::error::Result;

/// Pixel colour spaces for pixmaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Colorspace {
    /// One grey component.
    Gray,
    /// Red, green, and blue components.
    Rgb,
    /// Cyan, magenta, yellow, and black components.
    Cmyk,
}

impl Colorspace {
    /// Recorded colour-space name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Gray => "DeviceGray",
            Self::Rgb => "DeviceRGB",
            Self::Cmyk => "DeviceCMYK",
        }
    }

    /// Components per pixel, excluding alpha.
    pub fn components(&self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
        }
    }

    /// Colour space for the recorded kind code.
    ///
    /// The recordings map 2 to grey and 3 to CMYK; every other code,
    /// including out-of-range ones, reads as RGB.
    pub fn new(_kind: i32) -> Self {
        match _kind {
            2 => Self::Gray,
            3 => Self::Cmyk,
            _ => Self::Rgb,
        }
    }

    /// Channel count, excluding alpha.
    pub fn n(&self) -> i32 {
        i32::from(self.components())
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("colorspaces cannot be ordered"))
    }
}

/// An owned pixel buffer: size, layout, samples, and resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct Pixmap {
    width: i32,
    height: i32,
    components: i32,
    alpha: bool,
    stride: usize,
    samples: Vec<u8>,
    xres: i32,
    yres: i32,
    origin_x: i32,
    origin_y: i32,
    colorspace: Colorspace,
}

/// Overhead the recorded `len`/`size` add on top of the samples.
///
/// The recordings pin `len(samples) + 96` for a rendered pixmap; with a
/// single data point the constant is the whole model.
const LEN_OVERHEAD: usize = 96;

/// Default resolution stamped on fresh and decoded pixmaps.
///
/// Renders without an explicit dpi record 96, and image facts record 96, so
/// fresh buffers follow the same default.
const DEFAULT_RES: i32 = 96;

/// Largest sample buffer this lane allocates: guards the `w * h * n`
/// product against overflow and absurd requests without a policy limit.
const MAX_SAMPLES: u64 = i32::MAX as u64;

impl Pixmap {
    /// Total channels per pixel, alpha included.
    fn channels(&self) -> usize {
        self.components.max(0) as usize
    }

    /// Colour channels per pixel, alpha excluded.
    fn color_channels(&self) -> usize {
        usize::from(self.colorspace.components())
    }

    /// Byte offset of pixel (`x`, `y`) in pixmap-local coordinates.
    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        let n = self.channels();
        (y as usize)
            .checked_mul(self.stride)?
            .checked_add((x as usize).checked_mul(n)?)
    }

    /// Intersect a box with the pixmap's own box; `None` selects the whole.
    fn clip_box(&self, bbox: Option<IRect>) -> IRect {
        let whole = self.irect();
        let Some(b) = bbox else {
            return whole;
        };
        IRect {
            x0: b.x0.max(whole.x0),
            y0: b.y0.max(whole.y0),
            x1: b.x1.min(whole.x1),
            y1: b.y1.min(whole.y1),
        }
    }

    /// Blank pixmap over an explicit origin and size.
    fn blank(
        colorspace: Colorspace,
        ox: i32,
        oy: i32,
        w: i32,
        h: i32,
        alpha: bool,
    ) -> Result<Self> {
        let w = w.max(0);
        let h = h.max(0);
        let n = i32::from(colorspace.components()) + i32::from(alpha);
        let len = u64::from(w.max(0) as u32)
            .checked_mul(u64::from(h.max(0) as u32))
            .and_then(|px| px.checked_mul(n as u64))
            .filter(|len| *len <= MAX_SAMPLES)
            .ok_or_else(|| ErrorKind::EngineFault.to_error("pixmap dimensions too large"))?;
        let stride = (w as usize).saturating_mul(n as usize);
        Ok(Self {
            width: w,
            height: h,
            components: n,
            alpha,
            stride,
            samples: vec![0; len as usize],
            xres: DEFAULT_RES,
            yres: DEFAULT_RES,
            origin_x: ox,
            origin_y: oy,
            colorspace,
        })
    }

    /// Blank pixmap over a box.
    pub fn new(_colorspace: Colorspace, _bounds: IRect, _alpha: bool) -> Result<Self> {
        let w = _bounds.x1.saturating_sub(_bounds.x0).max(0);
        let h = _bounds.y1.saturating_sub(_bounds.y0).max(0);
        Self::blank(_colorspace, _bounds.x0, _bounds.y0, w, h, _alpha)
    }

    /// Assemble a pixmap from explicit samples.
    ///
    /// Conversions rebuild every pixel through here; the samples must
    /// cover the box exactly.
    pub fn from_samples(
        colorspace: Colorspace,
        bounds: IRect,
        alpha: bool,
        samples: Vec<u8>,
        xres: i32,
        yres: i32,
    ) -> Result<Self> {
        let mut pixmap = Self::new(colorspace, bounds, alpha)?;
        if samples.len() != pixmap.samples.len() {
            return Err(ErrorKind::BadValue.to_error(format!(
                "samples cover {} bytes, want {}",
                samples.len(),
                pixmap.samples.len()
            )));
        }
        pixmap.samples = samples;
        pixmap.xres = xres;
        pixmap.yres = yres;
        Ok(pixmap)
    }

    /// Length without emptiness: the spec has no empty check here.
    #[allow(clippy::len_without_is_empty)]
    /// Sample count.
    pub fn len(&self) -> usize {
        self.samples.len() + LEN_OVERHEAD
    }

    /// Whether an alpha channel exists.
    pub fn alpha(&self) -> bool {
        self.alpha
    }

    /// Fill a box with a value.
    ///
    /// A missing value fills white; the fill covers every channel, alpha
    /// included.
    pub fn clear_with(&mut self, _value: Option<u8>, _bbox: Option<IRect>) -> Result<()> {
        let value = _value.unwrap_or(255);
        let area = self.clip_box(_bbox);
        let n = self.channels();
        // Recorded quirk (l3-pixmap-clear-with): the clipped fill covers the
        // box's last row, where the sibling box ops stop before it.
        let y_end = area
            .y1
            .saturating_add(1)
            .min(self.origin_y.saturating_add(self.height));
        for y in area.y0..y_end {
            let row = (y - self.origin_y) as usize * self.stride;
            let from = row + (area.x0 - self.origin_x) as usize * n;
            let to = row + (area.x1 - self.origin_x) as usize * n;
            if from < to && to <= self.samples.len() {
                self.samples[from..to].fill(value);
            }
        }
        Ok(())
    }

    /// Distinct colour count.
    ///
    /// Colours compare on the colour channels alone; a positive `colors`
    /// cap saturates the answer instead of counting past it.
    pub fn color_count(&self, _colors: i32, _clip: Option<IRect>) -> Result<i32> {
        let area = self.clip_box(_clip);
        let n = self.channels();
        let keep = self.color_channels();
        let cap = if _colors > 0 {
            _colors as usize
        } else {
            usize::MAX
        };
        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let Some(off) = self.offset(x - self.origin_x, y - self.origin_y) else {
                    continue;
                };
                seen.insert(self.samples[off..off + keep.min(n)].to_vec());
                if seen.len() >= cap {
                    return Ok(seen.len().min(i32::MAX as usize) as i32);
                }
            }
        }
        Ok(seen.len().min(i32::MAX as usize) as i32)
    }

    /// Most used colour and its count.
    ///
    /// Ties keep the first colour met in top-left scan order; the count
    /// covers the colour channels alone.
    pub fn color_topusage(&self, _clip: Option<IRect>) -> Result<(i32, Vec<u8>)> {
        let area = self.clip_box(_clip);
        let n = self.channels();
        let keep = self.color_channels().min(n);
        let mut counts: HashMap<Vec<u8>, u64> = HashMap::new();
        let mut order: Vec<Vec<u8>> = Vec::new();
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let Some(off) = self.offset(x - self.origin_x, y - self.origin_y) else {
                    continue;
                };
                let key = self.samples[off..off + keep].to_vec();
                if !counts.contains_key(&key) {
                    order.push(key.clone());
                }
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        let mut best: Vec<u8> = vec![0; keep];
        let mut best_count: u64 = 0;
        for key in &order {
            let count = counts[key];
            if count > best_count {
                best_count = count;
                best = key.clone();
            }
        }
        Ok((
            best_count.min(i64::MAX as u64).min(i32::MAX as u64) as i32,
            best,
        ))
    }

    /// Pixel colour space.
    pub fn colorspace(&self) -> Colorspace {
        self.colorspace
    }

    /// Copy a box from another pixmap.
    ///
    /// Both boxes live in the destination's box space; channels copy up to
    /// the narrower side, and a destination alpha with no source alpha reads
    /// opaque. Colour spaces must match.
    pub fn copy(&mut self, _source: &Pixmap, _bbox: IRect) -> Result<()> {
        if _source.colorspace != self.colorspace {
            return Err(ErrorKind::BadValue.to_error("cannot copy across colour spaces"));
        }
        let area = self.clip_box(Some(_bbox));
        let src_area = _source.clip_box(Some(_bbox));
        let from_x = area.x0.max(src_area.x0);
        let from_y = area.y0.max(src_area.y0);
        let to_x = area.x1.min(src_area.x1);
        let to_y = area.y1.min(src_area.y1);
        let dn = self.channels();
        let sn = _source.channels();
        let keep = dn.min(sn);
        for y in from_y..to_y {
            for x in from_x..to_x {
                let (Some(doff), Some(soff)) = (
                    self.offset(x - self.origin_x, y - self.origin_y),
                    _source.offset(x - _source.origin_x, y - _source.origin_y),
                ) else {
                    continue;
                };
                let (dst, src) = (doff, soff);
                for c in 0..keep {
                    self.samples[dst + c] = _source.samples[src + c];
                }
                if self.alpha && !_source.alpha {
                    self.samples[dst + dn - 1] = 255;
                }
            }
        }
        Ok(())
    }

    /// Sample digest: MD5 over the raw samples.
    pub fn digest(&self) -> Vec<u8> {
        md5::compute(&self.samples).0.to_vec()
    }

    /// Apply gamma correction.
    ///
    /// Each colour channel maps through `floor(255 * (v/255)^gamma)` with
    /// saturation; alpha is untouched and no gamma value errors.
    pub fn gamma_with(&mut self, _gamma: f64) -> Result<()> {
        let n = self.channels();
        let keep = self.color_channels().min(n);
        for row in self.samples.chunks_exact_mut(n.max(1)) {
            for v in row.iter_mut().take(keep) {
                let base = f64::from(*v) / 255.0;
                *v = (255.0 * base.powf(_gamma)).floor() as u8;
            }
        }
        Ok(())
    }

    /// Height in pixels.
    pub fn h(&self) -> i32 {
        self.height
    }

    /// Height in pixels.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Invert samples in a box.
    ///
    /// Colour channels flip; alpha stays as it is.
    pub fn invert_irect(&mut self, _bbox: Option<IRect>) -> Result<()> {
        let area = self.clip_box(_bbox);
        let n = self.channels();
        let keep = self.color_channels().min(n);
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let Some(off) = self.offset(x - self.origin_x, y - self.origin_y) else {
                    continue;
                };
                for v in self.samples[off..off + keep].iter_mut() {
                    *v = 255 - *v;
                }
            }
        }
        Ok(())
    }

    /// Pixel box.
    pub fn irect(&self) -> IRect {
        IRect {
            x0: self.origin_x,
            y0: self.origin_y,
            x1: self.origin_x.saturating_add(self.width),
            y1: self.origin_y.saturating_add(self.height),
        }
    }

    /// Whether single-colour.
    ///
    /// Grey buffers read as monochrome however their pixels look; anything
    /// with more than one colour channel does not.
    pub fn is_monochrome(&self) -> bool {
        self.colorspace == Colorspace::Gray
    }

    /// Whether one flat colour.
    pub fn is_unicolor(&self) -> bool {
        let n = self.channels().max(1);
        if self.samples.len() < n {
            return true;
        }
        let first = self.samples[..n].to_vec();
        self.samples.chunks_exact(n).all(|px| px == first)
    }

    /// Components per pixel.
    pub fn n(&self) -> i32 {
        self.components
    }

    /// Save an OCR PDF of the pixmap.
    ///
    /// This build ships no OCR engine, so the call always reports the
    /// missing component instead of writing a textless file.
    pub fn pdfocr_save(
        &self,
        _path: &str,
        _compress: bool,
        _language: Option<&str>,
        _tessdata: Option<&str>,
    ) -> Result<()> {
        Err(ErrorKind::MissingComponent.to_error("text recognition is not available in this build"))
    }

    /// OCR PDF bytes of the pixmap.
    ///
    /// Same missing engine as [`Self::pdfocr_save`]: recognition without a
    /// recogniser would be a silent lie.
    pub fn pdfocr_tobytes(
        &self,
        _compress: bool,
        _language: &str,
        _tessdata: Option<&str>,
    ) -> Result<Vec<u8>> {
        Err(ErrorKind::MissingComponent.to_error("text recognition is not available in this build"))
    }

    /// Imaging-library handle to the samples.
    ///
    /// The Rust core cannot speak to the imaging library; the Python
    /// binding implements this method itself and never calls here.
    pub fn pil_image(&self) -> Result<Value> {
        Err(ErrorKind::MissingComponent.to_error("imaging-library bridge lives in the binding"))
    }

    /// Save through the imaging library.
    ///
    /// Binding-implemented, like [`Self::pil_image`].
    pub fn pil_save(&self, _path: &str, _format: Option<&str>) -> Result<()> {
        Err(ErrorKind::MissingComponent.to_error("imaging-library bridge lives in the binding"))
    }

    /// Encode through the imaging library.
    ///
    /// Binding-implemented, like [`Self::pil_image`].
    pub fn pil_tobytes(&self, _format: &str) -> Result<Vec<u8>> {
        Err(ErrorKind::MissingComponent.to_error("imaging-library bridge lives in the binding"))
    }

    /// Sample at a pixel.
    ///
    /// Coordinates are absolute in the pixmap's box space; outside reads
    /// fail.
    pub fn pixel(&self, _x: i32, _y: i32) -> Result<Vec<u8>> {
        let Some(off) = self.offset(_x - self.origin_x, _y - self.origin_y) else {
            return Err(ErrorKind::EngineFault.to_error("pixel coordinates out of range"));
        };
        let n = self.channels();
        Ok(self.samples[off..off + n].to_vec())
    }

    /// Raw samples.
    pub fn samples(&self) -> &[u8] {
        &self.samples
    }

    /// Raw samples as a view.
    pub fn samples_mv(&self) -> &[u8] {
        &self.samples
    }

    /// Pointer to the samples.
    pub fn samples_ptr(&self) -> *const u8 {
        self.samples.as_ptr()
    }

    /// Save to a file.
    ///
    /// A missing output format follows the file extension, defaulting to
    /// PNG; an unknown extension is rejected like a bad `tobytes` name.
    pub fn save(&self, _path: &str, _output: Option<&str>, _jpg_quality: i32) -> Result<()> {
        let output = match _output {
            Some(name) => name.to_string(),
            None => match _path.rsplit('.').next() {
                Some(ext) if _path.contains('.') && Self::known_output(ext) => ext.to_string(),
                Some(ext) if _path.contains('.') => {
                    return Err(
                        ErrorKind::BadValue.to_error(format!("unsupported image format '{ext}'"))
                    );
                }
                _ => "png".to_string(),
            },
        };
        let bytes = self.tobytes(&output, _jpg_quality)?;
        std::fs::write(_path, bytes)
            .map_err(|e| ErrorKind::EngineFault.to_error(format!("cannot write '{_path}': {e}")))?;
        Ok(())
    }

    /// Whether `name` selects a supported encoding, case-insensitively.
    fn known_output(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "png" | "jpeg" | "jpg" | "ppm" | "pgm" | "pam"
        )
    }

    /// Set the alpha channel.
    ///
    /// `None` values paint the channel opaque; explicit bytes must cover
    /// every pixel exactly once. An `opaque` key rebuilds the channel as a
    /// chroma key (matches transparent, the rest opaque), `premultiply`
    /// scales the colours into the new alpha, and `matte` repaints fully
    /// transparent pixels afterwards.
    pub fn set_alpha(&mut self, _options: &AlphaOptions) -> Result<()> {
        if !self.alpha {
            return Err(ErrorKind::BadValue.to_error("pixmap has no alpha channel"));
        }
        let n = self.channels();
        let keep = self.color_channels().min(n);
        let pixels = (self.width.max(0) as usize).saturating_mul(self.height.max(0) as usize);
        match &_options.values {
            None => {
                for row in self.samples.chunks_exact_mut(n.max(1)) {
                    if let Some(a) = row.last_mut() {
                        *a = 255;
                    }
                }
            }
            Some(values) => {
                if values.len() != pixels {
                    return Err(ErrorKind::BadValue.to_error(format!(
                        "alpha values cover {} pixels, want {pixels}",
                        values.len()
                    )));
                }
                for (row, value) in self.samples.chunks_exact_mut(n.max(1)).zip(values.iter()) {
                    if let Some(a) = row.last_mut() {
                        *a = *value;
                    }
                }
            }
        }
        if let Some(opaque) = &_options.opaque {
            let key = value_color(opaque, keep)?;
            for row in self.samples.chunks_exact_mut(n.max(1)) {
                let hit = row.iter().take(keep).eq(key.iter());
                if let Some(a) = row.last_mut() {
                    *a = if hit { 0 } else { 255 };
                }
            }
        }
        if _options.premultiply {
            for row in self.samples.chunks_exact_mut(n.max(1)) {
                let a = u32::from(row[n - 1]);
                for v in row.iter_mut().take(keep) {
                    *v = ((u32::from(*v) * a / 255).min(255)) as u8;
                }
            }
        }
        if let Some(matte) = &_options.matte {
            let paint = value_color(matte, keep)?;
            for row in self.samples.chunks_exact_mut(n.max(1)) {
                if row[n - 1] == 0 {
                    for (v, p) in row.iter_mut().take(keep).zip(paint.iter()) {
                        *v = *p;
                    }
                }
            }
        }
        Ok(())
    }

    /// Set the resolution.
    pub fn set_dpi(&mut self, _xres: i32, _yres: i32) -> Result<()> {
        self.xres = _xres;
        self.yres = _yres;
        Ok(())
    }

    /// Set the box origin.
    pub fn set_origin(&mut self, _x: i32, _y: i32) -> Result<()> {
        self.origin_x = _x;
        self.origin_y = _y;
        Ok(())
    }

    /// Set one pixel.
    ///
    /// The colour must carry exactly one value per channel.
    pub fn set_pixel(&mut self, _x: i32, _y: i32, _color: &[u8]) -> Result<()> {
        let n = self.channels();
        if _color.len() != n {
            return Err(ErrorKind::BadValue
                .to_error(format!("colour carries {} values, want {n}", _color.len())));
        }
        let Some(off) = self.offset(_x - self.origin_x, _y - self.origin_y) else {
            return Err(ErrorKind::EngineFault.to_error("pixel coordinates out of range"));
        };
        self.samples[off..off + n].copy_from_slice(_color);
        Ok(())
    }

    /// Fill a box with a colour.
    ///
    /// The colour must carry exactly one value per channel; boxes outside
    /// the pixmap paint nothing.
    pub fn set_rect(&mut self, _bbox: IRect, _color: &[u8]) -> Result<()> {
        let n = self.channels();
        if _color.len() != n {
            return Err(ErrorKind::BadValue
                .to_error(format!("colour carries {} values, want {n}", _color.len())));
        }
        let area = self.clip_box(Some(_bbox));
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let Some(off) = self.offset(x - self.origin_x, y - self.origin_y) else {
                    continue;
                };
                self.samples[off..off + n].copy_from_slice(_color);
            }
        }
        Ok(())
    }

    /// Downsample by a factor.
    ///
    /// Each side shrinks by `2^factor`, rounding up, with every output
    /// pixel averaging its block; non-positive factors leave the pixmap
    /// alone.
    pub fn shrink(&mut self, _factor: i32) -> Result<()> {
        if _factor <= 0 || self.width <= 0 || self.height <= 0 {
            return Ok(());
        }
        let block = 1u64 << _factor.min(32);
        let out_w = (self.width.max(0) as u64)
            .div_ceil(block)
            .min(i32::MAX as u64) as i32;
        let out_h = (self.height.max(0) as u64)
            .div_ceil(block)
            .min(i32::MAX as u64) as i32;
        let (out_w, out_h) = (out_w.max(1), out_h.max(1));
        let n = self.channels();
        let mut samples = vec![0u8; (out_w as usize) * (out_h as usize) * n];
        for oy in 0..out_h {
            for ox in 0..out_w {
                let x0 = (ox as u64 * block).min(self.width as u64) as i32;
                let y0 = (oy as u64 * block).min(self.height as u64) as i32;
                let x1 = ((ox as u64 + 1) * block).min(self.width as u64) as i32;
                let y1 = ((oy as u64 + 1) * block).min(self.height as u64) as i32;
                let mut acc = vec![0u64; n];
                let mut count = 0u64;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let Some(off) = self.offset(x, y) else {
                            continue;
                        };
                        for (slot, v) in acc.iter_mut().zip(self.samples[off..off + n].iter()) {
                            *slot += u64::from(*v);
                        }
                        count += 1;
                    }
                }
                let base = (oy as usize * out_w as usize + ox as usize) * n;
                for (slot, total) in samples[base..base + n].iter_mut().zip(acc.iter()) {
                    *slot = if count == 0 {
                        0
                    } else {
                        ((total + count / 2) / count).min(255) as u8
                    };
                }
            }
        }
        self.width = out_w;
        self.height = out_h;
        self.stride = (out_w as usize).saturating_mul(n);
        self.samples = samples;
        Ok(())
    }

    /// Sample length in bytes.
    pub fn size(&self) -> usize {
        self.samples.len() + LEN_OVERHEAD
    }

    /// Row length in bytes.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Tint the samples.
    ///
    /// Each colour channel remaps linearly from `black` to `white`, read
    /// as packed `0xRRGGBB`; grey buffers use the low byte, CMYK leaves
    /// black alone, and alpha never moves.
    pub fn tint_with(&mut self, _black: i32, _white: i32) -> Result<()> {
        let (black, white) = (_black as u32, _white as u32);
        let ramp = |v: u8, lo: u8, hi: u8| {
            let (lo, hi, v) = (i32::from(lo), i32::from(hi), i32::from(v));
            (lo + (hi - lo) * v / 255).clamp(0, 255) as u8
        };
        let n = self.channels();
        let keep = self.color_channels().min(n);
        let map = if self.colorspace == Colorspace::Gray {
            vec![(black as u8, white as u8)]
        } else {
            (0..3)
                .map(|c| {
                    let shift = (2 - c) * 8;
                    (
                        ((black >> shift) & 0xff) as u8,
                        ((white >> shift) & 0xff) as u8,
                    )
                })
                .collect::<Vec<_>>()
        };
        for row in self.samples.chunks_exact_mut(n.max(1)) {
            for (i, v) in row.iter_mut().take(keep).enumerate() {
                if let Some((lo, hi)) = map.get(i) {
                    *v = ramp(*v, *lo, *hi);
                }
            }
        }
        Ok(())
    }

    /// Encode to bytes.
    ///
    /// Names match case-insensitively; JPEG flattens transparency onto
    /// white and CMYK renders through RGB, because neither format stores
    /// what it cannot hold.
    pub fn tobytes(&self, _output: &str, _jpg_quality: i32) -> Result<Vec<u8>> {
        match _output.to_ascii_lowercase().as_str() {
            "png" => self.encode_png(),
            "jpeg" | "jpg" => self.encode_jpeg(_jpg_quality),
            "ppm" => Ok(self.encode_pnm(false)),
            "pgm" => Ok(self.encode_pnm(true)),
            "pam" => Ok(self.encode_pam()),
            other => {
                Err(ErrorKind::BadValue.to_error(format!("unsupported image format '{other}'")))
            }
        }
    }

    /// Width in pixels.
    pub fn w(&self) -> i32 {
        self.width
    }

    /// Warped copy over a quad.
    ///
    /// The destination spans `width` by `height` pixels; each output pixel
    /// samples the source through the inverse perspective map with bilinear
    /// filtering, clamping at the source edges.
    pub fn warp(&self, _quad: Quad, _width: i32, _height: i32) -> Result<Pixmap> {
        if _width <= 0 || _height <= 0 {
            return Err(ErrorKind::BadValue.to_error("warp target must have positive size"));
        }
        if self.width <= 0 || self.height <= 0 {
            return Self::blank(self.colorspace, 0, 0, _width, _height, self.alpha);
        }
        let map = Homography::dest_to_source(
            f64::from(_width),
            f64::from(_height),
            _quad,
            self.origin_x,
            self.origin_y,
        )
        .ok_or_else(|| ErrorKind::BadValue.to_error("warp quad is degenerate"))?;
        let mut out = Self::blank(self.colorspace, 0, 0, _width, _height, self.alpha)?;
        out.xres = self.xres;
        out.yres = self.yres;
        out.sample_warped(self, &map);
        Ok(out)
    }

    /// Fill `self` by sampling `source` through a destination-to-source map.
    fn sample_warped(&mut self, source: &Pixmap, map: &Homography) {
        let n = self.channels();
        let (sw, sh) = (source.width, source.height);
        for y in 0..self.height {
            for x in 0..self.width {
                // Sample at pixel centres so an identity-ish map round-trips.
                let (sx, sy) = map.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
                let base = (y as usize * self.width as usize + x as usize) * n;
                for c in 0..n {
                    self.samples[base + c] = bilinear(source, c, sx - 0.5, sy - 0.5, sw, sh);
                }
            }
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Horizontal box origin.
    pub fn x(&self) -> i32 {
        self.origin_x
    }

    /// Horizontal resolution.
    pub fn xres(&self) -> i32 {
        self.xres
    }

    /// Vertical box origin.
    pub fn y(&self) -> i32 {
        self.origin_y
    }

    /// Vertical resolution.
    pub fn yres(&self) -> i32 {
        self.yres
    }

    /// Decode a pixmap from a file.
    pub fn from_file(_path: &str) -> Result<Self> {
        let bytes = std::fs::read(_path)
            .map_err(|e| ErrorKind::EngineFault.to_error(format!("cannot read '{_path}': {e}")))?;
        Self::from_bytes(&bytes)
    }

    /// Decode a pixmap from bytes.
    ///
    /// PNG and JPEG decode through their crates, the portable pixmap
    /// family parses by hand; anything else, however image-shaped its
    /// extension, fails as an unreadable file.
    pub fn from_bytes(_data: &[u8]) -> Result<Self> {
        if _data.is_empty() {
            return Err(ErrorKind::BadValue.to_error("cannot decode an empty image"));
        }
        if _data.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) {
            return Self::decode_png(_data);
        }
        if _data.starts_with(&[0xff, 0xd8]) {
            return Self::decode_jpeg(_data);
        }
        if _data.len() > 2 && _data[0] == b'P' && _data[1].is_ascii_digit() {
            return Self::decode_pnm(_data);
        }
        Err(ErrorKind::UnreadableFile.to_error("bytes are not a supported image"))
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::WrongType.to_error("pixmaps cannot be ordered"))
    }
}

/// Read a colour out of a dynamic option value, clamping defensively.
fn value_color(value: &Value, keep: usize) -> Result<Vec<u8>> {
    let Value::List(items) = value else {
        return Err(ErrorKind::BadValue.to_error("colour must be a sequence of bytes"));
    };
    if items.len() != keep {
        return Err(ErrorKind::BadValue.to_error(format!(
            "colour carries {} values, want {keep}",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(keep);
    for item in items {
        let Value::Int(v) = item else {
            return Err(ErrorKind::BadValue.to_error("colour values must be integers"));
        };
        out.push((*v).clamp(0, 255) as u8);
    }
    Ok(out)
}

/// A 3x3 perspective map in row-major order, destination to source.
struct Homography {
    m: [f64; 9],
}

impl Homography {
    /// Map destination pixels onto the source quad.
    ///
    /// Solves for the map sending the destination corners to the quad
    /// corners (upper-left, upper-right, lower-left, lower-right); a
    /// singular system means a degenerate quad.
    fn dest_to_source(w: f64, h: f64, quad: Quad, ox: i32, oy: i32) -> Option<Self> {
        let dst = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
        let src = [
            (quad.ul.x - f64::from(ox), quad.ul.y - f64::from(oy)),
            (quad.ur.x - f64::from(ox), quad.ur.y - f64::from(oy)),
            (quad.ll.x - f64::from(ox), quad.ll.y - f64::from(oy)),
            (quad.lr.x - f64::from(ox), quad.lr.y - f64::from(oy)),
        ];
        // Direct linear solve for h11..h32 with h33 = 1.
        let mut rows = [[0.0f64; 9]; 8];
        for (i, ((dx, dy), (sx, sy))) in dst.iter().zip(src.iter()).enumerate() {
            let (r0, r1) = (i * 2, i * 2 + 1);
            rows[r0] = [*dx, *dy, 1.0, 0.0, 0.0, 0.0, -sx * dx, -sx * dy, *sx];
            rows[r1] = [0.0, 0.0, 0.0, *dx, *dy, 1.0, -sy * dx, -sy * dy, *sy];
        }
        let mut coef = [0.0f64; 8];
        for col in 0..8 {
            let mut pivot = col;
            for row in col..8 {
                if rows[row][col].abs() > rows[pivot][col].abs() {
                    pivot = row;
                }
            }
            if rows[pivot][col].abs() < 1e-12 {
                return None;
            }
            rows.swap(col, pivot);
            let scale = rows[col][col];
            for slot in rows[col].iter_mut().skip(col) {
                *slot /= scale;
            }
            let pivot_row = rows[col];
            for (row, line) in rows.iter_mut().enumerate() {
                if row != col {
                    let factor = line[col];
                    for (slot, pivot) in line.iter_mut().skip(col).zip(pivot_row[col..].iter()) {
                        *slot -= factor * pivot;
                    }
                }
            }
        }
        for (i, slot) in coef.iter_mut().enumerate() {
            *slot = rows[i][8];
        }
        Some(Self {
            m: [
                coef[0], coef[1], coef[2], coef[3], coef[4], coef[5], coef[6], coef[7], 1.0,
            ],
        })
    }

    /// Apply the map; a zero divisor pins the sample at the origin.
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = &self.m;
        let w = m[6] * x + m[7] * y + m[8];
        if w.abs() < 1e-12 {
            return (0.0, 0.0);
        }
        (
            (m[0] * x + m[1] * y + m[2]) / w,
            (m[3] * x + m[4] * y + m[5]) / w,
        )
    }
}

/// One bilinear sample of `channel`, clamped to the source edges.
fn bilinear(source: &Pixmap, channel: usize, x: f64, y: f64, sw: i32, sh: i32) -> u8 {
    let n = source.channels().max(1);
    let at = |x: i32, y: i32| {
        let x = x.clamp(0, sw - 1).max(0) as usize;
        let y = y.clamp(0, sh - 1).max(0) as usize;
        source
            .samples
            .get(y * source.stride + x * n + channel)
            .copied()
            .unwrap_or(0)
    };
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let (fx, fy) = (x - f64::from(x0), y - f64::from(y0));
    let top = f64::from(at(x0, y0)) * (1.0 - fx) + f64::from(at(x0 + 1, y0)) * fx;
    let bottom = f64::from(at(x0, y0 + 1)) * (1.0 - fx) + f64::from(at(x0 + 1, y0 + 1)) * fx;
    (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
}

/// Rec.601 luma of an sRGB triple, rounded half up.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 + 500) / 1000).min(255) as u8
}

/// Naive CMYK ink complements to sRGB, rounded half up.
fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
    let inv = 255 - u32::from(k);
    [
        ((255 - u32::from(c)) * inv + 127) / 255,
        ((255 - u32::from(m)) * inv + 127) / 255,
        ((255 - u32::from(y)) * inv + 127) / 255,
    ]
    .map(|v| v.min(255) as u8)
}

/// Naive sRGB complements to CMYK ink, rounded half up.
fn rgb_to_cmyk(r: u8, g: u8, b: u8) -> [u8; 4] {
    let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
    let k = 255 - r.max(g).max(b);
    if k == 255 {
        return [0, 0, 0, 255];
    }
    let denom = 255 - k;
    [
        ((255 - r - k) * 255 + denom / 2) / denom,
        ((255 - g - k) * 255 + denom / 2) / denom,
        ((255 - b - k) * 255 + denom / 2) / denom,
        k,
    ]
    .map(|v| v.min(255) as u8)
}

impl Pixmap {
    /// Flatten transparency onto white, returning RGB triples.
    fn flattened_rgb(&self) -> Vec<u8> {
        let n = self.channels();
        let mut out =
            Vec::with_capacity((self.width.max(0) as usize) * (self.height.max(0) as usize) * 3);
        for row in self.samples.chunks_exact(n.max(1)) {
            let mut triple = match self.colorspace {
                Colorspace::Gray => [row.first().copied().unwrap_or(0); 3],
                Colorspace::Rgb => [
                    row.first().copied().unwrap_or(0),
                    row.get(1).copied().unwrap_or(0),
                    row.get(2).copied().unwrap_or(0),
                ],
                Colorspace::Cmyk => cmyk_to_rgb(
                    row.first().copied().unwrap_or(0),
                    row.get(1).copied().unwrap_or(0),
                    row.get(2).copied().unwrap_or(0),
                    row.get(3).copied().unwrap_or(0),
                ),
            };
            if self.alpha {
                let a = u32::from(row[n - 1]);
                for v in triple.iter_mut() {
                    *v = ((u32::from(*v) * a + 255 * (255 - a) + 127) / 255).min(255) as u8;
                }
            }
            out.extend_from_slice(&triple);
        }
        out
    }

    /// Flatten transparency onto white, returning grey samples.
    fn flattened_gray(&self) -> Vec<u8> {
        let rgb = self.flattened_rgb();
        rgb.chunks_exact(3)
            .map(|t| luma(t[0], t[1], t[2]))
            .collect()
    }

    /// Encode the samples as PNG.
    #[cfg(not(target_arch = "wasm32"))]
    fn encode_png(&self) -> Result<Vec<u8>> {
        use png::{BitDepth, ColorType, Encoder};
        let (w, h) = (self.width.max(0) as u32, self.height.max(0) as u32);
        let (color, data) = match (self.colorspace, self.alpha) {
            (Colorspace::Gray, false) => (ColorType::Grayscale, self.samples.clone()),
            (Colorspace::Gray, true) => (ColorType::GrayscaleAlpha, self.samples.clone()),
            (Colorspace::Rgb, false) => (ColorType::Rgb, self.samples.clone()),
            (Colorspace::Rgb, true) => (ColorType::Rgba, self.samples.clone()),
            (Colorspace::Cmyk, _) => {
                let rgb = self.flattened_rgb();
                let data = if self.alpha {
                    let n = self.channels();
                    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
                    for (t, row) in rgb.chunks_exact(3).zip(self.samples.chunks_exact(n.max(1))) {
                        rgba.extend_from_slice(t);
                        rgba.push(row[n - 1]);
                    }
                    rgba
                } else {
                    rgb
                };
                (
                    if self.alpha {
                        ColorType::Rgba
                    } else {
                        ColorType::Rgb
                    },
                    data,
                )
            }
        };
        let mut buf = Vec::new();
        {
            let mut encoder = Encoder::new(&mut buf, w, h);
            encoder.set_color(color);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| ErrorKind::EngineFault.to_error(format!("png header failed: {e}")))?;
            writer
                .write_image_data(&data)
                .map_err(|e| ErrorKind::EngineFault.to_error(format!("png write failed: {e}")))?;
        }
        Ok(buf)
    }

    /// Encode the samples as JPEG, flattening onto white.
    #[cfg(not(target_arch = "wasm32"))]
    fn encode_jpeg(&self, _jpg_quality: i32) -> Result<Vec<u8>> {
        use image::ImageEncoder;
        let quality = _jpg_quality.clamp(1, 100) as u8;
        let rgb = self.flattened_rgb();
        let mut buf = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
        encoder
            .write_image(
                &rgb,
                self.width.max(0) as u32,
                self.height.max(0) as u32,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| ErrorKind::EngineFault.to_error(format!("jpeg encoding failed: {e}")))?;
        Ok(buf)
    }

    /// Encode the samples as P6 PPM, or P5 PGM for grey buffers.
    ///
    /// Grey `pgm` output keeps one channel; every other buffer writes P6,
    /// matching the recorded RGB behaviour.
    fn encode_pnm(&self, gray: bool) -> Vec<u8> {
        let w = self.width.max(0);
        let h = self.height.max(0);
        if gray && self.colorspace == Colorspace::Gray && !self.alpha {
            let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
            out.extend_from_slice(&self.samples);
            out
        } else if gray && self.colorspace == Colorspace::Gray {
            let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
            out.extend_from_slice(&self.flattened_gray());
            out
        } else {
            let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
            out.extend_from_slice(&self.flattened_rgb());
            out
        }
    }

    /// Encode the samples as P7 PAM with every channel raw.
    fn encode_pam(&self) -> Vec<u8> {
        let (w, h, n) = (self.width.max(0), self.height.max(0), self.channels());
        let tupl = match (self.colorspace, self.alpha) {
            (Colorspace::Gray, false) => "GRAYSCALE",
            (Colorspace::Gray, true) => "GRAYSCALE_ALPHA",
            (Colorspace::Rgb, false) => "RGB",
            (Colorspace::Rgb, true) => "RGB_ALPHA",
            (Colorspace::Cmyk, false) => "CMYK",
            (Colorspace::Cmyk, true) => "CMYK_ALPHA",
        };
        let mut out =
            format!("P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH {n}\nMAXVAL 255\nTUPLTYPE {tupl}\nENDHDR\n")
                .into_bytes();
        out.extend_from_slice(&self.samples);
        out
    }

    /// Decode a PNG, expanding every colour type to 8-bit channels.
    #[cfg(not(target_arch = "wasm32"))]
    fn decode_png(data: &[u8]) -> Result<Self> {
        use png::ColorType;
        let decoder = png::Decoder::new(data);
        let mut reader = decoder
            .read_info()
            .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("png header failed: {e}")))?;
        let mut raw = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut raw)
            .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("png data failed: {e}")))?;
        if reader.info().interlaced {
            return Err(ErrorKind::UnreadableFile.to_error("interlaced png is not supported"));
        }
        let (w, h) = (info.width, info.height);
        if w > i32::MAX as u32 || h > i32::MAX as u32 {
            return Err(ErrorKind::UnreadableFile.to_error("png dimensions too large"));
        }
        let frame = &raw[..info.buffer_size()];
        let (colorspace, alpha, samples) = match (info.color_type, info.bit_depth) {
            (ColorType::Grayscale, png::BitDepth::Eight) => {
                (Colorspace::Gray, false, frame.to_vec())
            }
            (ColorType::Grayscale, png::BitDepth::Sixteen) => (
                Colorspace::Gray,
                false,
                frame.chunks_exact(2).map(|b| b[0]).collect(),
            ),
            (ColorType::Grayscale, _) => (
                Colorspace::Gray,
                false,
                expand_bits(frame, w, h, 1, info.bit_depth)?,
            ),
            (ColorType::GrayscaleAlpha, png::BitDepth::Eight) => {
                (Colorspace::Gray, true, frame.to_vec())
            }
            (ColorType::GrayscaleAlpha, png::BitDepth::Sixteen) => (
                Colorspace::Gray,
                true,
                frame.chunks_exact(4).flat_map(|b| [b[0], b[2]]).collect(),
            ),
            (ColorType::Rgb, png::BitDepth::Eight) => (Colorspace::Rgb, false, frame.to_vec()),
            (ColorType::Rgb, png::BitDepth::Sixteen) => (
                Colorspace::Rgb,
                false,
                frame
                    .chunks_exact(6)
                    .flat_map(|b| [b[0], b[2], b[4]])
                    .collect(),
            ),
            (ColorType::Rgba, png::BitDepth::Eight) => (Colorspace::Rgb, true, frame.to_vec()),
            (ColorType::Rgba, png::BitDepth::Sixteen) => (
                Colorspace::Rgb,
                true,
                frame
                    .chunks_exact(8)
                    .flat_map(|b| [b[0], b[2], b[4], b[6]])
                    .collect(),
            ),
            (ColorType::Indexed, _) => {
                let palette = reader.info().palette.clone().unwrap_or_default();
                let trns = reader.info().trns.clone().unwrap_or_default();
                let indices = expand_bits(frame, w, h, 1, info.bit_depth)?;
                let has_alpha = !trns.is_empty();
                let mut samples = Vec::with_capacity(indices.len() * if has_alpha { 4 } else { 3 });
                for index in indices {
                    let base = usize::from(index) * 3;
                    let triple = [
                        palette.get(base).copied().unwrap_or(0),
                        palette.get(base + 1).copied().unwrap_or(0),
                        palette.get(base + 2).copied().unwrap_or(0),
                    ];
                    samples.extend_from_slice(&triple);
                    if has_alpha {
                        samples.push(trns.get(usize::from(index)).copied().unwrap_or(255));
                    }
                }
                (Colorspace::Rgb, has_alpha, samples)
            }
            _ => {
                return Err(ErrorKind::UnreadableFile.to_error("unsupported png colour type"));
            }
        };
        let mut pixmap = Self::blank(colorspace, 0, 0, w as i32, h as i32, alpha)?;
        let want = pixmap.samples.len();
        if samples.len() < want {
            return Err(ErrorKind::UnreadableFile.to_error("truncated png data"));
        }
        pixmap.samples.copy_from_slice(&samples[..want]);
        Ok(pixmap)
    }

    /// Decode a JPEG, keeping grey sources grey.
    #[cfg(not(target_arch = "wasm32"))]
    fn decode_jpeg(data: &[u8]) -> Result<Self> {
        let image = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
            .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("jpeg decode failed: {e}")))?;
        let (w, h) = (image.width(), image.height());
        if w > i32::MAX as u32 || h > i32::MAX as u32 {
            return Err(ErrorKind::UnreadableFile.to_error("jpeg dimensions too large"));
        }
        let (colorspace, samples) = match &image {
            image::DynamicImage::ImageLuma8(gray) => (Colorspace::Gray, gray.as_raw().clone()),
            other => (Colorspace::Rgb, other.to_rgb8().into_raw()),
        };
        let mut pixmap = Self::blank(colorspace, 0, 0, w as i32, h as i32, false)?;
        let want = pixmap.samples.len();
        if samples.len() < want {
            return Err(ErrorKind::UnreadableFile.to_error("truncated jpeg data"));
        }
        pixmap.samples.copy_from_slice(&samples[..want]);
        Ok(pixmap)
    }

    /// Decode P5/P6/P7 portable pixmaps.
    fn decode_pnm(data: &[u8]) -> Result<Self> {
        let bad = |why: &str| ErrorKind::UnreadableFile.to_error(format!("bad pnm image: {why}"));
        let magic = data.first().copied().unwrap_or(0);
        let kind = data.get(1).copied().unwrap_or(0);
        if magic != b'P' || !matches!(kind, b'5' | b'6' | b'7') {
            return Err(ErrorKind::UnreadableFile.to_error("bytes are not a supported image"));
        }
        let mut pos = 2;
        let mut tokens: Vec<String> = Vec::new();
        // The PAM header ends at ENDHDR; P5/P6 need three tokens.
        while pos < data.len() && (kind == b'7' || tokens.len() < 3) {
            while pos < data.len() && data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos < data.len() && data[pos] == b'#' {
                while pos < data.len() && data[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            let start = pos;
            while pos < data.len() && !data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if start == pos {
                break;
            }
            let token = std::str::from_utf8(&data[start..pos]).map_err(|_| bad("bad header"))?;
            if kind == b'7' && token == "ENDHDR" {
                break;
            }
            tokens.push(token.to_string());
        }
        if pos < data.len() && data[pos].is_ascii_whitespace() {
            // A single whitespace byte separates header and raster.
            pos += 1;
        }
        let raster = data.get(pos..).ok_or_else(|| bad("missing raster"))?;
        let (w, h, depth, maxval) = if kind == b'7' {
            parse_pam_header(&tokens).ok_or_else(|| bad("bad pam header"))?
        } else {
            let w: i32 = tokens
                .first()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| bad("bad width"))?;
            let h: i32 = tokens
                .get(1)
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| bad("bad height"))?;
            let max: u32 = tokens
                .get(2)
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| bad("bad maxval"))?;
            (w, h, if kind == b'5' { 1 } else { 3 }, max)
        };
        if maxval != 255 {
            return Err(bad("only maxval 255 is supported"));
        }
        if w < 0 || h < 0 {
            return Err(bad("bad dimensions"));
        }
        let (colorspace, alpha) = match depth {
            1 => (Colorspace::Gray, false),
            2 => (Colorspace::Gray, true),
            3 => (Colorspace::Rgb, false),
            4 => (Colorspace::Rgb, true),
            _ => return Err(bad("bad depth")),
        };
        let mut pixmap = Self::blank(colorspace, 0, 0, w, h, alpha)?;
        let want = pixmap.samples.len();
        if raster.len() < want {
            return Err(ErrorKind::UnreadableFile.to_error("truncated pnm data"));
        }
        pixmap.samples.copy_from_slice(&raster[..want]);
        Ok(pixmap)
    }
}

/// Parse PAM header tokens into width, height, depth, and maxval.
fn parse_pam_header(tokens: &[String]) -> Option<(i32, i32, i32, u32)> {
    if tokens.first().map(String::as_str) != Some("P7") {
        // The tokenizer skipped the magic; headers start at WIDTH.
    }
    let mut w = None;
    let mut h = None;
    let mut depth = None;
    let mut maxval = None;
    let mut i = 0;
    while i + 1 < tokens.len() {
        match tokens[i].as_str() {
            "WIDTH" => w = tokens[i + 1].parse().ok(),
            "HEIGHT" => h = tokens[i + 1].parse().ok(),
            "DEPTH" => depth = tokens[i + 1].parse().ok(),
            "MAXVAL" => maxval = tokens[i + 1].parse().ok(),
            "TUPLTYPE" => {}
            _ => {}
        }
        i += 2;
    }
    Some((w?, h?, depth?, maxval?))
}

/// Unpack sub-byte grey samples to one byte per pixel.
#[cfg(not(target_arch = "wasm32"))]
fn expand_bits(
    frame: &[u8],
    w: u32,
    h: u32,
    channels: u32,
    depth: png::BitDepth,
) -> Result<Vec<u8>> {
    let bits = match depth {
        png::BitDepth::One => 1,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        _ => {
            return Err(ErrorKind::UnreadableFile.to_error("unsupported png bit depth"));
        }
    };
    let row_bits = w * channels * bits;
    let row_bytes = row_bits.div_ceil(8) as usize;
    if frame.len() < row_bytes * h as usize {
        return Err(ErrorKind::UnreadableFile.to_error("truncated png data"));
    }
    let scale = 255 / ((1u32 << bits) - 1);
    let mut out = Vec::with_capacity((w * h * channels) as usize);
    for row in frame.chunks_exact(row_bytes).take(h as usize) {
        let mut bit = 0;
        for _ in 0..w * channels {
            let byte = row[(bit / 8) as usize];
            let shift = 8 - bits - (bit % 8);
            out.push((((u32::from(byte) >> shift) & ((1 << bits) - 1)) * scale).min(255) as u8);
            bit += bits;
        }
    }
    Ok(out)
}

/// Options for rendering: shared by every call producing a pixmap.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RenderOptions {
    /// Transformation applied while rendering.
    pub matrix: Matrix,
    /// Resolution in dots per inch, or the default when `None`.
    pub dpi: Option<f64>,
    /// Target colour space, or the default when `None`.
    pub colorspace: Option<Colorspace>,
    /// Clip rectangle, or the whole page.
    pub clip: Option<Rect>,
    /// Keep an alpha channel.
    pub alpha: bool,
    /// Draw annotations.
    pub annots: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            matrix: Matrix {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
            dpi: None,
            colorspace: None,
            clip: None,
            alpha: false,
            annots: true,
        }
    }
}

impl RenderOptions {
    /// Render through this transformation.
    pub fn with_matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = matrix;
        self
    }

    /// Render at this resolution in dots per inch.
    pub fn with_dpi(mut self, dpi: f64) -> Self {
        self.dpi = Some(dpi);
        self
    }

    /// Render into this colour space.
    pub fn with_colorspace(mut self, colorspace: Colorspace) -> Self {
        self.colorspace = Some(colorspace);
        self
    }

    /// Clip rendering to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Keep an alpha channel.
    pub fn with_alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// Draw annotations.
    pub fn with_annots(mut self, annots: bool) -> Self {
        self.annots = annots;
        self
    }
}

/// One image on a page: object numbers, size, and encoding notes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRef {
    /// Image object number.
    pub xref: i32,
    /// Soft-mask object number, or zero.
    pub mask: i32,
    /// Image width, in pixels.
    pub width: i32,
    /// Image height, in pixels.
    pub height: i32,
    /// Bits per component.
    pub bits: i32,
    /// Colour-space name.
    pub colorspace_name: String,
    /// Alternate name, if any.
    pub alt: String,
    /// Resource name.
    pub name: String,
    /// Compression filter name.
    pub filter: String,
}

/// One placed image: box, transform, size, and colour notes.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedImage {
    /// Image box on the page, in points.
    pub bbox: Rect,
    /// Image-to-page transformation.
    pub transform: Matrix,
    /// Image width, in pixels.
    pub width: i32,
    /// Image height, in pixels.
    pub height: i32,
    /// Bits per component.
    pub bits: i32,
    /// Encoded size, in bytes.
    pub size: i32,
    /// Placement number on the page.
    pub number: i32,
    /// Colour-space name.
    pub colorspace_name: String,
    /// Whether the image carries a mask.
    pub has_mask: bool,
    /// Horizontal resolution, in dots per inch.
    pub xres: i32,
    /// Vertical resolution, in dots per inch.
    pub yres: i32,
    /// Sample digest, present when requested.
    pub digest: Option<Vec<u8>>,
}

/// One placed image with the object number it was drawn from.
///
/// [`PlacedImage`] carries no xref, so flag queries that report one use
/// this wrapper instead of growing the fixed struct.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedImageWithXref {
    /// The placement facts.
    pub image: PlacedImage,
    /// Image object number.
    pub xref: i32,
    /// Resource name the placement was drawn under.
    pub name: String,
    /// Components per pixel.
    pub comps: i32,
}

/// One extracted image: pixels plus the recorded header fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    /// Image width, in pixels.
    pub width: i32,
    /// Image height, in pixels.
    pub height: i32,
    /// Bits per component.
    pub bits: i32,
    /// Colour-space code.
    pub colorspace: i32,
    /// Colour-space name.
    pub colorspace_name: String,
    /// Suggested file extension.
    pub ext: String,
    /// Encoded size, in bytes.
    pub size: i32,
    /// Whether the image is masked.
    pub masked: bool,
    /// Horizontal resolution, in dots per inch.
    pub xres: i32,
    /// Vertical resolution, in dots per inch.
    pub yres: i32,
    /// Encoded image bytes.
    pub data: Vec<u8>,
}

/// Sniffed image facts: size and encoding notes for raw bytes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ImageProfile {
    /// Image width, in pixels.
    pub width: i32,
    /// Image height, in pixels.
    pub height: i32,
    /// Suggested file extension.
    pub ext: String,
}

/// Options for rewriting images: targets plus one switch per image kind.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RewriteOptions {
    /// Rewrite images above this resolution only.
    pub dpi_threshold: Option<i32>,
    /// Target resolution in dots per inch.
    pub dpi_target: i32,
    /// Target quality level.
    pub quality: i32,
    /// Rewrite lossy images.
    pub lossy: bool,
    /// Rewrite lossless images.
    pub lossless: bool,
    /// Rewrite bitonal images.
    pub bitonal: bool,
    /// Rewrite colour images.
    pub color: bool,
    /// Rewrite grey images.
    pub gray: bool,
    /// Convert rewritten images to grey.
    pub set_to_gray: bool,
}

impl Default for RewriteOptions {
    fn default() -> Self {
        Self {
            dpi_threshold: None,
            dpi_target: 0,
            quality: 0,
            lossy: true,
            lossless: true,
            bitonal: true,
            color: true,
            gray: true,
            set_to_gray: false,
        }
    }
}

impl RewriteOptions {
    /// Rewrite images above this resolution only.
    pub fn with_dpi_threshold(mut self, dpi_threshold: i32) -> Self {
        self.dpi_threshold = Some(dpi_threshold);
        self
    }

    /// Retarget images to this resolution in dots per inch.
    pub fn with_dpi_target(mut self, dpi_target: i32) -> Self {
        self.dpi_target = dpi_target;
        self
    }

    /// Retarget images to this quality level.
    pub fn with_quality(mut self, quality: i32) -> Self {
        self.quality = quality;
        self
    }

    /// Rewrite lossy images.
    pub fn with_lossy(mut self, lossy: bool) -> Self {
        self.lossy = lossy;
        self
    }

    /// Rewrite lossless images.
    pub fn with_lossless(mut self, lossless: bool) -> Self {
        self.lossless = lossless;
        self
    }

    /// Rewrite bitonal images.
    pub fn with_bitonal(mut self, bitonal: bool) -> Self {
        self.bitonal = bitonal;
        self
    }

    /// Rewrite colour images.
    pub fn with_color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    /// Rewrite grey images.
    pub fn with_gray(mut self, gray: bool) -> Self {
        self.gray = gray;
        self
    }

    /// Convert rewritten images to grey.
    pub fn with_set_to_gray(mut self, set_to_gray: bool) -> Self {
        self.set_to_gray = set_to_gray;
        self
    }
}

/// Options for setting alpha: values plus premultiply and matte knobs.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct AlphaOptions {
    /// Alpha values, or the default fill when `None`.
    pub values: Option<Vec<u8>>,
    /// Premultiply colour samples by alpha.
    pub premultiply: bool,
    /// Opaque colour override.
    pub opaque: Option<Value>,
    /// Matte colour override.
    pub matte: Option<Value>,
}

impl<'a> Page<'a> {
    /// Box of a named image.
    ///
    /// Unions every placement drawn under the name; the transform flag is
    /// a binding-side spelling (matrices come from [`page_image_bbox_in`]).
    pub fn get_image_bbox(&self, _name: &str, _transform: bool) -> Result<Rect> {
        let doc = self.document().lopdf();
        let page = page_object_id(doc, self.index())?;
        page_image_bbox_in(doc, page, _name).map(|(bbox, _)| bbox)
    }

    /// Placed images with facts.
    pub fn get_image_info(&self, _hashes: bool, _xrefs: bool) -> Result<Vec<PlacedImage>> {
        let doc = self.document().lopdf();
        let page = page_object_id(doc, self.index())?;
        let digest = _hashes || _xrefs;
        Ok(page_placements_in(doc, page, digest)?
            .into_iter()
            .map(|p| p.image)
            .collect())
    }

    /// Boxes of a named image.
    pub fn get_image_rects(&self, _name: &str, _transform: bool) -> Result<Vec<Rect>> {
        let doc = self.document().lopdf();
        let page = page_object_id(doc, self.index())?;
        let rects: Vec<Rect> = page_placements_in(doc, page, false)?
            .iter()
            .filter(|p| p.name == _name)
            .map(|p| p.image.bbox)
            .collect();
        if rects.is_empty() {
            return Err(ErrorKind::BadValue.to_error(format!("unknown image '{_name}'")));
        }
        Ok(rects)
    }

    /// Images on the page.
    pub fn get_images(&self, _full: bool) -> Result<Vec<ImageRef>> {
        let doc = self.document().lopdf();
        let page = page_object_id(doc, self.index())?;
        page_images_in(doc, page)
    }

    /// Rendered pixmap of the page.
    pub fn get_pixmap(&self, _options: &RenderOptions) -> Result<Pixmap> {
        render_pixmap(self.document().engine(), self.index(), _options)
    }

    /// SVG image of the page.
    pub fn get_svg_image(&self, _matrix: Option<Matrix>, _text_as_path: bool) -> Result<String> {
        svg_for_page(
            self.document().engine(),
            self.index(),
            _matrix,
            _text_as_path,
        )
    }
}

impl PdfDocument {
    /// Image bytes and header.
    pub fn extract_image(&self, _xref: i32) -> Result<ImageData> {
        extract_image_in(self.lopdf(), _xref)
    }

    /// Images on a page.
    pub fn get_page_images(&self, _page: i32, _full: bool) -> Result<Vec<ImageRef>> {
        let page = page_number_id(self.lopdf(), _page)?;
        page_images_in(self.lopdf(), page)
    }

    /// Rendered pixmap of a page.
    pub fn get_page_pixmap(&self, _page: i32, _options: &RenderOptions) -> Result<Pixmap> {
        if _page < 0 {
            return Err(ErrorKind::BadValue.to_error(format!("bad page number {_page}")));
        }
        render_pixmap(self.engine(), _page as usize, _options)
    }

    /// Rewrite images in place.
    pub fn rewrite_images(&mut self, _options: &RewriteOptions) -> Result<()> {
        rewrite_images_in(self.lopdf_mut(), _options)
    }

    /// Whether an object is an image.
    pub fn xref_is_image(&self, _xref: i32) -> Result<bool> {
        xref_is_image_in(self.lopdf(), _xref)
    }
}

/// Matrix placing an image.
///
/// Rotates the unit square by `rotate` degrees, keeping its footprint at
/// the origin, then maps that footprint onto `target`: onto the whole box,
/// or onto the centred aspect fit of the rotated `width` by `height` frame
/// when `keep` holds.
pub fn calc_image_matrix(
    _width: f64,
    _height: f64,
    _target: Rect,
    _rotate: i32,
    _keep: bool,
) -> Matrix {
    let tw = (_target.x1 - _target.x0).max(0.0);
    let th = (_target.y1 - _target.y0).max(0.0);
    let angle = (_rotate % 360) as f64 * std::f64::consts::PI / 180.0;
    let snap = |v: f64| {
        if (v - 1.0).abs() < 1e-9 {
            1.0
        } else if (v + 1.0).abs() < 1e-9 {
            -1.0
        } else if v.abs() < 1e-9 {
            0.0
        } else {
            v
        }
    };
    let (sin, cos) = (snap(angle.sin()), snap(angle.cos()));
    // Footprint of the rotated unit square about the origin.
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        let (rx, ry) = (cos * x - sin * y, sin * x + cos * y);
        min_x = min_x.min(rx);
        min_y = min_y.min(ry);
        max_x = max_x.max(rx);
        max_y = max_y.max(ry);
    }
    let bw = (max_x - min_x).max(f64::EPSILON);
    let bh = (max_y - min_y).max(f64::EPSILON);
    let rot = Matrix {
        a: cos,
        b: sin,
        c: -sin,
        d: cos,
        e: -min_x,
        f: -min_y,
    };
    // The rotated image frame, for the aspect fit.
    let ew = (_width.abs() * cos.abs() + _height.abs() * sin.abs()).max(f64::EPSILON);
    let eh = (_width.abs() * sin.abs() + _height.abs() * cos.abs()).max(f64::EPSILON);
    let (fx, fy, fw, fh) = if _keep && _width != 0.0 && _height != 0.0 {
        let s = (tw / ew).min(th / eh);
        let (fw, fh) = (ew * s, eh * s);
        (
            _target.x0 + (tw - fw) / 2.0,
            _target.y0 + (th - fh) / 2.0,
            fw,
            fh,
        )
    } else {
        (_target.x0, _target.y0, tw, th)
    };
    let place = Matrix {
        a: fw / bw,
        b: 0.0,
        c: 0.0,
        d: fh / bh,
        e: fx,
        f: fy,
    };
    concat_matrix(place, rot)
}

/// Sniffed facts for image bytes.
pub fn image_profile(_data: &[u8]) -> Result<ImageProfile> {
    if _data.is_empty() {
        return Err(ErrorKind::BadValue.to_error("cannot profile empty bytes"));
    }
    if let Ok(pixmap) = Pixmap::from_bytes(_data) {
        let ext = if _data.starts_with(&[0x89, b'P', b'N', b'G']) {
            "png"
        } else if _data.starts_with(&[0xff, 0xd8]) {
            "jpeg"
        } else if _data.starts_with(b"P5") {
            "pgm"
        } else if _data.starts_with(b"P6") {
            "ppm"
        } else {
            "pam"
        };
        return Ok(ImageProfile {
            width: pixmap.width(),
            height: pixmap.height(),
            ext: ext.to_string(),
        });
    }
    Err(ErrorKind::UnreadableFile.to_error("bytes are not a supported image"))
}

/// Union of two boxes.
fn union_rect(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// Affine composition `outer` after `inner`: `outer(inner(p))`.
fn concat_matrix(outer: Matrix, inner: Matrix) -> Matrix {
    Matrix {
        a: outer.a * inner.a + outer.c * inner.b,
        b: outer.b * inner.a + outer.d * inner.b,
        c: outer.a * inner.c + outer.c * inner.d,
        d: outer.b * inner.c + outer.d * inner.d,
        e: outer.a * inner.e + outer.c * inner.f + outer.e,
        f: outer.b * inner.e + outer.d * inner.f + outer.f,
    }
}

// ---------------------------------------------------------------------------
// Shared document cores: the bindings call these over their own handles.
// ---------------------------------------------------------------------------

/// Page object id for a 0-based index.
pub fn page_object_id(doc: &LopdfDocument, index: usize) -> Result<ObjectId> {
    doc.get_pages()
        .values()
        .nth(index)
        .copied()
        .ok_or_else(|| ErrorKind::BadValue.to_error(format!("bad page number {index}")))
}

/// Page object id for a 0-based page number.
pub fn page_number_id(doc: &LopdfDocument, page: i32) -> Result<ObjectId> {
    if page < 0 {
        return Err(ErrorKind::BadValue.to_error(format!("bad page number {page}")));
    }
    page_object_id(doc, page as usize)
}

/// Follow a reference, if that is what the object is.
fn deref<'a>(doc: &'a LopdfDocument, object: &'a Object) -> &'a Object {
    doc.dereference(object)
        .map(|(_, target)| target)
        .unwrap_or(object)
}

/// Read a page-tree entry, inheriting from ancestors.
fn page_entry<'a>(doc: &'a LopdfDocument, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..32 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(found) = dict.get(key) {
            return Some(found);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// A PDF number as f64.
fn num(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(v) => Some(*v as f64),
        Object::Real(v) => Some(f64::from(*v)),
        _ => None,
    }
}

/// A PDF number array of the expected length.
fn num_array(doc: &LopdfDocument, object: &Object, len: usize) -> Option<Vec<f64>> {
    let array = deref(doc, object).as_array().ok()?;
    if array.len() != len {
        return None;
    }
    array.iter().map(|o| num(deref(doc, o))).collect()
}

/// A rectangle entry, defaulting to US Letter.
fn box_entry(doc: &LopdfDocument, page: ObjectId, key: &[u8]) -> [f64; 4] {
    page_entry(doc, page, key)
        .and_then(|o| num_array(doc, o, 4))
        .map(|v| [v[0], v[1], v[2], v[3]])
        .unwrap_or([0.0, 0.0, 612.0, 792.0])
}

/// Page rotation in degrees, snapped to right angles.
fn rotation_entry(doc: &LopdfDocument, page: ObjectId) -> i32 {
    let raw = page_entry(doc, page, b"Rotate")
        .and_then(|o| num(deref(doc, o)))
        .unwrap_or(0.0) as i32;
    let norm = ((raw % 360) + 360) % 360;
    if matches!(norm, 0 | 90 | 180 | 270) {
        norm
    } else {
        0
    }
}

/// Display frame of a page: crop box mapped through rotation.
struct PageFrame {
    /// User-to-display affine, still y-up.
    to_display: [f64; 6],
    /// Display width and height in points.
    size: (f64, f64),
}

impl PageFrame {
    /// Build the frame from media box, crop box, and rotation.
    fn of(media: [f64; 4], crop: [f64; 4], rotate: i32) -> Self {
        let (mw, mh) = (
            (media[2] - media[0]).max(0.0),
            (media[3] - media[1]).max(0.0),
        );
        // Clockwise rotation within the media box, y-up throughout.
        let (rot, dw, dh) = match rotate {
            90 => ([0.0, -1.0, 1.0, 0.0, 0.0, mw], mh, mw),
            180 => ([-1.0, 0.0, 0.0, -1.0, mw, mh], mw, mh),
            270 => ([0.0, 1.0, -1.0, 0.0, mh, 0.0], mh, mw),
            _ => ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], mw, mh),
        };
        // Displayed crop box, then offset to its own origin.
        let corners = [
            apply_affine(&rot, crop[0], crop[1]),
            apply_affine(&rot, crop[2], crop[1]),
            apply_affine(&rot, crop[0], crop[3]),
            apply_affine(&rot, crop[2], crop[3]),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (x, y) in corners {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if !x0.is_finite() {
            x0 = 0.0;
            y0 = 0.0;
            x1 = dw;
            y1 = dh;
        }
        let to_display = concat_affine([1.0, 0.0, 0.0, 1.0, -x0, -y0], rot);
        Self {
            to_display,
            size: ((x1 - x0).max(0.0), (y1 - y0).max(0.0)),
        }
    }
}

/// Apply a six-coefficient affine map.
fn apply_affine(m: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Compose affines: `outer` after `inner`.
fn concat_affine(outer: [f64; 6], inner: [f64; 6]) -> [f64; 6] {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

/// Page display frame from the page tree.
fn page_frame(doc: &LopdfDocument, page: ObjectId) -> PageFrame {
    let media = box_entry(doc, page, b"MediaBox");
    let mut crop = box_entry(doc, page, b"CropBox");
    if page_entry(doc, page, b"CropBox").is_none() {
        crop = media;
    }
    PageFrame::of(media, crop, rotation_entry(doc, page))
}

/// A resolved image XObject and its dictionary facts.
struct ImageFacts {
    /// Object number, or zero for direct objects.
    xref: i32,
    /// Mask object number, or zero.
    mask: i32,
    /// Width in pixels.
    width: i32,
    /// Height in pixels.
    height: i32,
    /// Bits per component.
    bits: i32,
    /// Components per pixel.
    comps: i32,
    /// Colour-space display name.
    colorspace_name: String,
    /// Alternate colour-space name, if any.
    alt: String,
    /// First filter name, normalised, or empty.
    filter: String,
    /// Whether any filter is present.
    filtered: bool,
    /// Whether the image is a stencil mask.
    stencil: bool,
    /// Decode array, pairs per component.
    decode: Vec<f64>,
}

/// Read an image dictionary's facts; stencil callers pass their flag.
fn image_facts(
    doc: &LopdfDocument,
    id: Option<ObjectId>,
    dict: &Dictionary,
    direct_ok: bool,
) -> Option<ImageFacts> {
    let xref = id.map(|(n, _)| n as i32).unwrap_or(0);
    if id.is_none() && !direct_ok {
        return None;
    }
    let subtype = dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| deref(doc, o).as_name().ok())?;
    if subtype != b"Image" {
        return None;
    }
    let int_of = |key: &[u8], default: i32| {
        dict.get(key)
            .ok()
            .and_then(|o| num(deref(doc, o)))
            .map(|v| v as i32)
            .unwrap_or(default)
    };
    let stencil = dict
        .get(b"ImageMask")
        .ok()
        .and_then(|o| deref(doc, o).as_bool().ok())
        .unwrap_or(false);
    let (comps, colorspace_name, alt) = match dict.get(b"ColorSpace").ok() {
        Some(cs) => {
            let parsed = parse_colorspace(doc, deref(doc, cs));
            (parsed.0, parsed.1, parsed.2)
        }
        None if stencil => (1, "None".to_string(), String::new()),
        None => (3, "DeviceRGB".to_string(), String::new()),
    };
    let mask = mask_xref_of(doc, dict);
    let filters = filter_names(doc, dict);
    Some(ImageFacts {
        xref,
        mask,
        width: int_of(b"Width", 0),
        height: int_of(b"Height", 0),
        bits: int_of(b"BitsPerComponent", 8),
        comps,
        colorspace_name: if stencil {
            "None".to_string()
        } else {
            colorspace_name
        },
        alt,
        filter: filters.first().cloned().unwrap_or_default(),
        filtered: !filters.is_empty(),
        stencil,
        decode: dict
            .get(b"Decode")
            .ok()
            .and_then(|o| {
                deref(doc, o)
                    .as_array()
                    .ok()
                    .map(|a| a.iter().filter_map(|v| num(deref(doc, v))).collect())
            })
            .unwrap_or_default(),
    })
}

/// Mask object number behind `/SMask` or a `/Mask` stream.
fn mask_xref_of(doc: &LopdfDocument, dict: &Dictionary) -> i32 {
    if let Ok(smask) = dict.get(b"SMask") {
        let direct = smask.as_reference();
        let target = deref(doc, smask).as_reference().or(direct);
        if let Ok(id) = target {
            return id.0 as i32;
        }
    }
    if let Ok(mask) = dict.get(b"Mask") {
        if let Ok(id) = mask.as_reference() {
            return id.0 as i32;
        }
    }
    0
}

/// Normalised filter names behind `/Filter`.
fn filter_names(doc: &LopdfDocument, dict: &Dictionary) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(filter) = dict.get(b"Filter") else {
        return out;
    };
    let target = deref(doc, filter);
    let mut push = |name: &[u8]| {
        let full = match name {
            b"AHx" => "ASCIIHexDecode",
            b"A85" => "ASCII85Decode",
            b"LZW" => "LZWDecode",
            b"Fl" => "FlateDecode",
            b"RL" => "RunLengthDecode",
            b"CCF" => "CCITTFaxDecode",
            b"DCT" => "DCTDecode",
            other => std::str::from_utf8(other).unwrap_or(""),
        };
        if !full.is_empty() {
            out.push(full.to_string());
        }
    };
    if let Ok(name) = target.as_name() {
        push(name);
    } else if let Ok(array) = target.as_array() {
        for item in array {
            if let Ok(name) = deref(doc, item).as_name() {
                push(name);
            }
        }
    }
    out
}

/// Parse a colour space into component count, display name, and alternate.
fn parse_colorspace(doc: &LopdfDocument, object: &Object) -> (i32, String, String) {
    let target = deref(doc, object);
    if let Ok(name) = target.as_name() {
        return match name {
            b"DeviceGray" => (1, "DeviceGray".to_string(), String::new()),
            b"DeviceRGB" => (3, "DeviceRGB".to_string(), String::new()),
            b"DeviceCMYK" => (4, "DeviceCMYK".to_string(), String::new()),
            b"CalGray" => (1, "CalGray".to_string(), String::new()),
            b"CalRGB" => (3, "CalRGB".to_string(), String::new()),
            b"Lab" => (3, "Lab".to_string(), String::new()),
            b"Pattern" => (3, "Pattern".to_string(), String::new()),
            _ => (3, "DeviceRGB".to_string(), String::new()),
        };
    }
    let Ok(array) = target.as_array() else {
        return (3, "DeviceRGB".to_string(), String::new());
    };
    let head = array
        .first()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"");
    match head {
        b"ICCBased" => {
            let stream = array.get(1).map(|o| deref(doc, o));
            let n = stream
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.dict.get(b"N").ok())
                .and_then(|o| num(deref(doc, o)))
                .map(|v| v as i32)
                .unwrap_or(3);
            let alt = stream
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.dict.get(b"Alternate").ok())
                .map(|o| parse_colorspace(doc, deref(doc, o)).1)
                .unwrap_or_default();
            let short = alt
                .strip_prefix("Device")
                .map(str::to_ascii_uppercase)
                .unwrap_or_else(|| alt.to_ascii_uppercase());
            let desc = stream
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| icc_desc(&s.content))
                .map(|d| format!(",{d}"))
                .unwrap_or_default();
            (n, format!("ICCBased({short}{desc})"), alt)
        }
        b"Indexed" => {
            let base = array
                .get(1)
                .map(|o| parse_colorspace(doc, deref(doc, o)).1)
                .unwrap_or_else(|| "DeviceRGB".to_string());
            let hival = array.get(2).and_then(|o| num(deref(doc, o))).unwrap_or(0.0) as i32;
            (1, format!("Indexed({hival},{base})"), String::new())
        }
        b"Separation" => {
            let name = array
                .get(1)
                .and_then(|o| deref(doc, o).as_name().ok())
                .and_then(|n| std::str::from_utf8(n).ok())
                .unwrap_or("");
            let alt = array
                .get(2)
                .map(|o| parse_colorspace(doc, deref(doc, o)).1)
                .unwrap_or_default();
            (1, format!("Separation({name},{alt})"), alt)
        }
        b"DeviceN" => {
            let count = array
                .get(1)
                .and_then(|o| deref(doc, o).as_array().ok())
                .map(|a| a.len() as i32)
                .unwrap_or(0);
            let alt = array
                .get(2)
                .map(|o| parse_colorspace(doc, deref(doc, o)).1)
                .unwrap_or_default();
            (count.max(1), format!("DeviceN({alt})"), alt)
        }
        b"CalGray" => (1, "CalGray".to_string(), String::new()),
        b"CalRGB" => (3, "CalRGB".to_string(), String::new()),
        b"Lab" => (3, "Lab".to_string(), String::new()),
        b"Pattern" => (3, "Pattern".to_string(), String::new()),
        _ => (3, "DeviceRGB".to_string(), String::new()),
    }
}

/// Decoded image samples: one byte per component, palette expanded.
struct DecodedSamples {
    /// Components per pixel after expansion.
    comps: i32,
    /// Row-major samples.
    samples: Vec<u8>,
}

/// Decode an image stream to one byte per component.
///
/// Runs the stream filters, unpacks sub-byte samples, applies `/Decode`,
/// and expands indexed palettes. JPEG payloads decode through the JPEG
/// codec; anything the filters cannot handle fails here.
fn decode_image_samples(
    doc: &LopdfDocument,
    stream: &lopdf::Stream,
    facts: &ImageFacts,
) -> Result<DecodedSamples> {
    let fail =
        |why: &str| ErrorKind::UnreadableFile.to_error(format!("cannot decode image: {why}"));
    if facts.width <= 0 || facts.height <= 0 {
        return Err(fail("bad dimensions"));
    }
    if facts.filter == "DCTDecode" {
        let image = image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg)
            .map_err(|_| fail("bad jpeg data"))?;
        let rgb = image.to_rgb8();
        return Ok(DecodedSamples {
            comps: 3,
            samples: rgb.into_raw(),
        });
    }
    let raw = stream
        .decompressed_content()
        .map_err(|_| fail("unsupported filters"))?;
    let bpc = facts.bits;
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return Err(fail("bad bit depth"));
    }
    let (w, h, n) = (
        facts.width as usize,
        facts.height as usize,
        facts.comps.max(1) as usize,
    );
    let row_len = (w * bpc as usize * n).div_ceil(8);
    if raw.len() < row_len.saturating_mul(h) {
        return Err(fail("truncated image data"));
    }
    // Unpack to one value per component.
    let mut values = Vec::with_capacity(w * h * n);
    if bpc == 16 {
        for v in raw.chunks_exact(2).take(w * h * n) {
            values.push(v[0]);
        }
    } else if bpc == 8 {
        values.extend_from_slice(&raw[..w * h * n]);
    } else {
        for row in raw.chunks_exact(row_len).take(h) {
            let mut bit = 0usize;
            for _ in 0..w * n {
                let byte = row[bit / 8];
                let shift = 8 - bpc as usize - (bit % 8);
                values.push((byte >> shift) & ((1 << bpc) - 1));
                bit += bpc as usize;
            }
        }
    }
    // Apply the decode array; the default maps identity.
    let max = if bpc == 16 { 255u32 } else { (1u32 << bpc) - 1 };
    if !facts.decode.is_empty() {
        for (i, v) in values.iter_mut().enumerate() {
            let c = i % n;
            let (lo, hi) = (
                facts.decode.get(c * 2).copied().unwrap_or(0.0),
                facts.decode.get(c * 2 + 1).copied().unwrap_or(1.0),
            );
            let scaled = (f64::from(*v) / f64::from(max) - lo) / (hi - lo).max(f64::EPSILON);
            *v = (scaled * f64::from(max)).round().clamp(0.0, f64::from(max)) as u8;
        }
    }
    // Expand indexed palettes to their base components.
    if facts.colorspace_name.starts_with("Indexed(") {
        let (base_n, palette) = indexed_palette(doc, stream)?;
        let mut expanded = Vec::with_capacity(w * h * base_n);
        for index in values {
            let base = usize::from(index) * base_n;
            for c in 0..base_n {
                expanded.push(palette.get(base + c).copied().unwrap_or(0));
            }
        }
        return Ok(DecodedSamples {
            comps: base_n as i32,
            samples: expanded,
        });
    }
    if facts.stencil {
        // Stencils unpack above; keep single-bit values as bytes.
        return Ok(DecodedSamples {
            comps: 1,
            samples: values,
        });
    }
    Ok(DecodedSamples {
        comps: n as i32,
        samples: values,
    })
}

/// Palette bytes and base width behind an indexed colour space.
fn indexed_palette(doc: &LopdfDocument, stream: &lopdf::Stream) -> Result<(usize, Vec<u8>)> {
    let fail = || ErrorKind::UnreadableFile.to_error("cannot decode image: bad palette");
    let cs = stream.dict.get(b"ColorSpace").map_err(|_| fail())?;
    let array = deref(doc, cs).as_array().map_err(|_| fail())?;
    let base_n = array
        .get(1)
        .map(|o| parse_colorspace(doc, deref(doc, o)).0)
        .unwrap_or(3)
        .max(1) as usize;
    let hival = array
        .get(2)
        .and_then(|o| num(deref(doc, o)))
        .unwrap_or(0.0)
        .max(0.0) as usize;
    let raw = array.get(3).ok_or_else(fail)?;
    let target = deref(doc, raw);
    let mut palette: Vec<u8> = if let Ok(bytes) = target.as_str() {
        bytes.to_vec()
    } else if let Ok(nested) = target.as_stream() {
        nested
            .decompressed_content()
            .unwrap_or_else(|_| nested.content.clone())
    } else {
        return Err(fail());
    };
    palette.resize((hival + 1) * base_n, 0);
    Ok((base_n, palette))
}

/// Images on a page, in resource order.
pub fn page_images_in(doc: &LopdfDocument, page: ObjectId) -> Result<Vec<ImageRef>> {
    let mut out = Vec::new();
    for (name, id, dict) in page_xobjects(doc, page)? {
        let Some(facts) = image_facts(doc, Some(id), &dict, false) else {
            continue;
        };
        out.push(ImageRef {
            xref: facts.xref,
            mask: facts.mask,
            width: facts.width,
            height: facts.height,
            bits: facts.bits,
            colorspace_name: facts.colorspace_name,
            alt: facts.alt,
            name,
            filter: facts.filter,
        });
    }
    Ok(out)
}

/// One page image's listing row plus its component count.
///
/// [`ImageRef`] is a pinned recording without room for the count, so the
/// plain facade listing reads this wrapper instead of growing it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImageFacts {
    /// The listing row [`page_images_in`] would return.
    pub image: ImageRef,
    /// Components per pixel.
    pub comps: i32,
}

/// Page images with component counts, for the plain facade listing.
///
/// Same walk as [`page_images_in`] — same pages, same skips, same rows —
/// with the facts' component count attached to each row.
pub fn page_image_facts_in(doc: &LopdfDocument, page: ObjectId) -> Result<Vec<PageImageFacts>> {
    let mut out = Vec::new();
    for (name, id, dict) in page_xobjects(doc, page)? {
        let Some(facts) = image_facts(doc, Some(id), &dict, false) else {
            continue;
        };
        out.push(PageImageFacts {
            image: ImageRef {
                xref: facts.xref,
                mask: facts.mask,
                width: facts.width,
                height: facts.height,
                bits: facts.bits,
                colorspace_name: facts.colorspace_name,
                alt: facts.alt,
                name,
                filter: facts.filter,
            },
            comps: facts.comps,
        });
    }
    Ok(out)
}

/// Non-image XObjects on a page: object numbers with resource names.
pub fn page_form_xobjects(doc: &LopdfDocument, page: ObjectId) -> Result<Vec<(i32, String)>> {
    let mut out = Vec::new();
    for (name, id, dict) in page_xobjects(doc, page)? {
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| deref(doc, o).as_name().ok())
            .unwrap_or(b"");
        if subtype == b"Image" {
            continue;
        }
        out.push((id.0 as i32, name));
    }
    Ok(out)
}

/// One form XObject row: number, name, generation, and box corners.
pub type XObjectRow = (i32, String, i32, [f64; 4]);

/// Page form XObjects with generations and boxes.
pub fn page_form_xobject_rows(doc: &LopdfDocument, page: ObjectId) -> Result<Vec<XObjectRow>> {
    let mut out = Vec::new();
    for (name, id, dict) in page_xobjects(doc, page)? {
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| deref(doc, o).as_name().ok())
            .unwrap_or(b"");
        if subtype == b"Image" {
            continue;
        }
        out.push((id.0 as i32, name, id.1 as i32, xobject_box(&dict)));
    }
    Ok(out)
}

/// One XObject's box corners, defaulting to the empty box.
///
/// Files may store inverted corners; the recorded form is normalised.
fn xobject_box(dict: &Dictionary) -> [f64; 4] {
    let mut edges = [0.0, 0.0, 0.0, 0.0];
    if let Ok(bbox) = dict.get(b"BBox").and_then(|o| o.as_array()) {
        for (index, edge) in edges.iter_mut().enumerate() {
            *edge = bbox
                .get(index)
                .and_then(|o| o.as_float().ok().map(f64::from))
                .unwrap_or(0.0);
        }
    }
    [
        edges[0].min(edges[2]),
        edges[1].min(edges[3]),
        edges[0].max(edges[2]),
        edges[1].max(edges[3]),
    ]
}

/// Page XObjects with names, ids, and dictionaries, in resource order.
fn page_xobjects(
    doc: &LopdfDocument,
    page: ObjectId,
) -> Result<Vec<(String, ObjectId, Dictionary)>> {
    let mut out = Vec::new();
    let Some(resources) = page_entry(doc, page, b"Resources") else {
        return Ok(out);
    };
    let resources = deref(doc, resources);
    let Ok(dict) = resources.as_dict() else {
        return Ok(out);
    };
    let Ok(xobjects) = dict.get(b"XObject") else {
        return Ok(out);
    };
    let xobjects = deref(doc, xobjects);
    let Ok(entries) = xobjects.as_dict() else {
        return Ok(out);
    };
    for (name, target) in entries.iter() {
        let Ok(id) = target.as_reference() else {
            continue;
        };
        let Ok(object) = doc.get_object(id) else {
            continue;
        };
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        out.push((
            String::from_utf8_lossy(name).into_owned(),
            id,
            stream.dict.clone(),
        ));
    }
    Ok(out)
}

/// Resolve an xref to its object id, scanning generations past zero.
fn resolve_xref(doc: &LopdfDocument, xref: i32) -> Result<ObjectId> {
    if xref <= 0 {
        return Err(ErrorKind::BadValue.to_error(format!("bad reference {xref}")));
    }
    let number = xref as u32;
    if doc.get_object((number, 0)).is_ok() {
        return Ok((number, 0));
    }
    doc.objects
        .keys()
        .find(|(n, _)| *n == number)
        .copied()
        .ok_or_else(|| ErrorKind::BadValue.to_error(format!("bad reference {xref}")))
}

/// Whether an object is an image; unknown references fail.
pub fn xref_is_image_in(doc: &LopdfDocument, xref: i32) -> Result<bool> {
    let id = resolve_xref(doc, xref)?;
    let Ok(object) = doc.get_object(id) else {
        return Err(ErrorKind::BadValue.to_error(format!("bad reference {xref}")));
    };
    let Ok(stream) = object.as_stream() else {
        return Ok(false);
    };
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| deref(doc, o).as_name().ok())
        .unwrap_or(b"");
    Ok(subtype == b"Image")
}

/// Mask object number behind an image reference, or zero.
pub fn image_mask_xref(doc: &LopdfDocument, xref: i32) -> Result<i32> {
    let id = resolve_xref(doc, xref)?;
    let object = doc
        .get_object(id)
        .map_err(|_| ErrorKind::BadValue.to_error(format!("bad reference {xref}")))?;
    let stream = object
        .as_stream()
        .map_err(|_| ErrorKind::BadValue.to_error(format!("reference {xref} is not an image")))?;
    Ok(mask_xref_of(doc, &stream.dict))
}

/// Read the `desc` tag out of an ICC profile, bounds-checked.
fn icc_desc(data: &[u8]) -> Option<String> {
    if data.len() < 132 {
        return None;
    }
    let count = u32::from_be_bytes(data[128..132].try_into().ok()?) as usize;
    if count > 64 {
        return None;
    }
    for i in 0..count {
        let base = 132 + i * 12;
        let entry = data.get(base..base + 12)?;
        if &entry[0..4] != b"desc" {
            continue;
        }
        let off = u32::from_be_bytes(entry[4..8].try_into().ok()?) as usize;
        let size = u32::from_be_bytes(entry[8..12].try_into().ok()?) as usize;
        let tag = data.get(off..off + size.min(data.len().saturating_sub(off)))?;
        if tag.len() < 12 || &tag[0..4] != b"desc" {
            continue;
        }
        let len = u32::from_be_bytes(tag[8..12].try_into().ok()?) as usize;
        let raw = tag.get(12..12 + len.min(tag.len().saturating_sub(12)))?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let text = std::str::from_utf8(&raw[..end]).ok()?;
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    None
}

/// One raw placement: name, source, and image-space matrix.
struct RawPlacement {
    /// Resource name drawn under.
    name: String,
    /// Image object number.
    xref: i32,
    /// Image-space to user-space matrix.
    ctm: [f64; 6],
}

/// Graphics-state counters shared across a page walk.
struct WalkCounters {
    /// Text blocks opened so far.
    texts: i32,
    /// Image placements recorded so far.
    images: i32,
}

/// Placements on a page with full facts, in drawing order.
pub fn page_placements_in(
    doc: &LopdfDocument,
    page: ObjectId,
    with_digest: bool,
) -> Result<Vec<PlacedImageWithXref>> {
    let frame = page_frame(doc, page);
    let mut counters = WalkCounters {
        texts: 0,
        images: 0,
    };
    let mut raw: Vec<(RawPlacement, i32)> = Vec::new();
    let resources = page_entry(doc, page, b"Resources")
        .map(|o| deref(doc, o).as_dict().ok())
        .unwrap_or(None);
    for content_id in doc.get_page_contents(page) {
        let Ok(object) = doc.get_object(content_id) else {
            continue;
        };
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        walk_operations(
            doc,
            &bytes,
            resources,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            0,
            &mut counters,
            &mut raw,
        );
    }
    let mut out = Vec::with_capacity(raw.len());
    for (placement, number) in raw {
        let Ok(id) = resolve_xref(doc, placement.xref) else {
            continue;
        };
        let Ok(object) = doc.get_object(id) else {
            continue;
        };
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        let Some(facts) = image_facts(doc, Some(id), &stream.dict, false) else {
            continue;
        };
        let encoded = stream.content.len();
        let (decoded_len, digest) = match decode_image_samples(doc, stream, &facts) {
            Ok(decoded) => {
                let digest = with_digest.then(|| md5::compute(&decoded.samples).0.to_vec());
                (decoded.samples.len(), digest)
            }
            Err(_) => (
                encoded,
                with_digest.then(|| md5::compute(&stream.content).0.to_vec()),
            ),
        };
        let size = if facts.filtered {
            decoded_len.saturating_add(encoded).saturating_add(347)
        } else {
            decoded_len.saturating_add(57)
        };
        let (bbox, transform) = placement_box(&frame, &placement.ctm);
        out.push(PlacedImageWithXref {
            image: PlacedImage {
                bbox,
                transform,
                width: facts.width,
                height: facts.height,
                bits: facts.bits,
                size: size.min(i32::MAX as usize) as i32,
                number,
                colorspace_name: facts.colorspace_name,
                has_mask: facts.mask != 0,
                xres: DEFAULT_RES,
                yres: DEFAULT_RES,
                digest,
            },
            xref: placement.xref,
            name: placement.name,
            comps: facts.comps,
        });
    }
    Ok(out)
}

/// Union box of a named image with its first placement matrix.
pub fn page_image_bbox_in(
    doc: &LopdfDocument,
    page: ObjectId,
    name: &str,
) -> Result<(Rect, Matrix)> {
    let placements = page_placements_in(doc, page, false)?;
    let mut bbox: Option<Rect> = None;
    let mut first: Option<Matrix> = None;
    for placement in placements.iter().filter(|p| p.name == name) {
        bbox = Some(match bbox {
            None => placement.image.bbox,
            Some(acc) => union_rect(acc, placement.image.bbox),
        });
        if first.is_none() {
            first = Some(placement.image.transform);
        }
    }
    match (bbox, first) {
        (Some(bbox), Some(matrix)) => Ok((bbox, matrix)),
        _ => Err(ErrorKind::BadValue.to_error(format!("unknown image '{name}'"))),
    }
}

/// Walk content operations, recording image placements in order.
///
/// Tracks the matrix stack, descends into form XObjects with their own
/// resources, and counts text blocks alongside placements for numbering.
/// Unparseable streams and unknown names are skipped, never fatal.
#[allow(clippy::too_many_arguments)]
fn walk_operations(
    doc: &LopdfDocument,
    bytes: &[u8],
    resources: Option<&Dictionary>,
    ctm: [f64; 6],
    depth: u32,
    counters: &mut WalkCounters,
    out: &mut Vec<(RawPlacement, i32)>,
) {
    if depth > 16 {
        return;
    }
    let Ok(content) = lopdf::content::Content::decode(bytes) else {
        return;
    };
    let mut stack: Vec<[f64; 6]> = Vec::new();
    let mut ctm = ctm;
    for op in &content.operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => {
                if let Some(top) = stack.pop() {
                    ctm = top;
                }
            }
            "cm" => {
                let nums: Vec<f64> = op
                    .operands
                    .iter()
                    .filter_map(|o| num(deref(doc, o)))
                    .collect();
                if nums.len() == 6 {
                    ctm =
                        concat_affine(ctm, [nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]]);
                }
            }
            "BT" => counters.texts += 1,
            "Do" => {
                let Some(name) = op
                    .operands
                    .first()
                    .and_then(|o| deref(doc, o).as_name().ok())
                else {
                    continue;
                };
                draw_xobject(doc, resources, name, &ctm, depth, counters, out);
            }
            _ => {}
        }
    }
}

/// Resolve one `Do` name: record images, descend into forms.
#[allow(clippy::too_many_arguments)]
fn draw_xobject(
    doc: &LopdfDocument,
    resources: Option<&Dictionary>,
    name: &[u8],
    ctm: &[f64; 6],
    depth: u32,
    counters: &mut WalkCounters,
    out: &mut Vec<(RawPlacement, i32)>,
) {
    let Some(resources) = resources else {
        return;
    };
    let Ok(xobjects) = resources.get(b"XObject") else {
        return;
    };
    let xobjects = deref(doc, xobjects);
    let Ok(entries) = xobjects.as_dict() else {
        return;
    };
    let Ok(target) = entries.get(name) else {
        return;
    };
    let Ok(id) = target.as_reference() else {
        return;
    };
    let Ok(object) = doc.get_object(id) else {
        return;
    };
    let Ok(stream) = object.as_stream() else {
        return;
    };
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| deref(doc, o).as_name().ok())
        .unwrap_or(b"");
    let label = String::from_utf8_lossy(name).into_owned();
    if subtype == b"Image" {
        let number = counters.images + counters.texts;
        counters.images += 1;
        out.push((
            RawPlacement {
                name: label,
                xref: id.0 as i32,
                ctm: *ctm,
            },
            number,
        ));
    } else if subtype == b"Form" {
        let nested = stream
            .dict
            .get(b"Resources")
            .ok()
            .map(|o| deref(doc, o).as_dict().ok())
            .unwrap_or(None)
            .or(Some(resources));
        let matrix = stream
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|o| num_array(doc, o, 6))
            .map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]])
            .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        walk_operations(
            doc,
            &bytes,
            nested,
            concat_affine(*ctm, matrix),
            depth + 1,
            counters,
            out,
        );
    }
}

/// Box and reported transform of one placement.
///
/// The reported matrix maps display unit squares (origin top-left) to
/// top-left page points: page flip after crop and rotation after the
/// placement matrix after a vertical flip into image space.
fn placement_box(frame: &PageFrame, ctm: &[f64; 6]) -> (Rect, Matrix) {
    let flip_v = [1.0, 0.0, 0.0, -1.0, 0.0, 1.0];
    let to_user = concat_affine(*ctm, flip_v);
    let to_display = concat_affine(frame.to_display, to_user);
    let flip_down = [1.0, 0.0, 0.0, -1.0, 0.0, frame.size.1];
    let full = concat_affine(flip_down, to_display);
    let corners = [
        apply_affine(&full, 0.0, 0.0),
        apply_affine(&full, 1.0, 0.0),
        apply_affine(&full, 0.0, 1.0),
        apply_affine(&full, 1.0, 1.0),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let bbox = Rect {
        x0: if x0.is_finite() { x0 } else { 0.0 },
        y0: if y0.is_finite() { y0 } else { 0.0 },
        x1: if x1.is_finite() { x1 } else { 0.0 },
        y1: if y1.is_finite() { y1 } else { 0.0 },
    };
    let clean = |v: f64| if v.is_finite() { v } else { 0.0 };
    let transform = Matrix {
        a: clean(full[0]),
        b: clean(full[1]),
        c: clean(full[2]),
        d: clean(full[3]),
        e: clean(full[4]),
        f: clean(full[5]),
    };
    (bbox, transform)
}

/// Decode a stored image to a pixmap, expanding palettes and stencils.
pub fn pixmap_from_image_in(doc: &LopdfDocument, xref: i32) -> Result<Pixmap> {
    let id = resolve_xref(doc, xref)?;
    let object = doc
        .get_object(id)
        .map_err(|_| ErrorKind::BadValue.to_error(format!("bad reference {xref}")))?;
    let stream = object
        .as_stream()
        .map_err(|_| ErrorKind::BadValue.to_error(format!("reference {xref} is not an image")))?;
    let facts = image_facts(doc, Some(id), &stream.dict, false)
        .ok_or_else(|| ErrorKind::BadValue.to_error(format!("reference {xref} is not an image")))?;
    let decoded = decode_image_samples(doc, stream, &facts)?;
    let colorspace = match decoded.comps {
        1 => Colorspace::Gray,
        4 => Colorspace::Cmyk,
        _ => Colorspace::Rgb,
    };
    let bounds = IRect {
        x0: 0,
        y0: 0,
        x1: facts.width,
        y1: facts.height,
    };
    Pixmap::from_samples(
        colorspace,
        bounds,
        false,
        decoded.samples,
        DEFAULT_RES,
        DEFAULT_RES,
    )
}

/// Extract one image: header facts plus encoded bytes.
///
/// JPEG payloads pass through untouched; everything else decodes and
/// re-encodes as PNG, preserving grey and stencil bit depth.
pub fn extract_image_in(doc: &LopdfDocument, xref: i32) -> Result<ImageData> {
    let id = resolve_xref(doc, xref)?;
    let object = doc
        .get_object(id)
        .map_err(|_| ErrorKind::BadValue.to_error(format!("bad reference {xref}")))?;
    let stream = object
        .as_stream()
        .map_err(|_| ErrorKind::BadValue.to_error(format!("reference {xref} is not an image")))?;
    let facts = image_facts(doc, Some(id), &stream.dict, false)
        .ok_or_else(|| ErrorKind::BadValue.to_error(format!("reference {xref} is not an image")))?;
    let (ext, data) = if facts.filter == "DCTDecode" {
        ("jpeg".to_string(), stream.content.clone())
    } else {
        let decoded = decode_image_samples(doc, stream, &facts)?;
        ("png".to_string(), encode_png_bytes(&facts, &decoded)?)
    };
    Ok(ImageData {
        width: facts.width,
        height: facts.height,
        bits: facts.bits,
        colorspace: facts.comps,
        colorspace_name: facts.colorspace_name,
        ext,
        size: data.len().min(i32::MAX as usize) as i32,
        masked: facts.mask != 0,
        xres: DEFAULT_RES,
        yres: DEFAULT_RES,
        data,
    })
}

/// Encode decoded samples as a PNG, keeping grey and stencil depth.
#[cfg(not(target_arch = "wasm32"))]
fn encode_png_bytes(facts: &ImageFacts, decoded: &DecodedSamples) -> Result<Vec<u8>> {
    use png::{BitDepth, ColorType, Encoder};
    let (w, h) = (facts.width.max(0) as u32, facts.height.max(0) as u32);
    let (color, depth, data) = if facts.stencil {
        let w = facts.width.max(0) as usize;
        let row_bytes = w.div_ceil(8);
        let mut packed = vec![0u8; row_bytes * facts.height.max(0) as usize];
        for (i, v) in decoded.samples.iter().enumerate() {
            if *v != 0 {
                let (row, col) = (i / w.max(1), i % w.max(1));
                if let Some(byte) = packed.get_mut(row * row_bytes + col / 8) {
                    *byte |= 0x80 >> (col % 8);
                }
            }
        }
        (ColorType::Grayscale, BitDepth::One, packed)
    } else if decoded.comps == 1 {
        (
            ColorType::Grayscale,
            BitDepth::Eight,
            decoded.samples.clone(),
        )
    } else if decoded.comps == 4 {
        let mut rgb = Vec::with_capacity(decoded.samples.len() / 4 * 3);
        for px in decoded.samples.chunks_exact(4) {
            rgb.extend_from_slice(&cmyk_to_rgb(px[0], px[1], px[2], px[3]));
        }
        (ColorType::Rgb, BitDepth::Eight, rgb)
    } else {
        (ColorType::Rgb, BitDepth::Eight, decoded.samples.clone())
    };
    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buf, w, h);
        encoder.set_color(color);
        encoder.set_depth(depth);
        let mut writer = encoder
            .write_header()
            .map_err(|e| ErrorKind::EngineFault.to_error(format!("png header failed: {e}")))?;
        writer
            .write_image_data(&data)
            .map_err(|e| ErrorKind::EngineFault.to_error(format!("png write failed: {e}")))?;
    }
    Ok(buf)
}

/// Render one page to a pixmap through the engine.
///
/// Renders at the requested resolution, converts into the target colour
/// space, then applies the matrix and clip in pixel space. Axis-aligned
/// matrices remap exactly; anything else resamples bilinearly.
pub fn render_pixmap(
    engine: &pdf_engine::PdfDocument,
    index: usize,
    options: &RenderOptions,
) -> Result<Pixmap> {
    use pdf_engine::render::{PixelFormat, RenderOptions as EngineRenderOptions};
    if index >= engine.page_count() {
        return Err(ErrorKind::BadValue.to_error(format!("bad page number {index}")));
    }
    let geometry = engine.page_geometry(index).map_err(map_engine_err)?;
    let (page_w, page_h) = geometry.effective_dimensions();
    if page_w <= 0.0 || page_h <= 0.0 || !page_w.is_finite() || !page_h.is_finite() {
        return Err(ErrorKind::EngineFault.to_error("page has no size to render"));
    }
    let dpi = options
        .dpi
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(72.0);
    let res = options
        .dpi
        .filter(|d| d.is_finite() && *d > 0.0)
        .map(|d| d.round() as i32)
        .unwrap_or(DEFAULT_RES);
    let target = options.colorspace.unwrap_or(Colorspace::Rgb);
    let base = if target == Colorspace::Cmyk && !options.alpha && options.annots {
        let dpi_u32 = dpi.round().clamp(1.0, f64::from(u32::MAX)) as u32;
        let rendered = engine
            .render_page_cmyk(index, dpi_u32)
            .map_err(map_engine_err)?;
        if !matches!(rendered.pixel_format, PixelFormat::Cmyk8) {
            return Err(ErrorKind::EngineFault.to_error("renderer answered in the wrong layout"));
        }
        pixmap_from_raw(
            Colorspace::Cmyk,
            false,
            rendered.width,
            rendered.height,
            rendered.pixels,
            res,
        )
    } else {
        let engine_opts = EngineRenderOptions {
            dpi,
            background: if options.alpha {
                [0.0, 0.0, 0.0, 0.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            },
            render_annotations: options.annots,
            ..Default::default()
        };
        let rendered = engine
            .render_page(index, &engine_opts)
            .map_err(map_engine_err)?;
        if !matches!(rendered.pixel_format, PixelFormat::Rgba8) {
            return Err(ErrorKind::EngineFault.to_error("renderer answered in the wrong layout"));
        }
        rgba_to_pixmap(
            rendered.width,
            rendered.height,
            rendered.pixels,
            target,
            options.alpha,
            res,
        )
    }?;
    apply_matrix_clip(base, &options.matrix, options.clip, page_w, page_h, res)
}

/// Map an engine failure onto a compat failure.
fn map_engine_err(error: pdf_engine::EngineError) -> crate::Error {
    match error {
        pdf_engine::EngineError::PageOutOfRange { index, count } => {
            ErrorKind::BadValue.to_error(format!("bad page number {index} of {count}"))
        }
        other => ErrorKind::EngineFault.to_error(other.to_string()),
    }
}

/// Wrap raw channel samples in a pixmap, padding or trimming defensively.
fn pixmap_from_raw(
    colorspace: Colorspace,
    alpha: bool,
    width: u32,
    height: u32,
    mut pixels: Vec<u8>,
    res: i32,
) -> Result<Pixmap> {
    let (w, h) = (
        width.min(i32::MAX as u32) as i32,
        height.min(i32::MAX as u32) as i32,
    );
    let mut pixmap = Pixmap::blank(colorspace, 0, 0, w, h, alpha)?;
    pixmap.xres = res;
    pixmap.yres = res;
    pixels.resize(pixmap.samples.len(), 0);
    let want = pixmap.samples.len();
    pixmap.samples.copy_from_slice(&pixels[..want]);
    Ok(pixmap)
}

/// Premultiplied engine RGBA into a target-space pixmap.
fn rgba_to_pixmap(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    target: Colorspace,
    alpha: bool,
    res: i32,
) -> Result<Pixmap> {
    let (w, h) = (
        width.min(i32::MAX as u32) as i32,
        height.min(i32::MAX as u32) as i32,
    );
    let mut pixmap = Pixmap::blank(target, 0, 0, w, h, alpha)?;
    pixmap.xres = res;
    pixmap.yres = res;
    let n = pixmap.channels();
    let count = (w.max(0) as usize) * (h.max(0) as usize);
    for i in 0..count {
        let src = pixels.get(i * 4..i * 4 + 4).unwrap_or(&[0, 0, 0, 0]);
        let a = u32::from(src[3]);
        let mut triple = [src[0], src[1], src[2]];
        if a > 0 && a < 255 {
            for v in triple.iter_mut() {
                *v = ((u32::from(*v) * 255 + a / 2) / a).min(255) as u8;
            }
        } else if a == 0 {
            triple = [0, 0, 0];
        }
        let base = i * n;
        match target {
            Colorspace::Gray => pixmap.samples[base] = luma(triple[0], triple[1], triple[2]),
            Colorspace::Rgb => pixmap.samples[base..base + 3].copy_from_slice(&triple),
            Colorspace::Cmyk => {
                pixmap.samples[base..base + 4]
                    .copy_from_slice(&rgb_to_cmyk(triple[0], triple[1], triple[2]));
            }
        }
        if alpha {
            pixmap.samples[base + n - 1] = a.min(255) as u8;
        }
    }
    Ok(pixmap)
}

/// Apply the render matrix and clip to a rendered pixmap.
fn apply_matrix_clip(
    base: Pixmap,
    matrix: &Matrix,
    clip: Option<Rect>,
    page_w: f64,
    page_h: f64,
    res: i32,
) -> Result<Pixmap> {
    let identity = matrix.a == 1.0
        && matrix.b == 0.0
        && matrix.c == 0.0
        && matrix.d == 1.0
        && matrix.e == 0.0
        && matrix.f == 0.0;
    if identity && clip.is_none() {
        return Ok(base);
    }
    let (w0, h0) = (f64::from(base.width), f64::from(base.height));
    let m = [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f];
    let corners = [(0.0, 0.0), (w0, 0.0), (0.0, h0), (w0, h0)].map(|(x, y)| apply_affine(&m, x, y));
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if let Some(clip) = clip {
        let sx = if page_w > 0.0 { w0 / page_w } else { 1.0 };
        let sy = if page_h > 0.0 { h0 / page_h } else { 1.0 };
        let cx0 = (clip.x0 * sx).floor();
        let cy0 = (clip.y0 * sy).floor();
        let cx1 = (clip.x1 * sx).ceil();
        let cy1 = (clip.y1 * sy).ceil();
        let clipped =
            [(cx0, cy0), (cx1, cy0), (cx0, cy1), (cx1, cy1)].map(|(x, y)| apply_affine(&m, x, y));
        let (mut kx0, mut ky0, mut kx1, mut ky1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (x, y) in clipped {
            kx0 = kx0.min(x);
            ky0 = ky0.min(y);
            kx1 = kx1.max(x);
            ky1 = ky1.max(y);
        }
        x0 = x0.max(kx0);
        y0 = y0.max(ky0);
        x1 = x1.min(kx1);
        y1 = y1.min(ky1);
    }
    let (ow, oh) = ((x1 - x0).round() as i32, (y1 - y0).round() as i32);
    if ow <= 0 || oh <= 0 || !x0.is_finite() {
        let mut empty = Pixmap::blank(base.colorspace, 0, 0, 0, 0, base.alpha)?;
        empty.xres = res;
        empty.yres = res;
        return Ok(empty);
    }
    let mut out = Pixmap::blank(base.colorspace, 0, 0, ow, oh, base.alpha)?;
    out.xres = res;
    out.yres = res;
    if matrix.b == 0.0 && matrix.c == 0.0 {
        remap_axis_aligned(&base, &mut out, m, x0, y0, false);
    } else if matrix.a == 0.0 && matrix.d == 0.0 {
        remap_axis_aligned(&base, &mut out, m, x0, y0, true);
    } else {
        remap_bilinear(&base, &mut out, m, x0, y0);
    }
    Ok(out)
}

/// Outside fill for resampled renders: paper white, or transparent.
fn render_fill(colorspace: Colorspace, alpha: bool) -> Vec<u8> {
    let mut fill = match colorspace {
        Colorspace::Gray => vec![255],
        Colorspace::Rgb => vec![255, 255, 255],
        Colorspace::Cmyk => vec![0, 0, 0, 0],
    };
    if alpha {
        fill.push(0);
    }
    fill
}

/// Exact remap for axis-aligned matrices: scales, flips, quarter turns.
fn remap_axis_aligned(
    base: &Pixmap,
    out: &mut Pixmap,
    m: [f64; 6],
    ox: f64,
    oy: f64,
    swapped: bool,
) {
    let n = out.channels();
    let fill = render_fill(out.colorspace, out.alpha);
    for y in 0..out.height {
        for x in 0..out.width {
            let (sx, sy) = if swapped {
                (
                    (f64::from(y) + oy - m[5]) / m[1],
                    (f64::from(x) + ox - m[4]) / m[2],
                )
            } else {
                (
                    (f64::from(x) + ox - m[4]) / m[0],
                    (f64::from(y) + oy - m[5]) / m[3],
                )
            };
            let (ix, iy) = (sx.round() as i32, sy.round() as i32);
            let dst = (y as usize * out.width as usize + x as usize) * n;
            match base.offset(ix, iy) {
                Some(src) => out.samples[dst..dst + n].copy_from_slice(&base.samples[src..src + n]),
                None => out.samples[dst..dst + n].copy_from_slice(&fill[..n.min(fill.len())]),
            }
        }
    }
}

/// Bilinear remap for general matrices, filling outside paper-white.
fn remap_bilinear(base: &Pixmap, out: &mut Pixmap, m: [f64; 6], ox: f64, oy: f64) {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 {
        return;
    }
    let inv = [
        m[3] / det,
        -m[1] / det,
        -m[2] / det,
        m[0] / det,
        (m[2] * m[5] - m[3] * m[4]) / det,
        (m[1] * m[4] - m[0] * m[5]) / det,
    ];
    let n = out.channels();
    let fill = render_fill(out.colorspace, out.alpha);
    for y in 0..out.height {
        for x in 0..out.width {
            let (sx, sy) = apply_affine(&inv, f64::from(x) + ox + 0.5, f64::from(y) + oy + 0.5);
            let dst = (y as usize * out.width as usize + x as usize) * n;
            if sx < 0.0 || sy < 0.0 || sx >= f64::from(base.width) || sy >= f64::from(base.height) {
                out.samples[dst..dst + n].copy_from_slice(&fill[..n.min(fill.len())]);
                continue;
            }
            for c in 0..n {
                out.samples[dst + c] =
                    bilinear(base, c, sx - 0.5, sy - 0.5, base.width, base.height);
            }
        }
    }
}

/// SVG depiction of a page: the render embedded as a PNG data URI.
///
/// A genuine raster depiction, not a vector trace: callers that need text
/// as paths must look elsewhere, and `text_as_path` changes nothing here.
pub fn svg_for_page(
    engine: &pdf_engine::PdfDocument,
    index: usize,
    matrix: Option<Matrix>,
    text_as_path: bool,
) -> Result<String> {
    let _ = text_as_path;
    let options = RenderOptions {
        matrix: matrix.unwrap_or(Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }),
        dpi: None,
        colorspace: None,
        clip: None,
        alpha: false,
        annots: true,
    };
    let pixmap = render_pixmap(engine, index, &options)?;
    let png = pixmap.tobytes("png", 95)?;
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let (w, h) = (pixmap.width, pixmap.height);
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\"><image width=\"{w}\" height=\"{h}\" \
         href=\"data:image/png;base64,{}\"/></svg>",
        B64.encode(&png)
    ))
}

/// Rewrite images in place: downsample, grey-convert, and re-encode.
///
/// Collects every image XObject, measures its sharpest placement, then
/// rewrites only what the options select. Stencils are left alone, and an
/// image that fails to decode is skipped rather than failing the whole
/// pass; already-lossy images re-encode even under default options.
pub fn rewrite_images_in(doc: &mut LopdfDocument, options: &RewriteOptions) -> Result<()> {
    let mut ids: Vec<ObjectId> = Vec::new();
    for (id, object) in doc.objects.iter() {
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| deref(doc, o).as_name().ok())
            .unwrap_or(b"");
        if subtype != b"Image" {
            continue;
        }
        let stencil = stream
            .dict
            .get(b"ImageMask")
            .ok()
            .and_then(|o| deref(doc, o).as_bool().ok())
            .unwrap_or(false);
        if !stencil {
            ids.push(*id);
        }
    }
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut dpi: HashMap<i32, f64> = HashMap::new();
    for page in pages {
        let Ok(placements) = page_placements_in(doc, page, false) else {
            continue;
        };
        for placement in placements {
            let bw = (placement.image.bbox.x1 - placement.image.bbox.x0)
                .abs()
                .max(0.01);
            let bh = (placement.image.bbox.y1 - placement.image.bbox.y0)
                .abs()
                .max(0.01);
            let d = (f64::from(placement.image.width) * 72.0 / bw)
                .max(f64::from(placement.image.height) * 72.0 / bh);
            dpi.entry(placement.xref)
                .and_modify(|e| *e = e.max(d))
                .or_insert(d);
        }
    }
    for id in ids {
        rewrite_one_image(doc, id, options, dpi.get(&(id.0 as i32)).copied());
    }
    Ok(())
}

/// Rewrite one image XObject in place, skipping whatever gates it misses.
fn rewrite_one_image(
    doc: &mut LopdfDocument,
    id: ObjectId,
    options: &RewriteOptions,
    dpi: Option<f64>,
) {
    let Ok(object) = doc.get_object(id) else {
        return;
    };
    let Ok(stream) = object.as_stream() else {
        return;
    };
    let Some(facts) = image_facts(doc, Some(id), &stream.dict, false) else {
        return;
    };
    let Ok(decoded) = decode_image_samples(doc, stream, &facts) else {
        return;
    };
    let lossy = matches!(facts.filter.as_str(), "DCTDecode" | "JPXDecode");
    let bitonal = facts.bits == 1;
    let color = decoded.comps > 1;
    if (lossy && !options.lossy)
        || (!lossy && !options.lossless)
        || (bitonal && !options.bitonal)
        || (color && !options.color)
        || (!color && !bitonal && !options.gray)
    {
        return;
    }
    // Recorded shape (l3-doc-rewrite-images): the threshold gates
    // downsampling alone; already-lossy images re-encode whenever their
    // kind is selected, at quality 60 when no quality is given.
    let downsample = options.dpi_target > 0
        && dpi.is_some_and(|d| {
            d > f64::from(options.dpi_target)
                && options.dpi_threshold.map_or(true, |t| d > f64::from(t))
        });
    let to_gray = options.set_to_gray && color;
    let requality = lossy;
    if !(downsample || to_gray || requality) {
        return;
    }
    let (mut w, mut h) = (facts.width.max(1) as usize, facts.height.max(1) as usize);
    let mut samples = decoded.samples;
    let mut comps = decoded.comps.max(1) as usize;
    if downsample {
        let factor =
            f64::from(options.dpi_target) / dpi.unwrap_or(f64::from(options.dpi_target)).max(0.01);
        let (nw, nh) = (
            ((w as f64 * factor).round() as usize).clamp(1, w),
            ((h as f64 * factor).round() as usize).clamp(1, h),
        );
        if (nw, nh) != (w, h) {
            samples = resample_box(&samples, w, h, comps, nw, nh);
            (w, h) = (nw, nh);
        }
    }
    if to_gray {
        samples = samples
            .chunks_exact(comps)
            .map(|px| {
                let (r, g, b) = match comps {
                    1 => (px[0], px[0], px[0]),
                    4 => {
                        let rgb = cmyk_to_rgb(px[0], px[1], px[2], px[3]);
                        (rgb[0], rgb[1], rgb[2])
                    }
                    _ => (
                        px[0],
                        px.get(1).copied().unwrap_or(0),
                        px.get(2).copied().unwrap_or(0),
                    ),
                };
                luma(r, g, b)
            })
            .collect();
        comps = 1;
    }
    let as_jpeg = lossy;
    let quality = if options.quality > 0 {
        options.quality.clamp(1, 100) as u8
    } else {
        60
    };
    let content = if as_jpeg {
        match encode_jpeg_bytes(&samples, w as u32, h as u32, comps == 1, quality) {
            Ok(bytes) => bytes,
            Err(_) => return,
        }
    } else {
        samples.clone()
    };
    // Resample the mask first: a mask that cannot follow aborts the image.
    if facts.mask != 0
        && (w, h) != (facts.width.max(1) as usize, facts.height.max(1) as usize)
        && resample_mask(doc, facts.mask, w, h).is_err()
    {
        return;
    }
    let Ok(object) = doc.get_object_mut(id) else {
        return;
    };
    let Object::Stream(stream) = object else {
        return;
    };
    stream.content = content;
    stream.dict.set(&b"Width"[..], Object::Integer(w as i64));
    stream.dict.set(&b"Height"[..], Object::Integer(h as i64));
    stream
        .dict
        .set(&b"BitsPerComponent"[..], Object::Integer(8));
    let space = if comps == 1 {
        "DeviceGray"
    } else {
        "DeviceRGB"
    };
    stream
        .dict
        .set(&b"ColorSpace"[..], Object::Name(space.as_bytes().to_vec()));
    if as_jpeg {
        stream
            .dict
            .set(&b"Filter"[..], Object::Name(b"DCTDecode".to_vec()));
    } else {
        stream.dict.remove(b"Filter");
    }
    stream.dict.remove(b"DecodeParms");
    stream.dict.remove(b"Decode");
}

/// Resample a soft mask to new dimensions, keeping it raw grey.
fn resample_mask(doc: &mut LopdfDocument, mask: i32, w: usize, h: usize) -> Result<()> {
    let id = resolve_xref(doc, mask)?;
    let (mw, mh, samples) = {
        let object = doc
            .get_object(id)
            .map_err(|_| ErrorKind::EngineFault.to_error("mask went missing"))?;
        let stream = object
            .as_stream()
            .map_err(|_| ErrorKind::EngineFault.to_error("mask is not a stream"))?;
        let Some(facts) = image_facts(doc, Some(id), &stream.dict, false) else {
            return Err(ErrorKind::EngineFault.to_error("mask is not an image"));
        };
        let decoded = decode_image_samples(doc, stream, &facts)?;
        let gray: Vec<u8> = match decoded.comps {
            1 => decoded.samples,
            _ => {
                let comps = decoded.comps.max(1) as usize;
                decoded
                    .samples
                    .chunks_exact(comps)
                    .map(|px| {
                        luma(
                            px[0],
                            px.get(1).copied().unwrap_or(0),
                            px.get(2).copied().unwrap_or(0),
                        )
                    })
                    .collect()
            }
        };
        (
            facts.width.max(1) as usize,
            facts.height.max(1) as usize,
            gray,
        )
    };
    let resized = resample_box(&samples, mw, mh, 1, w, h);
    let object = doc
        .get_object_mut(id)
        .map_err(|_| ErrorKind::EngineFault.to_error("mask went missing"))?;
    let Object::Stream(stream) = object else {
        return Err(ErrorKind::EngineFault.to_error("mask is not a stream"));
    };
    stream.content = resized;
    stream.dict.set(&b"Width"[..], Object::Integer(w as i64));
    stream.dict.set(&b"Height"[..], Object::Integer(h as i64));
    stream
        .dict
        .set(&b"BitsPerComponent"[..], Object::Integer(8));
    stream
        .dict
        .set(&b"ColorSpace"[..], Object::Name(b"DeviceGray".to_vec()));
    stream.dict.remove(b"Filter");
    stream.dict.remove(b"DecodeParms");
    stream.dict.remove(b"Decode");
    Ok(())
}

/// Box-average resample to new dimensions, rounding half up.
fn resample_box(samples: &[u8], w: usize, h: usize, comps: usize, nw: usize, nh: usize) -> Vec<u8> {
    if (nw, nh) == (w, h) || w == 0 || h == 0 || nw == 0 || nh == 0 || comps == 0 {
        return samples.to_vec();
    }
    let comps = comps.max(1);
    let mut out = vec![0u8; nw * nh * comps];
    for oy in 0..nh {
        let y0 = oy * h / nh;
        let y1 = ((oy + 1) * h / nh).max(y0 + 1).min(h);
        for ox in 0..nw {
            let x0 = ox * w / nw;
            let x1 = ((ox + 1) * w / nw).max(x0 + 1).min(w);
            let mut acc = vec![0u64; comps];
            let mut count = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let base = (y * w + x) * comps;
                    for (slot, v) in acc.iter_mut().zip(samples[base..base + comps].iter()) {
                        *slot += u64::from(*v);
                    }
                    count += 1;
                }
            }
            let base = (oy * nw + ox) * comps;
            for (slot, total) in out[base..base + comps].iter_mut().zip(acc.iter()) {
                *slot = if count == 0 {
                    0
                } else {
                    ((total + count / 2) / count).min(255) as u8
                };
            }
        }
    }
    out
}

/// Encode raw samples as a JPEG at the given quality.
#[cfg(not(target_arch = "wasm32"))]
fn encode_jpeg_bytes(samples: &[u8], w: u32, h: u32, gray: bool, quality: u8) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    let color = if gray {
        image::ExtendedColorType::L8
    } else {
        image::ExtendedColorType::Rgb8
    };
    encoder
        .write_image(samples, w, h, color)
        .map_err(|e| ErrorKind::EngineFault.to_error(format!("jpeg encoding failed: {e}")))?;
    Ok(buf)
}

/// WebAssembly fallbacks for the native-only codecs.
#[cfg(target_arch = "wasm32")]
impl Pixmap {
    fn encode_png(&self) -> Result<Vec<u8>> {
        Err(ErrorKind::EngineFault.to_error("png encoding is not available in WebAssembly builds"))
    }

    fn encode_jpeg(&self, _jpg_quality: i32) -> Result<Vec<u8>> {
        Err(ErrorKind::EngineFault.to_error("jpeg encoding is not available in WebAssembly builds"))
    }

    fn decode_png(_data: &[u8]) -> Result<Self> {
        Err(ErrorKind::UnreadableFile
            .to_error("png decoding is not available in WebAssembly builds"))
    }

    fn decode_jpeg(_data: &[u8]) -> Result<Self> {
        Err(ErrorKind::UnreadableFile
            .to_error("jpeg decoding is not available in WebAssembly builds"))
    }
}

/// WebAssembly fallback for extraction PNG encoding.
#[cfg(target_arch = "wasm32")]
fn encode_png_bytes(_facts: &ImageFacts, _decoded: &DecodedSamples) -> Result<Vec<u8>> {
    Err(ErrorKind::EngineFault.to_error("png encoding is not available in WebAssembly builds"))
}

/// WebAssembly fallback for rewrite JPEG encoding.
#[cfg(target_arch = "wasm32")]
fn encode_jpeg_bytes(
    _samples: &[u8],
    _w: u32,
    _h: u32,
    _gray: bool,
    _quality: u8,
) -> Result<Vec<u8>> {
    Err(ErrorKind::EngineFault.to_error("jpeg encoding is not available in WebAssembly builds"))
}

/// WebAssembly stand-in for the native-only `image` crate, so the DCTDecode
/// branch of `decode_image_samples` compiles on wasm32 unchanged: every JPEG
/// payload is refused there, which that branch reports as `UnreadableFile`,
/// the error kind of the other wasm32 decode fallbacks above.
#[cfg(target_arch = "wasm32")]
mod image {
    pub(super) enum ImageFormat {
        Jpeg,
    }

    pub(super) struct NotAvailable;

    /// Never constructed: `load_from_memory_with_format` always refuses.
    pub(super) struct Decoded(Vec<u8>);

    impl Decoded {
        pub(super) fn to_rgb8(&self) -> Decoded {
            Decoded(self.0.clone())
        }

        pub(super) fn into_raw(self) -> Vec<u8> {
            self.0
        }
    }

    pub(super) fn load_from_memory_with_format(
        _data: &[u8],
        _format: ImageFormat,
    ) -> Result<Decoded, NotAvailable> {
        Err(NotAvailable)
    }
}
