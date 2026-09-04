//! Abstract paint commands for XFA layout rendering.
//!
//! Converts XFA layout output into renderer-agnostic paint commands.
//! These commands can be consumed by any backend: Device trait, content stream,
//! SVG, etc.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_interpret::color::Color;
use xfa_layout_engine::form::{FieldKind, FormNodeStyle};
use xfa_layout_engine::layout::{LayoutContent, LayoutNode, LayoutPage};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::TextAlign;

use crate::render_bridge::XfaRenderConfig;

/// An abstract rendering command from XFA layout.
/// Can be consumed by any renderer (Device trait, content stream, SVG, etc.)
#[derive(Debug, Clone)]
pub enum XfaPaintCommand {
    /// Fill a rectangle with a solid color.
    FillRect {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Fill color.
        color: Color,
    },
    /// Stroke a rectangle outline.
    StrokeRect {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Stroke color.
        color: Color,
        /// Line width.
        width: f64,
    },
    /// Draw a text string at a position.
    DrawText {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Text content.
        text: String,
        /// Font family for selecting the PDF font resource.
        font_family: FontFamily,
        /// Font size in points.
        font_size: f64,
        /// Text color.
        color: Color,
    },
    /// Draw multiple lines of text with alignment support.
    DrawMultilineText {
        /// X coordinate of the container left edge in PDF points.
        x: f64,
        /// Y coordinate of the first baseline in PDF points (bottom-left origin).
        y: f64,
        /// Text lines.
        lines: Vec<String>,
        /// Font family for selecting the PDF font resource.
        font_family: FontFamily,
        /// Font size in points.
        font_size: f64,
        /// Line height in points.
        line_height: f64,
        /// Text color.
        color: Color,
        /// Horizontal text alignment.
        text_align: TextAlign,
        /// Container width for alignment calculation.
        container_width: f64,
        /// Text padding from container edges.
        text_padding: f64,
    },
    /// Draw an image.
    DrawImage {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Raw image data.
        image_data: Vec<u8>,
        /// MIME type ("image/jpeg" or "image/png").
        mime_type: String,
    },
    /// Draw a checkbox or radio button.
    DrawCheckbox {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Whether the checkbox is checked.
        checked: bool,
        /// Border color (RGB 0-1).
        border_color: [f64; 3],
        /// Checkmark color (RGB 0-1).
        check_color: [f64; 3],
        /// Border line width.
        border_width: f64,
    },
}

/// Create a per-node config by applying XFA template style overrides to the
/// global config. Returns the original config unchanged if the node has no
/// style overrides (common case — avoids allocation).
fn apply_node_style(config: &XfaRenderConfig, style: &FormNodeStyle) -> XfaRenderConfig {
    let mut cfg = config.clone();

    // Apply background color — skip white (would cover underlying page content).
    if let Some((r, g, b)) = style.bg_color {
        if !(r >= 250 && g >= 250 && b >= 250) {
            cfg.background_color = Some([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]);
        }
    }

    // Apply border from the XFA template.
    // Only draw borders when explicitly specified; otherwise match Adobe behavior.
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

    // Apply text color.
    if let Some((r, g, b)) = style.text_color {
        cfg.text_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
    }

    if let Some(mark) = &style.check_button_mark {
        cfg.check_button_mark = Some(mark.clone());
    }

    cfg
}

/// Convert an XFA layout page into abstract paint commands.
pub fn layout_to_commands(page: &LayoutPage, config: &XfaRenderConfig) -> Vec<XfaPaintCommand> {
    let mut commands = Vec::new();
    let page_height = page.height;
    for node in &page.nodes {
        emit_node_commands(node, 0.0, 0.0, page_height, config, &mut commands);
    }
    commands
}

fn emit_node_commands(
    node: &LayoutNode,
    parent_x: f64,
    parent_y: f64,
    page_height: f64,
    config: &XfaRenderConfig,
    commands: &mut Vec<XfaPaintCommand>,
) {
    let abs_x = node.rect.x + parent_x;
    let abs_y = node.rect.y + parent_y;
    let w = node.rect.width;
    let h = node.rect.height;
    // Convert from top-left (XFA) to bottom-left (PDF) origin
    let pdf_y = page_height - abs_y - h;

    // Apply per-node style overrides from the XFA template.
    let node_config = apply_node_style(config, &node.style);

    // Draw background fill and borders for non-Field nodes (Draw, Subform, etc.)
    // Only when the XFA template explicitly defines bg/border styles.
    // Fields handle their own bg/borders below.
    if !matches!(node.content, LayoutContent::Field { .. }) {
        // Background: only from explicit node style (set by apply_node_style).
        if let Some(bg) = &node_config.background_color {
            commands.push(XfaPaintCommand::FillRect {
                x: abs_x,
                y: pdf_y,
                w,
                h,
                color: Color::from_device_rgb(bg[0] as f32, bg[1] as f32, bg[2] as f32),
            });
        }
        // Borders: only when the XFA template explicitly set border_width_pt.
        if let Some(bw) = node.style.border_width_pt {
            if bw > 0.0 && w > 0.0 && h > 0.0 {
                let bc = node
                    .style
                    .border_color
                    .map_or([0.0, 0.0, 0.0], |(r, g, b)| {
                        [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]
                    });
                commands.push(XfaPaintCommand::StrokeRect {
                    x: abs_x,
                    y: pdf_y,
                    w,
                    h,
                    color: Color::from_device_rgb(bc[0] as f32, bc[1] as f32, bc[2] as f32),
                    width: bw,
                });
            }
        }
    }

    match &node.content {
        LayoutContent::Field {
            value,
            field_kind,
            font_size,
            font_family,
        } => match field_kind {
            FieldKind::Checkbox | FieldKind::Radio => {
                let checked = !value.is_empty()
                    && !value.eq_ignore_ascii_case("0")
                    && !value.eq_ignore_ascii_case("off")
                    && !value.eq_ignore_ascii_case("false");
                commands.push(XfaPaintCommand::DrawCheckbox {
                    x: abs_x,
                    y: pdf_y,
                    w,
                    h,
                    checked,
                    border_color: node_config.border_color,
                    check_color: node_config.text_color,
                    border_width: node_config.border_width.max(0.5),
                });
            }
            _ => {
                emit_field_commands(
                    abs_x,
                    pdf_y,
                    w,
                    h,
                    value,
                    *font_size,
                    *font_family,
                    &node_config,
                    commands,
                );
            }
        },
        LayoutContent::Text(text) => {
            if !text.is_empty() {
                let text_color = make_color(&node_config.text_color);
                commands.push(XfaPaintCommand::DrawText {
                    x: abs_x + node_config.text_padding,
                    y: pdf_y + node_config.text_padding,
                    text: text.clone(),
                    font_family: FontFamily::SansSerif,
                    font_size: node_config.default_font_size,
                    color: text_color,
                });
            }
        }
        LayoutContent::WrappedText {
            lines,
            font_size,
            text_align,
            font_family,
            ..
        } => {
            let fs = *font_size;
            let line_height = fs * 1.2;
            if !lines.is_empty() {
                let text_color = make_color(&node_config.text_color);
                // Place first baseline at font_size below the element's XFA top.
                // Do NOT add text_padding vertically: draw elements often have tight
                // height budgets (h ≈ font_size).
                let first_line_y = page_height - abs_y - fs;
                commands.push(XfaPaintCommand::DrawMultilineText {
                    x: abs_x,
                    y: first_line_y,
                    lines: lines.clone(),
                    font_family: *font_family,
                    font_size: fs,
                    line_height,
                    color: text_color,
                    text_align: *text_align,
                    container_width: w,
                    text_padding: node_config.text_padding,
                });
            }
        }
        LayoutContent::Image { .. } => {}
        LayoutContent::Draw(_) => {}
        LayoutContent::None => {}
    }

    // Pass the GLOBAL config to children, not node_config — background_color
    // and other style properties should not cascade from parent to children.
    for child in &node.children {
        emit_node_commands(child, abs_x, abs_y, page_height, config, commands);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_field_commands(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    config: &XfaRenderConfig,
    commands: &mut Vec<XfaPaintCommand>,
) {
    // Background fill.
    if let Some(bg) = &config.background_color {
        commands.push(XfaPaintCommand::FillRect {
            x,
            y: pdf_y,
            w,
            h,
            color: Color::from_device_rgb(bg[0] as f32, bg[1] as f32, bg[2] as f32),
        });
    }
    // Border.
    if config.draw_borders && config.border_width > 0.0 {
        commands.push(XfaPaintCommand::StrokeRect {
            x,
            y: pdf_y,
            w,
            h,
            color: make_color(&config.border_color),
            width: config.border_width,
        });
    }
    // Text.
    if !value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        let p = config.text_padding;
        let content_w = (w - p * 2.0).max(0.0);
        let metrics = FontMetrics {
            size: fs,
            typeface: font_family,
            ..Default::default()
        };
        let text_w = metrics.measure_width(value);
        let text_color = make_color(&config.text_color);

        if text_w <= content_w || content_w <= 0.0 {
            // Single line — fits within field.
            commands.push(XfaPaintCommand::DrawText {
                x: x + p,
                y: pdf_y + p,
                text: value.to_string(),
                font_family,
                font_size: fs,
                color: text_color,
            });
        } else {
            // Multi-line: word-wrap within field width.
            let lines = wrap_text(value, content_w, &metrics);
            let line_height = fs * 1.2;
            commands.push(XfaPaintCommand::DrawMultilineText {
                x,
                y: pdf_y + h - p - fs,
                lines,
                font_family,
                font_size: fs,
                line_height,
                color: text_color,
                text_align: TextAlign::Left,
                container_width: w,
                text_padding: p,
            });
        }
    }
}

fn make_color(rgb: &[f64; 3]) -> Color {
    Color::from_device_rgb(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32)
}

/// Word-wrap text to fit within `max_width` using `metrics` for measurement.
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

/// Map a font family to the PDF font resource reference.
fn font_family_to_ref(ff: FontFamily) -> &'static str {
    match ff {
        FontFamily::Serif => "/F1",
        FontFamily::SansSerif => "/F2",
        FontFamily::Monospace => "/F3",
    }
}

/// Execute paint commands and return PDF content stream bytes.
///
/// The returned content stream includes the save/normalize state (q/Q) wrappers.
/// Image XObjects are referenced as /Im0, /Im1, etc. and must be added
/// to the page's resource dictionary separately.
pub fn execute_commands(commands: &[XfaPaintCommand]) -> Vec<u8> {
    let mut ops = Vec::new();
    ops.extend_from_slice(b"q\n");

    let mut image_index = 0usize;

    for cmd in commands {
        match cmd {
            XfaPaintCommand::FillRect { x, y, w, h, color } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                ops.extend(
                    format!(
                        "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        x,
                        y,
                        w,
                        h
                    )
                    .bytes(),
                );
            }
            XfaPaintCommand::StrokeRect {
                x,
                y,
                w,
                h,
                color,
                width,
            } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                ops.extend(format!("{:.2} w\n", width).bytes());
                ops.extend(
                    format!(
                        "{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        x,
                        y,
                        w,
                        h
                    )
                    .bytes(),
                );
            }
            XfaPaintCommand::DrawText {
                x,
                y,
                text,
                font_family,
                font_size,
                color,
            } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                let font_ref = font_family_to_ref(*font_family);
                ops.extend(
                    format!(
                        "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        font_ref,
                        font_size,
                        x,
                        y,
                        pdf_escape(text)
                    )
                    .bytes(),
                );
            }
            XfaPaintCommand::DrawMultilineText {
                x,
                y,
                lines,
                font_family,
                font_size,
                line_height,
                color,
                text_align,
                container_width,
                text_padding,
            } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                let font_ref = font_family_to_ref(*font_family);
                let p = *text_padding;
                let content_w = (container_width - p * 2.0).max(0.0);
                let metrics = FontMetrics {
                    size: *font_size,
                    typeface: *font_family,
                    ..Default::default()
                };
                ops.extend(
                    format!(
                        "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        font_ref,
                        font_size
                    )
                    .bytes(),
                );
                let mut prev_tx = x + p;
                for (i, line) in lines.iter().enumerate() {
                    let line_w = metrics.measure_width(line);
                    let text_x = match text_align {
                        TextAlign::Center => x + p + ((content_w - line_w) / 2.0).max(0.0),
                        TextAlign::Right => x + p + (content_w - line_w).max(0.0),
                        _ => x + p,
                    };
                    if i == 0 {
                        ops.extend(format!("{:.2} {:.2} Td\n", text_x, y).bytes());
                    } else {
                        let dx = text_x - prev_tx;
                        ops.extend(format!("{:.2} {:.2} Td\n", dx, -line_height).bytes());
                    }
                    prev_tx = text_x;
                    ops.extend(format!("({}) Tj\n", pdf_escape(line)).bytes());
                }
                ops.extend_from_slice(b"ET\n");
            }
            XfaPaintCommand::DrawImage {
                x,
                y,
                w,
                h,
                image_data: _,
                mime_type: _,
            } => {
                ops.extend(
                    format!(
                        "q\n{:.2} 0 0 {:.2} {:.2} {:.2} cm\n/Im{} Do\nQ\n",
                        w, h, x, y, image_index
                    )
                    .bytes(),
                );
                image_index += 1;
            }
            XfaPaintCommand::DrawCheckbox {
                x,
                y,
                w,
                h,
                checked,
                border_color,
                check_color,
                border_width,
            } => {
                // Draw checkbox border (square box).
                ops.extend(
                    format!(
                        "q\n{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
                        border_width, border_color[0], border_color[1], border_color[2], x, y, w, h
                    )
                    .bytes(),
                );
                if *checked {
                    // Draw an X mark inside the box.
                    let m = w.min(*h) * 0.15;
                    ops.extend(
                        format!(
                            "{:.2} w\n{:.3} {:.3} {:.3} RG\n\
                             {:.2} {:.2} m {:.2} {:.2} l S\n\
                             {:.2} {:.2} m {:.2} {:.2} l S\n",
                            border_width.max(1.0),
                            check_color[0],
                            check_color[1],
                            check_color[2],
                            x + m,
                            y + m,
                            x + w - m,
                            y + h - m,
                            x + m,
                            y + h - m,
                            x + w - m,
                            y + m,
                        )
                        .bytes(),
                    );
                }
                ops.extend_from_slice(b"Q\n");
            }
        }
    }

    ops.extend_from_slice(b"Q\n");
    ops
}

/// Escape a Unicode string for use inside a PDF literal string `(…)`.
///
/// The fonts we register use WinAnsiEncoding, so every character must be
/// mapped to its single-byte WinAnsi code point. Characters outside the
/// WinAnsi range are replaced with `?`. Bytes outside printable ASCII
/// (0x20–0x7E) are emitted as octal escapes `\NNN`.
fn pdf_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => r.push_str("\\("),
            ')' => r.push_str("\\)"),
            '\\' => r.push_str("\\\\"),
            // Printable ASCII passes through directly.
            '\x20'..='\x7e' => r.push(c),
            _ => {
                if let Some(b) = unicode_to_winansi(c) {
                    // Emit as octal escape for non-ASCII WinAnsi bytes.
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

/// Map a Unicode code point to its WinAnsiEncoding byte value.
///
/// Returns `None` for characters that have no WinAnsi representation.
/// Covers the 0x80–0x9F range (where WinAnsi differs from Latin-1) and
/// the 0xA0–0xFF Latin-1 supplement range.
fn unicode_to_winansi(c: char) -> Option<u8> {
    // Latin-1 Supplement range 0xA0–0xFF maps 1:1.
    let cp = c as u32;
    if (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    // WinAnsi 0x80–0x9F special mappings (Windows-1252).
    match c {
        '\u{20AC}' => Some(0x80), // €
        '\u{201A}' => Some(0x82), // ‚
        '\u{0192}' => Some(0x83), // ƒ
        '\u{201E}' => Some(0x84), // „
        '\u{2026}' => Some(0x85), // …
        '\u{2020}' => Some(0x86), // †
        '\u{2021}' => Some(0x87), // ‡
        '\u{02C6}' => Some(0x88), // ˆ
        '\u{2030}' => Some(0x89), // ‰
        '\u{0160}' => Some(0x8A), // Š
        '\u{2039}' => Some(0x8B), // ‹
        '\u{0152}' => Some(0x8C), // Œ
        '\u{017D}' => Some(0x8E), // Ž
        '\u{2018}' => Some(0x91), // '
        '\u{2019}' => Some(0x92), // '
        '\u{201C}' => Some(0x93), // "
        '\u{201D}' => Some(0x94), // "
        '\u{2022}' => Some(0x95), // •  (bullet)
        '\u{2013}' => Some(0x96), // –  (en-dash)
        '\u{2014}' => Some(0x97), // —  (em-dash)
        '\u{02DC}' => Some(0x98), // ˜
        '\u{2122}' => Some(0x99), // ™
        '\u{0161}' => Some(0x9A), // š
        '\u{203A}' => Some(0x9B), // ›
        '\u{0153}' => Some(0x9C), // œ
        '\u{017E}' => Some(0x9E), // ž
        '\u{0178}' => Some(0x9F), // Ÿ
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_bridge::XfaRenderConfig;
    use xfa_layout_engine::layout::{LayoutNode, LayoutPage};
    use xfa_layout_engine::types::Rect;

    fn test_config() -> XfaRenderConfig {
        XfaRenderConfig {
            default_font: "Helvetica".into(),
            default_font_size: 10.0,
            draw_borders: true,
            border_width: 0.5,
            border_color: [0.0, 0.0, 0.0],
            text_color: [0.0, 0.0, 0.0],
            background_color: Some([1.0, 1.0, 1.0]),
            text_padding: 2.0,
            font_map: std::sync::Arc::new(std::collections::HashMap::new()),
            font_metrics_data: std::sync::Arc::new(std::collections::HashMap::new()),
            check_button_mark: None,
            field_values_only: false,
        }
    }

    fn field_node(name: &str, x: f64, y: f64, w: f64, h: f64, value: &str) -> LayoutNode {
        LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            name: name.into(),
            content: LayoutContent::Field {
                value: value.into(),
                field_kind: xfa_layout_engine::form::FieldKind::Text,
                font_size: 0.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        }
    }

    fn field_node_with_border(
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        value: &str,
    ) -> LayoutNode {
        LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            name: name.into(),
            content: LayoutContent::Field {
                value: value.into(),
                field_kind: xfa_layout_engine::form::FieldKind::Text,
                font_size: 0.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: FormNodeStyle {
                border_width_pt: Some(0.5),
                border_color: Some((0, 0, 0)),
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        }
    }

    #[test]
    fn field_with_value_and_border_emits_fill_stroke_text() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node_with_border(
                "name", 10.0, 10.0, 200.0, 20.0, "Hello",
            )],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 3); // FillRect + StrokeRect + DrawText
        assert!(matches!(cmds[0], XfaPaintCommand::FillRect { .. }));
        assert!(matches!(cmds[1], XfaPaintCommand::StrokeRect { .. }));
        assert!(matches!(cmds[2], XfaPaintCommand::DrawText { .. }));
    }

    #[test]
    fn field_without_border_style_no_stroke() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "Hello")],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        // Default FormNodeStyle has no border_width_pt → no StrokeRect
        assert_eq!(cmds.len(), 2); // FillRect + DrawText
        assert!(matches!(cmds[0], XfaPaintCommand::FillRect { .. }));
        assert!(matches!(cmds[1], XfaPaintCommand::DrawText { .. }));
    }

    #[test]
    fn empty_field_no_text_command() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "")],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 1); // FillRect only, no border, no text
    }

    #[test]
    fn transparent_background_no_fill() {
        let mut config = test_config();
        config.background_color = None;
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "Hi")],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &config);
        assert_eq!(cmds.len(), 1); // DrawText only, no FillRect, no border
        assert!(matches!(cmds[0], XfaPaintCommand::DrawText { .. }));
    }

    #[test]
    fn multiline_text_emits_multiline_command() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![LayoutNode {
                form_node: xfa_layout_engine::form::FormNodeId(0),
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 200.0,
                    height: 60.0,
                },
                name: "memo".into(),
                content: LayoutContent::WrappedText {
                    lines: vec!["Line 1".into(), "Line 2".into()],
                    first_line_of_para: vec![true, false],
                    font_size: 10.0,
                    text_align: xfa_layout_engine::types::TextAlign::Left,
                    font_family: xfa_layout_engine::text::FontFamily::SansSerif,
                    space_above_pt: None,
                    space_below_pt: None,
                    from_field: false,
                },
                children: vec![],
                style: Default::default(),
                display_items: vec![],
                save_items: vec![],
            }],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        // FillRect (from bg) + DrawMultilineText
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], XfaPaintCommand::FillRect { .. }));
        assert!(matches!(cmds[1], XfaPaintCommand::DrawMultilineText { .. }));
    }

    #[test]
    fn multiple_nodes_coordinate_mapping() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![
                field_node("a", 10.0, 10.0, 100.0, 20.0, "A"),
                field_node("b", 10.0, 40.0, 100.0, 20.0, "B"),
            ],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        // 2 fields × 2 commands each = 4 (FillRect + DrawText, no borders)
        assert_eq!(cmds.len(), 4);
        // Second field has lower y in XFA, so higher pdf_y
        if let XfaPaintCommand::FillRect { y: y1, .. } = &cmds[0] {
            if let XfaPaintCommand::FillRect { y: y2, .. } = &cmds[2] {
                assert!(
                    y1 > y2,
                    "first field (y=10) should have higher PDF y than second (y=40)"
                );
            }
        }
    }

    #[test]
    fn checkbox_emits_checkbox_command() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![LayoutNode {
                form_node: xfa_layout_engine::form::FormNodeId(0),
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 15.0,
                    height: 15.0,
                },
                name: "check1".into(),
                content: LayoutContent::Field {
                    value: "1".into(),
                    field_kind: FieldKind::Checkbox,
                    font_size: 10.0,
                    font_family: FontFamily::SansSerif,
                },
                children: vec![],
                style: Default::default(),
                display_items: vec![],
                save_items: vec![],
            }],
            runtime_instantiated: false,
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            cmds[0],
            XfaPaintCommand::DrawCheckbox { checked: true, .. }
        ));
    }

    #[test]
    fn pdf_escape_winansi_encoding() {
        // ASCII passes through.
        assert_eq!(pdf_escape("Hello"), "Hello");
        // Parentheses and backslash are escaped.
        assert_eq!(pdf_escape("a(b)c\\d"), "a\\(b\\)c\\\\d");
        // En-dash U+2013 → WinAnsi 0x96 → octal \226.
        assert_eq!(pdf_escape("\u{2013}"), "\\226");
        // Bullet U+2022 → WinAnsi 0x95 → octal \225.
        assert_eq!(pdf_escape("\u{2022}"), "\\225");
        // Latin-1: © U+00A9 → WinAnsi 0xA9 → octal \251.
        assert_eq!(pdf_escape("\u{00A9}"), "\\251");
        // Unmapped character → '?'.
        assert_eq!(pdf_escape("\u{4E16}"), "?"); // CJK char
    }

    #[test]
    fn child_coordinates_accumulate() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![LayoutNode {
                form_node: xfa_layout_engine::form::FormNodeId(0),
                rect: Rect {
                    x: 50.0,
                    y: 100.0,
                    width: 200.0,
                    height: 200.0,
                },
                name: "parent".into(),
                content: LayoutContent::None,
                children: vec![LayoutNode {
                    form_node: xfa_layout_engine::form::FormNodeId(1),
                    rect: Rect {
                        x: 10.0,
                        y: 10.0,
                        width: 100.0,
                        height: 20.0,
                    },
                    name: "child".into(),
                    content: LayoutContent::Field {
                        value: "Test".into(),
                        field_kind: FieldKind::Text,
                        font_size: 10.0,
                        font_family: FontFamily::SansSerif,
                    },
                    children: vec![],
                    style: Default::default(),
                    display_items: vec![],
                    save_items: vec![],
                }],
                style: Default::default(),
                display_items: vec![],
                save_items: vec![],
            }],
            runtime_instantiated: false,
        };
        let config = XfaRenderConfig::default();
        let cmds = layout_to_commands(&page, &config);
        // Child should have x = 50 + 10 = 60 (accumulated)
        let text_cmd = cmds
            .iter()
            .find(|c| matches!(c, XfaPaintCommand::DrawText { .. }));
        assert!(text_cmd.is_some());
        if let Some(XfaPaintCommand::DrawText { x, .. }) = text_cmd {
            // x should be 60 (no default padding per XFA spec)
            assert!(
                (*x - 60.0).abs() < 0.1,
                "child x should be parent(50) + child(10) = 60, got {}",
                x
            );
        }
    }
}
