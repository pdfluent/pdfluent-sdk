//! Page rendering with z-order compositing.
//!
//! Renders a PDF page to RGBA pixel data using the hayro rendering stack.

use pdf_render::pdf_interpret::InterpreterSettings;
use pdf_render::pdf_syntax::page::Page;
use pdf_render::vello_cpu::color::palette::css::WHITE;
use pdf_render::vello_cpu::color::{AlphaColor, Srgb};
use pdf_render::{render, RenderSettings};

/// Color space handling during rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Convert all colors to sRGB (default).
    #[default]
    Srgb,
    /// Pass CMYK values through without conversion (output buffer will be 4-channel CMYK).
    PreserveCmyk,
    /// Simulate CMYK ink on white paper via the embedded device-CMYK ICC profile.
    SimulateCmyk,
}

/// Pixel layout of the rendered output buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PixelFormat {
    /// RGBA, 8 bits per channel, premultiplied alpha (default).
    #[default]
    Rgba8,
    /// CMYK, 8 bits per channel (produced by [`ColorMode::PreserveCmyk`]).
    Cmyk8,
}

/// High-level render configuration.
///
/// Combines color-mode policy and output DPI.  For fine-grained control
/// (forced dimensions, custom background colour) use [`RenderOptions`] directly.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// How CMYK colors are handled (default: [`ColorMode::Srgb`]).
    pub color_mode: ColorMode,
    /// Render resolution in dots per inch (default: 72).
    pub dpi: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::default(),
            dpi: 72,
        }
    }
}

impl From<&RenderConfig> for RenderOptions {
    fn from(cfg: &RenderConfig) -> Self {
        RenderOptions {
            dpi: cfg.dpi as f64,
            ..Default::default()
        }
    }
}

/// Options for rendering a page.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Resolution in dots per inch (default: 72.0 = 1:1 with PDF points).
    pub dpi: f64,
    /// Background colour as `[r, g, b, a]` in 0.0..1.0 (default: opaque white).
    pub background: [f32; 4],
    /// Whether to render annotations (default: true).
    pub render_annotations: bool,
    /// Force output width in pixels (overrides DPI for width).
    pub width: Option<u16>,
    /// Force output height in pixels (overrides DPI for height).
    pub height: Option<u16>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            dpi: 72.0,
            background: [1.0, 1.0, 1.0, 1.0],
            render_annotations: true,
            width: None,
            height: None,
        }
    }
}

/// A rendered page as pixel data.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel layout of the output buffer.
    pub pixel_format: PixelFormat,
    /// Pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

/// Render a single page to RGBA pixels.
pub(crate) fn render_page(
    page: &Page<'_>,
    options: &RenderOptions,
    settings: &InterpreterSettings,
) -> RenderedPage {
    let (width, height, pixels) = render_rgba_pixels(page, options, settings);
    RenderedPage {
        width,
        height,
        pixel_format: PixelFormat::Rgba8,
        pixels,
    }
}

/// Render a single page using the higher-level color-mode configuration.
///
/// [`ColorMode::Srgb`] and [`ColorMode::SimulateCmyk`] both return RGBA output.
/// The underlying `pdf-render`/`pdf-interpret` stack resolves ICCBased spaces and
/// the built-in DeviceCMYK ICC profile during rasterization.
/// [`ColorMode::PreserveCmyk`] re-encodes the RGBA output into a 4-channel CMYK buffer
/// via a standard sRGB→CMYK back-conversion.
pub(crate) fn render_page_with_config(
    page: &Page<'_>,
    config: &RenderConfig,
    settings: &InterpreterSettings,
) -> RenderedPage {
    let options = RenderOptions::from(config);
    match config.color_mode {
        ColorMode::Srgb | ColorMode::SimulateCmyk => render_page(page, &options, settings),
        ColorMode::PreserveCmyk => {
            let (width, height, rgba) = render_rgba_pixels(page, &options, settings);
            RenderedPage {
                width,
                height,
                pixel_format: PixelFormat::Cmyk8,
                pixels: rgba_to_cmyk_buffer(&rgba),
            }
        }
    }
}

/// Render a page as a thumbnail (fits within `max_dimension` on longest side).
pub(crate) fn render_thumbnail(
    page: &Page<'_>,
    max_dimension: u32,
    settings: &InterpreterSettings,
) -> RenderedPage {
    let (w, h) = page.render_dimensions();
    let longest = w.max(h) as f64;
    let scale = (max_dimension as f64 / longest) as f32;

    let rs = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
        ..Default::default()
    };

    let pixmap = render(page, settings, &rs);
    let pw = pixmap.width() as u32;
    let ph = pixmap.height() as u32;
    let pixels = pixmap.data_as_u8_slice().to_vec();

    RenderedPage {
        width: pw,
        height: ph,
        pixel_format: PixelFormat::Rgba8,
        pixels,
    }
}

fn render_rgba_pixels(
    page: &Page<'_>,
    options: &RenderOptions,
    settings: &InterpreterSettings,
) -> (u32, u32, Vec<u8>) {
    let scale = (options.dpi / 72.0) as f32;
    let bg = AlphaColor::<Srgb>::new(options.background);

    let rs = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        width: options.width,
        height: options.height,
        bg_color: bg,
    };

    let mut isettings = settings.clone();
    isettings.render_annotations = options.render_annotations;

    let pixmap = render(page, &isettings, &rs);
    let width = pixmap.width() as u32;
    let height = pixmap.height() as u32;
    let pixels = pixmap.data_as_u8_slice().to_vec();
    (width, height, pixels)
}

/// Convert an RGBA pixel buffer to CMYK (4 bytes per pixel).
///
/// Alpha is composited over white before conversion.
fn rgba_to_cmyk_buffer(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let alpha = px[3] as f32 / 255.0;
        let bg = 1.0 - alpha;
        let r = (px[0] as f32 / 255.0) * alpha + bg;
        let g = (px[1] as f32 / 255.0) * alpha + bg;
        let b = (px[2] as f32 / 255.0) * alpha + bg;
        let k = 1.0 - r.max(g).max(b);
        let (c, m, y) = if k >= 1.0 - f32::EPSILON {
            (0.0_f32, 0.0_f32, 0.0_f32)
        } else {
            let inv = 1.0 - k;
            ((inv - r) / inv, (inv - g) / inv, (inv - b) / inv)
        };
        out.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        out.push((m.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        out.push((y.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        out.push((k.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_options_defaults() {
        let opts = RenderOptions::default();
        assert!((opts.dpi - 72.0).abs() < f64::EPSILON);
        assert!(opts.render_annotations);
        assert!(opts.width.is_none());
        assert!(opts.height.is_none());
    }

    #[test]
    fn render_config_defaults() {
        let cfg = RenderConfig::default();
        assert_eq!(cfg.color_mode, ColorMode::Srgb);
        assert_eq!(cfg.dpi, 72);
    }

    #[test]
    fn rendered_page_empty() {
        let p = RenderedPage {
            width: 10,
            height: 20,
            pixel_format: PixelFormat::Rgba8,
            pixels: vec![0; 10 * 20 * 4],
        };
        assert_eq!(p.pixels.len(), 800);
    }

    #[test]
    fn rgba_to_cmyk_black() {
        let buf = rgba_to_cmyk_buffer(&[0, 0, 0, 255]);
        assert_eq!(buf, [0, 0, 0, 255]);
    }

    #[test]
    fn rgba_to_cmyk_white() {
        let buf = rgba_to_cmyk_buffer(&[255, 255, 255, 255]);
        assert_eq!(buf, [0, 0, 0, 0]);
    }

    #[test]
    fn rgba_to_cmyk_buffer_stride() {
        let buf = rgba_to_cmyk_buffer(&[255, 0, 0, 255, 0, 0, 0, 255]);
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn render_config_into_options() {
        let cfg = RenderConfig { dpi: 150, ..Default::default() };
        let opts = RenderOptions::from(&cfg);
        assert!((opts.dpi - 150.0).abs() < f64::EPSILON);
    }
}
