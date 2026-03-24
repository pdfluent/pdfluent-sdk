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

use xfa_layout_engine::form::{ContentArea, FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, Caption, CaptionPlacement, LayoutStrategy, Measurement};

use crate::error::{Result, XfaError};

/// Parse a `<template>` XML packet into a `FormTree`.
///
/// `xml` should be the raw content of the template packet, starting with
/// the `<template …>` element (with or without an XML declaration).
pub fn parse_template(xml: &str) -> Result<(FormTree, FormNodeId)> {
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
    let root_id = parse_node(&mut tree, template_elem, true)?;
    Ok((tree, root_id))
}

// ---------------------------------------------------------------------------
// Recursive node parser
// ---------------------------------------------------------------------------

fn parse_node(tree: &mut FormTree, elem: Node<'_, '_>, is_root: bool) -> Result<FormNodeId> {
    let tag = elem.tag_name().name();

    let node = match tag {
        "template" => parse_root(tree, elem)?,
        "subform" | "exclGroup" => parse_subform(tree, elem, is_root)?,
        "field" => parse_field(tree, elem)?,
        "draw" => parse_draw(tree, elem)?,
        "pageSet" => parse_page_set(tree, elem)?,
        "pageArea" => parse_page_area(tree, elem)?,
        _ => {
            // Unknown element — create a minimal placeholder so traversal
            // can continue to collect child nodes.
            let mut node = blank_node(tag);
            add_children(tree, &mut node, elem)?;
            node
        }
    };

    Ok(tree.add_node(node))
}

fn parse_root(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let mut node = FormNode {
        name: "root".to_string(),
        node_type: FormNodeType::Root,
        box_model: BoxModel::default(),
        layout: LayoutStrategy::TopToBottom,
        children: Vec::new(),
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    };
    add_children(tree, &mut node, elem)?;
    Ok(node)
}

fn parse_subform(tree: &mut FormTree, elem: Node<'_, '_>, _is_root: bool) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();
    let layout = parse_layout_attr(elem);
    let mut bm = parse_box_model(elem);
    // For paginate-layout root subforms, set a default US Letter size if
    // no explicit size is given.
    if layout == LayoutStrategy::TopToBottom && bm.width.is_none() {
        bm.width = Some(612.0);
    }

    let mut node = FormNode {
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
    };
    add_children(tree, &mut node, elem)?;
    Ok(node)
}

fn parse_field(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("").to_string();
    let bm = parse_box_model(elem);

    // Extract the field value (from <value><text>…</text></value>).
    let value = extract_value_text(elem).unwrap_or_default();

    // Extract caption text (from <caption><value><text>…</text></value>).
    let caption_text = extract_caption_text(elem);
    let mut bm_with_caption = bm.clone();
    if let Some(ref cap_text) = caption_text {
        bm_with_caption.caption = Some(Caption {
            placement: CaptionPlacement::Left,
            reserve: Some(72.0), // ~1 inch for caption by default
            text: cap_text.clone(),
        });
    }

    let node = FormNode {
        name,
        node_type: FormNodeType::Field { value },
        box_model: bm_with_caption,
        layout: LayoutStrategy::Positioned,
        children: Vec::new(),
        occur: Occur::once(),
        font: parse_font_metrics(elem),
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
    let content = extract_value_text(elem).unwrap_or_default();

    let node = FormNode {
        name,
        node_type: FormNodeType::Draw { content },
        box_model: bm,
        layout: LayoutStrategy::Positioned,
        children: Vec::new(),
        occur: Occur::once(),
        font: parse_font_metrics(elem),
        calculate: None,
        validate: None,
        column_widths: Vec::new(),
        col_span: 1,
    };
    let _ = tree;
    Ok(node)
}

/// Parse font size from `<font size="…">` child element (if present).
/// Returns `FontMetrics::default()` when no `<font>` element or `size` attr found.
fn parse_font_metrics(elem: Node<'_, '_>) -> FontMetrics {
    let size = find_first_child_by_name(elem, "font")
        .and_then(|f| attr(f, "size"))
        .and_then(parse_dim)
        .unwrap_or(FontMetrics::default().size);
    FontMetrics::new(size)
}

fn parse_page_set(tree: &mut FormTree, elem: Node<'_, '_>) -> Result<FormNode> {
    let name = attr(elem, "name").unwrap_or("pageSet").to_string();
    let mut node = FormNode {
        name,
        node_type: FormNodeType::PageSet,
        box_model: BoxModel::default(),
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
            let child_id = parse_node(tree, child, false)?;
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

    let node = FormNode {
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
    let _ = tree;
    Ok(node)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively add child form nodes (subform, field, draw, pageSet, pageArea).
fn add_children(tree: &mut FormTree, node: &mut FormNode, elem: Node<'_, '_>) -> Result<()> {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        match tag {
            "subform" | "field" | "draw" | "pageSet" | "pageArea" | "exclGroup" => {
                let child_id = parse_node(tree, child, false)?;
                node.children.push(child_id);
            }
            // Ignore XML elements that are layout metadata, not form nodes.
            "caption" | "value" | "ui" | "font" | "border" | "margin" | "para"
            | "format" | "items" | "medium" | "contentArea" | "desc" | "occur"
            | "event" | "bind" | "calculate" | "validate" | "assist" | "toolTip"
            | "fill" | "edge" | "corner" | "linear" | "radial" | "pattern"
            | "stipple" | "color" | "extras" | "traversal" | "proto" | "overflow" => {
                // Handled elsewhere or not needed for layout.
            }
            _ => {
                // Unknown element — skip silently.
            }
        }
    }
    Ok(())
}

fn blank_node(tag: &str) -> FormNode {
    FormNode {
        name: tag.to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel::default(),
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

    BoxModel {
        width: w,
        height: h,
        x,
        y,
        max_width: f64::MAX,
        max_height: f64::MAX,
        ..Default::default()
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
        let min: u32 = attr(occur, "min")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let max: Option<u32> = attr(occur, "max")
            .map(|s| if s == "-1" { None } else { s.parse().ok() })
            .unwrap_or(Some(1));
        let initial: u32 = attr(occur, "initial")
            .and_then(|s| s.parse().ok())
            .unwrap_or(min);
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
fn extract_value_text(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    // Try <text>, <float>, <integer>, <date>
    for tag in &["text", "float", "integer", "date", "dateTime", "decimal"] {
        if let Some(child) = find_first_child_by_name(value, tag) {
            let text = child.text().unwrap_or("").trim().to_string();
            if !text.is_empty() {
                return Some(text);
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

/// Walk all descendant text nodes of `node` and return their trimmed content
/// joined by spaces, with excess whitespace collapsed. Used to extract plain
/// text from XHTML-encoded `<exData>` nodes.
fn extract_text_from_descendants(node: Node<'_, '_>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for desc in node.descendants() {
        if desc.is_text() {
            if let Some(t) = desc.text() {
                let t = t.trim();
                if !t.is_empty() {
                    parts.push(t);
                }
            }
        }
    }
    parts.join(" ")
}

/// Extract caption text from `<caption><value><text>…</text></value></caption>`.
fn extract_caption_text(elem: Node<'_, '_>) -> Option<String> {
    let cap = find_first_child_by_name(elem, "caption")?;
    extract_value_text(cap)
}

/// Get an attribute value by local name, ignoring namespace prefixes.
fn attr<'a>(elem: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elem.attributes().find(|a| a.name() == name).map(|a| a.value())
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
        let (tree, root_id) = parse_template(SIMPLE_TEMPLATE).unwrap();
        let root = tree.get(root_id);
        // Root should have children (the paginate subform)
        assert!(!root.children.is_empty(), "root has no children");
    }

    #[test]
    fn field_with_default_value() {
        let (tree, root_id) = parse_template(SIMPLE_TEMPLATE).unwrap();
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
        let xml = format!(r#"<?xml version="1.0"?><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform layout="{s}"/></template>"#);
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
        let (tree, root_id) = parse_template(xml).unwrap();
        let node = find_node_by_name(&tree, root_id, "instructions")
            .expect("instructions draw not found");
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
}
