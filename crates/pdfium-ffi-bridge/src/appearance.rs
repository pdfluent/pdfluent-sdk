//! Appearance stream generation — convert LayoutNodes to PDF Form XObjects.
//!
//! For each field/draw element in the layout, generates a PDF appearance stream
//! containing the visual representation (text, borders, backgrounds).
//! These streams are embedded as Form XObjects and referenced from the page content.
//!
//! PDF operators used:
//! - Text: `BT`, `ET`, `Tf`, `Td`, `Tj`, `TJ`, `Tm`
//! - Graphics: `q`, `Q`, `re`, `f`, `S`, `w`, `RG`, `rg`, `cm`
//! - XObject: `Do`

use crate::error::Result;
use xfa_layout_engine::layout::{LayoutContent, LayoutNode};

/// Configuration for appearance stream generation.
#[derive(Debug, Clone)]
pub struct AppearanceConfig {
    /// Default font name for text rendering.
    pub default_font: String,
    /// Default font size in points.
    pub default_font_size: f64,
    /// Border width in points (0 = no border).
    pub border_width: f64,
    /// Border color (RGB, 0.0-1.0).
    pub border_color: [f64; 3],
    /// Background color (RGB, 0.0-1.0). None = transparent.
    pub background_color: Option<[f64; 3]>,
    /// Text color (RGB, 0.0-1.0).
    pub text_color: [f64; 3],
    /// Text padding from edges in points.
    pub text_padding: f64,
    /// Whether to compress streams with FlateDecode.
    pub compress: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            default_font: "Helvetica".to_string(),
            default_font_size: 10.0,
            border_width: 0.5,
            border_color: [0.0, 0.0, 0.0],
            background_color: Some([1.0, 1.0, 1.0]),
            text_color: [0.0, 0.0, 0.0],
            text_padding: 2.0,
            compress: true,
        }
    }
}

/// A generated appearance stream ready for embedding in a PDF.
#[derive(Debug, Clone)]
pub struct AppearanceStream {
    /// The raw content stream bytes (PDF operators).
    pub content: Vec<u8>,
    /// Bounding box [x, y, width, height] in points.
    pub bbox: [f64; 4],
    /// Font resource references used: (resource_name, font_name).
    pub font_resources: Vec<(String, String)>,
}

/// Generate an appearance stream for a text field.
pub fn field_appearance(
    value: &str,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();

    // Background fill
    if let Some(bg) = &config.background_color {
        write_color_fill(&mut ops, bg);
        write_rect(&mut ops, 0.0, 0.0, width, height);
        ops.extend_from_slice(b"f\n");
    }

    // Border
    if config.border_width > 0.0 {
        write_line_width(&mut ops, config.border_width);
        write_color_stroke(&mut ops, &config.border_color);
        write_rect(&mut ops, 0.0, 0.0, width, height);
        ops.extend_from_slice(b"S\n");
    }

    // Text
    if !value.is_empty() {
        let font_name = "F1";
        let font_size = config.default_font_size;
        let padding = config.text_padding;

        // Position text: baseline from bottom
        let text_x = padding;
        let text_y = height - font_size - padding;

        ops.extend_from_slice(b"BT\n");
        write_color_fill(&mut ops, &config.text_color);
        write_font(&mut ops, font_name, font_size);
        write_text_position(&mut ops, text_x, text_y);
        write_text_show(&mut ops, value);
        ops.extend_from_slice(b"ET\n");

        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![(font_name.to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}

/// Generate an appearance stream for a static text (draw) element.
pub fn draw_appearance(
    text: &str,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();

    // Background (usually transparent for draw)
    if let Some(bg) = &config.background_color {
        write_color_fill(&mut ops, bg);
        write_rect(&mut ops, 0.0, 0.0, width, height);
        ops.extend_from_slice(b"f\n");
    }

    // Text
    if !text.is_empty() {
        let font_name = "F1";
        let font_size = config.default_font_size;

        ops.extend_from_slice(b"BT\n");
        write_color_fill(&mut ops, &config.text_color);
        write_font(&mut ops, font_name, font_size);
        write_text_position(
            &mut ops,
            config.text_padding,
            height - font_size - config.text_padding,
        );
        write_text_show(&mut ops, text);
        ops.extend_from_slice(b"ET\n");

        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![(font_name.to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}

/// Generate an appearance stream for a multi-line text field.
pub fn multiline_appearance(
    lines: &[String],
    font_size: f64,
    line_height: f64,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();

    // Background
    if let Some(bg) = &config.background_color {
        write_color_fill(&mut ops, bg);
        write_rect(&mut ops, 0.0, 0.0, width, height);
        ops.extend_from_slice(b"f\n");
    }

    // Border
    if config.border_width > 0.0 {
        write_line_width(&mut ops, config.border_width);
        write_color_stroke(&mut ops, &config.border_color);
        write_rect(&mut ops, 0.0, 0.0, width, height);
        ops.extend_from_slice(b"S\n");
    }

    if !lines.is_empty() {
        let font_name = "F1";
        let padding = config.text_padding;

        ops.extend_from_slice(b"BT\n");
        write_color_fill(&mut ops, &config.text_color);
        write_font(&mut ops, font_name, font_size);

        let start_y = height - font_size - padding;
        for (i, line) in lines.iter().enumerate() {
            let abs_y = start_y - (i as f64 * line_height);
            if abs_y < 0.0 {
                break; // Clip at bottom
            }
            if i == 0 {
                // First Td: offset from BT origin (0,0) — effectively absolute
                write_text_position(&mut ops, padding, abs_y);
            } else {
                // Subsequent Td: relative to previous position
                // dx = 0 (same x), dy = -line_height (move down one line)
                write_text_position(&mut ops, 0.0, -line_height);
            }
            write_text_show(&mut ops, line);
        }

        ops.extend_from_slice(b"ET\n");

        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![(font_name.to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}

/// Generate an appearance stream for a checkbox.
pub fn checkbox_appearance(checked: bool, width: f64, height: f64) -> AppearanceStream {
    let mut ops = Vec::new();
    let size = width.min(height);

    // Border
    write_line_width(&mut ops, 0.5);
    write_color_stroke(&mut ops, &[0.0, 0.0, 0.0]);
    write_rect(&mut ops, 0.0, 0.0, size, size);
    ops.extend_from_slice(b"S\n");

    // Check mark (X pattern)
    if checked {
        write_line_width(&mut ops, 1.5);
        write_color_stroke(&mut ops, &[0.0, 0.0, 0.0]);

        // Diagonal 1
        let pad = size * 0.2;
        write_move_to(&mut ops, pad, pad);
        write_line_to(&mut ops, size - pad, size - pad);
        ops.extend_from_slice(b"S\n");

        // Diagonal 2
        write_move_to(&mut ops, size - pad, pad);
        write_line_to(&mut ops, pad, size - pad);
        ops.extend_from_slice(b"S\n");
    }

    AppearanceStream {
        content: ops,
        bbox: [0.0, 0.0, size, size],
        font_resources: vec![],
    }
}

/// Generate appearance streams for all nodes in a layout tree.
///
/// Returns a list of (node_name, absolute_x, absolute_y, appearance).
pub fn generate_appearances(
    nodes: &[LayoutNode],
    config: &AppearanceConfig,
) -> Result<Vec<(String, f64, f64, AppearanceStream)>> {
    let mut result = Vec::new();
    collect_appearances(nodes, 0.0, 0.0, config, &mut result);
    Ok(result)
}

fn collect_appearances(
    nodes: &[LayoutNode],
    parent_x: f64,
    parent_y: f64,
    config: &AppearanceConfig,
    result: &mut Vec<(String, f64, f64, AppearanceStream)>,
) {
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let width = node.rect.width;
        let height = node.rect.height;

        match &node.content {
            LayoutContent::Field { value } => {
                let appearance = field_appearance(value, width, height, config);
                result.push((node.name.clone(), abs_x, abs_y, appearance));
            }
            LayoutContent::Text(text) => {
                let appearance = draw_appearance(text, width, height, config);
                result.push((node.name.clone(), abs_x, abs_y, appearance));
            }
            LayoutContent::WrappedText { lines, font_size } => {
                let line_height = font_size * 1.2;
                let appearance =
                    multiline_appearance(lines, *font_size, line_height, width, height, config);
                result.push((node.name.clone(), abs_x, abs_y, appearance));
            }
            LayoutContent::None => {
                // Container — no appearance, but recurse into children
            }
        }

        // Recurse into children
        if !node.children.is_empty() {
            collect_appearances(&node.children, abs_x, abs_y, config, result);
        }
    }
}

// ── PDF Content Stream Helpers ──────────────────────────────────────

fn write_color_fill(ops: &mut Vec<u8>, rgb: &[f64; 3]) {
    ops.extend_from_slice(format!("{:.3} {:.3} {:.3} rg\n", rgb[0], rgb[1], rgb[2]).as_bytes());
}

fn write_color_stroke(ops: &mut Vec<u8>, rgb: &[f64; 3]) {
    ops.extend_from_slice(format!("{:.3} {:.3} {:.3} RG\n", rgb[0], rgb[1], rgb[2]).as_bytes());
}

fn write_rect(ops: &mut Vec<u8>, x: f64, y: f64, w: f64, h: f64) {
    ops.extend_from_slice(format!("{:.2} {:.2} {:.2} {:.2} re\n", x, y, w, h).as_bytes());
}

fn write_line_width(ops: &mut Vec<u8>, w: f64) {
    ops.extend_from_slice(format!("{:.2} w\n", w).as_bytes());
}

fn write_font(ops: &mut Vec<u8>, name: &str, size: f64) {
    ops.extend_from_slice(format!("/{name} {:.1} Tf\n", size).as_bytes());
}

fn write_text_position(ops: &mut Vec<u8>, x: f64, y: f64) {
    ops.extend_from_slice(format!("{:.2} {:.2} Td\n", x, y).as_bytes());
}

fn write_text_show(ops: &mut Vec<u8>, text: &str) {
    // Convert Unicode text to WinAnsiEncoding bytes. PDF string literals
    // with WinAnsiEncoding expect single-byte codes, not raw UTF-8.
    ops.push(b'(');
    for ch in text.chars() {
        match unicode_to_winansi(ch) {
            Some(b @ (b'(' | b')' | b'\\')) => {
                ops.push(b'\\');
                ops.push(b);
            }
            Some(b) => ops.push(b),
            None => ops.push(b'?'), // Unmappable character
        }
    }
    ops.extend_from_slice(b") Tj\n");
}

fn write_move_to(ops: &mut Vec<u8>, x: f64, y: f64) {
    ops.extend_from_slice(format!("{:.2} {:.2} m\n", x, y).as_bytes());
}

fn write_line_to(ops: &mut Vec<u8>, x: f64, y: f64) {
    ops.extend_from_slice(format!("{:.2} {:.2} l\n", x, y).as_bytes());
}

/// Escape a string for use in a PDF string literal `(...)`.
#[cfg(test)]
fn pdf_escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '\\' => result.push_str("\\\\"),
            _ => result.push(c),
        }
    }
    result
}

/// Map a Unicode character to its WinAnsiEncoding byte code.
fn unicode_to_winansi(ch: char) -> Option<u8> {
    let cp = ch as u32;
    if (32..=127).contains(&cp) || (160..=255).contains(&cp) {
        return Some(cp as u8);
    }
    match cp {
        0x20AC => Some(128), // Euro
        0x201A => Some(130), // quotesinglbase
        0x0192 => Some(131), // florin
        0x201E => Some(132), // quotedblbase
        0x2026 => Some(133), // ellipsis
        0x2020 => Some(134), // dagger
        0x2021 => Some(135), // daggerdbl
        0x02C6 => Some(136), // circumflex
        0x2030 => Some(137), // perthousand
        0x0160 => Some(138), // Scaron
        0x2039 => Some(139), // guilsinglleft
        0x0152 => Some(140), // OE
        0x017D => Some(142), // Zcaron
        0x2018 => Some(145), // quoteleft
        0x2019 => Some(146), // quoteright
        0x201C => Some(147), // quotedblleft
        0x201D => Some(148), // quotedblright
        0x2022 => Some(149), // bullet
        0x2013 => Some(150), // endash
        0x2014 => Some(151), // emdash
        0x02DC => Some(152), // tilde
        0x2122 => Some(153), // trademark
        0x0161 => Some(154), // scaron
        0x203A => Some(155), // guilsinglright
        0x0153 => Some(156), // oe
        0x017E => Some(158), // zcaron
        0x0178 => Some(159), // Ydieresis
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::FormNodeId;
    use xfa_layout_engine::types::Rect;

    #[test]
    fn field_appearance_basic() {
        let config = AppearanceConfig::default();
        let ap = field_appearance("Hello", 100.0, 20.0, &config);

        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("BT"), "Should have text begin");
        assert!(content.contains("ET"), "Should have text end");
        assert!(content.contains("(Hello) Tj"), "Should show text");
        assert!(content.contains("/F1"), "Should reference font");
        assert_eq!(ap.bbox, [0.0, 0.0, 100.0, 20.0]);
        assert_eq!(ap.font_resources.len(), 1);
    }

    #[test]
    fn field_appearance_empty_value() {
        let config = AppearanceConfig::default();
        let ap = field_appearance("", 100.0, 20.0, &config);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(
            !content.contains("BT"),
            "Should not have text for empty value"
        );
        assert!(ap.font_resources.is_empty());
    }

    #[test]
    fn draw_appearance_basic() {
        let config = AppearanceConfig::default();
        let ap = draw_appearance("Label:", 80.0, 15.0, &config);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("(Label:) Tj"));
    }

    #[test]
    fn multiline_appearance_basic() {
        let config = AppearanceConfig::default();
        let lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        let ap = multiline_appearance(&lines, 10.0, 12.0, 200.0, 50.0, &config);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("(Line 1) Tj"));
        assert!(content.contains("(Line 2) Tj"));
    }

    #[test]
    fn multiline_td_uses_relative_offsets() {
        let config = AppearanceConfig {
            text_padding: 2.0,
            ..Default::default()
        };
        let lines = vec![
            "First".to_string(),
            "Second".to_string(),
            "Third".to_string(),
        ];
        let ap = multiline_appearance(&lines, 10.0, 12.0, 200.0, 60.0, &config);
        let content = String::from_utf8_lossy(&ap.content);

        // First Td should be absolute from BT origin: padding, start_y
        let start_y = 60.0 - 10.0 - 2.0; // height - font_size - padding = 48.0
        let expected_first = format!("{:.2} {:.2} Td", 2.0, start_y);
        assert!(
            content.contains(&expected_first),
            "First Td should be absolute: {expected_first}\nGot: {content}"
        );

        // Subsequent Td should be relative: 0, -line_height
        let expected_rel = format!("{:.2} {:.2} Td", 0.0, -12.0);
        // Count occurrences — should appear exactly twice (for lines 2 and 3)
        let rel_count = content.matches(&expected_rel).count();
        assert_eq!(
            rel_count, 2,
            "Should have 2 relative Td ops, got {rel_count}\nContent: {content}"
        );
    }

    #[test]
    fn checkbox_unchecked() {
        let ap = checkbox_appearance(false, 12.0, 12.0);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("re"), "Should have rectangle border");
        assert!(
            !content.contains(" m\n"),
            "Should not have check mark lines"
        );
    }

    #[test]
    fn checkbox_checked() {
        let ap = checkbox_appearance(true, 12.0, 12.0);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("re"), "Should have border");
        assert!(content.contains(" m\n"), "Should have diagonal lines");
        assert!(content.contains(" l\n"), "Should have line-to");
    }

    #[test]
    fn pdf_escape() {
        assert_eq!(pdf_escape_string("Hello"), "Hello");
        assert_eq!(pdf_escape_string("(test)"), "\\(test\\)");
        assert_eq!(pdf_escape_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn generate_appearances_from_layout() {
        let config = AppearanceConfig::default();
        let nodes = vec![
            LayoutNode {
                form_node: FormNodeId(0),
                rect: Rect::new(10.0, 10.0, 100.0, 20.0),
                name: "Field1".to_string(),
                content: LayoutContent::Field {
                    value: "Hello".to_string(),
                },
                children: vec![],
            },
            LayoutNode {
                form_node: FormNodeId(1),
                rect: Rect::new(10.0, 40.0, 80.0, 15.0),
                name: "Label1".to_string(),
                content: LayoutContent::Text("Name:".to_string()),
                children: vec![],
            },
        ];

        let result = generate_appearances(&nodes, &config).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Field1");
        assert_eq!(result[0].1, 10.0); // abs_x
        assert_eq!(result[0].2, 10.0); // abs_y
        assert_eq!(result[1].0, "Label1");
    }

    #[test]
    fn generate_appearances_nested() {
        let config = AppearanceConfig::default();
        let nodes = vec![LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(50.0, 50.0, 200.0, 100.0),
            name: "Container".to_string(),
            content: LayoutContent::None,
            children: vec![LayoutNode {
                form_node: FormNodeId(1),
                rect: Rect::new(10.0, 10.0, 100.0, 20.0),
                name: "ChildField".to_string(),
                content: LayoutContent::Field {
                    value: "Nested".to_string(),
                },
                children: vec![],
            }],
        }];

        let result = generate_appearances(&nodes, &config).unwrap();
        assert_eq!(result.len(), 1); // Only the field, not the container
        assert_eq!(result[0].0, "ChildField");
        assert_eq!(result[0].1, 60.0); // 50 + 10
        assert_eq!(result[0].2, 60.0); // 50 + 10
    }

    #[test]
    fn appearance_stream_has_background() {
        let config = AppearanceConfig {
            background_color: Some([0.9, 0.9, 0.9]),
            ..Default::default()
        };
        let ap = field_appearance("Test", 100.0, 20.0, &config);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(
            content.contains("0.900 0.900 0.900 rg"),
            "Should set fill color"
        );
        assert!(content.contains("re\nf"), "Should fill rectangle");
    }
}
