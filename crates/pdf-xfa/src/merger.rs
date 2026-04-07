//! XFA Form DOM — the merged result of template + data.
//!
//! Implements the Form DOM from XFA 3.3 §3 and §5.
//! The Form DOM is a hierarchical tree of merged nodes, where repeating
//! subforms have been expanded based on data instances.

use crate::error::{Result, XfaError};
use roxmltree::Node;
use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_layout_engine::form::{
    ContentArea, DrawContent, EventScript, FieldKind, FormNode, FormNodeId, FormNodeMeta,
    FormNodeStyle, FormNodeType, FormTree, GroupKind, Occur, Presence, ScriptLanguage,
};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{
    BoxModel, Caption, CaptionPlacement, Insets, LayoutStrategy, Measurement, TextAlign,
    VerticalAlign,
};

/// Merges an XFA template (XML) with data from a DataDom to produce a FormTree.
pub struct FormMerger<'a> {
    data_dom: &'a DataDom,
    form_tree: FormTree,
}

impl<'a> FormMerger<'a> {
    pub fn new(data_dom: &'a DataDom) -> Self {
        Self {
            data_dom,
            form_tree: FormTree::new(),
        }
    }

    /// Merge the template XML into a FormTree.
    pub fn merge(mut self, template_xml: &str) -> Result<(FormTree, FormNodeId)> {
        let doc = roxmltree::Document::parse(template_xml)
            .map_err(|e| XfaError::ParseFailed(format!("template XML parse error: {e}")))?;
        let root_elem = doc.root_element();

        // The packet may start with <template> directly, or may have been
        // wrapped inside <xdp:xdp>. Accept both.
        let template_elem = if root_elem.tag_name().name() == "template" {
            root_elem
        } else {
            // Find the first <template> descendant.
            find_first_child_by_name(root_elem, "template").ok_or_else(|| {
                XfaError::PacketNotFound("no <template> element found".to_string())
            })?
        };

        let (root_id, _trailing) = self.parse_node(template_elem, None, true)?;

        Ok((self.form_tree, root_id))
    }

    fn parse_node(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
        is_root: bool,
    ) -> Result<(FormNodeId, (bool, Option<String>))> {
        let tag = elem.tag_name().name();

        let (node, trailing_info) = match tag {
            "template" => {
                let mut n = self.blank_node("root", FormNodeType::Root);
                n.layout = LayoutStrategy::TopToBottom;
                let ti = self.add_children(&mut n, elem, data_context)?;
                (n, ti)
            }
            "subform" | "exclGroup" => {
                let name = attr(elem, "name").unwrap_or("").to_string();
                let layout = parse_layout_attr(elem);
                let mut bm = parse_box_model(elem);
                if layout == LayoutStrategy::TopToBottom && bm.width.is_none() && is_root {
                    bm.width = Some(612.0);
                }

                let occur = parse_occur(elem);

                // Repeating subform expansion
                if occur.is_repeating() && !name.is_empty() {
                    // Use bind ref data name if present (e.g.
                    // <bind match="dataRef" ref="$.listInitiales[*]"> →
                    // data name "listInitiales"), otherwise fall back to
                    // the subform name.
                    let data_name = parse_bind_data_name(elem).unwrap_or_else(|| name.clone());
                    return self.expand_repeating_subform(
                        elem,
                        &name,
                        &data_name,
                        occur,
                        data_context,
                    );
                }

                // Normal subform
                let mut child_context = data_context;
                if !name.is_empty() {
                    if let Some(ctx) = data_context {
                        let matches = self.data_dom.children_by_name(ctx, &name);
                        if let Some(&first) = matches.first() {
                            child_context = Some(first);
                        }
                    } else if let Some(root) = self.data_dom.root() {
                        // XFA §4.7.2: the root subform binds to the data root
                        // element if their names match. Check root name first
                        // before searching among its children.
                        if self.data_dom.get(root).is_some_and(|n| n.name() == name) {
                            child_context = Some(root);
                        } else {
                            let matches = self.data_dom.children_by_name(root, &name);
                            if let Some(&first) = matches.first() {
                                child_context = Some(first);
                            } else {
                                // Fallback: use the first child group as the
                                // context (common pattern: template root="form1"
                                // but data root child="DOCUMENT" or "MCD").
                                let children = self.data_dom.children(root);
                                if let Some(&first_child) = children.first() {
                                    if self.data_dom.get(first_child).is_some_and(|n| n.is_group())
                                    {
                                        child_context = Some(first_child);
                                    }
                                }
                            }
                        }
                    }
                }

                let mut n = FormNode {
                    name,
                    node_type: FormNodeType::Subform,
                    box_model: bm,
                    layout,
                    children: Vec::new(),
                    occur,
                    font: FontMetrics::default(),
                    calculate: None,
                    validate: None,
                    column_widths: Vec::new(),
                    col_span: 1,
                };
                let ti = self.add_children(&mut n, elem, child_context)?;
                (n, ti)
            }
            "field" => (self.parse_field(elem, data_context)?, (false, None)),
            "draw" => (self.parse_draw(elem, data_context)?, (false, None)),
            "pageSet" => (self.parse_page_set(elem, data_context)?, (false, None)),
            "pageArea" => (self.parse_page_area(elem, data_context)?, (false, None)),
            _ => {
                let mut n = self.blank_node(tag, FormNodeType::Subform);
                let ti = self.add_children(&mut n, elem, data_context)?;
                (n, ti)
            }
        };

        let mut meta = parse_node_meta(elem);
        // For draw elements with exData HTML: extract font-weight from HTML
        // styles when the XFA <font> element doesn't specify weight.
        if tag == "draw" && meta.style.font_weight.is_none() {
            if let Some(weight) = extract_exdata_font_weight(elem) {
                meta.style.font_weight = Some(weight);
            }
        }
        let id = self.form_tree.add_node_with_meta(node, meta);
        Ok((id, trailing_info))
    }

    fn expand_repeating_subform(
        &mut self,
        element: Node<'_, '_>,
        name: &str,
        data_name: &str,
        occur: Occur,
        data_context: Option<DataNodeId>,
    ) -> Result<(FormNodeId, (bool, Option<String>))> {
        let data_instances = if let Some(ctx) = data_context {
            self.data_dom.children_by_name(ctx, data_name)
        } else if let Some(root) = self.data_dom.root() {
            self.data_dom.children_by_name(root, data_name)
        } else {
            Vec::new()
        };

        let count = (data_instances.len() as u32).max(occur.min);
        let count = if let Some(max) = occur.max {
            count.min(max)
        } else {
            count
        };

        let mut instances = Vec::new();
        let layout = parse_layout_attr(element);
        let bm = parse_box_model(element);

        for i in 0..count {
            let instance_data_ctx = data_instances.get(i as usize).copied();

            let mut inst_node = FormNode {
                name: name.to_string(),
                node_type: FormNodeType::Subform,
                box_model: bm.clone(),
                layout,
                children: Vec::new(),
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: Vec::new(),
                col_span: 1,
            };

            self.add_children(&mut inst_node, element, instance_data_ctx)?;

            let meta = parse_node_meta(element);
            let inst_id = self.form_tree.add_node_with_meta(inst_node, meta);
            instances.push(inst_id);
        }

        let container = FormNode {
            name: format!("{}_container", name),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: instances,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: Vec::new(),
            col_span: 1,
        };

        let container_id = self.form_tree.add_node(container);
        Ok((container_id, (false, None)))
    }

    /// Search descendants of a data node for a DataValue with the given name.
    /// Returns the first matching value (breadth-first).
    fn find_value_in_descendants(&self, node: DataNodeId, name: &str) -> Option<String> {
        for &child in self.data_dom.children(node) {
            if let Some(cn) = self.data_dom.get(child) {
                if cn.name() == name && cn.is_value() {
                    return self.data_dom.value(child).ok().map(|s| s.to_string());
                }
            }
        }
        // Recurse into child groups
        for &child in self.data_dom.children(node) {
            if self.data_dom.get(child).is_some_and(|n| n.is_group()) {
                if let Some(val) = self.find_value_in_descendants(child, name) {
                    return Some(val);
                }
            }
        }
        None
    }

    fn parse_field(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let bm = parse_box_model(elem);

        let mut value = extract_value_text(elem).unwrap_or_default();

        // Data binding: try current context first, then walk up to root
        // (XFA §4.7.2 global data binding fallback).
        if !name.is_empty() {
            if let Some(ctx) = data_context {
                let matches = self.data_dom.children_by_name(ctx, &name);
                if let Some(&val_id) = matches.first() {
                    if let Some(dv) = self.data_dom.get(val_id) {
                        if dv.is_value() {
                            value = self.data_dom.value(val_id).unwrap_or_default().to_string();
                        }
                    }
                } else if let Some(root) = self.data_dom.root() {
                    // Fallback: search descendants of data root for a matching
                    // value node. This handles data saved in a flat structure
                    // while the template uses nested subforms.
                    if let Some(val) = self.find_value_in_descendants(root, &name) {
                        value = val;
                    }
                }
            }
        }

        let mut bm_with_caption = bm.clone();
        if !is_hidden(elem) {
            if let Some(cap) = parse_caption(elem) {
                bm_with_caption.caption = Some(cap);
            }
        }

        let mut font = parse_font_metrics(elem);
        if let Some(html_size) = extract_exdata_font_size(elem) {
            font.size = html_size;
        }

        Ok(FormNode {
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
        })
    }

    fn parse_draw(
        &mut self,
        elem: Node<'_, '_>,
        _data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let bm = parse_box_model(elem);
        let content = extract_value_text(elem).unwrap_or_default();

        let mut font = parse_font_metrics(elem);
        if let Some(html_size) = extract_exdata_font_size(elem) {
            font.size = html_size;
        }

        Ok(FormNode {
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
        })
    }

    fn parse_page_set(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
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
        for child in elem.children().filter(|n| n.is_element()) {
            if child.tag_name().name() == "pageArea" {
                let (child_id, _) = self.parse_node(child, data_context, false)?;
                node.children.push(child_id);
            }
        }
        Ok(node)
    }

    fn parse_page_area(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let (page_w, page_h) = read_medium(elem);
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
        self.add_children(&mut node, elem, data_context)?;
        Ok(node)
    }

    fn add_children(
        &mut self,
        node: &mut FormNode,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<(bool, Option<String>)> {
        let mut pending_break = false;
        let mut pending_break_target = None;
        let mut pending_ca_break = false;

        for child in elem.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            match tag {
                "subform" | "field" | "draw" | "pageSet" | "pageArea" | "exclGroup" => {
                    let (child_id, (trailing_break, trailing_target)) =
                        self.parse_node(child, data_context, false)?;
                    if pending_break {
                        let meta = self.form_tree.meta_mut(child_id);
                        meta.page_break_before = true;
                        if meta.break_target.is_none() {
                            meta.break_target = pending_break_target.take();
                        }
                        pending_break = false;
                    }
                    if pending_ca_break {
                        self.form_tree.meta_mut(child_id).content_area_break = true;
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
                "break" => {
                    if attr(child, "before") == Some("pageArea") {
                        pending_break = true;
                        pending_break_target = attr(child, "target").map(|s| s.to_string());
                    }
                }
                _ => {}
            }
        }
        Ok((pending_break, pending_break_target))
    }

    fn blank_node(&self, name: &str, node_type: FormNodeType) -> FormNode {
        FormNode {
            name: name.to_string(),
            node_type,
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
}

// ---------------------------------------------------------------------------
// Helper functions (mirrored from template_parser.rs)
// ---------------------------------------------------------------------------

fn attr<'a>(elem: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elem.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

fn find_first_child_by_name<'a, 'input>(
    elem: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    elem.children()
        .filter(|n| n.is_element())
        .find(|n| n.tag_name().name() == name)
}

fn parse_layout_attr(elem: Node<'_, '_>) -> LayoutStrategy {
    match attr(elem, "layout").unwrap_or("") {
        "tb" => LayoutStrategy::TopToBottom,
        "lr-tb" => LayoutStrategy::LeftToRightTB,
        "rl-tb" => LayoutStrategy::RightToLeftTB,
        "table" => LayoutStrategy::Table,
        "row" => LayoutStrategy::Row,
        "paginate" => LayoutStrategy::TopToBottom,
        "position" => LayoutStrategy::Positioned,
        _ => LayoutStrategy::Positioned,
    }
}

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

fn parse_dim(s: &str) -> Option<f64> {
    if s.trim().parse::<f64>().is_ok() {
        return Measurement::parse(&format!("{s}in")).map(|m: Measurement| m.to_points());
    }
    Measurement::parse(s).map(|m: Measurement| m.to_points())
}

/// Parse a percentage string like `"96%"` → `0.96`, `"110%"` → `1.1`.
fn parse_percentage(s: &str) -> Option<f64> {
    let s = s.trim();
    let num_str = s.strip_suffix('%')?;
    let v: f64 = num_str.trim().parse().ok()?;
    Some(v / 100.0)
}

/// Parse a letter-spacing string (`"-0.018em"`, `"0.5pt"`, etc.) to points.
fn parse_letter_spacing(s: &str, font_size_pt: f64) -> Option<f64> {
    let s = s.trim();
    if s == "0" {
        return Some(0.0);
    }
    if let Some(num_str) = s.strip_suffix("em") {
        let v: f64 = num_str.trim().parse().ok()?;
        return Some(v * font_size_pt);
    }
    Measurement::parse(s).map(|m| m.to_points())
}

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
            .unwrap_or(min);
        Occur::repeating(min, max, initial)
    } else {
        Occur::once()
    }
}

fn parse_font_size(s: &str) -> Option<f64> {
    if let Ok(v) = s.trim().parse::<f64>() {
        return if v > 0.0 { Some(v) } else { None };
    }
    Measurement::parse(s).map(|m: Measurement| m.to_points())
}

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

fn read_medium(page_area: Node<'_, '_>) -> (f64, f64) {
    if let Some(m) = find_first_child_by_name(page_area, "medium") {
        let short = attr(m, "short").and_then(parse_dim).unwrap_or(612.0);
        let long_ = attr(m, "long").and_then(parse_dim).unwrap_or(792.0);
        (short, long_)
    } else {
        (612.0, 792.0)
    }
}

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

fn extract_value_text(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
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
    if let Some(ex) = find_first_child_by_name(value, "exData") {
        let text = extract_text_from_descendants(ex);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

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

fn extract_exdata_font_size(elem: Node<'_, '_>) -> Option<f64> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc
            .attribute("style")
            .or_else(|| desc.attribute("Style"))?;
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

/// Extract the dominant `font-weight` from `<exData contentType="text/html">` styles.
/// Returns `Some("bold")` when the first styled `<p>` or `<span>` has `font-weight:bold`.
fn extract_exdata_font_weight(elem: Node<'_, '_>) -> Option<String> {
    let value = find_first_child_by_name(elem, "value")?;
    let ex = find_first_child_by_name(value, "exData")?;
    for desc in ex.descendants() {
        if !desc.is_element() {
            continue;
        }
        let style = desc.attribute("style")?;
        for part in style.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("font-weight:")
                .or_else(|| part.strip_prefix("font-weight :"))
            {
                let val = val.trim();
                if val == "bold" {
                    return Some("bold".to_string());
                }
            }
        }
    }
    None
}

fn parse_caption(elem: Node<'_, '_>) -> Option<Caption> {
    let cap_elem = find_first_child_by_name(elem, "caption")?;
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
    })
}

fn is_hidden(elem: Node<'_, '_>) -> bool {
    matches!(
        attr(elem, "presence"),
        Some("hidden") | Some("invisible") | Some("inactive")
    )
}

fn parse_col_span(elem: Node<'_, '_>) -> i32 {
    attr(elem, "colSpan")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

fn parse_node_meta(elem: Node<'_, '_>) -> FormNodeMeta {
    let tag = elem.tag_name().name();
    let presence = match attr(elem, "presence") {
        Some("hidden") => Presence::Hidden,
        Some("invisible") => Presence::Invisible,
        Some("inactive") => Presence::Inactive,
        _ => Presence::Visible,
    };

    let (page_break_before, break_before_target) = detect_page_break_before(elem);
    let (page_break_after, break_after_target) = detect_page_break_after(elem);
    let content_area_break = detect_content_area_break(elem);
    let break_target = break_before_target.or(break_after_target);

    let event_scripts = collect_event_scripts(elem);
    let (keep_next_content_area, keep_previous_content_area, keep_intact_content_area) =
        parse_keep(elem);
    let (overflow_leader, overflow_trailer) = parse_overflow(elem);

    let group_kind = if tag == "exclGroup" {
        GroupKind::ExclusiveChoice
    } else {
        GroupKind::None
    };

    let item_value = if tag == "field" {
        parse_item_value(elem)
    } else {
        None
    };

    let (display_items, save_items) = if tag == "field" {
        parse_items_lists(elem)
    } else {
        (Vec::new(), Vec::new())
    };

    let xfa_id = attr(elem, "id").map(|s| s.to_string());
    let field_kind = detect_field_kind(elem);
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
        display_items,
        save_items,
        ..Default::default()
    }
}

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

fn detect_page_break_after(elem: Node<'_, '_>) -> (bool, Option<String>) {
    let mut last_content_idx = 0;
    let children: Vec<_> = elem.children().filter(|n| n.is_element()).collect();
    for (i, child) in children.iter().enumerate() {
        let tag = child.tag_name().name();
        if matches!(tag, "subform" | "field" | "draw" | "exclGroup") {
            last_content_idx = i;
        }
    }

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

fn detect_content_area_break(elem: Node<'_, '_>) -> bool {
    for child in elem.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if tag == "breakBefore" && attr(child, "targetType") == Some("contentArea") {
            return true;
        }
    }
    false
}

fn collect_event_scripts(elem: Node<'_, '_>) -> Vec<EventScript> {
    let mut scripts = Vec::new();
    for child in elem.children().filter(|n| n.is_element()) {
        let child_tag = child.tag_name().name();
        if child_tag == "event" {
            let activity = attr(child, "activity");
            let event_ref = attr(child, "ref");
            if activity == Some("ready") && event_ref == Some("$layout") {
                continue;
            }
            if let Some(script_elem) = find_first_child_by_name(child, "script") {
                if let Some(script) =
                    build_event_script(script_elem, activity, event_ref, attr(script_elem, "runAt"))
                {
                    scripts.push(script);
                }
            }
        } else if child_tag == "calculate" {
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

fn parse_overflow(elem: Node<'_, '_>) -> (Option<String>, Option<String>) {
    if let Some(overflow) = find_first_child_by_name(elem, "overflow") {
        let leader = attr(overflow, "leader").map(|s| s.to_string());
        let trailer = attr(overflow, "trailer").map(|s| s.to_string());
        (leader, trailer)
    } else {
        (None, None)
    }
}

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

fn parse_node_style(elem: Node<'_, '_>) -> FormNodeStyle {
    let mut style = FormNodeStyle::default();
    if let Some(fill) = find_first_child_by_name(elem, "fill") {
        if !is_hidden(fill) {
            if let Some(color) = find_first_child_by_name(fill, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.bg_color = Some(rgb);
                }
            }
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
            let stroke = attr(edge, "stroke").unwrap_or("solid");
            if stroke != "none" {
                let thickness = attr(edge, "thickness")
                    .and_then(Measurement::parse)
                    .map(|m: Measurement| m.to_points())
                    .unwrap_or(0.5);
                if thickness > 0.0 {
                    style.border_width_pt = Some(thickness);
                }
            }
        }
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
    if let Some(font) = find_first_child_by_name(elem, "font") {
        if let Some(typeface) = attr(font, "typeface") {
            style.font_family = Some(typeface.to_string());
        }
        if let Some(size_str) = attr(font, "size") {
            if let Some(m) = Measurement::parse(size_str) {
                let m: Measurement = m;
                style.font_size = Some(m.to_points());
            }
        }
        if let Some(weight) = attr(font, "weight") {
            style.font_weight = Some(weight.to_string());
        }
        if let Some(posture) = attr(font, "posture") {
            style.font_style = Some(posture.to_string());
        }
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
    }

    // Parse <para> for paragraph attributes (XFA 3.3 §D.7).
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
        if let Some(va) = attr(para, "vAlign") {
            style.v_align = Some(match va {
                "middle" => VerticalAlign::Middle,
                "bottom" => VerticalAlign::Bottom,
                _ => VerticalAlign::Top,
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

    // Parse <caption> for caption text, placement, and reserve (XFA 3.3 §7.4).
    if let Some(cap) = parse_caption(elem) {
        style.caption_text = Some(cap.text);
        style.caption_placement = Some(
            match cap.placement {
                CaptionPlacement::Left => "left",
                CaptionPlacement::Right => "right",
                CaptionPlacement::Top => "top",
                CaptionPlacement::Bottom => "bottom",
                CaptionPlacement::Inline => "inline",
            }
            .to_string(),
        );
        style.caption_reserve = cap.reserve;
    }

    style
}

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

/// Extract the data field name from a bind ref like `$.listInitiales[*]`.
/// Returns `Some("listInitiales")` for that example, or None if no bind/ref.
fn parse_bind_data_name(elem: Node<'_, '_>) -> Option<String> {
    let bind = find_first_child_by_name(elem, "bind")?;
    let ref_val = attr(bind, "ref")?;
    // Typical refs: "$.fieldName", "$.fieldName[*]", "$record.fieldName"
    // Extract the last dot-separated segment, strip any trailing [*] etc.
    let segment = ref_val.rsplit('.').next().unwrap_or(ref_val);
    let name = segment.split('[').next().unwrap_or(segment).trim();
    if name.is_empty() || name == "$" {
        None
    } else {
        Some(name.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_dom_resolver::data_dom::DataDom;

    #[test]
    fn repeating_subform_expands_from_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Orders" layout="tb" w="500pt">
      <subform name="Order" layout="position" w="500pt" h="60pt">
        <occur min="0" max="10" initial="1"/>
        <field name="Item" w="200pt" h="20pt" x="0pt" y="0pt"/>
        <field name="Qty" w="100pt" h="20pt" x="200pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form1>
      <Order><Item>Widget A</Item><Qty>5</Qty></Order>
      <Order><Item>Widget B</Item><Qty>3</Qty></Order>
      <Order><Item>Widget C</Item><Qty>7</Qty></Order>
    </form1>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let orders_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Orders")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let container_id = tree.get(orders_id).children[0];
        let container = tree.get(container_id);
        assert_eq!(container.name, "Order_container");
        assert_eq!(container.children.len(), 3);
    }

    /// Double-wrapped <xfa:data> must be unwrapped for data binding to work.
    #[test]
    fn repeating_subform_double_wrapped_data() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="tb">
    <pageSet>
      <pageArea name="Page">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="Orders" layout="tb" w="559pt">
      <subform name="CoreOrders" layout="tb" w="361pt">
        <subform name="Order" layout="position" w="360pt" h="162pt">
          <occur min="0" max="3" initial="1"/>
          <field name="Item" w="200pt" h="25pt" x="9pt" y="9pt"/>
        </subform>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = concat!(
            r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">"#,
            r#"<xfa:data><xfa:data xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">"#,
            r#"<form1>"#,
            r#"<Order><Item>A</Item></Order>"#,
            r#"<Order><Item>B</Item></Order>"#,
            r#"<Order><Item>C</Item></Order>"#,
            r#"</form1>"#,
            r#"</xfa:data></xfa:data></xfa:datasets>"#,
        );

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let container_id = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.name == "Order_container")
            .map(|(i, _)| FormNodeId(i))
            .unwrap();
        let container = tree.get(container_id);
        assert_eq!(
            container.children.len(),
            3,
            "Expected 3 Order instances from double-wrapped data"
        );
    }

    /// Bind ref resolution: when subform name differs from data name,
    /// <bind match="dataRef" ref="$.dataName[*]"> should use the data name.
    #[test]
    fn repeating_subform_bind_ref_resolves_data_name() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <subform name="ListItems" layout="tb" w="500pt">
      <subform name="ItemGroup" layout="tb" w="500pt" h="50pt">
        <occur min="0" max="-1"/>
        <bind match="dataRef" ref="$.itemGroup[*]"/>
        <field name="title" w="200pt" h="20pt" x="0pt" y="0pt"/>
      </subform>
    </subform>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <itemGroup><title>Group A</title></itemGroup>
      <itemGroup><title>Group B</title></itemGroup>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let container = tree
            .nodes
            .iter()
            .find(|n| n.name == "ItemGroup_container")
            .expect("ItemGroup_container must exist");
        assert_eq!(
            container.children.len(),
            2,
            "Expected 2 instances from bind ref $.itemGroup[*]"
        );
    }

    /// Root subform binds to data root when names match (XFA §4.7.2).
    #[test]
    fn root_subform_binds_to_matching_data_root() {
        let template = r#"<?xml version="1.0"?>
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form" layout="tb">
    <pageSet>
      <pageArea name="Page1">
        <contentArea w="595pt" h="842pt"/>
        <medium short="595pt" long="842pt"/>
      </pageArea>
    </pageSet>
    <field name="title" w="200pt" h="20pt" x="0pt" y="0pt"/>
    <field name="code" w="200pt" h="20pt" x="0pt" y="20pt"/>
  </subform>
</template>"#;

        let data_xml = r#"<?xml version="1.0"?>
<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
  <xfa:data>
    <form>
      <title>Hello World</title>
      <code>42</code>
    </form>
  </xfa:data>
</xfa:datasets>"#;

        let data_dom = DataDom::from_xml(data_xml).unwrap();
        let merger = FormMerger::new(&data_dom);
        let (tree, _root_id) = merger.merge(template).unwrap();

        let title_node = tree
            .nodes
            .iter()
            .find(|n| n.name == "title")
            .expect("title field must exist");
        match &title_node.node_type {
            FormNodeType::Field { value } => {
                assert_eq!(value, "Hello World", "title should bind to data root");
            }
            _ => panic!("title should be a field"),
        }
    }
}
