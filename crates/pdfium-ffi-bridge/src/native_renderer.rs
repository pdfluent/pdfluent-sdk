//! Native rendering pipeline — Layout DOM to pixel output (pure Rust).
//!
//! Renders XFA layout output as rasterized images without any C/C++ dependencies.
//! Draws rectangles, borders, and text placeholders for form elements.

use image::{DynamicImage, Rgba, RgbaImage};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};

/// Rendering configuration.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Scale factor: points to pixels (1.0 = 72 DPI, 2.0 = 144 DPI).
    pub scale: f64,
    /// Background color for pages.
    pub background: Rgba<u8>,
    /// Default border color for containers.
    pub border_color: Rgba<u8>,
    /// Field border color.
    pub field_color: Rgba<u8>,
    /// Draw/text element color.
    pub text_color: Rgba<u8>,
    /// Whether to draw text content as simple character blocks.
    pub render_text: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            scale: 1.0,
            background: Rgba([255, 255, 255, 255]),
            border_color: Rgba([180, 180, 180, 255]),
            field_color: Rgba([0, 0, 180, 255]),
            text_color: Rgba([0, 0, 0, 255]),
            render_text: true,
        }
    }
}

impl RenderConfig {
    /// Create a config for a specific DPI.
    pub fn with_dpi(dpi: f64) -> Self {
        Self {
            scale: dpi / 72.0,
            ..Default::default()
        }
    }
}

/// Render a full layout DOM to page images.
pub fn render_layout(layout: &LayoutDom, config: &RenderConfig) -> Vec<DynamicImage> {
    layout
        .pages
        .iter()
        .map(|page| render_page(page, config))
        .collect()
}

/// Render a single layout page to an image.
fn render_page(page: &LayoutPage, config: &RenderConfig) -> DynamicImage {
    let w = (page.width * config.scale) as u32;
    let h = (page.height * config.scale) as u32;
    let mut img = RgbaImage::from_pixel(w, h, config.background);

    for node in &page.nodes {
        render_node(&mut img, node, config, 0, 0.0, 0.0);
    }

    DynamicImage::ImageRgba8(img)
}

/// Render a layout node and its children recursively.
///
/// `parent_x` and `parent_y` are the accumulated offsets from parent containers,
/// since child node coordinates are relative to their parent.
fn render_node(
    img: &mut RgbaImage,
    node: &LayoutNode,
    config: &RenderConfig,
    depth: usize,
    parent_x: f64,
    parent_y: f64,
) {
    let abs_x = node.rect.x + parent_x;
    let abs_y = node.rect.y + parent_y;
    let x = (abs_x * config.scale) as i32;
    let y = (abs_y * config.scale) as i32;
    let w = (node.rect.width * config.scale) as i32;
    let h = (node.rect.height * config.scale) as i32;

    if w <= 0 || h <= 0 {
        return;
    }

    match &node.content {
        LayoutContent::WrappedText { lines, font_size } => {
            // Draw field/text background
            let fill = lighten(config.field_color, 200);
            draw_filled_rect(img, x, y, w, h, fill);
            draw_rect(img, x, y, w, h, config.field_color);

            // Draw text lines
            if config.render_text {
                let line_h = (*font_size * 1.2 * config.scale) as i32;
                let char_w = (*font_size * 0.5 * config.scale) as i32;
                for (i, line) in lines.iter().enumerate() {
                    let ly = y + 2 + i as i32 * line_h;
                    draw_text_line(img, x + 2, ly, line, char_w, line_h, config.text_color);
                }
            }
        }
        LayoutContent::Field { .. } => {
            let fill = lighten(config.field_color, 200);
            draw_filled_rect(img, x, y, w, h, fill);
            draw_rect(img, x, y, w, h, config.field_color);
        }
        LayoutContent::Text(_) => {
            let fill = lighten(Rgba([0, 150, 0, 255]), 200);
            draw_filled_rect(img, x, y, w, h, fill);
            draw_rect(img, x, y, w, h, Rgba([0, 150, 0, 255]));
        }
        LayoutContent::None => {
            // Container: subtle background at deeper levels
            if depth > 0 {
                let gray = (240u8).saturating_sub((depth as u8).min(5) * 8);
                let fill = Rgba([gray, gray, gray, 255]);
                draw_filled_rect(img, x, y, w, h, fill);
            }
            let border_gray = (200u8).saturating_sub((depth as u8).min(5) * 15);
            draw_rect(
                img,
                x,
                y,
                w,
                h,
                Rgba([border_gray, border_gray, border_gray, 255]),
            );
        }
    }

    // Render children with accumulated parent offset.
    for child in &node.children {
        render_node(img, child, config, depth + 1, abs_x, abs_y);
    }
}

/// Draw a simple text line as character blocks.
fn draw_text_line(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    text: &str,
    char_w: i32,
    line_h: i32,
    color: Rgba<u8>,
) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let text_h = (line_h as f64 * 0.7) as i32; // Character height ~70% of line height

    for (ci, ch) in text.chars().enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        let cx = x + ci as i32 * char_w;
        if cx + char_w > iw || cx < 0 {
            continue;
        }

        // Draw a small filled rectangle for each character
        let cw = (char_w as f64 * 0.7) as i32; // Character width
        for py in y.max(0)..(y + text_h).min(ih) {
            for px in cx.max(0)..(cx + cw).min(iw) {
                img.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

/// Lighten a color by adding an offset to each channel.
fn lighten(color: Rgba<u8>, amount: u8) -> Rgba<u8> {
    Rgba([
        color[0].saturating_add(amount).min(250),
        color[1].saturating_add(amount).min(250),
        color[2].saturating_add(amount).min(250),
        255,
    ])
}

/// Draw a filled rectangle on the image.
fn draw_filled_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for py in y.max(0)..(y + h).min(ih) {
        for px in x.max(0)..(x + w).min(iw) {
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

/// Draw a rectangle border on the image.
fn draw_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    let (iw, ih) = (img.width() as i32, img.height() as i32);

    for px in x.max(0)..(x + w).min(iw) {
        if y >= 0 && y < ih {
            img.put_pixel(px as u32, y as u32, color);
        }
        let by = y + h - 1;
        if by >= 0 && by < ih {
            img.put_pixel(px as u32, by as u32, color);
        }
    }

    for py in y.max(0)..(y + h).min(ih) {
        if x >= 0 && x < iw {
            img.put_pixel(x as u32, py as u32, color);
        }
        let rx = x + w - 1;
        if rx >= 0 && rx < iw {
            img.put_pixel(rx as u32, py as u32, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::FormNodeId;
    use xfa_layout_engine::types::Rect;

    #[test]
    fn render_empty_page() {
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 100.0,
                height: 100.0,
                nodes: vec![],
            }],
        };
        let config = RenderConfig::default();
        let images = render_layout(&layout, &config);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width(), 100);
        assert_eq!(images[0].height(), 100);
    }

    #[test]
    fn render_with_dpi_scaling() {
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 100.0,
                height: 100.0,
                nodes: vec![],
            }],
        };
        let config = RenderConfig::with_dpi(144.0); // 2x scale
        let images = render_layout(&layout, &config);
        assert_eq!(images[0].width(), 200);
        assert_eq!(images[0].height(), 200);
    }

    #[test]
    fn render_field_node() {
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 200.0,
                height: 100.0,
                nodes: vec![LayoutNode {
                    form_node: FormNodeId(0),
                    rect: Rect::new(10.0, 10.0, 100.0, 25.0),
                    name: "Name".to_string(),
                    content: LayoutContent::WrappedText {
                        lines: vec!["Hello".to_string()],
                        font_size: 10.0,
                    },
                    children: vec![],
                }],
            }],
        };
        let config = RenderConfig::default();
        let images = render_layout(&layout, &config);

        let img = images[0].as_rgba8().unwrap();
        // Border pixel should not be white
        let pixel = img.get_pixel(10, 10);
        assert_ne!(*pixel, Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn render_text_characters() {
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 200.0,
                height: 50.0,
                nodes: vec![LayoutNode {
                    form_node: FormNodeId(0),
                    rect: Rect::new(5.0, 5.0, 150.0, 20.0),
                    name: "Text".to_string(),
                    content: LayoutContent::WrappedText {
                        lines: vec!["AB".to_string()],
                        font_size: 10.0,
                    },
                    children: vec![],
                }],
            }],
        };
        let config = RenderConfig::default();
        let images = render_layout(&layout, &config);
        assert_eq!(images.len(), 1);
        // Should have drawn text characters (dark pixels inside the field area)
        let img = images[0].as_rgba8().unwrap();
        // Check a pixel where the first character should be drawn
        let pixel = img.get_pixel(9, 9);
        assert_ne!(
            *pixel,
            Rgba([255, 255, 255, 255]),
            "Character area should not be white"
        );
    }
}
