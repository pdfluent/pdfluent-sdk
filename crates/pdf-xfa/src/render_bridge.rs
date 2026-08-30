//! XFA layout output to PDF content stream overlay generation.
//!
//! Converts LayoutDom (from xfa-layout-engine) into PDF content stream
//! operators that can be overlaid on existing PDF pages.
//!
//! XFA Spec 3.3 §2.6 (p55-56) — Transformations: XFA uses top-left origin
//! (y grows downward), PDF uses bottom-left origin (y grows upward).
//! The `CoordinateMapper` handles this transformation.
//!
//! XFA Spec 3.3 §2.7 — Z-Order: objects are rendered in document order.
//! Later objects appear on top of earlier objects (painter's algorithm).

use crate::error::Result;
use crate::font_bridge::font_variant_key;
use std::collections::HashMap;
use std::sync::Arc;
use xfa_layout_engine::form::{DrawContent, FieldKind, FormNodeStyle, RichTextSpan};
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
    ///
    /// Wrapped in [`Arc`] so the immutable font-resource map can be shared
    /// across thousands of `XfaRenderConfig::clone()` calls (PERF2-02:
    /// `apply_node_style` clones the config per layout node). The field
    /// dereferences to `HashMap<String, String>` transparently, so existing
    /// `&config.font_map.get(...)` call sites continue to work unchanged.
    pub font_map: Arc<HashMap<String, String>>,
    /// Resolved font metrics per typeface.
    ///
    /// Same `Arc`-sharing rationale as [`Self::font_map`]: avoids deep-cloning
    /// the per-typeface widths array on every node-level config clone.
    pub font_metrics_data: Arc<HashMap<String, FontMetricsData>>,
    /// CheckButton mark style (check, circle, cross, diamond, square, star).
    pub check_button_mark: Option<String>,
    /// When true, only render field value text — skip backgrounds, borders,
    /// captions, draws, and wrapped text.  Used by the preserve_static hybrid
    /// path to add XFA field values on top of pre-rendered page content without
    /// duplicating the visual structure.
    pub field_values_only: bool,
}

/// Resolved font metrics for a typeface, used for accurate text measurement.
#[derive(Debug, Clone)]
pub struct FontMetricsData {
    /// Unicode-indexed widths derived from PDF `/Widths` or font data.
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
    /// Optional Unicode->code map for simple PDF fonts with custom encodings.
    ///
    /// When Identity-H glyph encoding is unavailable (`font_data == None`),
    /// this map lets us emit bytes in the source font's actual encoding space
    /// instead of assuming WinAnsi for every simple font.
    pub simple_unicode_to_code: Option<HashMap<u16, u8>>,
}

/// Image data collected during rendering for XObject embedding.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// name.
    pub name: String,
    /// data.
    pub data: Vec<u8>,
    /// mime_type.
    pub mime_type: String,
}

/// Overlay result for a single page, including content stream and images.
#[derive(Debug, Clone)]
pub struct PageOverlay {
    /// content_stream.
    pub content_stream: Vec<u8>,
    /// images.
    pub images: Vec<ImageInfo>,
}

impl Default for XfaRenderConfig {
    fn default() -> Self {
        Self {
            default_font: "Helvetica".to_string(),
            default_font_size: 10.0,
            draw_borders: true,
            border_width: 1.0, // fix(#808): XFA default border thickness is 1pt, not 0.5pt.
            // BoxModel::border_width also defaults to 1.0pt (types.rs:368), so rendering
            // must match this to avoid 1px visual difference at 150 DPI (0.5pt vs 1pt ≈ 1px).
            border_color: [0.0, 0.0, 0.0],
            text_color: [0.0, 0.0, 0.0],
            background_color: None,
            text_padding: xfa_layout_engine::types::DEFAULT_TEXT_PADDING,
            font_map: Arc::new(HashMap::new()),
            font_metrics_data: Arc::new(HashMap::new()),
            check_button_mark: None,
            field_values_only: false,
        }
    }
}

/// Maps XFA coordinates (top-left origin) to PDF coordinates (bottom-left origin).
pub struct CoordinateMapper {
    page_height: f64,
    page_width: f64,
}

impl CoordinateMapper {
    /// new.
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
        cfg.background_color = Some([r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]);
    }

    cfg.draw_borders = false;
    // Some XFA templates only expose a usable border width via per-edge data
    // (for example when the first edge is hidden and later edges remain
    // visible). Treat those widths as sufficient to enable border rendering;
    // otherwise visible right/left/bottom borders disappear because
    // border_width_pt stays unset.
    if let Some(bw) = effective_border_width(style) {
        cfg.border_width = bw;
        cfg.draw_borders = true;
        if let Some((r, g, b)) = style.border_color {
            cfg.border_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
        }
    }

    if let Some((r, g, b)) = style.text_color {
        cfg.text_color = [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0];
    }

    if let Some(mark) = &style.check_button_mark {
        cfg.check_button_mark = Some(mark.clone());
    }

    cfg
}

fn effective_border_width(style: &FormNodeStyle) -> Option<f64> {
    if let Some(bw) = style.border_width_pt.filter(|bw| *bw > 0.0) {
        return Some(bw);
    }

    style
        .border_widths
        .as_ref()
        .map(|widths| {
            widths
                .iter()
                .zip(style.border_edges.iter())
                .filter_map(|(width, visible)| (*visible && *width > 0.0).then_some(*width))
                .fold(0.0, f64::max)
        })
        .filter(|bw| *bw > 0.0)
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

// ──────────────────────────────────────────────────────────────────────────────
// RenderTree — inspectable intermediate representation (XFA-F5-01 / #1104)
// ──────────────────────────────────────────────────────────────────────────────

/// A node in the intermediate render representation.
///
/// `RenderTree` is cheap to build and intended for debug/inspection only.
/// It mirrors the structure of the layout DOM but strips out raw PDF operator
/// details in favour of human-readable fields.
#[derive(Debug, Clone)]
pub enum RenderNode {
    /// A page container with known dimensions.
    Page {
        /// item.
        width: f64,
        /// item.
        height: f64,
        /// item.
        children: Vec<RenderNode>,
    },
    /// A single line of text positioned in PDF coordinate space.
    Text {
        /// item.
        x: f64,
        /// item.
        y: f64,
        /// item.
        content: String,
        /// item.
        font: String,
        /// item.
        size: f64,
    },
    /// A filled/stroked rectangle.
    Rect {
        /// item.
        x: f64,
        /// item.
        y: f64,
        /// item.
        width: f64,
        /// item.
        height: f64,
        /// item.
        fill: Option<[u8; 3]>,
        /// item.
        stroke: Option<[u8; 3]>,
    },
    /// An embedded image placeholder (actual bytes excluded for brevity).
    Image {
        /// item.
        x: f64,
        /// item.
        y: f64,
        /// item.
        width: f64,
        /// item.
        height: f64,
        /// item.
        data_len: usize,
    },
    /// An interactive form widget.
    Widget {
        /// item.
        x: f64,
        /// item.
        y: f64,
        /// item.
        width: f64,
        /// item.
        height: f64,
        /// item.
        field_name: String,
        /// item.
        value: String,
    },
    /// A group of child nodes (e.g. a subform container).
    Group {
        /// Child render nodes.
        children: Vec<RenderNode>,
    },
}

impl RenderNode {
    /// Produce a human-readable, indented tree representation.
    fn fmt_indented(&self, buf: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            RenderNode::Page {
                width,
                height,
                children,
            } => {
                buf.push_str(&format!("{indent}Page({width:.1}x{height:.1})\n"));
                for c in children {
                    c.fmt_indented(buf, depth + 1);
                }
            }
            RenderNode::Text {
                x,
                y,
                content,
                font,
                size,
            } => {
                let preview: String = content.chars().take(40).collect();
                let ellipsis = if content.len() > 40 { "…" } else { "" };
                buf.push_str(&format!(
                    "{indent}Text({x:.1},{y:.1}) font={font} size={size:.1} \"{preview}{ellipsis}\"\n"
                ));
            }
            RenderNode::Rect {
                x,
                y,
                width,
                height,
                fill,
                stroke,
            } => {
                let fill_str = fill
                    .map(|[r, g, b]| format!("fill=#{r:02X}{g:02X}{b:02X}"))
                    .unwrap_or_default();
                let stroke_str = stroke
                    .map(|[r, g, b]| format!("stroke=#{r:02X}{g:02X}{b:02X}"))
                    .unwrap_or_default();
                buf.push_str(&format!(
                    "{indent}Rect({x:.1},{y:.1} {width:.1}x{height:.1}) {fill_str} {stroke_str}\n"
                ));
            }
            RenderNode::Image {
                x,
                y,
                width,
                height,
                data_len,
            } => {
                buf.push_str(&format!(
                    "{indent}Image({x:.1},{y:.1} {width:.1}x{height:.1}) bytes={data_len}\n"
                ));
            }
            RenderNode::Widget {
                x,
                y,
                width,
                height,
                field_name,
                value,
            } => {
                let preview: String = value.chars().take(30).collect();
                buf.push_str(&format!(
                    "{indent}Widget({x:.1},{y:.1} {width:.1}x{height:.1}) name={field_name} value=\"{preview}\"\n"
                ));
            }
            RenderNode::Group { children } => {
                buf.push_str(&format!("{indent}Group\n"));
                for c in children {
                    c.fmt_indented(buf, depth + 1);
                }
            }
        }
    }
}

/// The full intermediate render tree for a document.
///
/// Each element of `pages` is a `RenderNode::Page` containing the positioned
/// content nodes for that page.
#[derive(Debug, Clone)]
pub struct RenderTree {
    /// pages.
    pub pages: Vec<RenderNode>,
}

impl RenderTree {
    /// Produce a human-readable, indented tree string for debug output.
    pub fn to_debug_string(&self) -> String {
        let mut buf = String::new();
        for page in &self.pages {
            page.fmt_indented(&mut buf, 0);
        }
        buf
    }
}

/// Build a `RenderTree` from a `LayoutDom`.
///
/// This is a lightweight, non-mutating walk: no PDF operators are emitted.
/// Intended for inspection and debug tooling only; not called on the hot path.
pub fn layout_dom_to_render_tree(layout: &LayoutDom, config: &XfaRenderConfig) -> RenderTree {
    let pages = layout
        .pages
        .iter()
        .map(|page| {
            let mapper = CoordinateMapper::new(page.height, page.width);
            let children = render_tree_nodes(&page.nodes, 0.0, 0.0, &mapper, config);
            RenderNode::Page {
                width: page.width,
                height: page.height,
                children,
            }
        })
        .collect();
    RenderTree { pages }
}

fn render_tree_nodes(
    nodes: &[LayoutNode],
    parent_x: f64,
    parent_y: f64,
    mapper: &CoordinateMapper,
    config: &XfaRenderConfig,
) -> Vec<RenderNode> {
    let mut result = Vec::new();
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let w = node.rect.width;
        let h = node.rect.height;
        let pdf_y = mapper.xfa_to_pdf_y(abs_y, h);
        let node_cfg = apply_node_style(config, &node.style);

        // Emit a Rect node when background or border is configured.
        let fill = node_cfg.background_color.map(|c| {
            [
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            ]
        });
        let stroke = if node_cfg.draw_borders && node_cfg.border_width > 0.0 {
            let bc = node_cfg.border_color;
            Some([
                (bc[0] * 255.0) as u8,
                (bc[1] * 255.0) as u8,
                (bc[2] * 255.0) as u8,
            ])
        } else {
            None
        };
        if fill.is_some() || stroke.is_some() {
            result.push(RenderNode::Rect {
                x: abs_x,
                y: pdf_y,
                width: w,
                height: h,
                fill,
                stroke,
            });
        }

        let leaf = match &node.content {
            LayoutContent::Field {
                value,
                field_kind,
                font_size,
                font_family,
            } => {
                use xfa_layout_engine::form::FieldKind;
                match field_kind {
                    FieldKind::Checkbox | FieldKind::Radio => Some(RenderNode::Widget {
                        x: abs_x,
                        y: pdf_y,
                        width: w,
                        height: h,
                        field_name: node.name.clone(),
                        value: value.clone(),
                    }),
                    _ => {
                        let font_ref = config
                            .font_map
                            .get(&font_bridge_key_for_tree(*font_family))
                            .cloned()
                            .unwrap_or_else(|| node_cfg.default_font.clone());
                        if !value.is_empty() {
                            Some(RenderNode::Text {
                                x: abs_x,
                                y: pdf_y,
                                content: value.clone(),
                                font: font_ref,
                                size: if *font_size > 0.0 {
                                    *font_size
                                } else {
                                    node_cfg.default_font_size
                                },
                            })
                        } else {
                            Some(RenderNode::Widget {
                                x: abs_x,
                                y: pdf_y,
                                width: w,
                                height: h,
                                field_name: node.name.clone(),
                                value: value.clone(),
                            })
                        }
                    }
                }
            }
            LayoutContent::Text(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some(RenderNode::Text {
                        x: abs_x,
                        y: pdf_y,
                        content: text.clone(),
                        font: node_cfg.default_font.clone(),
                        size: node_cfg.default_font_size,
                    })
                }
            }
            LayoutContent::WrappedText {
                lines, font_size, ..
            } => {
                if lines.is_empty() {
                    None
                } else {
                    Some(RenderNode::Text {
                        x: abs_x,
                        y: pdf_y,
                        content: lines.join(" "),
                        font: node_cfg.default_font.clone(),
                        size: *font_size,
                    })
                }
            }
            LayoutContent::Image { data, .. } => Some(RenderNode::Image {
                x: abs_x,
                y: pdf_y,
                width: w,
                height: h,
                data_len: data.len(),
            }),
            LayoutContent::Draw(_) | LayoutContent::None => None,
        };
        if let Some(leaf_node) = leaf {
            result.push(leaf_node);
        }

        // Recurse into children.
        if !node.children.is_empty() {
            let child_x = abs_x + node.style.inset_left_pt.unwrap_or(0.0);
            let child_y = abs_y + node.style.inset_top_pt.unwrap_or(0.0);
            let child_nodes = render_tree_nodes(&node.children, child_x, child_y, mapper, config);
            if !child_nodes.is_empty() {
                result.push(RenderNode::Group {
                    children: child_nodes,
                });
            }
        }
    }
    result
}

/// Return a font-bridge lookup key from a FontFamily enum for render-tree use.
fn font_bridge_key_for_tree(family: xfa_layout_engine::text::FontFamily) -> String {
    use xfa_layout_engine::text::FontFamily;
    match family {
        FontFamily::SansSerif => "sans-serif".to_string(),
        FontFamily::Monospace => "monospace".to_string(),
        FontFamily::Serif => "serif".to_string(),
    }
}

/// Generate overlays containing only field value text — no backgrounds,
/// borders, captions, draws, or images.  Used by the preserve_static path
/// when widgets lack AP streams.
pub fn generate_field_values_overlays(
    layout: &LayoutDom,
    config: &XfaRenderConfig,
) -> Result<Vec<PageOverlay>> {
    let mut fv_config = config.clone();
    fv_config.field_values_only = true;
    layout
        .pages
        .iter()
        .map(|page| generate_page_overlay(page, &fv_config))
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

        // XFA §2.5.6 — Margin insets define the space between the element's
        // outer edges and its border/content.  Compute the inner rect (after
        // insets) and use it for caption/value offset and border/bg drawing.
        let inset_l = node.style.inset_left_pt.unwrap_or(0.0);
        let inset_t = node.style.inset_top_pt.unwrap_or(0.0);
        let inset_r = node.style.inset_right_pt.unwrap_or(0.0);
        let inset_b = node.style.inset_bottom_pt.unwrap_or(0.0);
        let inner_w = (w - inset_l - inset_r).max(0.0);
        let inner_h = (h - inset_t - inset_b).max(0.0);
        let (caption_font_size, caption_font_family) =
            caption_font_for_content(&node.content, &node.style, &node_config);
        let is_button = matches!(
            &node.content,
            LayoutContent::Field {
                field_kind: FieldKind::Button,
                ..
            }
        );
        let caption_reserve = if is_button {
            0.0
        } else {
            effective_caption_reserve(
                &node.style,
                caption_font_size,
                caption_font_family,
                &node_config,
            )
        };

        // Caption/value offset computed from inner rect (after margin insets).
        let (cap_dx, cap_dy, val_w, val_h) =
            caption_value_offset(&node.style, caption_reserve, inner_w, inner_h);
        let val_x = abs_x + inset_l + cap_dx;
        let val_y_offset = inset_t + cap_dy;
        let val_pdf_y = mapper.xfa_to_pdf_y(abs_y + val_y_offset, val_h);

        if !matches!(node.content, LayoutContent::Field { .. }) && !config.field_values_only {
            let border_radius = node.style.border_radius_pt.unwrap_or(0.0);
            let border_style = node.style.border_style.as_deref();
            // Border/bg at inner rect (after margin insets), or at value
            // area when a caption is present.
            let (bx, by, bw, bh) = if node.style.caption_text.is_some() {
                (val_x, val_pdf_y, val_w, val_h)
            } else {
                let inner_pdf_y = mapper.xfa_to_pdf_y(abs_y + inset_t, inner_h);
                (abs_x + inset_l, inner_pdf_y, inner_w, inner_h)
            };
            if let Some(bg) = &node_config.background_color {
                write_ops(
                    ops,
                    format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
                );
                emit_rect_path(ops, bx, by, bw, bh, border_radius);
                ops.extend_from_slice(b"f\n");
            }
            if node_config.draw_borders && node_config.border_width > 0.0 && bw > 0.0 && bh > 0.0 {
                let bwid = node_config.border_width;
                let bc = node_config.border_color;
                write_ops(
                    ops,
                    format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", bwid, bc[0], bc[1], bc[2]),
                );
                let per_edge = node.style.border_colors.map(|cs| {
                    cs.map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
                });
                let per_edge_widths = node.style.border_widths.as_ref();
                apply_border_dash(ops, border_style);
                let edges = node.style.border_edges;
                if per_edge.is_some() || per_edge_widths.is_some() {
                    emit_individual_edges(
                        ops,
                        bx,
                        by,
                        bw,
                        bh,
                        &edges,
                        per_edge.as_ref(),
                        per_edge_widths,
                        bwid,
                    );
                } else if edges[0] && edges[1] && edges[2] && edges[3] {
                    emit_rect_path(ops, bx, by, bw, bh, border_radius);
                    ops.extend_from_slice(b"S\n");
                } else {
                    emit_individual_edges(ops, bx, by, bw, bh, &edges, None, None, bwid);
                }
                reset_border_dash(ops, border_style);
            }
        }

        // XFA §8 — Clip content to the node's declared bounds so that
        // text in fixed-height fields cannot overflow into adjacent nodes.
        ops.extend_from_slice(b"q\n");
        // A spanned `<line>` draw fills its box edge-to-edge, but the grid rules
        // have a zero-width or zero-height box, so the usual per-node clip
        // (re W n) is a zero-area region that would erase the stroke. Skip the
        // clip for such degenerate spanned line draws (a line has nothing to
        // clip). Gated on the span flag so the opt-out path stays byte-identical.
        let skip_line_clip = draw_line_span_enabled()
            && (w <= 0.01 || h <= 0.01)
            && matches!(
                &node.content,
                LayoutContent::Draw(DrawContent::Line { x1, y1, x2, y2 })
                    if *x1 == 0.0 && *y1 == 0.0 && *x2 == 0.0 && *y2 == 0.0
            );
        if !skip_line_clip {
            write_ops(
                ops,
                format_args!("{:.2} {:.2} {:.2} {:.2} re W n\n", abs_x, pdf_y, w, h),
            );
        }

        let is_bold = node.style.font_weight.as_deref() == Some("bold");

        // Render caption for any node that has caption_text in its style.
        // For Button fields, skip the external caption — the caption text is
        // used as the button label rendered inside the button body instead.
        let is_field = matches!(&node.content, LayoutContent::Field { .. });
        let field_caption_needs_post_body_render =
            is_field && matches!(node.style.caption_placement.as_deref(), Some("top"));
        if node.style.caption_text.is_some()
            && !is_button
            && (!is_field || !field_caption_needs_post_body_render)
            && !config.field_values_only
        {
            render_caption(
                abs_x + inset_l,
                mapper.xfa_to_pdf_y(abs_y + inset_t, inner_h),
                inner_w,
                inner_h,
                caption_reserve,
                caption_font_size,
                caption_font_family,
                &node.style,
                &node_config,
                ops,
            );
        }

        match &node.content {
            LayoutContent::Field {
                value,
                field_kind,
                font_size,
                font_family,
            } => {
                match field_kind {
                    FieldKind::Checkbox => render_checkbox(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        value,
                        &node.style,
                        &node_config,
                        ops,
                    ),
                    FieldKind::Radio => render_radio(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        value,
                        &node.style,
                        &node_config,
                        ops,
                    ),
                    FieldKind::Dropdown => render_dropdown(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        value,
                        *font_size,
                        *font_family,
                        &node.style,
                        &node_config,
                        ops,
                        &node.display_items,
                        &node.save_items,
                    ),
                    FieldKind::Button => {
                        let label = if value.is_empty() {
                            node.style.caption_text.as_deref().unwrap_or("")
                        } else {
                            value
                        };
                        if !label.is_empty() {
                            render_button(
                                val_x,
                                val_pdf_y,
                                val_w,
                                val_h,
                                label,
                                *font_size,
                                *font_family,
                                &node.style,
                                &node_config,
                                ops,
                            )
                        }
                    }
                    FieldKind::PasswordEdit => {
                        let masked_value: String = value.chars().map(|_| '•').collect();
                        render_field(
                            val_x,
                            val_pdf_y,
                            val_w,
                            val_h,
                            &masked_value,
                            *font_size,
                            *font_family,
                            &node.style,
                            &node_config,
                            ops,
                        )
                    }
                    FieldKind::Barcode => render_barcode(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        value,
                        &node.style,
                        &node_config,
                        ops,
                    ),
                    FieldKind::Signature => render_signature(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        value,
                        &node.style,
                        &node_config,
                        ops,
                    ),
                    _ => {
                        let display_val = if node.style.format_pattern.is_some() {
                            crate::appearance_bridge::format_value(
                                value,
                                node.style.format_pattern.as_deref(),
                            )
                        } else if matches!(field_kind, FieldKind::NumericEdit) {
                            crate::appearance_bridge::format_numeric_default(value)
                        } else {
                            value.to_string()
                        };
                        render_field(
                            val_x,
                            val_pdf_y,
                            val_w,
                            val_h,
                            &display_val,
                            *font_size,
                            *font_family,
                            &node.style,
                            &node_config,
                            ops,
                        )
                    }
                }

                if node.style.caption_text.is_some()
                    && !is_button
                    && field_caption_needs_post_body_render
                    && !config.field_values_only
                {
                    // fixes #818: only top-placed field captions need the
                    // post-body path. Left/right/bottom captions relied on the
                    // legacy pre-body ordering, and moving all field captions
                    // after the body made caption-only empty text fields show
                    // labels that Adobe/pdfRest keep visually blank (#3f563698).
                    render_caption(
                        abs_x + inset_l,
                        mapper.xfa_to_pdf_y(abs_y + inset_t, inner_h),
                        inner_w,
                        inner_h,
                        caption_reserve,
                        caption_font_size,
                        caption_font_family,
                        &node.style,
                        &node_config,
                        ops,
                    );
                }
            }
            LayoutContent::Text(_) if config.field_values_only => {}
            LayoutContent::Text(text) => {
                let inner_pdf_y = mapper.xfa_to_pdf_y(abs_y + inset_t, inner_h);
                render_text(
                    abs_x + inset_l,
                    inner_pdf_y,
                    inner_w,
                    inner_h,
                    text,
                    &node.style,
                    &node_config,
                    ops,
                )
            }
            LayoutContent::WrappedText {
                from_field: false, ..
            } if config.field_values_only => {}
            LayoutContent::WrappedText {
                lines,
                first_line_of_para,
                font_size,
                text_align,
                font_family,
                ..
            } => {
                // Only use the rich-text renderer when there are multiple
                // spans with distinct formatting. Single-span rich text
                // (or spans with no style overrides) renders better via
                // the standard multiline path which has more mature
                // positioning logic.
                let use_rich = node.style.rich_text_spans.as_ref().is_some_and(|spans| {
                    spans.len() > 1
                        || spans.iter().any(|s| {
                            s.font_size.is_some()
                                || s.font_family.is_some()
                                || s.font_weight.is_some()
                                || s.font_style.is_some()
                                || s.text_color.is_some()
                                || s.underline
                        })
                });
                if use_rich {
                    if let Some(ref spans) = node.style.rich_text_spans {
                        render_rich_multiline(
                            val_x,
                            val_w,
                            val_h,
                            lines,
                            first_line_of_para,
                            spans,
                            *font_size,
                            *text_align,
                            *font_family,
                            mapper,
                            abs_y + val_y_offset,
                            &node.style,
                            &node_config,
                            ops,
                        );
                    }
                } else {
                    render_multiline(
                        val_x,
                        val_pdf_y,
                        val_w,
                        val_h,
                        lines,
                        first_line_of_para,
                        *font_size,
                        *text_align,
                        *font_family,
                        is_bold,
                        mapper,
                        abs_y + val_y_offset,
                        &node.style,
                        &node_config,
                        ops,
                    );
                }
            }
            LayoutContent::Image { .. } if config.field_values_only => {}
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
            LayoutContent::Draw(_) if config.field_values_only => {}
            LayoutContent::Draw(draw_content) => {
                render_draw(
                    draw_content,
                    abs_x,
                    pdf_y,
                    w,
                    h,
                    &node.style,
                    &node_config,
                    ops,
                );
            }
            LayoutContent::None => {}
        }

        // Restore graphics state (removes per-node clip rect).
        ops.extend_from_slice(b"Q\n");

        if !node.children.is_empty() {
            // Children are laid out relative to the content area (after insets),
            // so offset by the parent's margin insets (XFA <margin leftInset/topInset>).
            // Apply both left and top insets symmetrically when entering the
            // child coordinate space.
            let child_origin_x = abs_x + node.style.inset_left_pt.unwrap_or(0.0);
            let child_origin_y = abs_y + node.style.inset_top_pt.unwrap_or(0.0);
            render_nodes(
                &node.children,
                child_origin_x,
                child_origin_y,
                mapper,
                config,
                ops,
                images,
            );
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

/// Draw individual border edges with optional per-edge colors and widths.
#[allow(clippy::too_many_arguments)]
fn emit_individual_edges(
    ops: &mut Vec<u8>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    edges: &[bool; 4],
    colors: Option<&[[f64; 3]; 4]>,
    widths: Option<&[f64; 4]>,
    default_width: f64,
) {
    if edges[0] {
        let ww = widths.map(|w| w[0]).unwrap_or(default_width);
        if let Some(c) = colors.map(|c| &c[0]) {
            write_ops(
                ops,
                format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", ww, c[0], c[1], c[2]),
            );
        } else {
            write_ops(ops, format_args!("{:.2} w\n", ww));
        }
        write_ops(
            ops,
            format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x, y + h, x + w, y + h),
        );
    }
    if edges[1] {
        let ww = widths.map(|w| w[1]).unwrap_or(default_width);
        if let Some(c) = colors.map(|c| &c[1]) {
            write_ops(
                ops,
                format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", ww, c[0], c[1], c[2]),
            );
        } else {
            write_ops(ops, format_args!("{:.2} w\n", ww));
        }
        write_ops(
            ops,
            format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x + w, y, x + w, y + h),
        );
    }
    if edges[2] {
        let ww = widths.map(|w| w[2]).unwrap_or(default_width);
        if let Some(c) = colors.map(|c| &c[2]) {
            write_ops(
                ops,
                format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", ww, c[0], c[1], c[2]),
            );
        } else {
            write_ops(ops, format_args!("{:.2} w\n", ww));
        }
        write_ops(
            ops,
            format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x, y, x + w, y),
        );
    }
    if edges[3] {
        let ww = widths.map(|w| w[3]).unwrap_or(default_width);
        if let Some(c) = colors.map(|c| &c[3]) {
            write_ops(
                ops,
                format_args!("{:.2} w\n{:.3} {:.3} {:.3} RG\n", ww, c[0], c[1], c[2]),
            );
        } else {
            write_ops(ops, format_args!("{:.2} w\n", ww));
        }
        write_ops(
            ops,
            format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x, y, x, y + h),
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

/// Emit a 3D "lowered" or "raised" border (XFA edge stroke attribute).
///
/// "lowered" — top/left edges are dark (shadow), bottom/right are light (highlight).
/// "raised"  — top/left edges are light, bottom/right are dark.
fn emit_3d_border(
    ops: &mut Vec<u8>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    line_w: f64,
    style: Option<&str>,
) {
    let dark = [0.502, 0.502, 0.502]; // mid-gray shadow
    let light = [0.831, 0.831, 0.831]; // light-gray highlight
    let (tl, br) = match style {
        Some("lowered") => (dark, light),
        _ => (light, dark), // raised
    };
    write_ops(ops, format_args!("{:.2} w\n", line_w));
    // Top edge (tl color)
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.2} {:.2} m {:.2} {:.2} l S\n",
            tl[0],
            tl[1],
            tl[2],
            x,
            y + h,
            x + w,
            y + h,
        ),
    );
    // Left edge (tl color)
    write_ops(
        ops,
        format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x, y + h, x, y,),
    );
    // Bottom edge (br color)
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.2} {:.2} m {:.2} {:.2} l S\n",
            br[0],
            br[1],
            br[2],
            x,
            y,
            x + w,
            y,
        ),
    );
    // Right edge (br color)
    write_ops(
        ops,
        format_args!("{:.2} {:.2} m {:.2} {:.2} l S\n", x + w, y, x + w, y + h,),
    );
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
    node_style.font_weight.as_deref() == Some("bold")
}

/// Returns true when the resolved resource came from an actual bold variant in
/// the font map, so synthetic bold stroking is unnecessary.
fn style_uses_real_bold_variant(
    font_map: &HashMap<String, String>,
    node_style: &FormNodeStyle,
) -> bool {
    if !is_bold_style(node_style) {
        return false;
    }
    let Some(typeface) = node_style.font_family.as_deref() else {
        return false;
    };
    let vkey = font_variant_key(
        typeface,
        node_style.font_weight.as_deref(),
        node_style.font_style.as_deref(),
    );
    font_map.contains_key(&vkey)
}

/// Emit synthetic bold operators: fill+stroke rendering mode with thin stroke.
/// Uses text rendering mode 2 (fill then stroke) to simulate bold weight when
/// the actual bold font variant is unavailable.
fn emit_synthetic_bold_ops(
    font_map: &HashMap<String, String>,
    node_style: &FormNodeStyle,
    font_size: f64,
    text_color: &[f64; 3],
    ops: &mut Vec<u8>,
) {
    if is_bold_style(node_style) && !style_uses_real_bold_variant(font_map, node_style) {
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
fn reset_synthetic_bold_ops(
    font_map: &HashMap<String, String>,
    node_style: &FormNodeStyle,
    ops: &mut Vec<u8>,
) {
    if is_bold_style(node_style) && !style_uses_real_bold_variant(font_map, node_style) {
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

/// Whether a caption's own `<font>` styles its label (XFA 3.3 §7.4). Default-ON;
/// opt out with `XFA_CAPTION_OWN_FONT=0|off|false` to restore the legacy
/// behaviour where the caption borrowed the field's font.
fn caption_own_font_enabled() -> bool {
    !matches!(
        std::env::var("XFA_CAPTION_OWN_FONT").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("OFF") | Ok("FALSE")
    )
}

fn caption_font_for_content(
    content: &LayoutContent,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
) -> (f64, FontFamily) {
    let (field_size, field_family) = match content {
        LayoutContent::Field {
            font_size,
            font_family,
            ..
        }
        | LayoutContent::WrappedText {
            font_size,
            font_family,
            ..
        } => (*font_size, *font_family),
        _ => (
            node_style.font_size.unwrap_or(config.default_font_size),
            FontFamily::SansSerif,
        ),
    };
    // The caption's own typeface wins over the field's — otherwise an Arial
    // caption on a Times field is classified Serif and renders serif.
    if caption_own_font_enabled() {
        if let Some(cap_face) = &node_style.caption_font_family {
            return (
                node_style.caption_font_size.unwrap_or(field_size),
                classify_font_family(cap_face),
            );
        }
    }
    (field_size, field_family)
}

fn effective_caption_reserve(
    style: &FormNodeStyle,
    font_size: f64,
    font_family: FontFamily,
    config: &XfaRenderConfig,
) -> f64 {
    let caption_text = match style.caption_text.as_deref() {
        Some(text) if !text.is_empty() => text,
        _ => return 0.0,
    };
    if let Some(reserve) = style.caption_reserve {
        return reserve.max(0.0);
    }
    let fs = if font_size > 0.0 {
        font_size
    } else {
        config.default_font_size
    };
    let metrics = build_font_metrics(fs, font_family, style, config);
    match style.caption_placement.as_deref().unwrap_or("left") {
        "left" | "right" => metrics.measure_width(caption_text),
        "top" | "bottom" => metrics.line_height_pt(),
        _ => 0.0,
    }
}

/// Compute the offset and size of the value area within a field that has a caption.
///
/// Returns (dx, dy, value_width, value_height) where dx/dy are the offsets from
/// the field origin to the value area origin.
fn caption_value_offset(
    style: &FormNodeStyle,
    reserve: f64,
    w: f64,
    h: f64,
) -> (f64, f64, f64, f64) {
    if reserve <= 0.0 || style.caption_text.is_none() {
        return (0.0, 0.0, w, h);
    }
    match style.caption_placement.as_deref().unwrap_or("left") {
        "left" => (reserve, 0.0, (w - reserve).max(0.0), h),
        "right" => (0.0, 0.0, (w - reserve).max(0.0), h),
        "top" => (0.0, reserve, w, (h - reserve).max(0.0)),
        "bottom" => (0.0, 0.0, w, (h - reserve).max(0.0)),
        _ => (0.0, 0.0, w, h),
    }
}

/// Render field caption text (shared across all field types).
///
/// This renders `<caption>` text at the placement offset (left/right/top/bottom)
/// relative to the node's full inner rectangle.
///
/// Field captions are emitted after the field body so the editable value-area
/// fill and border cannot paint over them. XFA 3.3 §7.4 treats caption reserve
/// as a separate region inside the field allocation rectangle. (#818)
#[allow(clippy::too_many_arguments)]
fn render_caption(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    caption_reserve: f64,
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    let caption_text = match &node_style.caption_text {
        Some(t) if !t.is_empty() => t,
        _ => return,
    };
    let caption_placement = node_style.caption_placement.as_deref().unwrap_or("left");
    let fs = if font_size > 0.0 {
        font_size
    } else {
        config.default_font_size
    };
    // Resolve the caption's font from ITS OWN typeface when specified, so it is
    // not drawn in the field's face (the field's `<font>` styles the value, not
    // the caption). Layout/placement still uses the field's node_style.
    let cap_owned;
    let cap_style: &FormNodeStyle = match &node_style.caption_font_family {
        Some(cf) if caption_own_font_enabled() => {
            cap_owned = FormNodeStyle {
                font_family: Some(cf.clone()),
                ..node_style.clone()
            };
            &cap_owned
        }
        _ => node_style,
    };
    let metrics = build_font_metrics(fs, font_family, cap_style, config);
    let font_ref = resolve_font_ref(&config.font_map, cap_style, font_family);
    let idh_metrics = lookup_font_metrics(cap_style, config);

    // Determine the caption bounding box.
    let (cap_x, cap_y, cap_w, cap_h) = match caption_placement {
        "left" => (x, pdf_y, caption_reserve, h),
        "right" => (x + w - caption_reserve, pdf_y, caption_reserve, h),
        "top" => (x, pdf_y + h - caption_reserve, w, caption_reserve),
        "bottom" => (x, pdf_y, w, caption_reserve),
        _ => (x, pdf_y, caption_reserve, h),
    };

    // For multi-line captions (contains newlines or wider than caption area),
    // wrap and render line-by-line.  Single-line captions use the fast path.
    let is_multiline = caption_text.contains('\n') || metrics.measure_width(caption_text) > cap_w;

    if is_multiline {
        let line_height = node_style
            .line_height_pt
            .unwrap_or_else(|| metrics.line_height_pt());
        let pad_left = node_style.margin_left_pt.unwrap_or(0.0);
        let pad_right = node_style.margin_right_pt.unwrap_or(0.0);
        let text_indent = node_style.text_indent_pt.unwrap_or(0.0);
        let usable_w = (cap_w - pad_left - pad_right).max(1.0);

        // Wrap text paragraphs.
        let layout = xfa_layout_engine::text::wrap_text(
            caption_text,
            usable_w,
            &metrics,
            text_indent,
            node_style.line_height_pt,
        );

        if layout.lines.is_empty() {
            return;
        }

        let asc_pt = ascender_pt(&metrics, fs);
        let space_above = node_style.space_above_pt.unwrap_or(0.0);
        let total_text_h = layout.lines.len() as f64 * line_height;

        // Vertical start position (PDF y, top-of-first-line baseline).
        let first_line_pdf_y = match node_style.v_align {
            Some(VerticalAlign::Middle) => {
                cap_y + cap_h - asc_pt - space_above - (cap_h - space_above - total_text_h) / 2.0
                    + (cap_h - space_above - total_text_h) / 2.0
            }
            Some(VerticalAlign::Bottom) => cap_y + total_text_h - asc_pt,
            _ => cap_y + cap_h - asc_pt - space_above,
        };

        let tc = node_style
            .text_color
            .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
            .unwrap_or(config.text_color);

        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                tc[0], tc[1], tc[2], font_ref, fs
            ),
        );
        emit_text_style_ops(node_style, ops);

        let text_x_base = cap_x + pad_left;
        let mut prev_x = text_x_base;
        for (i, line) in layout.lines.iter().enumerate() {
            let is_para_start = layout.first_line_of_para.get(i).copied().unwrap_or(false);
            let indent = if is_para_start { text_indent } else { 0.0 };
            let text_x = text_x_base + indent;
            let line_y = first_line_pdf_y - (i as f64 * line_height);
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
        ops.extend_from_slice(b"ET\n");
    } else {
        // Single-line fast path.
        let asc_pt = ascender_pt(&metrics, fs);
        let text_y = cap_y + cap_h - asc_pt;

        let encoded = pdf_encode_text(caption_text, idh_metrics);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                config.text_color[0], config.text_color[1], config.text_color[2], font_ref, fs,
            ),
        );
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!("{:.2} {:.2} Td\n{} Tj\n", cap_x, text_y, encoded),
        );
        reset_text_style_ops(node_style, ops);
        ops.extend_from_slice(b"ET\n");
    }
}

#[allow(clippy::too_many_arguments)]
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

    if !config.field_values_only {
        // fix(#809): flattening should only paint explicit template fills.
        // The light-gray interactive widget default is a viewer affordance, not a
        // flatten artifact in Adobe/pdfRest output.
        if let Some(bg) = config.background_color {
            write_ops(
                ops,
                format_args!("{:.3} {:.3} {:.3} rg\n", bg[0], bg[1], bg[2]),
            );
            emit_rect_path(ops, x, pdf_y, w, h, border_radius);
            ops.extend_from_slice(b"f\n");
        }
        if config.draw_borders && config.border_width > 0.0 {
            if matches!(border_style, Some("lowered") | Some("raised")) {
                emit_3d_border(ops, x, pdf_y, w, h, config.border_width, border_style);
            } else {
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
                let per_edge = node_style.border_colors.map(|cs| {
                    cs.map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
                });
                let per_edge_widths = node_style.border_widths.as_ref();
                apply_border_dash(ops, border_style);
                let edges = node_style.border_edges;
                if per_edge.is_some() || per_edge_widths.is_some() {
                    emit_individual_edges(
                        ops,
                        x,
                        pdf_y,
                        w,
                        h,
                        &edges,
                        per_edge.as_ref(),
                        per_edge_widths,
                        config.border_width,
                    );
                } else if edges[0] && edges[1] && edges[2] && edges[3] {
                    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
                    ops.extend_from_slice(b"S\n");
                } else {
                    emit_individual_edges(
                        ops,
                        x,
                        pdf_y,
                        w,
                        h,
                        &edges,
                        None,
                        None,
                        config.border_width,
                    );
                }
                reset_border_dash(ops, border_style);
            }
        }
    } // end if !config.field_values_only
    if !value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        // Insets already applied by render_nodes — x, w, h are the value
        // area inside margin insets.  Only para marginLeft/Right apply here.
        let pad_left = node_style.margin_left_pt.unwrap_or(0.0);
        let pad_right = node_style.margin_right_pt.unwrap_or(0.0);
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
                Some(VerticalAlign::Middle) => {
                    pdf_y + space_above + (h - space_above - line_h) / 2.0
                }
                Some(VerticalAlign::Bottom) => pdf_y + space_above,
                _ => pdf_y + h - space_above - asc_pt,
            };
            let encoded = pdf_encode_text(value, idh_metrics);
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                    config.text_color[0], config.text_color[1], config.text_color[2], font_ref, fs,
                ),
            );
            emit_synthetic_bold_ops(&config.font_map, node_style, fs, &config.text_color, ops);
            emit_text_style_ops(node_style, ops);
            write_ops(
                ops,
                format_args!("{:.2} {:.2} Td\n{} Tj\n", x + pad_left, text_y, encoded),
            );
            reset_text_style_ops(node_style, ops);
            reset_synthetic_bold_ops(&config.font_map, node_style, ops);
            ops.extend_from_slice(b"ET\n");
        } else {
            let lines = wrap_text(value, content_w, &metrics);
            let line_height = metrics.line_height_pt();
            let asc_pt = ascender_pt(&metrics, fs);
            let total_content_h = lines.len() as f64 * line_height;
            let text_start_y = match node_style.v_align {
                Some(VerticalAlign::Middle) => {
                    pdf_y + space_above + (h - space_above - total_content_h) / 2.0
                }
                Some(VerticalAlign::Bottom) => pdf_y + space_above,
                _ => pdf_y + h - space_above - total_content_h,
            };
            write_ops(
                ops,
                format_args!(
                    "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                    config.text_color[0], config.text_color[1], config.text_color[2], font_ref, fs,
                ),
            );
            emit_synthetic_bold_ops(&config.font_map, node_style, fs, &config.text_color, ops);
            emit_text_style_ops(node_style, ops);
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} Td\n",
                    x + pad_left,
                    text_start_y + total_content_h - asc_pt,
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
            reset_synthetic_bold_ops(&config.font_map, node_style, ops);
            ops.extend_from_slice(b"ET\n");
        }
    }
}

/// Draw a check mark symbol inside a checkbox/radio bounding box.
///
/// Supported marks (XFA §8.2): check, circle, cross, diamond, square, star.
#[allow(clippy::too_many_arguments)]
fn draw_check_mark(
    mark: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    m: f64,
    color: [f64; 3],
    ops: &mut Vec<u8>,
) {
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.3} {:.3} {:.3} rg\n",
            color[0], color[1], color[2], color[0], color[1], color[2]
        ),
    );
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    match mark {
        "check" => {
            // Checkmark: three line segments
            let lw = (w.min(h) * 0.08).max(0.5);
            write_ops(ops, format_args!("{:.2} w\n", lw));
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\n{:.2} {:.2} l\nS\n",
                    x + m,
                    cy,
                    cx - m * 0.3,
                    y + m,
                    x + w - m,
                    y + h - m,
                ),
            );
        }
        "circle" => {
            let r = (w.min(h) / 2.0 - m).max(1.0);
            let k = r * 0.5523; // bezier approx for circle
            write_ops(ops, format_args!(
                "{:.2} {:.2} m\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\nf\n",
                cx + r, cy,
                cx + r, cy + k, cx + k, cy + r, cx, cy + r,
                cx - k, cy + r, cx - r, cy + k, cx - r, cy,
                cx - r, cy - k, cx - k, cy - r, cx, cy - r,
                cx + k, cy - r, cx + r, cy - k, cx + r, cy,
            ));
        }
        "diamond" => {
            let d = (w.min(h) / 2.0 - m).max(1.0);
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\n{:.2} {:.2} l\n{:.2} {:.2} l\nf\n",
                    cx,
                    cy + d,
                    cx - d,
                    cy,
                    cx,
                    cy - d,
                    cx + d,
                    cy,
                ),
            );
        }
        "square" => {
            let s = (w.min(h) - 2.0 * m).max(1.0);
            write_ops(
                ops,
                format_args!("{:.2} {:.2} {:.2} {:.2} re\nf\n", x + m, y + m, s, s,),
            );
        }
        "star" => {
            // Simplified 5-point star via cross pattern
            let lw = (w.min(h) * 0.08).max(0.5);
            write_ops(ops, format_args!("{:.2} w\n", lw));
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\nS\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
                    x + m,
                    y + m,
                    x + w - m,
                    y + h - m,
                    x + w - m,
                    y + m,
                    x + m,
                    y + h - m,
                ),
            );
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
                    cx,
                    y + m * 0.5,
                    cx,
                    y + h - m * 0.5,
                ),
            );
        }
        _ => {
            // Default "cross"
            let lw = (w.min(h) * 0.08).max(0.5);
            write_ops(ops, format_args!("{:.2} w\n", lw));
            write_ops(
                ops,
                format_args!(
                    "{:.2} {:.2} m\n{:.2} {:.2} l\nS\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
                    x + m,
                    y + m,
                    x + w - m,
                    y + h - m,
                    x + w - m,
                    y + m,
                    x + m,
                    y + h - m,
                ),
            );
        }
    }
}

/// Render a checkbox widget (XFA `<checkButton>`).
///
/// # Checked/unchecked state (XFA-F5-03)
///
/// The checked state is resolved by `is_check_button_checked`, which compares
/// the bound `value` against:
/// - `node_style.check_button_on_value` — the "on" value string from the XFA
///   template's `<items>` or `<value><integer>1</integer></value>` entry.
/// - A set of conventional truthy strings: "1", "true", "yes", "on" (case-
///   insensitive) when no explicit on-value is set.
///
/// When checked, `draw_check_mark` renders the mark symbol according to
/// `config.check_button_mark` (XFA `<checkButton mark="check|cross|…">`).
/// Supported marks: check (default), cross, circle, diamond, square, star.
///
/// When unchecked, only the outer rectangle border is drawn.
///
/// Background fill is only applied when `node_style.bg_color` is explicitly set
/// (i.e. the XFA template has a `<fill>` element).  Global config background
/// is intentionally ignored to avoid regression with check/radio controls.
#[allow(clippy::too_many_arguments)]
fn render_checkbox(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    let bw = config.border_width;
    // Only explicit template fill (<fill>/<border><fill>) should paint widget
    // background for check/radio controls. Do not inherit global/default field
    // background config here; that caused checkbox/radio regressions (#fill).
    write_ops(ops, format_args!("q\n"));
    if let Some((r_u8, g_u8, b_u8)) = node_style.bg_color {
        let bg = [
            r_u8 as f64 / 255.0,
            g_u8 as f64 / 255.0,
            b_u8 as f64 / 255.0,
        ];
        write_ops(
            ops,
            format_args!(
                "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
                bg[0], bg[1], bg[2], x, pdf_y, w, h
            ),
        );
    }
    write_ops(
        ops,
        format_args!(
            "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
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
    let checked = is_check_button_checked(value, FieldKind::Checkbox, node_style);
    if checked {
        let mark = config
            .check_button_mark
            .as_deref()
            .unwrap_or(default_check_button_mark(FieldKind::Checkbox));
        let m = w.min(h) * 0.15;
        let color = config.text_color;
        draw_check_mark(mark, x, pdf_y, w, h, m, color, ops);
    }
    write_ops(ops, format_args!("Q\n"));
}

/// Render a radio-button widget (XFA `<checkButton shape="round">`).
///
/// # Selected state (XFA-F5-03)
///
/// Selected state is resolved identically to checkboxes via
/// `is_check_button_checked` using `FieldKind::Radio`.
///
/// When selected, a filled inner circle (smaller radius, ~55% of outer) is
/// drawn using 4 Bézier curves (`c` operator) unless `node_style.check_button_mark`
/// overrides the symbol type (e.g. `mark="cross"` draws an ✕ instead).
///
/// When deselected, only the outer circle border is drawn (stroke only, no fill).
///
/// Background fill follows the same rule as checkboxes: only `node_style.bg_color`
/// (explicit template fill) is honoured; global config background is ignored.
#[allow(clippy::too_many_arguments)]
fn render_radio(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    let bw = config.border_width;
    let cx = x + w / 2.0;
    let cy = pdf_y + h / 2.0;
    let r = w.min(h) / 2.0;

    // Draw circle using 4 Bezier curves (standard circle approximation).
    let k = 0.5523; // kappa ≈ 4*(√2-1)/3
    let kx = r * k;
    let ky = r * k;
    // Only explicit template fill should paint radio background. Do not use
    // inherited/global background config for radio controls.
    write_ops(ops, format_args!("q\n",));
    if let Some((r_u8, g_u8, b_u8)) = node_style.bg_color {
        let bg = [
            r_u8 as f64 / 255.0,
            g_u8 as f64 / 255.0,
            b_u8 as f64 / 255.0,
        ];
        write_ops(
            ops,
            format_args!(
                "{:.3} {:.3} {:.3} rg\n\
                 {:.2} {:.2} m\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                 f\n",
                bg[0],
                bg[1],
                bg[2],
                cx + r,
                cy,
                cx + r,
                cy + ky,
                cx + kx,
                cy + r,
                cx,
                cy + r,
                cx - kx,
                cy + r,
                cx - r,
                cy + ky,
                cx - r,
                cy,
                cx - r,
                cy - ky,
                cx - kx,
                cy - r,
                cx,
                cy - r,
                cx + kx,
                cy - r,
                cx + r,
                cy - ky,
                cx + r,
                cy,
            ),
        );
    }
    write_ops(
        ops,
        format_args!(
            "{:.2} w\n{:.3} {:.3} {:.3} RG\n",
            bw, config.border_color[0], config.border_color[1], config.border_color[2],
        ),
    );
    write_ops(
        ops,
        format_args!(
            "{:.2} {:.2} m\n\
             {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
             {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
             {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
             {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
             S\n",
            cx + r,
            cy,
            cx + r,
            cy + ky,
            cx + kx,
            cy + r,
            cx,
            cy + r,
            cx - kx,
            cy + r,
            cx - r,
            cy + ky,
            cx - r,
            cy,
            cx - r,
            cy - ky,
            cx - kx,
            cy - r,
            cx,
            cy - r,
            cx + kx,
            cy - r,
            cx + r,
            cy - ky,
            cx + r,
            cy,
        ),
    );

    let checked = is_check_button_checked(value, FieldKind::Radio, node_style);
    if checked {
        let mark = config
            .check_button_mark
            .as_deref()
            .unwrap_or(default_check_button_mark(FieldKind::Radio));
        if mark == "circle" {
            // fixes #798: Acrobat uses a filled inner circle for asserted
            // round radios unless the template overrides `mark`.
            let ir = r * 0.4;
            let ikx = ir * k;
            let iky = ir * k;
            write_ops(
                ops,
                format_args!(
                    "{:.3} {:.3} {:.3} rg\n\
                     {:.2} {:.2} m\n\
                     {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                     {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                     {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                     {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c\n\
                     f\n",
                    config.text_color[0],
                    config.text_color[1],
                    config.text_color[2],
                    cx + ir,
                    cy,
                    cx + ir,
                    cy + iky,
                    cx + ikx,
                    cy + ir,
                    cx,
                    cy + ir,
                    cx - ikx,
                    cy + ir,
                    cx - ir,
                    cy + iky,
                    cx - ir,
                    cy,
                    cx - ir,
                    cy - iky,
                    cx - ikx,
                    cy - ir,
                    cx,
                    cy - ir,
                    cx + ikx,
                    cy - ir,
                    cx + ir,
                    cy - iky,
                    cx + ir,
                    cy,
                ),
            );
        } else {
            let m = w.min(h) * 0.15;
            draw_check_mark(mark, x, pdf_y, w, h, m, config.text_color, ops);
        }
    }
    write_ops(ops, format_args!("Q\n"));
}

fn default_check_button_mark(field_kind: FieldKind) -> &'static str {
    match field_kind {
        FieldKind::Radio => "circle",
        _ => "cross",
    }
}

fn is_check_button_checked(value: &str, field_kind: FieldKind, node_style: &FormNodeStyle) -> bool {
    // fixes #798: XFA 3.3 §11.2.1 / §17.8 says checkButton state is driven by
    // the template's `<items>` list, not by hardcoded 1/0-only semantics.
    let on_value = node_style.check_button_on_value.as_deref().unwrap_or("1");
    let off_value = node_style.check_button_off_value.as_deref().unwrap_or("");
    let neutral_value = node_style
        .check_button_neutral_value
        .as_deref()
        .unwrap_or("");
    if value == on_value {
        return true;
    }
    if value == off_value {
        return false;
    }
    if field_kind == FieldKind::Checkbox && value == neutral_value {
        return false;
    }

    false
}

/// Render a dropdown/listbox widget (XFA `<choiceList>`).
///
/// # Selected value display (XFA-F5-03)
///
/// The dropdown renders its **display value** (the human-readable label),
/// not the raw data value.  Per XFA Spec 3.3 §7.7, a `<choiceList>` has two
/// parallel item lists:
/// - `save_items` — the programmatic values bound to the data model.
/// - `display_items` — the labels shown to the user.
///
/// Resolution order:
/// 1. Look up `value` in `save_items`; if found, render `display_items[idx]`.
/// 2. If no match, render `value` as-is (fallback for pre-filled raw text).
/// 3. If the resolved display value is empty, nothing is rendered (matches
///    Adobe Acrobat behaviour for empty choice fields).
///
/// The dropdown arrow indicator (triangle) is rendered via `/F2` (Symbol font)
/// so it appears even when the main font map does not include Symbol.
#[allow(clippy::too_many_arguments)]
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
    display_items: &[String],
    save_items: &[String],
) {
    // XFA 3.3 §7.7: choiceList display value resolution.
    // If the field value matches a save item, use the corresponding display item.
    let display_value = if let Some(idx) = save_items.iter().position(|s| s == value) {
        display_items.get(idx).map(|s| s.as_str()).unwrap_or(value)
    } else {
        value
    };

    // Adobe behavior: empty dropdowns are invisible
    if display_value.is_empty() {
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

    if !display_value.is_empty() {
        let fs = if font_size > 0.0 {
            font_size
        } else {
            config.default_font_size
        };
        let _metrics = build_font_metrics(fs, font_family, node_style, config);
        let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
        let idh_metrics = lookup_font_metrics(node_style, config);

        let v_offset = pdf_y + h / 2.0 - fs / 2.0;
        let encoded = pdf_encode_text(display_value, idh_metrics);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                config.text_color[0], config.text_color[1], config.text_color[2], font_ref, fs,
            ),
        );
        emit_synthetic_bold_ops(&config.font_map, node_style, fs, &config.text_color, ops);
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!("{:.2} {:.2} Td\n{} Tj\n", x + 2.0, v_offset, encoded),
        );
        reset_text_style_ops(node_style, ops);
        reset_synthetic_bold_ops(&config.font_map, node_style, ops);
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

#[allow(clippy::too_many_arguments)]
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
    let border_radius = node_style.border_radius_pt.unwrap_or(0.0);
    let bw = config.border_width.max(0.0);

    // Use the node's bg_color (from <border><fill><color>) when available;
    // otherwise fall back to computed shading from the config border color.
    let fill_color = if let Some((r, g, b)) = node_style.bg_color {
        [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0]
    } else {
        [
            (config.border_color[0] + 0.3).min(1.0),
            (config.border_color[1] + 0.3).min(1.0),
            (config.border_color[2] + 0.3).min(1.0),
        ]
    };
    let border_color = node_style
        .border_color
        .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
        .unwrap_or([
            fill_color[0] * 0.6,
            fill_color[1] * 0.6,
            fill_color[2] * 0.6,
        ]);

    write_ops(
        ops,
        format_args!(
            "q\n{:.3} {:.3} {:.3} rg\n",
            fill_color[0], fill_color[1], fill_color[2]
        ),
    );
    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
    ops.extend_from_slice(b"f\n");

    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.2} w\n",
            border_color[0], border_color[1], border_color[2], bw
        ),
    );
    emit_rect_path(ops, x, pdf_y, w, h, border_radius);
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
        let tc = node_style
            .text_color
            .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
            .unwrap_or(config.text_color);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                tc[0], tc[1], tc[2], font_ref, fs,
            ),
        );
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!("{:.2} {:.2} Td\n{} Tj\n", text_x, v_offset, encoded),
        );
        reset_text_style_ops(node_style, ops);
        ops.extend_from_slice(b"ET\n");
    }
    // Balance the `q` pushed at the top of this function. Without this the
    // button leaves an extra graphics-state frame on the stack so the caller's
    // outer `Q` (render_nodes) ends up popping this frame instead of the
    // per-node clip frame. The button's clip region then leaks across sibling
    // fields — see 053ecab3: every TextField rendered after the button ended
    // up clipped to the empty intersection of its own rect and the button's.
    ops.extend_from_slice(b"Q\n");
}

#[allow(clippy::too_many_arguments)]
/// Teken een barcode in het veld.
///
/// Een barcodeveld werd herkend en op maat gebracht en daarna als gewone tekst
/// getekend: het formulier was niet scanbaar, wat de enige reden is dat er een
/// barcode op staat. Zie #147.
///
/// Valt terug op niets tekenen wanneer de waarde leeg is of tekens bevat die
/// Code 39 niet kent. Stil weglaten van tekens zou een barcode opleveren die
/// naar iets anders scant dan er staat, en dat is erger dan geen barcode -- dat
/// merk je pas bij de klant met een handscanner in de hand.
fn render_barcode(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    _node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if value.is_empty() || w <= 0.0 || h <= 0.0 {
        return;
    }
    let Some(elementen) = crate::barcode::encode_code39(value) else {
        return;
    };
    let modules = crate::barcode::total_modules(&elementen);
    if modules == 0 {
        return;
    }
    // Schaal de code op de veldbreedte. Smaller dan een kwart punt per module
    // is niet meer te scannen; dan tekenen we liever niets dan een streepjespatroon
    // waar niemand iets aan heeft.
    let module_breedte = w / f64::from(modules);
    if module_breedte < 0.25 {
        return;
    }

    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} rg\n",
            config.text_color[0], config.text_color[1], config.text_color[2]
        ),
    );
    let mut cursor = x;
    for e in &elementen {
        let breedte = module_breedte * f64::from(e.modules);
        if e.is_bar {
            write_ops(
                ops,
                format_args!("{cursor:.2} {pdf_y:.2} {breedte:.2} {h:.2} re\n"),
            );
        }
        cursor += breedte;
    }
    ops.extend_from_slice(b"f\n");
}

#[allow(clippy::too_many_arguments)]
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
        let fs = node_style.font_size.unwrap_or(config.default_font_size);
        // XFA spec: margin_left_pt determines left padding for text, not config.text_padding
        let text_x = x + node_style.margin_left_pt.unwrap_or(0.0);
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

/// Render a single-line static text element (XFA `<draw>` with text content).
///
/// # Font selection (XFA-F5-02)
///
/// Font is resolved in priority order:
/// 1. `config.font_map` keyed by the node's resolved `FontFamily` enum (via
///    `resolve_font_ref`). This maps XFA typeface names to PDF `/XFA_Fn` refs
///    built by `font_bridge` during the flatten pipeline.
/// 2. Fallback to the globally configured `config.default_font` (e.g.
///    `"Helvetica"` for built-in Type1 fonts).
///
/// Font size comes from `node_style.font_size` with `config.default_font_size`
/// as a fallback.
///
/// # Character / word spacing
///
/// Character and word spacing operators (`Tc`, `Tw`) are NOT emitted by this
/// function.  XFA does not expose character/word spacing in its data model, so
/// the PDF default (0 pt spacing) is used.  Spacing is effectively controlled
/// by the font metrics and layout engine word-wrapping instead.
///
/// # Line wrapping
///
/// Single-line text nodes are never wrapped by the render bridge.  All
/// word-wrapping decisions are made in the layout engine (`xfa-layout-engine`)
/// before the `LayoutDom` is produced.  Multi-line output arrives as
/// `LayoutContent::WrappedText` with pre-computed `lines`.
#[allow(clippy::too_many_arguments)]
fn render_text(
    x: f64,
    pdf_y: f64,
    _w: f64,
    h: f64,
    text: &str,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if text.is_empty() {
        return;
    }
    let fs = node_style.font_size.unwrap_or(config.default_font_size);
    // XFA spec: margin_left_pt determines left padding for text, not config.text_padding
    let p = node_style.margin_left_pt.unwrap_or(0.0);
    let font_family = match node_style.font_family.as_deref() {
        Some(f) if f.contains("Courier") || f.contains("Mono") => FontFamily::Monospace,
        Some(f)
            if f.contains("Helvetica")
                || f.contains("Arial")
                || f.contains("Sans")
                || f.contains("Myriad") =>
        {
            FontFamily::SansSerif
        }
        _ => FontFamily::Serif,
    };
    let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
    let tc = node_style
        .text_color
        .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
        .unwrap_or(config.text_color);
    let metrics = build_font_metrics(fs, font_family, node_style, config);
    let asc_pt = ascender_pt(&metrics, fs);
    let line_h = metrics.line_height_pt();
    let idh_metrics = lookup_font_metrics(node_style, config);
    let encoded = pdf_encode_text(text, idh_metrics);
    let desc_pt =
        if let (Some(desc), Some(upem)) = (metrics.resolved_descender, metrics.resolved_upem) {
            if upem > 0 {
                desc as f64 / upem as f64 * fs
            } else {
                fs * 0.2
            }
        } else {
            fs * 0.2
        };
    let text_y = match node_style.v_align {
        Some(VerticalAlign::Middle) => pdf_y + (h - line_h) / 2.0,
        Some(VerticalAlign::Bottom) => pdf_y + desc_pt,
        _ => pdf_y + h - p - asc_pt,
    };
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            tc[0], tc[1], tc[2], font_ref, fs,
        ),
    );
    emit_synthetic_bold_ops(&config.font_map, node_style, fs, &tc, ops);
    write_ops(
        ops,
        format_args!("{:.2} {:.2} Td\n{} Tj\n", x + p, text_y, encoded),
    );
    reset_synthetic_bold_ops(&config.font_map, node_style, ops);
    ops.extend_from_slice(b"ET\n");
    let text_x = x + p;
    let text_y = pdf_y + p;
    let line_thickness = (fs * 0.05).max(0.5);
    if node_style.underline {
        let underline_y = text_y - desc_pt;
        let text_w = metrics.measure_width(text);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\nET\n",
                line_thickness,
                tc[0],
                tc[1],
                tc[2],
                text_x,
                underline_y,
                text_x + text_w,
                underline_y,
            ),
        );
    }
    if node_style.line_through {
        let mid_y = text_y + fs * 0.5 - asc_pt * 0.1;
        let text_w = metrics.measure_width(text);
        write_ops(
            ops,
            format_args!(
                "BT\n{:.3} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\nET\n",
                line_thickness,
                tc[0],
                tc[1],
                tc[2],
                text_x,
                mid_y,
                text_x + text_w,
                mid_y,
            ),
        );
    }
}

/// Render pre-wrapped multiline text from a `LayoutContent::WrappedText` node.
///
/// # Line-spacing / Td offsets (XFA-F5-02)
///
/// Each line is positioned with a PDF `Td` operator:
/// - **First line**: absolute `Td` (`text_x`, `first_line_pdf_y`) places the
///   baseline at the resolved starting Y coordinate.
/// - **Subsequent lines**: relative `Td` (0, `-line_height`) advances downward
///   by `line_height` (in PDF's coordinate system the Y axis grows upward, so
///   a negative delta moves down).
///
/// `line_height` is taken from `node_style.line_height_pt` if explicitly set,
/// otherwise derived from the resolved font's ascender + descender scaled to
/// the current font size (`font_metrics.line_height_pt()`).
///
/// # Paragraph indentation
///
/// When `first_line_of_para[i]` is `true`, the first character of line `i` is
/// shifted right by `node_style.text_indent_pt`.  All other lines are indented
/// by `pad_left` (margin-left).
///
/// # Character / word spacing
///
/// Neither `Tc` nor `Tw` are emitted.  XFA does not surface these properties.
#[allow(clippy::too_many_arguments)]
fn render_multiline(
    x: f64,
    _pdf_y: f64,
    container_width: f64,
    container_height: f64,
    lines: &[String],
    first_line_of_para: &[bool],
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
    // Insets already applied by render_nodes — x, container_width, and
    // abs_y_xfa are the value area inside margin insets.
    let pad_left = node_style.margin_left_pt.unwrap_or(0.0);
    let pad_right = node_style.margin_right_pt.unwrap_or(0.0);
    let space_above = node_style.space_above_pt.unwrap_or(0.0);
    let text_indent = node_style.text_indent_pt.unwrap_or(0.0);
    let font_metrics = build_font_metrics(font_size, font_family, node_style, config);
    let line_height = node_style
        .line_height_pt
        .unwrap_or_else(|| font_metrics.line_height_pt());
    let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
    let tc = node_style
        .text_color
        .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
        .unwrap_or(config.text_color);
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            tc[0], tc[1], tc[2], font_ref, font_size
        ),
    );
    emit_synthetic_bold_ops(&config.font_map, node_style, font_size, &tc, ops);
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
    let total_text_h = lines.len() as f64 * line_height;
    let first_line_y_xfa = match node_style.v_align {
        Some(VerticalAlign::Middle) => {
            abs_y_xfa
                + space_above
                + (container_height - space_above - total_text_h) / 2.0
                + ascender_pt
        }
        Some(VerticalAlign::Bottom) => abs_y_xfa + container_height - total_text_h + ascender_pt,
        _ => abs_y_xfa + space_above + ascender_pt,
    };
    let first_line_pdf_y = mapper.xfa_to_pdf_y(first_line_y_xfa, 0.0);
    let content_w = (container_width - pad_left - pad_right).max(0.0);
    let idh_metrics = lookup_font_metrics(node_style, config);
    let mut prev_x = x + pad_left;
    for (i, line) in lines.iter().enumerate() {
        let is_para_start = first_line_of_para.get(i).copied().unwrap_or(false);
        let indent_offset = if is_para_start { text_indent } else { 0.0 };
        let line_y = first_line_pdf_y - (i as f64 * line_height);
        let line_w = font_metrics.measure_width(line);
        let text_x = match text_align {
            TextAlign::Center => {
                x + pad_left + indent_offset + ((content_w - indent_offset - line_w) / 2.0).max(0.0)
            }
            TextAlign::Right => x + pad_left + (content_w - line_w).max(0.0),
            _ => x + pad_left + indent_offset,
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
    reset_synthetic_bold_ops(&config.font_map, node_style, ops);
    ops.extend_from_slice(b"ET\n");
}

/// Render multiline rich text with per-span font/color/weight switching.
#[allow(clippy::too_many_arguments)]
fn render_rich_multiline(
    x: f64,
    container_width: f64,
    container_height: f64,
    lines: &[String],
    first_line_of_para: &[bool],
    spans: &[RichTextSpan],
    font_size: f64,
    text_align: TextAlign,
    font_family: FontFamily,
    mapper: &CoordinateMapper,
    abs_y_xfa: f64,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if lines.is_empty() || spans.is_empty() {
        return;
    }
    // Insets already applied by render_nodes — x, container_width, and
    // abs_y_xfa are the value area inside margin insets.
    let pad_left = node_style.margin_left_pt.unwrap_or(0.0);
    let pad_right = node_style.margin_right_pt.unwrap_or(0.0);
    let space_above = node_style.space_above_pt.unwrap_or(0.0);
    let text_indent = node_style.text_indent_pt.unwrap_or(0.0);
    let font_metrics = build_font_metrics(font_size, font_family, node_style, config);
    let line_height = node_style
        .line_height_pt
        .unwrap_or_else(|| font_metrics.line_height_pt());
    let asc_pt = if let (Some(asc), Some(upem)) =
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
    let total_text_h = lines.len() as f64 * line_height;
    let first_line_y_xfa = match node_style.v_align {
        Some(VerticalAlign::Middle) => {
            abs_y_xfa + space_above + (container_height - space_above - total_text_h) / 2.0 + asc_pt
        }
        Some(VerticalAlign::Bottom) => abs_y_xfa + container_height - total_text_h + asc_pt,
        _ => abs_y_xfa + space_above + asc_pt,
    };
    let first_line_pdf_y = mapper.xfa_to_pdf_y(first_line_y_xfa, 0.0);
    let content_w = (container_width - pad_left - pad_right).max(0.0);
    let line_segments = map_spans_to_lines(spans, lines);

    ops.extend_from_slice(b"BT\n");
    let base_font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
    let base_tc = node_style
        .text_color
        .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
        .unwrap_or(config.text_color);
    let idh_metrics = lookup_font_metrics(node_style, config);

    let mut cur_font_ref = base_font_ref;
    let mut cur_fs = font_size;
    let mut cur_tc = base_tc;
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            cur_tc[0], cur_tc[1], cur_tc[2], cur_font_ref, cur_fs,
        ),
    );
    emit_text_style_ops(node_style, ops);

    let mut prev_x = x + pad_left;
    for (i, line) in lines.iter().enumerate() {
        let is_para_start = first_line_of_para.get(i).copied().unwrap_or(false);
        let indent_offset = if is_para_start { text_indent } else { 0.0 };
        let line_y = first_line_pdf_y - (i as f64 * line_height);
        let line_w = font_metrics.measure_width(line);
        let text_x = match text_align {
            TextAlign::Center => {
                x + pad_left + indent_offset + ((content_w - indent_offset - line_w) / 2.0).max(0.0)
            }
            TextAlign::Right => x + pad_left + (content_w - line_w).max(0.0),
            _ => x + pad_left + indent_offset,
        };
        if i == 0 {
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", text_x, line_y));
        } else {
            let dx = text_x - prev_x;
            write_ops(ops, format_args!("{:.2} {:.2} Td\n", dx, -line_height));
        }
        prev_x = text_x;

        if let Some(segs) = line_segments.get(i) {
            if segs.is_empty() {
                let encoded = pdf_encode_text(line, idh_metrics);
                write_ops(ops, format_args!("{} Tj\n", encoded));
                continue;
            }
            for seg in segs {
                let span = &spans[seg.span_idx];
                let span_family = span
                    .font_family
                    .as_deref()
                    .map(classify_font_family)
                    .unwrap_or(font_family);
                let span_style = span_to_node_style(span, node_style);
                let span_font_ref = resolve_font_ref(&config.font_map, &span_style, span_family);
                let span_fs = span.font_size.unwrap_or(font_size);
                let span_tc = span
                    .text_color
                    .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
                    .unwrap_or(base_tc);

                if span_font_ref != cur_font_ref || (span_fs - cur_fs).abs() > 0.01 {
                    write_ops(ops, format_args!("{} {:.1} Tf\n", span_font_ref, span_fs));
                    cur_font_ref = span_font_ref;
                    cur_fs = span_fs;
                }
                if (span_tc[0] - cur_tc[0]).abs() > 0.001
                    || (span_tc[1] - cur_tc[1]).abs() > 0.001
                    || (span_tc[2] - cur_tc[2]).abs() > 0.001
                {
                    write_ops(
                        ops,
                        format_args!("{:.3} {:.3} {:.3} rg\n", span_tc[0], span_tc[1], span_tc[2]),
                    );
                    cur_tc = span_tc;
                }
                let is_span_bold = span.font_weight.as_deref() == Some("bold");
                let span_has_real_bold =
                    style_uses_real_bold_variant(&config.font_map, &span_style);
                if is_span_bold && !span_has_real_bold {
                    let stroke_w = span_fs * 0.03;
                    write_ops(
                        ops,
                        format_args!(
                            "2 Tr\n{:.4} w\n{:.3} {:.3} {:.3} RG\n",
                            stroke_w, span_tc[0], span_tc[1], span_tc[2],
                        ),
                    );
                }
                let encoded = pdf_encode_text(&seg.text, idh_metrics);
                write_ops(ops, format_args!("{} Tj\n", encoded));
                if is_span_bold && !span_has_real_bold {
                    write_ops(ops, format_args!("0 Tr\n"));
                }
            }
        } else {
            let encoded = pdf_encode_text(line, idh_metrics);
            write_ops(ops, format_args!("{} Tj\n", encoded));
        }
    }
    reset_text_style_ops(node_style, ops);
    ops.extend_from_slice(b"ET\n");
}

fn span_to_node_style(span: &RichTextSpan, base: &FormNodeStyle) -> FormNodeStyle {
    let mut style = base.clone();
    if let Some(ref fam) = span.font_family {
        style.font_family = Some(fam.clone());
    }
    if let Some(ref w) = span.font_weight {
        style.font_weight = Some(w.clone());
    }
    if let Some(ref s) = span.font_style {
        style.font_style = Some(s.clone());
    }
    style
}

fn classify_font_family(name: &str) -> FontFamily {
    if name.contains("Courier") || name.contains("Mono") {
        FontFamily::Monospace
    } else if name.contains("Helvetica")
        || name.contains("Arial")
        || name.contains("Sans")
        || name.contains("Myriad")
    {
        FontFamily::SansSerif
    } else {
        FontFamily::Serif
    }
}

struct LineSpanSegment {
    text: String,
    span_idx: usize,
}

fn leading_whitespace_len(s: &str) -> usize {
    s.char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

fn map_spans_to_lines(spans: &[RichTextSpan], lines: &[String]) -> Vec<Vec<LineSpanSegment>> {
    let mut result = Vec::with_capacity(lines.len());
    let mut span_idx = 0_usize;
    let mut span_off = 0_usize;

    for line in lines {
        while span_idx < spans.len() {
            if spans[span_idx].text == "\n" || span_off >= spans[span_idx].text.len() {
                span_idx += 1;
                span_off = 0;
            } else {
                break;
            }
        }

        let mut segs: Vec<LineSpanSegment> = Vec::new();
        let mut line_pos = 0_usize;

        while line_pos < line.len() && span_idx < spans.len() {
            let span = &spans[span_idx];
            if span.text == "\n" {
                span_idx += 1;
                span_off = 0;
                continue;
            }
            let span_rest = &span.text[span_off..];
            let line_rest = &line[line_pos..];

            let common = line_rest
                .chars()
                .zip(span_rest.chars())
                .take_while(|(a, b)| a == b || (a.is_whitespace() && b.is_whitespace()))
                .count();

            if common > 0 {
                let common_str: String = line_rest.chars().take(common).collect();
                let common_line_byte_len = common_str.len();
                let common_span_byte_len: usize =
                    span_rest.chars().take(common).map(char::len_utf8).sum();
                segs.push(LineSpanSegment {
                    text: common_str,
                    span_idx,
                });
                line_pos += common_line_byte_len;
                span_off += common_span_byte_len;
                if span_off >= span.text.len() {
                    span_idx += 1;
                    span_off = 0;
                }
            } else {
                let span_skip = leading_whitespace_len(span_rest);
                if span_skip > 0 {
                    span_off += span_skip;
                    if span_off >= span.text.len() {
                        span_idx += 1;
                        span_off = 0;
                    }
                    continue;
                }

                let line_skip = leading_whitespace_len(line_rest);
                if line_skip > 0 {
                    segs.push(LineSpanSegment {
                        text: line_rest[..line_skip].to_string(),
                        span_idx,
                    });
                    line_pos += line_skip;
                } else {
                    segs.push(LineSpanSegment {
                        text: line_rest.to_string(),
                        span_idx: 0,
                    });
                    break;
                }
            }
        }

        result.push(segs);

        while span_idx < spans.len() {
            let span = &spans[span_idx];
            if span.text == "\n" {
                break;
            }
            let rest = &span.text[span_off..];
            let skip = leading_whitespace_len(rest);
            if skip > 0 {
                span_off += skip;
                if span_off >= span.text.len() {
                    span_idx += 1;
                    span_off = 0;
                }
            } else {
                break;
            }
        }
    }

    result
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

fn push_pdf_string_byte(out: &mut String, b: u8) {
    match b {
        b'(' => out.push_str("\\("),
        b')' => out.push_str("\\)"),
        b'\\' => out.push_str("\\\\"),
        0x20..=0x7E => out.push(b as char),
        _ => {
            use std::fmt::Write;
            let _ = write!(out, "\\{:03o}", b);
        }
    }
}

fn pdf_escape_with_simple_encoding(s: &str, unicode_to_code: Option<&HashMap<u16, u8>>) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Some(map) = unicode_to_code {
            let mapped = u16::try_from(c as u32)
                .ok()
                .and_then(|cp| map.get(&cp).copied())
                // Keep WinAnsi fallback for punctuation/shared code points.
                .or_else(|| unicode_to_winansi(c));
            if let Some(b) = mapped {
                push_pdf_string_byte(&mut out, b);
            } else {
                out.push('?');
            }
            continue;
        }

        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\x20'..='\x7e' => out.push(c),
            _ => {
                if let Some(b) = unicode_to_winansi(c) {
                    push_pdf_string_byte(&mut out, b);
                } else {
                    out.push('?');
                }
            }
        }
    }
    out
}

fn pdf_escape(s: &str) -> String {
    pdf_escape_with_simple_encoding(s, None)
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
    let simple_map = metrics.and_then(|m| m.simple_unicode_to_code.as_ref());
    format!("({})", pdf_escape_with_simple_encoding(s, simple_map))
}

/// Look up font metrics for a typeface from the render config.
///
/// Tries the variant key (with weight/posture) first, then falls back to
/// the plain typeface name.
///
/// NOTE: We intentionally return simple-font metrics even when `font_data` is
/// `None`. In that case `pdf_encode_text()` uses simple-font byte encoding
/// fallback (e.g. `/Differences`) rather than Identity-H glyph IDs.
fn lookup_font_metrics<'a>(
    node_style: &FormNodeStyle,
    config: &'a XfaRenderConfig,
) -> Option<&'a FontMetricsData> {
    node_style.font_family.as_ref().and_then(|tf| {
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
}

/// XFA 3.3 §2.6 — render a coordless `<line>` value by spanning the draw's
/// laid-out box (the cell-grid rules in table-style forms). Default-on; opt out
/// with `XFA_DRAW_LINE_SPAN=0|off|false|no` to restore the legacy zero-length
/// point (byte-identical render).
fn draw_line_span_enabled() -> bool {
    !matches!(
        std::env::var("XFA_DRAW_LINE_SPAN").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    )
}

#[allow(clippy::too_many_arguments)]
fn render_draw(
    draw_content: &DrawContent,
    abs_x: f64,
    pdf_y: f64,
    container_w: f64,
    container_h: f64,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    match draw_content {
        DrawContent::Text(text) => {
            if !text.is_empty() {
                let fs = node_style.font_size.unwrap_or(config.default_font_size);
                let font_family = match node_style.font_family.as_deref() {
                    Some(f) if f.contains("Courier") || f.contains("Mono") => FontFamily::Monospace,
                    Some(f)
                        if f.contains("Helvetica")
                            || f.contains("Arial")
                            || f.contains("Sans")
                            || f.contains("Myriad") =>
                    {
                        FontFamily::SansSerif
                    }
                    _ => FontFamily::Serif,
                };
                let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
                let tc = node_style
                    .text_color
                    .map(|(r, g, b)| [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0])
                    .unwrap_or(config.text_color);
                let idh_metrics = lookup_font_metrics(node_style, config);
                let encoded = pdf_encode_text(text, idh_metrics);
                write_ops(
                    ops,
                    format_args!(
                        "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
                        tc[0], tc[1], tc[2], font_ref, fs,
                    ),
                );
                emit_synthetic_bold_ops(&config.font_map, node_style, fs, &tc, ops);
                write_ops(
                    ops,
                    format_args!("{:.2} {:.2} Td\n{} Tj\n", abs_x, pdf_y, encoded),
                );
                reset_synthetic_bold_ops(&config.font_map, node_style, ops);
                ops.extend_from_slice(b"ET\n");
            }
        }
        DrawContent::Line { x1, y1, x2, y2 } => {
            // XFA 3.3 §2.6 — a `<line>` value carries NO x1/y1/x2/y2 attributes:
            // the line spans the draw's content box corner-to-corner per its
            // `slope` (default `\`, top-left→bottom-right). The parser therefore
            // yields `Line{0,0,0,0}` for every authored grid rule (the `FFLineN`
            // draws in gov forms), which the legacy path below strokes as a
            // zero-length point with butt caps => the whole cell grid vanishes.
            // When the span flag is on and the coords are absent, substitute the
            // laid-out box extent; for the axis-aligned h=0 / w=0 grid lines
            // this is exactly the horizontal / vertical rule. Stroke weight and
            // colour come from the line's `<edge>` (parsed into the node style,
            // mirroring the rectangle path). Opt out with
            // XFA_DRAW_LINE_SPAN=0|off|false|no to restore the byte-identical
            // legacy point.
            let coordless = *x1 == 0.0 && *y1 == 0.0 && *x2 == 0.0 && *y2 == 0.0;
            let span =
                draw_line_span_enabled() && coordless && (container_w > 0.0 || container_h > 0.0);
            if span {
                if let Some((r, g, b)) = node_style.border_color {
                    write_ops(
                        ops,
                        format_args!(
                            "{:.4} {:.4} {:.4} RG\n",
                            r as f64 / 255.0,
                            g as f64 / 255.0,
                            b as f64 / 255.0
                        ),
                    );
                }
                if let Some(w_pt) = node_style.border_width_pt {
                    write_ops(ops, format_args!("{:.2} w\n", w_pt));
                }
            }
            let (lx1, ly1, lx2, ly2) = if span {
                (0.0, 0.0, container_w, container_h)
            } else {
                (*x1, *y1, *x2, *y2)
            };
            let start_x = abs_x + lx1;
            let start_y = pdf_y + container_h - ly1;
            let end_x = abs_x + lx2;
            let end_y = pdf_y + container_h - ly2;
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
            // Apply border color from <value><rectangle><edge><color>.
            if let Some((r, g, b)) = node_style.border_color {
                write_ops(
                    ops,
                    format_args!(
                        "{:.4} {:.4} {:.4} RG\n",
                        r as f64 / 255.0,
                        g as f64 / 255.0,
                        b as f64 / 255.0
                    ),
                );
            }
            if let Some(w_pt) = node_style.border_width_pt {
                write_ops(ops, format_args!("{:.2} w\n", w_pt));
            }
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

pub(crate) fn unicode_to_winansi(c: char) -> Option<u8> {
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
            runtime_instantiated: false,
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
            display_items: vec![],
            save_items: vec![],
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
            display_items: vec![],
            save_items: vec![],
        }
    }

    fn make_styled_field_kind(
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        value: &str,
        field_kind: FieldKind,
        style: FormNodeStyle,
    ) -> LayoutNode {
        LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(x, y, w, h),
            name: "styled-kind".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style,
            display_items: vec![],
            save_items: vec![],
        }
    }

    fn make_styled_checkbox(
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
            name: "checkbox".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind: FieldKind::Checkbox,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style,
            display_items: vec![],
            save_items: vec![],
        }
    }

    fn make_styled_radio(
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
            name: "radio".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind: FieldKind::Radio,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style,
            display_items: vec![],
            save_items: vec![],
        }
    }

    fn make_styled_button(
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
            name: "button".to_string(),
            content: LayoutContent::Field {
                value: value.to_string(),
                field_kind: FieldKind::Button,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style,
            display_items: vec![],
            save_items: vec![],
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
    fn dropdown_renders_display_item_for_matching_save_value() {
        let node = LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "choice".to_string(),
            content: LayoutContent::Field {
                value: "CA".to_string(),
                field_kind: FieldKind::Dropdown,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec!["California".to_string(), "Nevada".to_string()],
            save_items: vec!["CA".to_string(), "NV".to_string()],
        };

        let s = overlay_str(&make_page(vec![node]));
        assert!(
            s.contains("(California) Tj"),
            "dropdown should render display item: {s}"
        );
        assert!(
            !s.contains("(CA) Tj"),
            "dropdown should not render raw save value: {s}"
        );
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

    fn styled_overlay_str_with_config(node: LayoutNode, config: XfaRenderConfig) -> String {
        let o = generate_page_overlay(&make_page(vec![node]), &config).unwrap();
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
    fn button_default_border_radius_is_zero() {
        let s = styled_overlay_str(make_styled_button(
            10.0,
            10.0,
            100.0,
            20.0,
            "Click",
            FormNodeStyle::default(),
        ));
        assert!(
            !s.contains(" c\n"),
            "default button border radius should stay square: {s}"
        );
    }

    #[test]
    fn button_with_caption_renders_even_when_value_is_empty() {
        let style = FormNodeStyle {
            caption_text: Some("Click".to_string()),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_button(10.0, 10.0, 100.0, 20.0, "", style));
        assert!(
            s.contains("(Click) Tj"),
            "button caption should render as label: {s}"
        );
    }

    #[test]
    fn caption_uses_its_own_font_not_the_field_font() {
        // Field font = Times New Roman (serif); caption font = Arial (sans).
        // The caption label must bind the Arial resource, otherwise it renders
        // serif (the dominant residual defect family on dense gov forms).
        let mk = || {
            let mut cfg = XfaRenderConfig::default();
            let mut fm = HashMap::new();
            fm.insert("Times New Roman".to_string(), "/Fserif".to_string());
            fm.insert("Arial".to_string(), "/Fsans".to_string());
            cfg.font_map = Arc::new(fm);
            let style = FormNodeStyle {
                font_family: Some("Times New Roman".to_string()),
                caption_text: Some("Name".to_string()),
                caption_placement: Some("top".to_string()),
                caption_reserve: Some(12.0),
                caption_font_family: Some("Arial".to_string()),
                ..Default::default()
            };
            (make_styled_field(10.0, 10.0, 200.0, 30.0, "", style), cfg)
        };
        // resource of the Tf governing the "(Name)" caption draw
        let font_of_name = |s: &str| -> String {
            let i = s.find("(Name) Tj").expect("caption must render");
            let pre = &s[..i];
            let tf = pre.rfind(" Tf").expect("a Tf before the caption");
            pre[..tf]
                .rsplit('/')
                .next()
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .to_string()
        };

        // Default-ON: caption binds the Arial (sans) resource.
        std::env::remove_var("XFA_CAPTION_OWN_FONT");
        let (node, cfg) = mk();
        assert_eq!(
            font_of_name(&styled_overlay_str_with_config(node, cfg)),
            "Fsans",
            "caption must render in its own Arial face"
        );

        // Opt-out restores the legacy field-font behaviour (serif).
        std::env::set_var("XFA_CAPTION_OWN_FONT", "0");
        let (node, cfg) = mk();
        assert_eq!(
            font_of_name(&styled_overlay_str_with_config(node, cfg)),
            "Fserif",
            "opt-out must restore the field font for the caption"
        );
        std::env::remove_var("XFA_CAPTION_OWN_FONT");
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
    fn field_per_edge_widths_render_without_uniform_border_width() {
        let style = FormNodeStyle {
            border_widths: Some([1.0, 2.0, 1.0, 3.0]),
            border_edges: [false, true, false, true],
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 10.0, 100.0, 20.0, "", style));
        assert!(s.contains("2.00 w"), "right edge width should be used: {s}");
        assert!(s.contains("3.00 w"), "left edge width should be used: {s}");
        assert!(
            s.contains("110.00 762.00 m 110.00 782.00 l S"),
            "right edge should render even without border_width_pt: {s}"
        );
        assert!(
            s.contains("10.00 762.00 m 10.00 782.00 l S"),
            "left edge should render even without border_width_pt: {s}"
        );
    }

    #[test]
    fn container_per_edge_widths_render_without_uniform_border_width() {
        let node = LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "box".to_string(),
            content: LayoutContent::None,
            children: vec![],
            style: FormNodeStyle {
                border_widths: Some([1.0, 2.0, 1.0, 3.0]),
                border_edges: [false, true, false, true],
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        };
        let s = styled_overlay_str(node);
        assert!(s.contains("2.00 w"), "right edge width should be used: {s}");
        assert!(s.contains("3.00 w"), "left edge width should be used: {s}");
        assert!(
            s.contains("110.00 762.00 m 110.00 782.00 l S"),
            "right edge should render for non-field nodes: {s}"
        );
        assert!(
            s.contains("10.00 762.00 m 10.00 782.00 l S"),
            "left edge should render for non-field nodes: {s}"
        );
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
    fn top_caption_renders_after_field_fill() {
        let style = FormNodeStyle {
            caption_text: Some("PROJECT INFORMATION/NAME".to_string()),
            caption_placement: Some("top".to_string()),
            caption_reserve: Some(12.0),
            bg_color: Some((12, 34, 56)),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 100.0, 200.0, 30.0, "", style));
        let fill_idx = s
            .find("0.047 0.133 0.220 rg")
            .expect("explicit field fill should be present");
        let caption_idx = s
            .find("(PROJECT INFORMATION/NAME) Tj")
            .expect("caption text should render");
        assert!(
            caption_idx > fill_idx,
            "caption should render after the field fill so it stays visible: {s}"
        );
    }

    #[test]
    fn left_caption_stays_in_pre_body_render_path() {
        let style = FormNodeStyle {
            caption_text: Some("Field 1".to_string()),
            caption_placement: Some("left".to_string()),
            bg_color: Some((12, 34, 56)),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(10.0, 100.0, 200.0, 30.0, "", style));
        let caption_idx = s.find("(Field 1) Tj").expect("caption text should render");
        let fill_idx = s
            .find("0.047 0.133 0.220 rg")
            .expect("explicit field fill should be present");
        assert!(
            caption_idx < fill_idx,
            "left captions should keep the legacy pre-body ordering: {s}"
        );
    }

    #[test]
    fn left_caption_without_explicit_reserve_shifts_field_body() {
        let style = FormNodeStyle {
            caption_text: Some("Field 1".to_string()),
            caption_placement: Some("left".to_string()),
            bg_color: Some((12, 34, 56)),
            ..Default::default()
        };
        let config = XfaRenderConfig::default();
        let reserve = effective_caption_reserve(&style, 10.0, FontFamily::Serif, &config);
        let metrics = build_font_metrics(10.0, FontFamily::Serif, &style, &config);
        assert!(
            (reserve - metrics.measure_width("Field 1")).abs() < 0.01,
            "auto reserve should match caption width for simple left captions"
        );

        let s = styled_overlay_str(make_styled_field(10.0, 100.0, 200.0, 30.0, "", style));
        let mapper = CoordinateMapper::new(792.0, 612.0);
        let expected_fill = format!(
            "{:.2} {:.2} {:.2} 30.00 re",
            10.0 + reserve,
            mapper.xfa_to_pdf_y(100.0, 30.0),
            200.0 - reserve
        );
        assert!(
            s.contains(&expected_fill),
            "field body should be shifted right by the caption reserve: {s}"
        );
    }

    #[test]
    fn button_caption_does_not_shrink_button_body() {
        let style = FormNodeStyle {
            caption_text: Some("Click".to_string()),
            caption_placement: Some("left".to_string()),
            bg_color: Some((12, 34, 56)),
            ..Default::default()
        };
        let config = XfaRenderConfig::default();
        let reserve = effective_caption_reserve(&style, 10.0, FontFamily::Serif, &config);
        assert!(
            reserve > 0.0,
            "button caption should still have measurable text"
        );

        let s = styled_overlay_str(make_styled_button(10.0, 100.0, 200.0, 30.0, "", style));
        let mapper = CoordinateMapper::new(792.0, 612.0);
        let expected_fill = format!(
            "{:.2} {:.2} 200.00 30.00 re",
            10.0,
            mapper.xfa_to_pdf_y(100.0, 30.0)
        );
        assert!(
            s.contains(&expected_fill),
            "button body should keep the full field width because its caption is rendered internally: {s}"
        );
    }

    #[test]
    fn top_caption_uses_full_inner_rect_height() {
        let style = FormNodeStyle {
            caption_text: Some("TOP CAPTION".to_string()),
            caption_placement: Some("top".to_string()),
            caption_reserve: Some(12.0),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(
            10.0,
            100.0,
            200.0,
            30.0,
            "",
            style.clone(),
        ));
        let config = XfaRenderConfig::default();
        let metrics = build_font_metrics(10.0, FontFamily::Serif, &style, &config);
        let asc_pt = ascender_pt(&metrics, 10.0);
        let mapper = CoordinateMapper::new(792.0, 612.0);
        let inner_pdf_y = mapper.xfa_to_pdf_y(100.0, 30.0);
        let expected = format!(
            "{:.2} {:.2} Td\n(TOP CAPTION) Tj",
            10.0,
            inner_pdf_y + 30.0 - asc_pt
        );
        assert!(
            s.contains(&expected),
            "top caption should be positioned against the full inner rect, not the value area: {s}"
        );
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
    fn text_field_without_explicit_fill_has_no_default_background() {
        let s = styled_overlay_str(make_styled_field(
            10.0,
            10.0,
            100.0,
            20.0,
            "Hi",
            FormNodeStyle::default(),
        ));
        assert!(
            !s.contains("0.949 0.949 0.949 rg"),
            "flatten output should not synthesize an interactive default field fill: {s}"
        );
    }

    #[test]
    fn numeric_field_without_explicit_fill_has_no_default_background() {
        let s = styled_overlay_str(make_styled_field_kind(
            10.0,
            10.0,
            100.0,
            20.0,
            "42",
            FieldKind::NumericEdit,
            FormNodeStyle::default(),
        ));
        assert!(
            !s.contains("0.949 0.949 0.949 rg"),
            "numeric fields should also require explicit template fill to paint a background: {s}"
        );
    }

    #[test]
    fn password_field_masks_plaintext_value() {
        let s = styled_overlay_str(make_styled_field_kind(
            10.0,
            10.0,
            100.0,
            20.0,
            "secret",
            FieldKind::PasswordEdit,
            FormNodeStyle::default(),
        ));
        assert!(
            !s.contains("(secret) Tj"),
            "password fields must not emit plaintext into the content stream: {s}"
        );
        assert!(
            s.contains("(\\225\\225\\225\\225\\225\\225) Tj"),
            "password fields should render bullet masking instead of plaintext: {s}"
        );
    }

    #[test]
    fn explicit_white_field_background_is_preserved() {
        let s = styled_overlay_str(make_styled_field(
            10.0,
            10.0,
            100.0,
            20.0,
            "Hi",
            FormNodeStyle {
                bg_color: Some((255, 255, 255)),
                ..Default::default()
            },
        ));
        assert!(
            s.contains("1.000 1.000 1.000 rg"),
            "explicit white field fills should stay white: {s}"
        );
    }

    #[test]
    fn checkbox_does_not_use_edit_field_default_background() {
        let s = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "0",
            FormNodeStyle {
                border_width_pt: Some(0.25),
                ..Default::default()
            },
        ));
        assert!(
            !s.contains("0.949 0.949 0.949 rg"),
            "non-edit widgets should not inherit the text field gray fill: {s}"
        );
    }

    #[test]
    fn checkbox_explicit_background_fill_is_rendered() {
        let s = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "0",
            FormNodeStyle {
                bg_color: Some((255, 255, 255)),
                ..Default::default()
            },
        ));
        assert!(
            s.contains("1.000 1.000 1.000 rg"),
            "checkbox fill color should be emitted when bg_color is present: {s}"
        );
        assert!(
            s.contains("20.00 20.00 re\nf"),
            "checkbox background should be painted as a filled rectangle before the border: {s}"
        );
    }

    #[test]
    fn radio_explicit_background_fill_is_rendered() {
        let s = styled_overlay_str(make_styled_radio(
            10.0,
            10.0,
            20.0,
            20.0,
            "N",
            FormNodeStyle {
                bg_color: Some((255, 255, 255)),
                check_button_on_value: Some("Y".to_string()),
                check_button_off_value: Some("N".to_string()),
                ..Default::default()
            },
        ));
        assert!(
            s.contains("1.000 1.000 1.000 rg"),
            "radio fill color should be emitted when bg_color is present: {s}"
        );
        assert!(
            s.contains(" c\n") && s.contains("\nf\n"),
            "radio background should be painted as a filled circle path before the border: {s}"
        );
    }

    #[test]
    fn checkbox_ignores_global_background_without_explicit_fill() {
        let config = XfaRenderConfig {
            background_color: Some([0.949, 0.949, 0.949]),
            ..Default::default()
        };
        let s = styled_overlay_str_with_config(
            make_styled_checkbox(10.0, 10.0, 20.0, 20.0, "0", FormNodeStyle::default()),
            config,
        );
        assert!(
            !s.contains("0.949 0.949 0.949 rg"),
            "checkbox should only fill from explicit style.bg_color: {s}"
        );
    }

    #[test]
    fn radio_ignores_global_background_without_explicit_fill() {
        let config = XfaRenderConfig {
            background_color: Some([0.949, 0.949, 0.949]),
            ..Default::default()
        };
        let s = styled_overlay_str_with_config(
            make_styled_radio(
                10.0,
                10.0,
                20.0,
                20.0,
                "N",
                FormNodeStyle {
                    check_button_on_value: Some("Y".to_string()),
                    check_button_off_value: Some("N".to_string()),
                    ..Default::default()
                },
            ),
            config,
        );
        assert!(
            !s.contains("0.949 0.949 0.949 rg"),
            "radio should only fill from explicit style.bg_color: {s}"
        );
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
            simple_unicode_to_code: None,
        };
        // Without font_data, should fall back to WinAnsi
        let encoded = pdf_encode_text("AB", Some(&metrics));
        assert_eq!(encoded, "(AB)");
    }

    #[test]
    fn pdf_encode_text_simple_encoding_fallback() {
        // Simulate a simple-font custom encoding that maps U+0163 (ţ) to byte 0x80.
        let mut custom_map = HashMap::new();
        custom_map.insert(0x0163, 0x80);
        let metrics = FontMetricsData {
            widths: vec![500; 256],
            upem: 1000,
            ascender: 800,
            descender: -200,
            font_data: None,
            face_index: 0,
            simple_unicode_to_code: Some(custom_map),
        };
        let encoded = pdf_encode_text("ţ", Some(&metrics));
        assert_eq!(encoded, "(\\200)");
    }

    #[test]
    fn rich_text_span_mapping_preserves_space_after_bold_label() {
        let spans = vec![
            RichTextSpan {
                text: "Instructions:".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("bold".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
            RichTextSpan {
                text: "This form is for your use.".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("normal".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
        ];
        let lines = vec!["Instructions: This form is for your use.".to_string()];

        let mapped = map_spans_to_lines(&spans, &lines);

        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].len(), 3);
        assert_eq!(mapped[0][0].text, "Instructions:");
        assert_eq!(mapped[0][0].span_idx, 0);
        assert_eq!(mapped[0][1].text, " ");
        assert_eq!(mapped[0][1].span_idx, 1);
        assert_eq!(mapped[0][2].text, "This form is for your use.");
        assert_eq!(mapped[0][2].span_idx, 1);
    }

    #[test]
    fn rich_text_span_mapping_treats_nbsp_spaceruns_as_normal_spaces() {
        let spans = vec![
            RichTextSpan {
                text: "Instructions:".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("bold".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
            RichTextSpan {
                text: "This form is for your use.".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("normal".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
            RichTextSpan {
                text: "\u{00A0}\u{00A0}".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("normal".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
            RichTextSpan {
                text: "Mail in at least 14 days before".to_string(),
                font_size: None,
                font_family: None,
                font_weight: Some("normal".to_string()),
                font_style: None,
                text_color: None,
                underline: false,
                line_through: false,
            },
        ];
        let lines = vec![
            "Instructions: This form is for your use.".to_string(),
            "Mail in at least 14 days before".to_string(),
        ];

        let mapped = map_spans_to_lines(&spans, &lines);

        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0][0].span_idx, 0);
        assert_eq!(mapped[0][2].span_idx, 1);
        assert_eq!(mapped[1].len(), 1);
        assert_eq!(mapped[1][0].text, "Mail in at least 14 days before");
        assert_eq!(mapped[1][0].span_idx, 3);
    }

    #[test]
    fn real_bold_font_variant_skips_synthetic_bold_stroke() {
        let mut config = XfaRenderConfig::default();
        let mut fm = HashMap::new();
        fm.insert("Arial_Bold_Normal".to_string(), "/XFA_Fbold".to_string());
        config.font_map = Arc::new(fm);

        let s = styled_overlay_str_with_config(
            make_styled_field(
                10.0,
                10.0,
                200.0,
                20.0,
                "Bold",
                FormNodeStyle {
                    font_family: Some("Arial".to_string()),
                    font_weight: Some("bold".to_string()),
                    ..Default::default()
                },
            ),
            config,
        );

        assert!(
            !s.contains("2 Tr"),
            "actual bold variants should not get synthetic stroke bolding: {s}"
        );
        assert!(
            s.contains("/XFA_Fbold 10.0 Tf"),
            "expected real bold resource: {s}"
        );
    }

    #[test]
    fn container_insets_offset_children() {
        // A parent container with leftInset=10, topInset=5 should offset
        // child positions by those amounts during rendering.
        let child = LayoutNode {
            form_node: FormNodeId(1),
            rect: Rect::new(0.0, 0.0, 50.0, 20.0),
            name: "child".to_string(),
            content: LayoutContent::Field {
                value: "Test".to_string(),
                field_kind: FieldKind::Text,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let parent = LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(100.0, 200.0, 200.0, 100.0),
            name: "parent".to_string(),
            content: LayoutContent::None,
            children: vec![child],
            style: FormNodeStyle {
                inset_left_pt: Some(10.0),
                inset_top_pt: Some(5.0),
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![parent]));
        // Child field at (0,0) rendered within parent at (100,200) with leftInset=10.
        // Text x = parent_x + inset_left = 100 + 10 = 110 (no default padding per XFA spec)
        assert!(
            s.contains("110.00"),
            "child x should include parent left inset offset: {s}"
        );
    }

    #[test]
    fn field_insets_reduce_text_wrap_width() {
        // A field with leftInset=8, rightInset=8 on a 100pt-wide box should
        // offset text x by inset_left + pad_left.
        let node = LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(10.0, 10.0, 100.0, 30.0),
            name: "field".to_string(),
            content: LayoutContent::Field {
                value: "Hello".to_string(),
                field_kind: FieldKind::Text,
                font_size: 10.0,
                font_family: FontFamily::Serif,
            },
            children: vec![],
            style: FormNodeStyle {
                inset_left_pt: Some(8.0),
                inset_right_pt: Some(8.0),
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        // Text x = field_x + inset_left = 10 + 8 = 18 (no default padding per XFA spec)
        assert!(
            s.contains("18.00"),
            "text x should include field left inset: {s}"
        );
    }

    #[test]
    fn checkbox_border_width_respects_style() {
        let s = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "0",
            FormNodeStyle {
                border_width_pt: Some(0.25),
                ..Default::default()
            },
        ));
        assert!(
            s.contains("\n0.25 w\n"),
            "checkbox should use styled border width: {s}"
        );
        assert!(
            !s.contains("\n0.50 w\n"),
            "checkbox should not fall back to default 0.5pt border width: {s}"
        );
        assert!(
            !s.contains("\n1.00 w\n"),
            "checkbox should not clamp to 1pt border width: {s}"
        );
    }

    #[test]
    fn container_children_y_offset_includes_inset() {
        let child = LayoutNode {
            form_node: FormNodeId(1),
            rect: Rect::new(0.0, 0.0, 50.0, 20.0),
            name: "child-box".to_string(),
            content: LayoutContent::None,
            children: vec![],
            style: FormNodeStyle {
                border_width_pt: Some(1.0),
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        };
        let parent = LayoutNode {
            form_node: FormNodeId(0),
            rect: Rect::new(100.0, 200.0, 200.0, 100.0),
            name: "parent".to_string(),
            content: LayoutContent::None,
            children: vec![child],
            style: FormNodeStyle {
                inset_top_pt: Some(10.0),
                ..Default::default()
            },
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![parent]));
        assert!(
            s.contains("100.00 562.00 50.00 20.00 re"),
            "child y should include parent inset_top offset: {s}"
        );
        assert!(
            !s.contains("100.00 572.00 50.00 20.00 re"),
            "child y should no longer ignore parent inset_top offset: {s}"
        );
    }

    #[test]
    fn checkbox_mark_style_controls_rendered_symbol() {
        let default_overlay = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "1",
            FormNodeStyle::default(),
        ));
        let circle_overlay = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "1",
            FormNodeStyle {
                check_button_mark: Some("circle".to_string()),
                ..Default::default()
            },
        ));

        assert!(
            !default_overlay.contains(" c\n"),
            "default checkbox mark should not emit Bezier circle commands: {default_overlay}"
        );
        assert!(
            circle_overlay.contains(" c\n"),
            "circle checkbox mark should emit Bezier circle commands: {circle_overlay}"
        );
    }

    #[test]
    fn unchecked_checkbox_with_empty_value_still_draws_outline() {
        let overlay = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "",
            FormNodeStyle::default(),
        ));

        assert!(
            overlay.contains("10.00 762.00 20.00 20.00 re"),
            "unchecked checkbox should still render its outline: {overlay}"
        );
    }

    #[test]
    fn checkbox_checked_state_uses_template_item_values() {
        let checked_overlay = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "Yes",
            FormNodeStyle {
                check_button_on_value: Some("Yes".to_string()),
                check_button_off_value: Some("No".to_string()),
                ..Default::default()
            },
        ));
        let unchecked_overlay = styled_overlay_str(make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "No",
            FormNodeStyle {
                check_button_on_value: Some("Yes".to_string()),
                check_button_off_value: Some("No".to_string()),
                ..Default::default()
            },
        ));

        assert!(
            checked_overlay.matches(" l\nS\n").count() >= 2,
            "asserted template on-value should render the check/cross mark: {checked_overlay}"
        );
        assert!(
            unchecked_overlay.matches(" l\nS\n").count() < 3,
            "template off-value should not render the asserted mark: {unchecked_overlay}"
        );
    }

    #[test]
    fn radio_explicit_mark_overrides_default_circle() {
        let default_overlay = styled_overlay_str(make_styled_radio(
            10.0,
            10.0,
            20.0,
            20.0,
            "Y",
            FormNodeStyle {
                check_button_on_value: Some("Y".to_string()),
                check_button_off_value: Some("N".to_string()),
                ..Default::default()
            },
        ));
        let cross_overlay = styled_overlay_str(make_styled_radio(
            10.0,
            10.0,
            20.0,
            20.0,
            "Y",
            FormNodeStyle {
                check_button_mark: Some("cross".to_string()),
                check_button_on_value: Some("Y".to_string()),
                check_button_off_value: Some("N".to_string()),
                ..Default::default()
            },
        ));

        assert!(
            default_overlay.matches(" c\n").count() >= 8,
            "default radio should render outer circle plus filled inner circle: {default_overlay}"
        );
        assert!(
            cross_overlay.matches(" l\nS\n").count() >= 2,
            "explicit radio mark should render the requested symbol: {cross_overlay}"
        );
    }

    // ── RenderTree tests (#1104) ──────────────────────────────────────────────

    #[test]
    fn render_tree_page_dimensions() {
        let page = make_page(vec![]);
        let layout = LayoutDom { pages: vec![page] };
        let tree = layout_dom_to_render_tree(&layout, &XfaRenderConfig::default());
        assert_eq!(tree.pages.len(), 1);
        match &tree.pages[0] {
            RenderNode::Page { width, height, .. } => {
                assert!((*width - 612.0).abs() < 0.01, "width should be 612pt");
                assert!((*height - 792.0).abs() < 0.01, "height should be 792pt");
            }
            other => panic!("expected Page node, got {other:?}"),
        }
    }

    #[test]
    fn render_tree_text_node_captured() {
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 20.0, 100.0, 15.0),
            name: "lbl".to_string(),
            content: LayoutContent::Text("Hello tree".to_string()),
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: vec![node],
                runtime_instantiated: false,
            }],
        };
        let tree = layout_dom_to_render_tree(&layout, &XfaRenderConfig::default());
        let debug = tree.to_debug_string();
        assert!(
            debug.contains("Hello tree"),
            "debug string should contain text content: {debug}"
        );
        assert!(
            debug.contains("Text("),
            "debug string should contain Text node: {debug}"
        );
    }

    // ── Text fidelity tests (#1105) ───────────────────────────────────────────

    #[test]
    fn text_node_emits_correct_font_and_size_operators() {
        // A text field with font_size=12 should emit /F1 12.0 Tf (or the default
        // font reference) in the content stream.
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "f".to_string(),
            content: LayoutContent::Field {
                value: "test value".to_string(),
                field_kind: FieldKind::Text,
                font_size: 12.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        // Content stream must contain a Tf operator and the size 12.0
        assert!(
            s.contains("Tf"),
            "should contain Tf font-select operator: {s}"
        );
        assert!(
            s.contains("12.0 Tf"),
            "should use specified font size 12.0: {s}"
        );
        assert!(
            s.contains("(test value) Tj"),
            "should render the value: {s}"
        );
    }

    #[test]
    fn multiline_text_has_correct_td_offsets() {
        // A WrappedText node with two lines should emit two Td position operators.
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 100.0, 40.0),
            name: "ml".to_string(),
            content: LayoutContent::WrappedText {
                lines: vec!["line one".to_string(), "line two".to_string()],
                first_line_of_para: vec![true, false],
                font_size: 10.0,
                text_align: xfa_layout_engine::types::TextAlign::Left,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
                space_above_pt: None,
                space_below_pt: None,
                // Synthetic test node, not produced from a form field.
                from_field: false,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        // Both lines should be rendered.
        assert!(s.contains("(line one) Tj"), "first line missing: {s}");
        assert!(s.contains("(line two) Tj"), "second line missing: {s}");
        // There should be at least two Td operators (one per line).
        let td_count = s.matches(" Td\n").count();
        assert!(td_count >= 2, "expected ≥2 Td operators for two lines: {s}");
    }

    // ── Widget rendering tests (#1106) ────────────────────────────────────────

    #[test]
    fn checkbox_checked_renders_nonempty_stream() {
        let node = make_styled_checkbox(
            10.0,
            10.0,
            20.0,
            20.0,
            "1",
            FormNodeStyle {
                check_button_on_value: Some("1".to_string()),
                check_button_off_value: Some("0".to_string()),
                border_width_pt: Some(0.5),
                ..Default::default()
            },
        );
        let s = overlay_str(&make_page(vec![node]));
        assert!(!s.is_empty(), "checked checkbox overlay must not be empty");
        assert!(s.contains("re"), "checkbox must emit a rectangle: {s}");
    }

    #[test]
    fn radio_selected_renders_nonempty_stream() {
        let node = make_styled_radio(
            10.0,
            10.0,
            20.0,
            20.0,
            "yes",
            FormNodeStyle {
                check_button_on_value: Some("yes".to_string()),
                check_button_off_value: Some("no".to_string()),
                ..Default::default()
            },
        );
        let s = overlay_str(&make_page(vec![node]));
        assert!(!s.is_empty(), "selected radio overlay must not be empty");
        // Selected radio should render a filled inner circle (Bezier 'c' operators).
        assert!(
            s.contains(" c\n"),
            "selected radio must render circular fill: {s}"
        );
    }

    #[test]
    fn dropdown_selected_value_renders_nonempty_stream() {
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "dd".to_string(),
            content: LayoutContent::Field {
                value: "opt1".to_string(),
                field_kind: FieldKind::Dropdown,
                font_size: 10.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec!["Option 1".to_string()],
            save_items: vec!["opt1".to_string()],
        };
        let s = overlay_str(&make_page(vec![node]));
        assert!(!s.is_empty(), "dropdown overlay must not be empty");
        assert!(
            s.contains("(Option 1) Tj"),
            "dropdown must render display label: {s}"
        );
    }

    // ── Field rendering tests (#1107) ─────────────────────────────────────────

    #[test]
    fn text_field_renders_bound_value() {
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "name_field".to_string(),
            content: LayoutContent::Field {
                value: "John Doe".to_string(),
                field_kind: FieldKind::Text,
                font_size: 10.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        assert!(
            s.contains("(John Doe) Tj"),
            "text field must render its bound value: {s}"
        );
    }

    #[test]
    fn numeric_field_formats_value_with_default_pattern() {
        // NumericEdit with no explicit pattern: trailing zeros stripped.
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 80.0, 20.0),
            name: "amount".to_string(),
            content: LayoutContent::Field {
                value: "42.00000000".to_string(),
                field_kind: FieldKind::NumericEdit,
                font_size: 10.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        // format_numeric_default strips trailing zeros → "42"
        assert!(
            s.contains("(42) Tj"),
            "numeric field should render cleaned value: {s}"
        );
    }

    #[test]
    fn date_field_renders_value_with_format_pattern() {
        // DateField: value rendered with the format_pattern if set, else raw.
        // Here we simulate a date field with a raw value and no pattern: must render value as-is.
        let node = LayoutNode {
            form_node: xfa_layout_engine::form::FormNodeId(0),
            rect: xfa_layout_engine::types::Rect::new(10.0, 10.0, 100.0, 20.0),
            name: "date_field".to_string(),
            content: LayoutContent::Field {
                value: "2024-01-15".to_string(),
                field_kind: FieldKind::DateTimePicker,
                font_size: 10.0,
                font_family: xfa_layout_engine::text::FontFamily::Serif,
            },
            children: vec![],
            style: Default::default(),
            display_items: vec![],
            save_items: vec![],
        };
        let s = overlay_str(&make_page(vec![node]));
        assert!(
            s.contains("(2024-01-15) Tj"),
            "date field must render its value: {s}"
        );
    }

    #[test]
    fn draw_line_span_gate_default_on_and_opt_out() {
        // A coordless <line/> (the parser yields Line{0,0,0,0} because XFA
        // <line> values have no x1/y1/x2/y2) inside a horizontal grid box
        // (w=200, h=0) at abs_x=30, pdf_y=50, carrying the authored 6.01pt
        // (≈2.12mm) edge weight in the node style. Default-on must span the box
        // edge-to-edge (30 -> 230) at that weight; opt-out restores the legacy
        // zero-length point at the box origin with no weight emitted.
        let line = DrawContent::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
        };
        let style = FormNodeStyle {
            border_width_pt: Some(6.01),
            border_color: Some((0, 0, 0)),
            ..Default::default()
        };
        let config = XfaRenderConfig::default();

        // Default (flag unset) → spans the 200pt box at the authored weight.
        std::env::remove_var("XFA_DRAW_LINE_SPAN");
        let mut ops_on = Vec::new();
        render_draw(&line, 30.0, 50.0, 200.0, 0.0, &style, &config, &mut ops_on);
        let s_on = String::from_utf8(ops_on).unwrap();
        assert!(s_on.contains("6.01 w\n"), "authored stroke weight:\n{s_on}");
        assert!(
            s_on.contains("30.00 50.00 m\n230.00 50.00 l\nS\n"),
            "horizontal grid rule must span the box (30 -> 230):\n{s_on}"
        );

        // Opt-out → legacy zero-length point at the box origin, no weight.
        std::env::set_var("XFA_DRAW_LINE_SPAN", "0");
        let mut ops_off = Vec::new();
        render_draw(&line, 30.0, 50.0, 200.0, 0.0, &style, &config, &mut ops_off);
        std::env::remove_var("XFA_DRAW_LINE_SPAN");
        let s_off = String::from_utf8(ops_off).unwrap();
        assert!(
            !s_off.contains(" w\n"),
            "opt-out emits no stroke weight:\n{s_off}"
        );
        assert!(
            s_off.contains("30.00 50.00 m\n30.00 50.00 l\nS\n"),
            "opt-out = legacy zero-length point:\n{s_off}"
        );
    }

    /// A render config with borders on or off. Came across from the branch
    /// alongside the barcode work; the call was resolved into the merge and the
    /// definition was not, so the build failed on a name that existed nowhere.
    fn config_with_border(border: bool) -> XfaRenderConfig {
        XfaRenderConfig {
            draw_borders: border,
            border_width: if border { 1.0 } else { 0.0 },
            ..XfaRenderConfig::default()
        }
    }

    #[test]
    fn a_filled_signature_still_draws_its_content() {
        let mut ops = Vec::new();
        render_signature(
            10.0,
            20.0,
            100.0,
            30.0,
            "J. de Winter",
            &FormNodeStyle::default(),
            &config_with_border(true),
            &mut ops,
        );
        assert!(!ops.is_empty(), "een ingevulde handtekening tekende niets");
    }

    // ---- barcode renderen (#147) ----

    #[test]
    fn een_barcodeveld_tekent_balken_en_geen_tekst() {
        let mut ops = Vec::new();
        render_barcode(
            10.0,
            20.0,
            200.0,
            40.0,
            "ABC-123",
            &FormNodeStyle::default(),
            &XfaRenderConfig::default(),
            &mut ops,
        );
        let uit = String::from_utf8_lossy(&ops);
        assert!(!ops.is_empty(), "er is niets getekend");
        assert!(
            uit.contains(" re\n"),
            "geen rechthoeken; er staan geen balken: {uit}"
        );
        assert!(uit.trim_end().ends_with('f'), "het pad wordt niet gevuld");
        assert!(
            !uit.contains(" Tj"),
            "er wordt tekst getekend in plaats van een barcode"
        );
    }

    #[test]
    fn een_onbekend_teken_tekent_niets() {
        // Stil weglaten zou een barcode geven die naar iets anders scant dan er
        // staat. Dat merk je pas met een handscanner bij de klant.
        let mut ops = Vec::new();
        render_barcode(
            10.0,
            20.0,
            200.0,
            40.0,
            "hallo!",
            &FormNodeStyle::default(),
            &XfaRenderConfig::default(),
            &mut ops,
        );
        assert!(
            ops.is_empty(),
            "er is toch iets getekend: {}",
            String::from_utf8_lossy(&ops)
        );
    }

    #[test]
    fn een_te_smal_veld_tekent_niets() {
        // Onder een kwart punt per module is de code niet meer te scannen. Dan
        // is een streepjespatroon tekenen erger dan niets: het ziet eruit als
        // een werkende barcode.
        let mut ops = Vec::new();
        render_barcode(
            10.0,
            20.0,
            2.0,
            40.0,
            "ABC-123",
            &FormNodeStyle::default(),
            &XfaRenderConfig::default(),
            &mut ops,
        );
        assert!(
            ops.is_empty(),
            "een onleesbaar smalle barcode werd toch getekend"
        );
    }

    #[test]
    fn de_balken_passen_binnen_de_veldbreedte() {
        // Zou de schaling misgaan, dan loopt de code buiten zijn veld en over
        // de tekst ernaast heen -- zichtbaar fout, maar niet als je alleen
        // controleert dát er balken staan.
        let mut ops = Vec::new();
        let (x, breedte) = (10.0_f64, 200.0_f64);
        render_barcode(
            x,
            20.0,
            breedte,
            40.0,
            "123",
            &FormNodeStyle::default(),
            &XfaRenderConfig::default(),
            &mut ops,
        );
        let uit = String::from_utf8_lossy(&ops);
        let mut rechtsterand = x;
        for regel in uit.lines().filter(|l| l.ends_with(" re")) {
            let d: Vec<f64> = regel
                .trim_end_matches(" re")
                .split(' ')
                .filter_map(|t| t.parse().ok())
                .collect();
            if d.len() == 4 {
                rechtsterand = rechtsterand.max(d[0] + d[2]);
            }
        }
        assert!(
            rechtsterand <= x + breedte + 0.5,
            "de barcode loopt tot {rechtsterand} en het veld eindigt op {}",
            x + breedte
        );
    }
}
