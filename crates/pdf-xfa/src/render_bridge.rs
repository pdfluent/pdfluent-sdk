//! XFA layout output to PDF content stream overlay generation.
//!
//! Converts LayoutDom (from xfa-layout-engine) into PDF content stream
//! operators that can be overlaid on existing PDF pages.
//!
//! Coordinate mapping: XFA uses top-left origin (y grows downward),
//! PDF uses bottom-left origin (y grows upward).

use crate::error::Result;
use xfa_layout_engine::form::{FieldKind, FormNodeStyle};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::TextAlign;

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

    // Apply text color — skip black (default).
    if let Some((r, g, b)) = style.text_color {
        cfg.text_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
    }

    cfg
}

/// Generate a PDF content stream overlay for a single page.
pub fn generate_page_overlay(page: &LayoutPage, config: &XfaRenderConfig) -> Result<Vec<u8>> {
    let mapper = CoordinateMapper::new(page.height, page.width);
    let mut ops = Vec::new();
    ops.extend_from_slice(b"q\n");
    render_nodes(&page.nodes, 0.0, 0.0, &mapper, config, &mut ops);
    ops.extend_from_slice(b"Q\n");
    Ok(ops)
}

/// Generate PDF content stream overlays for all pages in a layout.
pub fn generate_all_overlays(layout: &LayoutDom, config: &XfaRenderConfig) -> Result<Vec<Vec<u8>>> {
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
) {
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let w = node.rect.width;
        let h = node.rect.height;
        let pdf_y = mapper.xfa_to_pdf_y(abs_y, h);

        // Apply per-node style overrides from the XFA template.
        let node_config = apply_node_style(config, &node.style);

        // Draw background fill and borders for non-Field nodes (Draw, Subform, etc.)
        // Only when the XFA template explicitly defines bg/border styles.
        // Fields handle their own bg/borders in render_field.
        if !matches!(node.content, LayoutContent::Field { .. }) {
            // Background: only from explicit node style (set by apply_node_style).
            if let Some(bg) = &node_config.background_color {
                write_ops(
                    ops,
                    format_args!(
                        "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
                        bg[0], bg[1], bg[2], abs_x, pdf_y, w, h
                    ),
                );
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
                    write_ops(
                        ops,
                        format_args!(
                            "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
                            bw, bc[0], bc[1], bc[2], abs_x, pdf_y, w, h
                        ),
                    );
                }
            }
        }

        // Check if this node's font is bold (from XFA template style).
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
                &node_config,
                ops,
            ),
            LayoutContent::Image { data, mime_type } => {
                ops.extend(
                    format!(
                        "q\n{:.2} 0 0 {:.2} {:.2} {:.2} cm\n/Im0 Do\nQ\n",
                        w, h, abs_x, pdf_y
                    )
                    .bytes(),
                );
                // TODO: add image data to page resource dictionary as XObject
                // The caller must add: /XObject << /Im0 << /Type /XObject /Subtype /Image ... >> >>
                let _ = (data, mime_type);
            }
            LayoutContent::None => {}
        }

        if !node.children.is_empty() {
            // Pass the GLOBAL config to children, not node_config — background_color
            // and other style properties should not cascade from parent to children.
            // Each child applies its own style via apply_node_style.
            render_nodes(&node.children, abs_x, abs_y, mapper, config, ops);
        }
    }
}

fn render_field(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if let Some(bg) = &config.background_color {
        write_ops(
            ops,
            format_args!(
                "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
                bg[0], bg[1], bg[2], x, pdf_y, w, h
            ),
        );
    }
    if config.draw_borders && config.border_width > 0.0 {
        write_ops(
            ops,
            format_args!(
                "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
                config.border_width,
                config.border_color[0],
                config.border_color[1],
                config.border_color[2],
                x,
                pdf_y,
                w,
                h
            ),
        );
    }
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
        let font_ref = match font_family {
            FontFamily::Serif => "/F1",
            FontFamily::SansSerif => "/F2",
            FontFamily::Monospace => "/F3",
        };
        let text_w = metrics.measure_width(value);

        if text_w <= content_w || content_w <= 0.0 {
            // Single line — fits within field.
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                    x + p,
                    pdf_y + p,
                    pdf_escape(value)
                ),
            );
        } else {
            // Multi-line: word-wrap within field width.
            let lines = wrap_text(value, content_w, &metrics);
            let line_height = fs * 1.2;
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    font_ref,
                    fs,
                    x + p,
                    pdf_y + h - p - fs,
                ),
            );
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    write_ops(ops, format_args!("0 {:.2} Td\n", -line_height));
                }
                // Stop if we'd go below the field bottom.
                let line_top = h - p - fs - (i as f64 * line_height);
                if line_top < -p {
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
    // Draw checkbox border (square box)
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
    // If checked (non-empty value, not "0" or "off"), draw a checkmark
    let checked = !value.is_empty()
        && !value.eq_ignore_ascii_case("0")
        && !value.eq_ignore_ascii_case("off")
        && !value.eq_ignore_ascii_case("false");
    if checked {
        // Draw an X mark inside the box
        let m = w.min(h) * 0.15; // margin
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
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if lines.is_empty() {
        return;
    }
    let p = config.text_padding;
    let line_height = font_size * 1.2;
    // Select PDF font resource based on the template's font family.
    let font_ref = match font_family {
        FontFamily::Serif => "/F1",
        FontFamily::SansSerif => "/F2",
        FontFamily::Monospace => "/F3",
    };
    // Use per-character width measurement for alignment calculations.
    let font_metrics = xfa_layout_engine::text::FontMetrics {
        size: font_size,
        typeface: font_family,
        ..Default::default()
    };
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            config.text_color[0], config.text_color[1], config.text_color[2], font_ref, font_size
        ),
    );
    // Place the first-line baseline at `font_size` below the element's XFA top.
    // Do NOT add text_padding vertically: draw elements often have tight height
    // budgets (h ≈ font_size), and adding padding would push the baseline below
    // the element boundary, causing the clip-guard below to suppress all text.
    let first_line_pdf_y = mapper.xfa_to_pdf_y(abs_y_xfa + font_size, 0.0);
    let content_w = (container_width - p * 2.0).max(0.0);
    let mut prev_x = x + p;
    for (i, line) in lines.iter().enumerate() {
        let line_y = first_line_pdf_y - (i as f64 * line_height);
        let line_w = font_metrics.measure_width(line);
        let text_x = match text_align {
            TextAlign::Center => x + p + ((content_w - line_w) / 2.0).max(0.0),
            TextAlign::Right => x + p + (content_w - line_w).max(0.0),
            _ => x + p,
        };
        if i == 0 {
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", text_x, line_y));
        } else {
            // Td is relative to previous text position; compute delta from previous x.
            let dx = text_x - prev_x;
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", dx, -line_height));
        }
        prev_x = text_x;
        write_ops(ops, format_args!("({}) Tj\n", pdf_escape(line)));
    }
    ops.extend_from_slice(b"ET\n");
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
                field_kind: xfa_layout_engine::form::FieldKind::Text,
                font_size: 0.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
        }
    }

    #[test]
    fn coordinate_mapping() {
        let mapper = CoordinateMapper::new(792.0, 612.0);
        let pdf_y = mapper.xfa_to_pdf_y(0.0, 20.0);
        assert!((pdf_y - 772.0).abs() < 0.001);
    }

    #[test]
    fn empty_page_overlay() {
        let page = make_page(vec![]);
        let config = XfaRenderConfig::default();
        let overlay = generate_page_overlay(&page, &config).unwrap();
        let content = String::from_utf8_lossy(&overlay);
        assert!(content.starts_with("q\n"));
        assert!(content.ends_with("Q\n"));
    }

    #[test]
    fn field_renders_text() {
        let page = make_page(vec![make_field_node(10.0, 10.0, 100.0, 20.0, "Hello")]);
        let config = XfaRenderConfig::default();
        let overlay = generate_page_overlay(&page, &config).unwrap();
        let content = String::from_utf8_lossy(&overlay);
        assert!(content.contains("(Hello) Tj"));
        assert!(content.contains("BT"));
        assert!(content.contains("ET"));
    }

    #[test]
    fn empty_field_no_text() {
        let page = make_page(vec![make_field_node(10.0, 10.0, 100.0, 20.0, "")]);
        let config = XfaRenderConfig::default();
        let overlay = generate_page_overlay(&page, &config).unwrap();
        let content = String::from_utf8_lossy(&overlay);
        assert!(!content.contains("BT"));
    }

    #[test]
    fn all_overlays() {
        let layout = LayoutDom {
            pages: vec![
                make_page(vec![make_field_node(0.0, 0.0, 50.0, 20.0, "P1")]),
                make_page(vec![make_field_node(0.0, 0.0, 50.0, 20.0, "P2")]),
            ],
        };
        let config = XfaRenderConfig::default();
        let overlays = generate_all_overlays(&layout, &config).unwrap();
        assert_eq!(overlays.len(), 2);
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
}
