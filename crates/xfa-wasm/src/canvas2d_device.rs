// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use kurbo::{BezPath, PathEl};
use pdf_render::pdf_interpret::FillRule;

#[cfg(target_arch = "wasm32")]
use js_sys::Array;
#[cfg(target_arch = "wasm32")]
use kurbo::{Affine, Cap, Join, Point};
#[cfg(target_arch = "wasm32")]
use pdf_render::pdf_interpret::cmap::BfString;
#[cfg(target_arch = "wasm32")]
use pdf_render::pdf_interpret::font::Glyph;
#[cfg(target_arch = "wasm32")]
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask, StrokeProps,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CanvasPathCommand {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadTo(f64, f64, f64, f64),
    CurveTo(f64, f64, f64, f64, f64, f64),
    ClosePath,
}

pub fn for_each_path_command(path: &BezPath, mut emit: impl FnMut(CanvasPathCommand)) {
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => emit(CanvasPathCommand::MoveTo(point.x, point.y)),
            PathEl::LineTo(point) => emit(CanvasPathCommand::LineTo(point.x, point.y)),
            PathEl::QuadTo(control, point) => {
                emit(CanvasPathCommand::QuadTo(
                    control.x, control.y, point.x, point.y,
                ));
            }
            PathEl::CurveTo(control1, control2, point) => {
                emit(CanvasPathCommand::CurveTo(
                    control1.x, control1.y, control2.x, control2.y, point.x, point.y,
                ));
            }
            PathEl::ClosePath => emit(CanvasPathCommand::ClosePath),
        }
    }
}

pub fn path_commands(path: &BezPath) -> Vec<CanvasPathCommand> {
    let mut commands = Vec::with_capacity(path.elements().len());
    for_each_path_command(path, |command| commands.push(command));
    commands
}

pub fn fill_rule_name(fill_rule: FillRule) -> &'static str {
    match fill_rule {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    }
}

#[cfg(target_arch = "wasm32")]
fn blend_mode_name(blend_mode: BlendMode) -> &'static str {
    match blend_mode {
        BlendMode::Normal => "source-over",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    }
}

#[cfg(target_arch = "wasm32")]
fn glyph_text(glyph: &Glyph<'_>) -> Option<String> {
    match glyph.as_unicode()? {
        BfString::Char(ch) => Some(ch.to_string()),
        BfString::String(text) if !text.is_empty() => Some(text),
        BfString::String(_) => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn max_factor(transform: &Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let scale_skew_transform = Affine::new([a, b, c, d, 0.0, 0.0]);
    let x_advance = scale_skew_transform * Point::new(1.0, 0.0);
    let y_advance = scale_skew_transform * Point::new(0.0, 1.0);

    x_advance
        .to_vec2()
        .length()
        .max(y_advance.to_vec2().length())
}

#[cfg(target_arch = "wasm32")]
fn adjusted_line_width(stroke_props: &StrokeProps, transform: &Affine) -> f64 {
    let mut line_width = stroke_props.line_width.max(0.01) as f64;
    let factor = max_factor(transform);
    let effective_width = line_width * factor;

    if effective_width > 0.0 && effective_width < 0.25 {
        line_width *= 0.25 / effective_width;
    }

    line_width
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use web_sys::{CanvasRenderingContext2d, CanvasWindingRule};

#[cfg(target_arch = "wasm32")]
pub struct Canvas2DDevice {
    ctx: CanvasRenderingContext2d,
    fallback_reason: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl Canvas2DDevice {
    pub fn new(ctx: CanvasRenderingContext2d) -> Self {
        Self {
            ctx,
            fallback_reason: None,
        }
    }

    pub fn needs_fallback(&self) -> bool {
        self.fallback_reason.is_some()
    }

    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    fn mark_fallback(&mut self, reason: impl Into<String>) {
        if self.fallback_reason.is_none() {
            self.fallback_reason = Some(reason.into());
        }
    }

    fn record_js_error(&mut self, action: &str, error: JsValue) {
        self.mark_fallback(format!("{action}: {error:?}"));
    }

    fn with_transform(&mut self, transform: Affine, op: impl FnOnce(&mut Self)) {
        self.ctx.save();

        let [a, b, c, d, e, f] = transform.as_coeffs();
        if let Err(error) = self.ctx.transform(a, b, c, d, e, f) {
            self.record_js_error("ctx.transform", error);
            self.ctx.restore();
            return;
        }

        op(self);
        self.ctx.restore();
    }

    fn trace_path(&mut self, path: &BezPath) {
        for_each_path_command(path, |command| match command {
            CanvasPathCommand::MoveTo(x, y) => self.ctx.move_to(x, y),
            CanvasPathCommand::LineTo(x, y) => self.ctx.line_to(x, y),
            CanvasPathCommand::QuadTo(cx, cy, x, y) => self.ctx.quadratic_curve_to(cx, cy, x, y),
            CanvasPathCommand::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
                self.ctx.bezier_curve_to(c1x, c1y, c2x, c2y, x, y);
            }
            CanvasPathCommand::ClosePath => self.ctx.close_path(),
        });
    }

    fn apply_fill_style(&mut self, paint: &Paint<'_>) -> bool {
        match paint {
            Paint::Color(color) => {
                self.ctx.set_fill_style_str(&paint_to_css(color));
                true
            }
            Paint::Pattern(_) => {
                self.mark_fallback("Paint::Pattern is not supported by Canvas2DDevice yet");
                false
            }
        }
    }

    fn apply_stroke_style(&mut self, paint: &Paint<'_>) -> bool {
        match paint {
            Paint::Color(color) => {
                self.ctx.set_stroke_style_str(&paint_to_css(color));
                true
            }
            Paint::Pattern(_) => {
                self.mark_fallback("Paint::Pattern is not supported by Canvas2DDevice yet");
                false
            }
        }
    }

    fn apply_stroke_props(&mut self, stroke_props: &StrokeProps, transform: &Affine) {
        self.ctx
            .set_line_width(adjusted_line_width(stroke_props, transform));
        self.ctx.set_line_cap(line_cap_name(stroke_props.line_cap));
        self.ctx
            .set_line_join(line_join_name(stroke_props.line_join));
        self.ctx.set_miter_limit(stroke_props.miter_limit as f64);
        self.ctx
            .set_line_dash_offset(stroke_props.dash_offset as f64);

        let dash = Array::new();
        for value in &stroke_props.dash_array {
            dash.push(&JsValue::from_f64(*value as f64));
        }

        if let Err(error) = self.ctx.set_line_dash(&dash.into()) {
            self.record_js_error("ctx.setLineDash", error);
        }
    }

    fn draw_path_fill(&mut self, fill_rule: FillRule) {
        self.ctx
            .fill_with_canvas_winding_rule(fill_rule_to_canvas(fill_rule));
    }

    fn draw_path_stroke(&mut self) {
        self.ctx.stroke();
    }
}

#[cfg(target_arch = "wasm32")]
impl Device<'_> for Canvas2DDevice {
    fn set_soft_mask(&mut self, mask: Option<SoftMask<'_>>) {
        if mask.is_some() {
            self.mark_fallback("soft masks are not supported by Canvas2DDevice yet");
        }
    }

    fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        if let Err(error) = self
            .ctx
            .set_global_composite_operation(blend_mode_name(blend_mode))
        {
            self.record_js_error("ctx.globalCompositeOperation", error);
        }
    }

    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        self.with_transform(transform, |device| {
            device.ctx.begin_path();
            device.trace_path(path);

            match draw_mode {
                PathDrawMode::Fill(fill_rule) => {
                    if device.apply_fill_style(paint) {
                        device.draw_path_fill(*fill_rule);
                    }
                }
                PathDrawMode::Stroke(stroke_props) => {
                    if device.apply_stroke_style(paint) {
                        device.apply_stroke_props(stroke_props, &transform);
                        device.draw_path_stroke();
                    }
                }
            }
        });
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        self.ctx.save();
        self.ctx.begin_path();
        self.trace_path(&clip_path.path);
        self.ctx
            .clip_with_canvas_winding_rule(fill_rule_to_canvas(clip_path.fill));
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'_>>,
        blend_mode: BlendMode,
    ) {
        if mask.is_some() {
            self.mark_fallback("transparency groups with soft masks are not supported yet");
        }

        self.ctx.save();
        self.ctx.set_global_alpha(opacity as f64);
        if let Err(error) = self
            .ctx
            .set_global_composite_operation(blend_mode_name(blend_mode))
        {
            self.record_js_error("ctx.globalCompositeOperation", error);
        }
    }

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &GlyphDrawMode,
    ) {
        if matches!(draw_mode, GlyphDrawMode::Invisible) {
            return;
        }

        let Some(text) = glyph_text(glyph) else {
            return;
        };

        let glyph_matrix = transform * glyph_transform;
        self.with_transform(glyph_matrix, |device| {
            device.ctx.set_font("1000px sans-serif");

            match draw_mode {
                GlyphDrawMode::Fill => {
                    if device.apply_fill_style(paint) {
                        if let Err(error) = device.ctx.fill_text(&text, 0.0, 0.0) {
                            device.record_js_error("ctx.fillText", error);
                        }
                    }
                }
                GlyphDrawMode::Stroke(stroke_props) => {
                    if device.apply_stroke_style(paint) {
                        device.apply_stroke_props(stroke_props, &glyph_matrix);
                        if let Err(error) = device.ctx.stroke_text(&text, 0.0, 0.0) {
                            device.record_js_error("ctx.strokeText", error);
                        }
                    }
                }
                GlyphDrawMode::Invisible => {}
            }
        });
    }

    fn draw_image(&mut self, _image: Image<'_, '_>, _transform: Affine) {
        self.mark_fallback("images are not supported by Canvas2DDevice yet");
    }

    fn pop_clip_path(&mut self) {
        self.ctx.restore();
    }

    fn pop_transparency_group(&mut self) {
        self.ctx.restore();
    }

    fn draw_rect(
        &mut self,
        rect: &kurbo::Rect,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        self.with_transform(transform, |device| match draw_mode {
            PathDrawMode::Fill(_) => {
                if device.apply_fill_style(paint) {
                    device
                        .ctx
                        .fill_rect(rect.x0, rect.y0, rect.width(), rect.height());
                }
            }
            PathDrawMode::Stroke(stroke_props) => {
                if device.apply_stroke_style(paint) {
                    device.apply_stroke_props(stroke_props, &transform);
                    device
                        .ctx
                        .stroke_rect(rect.x0, rect.y0, rect.width(), rect.height());
                }
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn fill_rule_to_canvas(fill_rule: FillRule) -> CanvasWindingRule {
    match fill_rule {
        FillRule::NonZero => CanvasWindingRule::Nonzero,
        FillRule::EvenOdd => CanvasWindingRule::Evenodd,
    }
}

#[cfg(target_arch = "wasm32")]
fn line_cap_name(line_cap: Cap) -> &'static str {
    match line_cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    }
}

#[cfg(target_arch = "wasm32")]
fn line_join_name(line_join: Join) -> &'static str {
    match line_join {
        Join::Miter => "miter",
        Join::Round => "round",
        Join::Bevel => "bevel",
    }
}

#[cfg(target_arch = "wasm32")]
fn paint_to_css(color: &pdf_render::pdf_interpret::color::Color) -> String {
    let rgba = color.to_rgba().to_rgba8();
    format!(
        "rgba({}, {}, {}, {:.6})",
        rgba[0],
        rgba[1],
        rgba[2],
        rgba[3] as f64 / 255.0
    )
}
