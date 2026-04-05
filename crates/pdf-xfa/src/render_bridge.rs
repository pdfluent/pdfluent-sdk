//! XFA layout output to PDF content stream overlay generation.
//!
//! Converts LayoutDom (from xfa-layout-engine) into PDF content stream
//! operators that can be overlaid on existing PDF pages.
//!
//! Coordinate mapping: XFA uses top-left origin (y grows downward),
//! PDF uses bottom-left origin (y grows upward).

use crate::error::Result;
use std::collections::HashMap;
use xfa_layout_engine::form::{FieldKind, FormNodeStyle};
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

/// Build a `FontMetrics` with resolved data injected from `config.font_metrics_data`.
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
        if let Some(data) = config.font_metrics_data.get(typeface) {
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

        if text_w <= content_w || content_w <= 0.0 {
            let line_h = metrics.line_height_pt();
            let text_y = match node_style.v_align {
                Some(VerticalAlign::Middle) => pdf_y + (h - line_h) / 2.0,
                Some(VerticalAlign::Bottom) => pdf_y + space_above,
                _ => pdf_y + h - space_above - fs,
            };
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                    x + pad_left,
                    text_y,
                    pdf_escape(value)
                ),
            );
        } else {
            let lines = wrap_text(value, content_w, &metrics);
            let line_height = metrics.line_height_pt();
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                    x + pad_left,
                    pdf_y + h - space_above - fs,
                ),
            );
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    write_ops(ops, format_args!("0 {:.2} Td\n", -line_height));
                }
                let line_top = h - space_above - fs - (i as f64 * line_height);
                if line_top < 0.0 {
                    break;
                }
                write_ops(ops, format_args!("({}) Tj\n", pdf_escape(line)));
            }
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

fn render_text(x: f64, pdf_y: f64, text: &str, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    if text.is_empty() {
        return;
    }
    let fs = config.default_font_size;
    let p = config.text_padding;
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0],
            config.text_color[1],
            config.text_color[2],
            fs,
            x + p,
            pdf_y + p,
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
    let ascender_pt = if let (Some(asc), Some(upem)) = (
        font_metrics.resolved_ascender,
        font_metrics.resolved_upem,
    ) {
        if upem > 0 { asc as f64 / upem as f64 * font_size } else { font_size }
    } else {
        font_size
    };
    let first_line_pdf_y = mapper.xfa_to_pdf_y(abs_y_xfa + space_above + ascender_pt, 0.0);
    let content_w = (container_width - pad_left - pad_right).max(0.0);
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
        write_ops(ops, format_args!("({}) Tj\n", pdf_escape(line)));
    }
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
}
