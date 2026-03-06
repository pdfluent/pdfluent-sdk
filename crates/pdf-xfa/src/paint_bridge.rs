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
}

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
        LayoutContent::Field { value } => {
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
        LayoutContent::WrappedText { lines, font_size } => {
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
        LayoutContent::None => {}
    }

    for child in &node.children {
        emit_node_commands(child, page_height, config, commands);
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
            },
            children: vec![],
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
                },
                children: vec![],
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
