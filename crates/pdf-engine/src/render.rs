//! Page rendering with z-order compositing.
//!
//! Renders a PDF page to RGBA pixel data using the hayro rendering stack.

use pdf_render::pdf_interpret::InterpreterSettings;
use pdf_render::pdf_syntax::page::Page;
use pdf_render::vello_cpu::color::palette::css::WHITE;
use pdf_render::vello_cpu::color::{AlphaColor, Srgb};
use pdf_render::{render, RenderSettings};

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

/// A rendered page as RGBA pixel data (premultiplied alpha).
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

/// Render a single page to RGBA pixels.
pub fn render_page(
    page: &Page<'_>,
    options: &RenderOptions,
    settings: &InterpreterSettings,
) -> RenderedPage {
    let scale = (options.dpi / 72.0) as f32;
    let bg = AlphaColor::<Srgb>::new(options.background);

    let rs = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        width: options.width,
        height: options.height,
        bg_color: bg,
    };

    let pixmap = render(page, settings, &rs);
    let w = pixmap.width() as u32;
    let h = pixmap.height() as u32;
    let pixels = pixmap.data_as_u8_slice().to_vec();

    RenderedPage {
        width: w,
        height: h,
        pixels,
    }
}

/// Render a page as a thumbnail (fits within `max_dimension` on longest side).
pub fn render_thumbnail(
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
        pixels,
    }
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
    fn rendered_page_empty() {
        let p = RenderedPage {
            width: 10,
            height: 20,
            pixels: vec![0; 10 * 20 * 4],
        };
        assert_eq!(p.pixels.len(), 800);
    }
}
