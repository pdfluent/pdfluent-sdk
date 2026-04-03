//! Abstract paint commands for XFA layout rendering.
//!
//! Converts XFA layout output into renderer-agnostic paint commands.
//! These commands can be consumed by any backend: Device trait, content stream,
//! SVG, etc.

use pdf_interpret::color::Color;
use xfa_layout_engine::layout::{LayoutContent, LayoutNode, LayoutPage};

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
        /// Font name.
        font_name: String,
        /// Font size in points.
        font_size: f64,
        /// Text color.
        color: Color,
    },
    /// Draw multiple lines of text.
    DrawMultilineText {
        /// X coordinate in PDF points.
        x: f64,
        /// Y coordinate in PDF points (bottom-left origin).
        y: f64,
        /// Text lines.
        lines: Vec<String>,
        /// Font name.
        font_name: String,
        /// Font size in points.
        font_size: f64,
        /// Line height in points.
        line_height: f64,
        /// Text color.
        color: Color,
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
}

// TODO: connect XFA <draw>/<image> parser to DrawImage (#666)
// Currently FormNodeType::Draw only has text content. When XFA image
// parsing is added, connect LayoutContent::Image variant to emit
// XfaPaintCommand::DrawImage here.

/// Convert an XFA layout page into abstract paint commands.
pub fn layout_to_commands(page: &LayoutPage, config: &XfaRenderConfig) -> Vec<XfaPaintCommand> {
    let mut commands = Vec::new();
    let page_height = page.height;
    for node in &page.nodes {
        emit_node_commands(node, page_height, config, &mut commands);
    }
    commands
}

fn emit_node_commands(
    node: &LayoutNode,
    page_height: f64,
    config: &XfaRenderConfig,
    commands: &mut Vec<XfaPaintCommand>,
) {
    let x = node.rect.x;
    let w = node.rect.width;
    let h = node.rect.height;
    // Convert from top-left (XFA) to bottom-left (PDF) origin
    let pdf_y = page_height - node.rect.y - h;

    let bg_color = config
        .background_color
        .map(|c| Color::from_device_rgb(c[0] as f32, c[1] as f32, c[2] as f32));
    let border_color = Color::from_device_rgb(
        config.border_color[0] as f32,
        config.border_color[1] as f32,
        config.border_color[2] as f32,
    );
    let text_color = Color::from_device_rgb(
        config.text_color[0] as f32,
        config.text_color[1] as f32,
        config.text_color[2] as f32,
    );

    match &node.content {
        LayoutContent::Field { value, .. } => {
            if let Some(bg) = bg_color {
                commands.push(XfaPaintCommand::FillRect {
                    x,
                    y: pdf_y,
                    w,
                    h,
                    color: bg,
                });
            }
            if config.draw_borders {
                commands.push(XfaPaintCommand::StrokeRect {
                    x,
                    y: pdf_y,
                    w,
                    h,
                    color: border_color.clone(),
                    width: config.border_width,
                });
            }
            if !value.is_empty() {
                commands.push(XfaPaintCommand::DrawText {
                    x: x + config.text_padding,
                    y: pdf_y + config.text_padding,
                    text: value.clone(),
                    font_name: config.default_font.clone(),
                    font_size: config.default_font_size,
                    color: text_color,
                });
            }
        }
        LayoutContent::Text(text) => {
            if !text.is_empty() {
                commands.push(XfaPaintCommand::DrawText {
                    x: x + config.text_padding,
                    y: pdf_y + config.text_padding,
                    text: text.clone(),
                    font_name: config.default_font.clone(),
                    font_size: config.default_font_size,
                    color: text_color,
                });
            }
        }
        LayoutContent::WrappedText {
            lines, font_size, ..
        } => {
            let fs = *font_size;
            let line_height = fs * 1.2;
            if !lines.is_empty() {
                commands.push(XfaPaintCommand::DrawMultilineText {
                    x: x + config.text_padding,
                    y: pdf_y + h - config.text_padding - fs,
                    lines: lines.clone(),
                    font_name: config.default_font.clone(),
                    font_size: fs,
                    line_height,
                    color: text_color,
                });
            }
        }
        LayoutContent::Image { .. } => {}
        LayoutContent::None => {}
    }

    for child in &node.children {
        emit_node_commands(child, page_height, config, commands);
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
                        "{:.3} {:.3} {:.3} rg\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0
                    )
                    .bytes(),
                );
                ops.extend(format!("{:.2} {:.2} {:.2} {:.2} re\nf\n", x, y, w, h).bytes());
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
                        "{:.3} {:.3} {:.3} RG\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0
                    )
                    .bytes(),
                );
                ops.extend(format!("{:.2} {:.2} {:.2} {:.2} re\nS\n", x, y, w, h).bytes());
            }
            XfaPaintCommand::DrawText {
                x,
                y,
                text,
                font_name,
                font_size,
                color,
            } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                let font_ref = match font_name.as_str() {
                    "Helvetica" | "sans-serif" => "/F1",
                    "Times-Roman" | "serif" => "/F2",
                    "Courier" | "monospace" => "/F3",
                    _ => "/F1",
                };
                ops.extend(
                    format!(
                        "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n{:.2} {:.2} Td\n",
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        font_ref,
                        font_size,
                        x,
                        y
                    )
                    .bytes(),
                );
                ops.extend(format!("({}) Tj\nET\n", pdf_escape(text)).bytes());
            }
            XfaPaintCommand::DrawMultilineText {
                x,
                y,
                lines,
                font_name,
                font_size,
                line_height,
                color,
            } => {
                let rgba = color.to_rgba();
                let [r, g, b, _] = rgba.to_rgba8();
                let font_ref = match font_name.as_str() {
                    "Helvetica" | "sans-serif" => "/F1",
                    "Times-Roman" | "serif" => "/F2",
                    "Courier" | "monospace" => "/F3",
                    _ => "/F1",
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
                for (i, line) in lines.iter().enumerate() {
                    let ly = y - (i as f64 * line_height);
                    ops.extend(format!("{:.2} {:.2} Td\n", x, ly).bytes());
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
        }
    }

    ops.extend_from_slice(b"Q\n");
    ops
}

fn pdf_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
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
        }
    }

    #[test]
    fn field_with_value_emits_fill_stroke_text() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "Hello")],
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 3); // FillRect + StrokeRect + DrawText
        assert!(matches!(cmds[0], XfaPaintCommand::FillRect { .. }));
        assert!(matches!(cmds[1], XfaPaintCommand::StrokeRect { .. }));
        assert!(matches!(cmds[2], XfaPaintCommand::DrawText { .. }));
    }

    #[test]
    fn empty_field_no_text_command() {
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "")],
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 2); // FillRect + StrokeRect, no DrawText
    }

    #[test]
    fn transparent_background_no_fill() {
        let mut config = test_config();
        config.background_color = None;
        let page = LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![field_node("name", 10.0, 10.0, 200.0, 20.0, "Hi")],
        };
        let cmds = layout_to_commands(&page, &config);
        assert_eq!(cmds.len(), 2); // StrokeRect + DrawText, no FillRect
        assert!(matches!(cmds[0], XfaPaintCommand::StrokeRect { .. }));
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
                    font_size: 10.0,
                    text_align: xfa_layout_engine::types::TextAlign::Left,
                    font_family: xfa_layout_engine::text::FontFamily::SansSerif,
                },
                children: vec![],
                style: Default::default(),
            }],
        };
        let cmds = layout_to_commands(&page, &test_config());
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], XfaPaintCommand::DrawMultilineText { .. }));
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
        };
        let cmds = layout_to_commands(&page, &test_config());
        // 2 fields × 3 commands each = 6
        assert_eq!(cmds.len(), 6);
        // Second field has lower y in XFA, so higher pdf_y
        if let XfaPaintCommand::FillRect { y: y1, .. } = &cmds[0] {
            if let XfaPaintCommand::FillRect { y: y2, .. } = &cmds[3] {
                assert!(
                    y1 > y2,
                    "first field (y=10) should have higher PDF y than second (y=40)"
                );
            }
        }
    }
}
