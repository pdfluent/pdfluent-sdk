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
    pub font_map: HashMap<String, String>,
    /// Resolved font metrics per typeface.
    pub font_metrics_data: HashMap<String, FontMetricsData>,
    /// CheckButton mark style (check, circle, cross, diamond, square, star).
    pub check_button_mark: Option<String>,
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

// XFA Template defines white as the default color for an explicit solid
// <Fill>, but Acrobat/pdfRest still paint editable widgets with a light-gray
// UI background when the template omits a field fill. Limit this compatibility
// default to edit-style field widgets only. (#GATE-25)
const ADOBE_DEFAULT_EDIT_FIELD_BACKGROUND: [f64; 3] =
    [242.0 / 255.0, 242.0 / 255.0, 242.0 / 255.0];

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
            text_padding: xfa_layout_engine::types::DEFAULT_TEXT_PADDING,
            font_map: HashMap::new(),
            font_metrics_data: HashMap::new(),
            check_button_mark: None,
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

fn default_edit_field_background(
    field_kind: FieldKind,
    config: &XfaRenderConfig,
) -> Option<[f64; 3]> {
    config.background_color.or_else(|| {
        matches!(
            field_kind,
            FieldKind::Text
                | FieldKind::NumericEdit
                | FieldKind::PasswordEdit
                | FieldKind::DateTimePicker
        )
        .then_some(ADOBE_DEFAULT_EDIT_FIELD_BACKGROUND)
    })
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

        // Caption/value offset computed from inner rect (after margin insets).
        let (cap_dx, cap_dy, val_w, val_h) = caption_value_offset(&node.style, inner_w, inner_h);
        let val_x = abs_x + inset_l + cap_dx;
        let val_y_offset = inset_t + cap_dy;
        let val_pdf_y = mapper.xfa_to_pdf_y(abs_y + val_y_offset, val_h);

        if !matches!(node.content, LayoutContent::Field { .. }) {
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
        write_ops(
            ops,
            format_args!("{:.2} {:.2} {:.2} {:.2} re W n\n", abs_x, pdf_y, w, h),
        );

        let is_bold = node.style.font_weight.as_deref() == Some("bold");

        // Render caption for any node that has caption_text in its style.
        // For Button fields, skip the external caption — the caption text is
        // used as the button label rendered inside the button body instead.
        let is_button = matches!(
            &node.content,
            LayoutContent::Field {
                field_kind: FieldKind::Button,
                ..
            }
        );
        if node.style.caption_text.is_some() && !is_button {
            let (cap_fs, cap_ff) = match &node.content {
                LayoutContent::Field {
                    font_size,
                    font_family,
                    ..
                } => (*font_size, *font_family),
                LayoutContent::WrappedText {
                    font_size,
                    font_family,
                    ..
                } => (*font_size, *font_family),
                _ => (
                    node.style.font_size.unwrap_or(config.default_font_size),
                    FontFamily::SansSerif,
                ),
            };
            render_caption(
                abs_x + inset_l,
                mapper.xfa_to_pdf_y(abs_y + inset_t, inner_h),
                val_w,
                val_h,
                cap_fs,
                cap_ff,
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
            } => match field_kind {
                FieldKind::Checkbox => {
                    render_checkbox(val_x, val_pdf_y, val_w, val_h, value, &node_config, ops)
                }
                FieldKind::Radio => {
                    render_radio(val_x, val_pdf_y, val_w, val_h, value, &node_config, ops)
                }
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
                ),
                FieldKind::Button => {
                    // XFA buttons use their <caption> as the button label.
                    // When the field value is empty (typical for buttons), fall
                    // back to the caption text so the label renders centered
                    // inside the button body.
                    let label = if value.is_empty() {
                        node.style.caption_text.as_deref().unwrap_or("")
                    } else {
                        value
                    };
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
                _ => render_field(
                    val_x,
                    val_pdf_y,
                    val_w,
                    val_h,
                    *field_kind,
                    value,
                    *font_size,
                    *font_family,
                    &node.style,
                    &node_config,
                    ops,
                ),
            },
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
            // NOTE: inset_* is NOT added here because the layout engine already
            // positions children at box_model.y which is relative to the content
            // area (inside margins). Adding inset_top_pt would double-offset.
            let child_origin_x = abs_x + node.style.inset_left_pt.unwrap_or(0.0);
            let child_origin_y = abs_y;
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
            tl[0], tl[1], tl[2], x, y + h, x + w, y + h,
        ),
    );
    // Left edge (tl color)
    write_ops(
        ops,
        format_args!(
            "{:.2} {:.2} m {:.2} {:.2} l S\n",
            x, y + h, x, y,
        ),
    );
    // Bottom edge (br color)
    write_ops(
        ops,
        format_args!(
            "{:.3} {:.3} {:.3} RG\n{:.2} {:.2} m {:.2} {:.2} l S\n",
            br[0], br[1], br[2], x, y, x + w, y,
        ),
    );
    // Right edge (br color)
    write_ops(
        ops,
        format_args!(
            "{:.2} {:.2} m {:.2} {:.2} l S\n",
            x + w, y, x + w, y + h,
        ),
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

/// Returns true if the font reference indicates a bold variant.
/// PDF font names typically include "Bold" in the name for bold fonts.
fn font_ref_is_bold(font_ref: &str) -> bool {
    font_ref.to_uppercase().contains("BOLD")
}

/// Emit synthetic bold operators: fill+stroke rendering mode with thin stroke.
/// Uses text rendering mode 2 (fill then stroke) to simulate bold weight when
/// the actual bold font variant is unavailable.
fn emit_synthetic_bold_ops(
    node_style: &FormNodeStyle,
    font_ref: &str,
    font_size: f64,
    text_color: &[f64; 3],
    ops: &mut Vec<u8>,
) {
    if is_bold_style(node_style) && !font_ref_is_bold(font_ref) {
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
fn reset_synthetic_bold_ops(node_style: &FormNodeStyle, font_ref: &str, ops: &mut Vec<u8>) {
    if is_bold_style(node_style) && !font_ref_is_bold(font_ref) {
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

/// Compute the offset and size of the value area within a field that has a caption.
///
/// Returns (dx, dy, value_width, value_height) where dx/dy are the offsets from
/// the field origin to the value area origin.
fn caption_value_offset(style: &FormNodeStyle, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let reserve = style.caption_reserve.unwrap_or(0.0);
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
/// relative to the field box. Called before the field-specific renderer so that
/// captions appear for Dropdown, Checkbox, Radio, Button, Signature, and Text.
#[allow(clippy::too_many_arguments)]
fn render_caption(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
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
    let caption_reserve = node_style.caption_reserve.unwrap_or(0.0);
    let fs = if font_size > 0.0 {
        font_size
    } else {
        config.default_font_size
    };
    let metrics = build_font_metrics(fs, font_family, node_style, config);
    let font_ref = resolve_font_ref(&config.font_map, node_style, font_family);
    let idh_metrics = lookup_font_metrics(node_style, config);

    let (text_x, text_y) = match caption_placement {
        "right" => {
            // Caption in the right portion of the field
            let cap_x = x + w - caption_reserve;
            let asc_pt = ascender_pt(&metrics, fs);
            (cap_x, pdf_y + h - asc_pt)
        }
        "top" => {
            // Caption above the value area (within the field's total height)
            let asc_pt = ascender_pt(&metrics, fs);
            let text_y = pdf_y + h - asc_pt;
            (x, text_y)
        }
        "bottom" => {
            // Caption below the value area (within the field's total height)
            let asc_pt = ascender_pt(&metrics, fs);
            let text_y = pdf_y + caption_reserve - asc_pt;
            (x, text_y)
        }
        _ => {
            // "left" (default): caption in the left portion of the field
            let asc_pt = ascender_pt(&metrics, fs);
            (x, pdf_y + h - asc_pt)
        }
    };

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
        format_args!("{:.2} {:.2} Td\n{} Tj\n", text_x, text_y, encoded),
    );
    reset_text_style_ops(node_style, ops);
    ops.extend_from_slice(b"ET\n");
}

#[allow(clippy::too_many_arguments)]
fn render_field(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    field_kind: FieldKind,
    value: &str,
    font_size: f64,
    font_family: FontFamily,
    node_style: &FormNodeStyle,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    let border_radius = node_style.border_radius_pt.unwrap_or(0.0);
    let border_style = node_style.border_style.as_deref();

    if let Some(bg) = default_edit_field_background(field_kind, config) {
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
            let per_edge = node_style
                .border_colors
                .map(|cs| {
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
                    ops, x, pdf_y, w, h, &edges, None, None, config.border_width,
                );
            }
            reset_border_dash(ops, border_style);
        }
    }
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
            emit_synthetic_bold_ops(node_style, font_ref, fs, &config.text_color, ops);
            emit_text_style_ops(node_style, ops);
            write_ops(
                ops,
                format_args!("{:.2} {:.2} Td\n{} Tj\n", x + pad_left, text_y, encoded),
            );
            reset_text_style_ops(node_style, ops);
            reset_synthetic_bold_ops(node_style, font_ref, ops);
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
            emit_synthetic_bold_ops(node_style, font_ref, fs, &config.text_color, ops);
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
            reset_synthetic_bold_ops(node_style, font_ref, ops);
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

fn render_checkbox(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if value.is_empty() {
        return;
    }
    let bw = config.border_width;
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
        let mark = config.check_button_mark.as_deref().unwrap_or("cross");
        let m = w.min(h) * 0.15;
        let color = config.text_color;
        draw_check_mark(mark, x, pdf_y, w, h, m, color, ops);
    }
    write_ops(ops, format_args!("Q\n"));
}

fn render_radio(
    x: f64,
    pdf_y: f64,
    w: f64,
    h: f64,
    value: &str,
    config: &XfaRenderConfig,
    ops: &mut Vec<u8>,
) {
    if value.is_empty() {
        return;
    }
    let bw = config.border_width;
    let cx = x + w / 2.0;
    let cy = pdf_y + h / 2.0;
    let r = w.min(h) / 2.0;

    // Draw circle using 4 Bezier curves (standard circle approximation).
    let k = 0.5523; // kappa ≈ 4*(√2-1)/3
    let kx = r * k;
    let ky = r * k;
    write_ops(
        ops,
        format_args!(
            "q\n{:.2} w\n{:.3} {:.3} {:.3} RG\n",
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

    let checked = !value.is_empty()
        && !value.eq_ignore_ascii_case("0")
        && !value.eq_ignore_ascii_case("off")
        && !value.eq_ignore_ascii_case("false");
    if checked {
        // Draw filled inner circle (bullet).
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
    }
    write_ops(ops, format_args!("Q\n"));
}

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
                config.text_color[0], config.text_color[1], config.text_color[2], font_ref, fs,
            ),
        );
        emit_synthetic_bold_ops(node_style, font_ref, fs, &config.text_color, ops);
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!("{:.2} {:.2} Td\n{} Tj\n", x + 2.0, v_offset, encoded),
        );
        reset_text_style_ops(node_style, ops);
        reset_synthetic_bold_ops(node_style, font_ref, ops);
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
            (fill_color[0] * 0.6),
            (fill_color[1] * 0.6),
            (fill_color[2] * 0.6),
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
        emit_synthetic_bold_ops(node_style, font_ref, fs, &tc, ops);
        emit_text_style_ops(node_style, ops);
        write_ops(
            ops,
            format_args!("{:.2} {:.2} Td\n{} Tj\n", text_x, v_offset, encoded),
        );
        reset_text_style_ops(node_style, ops);
        reset_synthetic_bold_ops(node_style, font_ref, ops);
        ops.extend_from_slice(b"ET\n");
    }
    write_ops(ops, format_args!("Q\n"));
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
    let text_y = match node_style.v_align {
        Some(VerticalAlign::Middle) => pdf_y + (h - line_h) / 2.0,
        Some(VerticalAlign::Bottom) => pdf_y + p,
        _ => pdf_y + h - p - asc_pt,
    };
    write_ops(
        ops,
        format_args!(
            "BT\n{:.3} {:.3} {:.3} rg\n{} {:.1} Tf\n",
            tc[0], tc[1], tc[2], font_ref, fs,
        ),
    );
    emit_synthetic_bold_ops(node_style, font_ref, fs, &tc, ops);
    write_ops(
        ops,
        format_args!("{:.2} {:.2} Td\n{} Tj\n", x + p, text_y, encoded),
    );
    reset_synthetic_bold_ops(node_style, font_ref, ops);
    ops.extend_from_slice(b"ET\n");
    let text_x = x + p;
    let text_y = pdf_y + p;
    let line_thickness = (fs * 0.05).max(0.5);
    if node_style.underline {
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
    emit_synthetic_bold_ops(node_style, font_ref, font_size, &tc, ops);
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
    reset_synthetic_bold_ops(node_style, font_ref, ops);
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
                if is_span_bold && !font_ref_is_bold(span_font_ref) {
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
                if is_span_bold && !font_ref_is_bold(span_font_ref) {
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
                .take_while(|(a, b)| a == b)
                .count();

            if common > 0 {
                let common_str: String = line_rest.chars().take(common).collect();
                let common_byte_len = common_str.len();
                segs.push(LineSpanSegment {
                    text: common_str,
                    span_idx,
                });
                line_pos += common_byte_len;
                span_off += common_byte_len;
                if span_off >= span.text.len() {
                    span_idx += 1;
                    span_off = 0;
                }
            } else {
                let skip = span_rest
                    .bytes()
                    .take_while(|b: &u8| b.is_ascii_whitespace())
                    .count();
                if skip > 0 {
                    span_off += skip;
                    if span_off >= span.text.len() {
                        span_idx += 1;
                        span_off = 0;
                    }
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
            let skip = rest
                .bytes()
                .take_while(|b: &u8| b.is_ascii_whitespace())
                .count();
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

#[allow(clippy::too_many_arguments)]
fn render_draw(
    draw_content: &DrawContent,
    abs_x: f64,
    pdf_y: f64,
    _w: f64,
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
                emit_synthetic_bold_ops(node_style, font_ref, fs, &tc, ops);
                write_ops(
                    ops,
                    format_args!("{:.2} {:.2} Td\n{} Tj\n", abs_x, pdf_y, encoded),
                );
                reset_synthetic_bold_ops(node_style, font_ref, ops);
                ops.extend_from_slice(b"ET\n");
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
    fn v_align_middle() {
        let style = FormNodeStyle {
            v_align: Some(VerticalAlign::Middle),
            ..Default::default()
        };
        let s = styled_overlay_str(make_styled_field(0.0, 0.0, 200.0, 40.0, "Mid", style));
        assert!(s.contains("(Mid) Tj"));
    }

    #[test]
    fn text_field_default_background_is_light_gray() {
        let s = styled_overlay_str(make_styled_field(
            10.0,
            10.0,
            100.0,
            20.0,
            "Hi",
            FormNodeStyle::default(),
        ));
        assert!(
            s.contains("0.949 0.949 0.949 rg"),
            "default editable field background should be Adobe light gray: {s}"
        );
    }

    #[test]
    fn numeric_field_default_background_is_light_gray() {
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
            s.contains("0.949 0.949 0.949 rg"),
            "numeric edit fields should use the same default gray background: {s}"
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
    fn container_children_y_offset_excludes_inset() {
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
        };
        let s = overlay_str(&make_page(vec![parent]));
        assert!(
            s.contains("100.00 572.00 50.00 20.00 re"),
            "child y should stay anchored to parent y without inset_top offset: {s}"
        );
        assert!(
            !s.contains("100.00 562.00 50.00 20.00 re"),
            "child y should not be shifted down by parent inset_top: {s}"
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
}
