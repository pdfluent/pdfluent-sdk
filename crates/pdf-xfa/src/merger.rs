//! XFA Form DOM — the merged result of template + data.
//!
//! Implements the Form DOM from XFA 3.3 §3 and §5.
//! The Form DOM is a hierarchical tree of merged nodes, where repeating
//! subforms have been expanded based on data instances.

use crate::error::{Result, XfaError};
use roxmltree::Node;
use xfa_dom_resolver::data_dom::{DataDom, DataNodeId};
use xfa_layout_engine::form::{
    ContentArea, FieldKind, FormNode, FormNodeId, FormNodeMeta, FormNodeStyle, FormNodeType,
    FormTree, GroupKind, Occur,
};
use xfa_layout_engine::text::{FontFamily, FontMetrics};
use xfa_layout_engine::types::{
    BoxModel, Caption, CaptionPlacement, LayoutStrategy, Measurement, TextAlign,
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
            find_first_child_by_name(root_elem, "template")
                .ok_or_else(|| XfaError::PacketNotFound("no <template> element found".to_string()))?
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

        let (mut node, trailing_info) = match tag {
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
                    return self.expand_repeating_subform(elem, &name, occur, data_context);
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
                        let matches = self.data_dom.children_by_name(root, &name);
                        if let Some(&first) = matches.first() {
                            child_context = Some(first);
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

        let meta = parse_node_meta(elem);
        let id = self.form_tree.add_node_with_meta(node, meta);
        Ok((id, trailing_info))
    }

    fn expand_repeating_subform(
        &mut self,
        element: Node<'_, '_>,
        name: &str,
        occur: Occur,
        data_context: Option<DataNodeId>,
    ) -> Result<(FormNodeId, (bool, Option<String>))> {
        let data_instances = if let Some(ctx) = data_context {
            self.data_dom.children_by_name(ctx, name)
        } else if let Some(root) = self.data_dom.root() {
            self.data_dom.children_by_name(root, name)
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
            box_model: BoxModel::default(),
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

    fn parse_field(
        &mut self,
        elem: Node<'_, '_>,
        data_context: Option<DataNodeId>,
    ) -> Result<FormNode> {
        let name = attr(elem, "name").unwrap_or("").to_string();
        let bm = parse_box_model(elem);

        let mut value = extract_value_text(elem).unwrap_or_default();

        // Data binding
        if !name.is_empty() {
            if let Some(ctx) = data_context {
                let matches = self.data_dom.children_by_name(ctx, &name);
                if let Some(&val_id) = matches.first() {
                    if let Some(dv) = self.data_dom.get(val_id) {
                        if dv.is_value() {
                            value = self.data_dom.value(val_id).unwrap_or_default().to_string();
                        }
                    }
                }
            }
        }

        let caption_text = if !is_hidden(elem) {
            extract_caption_text(elem)
        } else {
            None
        };
        let mut bm_with_caption = bm.clone();
        if let Some(ref cap_text) = caption_text {
            bm_with_caption.caption = Some(Caption {
                placement: CaptionPlacement::Left,
                reserve: Some(72.0),
                text: cap_text.clone(),
            });
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

fn parse_dim(s: &str) -> Option<f64> {
    if s.trim().parse::<f64>().is_ok() {
        return Measurement::parse(&format!("{s}in")).map(|m: Measurement| m.to_points());
    }
    Measurement::parse(s).map(|m: Measurement| m.to_points())
}

fn parse_occur(elem: Node<'_, '_>) -> Occur {
    if let Some(occur) = find_first_child_by_name(elem, "occur") {
        let min: u32 = attr(occur, "min").and_then(|s| s.parse().ok()).unwrap_or(1);
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
            let text = child.text().unwrap_or("").trim().to_string();
            if !text.is_empty() {
                return Some(text);
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

fn extract_caption_text(elem: Node<'_, '_>) -> Option<String> {
    let cap = find_first_child_by_name(elem, "caption")?;
    extract_value_text(cap)
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
    let presence = attr(elem, "presence");
    let presence_hidden = matches!(
        presence,
        Some("hidden") | Some("inactive") | Some("invisible")
    );
    let presence_invisible = presence == Some("invisible");

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

    let xfa_id = attr(elem, "id").map(|s| s.to_string());
    let field_kind = detect_field_kind(elem);
    let style = parse_node_style(elem);
    let (data_bind_ref, data_bind_none) = parse_bind(elem);

    FormNodeMeta {
        xfa_id,
        presence_hidden,
        presence_invisible,
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

fn collect_event_scripts(elem: Node<'_, '_>) -> Vec<String> {
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
                if let Some(text) = script_elem.text() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        scripts.push(trimmed.to_string());
                    }
                }
            }
        } else if child_tag == "calculate" {
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
    if let Some(border) = find_first_child_by_name(elem, "border") {
        if let Some(edge) = find_first_child_by_name(border, "edge") {
            if let Some(color) = find_first_child_by_name(edge, "color") {
                if let Some(rgb) = parse_xfa_color(color) {
                    style.border_color = Some(rgb);
                }
            }
        }
        if style.bg_color.is_none() {
            if let Some(fill) = find_first_child_by_name(border, "fill") {
                if let Some(color) = find_first_child_by_name(fill, "color") {
                    if let Some(rgb) = parse_xfa_color(color) {
                        style.bg_color = Some(rgb);
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
