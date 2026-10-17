//! Optional vision layer: page raster plus ONNX runner (#636).
//!
//! Behind the `vision` cargo feature (off by default). No trained model ships;
//! a separate team plugs models in later. This module is infrastructure only:
//!
//! * [`rasterize_page_with_dpi`] renders a page through the engine's existing
//!   renderer ([`crate::PdfDocument::render_page`]) and converts the RGBA
//!   output to grayscale or RGB bytes. No new renderer.
//! * [`VisionRunner`] loads an ONNX model from bytes or a path with
//!   `tract-onnx` (MIT OR Apache-2.0) and runs it on a [`VisionRaster`] or on
//!   a crop named by a bbox in page points, returning the output tensor as
//!   [`VisionOutput`].
//!
//! Input tensors are NCHW `f32` in 0.0..=1.0 (bytes divided by 255). Output is
//! the model's first output, which must be `f32`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::Path;

use tract_onnx::prelude::{tvec, Framework as _, IntoTValue as _};

use crate::{PdfDocument, RenderOptions};

/// Default rasterization resolution for vision input, in dots per inch.
///
/// 72 dpi is 1:1 with PDF points, so a US Letter page (612x792 pt) rasterizes
/// to 612x792 pixels.
pub const DEFAULT_VISION_DPI: u32 = 72;

/// Pixel layout of a [`VisionRaster`].
///
/// `#[non_exhaustive]`: new layouts land in a published crate without
/// breaking callers that match with a fallback arm.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionPixelFormat {
    /// One byte per pixel, ITU-R BT.601 luma (0 = black, 255 = white).
    Gray8,
    /// Three bytes per pixel, interleaved RGB, row-major.
    Rgb8,
}

impl VisionPixelFormat {
    /// Bytes per pixel.
    pub fn channels(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Rgb8 => 3,
        }
    }
}

/// A page (or crop) rasterized for vision input, row-major.
#[derive(Debug, Clone)]
pub struct VisionRaster {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel layout of [`Self::data`].
    pub format: VisionPixelFormat,
    /// Pixel bytes, row-major, top-left origin.
    pub data: Vec<u8>,
}

impl VisionRaster {
    /// Crop a pixel rectangle out of this raster (top-left origin).
    ///
    /// Returns `None` when the rectangle is empty or lies outside the raster.
    pub fn crop_pixels(&self, x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let (x_end, y_end) = (x.checked_add(width)?, y.checked_add(height)?);
        if x_end > self.width || y_end > self.height {
            return None;
        }
        let ch = self.format.channels();
        let stride = self.width as usize * ch;
        let mut data = Vec::with_capacity(width as usize * height as usize * ch);
        for row in y..y_end {
            let start = row as usize * stride + x as usize * ch;
            data.extend_from_slice(&self.data[start..start + width as usize * ch]);
        }
        Some(Self {
            width,
            height,
            format: self.format,
            data,
        })
    }

    /// Crop a bbox in page points out of this raster.
    ///
    /// `bbox` is `[x0, y0, x1, y1]` in PDF points with the origin at the
    /// bottom-left (the [`crate::PageBox`] convention). `page_size_points` is
    /// the page's `(width, height)` in points, normally
    /// [`crate::PageGeometry::effective_dimensions`]. The mapping is
    /// `pixels = points * dpi / 72`, with the Y axis flipped to the raster's
    /// top-left origin. The bbox is clamped to the page; an empty intersection
    /// is an error.
    pub fn crop_points(
        &self,
        bbox: [f64; 4],
        page_size_points: (f64, f64),
        dpi: u32,
    ) -> VisionResult<Self> {
        let scale = f64::from(dpi) / 72.0;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(VisionError::InvalidCrop(format!(
                "dpi must be positive, got {dpi}"
            )));
        }
        let (page_w, page_h) = page_size_points;
        let x0 = bbox[0].min(bbox[2]).clamp(0.0, page_w);
        let x1 = bbox[0].max(bbox[2]).clamp(0.0, page_w);
        let y0 = bbox[1].min(bbox[3]).clamp(0.0, page_h);
        let y1 = bbox[1].max(bbox[3]).clamp(0.0, page_h);
        let px0 = (x0 * scale).floor().max(0.0) as u32;
        let px1 = (x1 * scale).ceil().max(0.0) as u32;
        // Bottom-origin pixel rows, then flipped to the raster's top origin.
        let py0 = (y0 * scale).floor().max(0.0) as u32;
        let py1 = (y1 * scale).ceil().max(0.0) as u32;
        let px1 = px1.min(self.width);
        let py1 = py1.min(self.height);
        if px1 <= px0 || py1 <= py0 {
            return Err(VisionError::InvalidCrop(format!(
                "bbox {bbox:?} covers no pixels at {dpi} dpi"
            )));
        }
        let y_top = self.height - py1;
        let height = py1 - py0;
        self.crop_pixels(px0, y_top, px1 - px0, height)
            .ok_or_else(|| {
                VisionError::InvalidCrop(format!("bbox {bbox:?} lies outside the raster"))
            })
    }

    /// Convert to an NCHW `f32` input tensor in 0.0..=1.0.
    fn to_input_tensor(&self) -> tract_onnx::prelude::Tensor {
        let (w, h) = (self.width as usize, self.height as usize);
        let ch = self.format.channels();
        let mut planar = vec![0.0f32; w * h * ch];
        match self.format {
            VisionPixelFormat::Gray8 => {
                for (dst, src) in planar.iter_mut().zip(self.data.iter()) {
                    *dst = f32::from(*src) / 255.0;
                }
            }
            VisionPixelFormat::Rgb8 => {
                // Interleaved RGB -> planar NCHW.
                let plane = w * h;
                for (i, px) in self.data.chunks_exact(3).enumerate() {
                    planar[i] = f32::from(px[0]) / 255.0;
                    planar[plane + i] = f32::from(px[1]) / 255.0;
                    planar[2 * plane + i] = f32::from(px[2]) / 255.0;
                }
            }
        }
        tract_onnx::prelude::Tensor::from_shape(&[1, ch, h, w], &planar)
            .expect("shape matches buffer length")
    }
}

/// Errors from the vision layer.
///
/// `#[non_exhaustive]`: new failure modes land in a published crate
/// without breaking callers that match with a fallback arm.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// The page could not be rendered.
    #[error("vision render failed: {0}")]
    Render(#[from] crate::EngineError),
    /// The ONNX model could not be loaded.
    #[error("vision model failed to load: {0}")]
    Model(String),
    /// Inference failed.
    #[error("vision inference failed: {0}")]
    Inference(String),
    /// The model's output is missing or not `f32`.
    #[error("vision output unsupported: {0}")]
    Output(String),
    /// The crop bbox covers no pixels.
    #[error("vision crop invalid: {0}")]
    InvalidCrop(String),
}

/// Convenience alias for vision operations.
pub type VisionResult<T> = std::result::Result<T, VisionError>;

/// Rasterize a page to a grayscale or RGB buffer at [`DEFAULT_VISION_DPI`].
pub fn rasterize_page(
    doc: &PdfDocument,
    page: usize,
    format: VisionPixelFormat,
) -> VisionResult<VisionRaster> {
    rasterize_page_with_dpi(doc, page, DEFAULT_VISION_DPI, format)
}

/// Rasterize a page to a grayscale or RGB buffer at the given dpi.
///
/// Uses the engine's existing renderer ([`PdfDocument::render_page`]); the
/// RGBA output is composited over white and converted (BT.601 luma for
/// [`VisionPixelFormat::Gray8`], via [`crate::document::gray_page`], the one
/// conversion the dump's image arm also reads).
pub fn rasterize_page_with_dpi(
    doc: &PdfDocument,
    page: usize,
    dpi: u32,
    format: VisionPixelFormat,
) -> VisionResult<VisionRaster> {
    if format == VisionPixelFormat::Gray8 {
        let (width, height, data) = crate::document::gray_page(doc, page, dpi)?;
        return Ok(VisionRaster {
            width,
            height,
            format,
            data,
        });
    }
    let options = RenderOptions {
        dpi: f64::from(dpi),
        ..Default::default()
    };
    let rendered = doc.render_page(page, &options)?;
    let mut data = Vec::with_capacity(rendered.width as usize * rendered.height as usize * 3);
    for px in rendered.pixels.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        // Composite over white; the default background is opaque white so
        // alpha is 255 in practice, but never trust it.
        let alpha = f32::from(a) / 255.0;
        let blend = |c: u8| (f32::from(c) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        data.extend_from_slice(&[blend(r), blend(g), blend(b)]);
    }
    Ok(VisionRaster {
        width: rendered.width,
        height: rendered.height,
        format,
        data,
    })
}

/// The output tensor of one model run: shape plus row-major `f32` data.
#[derive(Debug, Clone)]
pub struct VisionOutput {
    /// Tensor shape, e.g. `[1, 1, 612, 792]` for an identity over NCHW input.
    pub shape: Vec<usize>,
    /// Tensor data, row-major.
    pub data: Vec<f32>,
}

/// An ONNX model loaded from bytes or a path, runnable on a [`VisionRaster`].
pub struct VisionRunner {
    model: std::sync::Arc<tract_onnx::prelude::TypedRunnableModel>,
}

impl VisionRunner {
    /// Load a model from ONNX bytes (e.g. `include_bytes!`).
    pub fn from_bytes(bytes: &[u8]) -> VisionResult<Self> {
        let mut cursor = std::io::Cursor::new(bytes);
        let model = tract_onnx::onnx()
            .model_for_read(&mut cursor)
            .map_err(|e| VisionError::Model(e.to_string()))?;
        Self::from_inference_model(model)
    }

    /// Load a model from an `.onnx` file path.
    pub fn from_path(path: impl AsRef<Path>) -> VisionResult<Self> {
        let model = tract_onnx::onnx()
            .model_for_path(path.as_ref())
            .map_err(|e| VisionError::Model(e.to_string()))?;
        Self::from_inference_model(model)
    }

    fn from_inference_model(model: tract_onnx::prelude::InferenceModel) -> VisionResult<Self> {
        use tract_onnx::prelude::{InferenceModelExt as _, IntoRunnable as _};
        let typed = model
            .into_optimized()
            .map_err(|e| VisionError::Model(e.to_string()))?;
        let runnable = typed
            .into_runnable()
            .map_err(|e| VisionError::Model(e.to_string()))?;
        Ok(Self { model: runnable })
    }

    /// Run the model on a raster, returning the first output tensor.
    pub fn run(&self, raster: &VisionRaster) -> VisionResult<VisionOutput> {
        let input = raster.to_input_tensor().into_tvalue();
        let outputs = self
            .model
            .run(tvec!(input))
            .map_err(|e| VisionError::Inference(e.to_string()))?;
        let tensor = outputs
            .first()
            .ok_or_else(|| VisionError::Output("model returned no outputs".into()))?;
        let view = tensor
            .to_plain_array_view::<f32>()
            .map_err(|e| VisionError::Output(e.to_string()))?;
        Ok(VisionOutput {
            shape: view.shape().to_vec(),
            data: view.iter().copied().collect(),
        })
    }

    /// Rasterize a page, crop a bbox in page points, and run the model on it.
    ///
    /// `bbox` follows [`VisionRaster::crop_points`]; the page size comes from
    /// the document's own geometry.
    pub fn run_on_crop(
        &self,
        doc: &PdfDocument,
        page: usize,
        bbox: [f64; 4],
        dpi: u32,
        format: VisionPixelFormat,
    ) -> VisionResult<VisionOutput> {
        let raster = rasterize_page_with_dpi(doc, page, dpi, format)?;
        let geometry = doc.page_geometry(page)?;
        let (w, h) = geometry.effective_dimensions();
        let crop = raster.crop_points(bbox, (w, h), dpi)?;
        self.run(&crop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_match_layout() {
        assert_eq!(VisionPixelFormat::Gray8.channels(), 1);
        assert_eq!(VisionPixelFormat::Rgb8.channels(), 3);
    }

    #[test]
    fn default_dpi_is_72() {
        assert_eq!(DEFAULT_VISION_DPI, 72);
    }

    #[test]
    fn pixel_crop_copies_rows() {
        let raster = VisionRaster {
            width: 4,
            height: 4,
            format: VisionPixelFormat::Gray8,
            data: (0..16u8).collect(),
        };
        let crop = raster.crop_pixels(1, 1, 2, 2).expect("inside");
        assert_eq!(crop.width, 2);
        assert_eq!(crop.height, 2);
        assert_eq!(crop.data, vec![5, 6, 9, 10]);
    }

    #[test]
    fn point_crops_name_the_pixels_the_image_arm_reads() {
        // `learned::raster` reuses this arithmetic at 72 dpi without the vision
        // feature; the two must name the same pixels (#574).
        let raster = VisionRaster {
            width: 100,
            height: 40,
            format: VisionPixelFormat::Gray8,
            data: (0..4000u32).map(|i| (i % 251) as u8).collect(),
        };
        let gray = crate::learned::raster::GrayPage {
            width: 100,
            height: 40,
            data: &raster.data,
        };
        for bbox in [
            [10.0, 10.0, 50.0, 20.0],
            [0.0, 0.0, 100.0, 40.0],
            [10.5, 9.2, 49.7, 20.8],
            [50.0, 20.0, 10.0, 10.0],
            [-5.0, -5.0, 20.0, 20.0],
        ] {
            let crop = raster
                .crop_points(bbox, (100.0, 40.0), 72)
                .expect("an in-page box crops");
            let (c0, c1, r0, r1) = crate::learned::raster::pixel_box(bbox, &gray, 40.0);
            assert_eq!((crop.width, crop.height), (c1 - c0, r1 - r0), "{bbox:?}");
            let mut want = Vec::with_capacity(crop.data.len());
            for r in r0..r1 {
                let row = r as usize * 100;
                want.extend_from_slice(&raster.data[row + c0 as usize..row + c1 as usize]);
            }
            assert_eq!(crop.data, want, "{bbox:?}");
        }
        // Off the page: an error here, an empty rect there.
        assert!(raster
            .crop_points([200.0, 200.0, 300.0, 300.0], (100.0, 40.0), 72)
            .is_err());
        let (c0, c1, r0, r1) =
            crate::learned::raster::pixel_box([200.0, 200.0, 300.0, 300.0], &gray, 40.0);
        assert!(c0 == c1 || r0 == r1);
    }

    #[test]
    fn pixel_crop_outside_is_none() {
        let raster = VisionRaster {
            width: 2,
            height: 2,
            format: VisionPixelFormat::Gray8,
            data: vec![0; 4],
        };
        assert!(raster.crop_pixels(0, 0, 3, 1).is_none());
        assert!(raster.crop_pixels(1, 1, 2, 2).is_none());
        assert!(raster.crop_pixels(0, 0, 0, 1).is_none());
    }
}
