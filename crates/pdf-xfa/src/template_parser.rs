//! XDP template XML → FormTree parser.
//!
//! Reads the `<template>` packet from an XFA XDP document and builds a
//! `FormTree` suitable for `LayoutEngine::layout()`.
//!
//! XFA Spec 3.3 §2.1 — Form Structural Building Blocks:
//!   Container elements: subform, field, draw, exclGroup, area
//!   Page-level:         pageSet, pageArea, contentArea, medium
//!   Metadata:           caption, value, ui, font, border, margin, para
//!
//! XFA Spec 3.3 §2.2 — Basic Composition:
//!   Measurements use absolute units (in, cm, mm, pt) with inches as default.
//!   Dimension strings ("0.5in", "72pt", "10mm") are converted to PDF
//!   points via `Measurement::parse`.

use roxmltree::Node;

use xfa_layout_engine::form::{
    AnchorType, ContentArea, DrawContent, EventScript, FieldKind, FormNode, FormNodeId,
    FormNodeMeta, FormNodeStyle, FormNodeType, FormTree, GroupKind, Occur, Presence,
    ScriptLanguage,
};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{
    BoxModel, Caption, CaptionPlacement, Insets, LayoutStrategy, Measurement, TextAlign,
    VerticalAlign,
};

use crate::error::{Result, XfaError};

/// Parse a `<template>` XML packet into a `FormTree`.
///
/// `xml` should be the raw content of the template packet, starting with
/// the `<template …>` element (with or without an XML declaration).
///
/// If `datasets_xml` is provided, field values are merged from the
/// `<xfa:data>` section of the datasets packet.
pub fn parse_template(xml: &str, datasets_xml: Option<&str>) -> Result<(FormTree, FormNodeId)> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| XfaError::ParseFailed(format!("template XML parse error: {e}")))?;

    let root_elem = doc.root_element();
    // The packet may start with <template> directly, or may have been
    // wrapped inside <xdp:xdp>. Accept both.
    let template_elem = if root_elem.tag_name().name() == "template" {
        root_elem
    } else {
        // Find the first <template> descendant.
        find_first_child_by_name(root_elem, "template")
            .ok_or_else(|| XfaError::PacketNotFound("no <template> element found".to_string()))?
    };

    let mut tree = FormTree::new();
    let (root_id, _trailing) = parse_node(&mut tree, template_elem, true)?;

    // Data binding: merge field values from datasets XML.
    if let Some(ds_xml) = datasets_xml {
        if let Ok(ds_doc) = roxmltree::Document::parse(ds_xml) {
            if let Some(data_root) = find_data_root(ds_doc.root_element()) {
                bind_data(&mut tree, root_id, &data_root, &data_root);
            }
        }
    }

    Ok((tree, root_id))
}

// ---------------------------------------------------------------------------
// Recursive node parser
// ---------------------------------------------------------------------------

/// Parse an XML element into a form node and add it to the tree.
///
/// Returns `(FormNodeId, trailing_break)` where `trailing_break` is true when
/// the node's `add_children` ended with a pending breakBefore that could not
/// be consumed within the node (i.e. the breakBefore appeared after the last
/// content child and should propagate to the next sibling at the parent level).
fn parse_node(
    tree: &mut FormTree,
    elem: Node<'_, '_>,
    is_root: bool,
) -> Result<(FormNodeId, (bool, Option<String>))> {
    let tag = elem.tag_name().name();

    let (node, trailing_info) = match tag {
        "template" => {
            let mut n = parse_root_node(tree, elem)?;
            let ti = add_children(tree, &mut n, elem)?;
            (n, ti)
        }
        // XFA Spec 3.3 §2.1 — Five building blocks: subform, field, draw, exclGroup, area.
        // `area` is a fixed-size container identical to subform but does not grow;
        // if capacity is exceeded a new area is created. We parse it like a subform.
        "subform" | "exclGroup" | "area" => {
            let mut n = parse_subform_node(tree, elem, is_root)?;
            let ti = add_children(tree, &mut n, elem)?;
            // Per XFA 3.3 §2.1: checkButton inside exclGroup renders as radio button (circle).
            if tag == "exclGroup" {
                for &child_id in &n.children {
                    let child_meta = tree.meta_mut(child_id);
                    if child_meta.field_kind == FieldKind::Checkbox {
                        child_meta.field_kind = FieldKind::Radio;
                    }
                }
            }
            (n, ti)
        }
        "field" => (parse_field(tree, elem)?, (false, None)),
        "draw" => (parse_draw(tree, elem)?, (false, None)),
        "pageSet" => (parse_page_set(tree, elem)?, (false, None)),
        "pageArea" => (parse_page_area(tree, elem)?, (false, None)),
        _ => {
            let mut n = blank_node(tag);
            let ti = add_children(tree, &mut n, elem)?;
            (n, ti)
        }
    };

    let mut meta = parse_node_meta(elem);
    // Only fields need their <margin ...Inset> values forwarded to
    // style.inset_*_pt — the renderer uses that to offset the value and
    // shrink the border/bg to the inner rect. Draws are skipped because
    // their decorative insets in background-image forms (see 49f8705c)
    // cause visible misalignment with the pre-rendered backdrop; subforms
    // are skipped to avoid the double-application the layout engine
    // already handles via content_width()/content_height() subtraction.
    if tag == "field" {
        meta.style.inset_top_pt = Some(node.box_model.margins.top);
        meta.style.inset_bottom_pt = Some(node.box_model.margins.bottom);
        meta.style.inset_left_pt = Some(node.box_model.margins.left);
        meta.style.inset_right_pt = Some(node.box_model.margins.right);
    }
    Ok((tree.add_node_with_meta(node, meta), trailing_info))
}

/// Build the root FormNode (without children — `parse_node` calls `add_children`).
fn parse_root_node(_tree: &mut FormTree, _elem: Node<'_, '_>) -> Result<FormNode> {
    Ok(FormNode {
        name: "root".to_string(),
        node_type: FormNodeType::Root,
        box_model: BoxModel {
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    })
}

/// Build a subform FormNode (without children — `parse_node` calls `add_children`).
fn parse_subform_node(
    _tree: &mut FormTree,
    elem: Node<'_, '_>,
    _is_root: bool,
) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();
    let layout = parse_layout_attr(elem);
    let mut bm = parse_box_model(elem);
    // For paginate-layout root subforms, set a default US Letter size if
    // no explicit size is given.
    if layout == LayoutStrategy::TopToBottom && bm.width.is_none() {
        bm.width = Some(612.0);
    }

    Ok(FormNode {
        name,
        node_type: FormNodeType::Subform,
        box_model: bm,
        layout,
        children: Vec::new(),
        occur: parse_occur(elem),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    })
}

fn parse_field(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();
    let bm = parse_box_model(elem);

    // Always extract the field value and preserve the Field node type.
    // Visibility is controlled by FormNodeMeta.presence -- the layout
    // engine skips invisible/inactive nodes; hidden ones produce empty space.
    // Dynamic scripts can later toggle presence to "visible", so we must
    // preserve the content for all fields.
    let value = extract_value_text(elem).unwrap_or_default();

    // Extract caption from <caption> child element (placement, reserve, text).
    let mut bm_with_caption = bm.clone();
    if !is_hidden(elem) {
        if let Some(cap) = parse_caption(elem) {
            bm_with_caption.caption = Some(cap);
        }
    }

    let mut font = parse_font_metrics(elem);
    // Override font size from exData HTML if present.
    if let Some(html_size) = extract_exdata_font_size(elem) {
        font.size = html_size;
    }

    let node = FormNode {
        name,
        node_type: FormNodeType::Field { value },
        box_model: bm_with_caption,
        layout: LayoutStrategy::Positioned,
        children: Vec::new(),
        occur: Occur::once(),
        font,
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: parse_col_span(elem),
    };
    // Fields are leaf nodes — no child traversal needed.
    let _ = tree; // suppress unused warning
    Ok(node)
}

fn parse_draw(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();
    let bm = parse_box_model(elem);

    // Try geometric draw content (line, rectangle, arc) first
    if let Some(draw_content) = extract_draw_content(elem) {
        let node = FormNode {
            name,
            node_type: FormNodeType::Draw(draw_content),
            box_model: bm,
            layout: LayoutStrategy::Positioned,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };
        let _ = tree;
        return Ok(node);
    }

    if let Some((image_data, mime_type)) = extract_value_image(elem) {
        let node = FormNode {
            name,
            node_type: FormNodeType::Image {
                data: image_data,
                mime_type,
            },
            box_model: bm,
            layout: LayoutStrategy::Positioned,
            children: Vec::new(),
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };
        let _ = tree;
        return Ok(node);
    }

    let mut font = parse_font_metrics(elem);
    // If the content came from <exData contentType="text/html">, extract
    // the dominant font size from the HTML <span style="font-size:Xpt">.
    // This overrides the default 10pt when the HTML specifies differently.
    if let Some(html_size) = extract_exdata_font_size(elem) {
        font.size = html_size;
    }

    // Always extract content — visibility is controlled by metadata.
    let content = extract_value_text(elem).unwrap_or_default();

    let node = FormNode {
        name,
        node_type: FormNodeType::Draw(DrawContent::Text(content)),
        box_model: bm,
        layout: LayoutStrategy::Positioned,
        children: Vec::new(),
        occur: Occur::once(),
        font,
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    };
    let _ = tree;
    Ok(node)
}

/// Return `true` when the element has a `presence` attribute value that means
/// the element should not be rendered (`"hidden"`, `"invisible"`, `"inactive"`).
/// Elements with no `presence` attribute or `presence="visible"` are rendered.
fn is_hidden(elem: Node<'_, '_>) -> bool {
    matches!(
        attr(elem, "presence"),
        Some("hidden") | Some("invisible") | Some("inactive")
    )
}

/// Parse a font size string where bare numbers are in **points** (not inches).
///
/// XFA `<font size="…">` uses points as the default unit — a bare number like `"10"`
/// means 10pt, not 10 inches.  General dimension attributes (w, h, x, y) use inches
/// as the default, so they go through `parse_dim` instead.
fn parse_font_size(s: &str) -> Option<f64> {
    // Bare number → points (XFA default for font sizes).
    if let Ok(v) = s.trim().parse::<f64>() {
        return if v > 0.0 { Some(v) } else { None };
    }
    // Explicit unit ("10pt", "3mm", …) → convert to points.
    Measurement::parse(s).map(|m| m.to_points())
}

/// Parse a percentage string like `"96%"` → `0.96`, `"110%"` → `1.1`.
/// Returns `None` if the string is not a valid percentage.
fn parse_percentage(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str = s.strip_suffix('%')?;
    let v: f64 = num_str.trim().parse().ok()?;
    Some(v / 100.0)
}

/// Parse a letter-spacing string. Supported formats:
/// - `"-0.018em"` → converted to points using the given font size
/// - `"0.5pt"` → points directly
/// - `"1mm"` → converted to points via Measurement
///   Returns `None` if the string cannot be parsed.
fn parse_letter_spacing(s: &str, font_size_pt: f64) -> Option<f64> {
    let s = s.trim();
    if s == "0" {
        return Some(0.0);
    }
    // Try em-based value: "-0.018em"
    if let Some(num_str) = s.strip_suffix("em") {
        let v: f64 = num_str.trim().parse().ok()?;
        return Some(v * font_size_pt);
    }
    // Try standard measurement ("0.5pt", "1mm", etc.)
    Measurement::parse(s).map(|m| m.to_points())
}

/// Parse font size and text alignment from `<font size="…">` and `<para hAlign="…">` child
/// elements (XFA 3.3 §7.1). Returns `FontMetrics::default()` when no matching elements found.
///
/// XFA Spec 3.3 §2.6 (p57-58) — Font properties: typeface (default Courier for
/// data-entry), size (default 10pt), posture (normal/italic), weight (bold/normal),
/// baselineShift, fontHorizontalScale, fontVerticalScale, kerningMode, letterSpacing,
/// lineThrough/lineThroughPeriod, overline/overlinePeriod, underline/underlinePeriod.
///
/// TODO(§2.6): baselineShift, kerningMode, lineThrough, underline not parsed.
/// TODO(§2.4 p59): hAlign="radix" and hAlign="justifyAll" not handled.
///
/// XFA Spec 3.3 §28.1 — Adobe Non-conformance: Adobe ignores the "overline"
/// attribute on the font element (p1227) and the "lineThroughPeriod" attribute
/// (p1228). We intentionally skip these to match Adobe's behavior for SSIM.
fn parse_font_metrics(elem: Node<'_, '_>) -> FontMetrics {
    let font_elem = find_first_child_by_name(elem, "font");
    let size = font_elem
        .and_then(|f| attr(f, "size"))
        .and_then(parse_font_size)
        .unwrap_or(FontMetrics::default().size);
    // XFA Spec 3.3 §17 (p716) — genericFamily: fallback classification hint.
    // If present, use it to classify the font family instead of guessing from
    // the typeface name. This matches Adobe's §28.2 step 4 behavior.
    let generic_family_str = font_elem.and_then(|f| attr(f, "genericFamily"));
    let typeface = if let Some(gf) = generic_family_str {
        FontFamily::from_generic_family(gf)
    } else {
        font_elem
            .and_then(|f| attr(f, "typeface"))
            .map(FontFamily::from_typeface)
            .unwrap_or_default()
    };
    // XFA Spec 3.3 §2.4 (p44, p59-60) — hAlign values:
    //   left, center, right, justify, justifyAll, radix
    let text_align = find_first_child_by_name(elem, "para")
        .and_then(|p| attr(p, "hAlign"))
        .map(|a| match a {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "justify" | "justifyAll" => TextAlign::Justify,
            // TODO(§2.4): "radix" alignment needs radixOffset support
            _ => TextAlign::Left,
        })
        .unwrap_or_default();
    FontMetrics {
        size,
        text_align,
        typeface,
        ..FontMetrics::default()
    }
}

fn parse_page_set(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("pageSet").to_string();
    let mut node = FormNode {
        name,
        node_type: FormNodeType::PageSet,
        box_model: BoxModel {
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    };
    // Children of pageSet are pageArea elements.
    for child in elem.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "pageArea" {
            let (child_id, _) = parse_node(tree, child, false)?;
            node.children.push(child_id);
        }
    }
    Ok(node)
}

fn parse_page_area(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();

    // Read <medium> for page dimensions.
    let (page_w, page_h) = read_medium(elem);

    // Read <contentArea> elements.
    let content_areas = read_content_areas(elem, page_w, page_h);

    let bm = BoxModel {
        width: Some(page_w),
        height: Some(page_h),
        ..Default::default()
    };

    let mut node = FormNode {
        name,
        node_type: FormNodeType::PageArea { content_areas },
        box_model: bm,
        layout: LayoutStrategy::Positioned,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    };
    // Parse child draw/subform elements (page-level headers, footers, lines).
    add_children(tree, &mut node, elem)?;
    Ok(node)
}

// ---------------------------------------------------------------------------
// Metadata parsing
// ---------------------------------------------------------------------------

/// Build `FormNodeMeta` from XFA element attributes and child elements.
fn parse_node_meta(elem: Node<'_, '_>) -> FormNodeMeta {
    let tag = elem.tag_name().name();

    // (a) Presence attribute (XFA 3.3 §2.6 p67-68):
    //     visible  — normal rendering (default)
    //     invisible — takes space but not visible
    //     hidden   — no space, no visible rendering
    //     inactive — completely ignored (no binding, no space)
    let presence = match attr(elem, "presence") {
        Some("hidden") => Presence::Hidden,
        Some("invisible") => Presence::Invisible,
        Some("inactive") => Presence::Inactive,
        _ => Presence::Visible,
    };

    // (b) Page break detection: look for <breakBefore>, <breakAfter>, or <break> child.
    let (page_break_before, break_before_target) = detect_page_break_before(elem);
    let (page_break_after, break_after_target) = detect_page_break_after(elem);
    let content_area_break = detect_content_area_break(elem);

    // Prefer break_before_target, then break_after_target.
    let break_target = break_before_target.or(break_after_target);

    // (c) Event scripts.
    let event_scripts = collect_event_scripts(elem);

    // (d) Keep child attributes.
    let (keep_next_content_area, keep_previous_content_area, keep_intact_content_area) =
        parse_keep(elem);

    // (e) Overflow leader/trailer.
    let (overflow_leader, overflow_trailer) = parse_overflow(elem);

    // (f) Group kind: exclGroup → ExclusiveChoice.
    let group_kind = if tag == "exclGroup" {
        GroupKind::ExclusiveChoice
    } else {
        GroupKind::None
    };

    // (g) Item value for field elements: <items><text>VALUE</text></items>.
    let item_value = if tag == "field" {
        parse_item_value(elem)
    } else {
        None
    };

    // (g2) Choice list items for dropdown fields (XFA 3.3 §7.7).
    let (display_items, save_items) = if tag == "field" {
        parse_items_lists(elem)
    } else {
        (Vec::new(), Vec::new())
    };

    // (h) XFA id attribute.
    let xfa_id = attr(elem, "id").map(|s| s.to_string());

    // (i) Field UI kind: detect <checkButton>, <choiceList>, etc. inside <ui>.
    let field_kind = detect_field_kind(elem);

    // (j) Visual style: colors, borders, font from XFA template elements.
    let style = parse_node_style(elem);
    let (data_bind_ref, data_bind_none) = parse_bind(elem);
    let anchor_type = parse_anchor_type(elem);

    FormNodeMeta {
        xfa_id,
        presence,
        page_break_before,
        page_break_after,
        break_target,
        content_area_break,
        overflow_leader,
        overflow_trailer,
        keep_next_content_area,
        keep_previous_content_area,
        keep_intact_content_area,
        event_scripts,
        data_bind_ref,
        data_bind_none,
        group_kind,
        item_value,
        field_kind,
        style,
        display_items,
        save_items,
        anchor_type,
        ..Default::default()
    }
}

fn parse_bind(elem: Node<'_, '_>) -> (Option<String>, bool) {
    let Some(bind) = find_first_child_by_name(elem, "bind") else {
        return (None, false);
    };

    let bind_none = attr(bind, "match") == Some("none");
    let bind_ref = if bind_none {
        None
    } else {
        attr(bind, "ref").map(|s| s.trim().to_string())
    };
    (bind_ref, bind_none)
}

/// Parse visual style from XFA template elements.
///
/// Extracts colors from `<fill><color value="r,g,b"/>`, border from
/// `<border><edge><color value="r,g,b"/>`, and font from `<font>`.
fn parse_node_style(elem: Node<'_, '_>) -> FormNodeStyle {
    let mut style = FormNodeStyle {
        check_button_mark: parse_check_button_mark(elem),
        ..Default::default()
    };

    // Parse <fill><color value="r,g,b"/> for background color.
    // Skip when presence="hidden"/"invisible"/"inactive".
    if let Some(fill) = find_first_child_by_name(elem, "fill") {
        if !is_hidden(fill) {
            if let Some(color) = find_first_child_by_name(fill, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.bg_color = Some(rgb);
                }
            }
            // Also check <fill><solid><color .../> pattern.
            if style.bg_color.is_none() {
                if let Some(solid) = find_first_child_by_name(fill, "solid") {
                    if let Some(color) = find_first_child_by_name(solid, "color") {
                        if let Some(rgb) = parse_xfa_color(color) {
                            style.bg_color = Some(rgb);
                        }
                    }
                }
            }
        }
    }

    // Parse <border><edge> for border color and thickness.
    // Borders can live directly on the element OR inside <ui><textEdit|…><border>.
    let border = find_first_child_by_name(elem, "border").or_else(|| {
        let ui = find_first_child_by_name(elem, "ui")?;
        ui.children()
            .filter(|c| c.is_element() && c.tag_name().name() != "border")
            .find_map(|widget| find_first_child_by_name(widget, "border"))
    });
    if let Some(border) = border {
        // Collect all <edge> children for per-edge visibility (XFA §D.7).
        let edges: Vec<_> = border
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "edge")
            .collect();
        // Use first visible edge for color/thickness (backward compat).
        let first_visible = edges
            .iter()
            .find(|e| !is_hidden(**e))
            .or_else(|| edges.first());
        if let Some(edge) = first_visible {
            if let Some(color) = find_first_child_by_name(*edge, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.border_color = Some(rgb);
                }
            }
            let stroke = attr(*edge, "stroke").unwrap_or("solid");
            if stroke != "none" {
                // fix(#808): XFA default edge thickness is 1pt (matches BoxModel::border_width
                // default and Adobe's behavior). Previously defaulted to 0.5pt, causing borders
                // to appear ~1px thinner at 150 DPI rendering.
                let thickness = attr(*edge, "thickness")
                    .and_then(Measurement::parse)
                    .map(|m| m.to_points())
                    .unwrap_or(1.0);
                if thickness > 0.0 {
                    style.border_width_pt = Some(thickness);
                }
            }
        }
        // Per-edge visibility: 1=all, 2=even/odd, 3=T/RL/B, 4=T/R/B/L.
        let edge_visible = |e: &roxmltree::Node| -> bool {
            !is_hidden(*e) && attr(*e, "stroke").unwrap_or("solid") != "none"
        };
        style.border_edges = match edges.len() {
            0 => [true, true, true, true],
            1 => {
                let v = edge_visible(&edges[0]);
                [v, v, v, v]
            }
            2 => {
                let even = edge_visible(&edges[0]);
                let odd = edge_visible(&edges[1]);
                [even, odd, even, odd]
            }
            3 => {
                let top = edge_visible(&edges[0]);
                let rl = edge_visible(&edges[1]);
                let bot = edge_visible(&edges[2]);
                [top, rl, bot, rl]
            }
            _ => [
                edge_visible(&edges[0]),
                edge_visible(&edges[1]),
                edge_visible(&edges[2]),
                edge_visible(&edges[3]),
            ],
        };
        let default_thickness = style.border_width_pt.unwrap_or(0.5);
        let edge_thickness = |edge: roxmltree::Node<'_, '_>| -> f64 {
            attr(edge, "thickness")
                .and_then(Measurement::parse)
                .map(|m| m.to_points())
                .unwrap_or(default_thickness)
        };
        let per_edge_widths = match edges.len() {
            0 | 1 => None,
            2 => Some([
                edge_thickness(edges[0]),
                edge_thickness(edges[1]),
                edge_thickness(edges[0]),
                edge_thickness(edges[1]),
            ]),
            3 => Some([
                edge_thickness(edges[0]),
                edge_thickness(edges[1]),
                edge_thickness(edges[2]),
                edge_thickness(edges[1]),
            ]),
            _ => Some([
                edge_thickness(edges[0]),
                edge_thickness(edges[1]),
                edge_thickness(edges[2]),
                edge_thickness(edges[3]),
            ]),
        };
        if let Some(widths) = per_edge_widths {
            if !(widths[0] == widths[1] && widths[1] == widths[2] && widths[2] == widths[3]) {
                style.border_widths = Some(widths);
            }
        }
        // Also parse <border><fill><color .../> for border background (field bg).
        // Skip when fill has presence="hidden"/"invisible"/"inactive".
        if style.bg_color.is_none() {
            if let Some(fill) = find_first_child_by_name(border, "fill") {
                if !is_hidden(fill) {
                    if let Some(color) = find_first_child_by_name(fill, "color") {
                        if let Some(rgb) = parse_xfa_color(color) {
                            style.bg_color = Some(rgb);
                        }
                    }
                }
            }
        }
    }

    // Parse <value><rectangle><edge><color> for draw rectangle border color.
    // XFA rectangle elements define their stroke color on <edge> children
    // inside the <value> container, NOT inside <border>.
    if style.border_color.is_none() {
        if let Some(value) = find_first_child_by_name(elem, "value") {
            if let Some(rect) = find_first_child_by_name(value, "rectangle") {
                let edges: Vec<_> = rect
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "edge")
                    .collect();
                if let Some(edge) = edges.first() {
                    if let Some(color) = find_first_child_by_name(*edge, "color") {
                        if let Some(rgb) = parse_xfa_color(color) {
                            style.border_color = Some(rgb);
                        }
                    }
                    if style.border_width_pt.is_none() {
                        let thickness = attr(*edge, "thickness")
                            .and_then(Measurement::parse)
                            .map(|m| m.to_points())
                            .unwrap_or(1.0);
                        if thickness > 0.0 {
                            style.border_width_pt = Some(thickness);
                        }
                    }
                }
            }
        }
    }

    // XFA `<line>` draw shapes (the `FFLineN` cell-grid rules) likewise carry
    // their stroke on `<value><line><edge>`, not `<border>`. Mirror the
    // rectangle branch so `render_draw` can stroke the spanned line at the
    // authored weight (the merge path `merger::parse_node_style` agrees).
    if style.border_width_pt.is_none() {
        if let Some(line) = find_first_child_by_name(elem, "value")
            .and_then(|value| find_first_child_by_name(value, "line"))
        {
            if let Some(edge) = find_first_child_by_name(line, "edge") {
                if attr(edge, "stroke").unwrap_or("solid") != "none" {
                    if let Some(thickness) = attr(edge, "thickness")
                        .and_then(Measurement::parse)
                        .map(|m| m.to_points())
                    {
                        if thickness > 0.0 {
                            style.border_width_pt = Some(thickness);
                        }
                    }
                    if style.border_color.is_none() {
                        if let Some(rgb) =
                            find_first_child_by_name(edge, "color").and_then(parse_xfa_color)
                        {
                            style.border_color = Some(rgb);
                        }
                    }
                }
            }
        }
    }

    // Parse <font typeface="..." size="..." weight="..."> for font properties.
    // XFA Spec 3.3 §28.1 — Adobe Non-conformance:
    //   - font-weight: numeric values (100-900) ignored, only "bold"/"normal" (p1229)
    //   - font-stretch: not implemented in rich text (p1228)
    //   - font-family: only first name used in rich text (p1228)
    // We follow Adobe's behavior for all three.
    if let Some(font) = find_first_child_by_name(elem, "font") {
        if let Some(typeface) = attr(font, "typeface") {
            style.font_family = Some(typeface.to_string());
        }
        // XFA Spec 3.3 §17 (p716) — genericFamily attribute.
        if let Some(gf) = attr(font, "genericFamily") {
            style.generic_family = Some(gf.to_string());
        }
        if let Some(size_str) = attr(font, "size") {
            if let Some(m) = Measurement::parse(size_str) {
                style.font_size = Some(m.to_points());
            }
        }
        if let Some(weight) = attr(font, "weight") {
            style.font_weight = Some(weight.to_string());
        }
        if let Some(posture) = attr(font, "posture") {
            style.font_style = Some(posture.to_string());
        }
        // <font><fill><color .../> for text color
        if let Some(fill) = find_first_child_by_name(font, "fill") {
            if let Some(color) = find_first_child_by_name(fill, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.text_color = Some(rgb);
                }
            }
        }
        // <font color="#RRGGBB"> attribute (fallback when <fill><color> not present)
        if style.text_color.is_none() {
            if let Some(color_str) = attr(font, "color") {
                if let Some(rgb) = parse_font_color_attr(color_str) {
                    style.text_color = Some(rgb);
                }
            }
        }
        // fontHorizontalScale="96%" → 0.96
        if let Some(scale_str) = attr(font, "fontHorizontalScale") {
            if let Some(v) = parse_percentage(scale_str) {
                style.font_horizontal_scale = Some(v);
            }
        }
        // letterSpacing="-0.018em" or "0.5pt"
        if let Some(ls_str) = attr(font, "letterSpacing") {
            if let Some(v) = parse_letter_spacing(ls_str, style.font_size.unwrap_or(10.0)) {
                style.letter_spacing_pt = Some(v);
            }
        }
        // XFA Spec 3.3 §2.6 — underline="1" (single) or "2" (double)
        if let Some(underline_str) = attr(font, "underline") {
            style.underline = underline_str == "1" || underline_str == "2";
        }
        // XFA Spec 3.3 §2.6 — lineThrough="1"
        if let Some(line_through_str) = attr(font, "lineThrough") {
            style.line_through = line_through_str == "1";
        }
    }

    // XFA Spec 3.3 §17 "para" (p803) — Paragraph-level formatting attributes:
    // hAlign: left/center/right/justify (handled in parse_font_metrics)
    // vAlign: top/middle/bottom for vertical alignment within container
    // spaceAbove, spaceBelow: paragraph spacing in points
    // marginLeft, marginRight: paragraph indentation
    // Note: hAlign/vAlign on container elements are deprecated since XFA 2.4
    // and ignored by Adobe; we correctly read these only from <para>.
    if let Some(para) = find_first_child_by_name(elem, "para") {
        if let Some(v) = attr(para, "spaceAbove").and_then(Measurement::parse) {
            style.space_above_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "spaceBelow").and_then(Measurement::parse) {
            style.space_below_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "marginLeft").and_then(Measurement::parse) {
            style.margin_left_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "marginRight").and_then(Measurement::parse) {
            style.margin_right_pt = Some(v.to_points());
        }
        // XFA Spec 3.3 §17 "para" (p803) — lineHeight / textIndent.
        if let Some(v) = attr(para, "lineHeight").and_then(Measurement::parse) {
            style.line_height_pt = Some(v.to_points());
        }
        if let Some(v) = attr(para, "textIndent").and_then(Measurement::parse) {
            style.text_indent_pt = Some(v.to_points());
        }
        if let Some(va) = attr(para, "vAlign") {
            style.v_align = Some(match va {
                "middle" => VerticalAlign::Middle,
                "bottom" => VerticalAlign::Bottom,
                _ => VerticalAlign::Top,
            });
        }
        // XFA Spec 3.3 §8.3 (p282-284) — hAlign positions child within parent
        // layout container. Stored separately from FontMetrics.text_align which
        // controls text rendering alignment within the element itself.
        if let Some(ha) = attr(para, "hAlign") {
            style.h_align = Some(match ha {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            });
        }
    }

    // Parse <border><corner> for border radius and <border><edge> for border style.
    if let Some(border) = border {
        if let Some(corner) = find_first_child_by_name(border, "corner") {
            if let Some(v) = attr(corner, "radius").and_then(Measurement::parse) {
                style.border_radius_pt = Some(v.to_points());
            }
        }
        if let Some(edge) = find_first_child_by_name(border, "edge") {
            if let Some(stroke) = attr(edge, "stroke") {
                if stroke != "none" {
                    style.border_style = Some(stroke.to_string());
                }
            }
        }
    }

    // Parse <format><picture> for numeric/date/time formatting patterns.
    if let Some(format) = find_first_child_by_name(elem, "format") {
        if let Some(picture) = find_first_child_by_name(format, "picture") {
            if let Some(text) = picture.text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    style.format_pattern = Some(trimmed.to_string());
                }
            }
        }
    }

    style
}

fn parse_check_button_mark(elem: Node<'_, '_>) -> Option<String> {
    let ui = find_first_child_by_name(elem, "ui")?;
    let check_button = ui
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "checkButton")?;
    let mark = attr(check_button, "mark")?.to_ascii_lowercase();
    match mark.as_str() {
        "check" | "circle" | "cross" | "diamond" | "square" | "star" => Some(mark),
        _ => None,
    }
}

/// Parse XFA `<color value="r,g,b"/>` into (u8, u8, u8).
fn parse_xfa_color(color_node: Node<'_, '_>) -> Option<(u8, u8, u8)> {
    let value = attr(color_node, "value")?;
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() >= 3 {
        let r = parts[0].trim().parse::<u8>().ok()?;
        let g = parts[1].trim().parse::<u8>().ok()?;
        let b = parts[2].trim().parse::<u8>().ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Parse a color string from a `color` attribute on `<font>`.
///
/// Supported formats:
/// - `#RRGGBB` (e.g. `#000080`)
/// - `#RGB` shorthand (e.g. `#00F` → `#0000FF`)
/// - `r,g,b` with decimal values 0-255 (e.g. `0,0,128`)
fn parse_font_color_attr(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some((r, g, b))
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                Some((r * 17, g * 17, b * 17))
            }
            _ => None,
        }
    } else {
        // Try "r,g,b" decimal format
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            Some((r, g, b))
        } else {
            None
        }
    }
}

/// Detect field UI type from `<ui>` child element.
///
/// XFA Spec 3.3 §2.1 (p35) — User Interface: each container may have a `<ui>`
/// subelement specifying the widget type. If absent, defaults based on content type.
/// Supported: textEdit, checkButton, button, choiceList, dateTimeEdit,
///            numericEdit, passwordEdit, imageEdit, signature, barcode.
fn detect_field_kind(elem: Node<'_, '_>) -> FieldKind {
    let Some(ui) = find_first_child_by_name(elem, "ui") else {
        return FieldKind::Text;
    };
    for child in ui.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        match tag {
            "checkButton" => {
                // XFA 3.3 §7.2.7: shape="round" → radio button (circle).
                let shape = attr(child, "shape").unwrap_or("square");
                return if shape == "round" {
                    FieldKind::Radio
                } else {
                    FieldKind::Checkbox
                };
            }
            "choiceList" => return FieldKind::Dropdown,
            "button" => return FieldKind::Button,
            "dateTimeEdit" => return FieldKind::DateTimePicker,
            "numericEdit" => return FieldKind::NumericEdit,
            "passwordEdit" => return FieldKind::PasswordEdit,
            "imageEdit" => return FieldKind::ImageEdit,
            "signature" => return FieldKind::Signature,
            "barcode" => return FieldKind::Barcode,
            _ => {}
        }
    }
    FieldKind::Text
}

/// Detect page breaks: look for a child element named `breakBefore` or `break`.
///
/// Only considers breakBefore/break elements that appear BEFORE the first
/// content child (subform/field/draw/exclGroup). Inline breakBefore elements
/// between content children are handled by `add_children` which propagates
/// them to the next sibling's metadata.
///
/// Returns `(break_found, target_name)`.
fn detect_page_break_before(elem: Node<'_, '_>) -> (bool, Option<String>) {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if matches!(tag, "subform" | "field" | "draw" | "exclGroup") {
            break;
        }
        if tag == "breakBefore" && attr(child, "targetType") == Some("pageArea") {
            return (true, attr(child, "target").map(|s| s.to_string()));
        }
        if tag == "break" && attr(child, "before") == Some("pageArea") {
            // XFA §9.2.1: `before="pageArea"` triggers a page break
            // regardless of targetType. targetType only constrains which
            // specific page area to target.
            return (true, attr(child, "target").map(|s| s.to_string()));
        }
    }
    (false, None)
}

/// Detect page-break-after: look for a child element named `breakAfter` or `break`.
///
/// Scans all children after the last content child.
fn detect_page_break_after(elem: Node<'_, '_>) -> (bool, Option<String>) {
    let mut last_content_idx = 0;
    let children: Vec<_> = elem.children().filter(|n| n.is_element()).collect();
    for (i, child) in children.iter().enumerate() {
        let tag = child.tag_name().name();
        if matches!(tag, "subform" | "field" | "draw" | "exclGroup") {
            last_content_idx = i;
        }
    }

    // Check elements after the last content node.
    for child in children.iter().skip(last_content_idx) {
        let tag = child.tag_name().name();
        if tag == "breakAfter" && attr(*child, "targetType") == Some("pageArea") {
            return (true, attr(*child, "target").map(|s| s.to_string()));
        }
        if tag == "break" && attr(*child, "after") == Some("pageArea") {
            return (true, attr(*child, "target").map(|s| s.to_string()));
        }
    }
    (false, None)
}

/// Detect `breakBefore targetType="contentArea"` — the node targets a
/// specific named content area (e.g. "flatten", "eSign") and should be
/// excluded from the primary content flow.
///
/// Scans ALL children (not just before the first content child) because
/// contentArea breaks often appear as trailing elements inside subforms
/// (DOT form pattern: eSign/lock break appears after their content fields).
fn detect_content_area_break(elem: Node<'_, '_>) -> bool {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if tag == "breakBefore" && attr(child, "targetType") == Some("contentArea") {
            return true;
        }
    }
    false
}

/// Collect event scripts from `<event>` and `<calculate>` children.
fn collect_event_scripts(elem: Node<'_, '_>) -> Vec<EventScript> {
    let mut scripts = Vec::new();
    for child in elem.children().filter(|n| n.is_element()) {
        let child_tag = child.tag_name().name();
        if child_tag == "event" {
            // Skip layout-ready events (activity="ready" ref="$layout").
            let activity = attr(child, "activity");
            let event_ref = attr(child, "ref");
            if activity == Some("ready") && event_ref == Some("$layout") {
                continue;
            }
            // Look for a <script> child.
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(script) =
                    build_event_script(script_elem, activity, event_ref, attr(script_elem, "runAt"))
                {
                    scripts.push(script);
                }
            }
        } else if child_tag == "calculate" {
            // Direct <calculate><script>...</script></calculate>
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(script) = build_event_script(
                    script_elem,
                    Some("calculate"),
                    None,
                    attr(script_elem, "runAt"),
                ) {
                    scripts.push(script);
                }
            }
        }
    }
    scripts
}

fn build_event_script(
    script_elem: Node<'_, '_>,
    activity: Option<&str>,
    event_ref: Option<&str>,
    run_at: Option<&str>,
) -> Option<EventScript> {
    let text = script_elem.text()?.trim();
    if text.is_empty() {
        return None;
    }

    Some(EventScript::new(
        text.to_string(),
        detect_script_language(attr(script_elem, "contentType")),
        activity.map(str::to_string),
        event_ref.map(str::to_string),
        run_at.map(str::to_string),
    ))
}

fn detect_script_language(content_type: Option<&str>) -> ScriptLanguage {
    match content_type.map(|value| value.trim().to_ascii_lowercase()) {
        None => ScriptLanguage::FormCalc,
        Some(value) if value == "application/x-formcalc" || value.ends_with("/x-formcalc") => {
            ScriptLanguage::FormCalc
        }
        Some(value)
            if value == "application/x-javascript"
                || value == "application/javascript"
                || value == "text/javascript"
                || value.ends_with("/x-javascript") =>
        {
            ScriptLanguage::JavaScript
        }
        Some(_) => ScriptLanguage::Other,
    }
}

// XFA Spec 3.3 §17 "keep" (p776-777) — Controls whether content Area breaks are allowed:
// next: keep next content area together
// previous: keep previous content area together
// intact: keep content area intact (no breaks within)
fn parse_keep(elem: Node<'_, '_>) -> (bool, bool, bool) {
    if let Some(keep) = find_first_child_by_name(elem, "keep") {
        let next = attr(keep, "next") == Some("contentArea");
        let prev = attr(keep, "previous") == Some("contentArea");
        let intact = attr(keep, "intact") == Some("contentArea");
        (next, prev, intact)
    } else {
        (false, false, false)
    }
}

// XFA Spec 3.3 §17 "overflow" (p804-805) — Overflow leader/trailer for pagination:
// leader: reference to element to render before overflow content
// trailer: reference to element to render after overflow content
fn parse_overflow(elem: Node<'_, '_>) -> (Option<String>, Option<String>) {
    if let Some(overflow) = find_first_child_by_name(elem, "overflow") {
        let leader = attr(overflow, "leader").map(|s| s.to_string());
        let trailer = attr(overflow, "trailer").map(|s| s.to_string());
        (leader, trailer)
    } else {
        (None, None)
    }
}

/// Parse item value from `<items><text>VALUE</text></items>`.
fn parse_item_value(elem: Node<'_, '_>) -> Option<String> {
    let items = find_first_child_by_name(elem, "items")?;
    let text_elem = find_first_child_by_name(items, "text")?;
    let text = text_elem.text()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Extract all text values from an `<items>` element.
fn collect_items_texts(items_elem: Node<'_, '_>) -> Vec<String> {
    items_elem
        .children()
        .filter(|n| n.is_element())
        .filter_map(|child| {
            let txt = child.text().unwrap_or("").trim().to_string();
            if txt.is_empty() {
                None
            } else {
                Some(txt)
            }
        })
        .collect()
}

/// Parse choice list `<items>` elements from a `<field>` node (XFA 3.3 §7.7).
fn parse_items_lists(elem: Node<'_, '_>) -> (Vec<String>, Vec<String>) {
    let items_elems: Vec<_> = elem
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "items")
        .collect();
    match items_elems.len() {
        0 => (Vec::new(), Vec::new()),
        1 => {
            let vals = collect_items_texts(items_elems[0]);
            (vals, Vec::new())
        }
        _ => {
            let first = items_elems[0];
            let second = items_elems[1];
            let first_is_save = attr(first, "save") == Some("1");
            if first_is_save {
                (collect_items_texts(second), collect_items_texts(first))
            } else {
                (collect_items_texts(first), collect_items_texts(second))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Data binding
// ---------------------------------------------------------------------------

/// Find the data root element from a datasets document.
/// Datasets packet: `<xfa:datasets><xfa:data>...</xfa:data></xfa:datasets>`.
fn find_data_root<'a, 'input>(root: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    // Look for a child named "data".
    for child in root.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "data" {
            // Return first element child of <data>, or <data> itself.
            return child.children().find(|n| n.is_element()).or(Some(child));
        }
    }
    // If root is the data element itself.
    if root.tag_name().name() == "data" {
        return root.children().find(|n| n.is_element()).or(Some(root));
    }
    // Fall back to first element child.
    root.children().find(|n| n.is_element())
}

/// Recursively walk the form tree and bind data values from the datasets.
fn bind_data(
    tree: &mut FormTree,
    node_id: FormNodeId,
    data_root: &Node<'_, '_>,
    data_node: &Node<'_, '_>,
) {
    let name = tree.get(node_id).name.clone();
    let children: Vec<FormNodeId> = tree.get(node_id).children.clone();
    let meta = tree.meta(node_id).clone();
    let group_kind = meta.group_kind;

    // For exclGroups: look up group value, set matching child, clear others.
    if group_kind == GroupKind::ExclusiveChoice && !name.is_empty() {
        let data_value = lookup_bound_text(data_root, data_node, &meta, &name);
        // Pre-collect item values to avoid borrow conflicts.
        let child_item_vals: Vec<(FormNodeId, Option<String>)> = children
            .iter()
            .map(|&cid| (cid, tree.meta(cid).item_value.clone()))
            .collect();
        for (child_id, item_val) in child_item_vals {
            if let FormNodeType::Field { ref mut value } = tree.get_mut(child_id).node_type {
                if let Some(ref dv) = data_value {
                    if item_val.as_deref() == Some(dv.as_str()) {
                        *value = dv.clone();
                    } else {
                        *value = String::new();
                    }
                } else {
                    // No data found: clear all children to prevent template defaults
                    // from firing wrong scripts.
                    *value = String::new();
                }
            }
        }
        return;
    }

    // For fields: look up data value directly.
    if let FormNodeType::Field { ref mut value } = tree.get_mut(node_id).node_type {
        if let Some(dv) = lookup_bound_text(data_root, data_node, &meta, &name) {
            *value = dv;
        }
        return; // Fields are leaf nodes.
    }

    // For subforms: find matching data child and recurse.
    // When occur max > 1 and data has multiple matching children,
    // clone the subform for each additional data instance.
    let bound_nodes = resolve_bound_nodes(data_root, data_node, &meta, &name);
    let effective_data = bound_nodes.first().copied().unwrap_or(*data_node);

    // Check for repeating subform instances in the data.
    let occur = tree.get(node_id).occur.clone();
    let max_instances = occur.max.map(|max| max as usize).unwrap_or(usize::MAX);
    if max_instances > 1 && !meta.data_bind_none {
        let data_instances: Vec<_> = if let Some(bind_ref) = meta.data_bind_ref.as_deref() {
            resolve_bind_nodes(data_root, data_node, bind_ref)
        } else if !name.is_empty() {
            data_node
                .children()
                .filter(|child| child.is_element() && child.tag_name().name() == name)
                .collect()
        } else {
            Vec::new()
        };

        if !data_instances.is_empty() {
            tree.get_mut(node_id).occur.initial = 1;
        }

        if data_instances.len() > 1 {
            // Bind the first instance to the existing subform node.
            bind_data_children(tree, node_id, &children, data_root, &data_instances[0]);

            // Clone the subform for each additional data instance.
            let parent_id = tree
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.children.contains(&node_id))
                .map(|(i, _)| FormNodeId(i));
            let mut insert_pos = parent_id.and_then(|pid| {
                tree.get(pid)
                    .children
                    .iter()
                    .position(|&c| c == node_id)
                    .map(|pos| (pid, pos + 1))
            });

            for data_inst in &data_instances[1..data_instances.len().min(max_instances)] {
                let cloned_id = clone_subtree(tree, node_id);
                tree.get_mut(cloned_id).occur.initial = 1;
                bind_data_children(
                    tree,
                    cloned_id,
                    &tree.get(cloned_id).children.clone(),
                    data_root,
                    data_inst,
                );
                // Insert clone after the original in the parent's children list.
                if let Some((pid, pos)) = insert_pos.as_mut() {
                    let parent = tree.get_mut(*pid);
                    parent.children.insert(*pos, cloned_id);
                    *pos += 1;
                }
            }
            return;
        }
    }

    bind_data_children(tree, node_id, &children, data_root, &effective_data);
}

/// Bind data to a subform's children.
fn bind_data_children(
    tree: &mut FormTree,
    _parent_id: FormNodeId,
    children: &[FormNodeId],
    data_root: &Node<'_, '_>,
    data_node: &Node<'_, '_>,
) {
    for &child_id in children {
        bind_data(tree, child_id, data_root, data_node);
    }
}

/// Deep-clone a subtree in the FormTree, returning the new root's ID.
fn clone_subtree(tree: &mut FormTree, source_id: FormNodeId) -> FormNodeId {
    let source = tree.get(source_id).clone();
    let source_meta = tree.meta(source_id).clone();

    // Clone children recursively first.
    let new_children: Vec<FormNodeId> = source
        .children
        .iter()
        .map(|&child_id| clone_subtree(tree, child_id))
        .collect();

    let mut new_node = source;
    new_node.children = new_children;
    tree.add_node_with_meta(new_node, source_meta)
}

/// Look up a text value for a named element in the data node.
fn lookup_data_text(data_node: &Node<'_, '_>, name: &str) -> Option<String> {
    let child = find_child_element_by_name(data_node, name)?;
    child.text().map(|s| s.to_string())
}

fn lookup_bound_text(
    data_root: &Node<'_, '_>,
    data_node: &Node<'_, '_>,
    meta: &FormNodeMeta,
    fallback_name: &str,
) -> Option<String> {
    if meta.data_bind_none {
        return None;
    }

    if let Some(bind_ref) = meta.data_bind_ref.as_deref() {
        return resolve_bind_nodes(data_root, data_node, bind_ref)
            .into_iter()
            .next()
            .and_then(|node| node.text().map(|s| s.to_string()));
    }

    if fallback_name.is_empty() {
        None
    } else {
        lookup_data_text(data_node, fallback_name)
    }
}

fn resolve_bound_nodes<'a, 'input>(
    data_root: &Node<'a, 'input>,
    data_node: &Node<'a, 'input>,
    meta: &FormNodeMeta,
    fallback_name: &str,
) -> Vec<Node<'a, 'input>> {
    if meta.data_bind_none {
        return Vec::new();
    }

    if let Some(bind_ref) = meta.data_bind_ref.as_deref() {
        return resolve_bind_nodes(data_root, data_node, bind_ref);
    }

    if fallback_name.is_empty() {
        Vec::new()
    } else {
        find_child_element_by_name(data_node, fallback_name)
            .into_iter()
            .collect()
    }
}

fn resolve_bind_nodes<'a, 'input>(
    data_root: &Node<'a, 'input>,
    data_node: &Node<'a, 'input>,
    bind_ref: &str,
) -> Vec<Node<'a, 'input>> {
    let mut path = bind_ref.trim();
    if path.is_empty() {
        return Vec::new();
    }

    let mut current = if let Some(rest) = path.strip_prefix("$record.") {
        path = rest;
        vec![*data_root]
    } else if path == "$record" {
        return vec![*data_root];
    } else if let Some(rest) = path.strip_prefix("$.") {
        path = rest;
        vec![*data_node]
    } else {
        vec![*data_node]
    };

    let segments: Vec<&str> = path
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return current;
    }

    for segment in segments {
        let (name, selector) = parse_bind_segment(segment);
        if name.is_empty() {
            continue;
        }

        let mut next = Vec::new();
        for node in current {
            let matches: Vec<Node<'a, 'input>> = node
                .children()
                .filter(|child| child.is_element() && child.tag_name().name() == name)
                .collect();
            match selector {
                BindSelector::First => {
                    if let Some(first) = matches.into_iter().next() {
                        next.push(first);
                    }
                }
                BindSelector::All => next.extend(matches),
                BindSelector::Index(idx) => {
                    if let Some(found) = matches.into_iter().nth(idx) {
                        next.push(found);
                    }
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    current
}

#[derive(Clone, Copy)]
enum BindSelector {
    First,
    All,
    Index(usize),
}

fn parse_bind_segment(segment: &str) -> (&str, BindSelector) {
    let Some(start) = segment.find('[') else {
        return (segment, BindSelector::First);
    };
    let name = &segment[..start];
    let index = segment[start + 1..]
        .strip_suffix(']')
        .unwrap_or_default()
        .trim();
    match index {
        "*" => (name, BindSelector::All),
        "" => (name, BindSelector::First),
        _ => index
            .parse::<usize>()
            .map(|idx| (name, BindSelector::Index(idx)))
            .unwrap_or((name, BindSelector::First)),
    }
}

/// Find a direct child element by name.
fn find_child_element_by_name<'a, 'input>(
    node: &Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children()
        .filter(|n| n.is_element())
        .find(|n| n.tag_name().name() == name)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively add child form nodes (subform, field, draw, pageSet, pageArea).
///
/// When a `<breakBefore>` element appears between content children (inline
/// break), it is propagated as `page_break_before` on the next content
/// sibling's metadata.
///
/// Returns `(break_found, target_name)` if a pending break remains (i.e. a
/// `breakBefore` was found after the last content child), meaning the NEXT
/// sibling at the parent level should receive the break.
fn add_children(
    tree: &mut FormTree,
    node: &mut FormNode,
    elem: Node<'_, '_>,
) -> std::result::Result<(bool, Option<String>), crate::error::XfaError> {
    let mut pending_break = false;
    let mut pending_break_target = None;
    let mut pending_ca_break = false;
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        match tag {
            // XFA Spec 3.3 §2.1 — Container elements that produce form nodes.
            "subform" | "field" | "draw" | "pageSet" | "pageArea" | "exclGroup" | "area" => {
                let (child_id, (trailing_break, trailing_target)) = parse_node(tree, child, false)?;
                if pending_break {
                    let meta = tree.meta_mut(child_id);
                    meta.page_break_before = true;
                    if meta.break_target.is_none() {
                        meta.break_target = pending_break_target.take();
                    }
                    pending_break = false;
                }
                if pending_ca_break {
                    tree.meta_mut(child_id).content_area_break = true;
                    pending_ca_break = false;
                }
                node.children.push(child_id);
                if trailing_break {
                    pending_break = true;
                    pending_break_target = trailing_target;
                }
            }
            "breakBefore" => {
                let target_type = attr(child, "targetType");
                if target_type == Some("pageArea") {
                    pending_break = true;
                    pending_break_target = attr(child, "target").map(|s| s.to_string());
                } else if target_type == Some("contentArea") {
                    pending_ca_break = true;
                }
            }
            // Legacy <break> element between content children.
            "break"
                if attr(child, "before") == Some("pageArea")
                    && attr(child, "targetType") == Some("pageArea") =>
            {
                pending_break = true;
                pending_break_target = attr(child, "target").map(|s| s.to_string());
            }
            // Ignore XML elements that are layout metadata, not form nodes.
            // XFA Spec 3.3 §28.1 (p1229) — Adobe Non-conformance: stipple rate only
            // supports 25, 50, 75; others→100. Blends with WHITE, not bg color.
            // Currently not rendered; matches Adobe's limited implementation.
            "caption" | "value" | "ui" | "font" | "border" | "margin" | "para" | "format"
            | "items" | "medium" | "contentArea" | "desc" | "occur" | "event" | "bind"
            | "calculate" | "validate" | "assist" | "toolTip" | "fill" | "edge" | "corner"
            | "linear" | "radial" | "pattern" | "stipple" | "color" | "extras" | "traversal"
            | "proto" | "overflow" => {
                // Handled elsewhere or not needed for layout.
            }
            _ => {
                // Unknown element — skip silently.
            }
        }
    }
    Ok((pending_break, pending_break_target))
}

fn blank_node(tag: &str) -> FormNode {
    FormNode {
        name: tag.to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    }
}

/// Parse the `layout` attribute into a `LayoutStrategy`.
///
/// XFA Spec 3.3 §2.6 — Layout Strategies: positioned (fixed x,y) and
/// flowing (tb, lr-tb, rl-tb, table, row). `pageArea` uses positioned only.
/// Default for subforms without an explicit layout attribute is "position".
fn parse_layout_attr(elem: Node<'_, '_>) -> LayoutStrategy {
    match attr(elem, "layout").unwrap_or("") {
        "tb" => LayoutStrategy::TopToBottom,
        "lr-tb" => LayoutStrategy::LeftToRightTB,
        "rl-tb" => LayoutStrategy::RightToLeftTB,
        "table" => LayoutStrategy::Table,
        "row" => LayoutStrategy::Row,
        "paginate" => LayoutStrategy::TopToBottom, // root layout
        "position" => LayoutStrategy::Positioned,
        _ => LayoutStrategy::Positioned,
    }
}

/// Parse the `anchorType` attribute (XFA 3.3 §2.6, App A p1510).
///
/// Determines which anchor point of the element is placed at (x,y) in
/// positioned layout.  Default is `topLeft`.
fn parse_anchor_type(elem: Node<'_, '_>) -> AnchorType {
    match attr(elem, "anchorType").unwrap_or("") {
        "topCenter" => AnchorType::TopCenter,
        "topRight" => AnchorType::TopRight,
        "middleLeft" => AnchorType::MiddleLeft,
        "middleCenter" => AnchorType::MiddleCenter,
        "middleRight" => AnchorType::MiddleRight,
        "bottomLeft" => AnchorType::BottomLeft,
        "bottomCenter" => AnchorType::BottomCenter,
        "bottomRight" => AnchorType::BottomRight,
        _ => AnchorType::TopLeft,
    }
}

/// Parse dimensional attributes (w, h, x, y) into a `BoxModel`.
///
/// XFA Spec 3.3 §2.6 — Box Model (p49): nominal extent is w × h.
/// Margins lie inside the nominal extent. Borders lie inside margins.
/// Caption may occupy part of the nominal content region.
/// Constraints: minW/minH/maxW/maxH (§2.6 p53).
///
/// TODO(§2.6): rotate not parsed — counter-clockwise rotation in degrees (multiples of 90).
fn parse_box_model(elem: Node<'_, '_>) -> BoxModel {
    let w = attr(elem, "w").and_then(parse_dim);
    let h = attr(elem, "h").and_then(parse_dim);
    let x = attr(elem, "x").and_then(parse_dim).unwrap_or(0.0);
    let y = attr(elem, "y").and_then(parse_dim).unwrap_or(0.0);
    let min_h = attr(elem, "minH").and_then(parse_dim).unwrap_or(0.0);
    let min_w = attr(elem, "minW").and_then(parse_dim).unwrap_or(0.0);
    let max_h = attr(elem, "maxH").and_then(parse_dim).unwrap_or(f64::MAX);
    let max_w = attr(elem, "maxW").and_then(parse_dim).unwrap_or(f64::MAX);
    let margins = parse_margin(elem);

    BoxModel {
        width: w,
        height: h,
        x,
        y,
        margins,
        min_width: min_w,
        max_width: max_w,
        min_height: min_h,
        max_height: max_h,
        ..Default::default()
    }
}

fn parse_margin(elem: Node<'_, '_>) -> Insets {
    if let Some(margin) = find_first_child_by_name(elem, "margin") {
        Insets {
            top: attr(margin, "topInset").and_then(parse_dim).unwrap_or(0.0),
            bottom: attr(margin, "bottomInset")
                .and_then(parse_dim)
                .unwrap_or(0.0),
            left: attr(margin, "leftInset").and_then(parse_dim).unwrap_or(0.0),
            right: attr(margin, "rightInset")
                .and_then(parse_dim)
                .unwrap_or(0.0),
        }
    } else {
        Insets::default()
    }
}

/// Parse an XFA dimension string ("0.5in", "72pt", "10mm") to PDF points.
fn parse_dim(s: &str) -> Option<f64> {
    // Handle bare numbers as inches (common in XFA)
    if s.trim().parse::<f64>().is_ok() {
        return Measurement::parse(&format!("{s}in")).map(|m| m.to_points());
    }
    Measurement::parse(s).map(|m| m.to_points())
}

// XFA Spec 3.3 §17 "occur" (p800-802) — Specifies min/max/initial occurrences:
// min: minimum instances (default 1)
// max: maximum instances (-1 means unlimited)
// initial: number of instances at initialization
fn parse_occur(elem: Node<'_, '_>) -> Occur {
    if let Some(occur) = find_first_child_by_name(elem, "occur") {
        let min: u32 = attr(occur, "min").and_then(|s| s.parse().ok()).unwrap_or(1);
        let max: Option<u32> = attr(occur, "max")
            .map(|s| if s == "-1" { None } else { s.parse().ok() })
            .unwrap_or(Some(1));
        // XFA 3.3 §3.2.5: when initial is absent, default to at least 1 —
        // the subform exists in the template and should render once unless
        // explicitly suppressed by initial="0".
        let initial: u32 = attr(occur, "initial")
            .and_then(|s| s.parse().ok())
            .unwrap_or(min.max(1));
        Occur::repeating(min, max, initial)
    } else {
        Occur::once()
    }
}

/// Parse `colSpan` attribute.
fn parse_col_span(elem: Node<'_, '_>) -> i32 {
    attr(elem, "colSpan")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// Read the `<medium>` child and return (page_width, page_height) in points.
fn read_medium(page_area: Node<'_, '_>) -> (f64, f64) {
    if let Some(m) = find_first_child_by_name(page_area, "medium") {
        // XFA: short = narrow dimension, long = tall dimension.
        let short = attr(m, "short").and_then(parse_dim).unwrap_or(612.0);
        let long_ = attr(m, "long").and_then(parse_dim).unwrap_or(792.0);
        (short, long_)
    } else {
        (612.0, 792.0)
    }
}

/// Read all `<contentArea>` children and return their `ContentArea` structs.
fn read_content_areas(
    page_area: Node<'_, '_>,
    page_width: f64,
    page_height: f64,
) -> Vec<ContentArea> {
    let mut areas = Vec::new();
    for child in page_area.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "contentArea" {
            // XFA 3.3 §8.3.1 — contentArea x/y default to 0 when omitted.
            // Treating missing coordinates as a 0.5in inset shifts the entire
            // page content down/right for templates that define full-page
            // content areas with only w/h.
            let x = attr(child, "x").and_then(parse_dim).unwrap_or(0.0);
            let y = attr(child, "y").and_then(parse_dim).unwrap_or(0.0);
            // w/h default to full page dimensions when omitted, matching the
            // behavior of the empty (no contentArea) fallback. Previously used
            // hardcoded 540×720 (US Letter body), which was inconsistent with
            // the no-contentArea path and had no spec basis.
            let w = attr(child, "w").and_then(parse_dim).unwrap_or(page_width);
            let h = attr(child, "h").and_then(parse_dim).unwrap_or(page_height);
            areas.push(ContentArea {
                name: attr(child, "name").unwrap_or("").to_string(),
                x,
                y,
                width: w,
                height: h,
                leader: None,
                trailer: None,
            });
        }
    }
    if areas.is_empty() {
        areas.push(ContentArea {
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: page_width,
            height: page_height,
            leader: None,
            trailer: None,
        });
    }
    areas
}

/// Extract text from `<value><text>…</text></value>` or `<value><float>…</float></value>`.
///
/// Also handles `<value><exData contentType="text/html">…</exData></value>` by
/// stripping the HTML/XHTML markup and returning the concatenated plain text.
/// This covers XFA draw elements whose content is rich-text (e.g. IRS form
/// instructions stored as inline XHTML). (#557)
/// Extract image data from `<value><image contentType="image/…">…</image></value>`.
///
/// Returns `(raw_image_data, mime_type)` for supported image types:
/// - `image/jpeg` → JPEG bytes
/// - `image/png` → PNG bytes
/// - `image/bmp` → converted to PNG (PDF doesn't support BMP natively)
///
/// BMP images (magic bytes `0x42 0x4D`) are automatically converted to PNG
/// regardless of the declared `contentType`.
fn extract_value_image(elem: Node<'_, '_>) -> Option<(Vec<u8>, String)> {
    let value = find_first_child_by_name(elem, "value")?;
    let image = find_first_child_by_name(value, "image")?;
    let content_type = attr(image, "contentType")
        .unwrap_or("image/png")
        .to_string();
    let data = image.text().unwrap_or_default();
    let decoded = base64_decode(data);

    // BMP is not supported by PDF — convert to PNG.
    // Detect by magic bytes (0x42 0x4D = "BM") or declared content type.
    if decoded.starts_with(b"BM") || content_type == "image/bmp" {
        if let Some(png_data) = bmp_to_png(&decoded) {
            return Some((png_data, "image/png".to_string()));
        }
        // Conversion failed — log and skip this image.
        log::warn!("BMP to PNG conversion failed; skipping image");
        return None;
    }

    Some((decoded, content_type))
}

/// Convert BMP image data to PNG format.
fn bmp_to_png(bmp_data: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(bmp_data, image::ImageFormat::Bmp).ok()?;
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .ok()?;
    Some(buf)
}

fn extract_value_text(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    // Try <text>, <float>, <integer>, <date>
    for tag in &["text", "float", "integer", "date", "dateTime", "decimal"] {
        if let Some(child) = find_first_child_by_name(value, tag) {
            let text = child.text().unwrap_or("");
            let trimmed = text.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
            let trimmed = trimmed.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    // Fall back to <exData contentType="text/html|text/xml|…"> — collect all
    // descendant text nodes and join them, stripping the XHTML markup.
    if let Some(ex) = find_first_child_by_name(value, "exData") {
        let text = extract_text_from_descendants(ex);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// XFA Spec 3.3 §2.1 (p24) — Draw element: container for fixed content (boilerplate).
/// Contains text, lines, rectangles, arcs, or images that remain unchanged.
/// Also handles `<value><image>` for embedded image data (§2.3 p41-42).
fn extract_draw_content(elem: Node<'_, '_>) -> Option<DrawContent> {
    let value = find_first_child_by_name(elem, "value")?;

    if let Some(line) = find_first_child_by_name(value, "line") {
        let x1 = attr_as_f64(line, "x1").unwrap_or(0.0);
        let y1 = attr_as_f64(line, "y1").unwrap_or(0.0);
        let x2 = attr_as_f64(line, "x2").unwrap_or(0.0);
        let y2 = attr_as_f64(line, "y2").unwrap_or(0.0);
        return Some(DrawContent::Line { x1, y1, x2, y2 });
    }

    if let Some(rect) = find_first_child_by_name(value, "rectangle") {
        let x = attr_as_f64(rect, "x").unwrap_or(0.0);
        let y = attr_as_f64(rect, "y").unwrap_or(0.0);
        let w = attr_as_f64(rect, "w").unwrap_or(attr_as_f64(rect, "width").unwrap_or(0.0));
        let h = attr_as_f64(rect, "h").unwrap_or(attr_as_f64(rect, "height").unwrap_or(0.0));
        let radius =
            attr_as_f64(rect, "r").unwrap_or(attr_as_f64(rect, "cornerRadius").unwrap_or(0.0));
        return Some(DrawContent::Rectangle { x, y, w, h, radius });
    }

    if let Some(arc) = find_first_child_by_name(value, "arc") {
        let x = attr_as_f64(arc, "x").unwrap_or(0.0);
        let y = attr_as_f64(arc, "y").unwrap_or(0.0);
        let w = attr_as_f64(arc, "w").unwrap_or(attr_as_f64(arc, "width").unwrap_or(0.0));
        let h = attr_as_f64(arc, "h").unwrap_or(attr_as_f64(arc, "height").unwrap_or(0.0));
        let start_angle = attr_as_f64(arc, "startAngle").unwrap_or(0.0);
        let sweep_angle = attr_as_f64(arc, "sweepAngle").unwrap_or(0.0);
        return Some(DrawContent::Arc {
            x,
            y,
            w,
            h,
            start_angle,
            sweep_angle,
        });
    }

    None
}

/// Walk all descendant nodes of `node` and return text content joined
/// by newlines between block-level elements (e.g. `<p>`, `<div>`), preserving
/// paragraph structure. Used to extract plain text from XHTML-encoded
/// `<exData>` nodes. (#686)
fn extract_text_from_descendants(node: Node<'_, '_>) -> String {
    let block_tags = [
        "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr", "br",
    ];
    let mut result = String::new();
    let mut last_was_block = false;

    for desc in node.descendants() {
        let is_block = desc.is_element() && block_tags.contains(&desc.tag_name().name());

        if is_block && !result.is_empty() {
            result.push('\n');
            last_was_block = true;
        }

        if desc.is_text() {
            if let Some(t) = desc.text() {
                let t = t.trim();
                if !t.is_empty() {
                    if last_was_block || result.is_empty() {
                        result.push_str(t);
                    } else {
                        result.push(' ');
                        result.push_str(t);
                    }
                    last_was_block = false;
                }
            }
        }
    }

    result.trim().to_string()
}

fn base64_decode(input: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input.trim())
        .unwrap_or_default()
}

/// Extract the dominant font size from `<exData contentType="text/html">` HTML.
///
/// Scans `<span style="font-size:Xpt">` attributes and returns the first
/// (typically dominant) font size in points.  Returns `None` when the element
/// has no exData HTML or no font-size is specified.
fn extract_exdata_font_size(elem: Node<'_, '_>) -> Option<f64> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    // Walk all descendant elements looking for style="...font-size:Xpt..."
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc
            .attribute("style")
            .or_else(|| desc.attribute("Style"))?;
        // Parse font-size from CSS style string
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-size:")
                .or_else(|| part.strip_prefix("font-size :"))
            {
                let val = val.trim();
                if let Some(pt) = val.strip_suffix("pt") {
                    if let Ok(size) = pt.trim().parse::<f64>() {
                        if size > 0.0 {
                            return Some(size);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse caption from `<caption placement="..." reserve="...">` element.
///
/// XFA Spec 3.3 §2.6 (p51) — Captions: reserve is a height for top/bottom
/// placement and a width for left/right placement. When reserve is absent
/// or zero, the layout processor calculates the minimum size.
/// Note: Acrobat only renders captions on button and barcode fields (§2.1 p32 Note).
fn parse_caption(elem: Node<'_, '_>) -> Option<Caption> {
    let cap_elem = find_first_child_by_name(elem, "caption")?;
    // Skip captions with presence="hidden"/"invisible"/"inactive".
    if is_hidden(cap_elem) {
        return None;
    }
    let text = extract_value_text(cap_elem)?;
    if text.is_empty() {
        return None;
    }
    let placement = match attr(cap_elem, "placement") {
        Some("right") => CaptionPlacement::Right,
        Some("top") => CaptionPlacement::Top,
        Some("bottom") => CaptionPlacement::Bottom,
        Some("inline") => CaptionPlacement::Inline,
        _ => CaptionPlacement::Left,
    };
    let reserve = attr(cap_elem, "reserve")
        .and_then(Measurement::parse)
        .map(|m| m.to_points());
    Some(Caption {
        placement,
        reserve,
        text,
        font_family: None,
        font_size: None,
    })
}

/// Get an attribute value by local name, ignoring namespace prefixes.
fn attr<'a>(elem: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elem.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

fn attr_as_f64(elem: Node<'_, '_>, name: &str) -> Option<f64> {
    attr(elem, name)?.parse().ok()
}

/// Find the first direct child element with a given local tag name.
fn find_first_child_by_name<'a, 'input>(
    elem: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    elem.children()
        .filter(|n| n.is_element())
        .find(|n| n.tag_name().name() == name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb" w="7.5in">
      <field name="firstName" w="3.5in" h="0.3in">
        <caption><value><text>First Name</text></value></caption>
        <ui><textEdit/></ui>
        <value><text/></value>
      </field>
      <field name="lastName" w="3.5in" h="0.3in">
        <caption><value><text>Last Name</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>Default</text></value>
      </field>
    </subform>
  </subform>
</template>
</xdp:xdp>"#;

    #[test]
    fn parse_simple_form() {
        let (tree, root_id) = parse_template(SIMPLE_TEMPLATE, None).unwrap();
        let root = tree.get(root_id);
        // Root should have children (the paginate subform)
        assert!(!root.children.is_empty(), "root has no children");
    }

    #[test]
    fn field_with_default_value() {
        let (tree, root_id) = parse_template(SIMPLE_TEMPLATE, None).unwrap();
        // Walk to find lastName field
        let found = find_node_by_name(&tree, root_id, "lastName");
        assert!(found.is_some(), "lastName field not found");
        if let Some(n) = found {
            match &n.node_type {
                FormNodeType::Field { value } => assert_eq!(value, "Default"),
                other => panic!("expected Field, got {other:?}"),
            }
        }
    }

    #[test]
    fn unlimited_occur_expands_all_dataset_instances_in_order() {
        let template = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      </pageArea>
    </pageSet>
    <subform name="items" layout="tb" w="7in">
      <subform name="row" layout="tb" w="7in">
        <occur min="0" max="-1"/>
        <field name="value" w="2in" h="0.3in">
          <ui><textEdit/></ui>
          <value><text/></value>
        </field>
      </subform>
    </subform>
  </subform>
</template>"#;
        let datasets = r#"<?xml version="1.0" encoding="UTF-8"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <items>
        <row><value>A</value></row>
        <row><value>B</value></row>
        <row><value>C</value></row>
      </items>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let (tree, root_id) = parse_template(template, Some(datasets)).unwrap();
        let items_id = find_node_id_by_name(&tree, root_id, "items").unwrap();
        let row_ids = tree.get(items_id).children.clone();

        assert_eq!(row_ids.len(), 3);
        assert!(row_ids
            .iter()
            .all(|&row_id| tree.get(row_id).occur.count() == 1));

        let values: Vec<String> = row_ids
            .iter()
            .map(|&row_id| {
                let field_id = tree.get(row_id).children[0];
                match &tree.get(field_id).node_type {
                    FormNodeType::Field { value } => value.clone(),
                    other => panic!("expected Field, got {other:?}"),
                }
            })
            .collect();

        assert_eq!(values, vec!["A", "B", "C"]);
    }

    #[test]
    fn explicit_dataref_bind_repeats_subform_instances() {
        let template = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
      </pageArea>
    </pageSet>
    <subform name="items" layout="tb" w="7in">
      <subform name="entryRow" layout="tb" w="7in">
        <occur min="0" max="-1"/>
        <bind match="dataRef" ref="$.item[*]"/>
        <field name="label" w="2in" h="0.3in">
          <bind match="dataRef" ref="$.value"/>
          <ui><textEdit/></ui>
          <value><text/></value>
        </field>
      </subform>
    </subform>
  </subform>
</template>"#;
        let datasets = r#"<?xml version="1.0" encoding="UTF-8"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <items>
        <item><value>One</value></item>
        <item><value>Two</value></item>
        <item><value>Three</value></item>
      </items>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let (tree, root_id) = parse_template(template, Some(datasets)).unwrap();
        let items_id = find_node_id_by_name(&tree, root_id, "items").unwrap();
        let row_ids = tree.get(items_id).children.clone();

        assert_eq!(row_ids.len(), 3);
        assert!(row_ids
            .iter()
            .all(|&row_id| tree.get(row_id).occur.count() == 1));

        let values: Vec<String> = row_ids
            .iter()
            .map(|&row_id| {
                let field_id = tree.get(row_id).children[0];
                match &tree.get(field_id).node_type {
                    FormNodeType::Field { value } => value.clone(),
                    other => panic!("expected Field, got {other:?}"),
                }
            })
            .collect();

        assert_eq!(values, vec!["One", "Two", "Three"]);
    }

    #[test]
    fn dimension_parsing() {
        assert!((parse_dim("0.5in").unwrap() - 36.0).abs() < 0.01);
        assert!((parse_dim("72pt").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_dim("1in").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_dim("8.5in").unwrap() - 612.0).abs() < 0.1);
        assert!((parse_dim("11in").unwrap() - 792.0).abs() < 0.1);
    }

    #[test]
    fn layout_attr_parsing() {
        assert_eq!(parse_layout_str("tb"), LayoutStrategy::TopToBottom);
        assert_eq!(parse_layout_str("lr-tb"), LayoutStrategy::LeftToRightTB);
        assert_eq!(parse_layout_str("paginate"), LayoutStrategy::TopToBottom);
        assert_eq!(parse_layout_str("position"), LayoutStrategy::Positioned);
    }

    fn parse_layout_str(s: &str) -> LayoutStrategy {
        let xml = format!(
            r#"<?xml version="1.0"?><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform layout="{s}"/></template>"#
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        let subform = root.children().find(|n| n.is_element()).unwrap();
        parse_layout_attr(subform)
    }

    fn find_node_by_name<'a>(
        tree: &'a FormTree,
        id: FormNodeId,
        name: &str,
    ) -> Option<&'a FormNode> {
        let node = tree.get(id);
        if node.name == name {
            return Some(node);
        }
        for &child_id in &node.children {
            if let Some(found) = find_node_by_name(tree, child_id, name) {
                return Some(found);
            }
        }
        None
    }

    fn find_node_id_by_name(tree: &FormTree, id: FormNodeId, name: &str) -> Option<FormNodeId> {
        if tree.get(id).name == name {
            return Some(id);
        }
        for &child_id in &tree.get(id).children.clone() {
            if let Some(found) = find_node_id_by_name(tree, child_id, name) {
                return Some(found);
            }
        }
        None
    }

    /// <exData contentType="text/html"> rich-text draw nodes must have their
    /// HTML stripped and plain text extracted so LayoutEngine can render them.
    #[test]
    fn draw_exdata_html_text_extracted() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="instructions" w="7in" h="1in">
        <value>
          <exData contentType="text/html">
            <body xmlns="http://www.w3.org/1999/xhtml">
              <p>Do <span>not</span> file this form.</p>
            </body>
          </exData>
        </value>
      </draw>
    </subform>
  </subform>
</template>"#;
        let (tree, root_id) = parse_template(xml, None).unwrap();
        let node =
            find_node_by_name(&tree, root_id, "instructions").expect("instructions draw not found");
        match &node.node_type {
            FormNodeType::Draw(DrawContent::Text(content)) => {
                assert!(
                    content.contains("not") && content.contains("file"),
                    "expected HTML text extracted, got: {content:?}"
                );
            }
            other => panic!("expected Draw, got {other:?}"),
        }
    }

    /// Draw and field elements with presence="hidden" must not expose content
    /// to the renderer (content should be empty) while still occupying layout
    /// space (node is still present in the tree). (#557)
    #[test]
    fn hidden_elements_have_empty_content() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="visible_draw" w="7in" h="0.5in">
        <value><text>Visible text</text></value>
      </draw>
      <draw name="hidden_draw" w="7in" h="0.5in" presence="hidden">
        <value><text>DRAFT</text></value>
      </draw>
      <field name="hidden_field" w="3in" h="0.3in" presence="hidden">
        <value><text>secret</text></value>
      </field>
    </subform>
  </subform>
</template>"#;
        let (tree, root_id) = parse_template(xml, None).unwrap();

        // Visible draw retains its content.
        let visible = find_node_by_name(&tree, root_id, "visible_draw").unwrap();
        match &visible.node_type {
            FormNodeType::Draw(DrawContent::Text(content)) => assert_eq!(content, "Visible text"),
            other => panic!("expected Draw, got {other:?}"),
        }

        // Hidden draw preserves content (scripts may make it visible).
        // Visibility is tracked in FormNodeMeta.
        let hidden_draw = find_node_by_name(&tree, root_id, "hidden_draw").unwrap();
        match &hidden_draw.node_type {
            FormNodeType::Draw(DrawContent::Text(content)) => assert_eq!(content, "DRAFT"),
            other => panic!("expected Draw, got {other:?}"),
        }
        let hidden_draw_id = find_node_id_by_name(&tree, root_id, "hidden_draw").unwrap();
        assert!(tree.meta(hidden_draw_id).presence.is_not_visible());

        // Hidden fields preserve content and remain Field type — layout
        // engine skips them via metadata.
        let hidden_field = find_node_by_name(&tree, root_id, "hidden_field").unwrap();
        match &hidden_field.node_type {
            FormNodeType::Field { value } => assert_eq!(value, "secret"),
            other => panic!("expected Field, got {other:?}"),
        }
        let hidden_field_id = find_node_id_by_name(&tree, root_id, "hidden_field").unwrap();
        assert!(tree.meta(hidden_field_id).presence.is_not_visible());
    }

    /// Font sizes given as bare numbers (`<font size="10">`) must be treated as
    /// **points**, not inches.  A bare "10" used to go through `parse_dim` which
    /// added the "in" suffix, turning 10pt → 720pt and making text enormous. (#557)
    #[test]
    fn font_size_bare_number_is_points() {
        // parse_font_size must not multiply by 72.
        assert_eq!(parse_font_size("10"), Some(10.0));
        assert_eq!(parse_font_size("8"), Some(8.0));
        assert_eq!(parse_font_size("12"), Some(12.0));
        // Explicit unit must still work.
        assert!((parse_font_size("10pt").unwrap() - 10.0).abs() < 0.01);
        // Zero/negative → None.
        assert_eq!(parse_font_size("0"), None);
    }

    /// `<para hAlign="center/right/justify">` on draw/field elements must be
    /// reflected in `FontMetrics.text_align` so the renderer can align text
    /// within the element's bounding box. (#557)
    #[test]
    fn para_halign_parsed_into_font_metrics() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="left_draw" w="7in" h="0.5in">
        <value><text>Left</text></value>
        <para hAlign="left"/>
      </draw>
      <draw name="center_draw" w="7in" h="0.5in">
        <value><text>Centered</text></value>
        <para hAlign="center"/>
      </draw>
      <draw name="right_draw" w="7in" h="0.5in">
        <value><text>Right</text></value>
        <para hAlign="right"/>
      </draw>
    </subform>
  </subform>
</template>"#;
        let (tree, root_id) = parse_template(xml, None).unwrap();

        let left = find_node_by_name(&tree, root_id, "left_draw").unwrap();
        assert_eq!(
            left.font.text_align,
            TextAlign::Left,
            "left_draw should be Left"
        );

        let center = find_node_by_name(&tree, root_id, "center_draw").unwrap();
        assert_eq!(
            center.font.text_align,
            TextAlign::Center,
            "center_draw should be Center"
        );

        let right = find_node_by_name(&tree, root_id, "right_draw").unwrap();
        assert_eq!(
            right.font.text_align,
            TextAlign::Right,
            "right_draw should be Right"
        );
    }

    /// BMP images in `<image contentType="image/bmp">` must be converted to PNG.
    /// PDF does not support BMP natively, so the parser converts on extraction. (#670)
    #[test]
    fn bmp_image_converted_to_png() {
        // Minimal 1×1 BMP (24-bit, no compression): 58 bytes.
        let bmp_bytes: [u8; 58] = [
            0x42, 0x4D, // "BM" magic
            0x3A, 0x00, 0x00, 0x00, // file size = 58
            0x00, 0x00, 0x00, 0x00, // reserved
            0x36, 0x00, 0x00, 0x00, // pixel data offset = 54
            0x28, 0x00, 0x00, 0x00, // DIB header size = 40
            0x01, 0x00, 0x00, 0x00, // width = 1
            0x01, 0x00, 0x00, 0x00, // height = 1
            0x01, 0x00, // planes = 1
            0x18, 0x00, // bits per pixel = 24
            0x00, 0x00, 0x00, 0x00, // compression = 0
            0x04, 0x00, 0x00, 0x00, // image size = 4 (1 pixel + 1 byte padding)
            0x13, 0x0B, 0x00, 0x00, // h-res
            0x13, 0x0B, 0x00, 0x00, // v-res
            0x00, 0x00, 0x00, 0x00, // colors
            0x00, 0x00, 0x00, 0x00, // important colors
            0xFF, 0x00, 0x00,
            0x00, // pixel (BGR: blue=FF, green=0, red=0) + 1 byte row padding
        ];
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bmp_bytes);

        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="barcode_img" w="2in" h="0.5in">
        <value>
          <image contentType="image/bmp">{b64}</image>
        </value>
      </draw>
    </subform>
  </subform>
</template>"#
        );
        let (tree, root_id) = parse_template(&xml, None).unwrap();
        let node = find_node_by_name(&tree, root_id, "barcode_img").expect("barcode_img not found");
        match &node.node_type {
            FormNodeType::Image { data, mime_type } => {
                assert_eq!(mime_type, "image/png", "BMP should be converted to PNG");
                // PNG magic bytes: 0x89 P N G
                assert!(
                    data.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
                    "expected PNG magic bytes, got {:?}",
                    &data[..4.min(data.len())]
                );
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn parse_percentage_values() {
        assert!((parse_percentage("96%").unwrap() - 0.96).abs() < 1e-10);
        assert!((parse_percentage("110%").unwrap() - 1.10).abs() < 1e-10);
        assert!((parse_percentage("100%").unwrap() - 1.0).abs() < 1e-10);
        assert!((parse_percentage("50%").unwrap() - 0.50).abs() < 1e-10);
        assert!(parse_percentage("notanumber%").is_none());
        assert!(parse_percentage("96").is_none()); // no % suffix
    }

    #[test]
    fn parse_letter_spacing_values() {
        let font_size = 10.0;
        // em-based
        let v = parse_letter_spacing("-0.018em", font_size).unwrap();
        assert!((v - (-0.018 * 10.0)).abs() < 1e-10);
        let v = parse_letter_spacing("0.1em", font_size).unwrap();
        assert!((v - 1.0).abs() < 1e-10);
        // bare zero
        assert_eq!(parse_letter_spacing("0", font_size), Some(0.0));
        // pt-based (via Measurement)
        let v = parse_letter_spacing("0.5pt", font_size).unwrap();
        assert!((v - 0.5).abs() < 0.01);
    }

    #[test]
    fn parse_font_color_attr_hex6() {
        assert_eq!(parse_font_color_attr("#000080"), Some((0, 0, 128)));
        assert_eq!(parse_font_color_attr("#FF0000"), Some((255, 0, 0)));
        assert_eq!(parse_font_color_attr("#00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_font_color_attr("#ABCDEF"), Some((0xAB, 0xCD, 0xEF)));
    }

    #[test]
    fn parse_font_color_attr_hex3() {
        // #RGB shorthand: each digit is doubled (e.g. #F00 → #FF0000)
        assert_eq!(parse_font_color_attr("#F00"), Some((255, 0, 0)));
        assert_eq!(parse_font_color_attr("#0F0"), Some((0, 255, 0)));
        assert_eq!(parse_font_color_attr("#00F"), Some((0, 0, 255)));
        assert_eq!(parse_font_color_attr("#ABC"), Some((0xAA, 0xBB, 0xCC)));
    }

    #[test]
    fn parse_font_color_attr_decimal_csv() {
        assert_eq!(parse_font_color_attr("0,0,128"), Some((0, 0, 128)));
        assert_eq!(parse_font_color_attr("255, 128, 0"), Some((255, 128, 0)));
    }

    #[test]
    fn parse_font_color_attr_invalid() {
        assert_eq!(parse_font_color_attr(""), None);
        assert_eq!(parse_font_color_attr("#GG0000"), None);
        assert_eq!(parse_font_color_attr("#12345"), None);
        assert_eq!(parse_font_color_attr("not_a_color"), None);
    }

    /// `<font color="#000080">` attribute must be parsed into
    /// `style.text_color` when no `<fill><color>` child is present. (#740)
    #[test]
    fn font_color_attribute_parsed() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="blue_text" w="7in" h="0.5in">
        <value><text>Navy blue</text></value>
        <font typeface="Arial" size="10pt" color="#000080"/>
      </draw>
    </subform>
  </subform>
</template>"##;
        let (tree, root_id) = parse_template(xml, None).unwrap();
        let id = find_node_id_by_name(&tree, root_id, "blue_text").unwrap();
        let style = &tree.meta(id).style;
        assert_eq!(
            style.text_color,
            Some((0, 0, 128)),
            "font color=#000080 should parse to (0, 0, 128)"
        );
    }

    #[test]
    fn font_horizontal_scale_and_letter_spacing_parsed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="body" layout="tb" w="7.5in">
      <draw name="scaled_text" w="7in" h="0.5in">
        <value><text>Scaled</text></value>
        <font typeface="Arial" size="10pt" fontHorizontalScale="96%" letterSpacing="-0.018em"/>
      </draw>
    </subform>
  </subform>
</template>"#;
        let (tree, root_id) = parse_template(xml, None).unwrap();
        let id = find_node_id_by_name(&tree, root_id, "scaled_text").unwrap();
        let style = &tree.meta(id).style;
        assert!(
            (style.font_horizontal_scale.unwrap() - 0.96).abs() < 1e-10,
            "expected 0.96, got {:?}",
            style.font_horizontal_scale
        );
        assert!(
            (style.letter_spacing_pt.unwrap() - (-0.18)).abs() < 0.01,
            "expected -0.18pt, got {:?}",
            style.letter_spacing_pt
        );
    }

    #[test]
    fn choice_list_items_parsed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="root" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="8in" h="10in"/>
      </pageArea>
    </pageSet>
    <field name="country" w="3in" h="0.3in">
      <ui><choiceList/></ui>
      <value><text>US</text></value>
      <items>
        <text>United States</text>
        <text>United Kingdom</text>
        <text>Canada</text>
      </items>
      <items save="1">
        <text>US</text>
        <text>UK</text>
        <text>CA</text>
      </items>
    </field>
    <field name="single_items" w="3in" h="0.3in">
      <ui><choiceList/></ui>
      <value><text>Red</text></value>
      <items>
        <text>Red</text>
        <text>Green</text>
        <text>Blue</text>
      </items>
    </field>
  </subform>
</template>"#;

        let (tree, _pages) = parse_template(xml, None).unwrap();

        // Find the "country" field — should have display + save items
        let country = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "country")
            .map(|(i, _)| FormNodeId(i))
            .expect("country field not found");
        let meta = tree.meta(country);
        assert_eq!(meta.field_kind, FieldKind::Dropdown);
        assert_eq!(
            meta.display_items,
            vec!["United States", "United Kingdom", "Canada"]
        );
        assert_eq!(meta.save_items, vec!["US", "UK", "CA"]);

        // Find the "single_items" field — only display items, no save
        let single = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "single_items")
            .map(|(i, _)| FormNodeId(i))
            .expect("single_items field not found");
        let meta_s = tree.meta(single);
        assert_eq!(meta_s.display_items, vec!["Red", "Green", "Blue"]);
        assert!(meta_s.save_items.is_empty());
    }

    #[test]
    fn content_area_without_xy_defaults_to_origin() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="root" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="8in" h="10in"/>
      </pageArea>
    </pageSet>
  </subform>
</template>"#;

        let (tree, root_id) = parse_template(xml, None).unwrap();
        let page_area_id = find_node_id_by_name(&tree, root_id, "Page1").unwrap();
        let page_area = tree.get(page_area_id);

        match &page_area.node_type {
            FormNodeType::PageArea { content_areas } => {
                assert_eq!(content_areas.len(), 1);
                assert_eq!(content_areas[0].x, 0.0);
                assert_eq!(content_areas[0].y, 0.0);
                assert!((content_areas[0].width - 576.0).abs() < 0.01);
                assert!((content_areas[0].height - 720.0).abs() < 0.01);
            }
            other => panic!("expected PageArea, got {other:?}"),
        }
    }

    #[test]
    fn check_button_mark_parsed_into_style() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="root" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="8in" h="10in"/>
      </pageArea>
    </pageSet>
    <field name="agree" w="0.3in" h="0.3in">
      <ui><checkButton mark="circle"/></ui>
      <value><text>1</text></value>
    </field>
  </subform>
</template>"#;

        let (tree, root_id) = parse_template(xml, None).unwrap();
        let id = find_node_id_by_name(&tree, root_id, "agree").unwrap();
        let meta = tree.meta(id);
        assert_eq!(meta.field_kind, FieldKind::Checkbox);
        assert_eq!(meta.style.check_button_mark.as_deref(), Some("circle"));
    }

    #[test]
    fn border_widths_parsed_from_per_edge_template_border() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="root" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="8in" h="10in"/>
      </pageArea>
    </pageSet>
    <field name="amount" w="2in" h="0.3in">
      <ui><textEdit/></ui>
      <value><text>42</text></value>
      <border>
        <edge thickness="1pt"/>
        <edge thickness="2pt"/>
        <edge thickness="3pt"/>
        <edge thickness="4pt"/>
      </border>
    </field>
  </subform>
</template>"#;

        let (tree, root_id) = parse_template(xml, None).unwrap();
        let id = find_node_id_by_name(&tree, root_id, "amount").unwrap();
        let style = &tree.meta(id).style;

        assert_eq!(style.border_width_pt, Some(1.0));
        assert_eq!(style.border_widths, Some([1.0, 2.0, 3.0, 4.0]));
    }
}
