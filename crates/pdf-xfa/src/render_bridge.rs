//! XFA layout output to PDF content stream overlay generation.
//!
//! Converts LayoutDom (from xfa-layout-engine) into PDF content stream
//! operators that can be overlaid on existing PDF pages.
//!
//! Coordinate mapping: XFA uses top-left origin (y grows downward),
//! PDF uses bottom-left origin (y grows upward).

use crate::error::Result;
use crate::font_bridge::font_variant_key;
use std::collections::HashMap;
use xfa_layout_engine::form::{DrawContent, FieldKind, FormNodeStyle};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{TextAlign, VerticalAlign};

/// Configuration for PDF overlay rendering.
#[derive(Debug, Clone)]
pub struct XfaRenderConfig {
    /// Default font name to use in content streams.
    pub default_font: String,
    /// Default font size in points.
    pub default_font_size: f64,
    /// Whether to draw field borders.
    pub draw_borders: bool,
    /// Border line width.
    pub border_width: f64,
    /// Border color (RGB 0-1).
    pub border_color: [f64; 3],
    /// Text color (RGB 0-1).
    pub text_color: [f64; 3],
    /// Background color for fields (None = transparent).
    pub background_color: Option<[f64; 3]>,
    /// Text padding from field edges.
    pub text_padding: f64,
    /// Map from typeface name to PDF font resource name (e.g. "/XFA_F0").
    pub font_map: HashMap<String, String>,
    /// Resolved font metrics per typeface.
    pub font_metrics_data: HashMap<String, FontMetricsData>,
}

/// Resolved font metrics for a typeface, used for accurate text measurement.
#[derive(Debug, Clone)]
pub struct FontMetricsData {
    /// PDF glyph widths (indices 0..255).
    pub widths: Vec<u16>,
    /// Units per em of the font.
    pub upem: u16,
    /// Font ascender in font units.
    pub ascender: i16,
    /// Font descender in font units (typically negative).
    pub descender: i16,
    /// Raw font data for glyph ID lookup (Identity-H fonts).
    pub font_data: Option<Vec<u8>>,
    /// Font face index within a collection.
    pub face_index: u32,
}

/// Image data collected during rendering for XObject embedding.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub name: String,
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Overlay result for a single page, including content stream and images.
#[derive(Debug, Clone)]
pub struct PageOverlay {
    pub content_stream: Vec<u8>,
    pub images: Vec<ImageInfo>,
}

impl Default for XfaRenderConfig {
    fn default() -> Self {
        Self {
            default_font: "Helvetica".to_string(),
            default_font_size: 10.0,
            draw_borders: true,
            border_width: 0.5,
            border_color: [0.0, 0.0, 0.0],
            text_color: [0.0, 0.0, 0.0],
            background_color: None,
            text_padding: 1.0,
            font_map: HashMap::new(),
            font_metrics_data: HashMap::new(),
        }
    }
}

/// Maps XFA coordinates (top-left origin) to PDF coordinates (bottom-left origin).
pub struct CoordinateMapper {
    page_height: f64,
    page_width: f64,
}

impl CoordinateMapper {
    pub fn new(page_height: f64, page_width: f64) -> Self {
        Self {
            page_height,
            page_width,
        }
    }

    /// Convert XFA y-coordinate to PDF y-coordinate.
    pub fn xfa_to_pdf_y(&self, xfa_y: f64, element_height: f64) -> f64 {
        self.page_height - xfa_y - element_height
    }

    /// Returns the page width for bounding content.
    pub fn page_width(&self) -> f64 {
        self.page_width
    }
}

/// Create a per-node config by applying XFA template style overrides to the
/// global config.
fn apply_node_style(config: &XfaRenderConfig, style: &FormNodeStyle) -> XfaRenderConfig {
    let mut cfg = config.clone();

    if let Some((r, g, b)) = style.bg_color {
        if !(r >= 250 && g >= 250 && b >= 250) {
            cfg.background_color = Some([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]);
        }
    }

    cfg.draw_borders = false;
    if let Some(bw) = style.border_width_pt {
        if bw > 0.0 {
            cfg.border_width = bw;
            cfg.draw_borders = true;
            if let Some((r, g, b)) = style.border_color {
                cfg.border_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
            }
        }
    }

    if let Some((r, g, b)) = style.text_color {
        cfg.text_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
    }

    cfg
}

/// Generate a PDF content stream overlay for a single page.
pub fn generate_page_overlay(page: &LayoutPage, config: &XfaRenderConfig) -> Result<PageOverlay> {
    let mapper = CoordinateMapper::new(page.height, page.width);
    let mut ops = Vec::new();
    let mut images: Vec<ImageInfo> = Vec::new();
    ops.extend_from_slice(b"q\n");
    render_nodes(
        &page.nodes,
        0.0,
        0.0,
        &mapper,
        config,
        &mut ops,
        &mut images,
    );
    ops.extend_from_slice(b"Q\n");
    Ok(PageOverlay {
        content_stream: ops,
        images,
    })
}

/// Generate PDF content stream overlays for all pages in a layout.
pub fn generate_all_overlays(
    layout: &LayoutDom,
    config: &XfaRenderConfig,
) -> Result<Vec<PageOverlay>> {
    layout
        .pages
        .iter()
        .map(|page| generate_page_overlay(page, config))
        .collect()
}

fn render_nodes(
    nodes: &[LayoutNode],
    parent_x: f64,
    parent_y: f64,
    mapper: &CoordinateMapper,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
    images: &mut Vec<ImageInfo>,
) {
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let w = node.rect.width;
        let h = node.rect.height;
        let pdf_y = mapper.xfa_to_pdf_y(abs_y, h);

        let node_config = apply_node_style(config, &node.style);

        if !matches!(node.content, LayoutContent::Field { .. }) {
            let border_radius = node.style.border_radius_pt.unwrap_or(0.0);
            let border_style = node.style.border_style.as_deref();
            if let Some(bg) = &node_config.background_color {
                write_ops(
                    ops,
                    format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
                );
                emit_rect_path(ops, abs_x, pdf_y, w, h, border_radius);
                ops.extend_from_slice(b"f\n");
            }
            if let Some(bw) = node.style.border_width_pt {
                if bw > 0.0 && w > 0.0 && h > 0.0 {
                    let bc = node
                        .style
                        .border_color
                        .map_or([0.0, 0.0, 0.0], |(r, g, b)| {
                            [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]
                        });
                    write_ops(
                        ops,
                        format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", bw, bc[0], bc[1], bc[2]),
                    );
                    apply_border_dash(ops, border_style);
                    emit_rect_path(ops, abs_x, pdf_y, w, h, border_radius);
                    ops.extend_from_slice(b"S\n");
                    reset_border_dash(ops, border_style);
                }
            }
        }

        let is_bold = node
            .style
            .font_weight
            .as_deref()
            .map_or(false, |w| w == "bold");

        match &node.content {
            LayoutContent::Field {
                value,
                field_kind,
                font_size,
                font_family,
            } => match field_kind {
                FieldKind::Checkbox | FieldKind::Radio => {
                    render_checkbox(abs_x, pdf_y, w, h, value, &node_config, ops)
                }
                FieldKind::Dropdown => render_dropdown(
                    abs_x,
                    pdf_y,
                    w,
                    h,
                    value,
                    *font_size,
                    *font_family,
                    &node.style,
                    &node_config,
                    ops,
                ),
                FieldKind::Button => render_button(
                    abs_x,
                    pdf_y,
                    w,
                    h,
                    value,
                    *font_size,
                    *font_family,
                    &node.style,
                    &node_config,
                    ops,
                ),
                FieldKind::Signature => {
                    render_signature(abs_x, pdf_y, w, h, value, &node.style, &node_config, ops)
                }
                _ => render_field(
                    abs_x,
                    pdf_y,
                    w,
                    h,
                    value,
                    *font_size,
                    *font_family,
                    &node.style,
                    &node_config,
                    ops,
                ),
            },
            LayoutContent::Text(text) => render_text(abs_x, pdf_y, text, &node_config, ops),
            LayoutContent::WrappedText {
                lines,
                font_size,
                text_align,
                font_family,
            } => render_multiline(
                abs_x,
                pdf_y,
                w,
                lines,
                *font_size,
                *text_align,
                *font_family,
                is_bold,
                mapper,
                abs_y,
                &node.style,
                &node_config,
                ops,
            ),
            LayoutContent::Image { data, mime_type } => {
                let img_name = format!("XImg{}", images.len());
                ops.extend(crate::image_bridge::render_image_ops(
                    &img_name, abs_x, pdf_y, w, h,
                ));
                images.push(ImageInfo {
                    name: img_name,
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                });
            }
            LayoutContent::Draw(draw_content) => {
                render_draw(draw_content, abs_x, pdf_y, w, h, ops);
            }
            LayoutContent::None => {}
        }

        if !node.children.is_empty() {
            render_nodes(&node.children, abs_x, abs_y, mapper, config, ops, images);
        }
    }
}

/// Emit a rectangle path with optional rounded corners.
fn emit_rect_path(ops: &mut Vec<u8>, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    if radius <= 0.0 {
        write_ops(
            ops,
            format_args!("{:.2} {:.2} {:.2} {:.2} re\n", x, y, w, h),
        );
    } else {
        let r = radius.min(w / 2.0).min(h / 2.0);
        let k = r * 0.5522847498;
        write_ops(
            ops,
            format_args!(
                "{:.2} {:.2} m\n\
                 {:.2} {:.2} l\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} l\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} l\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} l\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 h\n",
                x,
                y + r,
                x,
                y + h - r,
                x,
                y + h - r + k,
                x + r - k,
                y + h,
                x + r,
                y + h,
                x + w - r,
                y + h,
                x + w - r + k,
                y + h,
                x + w,
                y + h - r + k,
                x + w,
                y + h - r,
                x + w,
                y + r,
                x + w,
                y + r - k,
                x + w - r + k,
                y,
                x + w - r,
                y,
                x + r,
                y,
                x + r - k,
                y,
                x,
                y + r - k,
                x,
                y + r,
            ),
        );
    }
}

fn apply_border_dash(ops: &mut Vec<u8>, style: Option<&str>) {
    match style {
        Some("dashed") => write_ops(ops, format_args!("[3 2] 0 d\n")),
        Some("dotted") => write_ops(ops, format_args!("[1 1] 0 d\n")),
        _ => {}
    }
}

fn reset_border_dash(ops: &mut Vec<u8>, style: Option<&str>) {
    if matches!(style, Some("dashed") | Some("dotted")) {
        write_ops(ops, format_args!("[] 0 d\n"));
    }
}

/// Select the PDF font resource reference for a node.
///
/// Uses the embedded font from `font_map` when the typeface is resolved,
/// otherwise falls back to the standard Base14 fonts (F1/F2/F3).
fn resolve_font_ref<'a>(
    font_map: &'a HashMap<String, String>,
    node_style: &FormNodeStyle,
    font_family: FontFamily,
) -> &'a str {
    if let Some(typeface) = &node_style.font_family {
        // Try variant-specific key first (includes weight/posture).
        let vkey = font_variant_key(
            typeface,
            node_style.font_weight.as_deref(),
            node_style.font_style.as_deref(),
        );
        if let Some(mapped) = font_map.get(&vkey) {
            return mapped;
        }
        // Fallback to base typeface name.
        if let Some(mapped) = font_map.get(typeface) {
            return mapped;
        }
    }
    match font_family {
        FontFamily::Serif => "/F1",
        FontFamily::SansSerif => "/F2",
        FontFamily::Monospace => "/F3",
    }
}

/// Emit PDF text state operators for fontHorizontalScale (Tz) and letterSpacing (Tc).
/// Only emits operators when values differ from defaults (100% scale, 0 spacing).
fn emit_text_style_ops(node_style: &FormNodeStyle, ops: &mut Vec<u8>) {
    if let Some(h_scale) = node_style.font_horizontal_scale {
        if (h_scale - 1.0).abs() > 0.001 {
            write_ops(ops, format_args!("{:.1} Tz\n", h_scale * 100.0));
        }
    }
    if let Some(spacing) = node_style.letter_spacing_pt {
        if spacing.abs() > 0.001 {
            write_ops(ops, format_args!("{:.3} Tc\n", spacing));
        }
    }
}

/// Returns true if this node requests bold weight.
fn is_bold_style(node_style: &FormNodeStyle) -> bool {
    node_style
        .font_weight
        .as_deref()
        .map_or(false, |w| w == "bold")
}

/// Emit synthetic bold operators: fill+stroke rendering mode with thin stroke.
/// Uses text rendering mode 2 (fill then stroke) to simulate bold weight when
/// the actual bold font variant is unavailable.
fn emit_synthetic_bold_ops(
    node_style: &FormNodeStyle,
    font_size: f64,
    text_color: &[f64; 3],
    ops: &mut Vec<u8>,
) {
    if is_bold_style(node_style) {
        let stroke_w = font_size * 0.03;
        write_ops(
            ops,
            format_args!(
                "2 Tr\n{:.4} w\n{:.3} {:.3} {:.3} RG\n",
                stroke_w, text_color[0], text_color[1], text_color[2],
            ),
        );
    }
}

/// Reset synthetic bold state back to fill-only rendering.
fn reset_synthetic_bold_ops(node_style: &FormNodeStyle, ops: &mut Vec<u8>) {
    if is_bold_style(node_style) {
        write_ops(ops, format_args!("0 Tr\n"));
    }
}

/// Reset text style operators to defaults after a BT/ET block (for safety).
fn reset_text_style_ops(node_style: &FormNodeStyle, ops: &mut Vec<u8>) {
    if node_style
        .font_horizontal_scale
        .is_some_and(|s| (s - 1.0).abs() > 0.001)
    {
        write_ops(ops, format_args!("100 Tz\n"));
    }
    if node_style
        .letter_spacing_pt
        .is_some_and(|s| s.abs() > 0.001)
    {
        write_ops(ops, format_args!("0 Tc\n"));
    }
}

/// Calculate the ascender height in points for a given font size and metrics.
fn ascender_pt(font_metrics: &FontMetrics, font_size: f64) -> f64 {
    if let (Some(asc), Some(upem)) = (font_metrics.resolved_ascender, font_metrics.resolved_upem) {
        if upem > 0 {
            return asc as f64 / upem as f64 * font_size;
        }
    }
    font_size
}

/// Build a `FontMetrics` with resolved data injected from `config.font_metrics_data`.
///
/// Uses a variant key (including weight/posture) to look up the correct font
/// metrics. Falls back to the base typeface name if no variant entry exists.
fn build_font_metrics(
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
) -> FontMetrics {
    let mut metrics = FontMetrics {
        size: font_size,
        typeface: font_family,
        ..Default::default()
    };
    if let Some(typeface) = &node_style.font_family {
        let vkey = font_variant_key(
            typeface,
            node_style.font_weight.as_deref(),
            node_style.font_style.as_deref(),
        );
        let data = config
            .font_metrics_data
            .get(&vkey)
            .or_else(|| config.font_metrics_data.get(typeface));
        if let Some(data) = data {
            metrics.resolved_widths = Some(data.widths.clone());
            metrics.resolved_upem = Some(data.upem);
            metrics.resolved_ascender = Some(data.ascender);
            metrics.resolved_descender = Some(data.descender);
        }
    }
    metrics
}

fn render_field(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    // Adobe behavior: empty fields are invisible (no border/background)
    if value.is_empty() {
        return;
    }
    let border_radius = node_style.border_radius_pt.unwrap_or(0.0);
    let border_style = node_style.border_style.as_deref();

    if let Some(bg) = &config.background_color {
        write_ops(
            ops,
            format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
        );
        emit_rect_path(ops, x, pdf_y, w, h, border_radius);
        ops.extend_from_slice(b"f\n");
    }
    if config.draw_borders && config.border_width > 0.0 {
        write_ops(
            ops,
            format_args!(
                "{:.2} w\n{:.3} {:.3} {:.3} RG\n",
                config.border_width,
                config.border_color[0],
                config.border_color[1],
                config.border_color[2],
            ),
        );
        apply_border_dash(ops, border_style);
        emit_rect_path(ops, x, pdf_y, w, h, border_radius);
        ops.extend_from_slice(b"S\n");
        reset_border_dash(ops, border_style);
    }
    if !value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        let pad_left = node_style.margin_left_pt.unwrap_or(config.text_padding);
        let pad_right = node_style.margin_right_pt.unwrap_or(config.text_padding);
        let space_above = node_style.space_above_pt.unwrap_or(0.0);
        let content_w = (w - pad_left - pad_right).max(0.0);
        let metrics = build_font_metrics(fs, font_family, node_style, config);
        let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
        let text_w = metrics.measure_width(value);

        let idh_metrics = lookup_font_metrics(node_style, config);

        if text_w <= content_w || content_w <= 0.0 {
            let line_h = metrics.line_height_pt();
            let asc_pt = ascender_pt(&metrics, fs);
            let text_y = match node_style.v_align {
                Some(VerticalAlign::Middle) => pdf_y + (h - line_h) / 2.0,
                Some(VerticalAlign::Bottom) => pdf_y + space_above,
                _ => pdf_y + h - space_above - asc_pt,
            };
            let encoded = pdf_encode_text(value, idh_metrics);
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                ),
            );
            emit_synthetic_bold_ops(node_style, fs, &config.text_color, ops);
            emit_text_style_ops(node_style, ops);
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} Td\n{} Tj\n",
                    x + pad_left,
                    text_y,
                    encoded
                ),
            );
            reset_text_style_ops(node_style, ops);
            reset_synthetic_bold_ops(node_style, ops);
            ops.extend_from_slice(b"ET\n");
        } else {
            let lines = wrap_text(value, content_w, &metrics);
            let line_height = metrics.line_height_pt();
            let asc_pt = ascender_pt(&metrics, fs);
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                ),
            );
            emit_synthetic_bold_ops(node_style, fs, &config.text_color, ops);
            emit_text_style_ops(node_style, ops);
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} Td\n",
                    x + pad_left,
                    pdf_y + h - space_above - asc_pt,
                ),
            );
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    write_ops(ops, format_args!("0 {:.2} Td\n", -line_height));
                }
                let line_top = h - space_above - asc_pt - (i as f64 * line_height);
                if line_top < 0.0 {
                    break;
                }
                let encoded = pdf_encode_text(line, idh_metrics);
                write_ops(ops, format_args!("{} Tj\n", encoded));
            }
            reset_text_style_ops(node_style, ops);
            reset_synthetic_bold_ops(node_style, ops);
            ops.extend_from_slice(b"ET\n");
        }
    }
}

fn render_checkbox(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    // Adobe behavior: empty checkboxes are invisible
    if value.is_empty() {
        return;
    }
    let bw = config.border_width.max(0.5);
    write_ops(
        ops,
        format_args!(
            "q\n{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            bw,
            config.border_color[0],
            config.border_color[1],
            config.border_color[2],
            x,
            pdf_y,
            w,
            h
        ),
    );
    let checked = !value.is_empty()
        && !value.eq_ignore_ascii_case("0")
        && !value.eq_ignore_ascii_case("off")
        && !value.eq_ignore_ascii_case("false");
    if checked {
        let m = w.min(h) * 0.15;
        write_ops(
            ops,
            format_args!(
                "{:.2} w\n{:.3} {:.3} {:.3} RG\n\
                 {:.2} {:.2} m {:.2} {:.2} l S\n\
                 {:.2} {:.2} m {:.2} {:.2} l S\n",
                bw.max(1.0),
                config.text_color[0],
                config.text_color[1],
                config.text_color[2],
                x + m,
                pdf_y + m,
                x + w - m,
                pdf_y + h - m,
                x + m,
                pdf_y + h - m,
                x + w - m,
                pdf_y + m,
            ),
        );
    }
    write_ops(ops, format_args!("Q\n"));
}

fn render_dropdown(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    // Adobe behavior: empty dropdowns are invisible
    if value.is_empty() {
        return;
    }
    let border_radius = node_style.border_radius_pt.unwrap_or(0.0);

    if let Some(bg) = &config.background_color {
        write_ops(
            ops,
            format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
        );
        emit_rect_path(ops, x, pdf_y, w, h, border_radius);
        ops.extend_from_slice(b"f\n");
    }

    if config.draw_borders && config.border_width > 0.0 {
        write_ops(
            ops,
            format_args!(
                "{:.2} w\n{:.3} {:.3} {:.3} RG\n",
                config.border_width,
                config.border_color[0],
                config.border_color[1],
                config.border_color[2],
            ),
        );
        emit_rect_path(ops, x, pdf_y, w, h, border_radius);
        ops.extend_from_slice(b"S\n");
    }

    let arrow_w = h.min(12.0);

    if !value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        let _metrics = build_font_metrics(fs, font_family, node_style, config);
        let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
        let idh_metrics = lookup_font_metrics(node_style, config);

        let v_offset = pdf_y + h / 2.0 - fs / 2.0;
        let encoded = pdf_encode_text(value, idh_metrics);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                config.text_color[0],
                config.text_color[1],
                config.text_color[2],
                font_ref,
                fs,
            ),
        );
        emit_synthetic_bold_ops(node_style, fs, &config.text_color, ops);
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!(
                "{:.2} {:.2} Td\n{} Tj\n",
                x + 2.0,
                v_offset,
                encoded
            ),
        );
        reset_text_style_ops(node_style, ops);
        reset_synthetic_bold_ops(node_style, ops);
        ops.extend_from_slice(b"ET\n");
    }

    let arrow_x = x + w - arrow_w - 1.0;
    let arrow_y_center = pdf_y + h / 2.0;
    let arrow_size = arrow_w * 0.6;
    let arrow_char = "\u{25BC}";
    write_ops(
        ops,
        format_args!(
            "BT\n/F2 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            arrow_size,
            arrow_x,
            arrow_y_center - arrow_size / 2.0,
            arrow_char
        ),
    );
}

fn render_button(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    // Adobe behavior: empty buttons are invisible
    if value.is_empty() {
        return;
    }
    let border_radius = node_style.border_radius_pt.unwrap_or(1.0);
    let bw = config.border_width.max(1.0);

    let light_shade = [
        (config.border_color[0] + 0.3).min(1.0),
        (config.border_color[1] + 0.3).min(1.0),
        (config.border_color[2] + 0.3).min(1.0),
    ];
    let dark_shade = [
        (config.border_color[0] - 0.3).max(0.0),
        (config.border_color[1] - 0.3).max(0.0),
        (config.border_color[2] - 0.3).max(0.0),
    ];

    write_ops(
        ops,
        format_args!(
            "q\n{:.3} {:.3} {:.3} rg\n",
            light_shade[0], light_shade[1], light_shade[2]
        ),
    );
    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
    ops.extend_from_slice(b"f\n");

    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.2} w\n",
            dark_shade[0], dark_shade[1], dark_shade[2], bw
        ),
    );
    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
    ops.extend_from_slice(b"S\n");

    write_ops(ops, format_args!("{:.2} w\n", bw / 2.0));
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n",
            light_shade[0], light_shade[1], light_shade[2]
        ),
    );
    emit_rect_path(ops, x + 0.5, pdf_y + 0.5, w - 1.0, h - 1.0, border_radius);
    ops.extend_from_slice(b"S\n");

    if !value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        let metrics = build_font_metrics(fs, font_family, node_style, config);
        let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
        let idh_metrics = lookup_font_metrics(node_style, config);
        let text_w = metrics.measure_width(value);
        let text_x = x + (w - text_w) / 2.0;
        let v_offset = pdf_y + h / 2.0 - fs / 2.0;
        let encoded = pdf_encode_text(value, idh_metrics);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                config.text_color[0],
                config.text_color[1],
                config.text_color[2],
                font_ref,
                fs,
            ),
        );
        emit_synthetic_bold_ops(node_style, fs, &config.text_color, ops);
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!(
                "{:.2} {:.2} Td\n{} Tj\n",
                text_x,
                v_offset,
                encoded
            ),
        );
        reset_text_style_ops(node_style, ops);
        reset_synthetic_bold_ops(node_style, ops);
        ops.extend_from_slice(b"ET\n");
    }
    write_ops(ops, format_args!("Q\n"));
}

fn render_signature(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    // Adobe behavior: empty signatures are invisible
    if value.is_empty() {
        return;
    }
    let border_radius = node_style.border_radius_pt.unwrap_or(0.0);

    if let Some(bg) = &config.background_color {
        write_ops(
            ops,
            format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
        );
        emit_rect_path(ops, x, pdf_y, w, h, border_radius);
        ops.extend_from_slice(b"f\n");
    }

    write_ops(
        ops,
        format_args!(
            "{:.2} w\n{:.3} {:.3} {:.3} RG\n",
            config.border_width,
            config.border_color[0],
            config.border_color[1],
            config.border_color[2],
        ),
    );
    write_ops(ops, format_args!("[4 2] 0 d\n"));
    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
    ops.extend_from_slice(b"S\n");
    write_ops(ops, format_args!("[] 0 d\n"));

    if !value.is_empty() {
        let fs = config.default_font_size * 0.8;
        let text_x = x + config.text_padding;
        let v_offset = pdf_y + h / 2.0 - fs / 2.0;
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
                config.text_color[0],
                config.text_color[1],
                config.text_color[2],
                fs,
                text_x,
                v_offset,
                pdf_escape(value)
            ),
        );
    }
}

fn render_text(x: f64, pdf_y: f64, text: &str, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    if text.is_empty() {
        return;
    }
    let fs = config.default_font_size;
    let p = config.text_padding;
    let asc_pt = fs * 0.8;
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0],
            config.text_color[1],
            config.text_color[2],
            fs,
            x + p,
            pdf_y + p - asc_pt * 0.2,
            pdf_escape(text)
        ),
    );
}

#[allow(clippy::too_many_arguments)]
fn render_multiline(
    x: f64,
    _pdf_y: f64,
    container_width: f64,
    lines: &[String],
    font_size: f64,
    text_align: TextAlign,
    font_family: FontFamily,
    _is_bold: bool,
    mapper: &CoordinateMapper,
    abs_y_xfa: f64,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if lines.is_empty() {
        return;
    }
    let pad_left = node_style.margin_left_pt.unwrap_or(config.text_padding);
    let pad_right = node_style.margin_right_pt.unwrap_or(config.text_padding);
    let space_above = node_style.space_above_pt.unwrap_or(0.0);
    let font_metrics = build_font_metrics(font_size, font_family, node_style, config);
    let line_height = font_metrics.line_height_pt();
    let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            config.text_color[0], config.text_color[1], config.text_color[2], font_ref, font_size
        ),
    );
    emit_synthetic_bold_ops(node_style, font_size, &config.text_color, ops);
    emit_text_style_ops(node_style, ops);
    let ascender_pt = if let (Some(asc), Some(upem)) =
        (font_metrics.resolved_ascender, font_metrics.resolved_upem)
    {
        if upem > 0 {
            asc as f64 / upem as f64 * font_size
        } else {
            font_size
        }
    } else {
        font_size
    };
    let first_line_pdf_y = mapper.xfa_to_pdf_y(abs_y_xfa + space_above + ascender_pt, 0.0);
    let content_w = (container_width - pad_left - pad_right).max(0.0);
    let idh_metrics = lookup_font_metrics(node_style, config);
    let mut prev_x = x + pad_left;
    for (i, line) in lines.iter().enumerate() {
        let line_y = first_line_pdf_y - (i as f64 * line_height);
        let line_w = font_metrics.measure_width(line);
        let text_x = match text_align {
            TextAlign::Center => x + pad_left + ((content_w - line_w) / 2.0).max(0.0),
            TextAlign::Right => x + pad_left + (content_w - line_w).max(0.0),
            _ => x + pad_left,
        };
        if i == 0 {
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", text_x, line_y));
        } else {
            let dx = text_x - prev_x;
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", dx, -line_height));
        }
        prev_x = text_x;
        let encoded = pdf_encode_text(line, idh_metrics);
        write_ops(ops, format_args!("{} Tj\n", encoded));
    }
    reset_text_style_ops(node_style, ops);
    reset_synthetic_bold_ops(node_style, ops);
    ops.extend_from_slice(b"ET\n");
}

fn wrap_text(text: &str, max_width: f64, metrics: &FontMetrics) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else {
            let candidate = format!("{} {}", current, word);
            if metrics.measure_width(&candidate) <= max_width {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() && !text.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

fn pdf_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => r.push_str("\\("),
            ')' => r.push_str("\\)"),
            '\\' => r.push_str("\\\\"),
            '\x20'..='\x7e' => r.push(c),
            _ => {
                if let Some(b) = unicode_to_winansi(c) {
                    use std::fmt::Write;
                    let _ = write!(r, "\\{:03o}", b);
                } else {
                    r.push('?');
                }
            }
        }
    }
    r
}

/// Encode text for a PDF content stream, choosing Identity-H (hex glyph IDs)
/// when the font has embedded data, or WinAnsi parenthesized string otherwise.
fn pdf_encode_text(s: &str, metrics: Option<&FontMetricsData>) -> String {
    if let Some(data) = metrics {
        if let Some(ref font_bytes) = data.font_data {
            if let Ok(face) = ttf_parser::Face::parse(font_bytes, data.face_index) {
                let mut hex = String::with_capacity(s.len() * 4 + 2);
                hex.push('<');
                for ch in s.chars() {
                    let gid = face.glyph_index(ch).map(|g| g.0).unwrap_or(0);
                    use std::fmt::Write;
                    let _ = write!(hex, "{:04X}", gid);
                }
                hex.push('>');
                return hex;
            }
        }
    }
    format!("({})", pdf_escape(s))
}

/// Look up font metrics for a typeface from the render config.
///
/// Tries the variant key (with weight/posture) first, then falls back to
/// the plain typeface name.
fn lookup_font_metrics<'a>(
    node_style: &FormNodeStyle,
    config: &'a XfaRenderConfig,
) -> Option<&'a FontMetricsData> {
    node_style
        .font_family
        .as_ref()
        .and_then(|tf| {
            let vkey = font_variant_key(
                tf,
                node_style.font_weight.as_deref(),
                node_style.font_style.as_deref(),
            );
            config
                .font_metrics_data
                .get(&vkey)
                .or_else(|| config.font_metrics_data.get(tf))
        })
        .filter(|m| m.font_data.is_some())
}

fn render_draw(
    draw_content: &DrawContent,
    abs_x: f64,
    pdf_y: f64,
    _w: f64,
    container_h: f64,
    ops: &mut Vec<u8>,
) {
    match draw_content {
        DrawContent::Text(text) => {
            if !text.is_empty() {
                let fs = 10.0;
                write_ops(
                    ops,
                    format_args!(
                        "BT\n0 0 0 rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
                        fs,
                        abs_x,
                        pdf_y,
                        pdf_escape(text)
                    ),
                );
            }
        }
        DrawContent::Line { x1, y1, x2, y2 } => {
            let start_x = abs_x + x1;
            let start_y = pdf_y + container_h - y1;
            let end_x = abs_x + x2;
            let end_y = pdf_y + container_h - y2;
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
                    start_x, start_y, end_x, end_y
                ),
            );
        }
        DrawContent::Rectangle { x, y, w, h, radius } => {
            let rx = abs_x + x;
            let ry = pdf_y + container_h - y - h;
            if *radius <= 0.0 {
                write_ops(
                    ops,
                    format_args!("{:.2} {:.2} {:.2} {:.2} re\nS\n", rx, ry, w, h),
                );
            } else {
                let r = radius.min(w / 2.0).min(h / 2.0);
                let k = r * 0.5522847498;
                write_ops(
                    ops,
                    format_args!(
                        "{:.2} {:.2} m\n\
                         {:.2} {:.2} l\n\
                         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                         {:.2} {:.2} l\n\
                         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                         {:.2} {:.2} l\n\
                         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                         {:.2} {:.2} l\n\
                         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                         h\nS\n",
                        rx,
                        ry + r,
                        rx,
                        ry + h - r,
                        rx,
                        ry + h - r + k,
                        rx + r - k,
                        ry + h,
                        rx + r,
                        ry + h,
                        rx + w - r,
                        ry + h,
                        rx + w - r + k,
                        ry + h,
                        rx + w,
                        ry + h - r + k,
                        rx + w,
                        ry + h - r,
                        rx + w,
                        ry + r,
                        rx + w,
                        ry + r - k,
                        rx + w - r + k,
                        ry,
                        rx + w - r,
                        ry,
                        rx + r,
                        ry,
                        rx + r - k,
                        ry,
                        rx,
                        ry + r - k,
                        rx,
                        ry + r,
                    ),
                );
            }
        }
        DrawContent::Arc {
            x,
            y,
            w,
            h,
            start_angle,
            sweep_angle,
        } => {
            let cx = abs_x + x + w / 2.0;
            let cy = pdf_y + container_h - y - h / 2.0;
            let rx = w / 2.0;
            let ry = h / 2.0;
            let start_rad = start_angle.to_radians();
            let sweep_rad = sweep_angle.to_radians();
            let end_angle = start_rad + sweep_rad;
            let k = 0.5522847498;
            let cos_start = start_rad.cos();
            let sin_start = start_rad.sin();
            let cos_end = end_angle.cos();
            let sin_end = end_angle.sin();
            let p1x = cx + rx * cos_start;
            let p1y = cy + ry * sin_start;
            let p2x = cx + rx * cos_end;
            let p2y = cy + ry * sin_end;
            let cp1x = cx - rx * k * sin_start;
            let cp1y = cy + ry * k * cos_start;
            let cp2x = cx + rx * k * sin_end;
            let cp2y = cy - ry * k * cos_end;
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\nS\n",
                    p1x, p1y, cp1x, cp1y, cp2x, cp2y, p2x, p2y
                ),
            );
        }
    }
}

fn unicode_to_winansi(c: char) -> Option<u8> {
    let cp = c as u32;
    if (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    match c {
        '\u{20AC}' => Some(0x80),
        '\u{201A}' => Some(0x82),
        '\u{0192}' => Some(0x83),
        '\u{201E}' => Some(0x84),
        '\u{2026}' => Some(0x85),
        '\u{2020}' => Some(0x86),
        '\u{2021}' => Some(0x87),
        '\u{02C6}' => Some(0x88),
        '\u{2030}' => Some(0x89),
        '\u{0160}' => Some(0x8A),
        '\u{2039}' => Some(0x8B),
        '\u{0152}' => Some(0x8C),
        '\u{017D}' => Some(0x8E),
        '\u{2018}' => Some(0x91),
        '\u{2019}' => Some(0x92),
        '\u{201C}' => Some(0x93),
        '\u{201D}' => Some(0x94),
        '\u{2022}' => Some(0x95),
        '\u{2013}' => Some(0x96),
        '\u{2014}' => Some(0x97),
        '\u{02DC}' => Some(0x98),
        '\u{2122}' => Some(0x99),
        '\u{0161}' => Some(0x9A),
        '\u{203A}' => Some(0x9B),
        '\u{0153}' => Some(0x9C),
        '\u{017E}' => Some(0x9E),
        '\u{0178}' => Some(0x9F),
        _ => None,
    }
}

fn write_ops(buf: &mut Vec<u8>, args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = buf.write_fmt(args);
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::FormNodeId;
    use xfa_layout_engine::types::Rect;

    fn make_page(nodes: Vec<LayoutNode>) -> LayoutPage {
        LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes,
        }
    }

    fn make_field_node(x: f64, y: f64, w: f64, h: f64, value: &str) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, w, h),
            name: "field1".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind: FieldKind::Text,
                font_size: 0.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
        }
    }

    fn make_styled_field(
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        value: &str,
        style: FormNodeStyle,
    ) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, w, h),
            name: "styled".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind: FieldKind::Text,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style,
        }
    }

    #[test]
    fn coordinate_mapping() {
        let mapper = CoordinateMapper::new(792.0, 612.0);
        assert!((mapper.xfa_to_pdf_y(0.0, 20.0) - 772.0).abs() < 0.001);
    }

    fn overlay_str(page: &LayoutPage) -> String {
        let o = generate_page_overlay(page, &XfaRenderConfig::default()).unwrap();
        String::from_utf8_lossy(&o.content_stream).into_owned()
    }

    #[test]
    fn empty_page_overlay() {
        let s = overlay_str(&make_page(vec![]));
        assert!(s.starts_with("q\n") && s.ends_with("Q\n"));
    }

    #[test]
    fn field_renders_text() {
        let s = overlay_str(&make_page(vec![make_field_node(
            10.0, 10.0, 100.0, 20.0, "Hello",
        )]));
        assert!(s.contains("(Hello) Tj") && s.contains("BT") && s.contains("ET"));
    }

    #[test]
    fn empty_field_no_text() {
        let s = overlay_str(&make_page(vec![make_field_node(
            10.0, 10.0, 100.0, 20.0, "",
        )]));
        assert!(!s.contains("BT"));
    }

    #[test]
    fn all_overlays() {
        let layout = LayoutDom {
            pages: vec![
                make_page(vec![make_field_node(0.0, 0.0, 50.0, 20.0, "P1")]),
                make_page(vec![make_field_node(0.0, 0.0, 50.0, 20.0, "P2")]),
            ],
        };
        assert_eq!(
            generate_all_overlays(&layout, &XfaRenderConfig::default())
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn pdf_escape_winansi_encoding() {
        assert_eq!(pdf_escape("Hello"), "Hello");
        assert_eq!(pdf_escape("a(b)c\\d"), "a\\(b\\)c\\\\d");
        assert_eq!(pdf_escape("\u{2013}"), "\\226");
        assert_eq!(pdf_escape("\u{2022}"), "\\225");
        assert_eq!(pdf_escape("\u{00A9}"), "\\251");
        assert_eq!(pdf_escape("\u{4E16}"), "?");
    }

    fn styled_overlay_str(node: LayoutNode) -> String {
        let o = generate_page_overlay(&make_page(vec![node]), &XfaRenderConfig::default()).unwrap();
        String::from_utf8_lossy(&o.content_stream).into_owned()
    }

    #[test]
    fn rounded_border_emits_bezier() {
        let style = FormNodeStyle {
            border_width_pt: Some(1.0),
            border_radius_pt: Some(5.0),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 10.0, 100.0, 20.0, "Hi", style));
        assert!(s.contains(" c\n"), "expected Bezier");
        assert!(s.contains("h\n"), "expected close-path");
    }

    #[test]
    fn dashed_border_emits_dash_pattern() {
        let style = FormNodeStyle {
            border_width_pt: Some(1.0),
            border_style: Some("dashed".to_string()),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 10.0, 100.0, 20.0, "Hi", style));
        assert!(s.contains("[3 2] 0 d"), "expected dash");
        assert!(s.contains("[] 0 d"), "expected reset");
    }

    #[test]
    fn para_margins_applied() {
        let style = FormNodeStyle {
            margin_left_pt: Some(5.0),
            margin_right_pt: Some(3.0),
            space_above_pt: Some(2.0),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 10.0, 200.0, 30.0, "Test", style));
        assert!(s.contains("15.00"), "expected margin_left offset 10+5=15");
    }

    #[test]
    fn v_align_middle() {
        let style = FormNodeStyle {
            v_align: Some(VerticalAlign::Middle),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(0.0, 0.0, 200.0, 40.0, "Mid", style));
        assert!(s.contains("(Mid) Tj"));
    }

    #[test]
    fn pdf_escape_polish_chars_fallback() {
        // Without Identity-H font data, Polish chars should fall back to '?'
        assert_eq!(pdf_escape("łżść"), "????");
    }

    #[test]
    fn pdf_encode_text_winansi_fallback() {
        // Without font data, pdf_encode_text wraps in parentheses like pdf_escape
        let encoded = pdf_encode_text("Hello", None);
        assert_eq!(encoded, "(Hello)");
    }

    #[test]
    fn pdf_encode_text_identity_h() {
        // With Identity-H font data, text should be encoded as hex glyph IDs
        let metrics = FontMetricsData {
            widths: vec![500; 256],
            upem: 1000,
            ascender: 800,
            descender: -200,
            font_data: None,
            face_index: 0,
        };
        // Without font_data, should fall back to WinAnsi
        let encoded = pdf_encode_text("AB", Some(&metrics));
        assert_eq!(encoded, "(AB)");
    }
}
