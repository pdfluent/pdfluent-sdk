//! XFA layout output to PDF content stream overlay generation.
//!
//! Converts LayoutDom (from xfa-layout-engine) into PDF content stream
//! operators that can be overlaid on existing PDF pages.
//!
//! Coordinate mapping: XFA uses top-left origin (y grows downward),
//! PDF uses bottom-left origin (y grows upward).

use crate::error::Result;
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};

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
            text_padding: 2.0,
        }
    }
}

/// Maps XFA coordinates (top-left origin) to PDF coordinates (bottom-left origin).
pub struct CoordinateMapper {
    page_height: f64,
}

impl CoordinateMapper {
    pub fn new(page_height: f64) -> Self {
        Self { page_height }
    }

    /// Convert XFA y-coordinate to PDF y-coordinate.
    pub fn xfa_to_pdf_y(&self, xfa_y: f64, element_height: f64) -> f64 {
        self.page_height - xfa_y - element_height
    }
}

/// Generate a PDF content stream overlay for a single page.
pub fn generate_page_overlay(page: &LayoutPage, config: &XfaRenderConfig) -> Result<Vec<u8>> {
    let mapper = CoordinateMapper::new(page.height);
    let mut ops = Vec::new();
    ops.extend_from_slice(b"q\n");
    render_nodes(&page.nodes, 0.0, 0.0, &mapper, config, &mut ops);
    ops.extend_from_slice(b"Q\n");
    Ok(ops)
}

/// Generate PDF content stream overlays for all pages in a layout.
pub fn generate_all_overlays(layout: &LayoutDom, config: &XfaRenderConfig) -> Result<Vec<Vec<u8>>> {
    layout.pages.iter().map(|page| generate_page_overlay(page, config)).collect()
}

fn render_nodes(nodes: &[LayoutNode], parent_x: f64, parent_y: f64, mapper: &CoordinateMapper, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let w = node.rect.width;
        let h = node.rect.height;
        let pdf_y = mapper.xfa_to_pdf_y(abs_y, h);

        match &node.content {
            LayoutContent::Field { value } => render_field(abs_x, pdf_y, w, h, value, config, ops),
            LayoutContent::Text(text) => render_text(abs_x, pdf_y, text, config, ops),
            LayoutContent::WrappedText { lines, font_size } => render_multiline(abs_x, pdf_y, lines, *font_size, mapper, abs_y, config, ops),
            LayoutContent::None => {}
        }

        if !node.children.is_empty() {
            render_nodes(&node.children, abs_x, abs_y, mapper, config, ops);
        }
    }
}

fn render_field(x: f64, pdf_y: f64, w: f64, h: f64, value: &str, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    if let Some(bg) = &config.background_color {
        write_ops(ops, format_args!("{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], x, pdf_y, w, h));
    }
    if config.draw_borders && config.border_width > 0.0 {
        write_ops(ops, format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            config.border_width, config.border_color[0], config.border_color[1], config.border_color[2], x, pdf_y, w, h));
    }
    if !value.is_empty() {
        let fs = config.default_font_size;
        let p = config.text_padding;
        write_ops(ops, format_args!("BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0], config.text_color[1], config.text_color[2], fs, x + p, pdf_y + p, pdf_escape(value)));
    }
}

fn render_text(x: f64, pdf_y: f64, text: &str, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    if text.is_empty() { return; }
    let fs = config.default_font_size;
    let p = config.text_padding;
    write_ops(ops, format_args!("BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
        config.text_color[0], config.text_color[1], config.text_color[2], fs, x + p, pdf_y + p, pdf_escape(text)));
}

fn render_multiline(x: f64, pdf_y: f64, lines: &[String], font_size: f64, mapper: &CoordinateMapper, abs_y_xfa: f64, config: &XfaRenderConfig, ops: &mut Vec<u8>) {
    if lines.is_empty() { return; }
    let p = config.text_padding;
    let line_height = font_size * 1.2;
    write_ops(ops, format_args!("BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n",
        config.text_color[0], config.text_color[1], config.text_color[2], font_size));
    let first_line_pdf_y = mapper.xfa_to_pdf_y(abs_y_xfa + p + font_size, 0.0);
    let text_x = x + p;
    for (i, line) in lines.iter().enumerate() {
        let line_y = first_line_pdf_y - (i as f64 * line_height);
        if line_y < pdf_y { break; }
        if i == 0 {
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", text_x, line_y));
        } else {
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", 0.0, -line_height));
        }
        write_ops(ops, format_args!("({}) Tj\n", pdf_escape(line)));
    }
    ops.extend_from_slice(b"ET\n");
}

fn pdf_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c { '(' => r.push_str("\\("), ')' => r.push_str("\\)"), '\\' => r.push_str("\\\\"), _ => r.push(c) }
    }
    r
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
        LayoutPage { width: 612.0, height: 792.0, nodes }
    }

    fn make_field_node(x: f64, y: f64, w: f64, h: f64, value: &str) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, w, h),
            name: "field1".to_string(),
            content: LayoutContent::Field { value: value.to_string() },
            children: vec![],
        }
    }

    #[test]
    fn coordinate_mapping() {
        let mapper = CoordinateMapper::new(792.0);
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
}
