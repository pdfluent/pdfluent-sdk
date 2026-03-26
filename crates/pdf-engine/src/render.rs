//! Page rendering with z-order compositing.
//!
//! Renders a PDF page to RGBA pixel data using the hayro rendering stack.

use crate::color::{blend_cmyk, preserve_device_cmyk, rgba_to_cmyk_buffer};
use kurbo::{Affine, BezPath, Point, Rect, Shape};
use pdf_interpret::font::Glyph;
use pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, FillRule, GlyphDrawMode,
    Image as PdfImage, InterpreterSettings, Paint, PathDrawMode, RasterImage, SoftMask,
};
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_syntax::page::Page;
use pdf_render::vello_cpu::color::palette::css::WHITE;
use pdf_render::vello_cpu::color::{AlphaColor, Srgb};
use pdf_render::vello_cpu::peniko::Fill as PenikoFill;
use pdf_render::vello_cpu::{
    Level, Pixmap, RenderContext, RenderMode, RenderSettings as CpuRenderSettings,
};
use pdf_render::{render, RenderSettings};

const AXIS_EPSILON: f64 = 1e-5;

/// Color space handling during rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Convert all colors to sRGB (default).
    #[default]
    Srgb,
    /// Preserve exact DeviceCMYK values where the rasterizer can prove they survive unchanged.
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
/// Combines color-mode policy and output DPI. For fine-grained control
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

struct CmykOverlay {
    data: Vec<Option<[u8; 4]>>,
    width: u32,
    height: u32,
}

impl CmykOverlay {
    fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![None; width as usize * height as usize],
            width,
            height,
        }
    }

    fn apply_mask(&mut self, alpha_mask: &[u8], cmyk: [u8; 4]) {
        for (slot, alpha) in self.data.iter_mut().zip(alpha_mask.iter().copied()) {
            match alpha {
                0 => {}
                255 => *slot = Some(cmyk),
                partial => {
                    if let Some(existing) = *slot {
                        *slot = Some(blend_cmyk(existing, cmyk, partial));
                    } else {
                        *slot = None;
                    }
                }
            }
        }
    }

    fn contaminate(&mut self, alpha_mask: &[u8]) {
        for (slot, alpha) in self.data.iter_mut().zip(alpha_mask.iter().copied()) {
            if alpha != 0 {
                *slot = None;
            }
        }
    }

    fn set_exact_pixel(&mut self, x: u32, y: u32, cmyk: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = y as usize * self.width as usize + x as usize;
        self.data[idx] = Some(cmyk);
    }

    fn compose_with_rgba_fallback(self, rgba: &[u8]) -> Vec<u8> {
        let mut out = rgba_to_cmyk_buffer(rgba);
        for (idx, exact) in self.data.iter().enumerate() {
            if let Some(exact) = exact {
                let start = idx * 4;
                out[start..start + 4].copy_from_slice(exact);
            }
        }
        out
    }
}

struct GroupState {
    previous_opacity: f32,
    unsupported: bool,
}

struct CmykOverlayDevice {
    overlay: CmykOverlay,
    clip_stack: Vec<ClipPath>,
    current_opacity: f32,
    current_soft_mask: bool,
    current_blend_mode: BlendMode,
    group_stack: Vec<GroupState>,
    unsupported_depth: usize,
    cpu_settings: CpuRenderSettings,
}

impl CmykOverlayDevice {
    fn new(width: u16, height: u16) -> Self {
        Self {
            overlay: CmykOverlay::new(width as u32, height as u32),
            clip_stack: Vec::new(),
            current_opacity: 1.0,
            current_soft_mask: false,
            current_blend_mode: BlendMode::Normal,
            group_stack: Vec::new(),
            unsupported_depth: 0,
            cpu_settings: CpuRenderSettings {
                level: Level::new(),
                num_threads: 0,
                render_mode: RenderMode::OptimizeSpeed,
            },
        }
    }

    fn finish(self) -> CmykOverlay {
        self.overlay
    }

    fn exact_cmyk_for_paint(&self, paint: &Paint<'_>) -> Option<[u8; 4]> {
        if !self.can_preserve_exact() {
            return None;
        }

        match paint {
            Paint::Color(color) => color
                .device_cmyk_components()
                .map(|[c, m, y, k]| preserve_device_cmyk(c, m, y, k)),
            Paint::Pattern(_) => None,
        }
    }

    fn paint_opacity(&self, paint: &Paint<'_>) -> f32 {
        let local = match paint {
            Paint::Color(color) => color.opacity(),
            Paint::Pattern(_) => 1.0,
        };
        (local * self.current_opacity).clamp(0.0, 1.0)
    }

    fn can_preserve_exact(&self) -> bool {
        self.unsupported_depth == 0
            && !self.current_soft_mask
            && self.current_blend_mode == BlendMode::Normal
    }

    fn handle_path_operation(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
        is_text: bool,
    ) {
        let alpha = self.paint_opacity(paint);
        if alpha <= 0.0 {
            return;
        }

        let mask = self.rasterize_path_mask(path, transform, draw_mode, alpha, is_text);
        if let Some(cmyk) = self.exact_cmyk_for_paint(paint) {
            self.overlay.apply_mask(&mask, cmyk);
        } else {
            self.overlay.contaminate(&mask);
        }
    }

    fn rasterize_path_mask(
        &self,
        path: &BezPath,
        transform: Affine,
        draw_mode: &PathDrawMode,
        alpha: f32,
        is_text: bool,
    ) -> Vec<u8> {
        let mut ctx = RenderContext::new_with(
            self.overlay.width as u16,
            self.overlay.height as u16,
            self.cpu_settings,
        );
        self.apply_clip_stack(&mut ctx);
        ctx.set_paint(AlphaColor::<Srgb>::new([1.0, 1.0, 1.0, alpha]));
        ctx.set_transform(transform);

        match draw_mode {
            PathDrawMode::Fill(fill_rule) => {
                ctx.set_fill_rule(convert_fill_rule(*fill_rule));
                ctx.fill_path(path);
            }
            PathDrawMode::Stroke(stroke_props) => {
                ctx.set_stroke(stroke_for_path(transform, stroke_props, is_text));
                ctx.stroke_path(path);
            }
        }

        self.finish_mask(ctx)
    }

    fn rasterize_rect_mask(&self, rect: &Rect, transform: Affine, alpha: f32) -> Vec<u8> {
        self.rasterize_path_mask(
            &rect.to_path(0.1),
            transform,
            &PathDrawMode::Fill(FillRule::NonZero),
            alpha,
            false,
        )
    }

    fn finish_mask(&self, mut ctx: RenderContext) -> Vec<u8> {
        let mut pixmap = Pixmap::new(self.overlay.width as u16, self.overlay.height as u16);
        ctx.flush();
        ctx.render_to_pixmap(&mut pixmap);
        pixmap
            .data_as_u8_slice()
            .chunks_exact(4)
            .map(|px| px[3])
            .collect()
    }

    fn apply_clip_stack(&self, ctx: &mut RenderContext) {
        for clip in &self.clip_stack {
            let old_transform = *ctx.transform();
            ctx.set_fill_rule(convert_fill_rule(clip.fill));
            ctx.set_transform(Affine::IDENTITY);
            ctx.push_clip_path(&clip.path);
            ctx.set_transform(old_transform);
        }
    }

    fn handle_raster_image(&mut self, image: &RasterImage<'_>, transform: Affine) -> bool {
        if !self.can_preserve_exact()
            || (self.current_opacity - 1.0).abs() > f32::EPSILON
            || self.clip_stack.len() > 1
        {
            return false;
        }

        let mut preserved = false;
        image.with_device_cmyk(
            |cmyk, alpha| {
                if alpha.is_some() {
                    return;
                }
                preserved = self.apply_axis_aligned_image(transform, cmyk);
            },
            None,
        );
        preserved
    }

    fn apply_axis_aligned_image(
        &mut self,
        transform: Affine,
        cmyk: pdf_interpret::CmykData,
    ) -> bool {
        let transform = transform
            * Affine::scale_non_uniform(cmyk.scale_factors.0 as f64, cmyk.scale_factors.1 as f64);
        let [sx, shy, shx, sy, tx, ty] = transform.as_coeffs();
        if shy.abs() > AXIS_EPSILON || shx.abs() > AXIS_EPSILON {
            return false;
        }
        if sx.abs() <= AXIS_EPSILON || sy.abs() <= AXIS_EPSILON {
            return false;
        }

        let bounds = (transform
            * Rect::new(0.0, 0.0, cmyk.width as f64, cmyk.height as f64).to_path(0.1))
        .bounding_box()
        .intersect(Rect::new(
            0.0,
            0.0,
            self.overlay.width as f64,
            self.overlay.height as f64,
        ));
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return true;
        }

        let min_x = bounds.x0.floor().max(0.0) as u32;
        let max_x = bounds.x1.ceil().min(self.overlay.width as f64) as u32;
        let min_y = bounds.y0.floor().max(0.0) as u32;
        let max_y = bounds.y1.ceil().min(self.overlay.height as f64) as u32;
        let inv_sx = 1.0 / sx;
        let inv_sy = 1.0 / sy;

        for y in min_y..max_y {
            for x in min_x..max_x {
                let src_x = ((x as f64 + 0.5) - tx) * inv_sx;
                let src_y = ((y as f64 + 0.5) - ty) * inv_sy;
                if src_x < 0.0
                    || src_x >= cmyk.width as f64
                    || src_y < 0.0
                    || src_y >= cmyk.height as f64
                {
                    continue;
                }

                let src_x = src_x.floor() as usize;
                let src_y = src_y.floor() as usize;
                let idx = (src_y * cmyk.width as usize + src_x) * 4;
                self.overlay.set_exact_pixel(
                    x,
                    y,
                    [
                        cmyk.data[idx],
                        cmyk.data[idx + 1],
                        cmyk.data[idx + 2],
                        cmyk.data[idx + 3],
                    ],
                );
            }
        }

        true
    }

    fn handle_image_fallback(&mut self, image: &PdfImage<'_, '_>, transform: Affine, alpha: f32) {
        if alpha <= 0.0 {
            return;
        }
        let rect = Rect::new(0.0, 0.0, image.width() as f64, image.height() as f64);
        let mask = self.rasterize_rect_mask(&rect, transform, alpha);
        self.overlay.contaminate(&mask);
    }
}

impl<'a> Device<'a> for CmykOverlayDevice {
    fn set_soft_mask(&mut self, mask: Option<SoftMask<'a>>) {
        self.current_soft_mask = mask.is_some();
    }

    fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        self.current_blend_mode = blend_mode;
    }

    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'a>,
        draw_mode: &PathDrawMode,
    ) {
        self.handle_path_operation(path, transform, paint, draw_mode, false);
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.clip_stack.push(clip_path.clone());
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'a>>,
        blend_mode: BlendMode,
    ) {
        let unsupported = mask.is_some() || blend_mode != BlendMode::Normal;
        self.group_stack.push(GroupState {
            previous_opacity: self.current_opacity,
            unsupported,
        });
        self.current_opacity = (self.current_opacity * opacity).clamp(0.0, 1.0);
        if unsupported {
            self.unsupported_depth += 1;
        }
    }

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
        draw_mode: &GlyphDrawMode,
    ) {
        match draw_mode {
            GlyphDrawMode::Invisible => {}
            GlyphDrawMode::Fill => match glyph {
                Glyph::Outline(outline) => {
                    self.handle_path_operation(
                        &outline.outline(),
                        transform * glyph_transform,
                        paint,
                        &PathDrawMode::Fill(FillRule::NonZero),
                        true,
                    );
                }
                Glyph::Type3(type3) => {
                    type3.interpret(self, transform, glyph_transform, paint);
                }
            },
            GlyphDrawMode::Stroke(stroke_props) => match glyph {
                Glyph::Outline(outline) => {
                    let path = glyph_transform * outline.outline();
                    self.handle_path_operation(
                        &path,
                        transform,
                        paint,
                        &PathDrawMode::Stroke(stroke_props.clone()),
                        true,
                    );
                }
                Glyph::Type3(type3) => {
                    type3.interpret(self, transform, glyph_transform, paint);
                }
            },
        }
    }

    fn draw_image(&mut self, image: PdfImage<'a, '_>, transform: Affine) {
        match &image {
            PdfImage::Raster(raster) if self.handle_raster_image(raster, transform) => {}
            PdfImage::Stencil(stencil) => {
                let _ = stencil;
                self.handle_image_fallback(&image, transform, self.current_opacity);
            }
            PdfImage::Raster(_) => {
                self.handle_image_fallback(&image, transform, self.current_opacity);
            }
        }
    }

    fn pop_clip_path(&mut self) {
        let _ = self.clip_stack.pop();
    }

    fn pop_transparency_group(&mut self) {
        if let Some(state) = self.group_stack.pop() {
            self.current_opacity = state.previous_opacity;
            if state.unsupported {
                self.unsupported_depth = self.unsupported_depth.saturating_sub(1);
            }
        }
    }
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
/// [`ColorMode::PreserveCmyk`] preserves exact DeviceCMYK samples where the
/// overlay pass can prove that the original values remain visible, and falls
/// back to RGBA→CMYK elsewhere.
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
            let overlay = build_cmyk_overlay(page, &options, settings, width, height);
            RenderedPage {
                width,
                height,
                pixel_format: PixelFormat::Cmyk8,
                pixels: overlay.compose_with_rgba_fallback(&rgba),
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

fn build_cmyk_overlay(
    page: &Page<'_>,
    options: &RenderOptions,
    settings: &InterpreterSettings,
    width: u32,
    height: u32,
) -> CmykOverlay {
    let scale = (options.dpi / 72.0) as f32;
    let initial_transform =
        Affine::scale_non_uniform(scale as f64, scale as f64) * page.initial_transform(true);
    let mut isettings = settings.clone();
    isettings.render_annotations = options.render_annotations;

    let mut ctx = Context::new(
        initial_transform,
        Rect::new(0.0, 0.0, width as f64, height as f64),
        page.xref(),
        isettings.clone(),
    );
    let mut device = CmykOverlayDevice::new(width as u16, height as u16);

    device.push_clip_path(&ClipPath {
        path: Rect::new(0.0, 0.0, width as f64, height as f64).to_path(0.1),
        fill: FillRule::NonZero,
    });
    device.push_transparency_group(1.0, None, BlendMode::Normal);
    interpret_page(page, &mut ctx, &mut device);
    device.pop_transparency_group();
    device.pop_clip_path();

    device.finish()
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

fn stroke_for_path(
    transform: Affine,
    stroke_props: &pdf_interpret::StrokeProps,
    is_text: bool,
) -> kurbo::Stroke {
    let threshold = if is_text { 0.25 } else { 1.0 };
    let min_factor = max_factor(&transform);
    let mut line_width = stroke_props.line_width.max(0.01);
    let transformed_width = line_width * min_factor;

    if transformed_width < threshold && transformed_width > 0.0 {
        line_width /= transformed_width;
        line_width *= threshold;
    }

    kurbo::Stroke {
        width: line_width as f64,
        join: stroke_props.line_join,
        miter_limit: stroke_props.miter_limit as f64,
        start_cap: stroke_props.line_cap,
        end_cap: stroke_props.line_cap,
        dash_pattern: stroke_props.dash_array.iter().map(|n| *n as f64).collect(),
        dash_offset: stroke_props.dash_offset as f64,
    }
}

fn max_factor(transform: &Affine) -> f32 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let x_advance = Affine::new([a, b, c, d, 0.0, 0.0]) * Point::new(1.0, 0.0);
    let y_advance = Affine::new([a, b, c, d, 0.0, 0.0]) * Point::new(0.0, 1.0);
    x_advance
        .to_vec2()
        .length()
        .max(y_advance.to_vec2().length()) as f32
}

fn convert_fill_rule(fill_rule: FillRule) -> PenikoFill {
    match fill_rule {
        FillRule::NonZero => PenikoFill::NonZero,
        FillRule::EvenOdd => PenikoFill::EvenOdd,
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
        let buf = crate::color::rgba_to_cmyk_buffer(&[0, 0, 0, 255]);
        assert_eq!(buf, [0, 0, 0, 255]);
    }

    #[test]
    fn rgba_to_cmyk_white() {
        let buf = crate::color::rgba_to_cmyk_buffer(&[255, 255, 255, 255]);
        assert_eq!(buf, [0, 0, 0, 0]);
    }

    #[test]
    fn rgba_to_cmyk_buffer_stride() {
        let buf = crate::color::rgba_to_cmyk_buffer(&[255, 0, 0, 255, 0, 0, 0, 255]);
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn render_config_into_options() {
        let cfg = RenderConfig {
            dpi: 150,
            ..Default::default()
        };
        let opts = RenderOptions::from(&cfg);
        assert!((opts.dpi - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overlay_partial_pixel_without_prior_exact_falls_back() {
        let mut overlay = CmykOverlay::new(1, 1);
        overlay.apply_mask(&[128], [1, 2, 3, 4]);
        assert_eq!(overlay.data, vec![None]);
    }

    #[test]
    fn overlay_partial_pixel_blends_existing_exact() {
        let mut overlay = CmykOverlay::new(1, 1);
        overlay.apply_mask(&[255], [0, 0, 0, 0]);
        overlay.apply_mask(&[128], [255, 128, 64, 32]);
        assert_eq!(overlay.data, vec![Some([128, 64, 32, 16])]);
    }
}
