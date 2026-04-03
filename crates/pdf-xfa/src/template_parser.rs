//! XDP template XML → FormTree parser.
//!
//! Reads the `<template>` packet from an XFA XDP document and builds a
//! `FormTree` suitable for `LayoutEngine::layout()`.
//!
//! Supported elements: subform, field, draw, pageSet, pageArea,
//! contentArea, medium, exclGroup, caption, value, ui.
//!
//! Dimension strings ("0.5in", "72pt", "10mm") are converted to PDF
//! points via `Measurement::parse`.

use roxmltree::Node;

use xfa_layout_engine::form::{
    ContentArea, FieldKind, FormNode, FormNodeId, FormNodeMeta, FormNodeStyle, FormNodeType,
    FormTree, GroupKind, Occur, Presence,
};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{
    BoxModel, Caption, CaptionPlacement, Insets, LayoutStrategy, Measurement, TextAlign,
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
        "subform" | "exclGroup" => {
            let mut n = parse_subform_node(tree, elem, is_root)?;
            let ti = add_children(tree, &mut n, elem)?;
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

    let meta = parse_node_meta(elem);
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

    // Extract caption text (from <caption><value><text>…</text></value>).
    let caption_text = if !is_hidden(elem) {
        extract_caption_text(elem)
    } else {
        None
    };
    let mut bm_with_caption = bm.clone();
    if let Some(ref cap_text) = caption_text {
        bm_with_caption.caption = Some(Caption {
            placement: CaptionPlacement::Left,
            reserve: Some(72.0), // ~1 inch for caption by default
            text: cap_text.clone(),
        });
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
        node_type: FormNodeType::Draw { content },
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

/// Parse font size and text alignment from `<font size="…">` and `<para hAlign="…">` child
/// elements (XFA 3.3 §7.1). Returns `FontMetrics::default()` when no matching elements found.
fn parse_font_metrics(elem: Node<'_, '_>) -> FontMetrics {
    let font_elem = find_first_child_by_name(elem, "font");
    let size = font_elem
        .and_then(|f| attr(f, "size"))
        .and_then(parse_font_size)
        .unwrap_or(FontMetrics::default().size);
    let typeface = font_elem
        .and_then(|f| attr(f, "typeface"))
        .map(FontFamily::from_typeface)
        .unwrap_or_default();
    let text_align = find_first_child_by_name(elem, "para")
        .and_then(|p| attr(p, "hAlign"))
        .map(|a| match a {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            "justify" => TextAlign::Justify,
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
    let content_areas = read_content_areas(elem);

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

    // (a) Presence attribute (XFA 3.3 S3.2.8).
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

    // (h) XFA id attribute.
    let xfa_id = attr(elem, "id").map(|s| s.to_string());

    // (i) Field UI kind: detect <checkButton>, <choiceList>, etc. inside <ui>.
    let field_kind = detect_field_kind(elem);

    // (j) Visual style: colors, borders, font from XFA template elements.
    let style = parse_node_style(elem);
    let (data_bind_ref, data_bind_none) = parse_bind(elem);

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
    let mut style = FormNodeStyle::default();

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
        if let Some(edge) = find_first_child_by_name(border, "edge") {
            if let Some(color) = find_first_child_by_name(edge, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.border_color = Some(rgb);
                }
            }
            // Parse edge thickness; default 0.5pt per XFA 3.3 spec §D.7.
            let stroke = attr(edge, "stroke").unwrap_or("solid");
            if stroke != "none" {
                let thickness = attr(edge, "thickness")
                    .and_then(Measurement::parse)
                    .map(|m| m.to_points())
                    .unwrap_or(0.5);
                if thickness > 0.0 {
                    style.border_width_pt = Some(thickness);
                }
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

    // Parse <font typeface="..." size="..." weight="..."> for font properties.
    if let Some(font) = find_first_child_by_name(elem, "font") {
        if let Some(typeface) = attr(font, "typeface") {
            style.font_family = Some(typeface.to_string());
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
    }

    style
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

/// Detect field UI type from `<ui>` child element.
fn detect_field_kind(elem: Node<'_, '_>) -> FieldKind {
    let Some(ui) = find_first_child_by_name(elem, "ui") else {
        return FieldKind::Text;
    };
    for child in ui.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "checkButton" => return FieldKind::Checkbox,
            "choiceList" => return FieldKind::Dropdown,
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
        if tag == "breakBefore" {
            if attr(child, "targetType") == Some("pageArea") {
                return (true, attr(child, "target").map(|s| s.to_string()));
            }
        }
        if tag == "break" && attr(child, "before") == Some("pageArea") {
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
        if tag == "breakAfter" {
            if attr(*child, "targetType") == Some("pageArea") {
                return (true, attr(*child, "target").map(|s| s.to_string()));
            }
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
fn collect_event_scripts(elem: Node<'_, '_>) -> Vec<String> {
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
                if let Some(text) = script_elem.text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        scripts.push(trimmed.to_string());
                    }
                }
            }
        } else if child_tag == "calculate" {
            // Direct <calculate><script>...</script></calculate>
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(text) = script_elem.text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        scripts.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    scripts
}

/// Parse `<keep>` child element attributes.
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

/// Parse `<overflow>` child element leader/trailer.
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
    let new_id = tree.add_node_with_meta(new_node, source_meta);
    new_id
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
            "subform" | "field" | "draw" | "pageSet" | "pageArea" | "exclGroup" => {
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
            "break" => {
                if attr(child, "before") == Some("pageArea") {
                    pending_break = true;
                    pending_break_target = attr(child, "target").map(|s| s.to_string());
                }
            }
            // Ignore XML elements that are layout metadata, not form nodes.
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

/// Parse dimensional attributes (w, h, x, y) into a `BoxModel`.
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

/// Parse the `occur` child element.
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
fn read_content_areas(page_area: Node<'_, '_>) -> Vec<ContentArea> {
    let mut areas = Vec::new();
    for child in page_area.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "contentArea" {
            let x = attr(child, "x").and_then(parse_dim).unwrap_or(36.0);
            let y = attr(child, "y").and_then(parse_dim).unwrap_or(36.0);
            let w = attr(child, "w").and_then(parse_dim).unwrap_or(540.0);
            let h = attr(child, "h").and_then(parse_dim).unwrap_or(720.0);
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
        // Default content area: US Letter with 0.5in margins.
        areas.push(ContentArea {
            name: String::new(),
            x: 36.0,
            y: 36.0,
            width: 540.0,
            height: 720.0,
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
    let decoded = base64_decode(&data);

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

/// Extract caption text from `<caption><value><text>…</text></value></caption>`.
fn extract_caption_text(elem: Node<'_, '_>) -> Option<String> {
    let cap = find_first_child_by_name(elem, "caption")?;
    extract_value_text(cap)
}

/// Get an attribute value by local name, ignoring namespace prefixes.
fn attr<'a>(elem: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elem.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
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
        let subform = root.children().filter(|n| n.is_element()).next().unwrap();
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
            FormNodeType::Draw { content } => {
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
            FormNodeType::Draw { content } => assert_eq!(content, "Visible text"),
            other => panic!("expected Draw, got {other:?}"),
        }

        // Hidden draw preserves content (scripts may make it visible).
        // Visibility is tracked in FormNodeMeta.
        let hidden_draw = find_node_by_name(&tree, root_id, "hidden_draw").unwrap();
        match &hidden_draw.node_type {
            FormNodeType::Draw { content } => assert_eq!(content, "DRAFT"),
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
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);

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
}
