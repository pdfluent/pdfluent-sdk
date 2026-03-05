//! XFA Template XML → FormTree parser with data merge.
//!
//! Parses the XFA template packet (XFA 3.3 §17) into a `FormTree` and
//! merges field values from the datasets packet. This bridges the gap
//! between raw XFA XML and the layout engine's form tree representation.

use crate::error::{PdfError, Result};
use xfa_layout_engine::form::{ContentArea, FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, Insets, LayoutStrategy};

/// Parse an XFA template XML string into a `FormTree`.
///
/// Optionally merges data values from a datasets XML string.
pub fn parse_template(
    template_xml: &str,
    datasets_xml: Option<&str>,
) -> Result<(FormTree, FormNodeId)> {
    let doc = roxmltree::Document::parse(template_xml)
        .map_err(|e| PdfError::XmlParse(format!("template: {e}")))?;

    let data_doc = datasets_xml
        .map(|xml| {
            roxmltree::Document::parse(xml)
                .map_err(|e| PdfError::XmlParse(format!("datasets: {e}")))
        })
        .transpose()?;

    let mut tree = FormTree::new();
    let mut parser = TemplateParser {
        tree: &mut tree,
        data_doc: data_doc.as_ref(),
    };

    let root_element = doc.root_element();
    let root_id = parser.parse_element(&root_element, &[])?;

    Ok((tree, root_id))
}

/// Extracts the `<xfa:data>` element from a datasets XML document.
fn find_data_root<'a>(doc: &'a roxmltree::Document<'a>) -> Option<roxmltree::Node<'a, 'a>> {
    let root = doc.root_element();
    // datasets packet: <xfa:datasets><xfa:data>...</xfa:data></xfa:datasets>
    for child in root.children().filter(|n| n.is_element()) {
        let name = child.tag_name().name();
        if name == "data" {
            return Some(child);
        }
    }
    // Maybe the root IS the data element
    if root.tag_name().name() == "data" {
        return Some(root);
    }
    None
}

/// Looks up a data value by walking a path of element names from a data root.
fn lookup_data_value<'a>(data_root: &roxmltree::Node<'a, 'a>, path: &[&str]) -> Option<String> {
    if path.is_empty() {
        return data_root.text().map(|s| s.to_string());
    }

    let first = path[0];
    let rest = &path[1..];

    for child in data_root.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == first {
            if rest.is_empty() {
                // Leaf: return text content
                return child.text().map(|s| s.to_string());
            } else {
                // Recurse deeper
                return lookup_data_value(&child, rest);
            }
        }
    }
    None
}

/// Finds the data group node matching a given name path.
fn find_data_group<'a>(
    data_root: &roxmltree::Node<'a, 'a>,
    path: &[&str],
) -> Option<roxmltree::Node<'a, 'a>> {
    if path.is_empty() {
        return Some(*data_root);
    }
    let first = path[0];
    let rest = &path[1..];

    for child in data_root.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == first {
            return find_data_group(&child, rest);
        }
    }
    None
}

/// Counts the number of sibling data elements with a given name.
fn count_data_instances(data_group: &roxmltree::Node, name: &str) -> usize {
    data_group
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == name)
        .count()
}

struct TemplateParser<'a, 'b> {
    tree: &'a mut FormTree,
    data_doc: Option<&'b roxmltree::Document<'b>>,
}

impl<'a, 'b> TemplateParser<'a, 'b> {
    fn parse_element(
        &mut self,
        element: &roxmltree::Node,
        data_path: &[&str],
    ) -> Result<FormNodeId> {
        let tag = element.tag_name().name();
        match tag {
            "subform" => self.parse_subform(element, data_path),
            "field" => self.parse_field(element, data_path),
            "draw" => self.parse_draw(element, data_path),
            "pageSet" => self.parse_page_set(element, data_path),
            "pageArea" => self.parse_page_area(element, data_path),
            // Template root element: treat as root subform
            "template" => self.parse_template_root(element, data_path),
            _ => {
                // Unknown elements: wrap as subform container
                self.parse_subform(element, data_path)
            }
        }
    }

    fn parse_template_root(
        &mut self,
        element: &roxmltree::Node,
        data_path: &[&str],
    ) -> Result<FormNodeId> {
        let mut children = Vec::new();
        for child in element.children().filter(|n| n.is_element()) {
            let child_id = self.parse_element(&child, data_path)?;
            children.push(child_id);
        }

        let root = self.tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::TopToBottom,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        Ok(root)
    }

    fn parse_subform(
        &mut self,
        element: &roxmltree::Node,
        data_path: &[&str],
    ) -> Result<FormNodeId> {
        let name = element.attribute("name").unwrap_or("").to_string();
        let box_model = parse_box_model(element);
        let layout = parse_layout_strategy(element);
        let occur = parse_occur(element);
        let column_widths = parse_column_widths(element);
        let calculate = find_script(element, "calculate");
        let validate = find_script(element, "validate");

        // Extend data path for non-empty named subforms
        let child_data_path: Vec<&str> = if !name.is_empty() {
            let mut p = data_path.to_vec();
            p.push(&name);
            p
        } else {
            data_path.to_vec()
        };

        // Handle repeating subforms: check data for instance count
        if occur.is_repeating() {
            return self.parse_repeating_subform(
                element,
                &name,
                &box_model,
                &layout,
                &occur,
                &column_widths,
                &calculate,
                &validate,
                data_path,
            );
        }

        let mut children = Vec::new();
        for child in element.children().filter(|n| n.is_element()) {
            let child_tag = child.tag_name().name();
            // Skip non-form elements
            if matches!(
                child_tag,
                "subform" | "field" | "draw" | "pageSet" | "pageArea" | "subformSet" | "area"
            ) {
                let child_id = self.parse_element(&child, &child_data_path)?;
                children.push(child_id);
            }
        }

        let id = self.tree.add_node(FormNode {
            name,
            node_type: FormNodeType::Subform,
            box_model,
            layout,
            children,
            occur,
            font: FontMetrics::default(),
            calculate,
            validate,
            column_widths,
            col_span: 1,
        });

        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_repeating_subform(
        &mut self,
        element: &roxmltree::Node,
        name: &str,
        box_model: &BoxModel,
        layout: &LayoutStrategy,
        occur: &Occur,
        column_widths: &[f64],
        calculate: &Option<String>,
        validate: &Option<String>,
        parent_data_path: &[&str],
    ) -> Result<FormNodeId> {
        // For repeating subforms, we create a single node (the caller manages siblings).
        // Check data for how many instances exist.
        let data_group = self
            .data_doc
            .and_then(|doc| find_data_root(doc))
            .and_then(|root| find_data_group(&root, parent_data_path));

        let instance_count = data_group
            .as_ref()
            .map(|g| count_data_instances(g, name).max(occur.count() as usize))
            .unwrap_or(occur.count() as usize);

        // Parse children using first instance data (index 0)
        let mut children = Vec::new();
        let child_data_path: Vec<&str> = {
            let mut p = parent_data_path.to_vec();
            p.push(name);
            p
        };
        for child in element.children().filter(|n| n.is_element()) {
            let child_tag = child.tag_name().name();
            if matches!(
                child_tag,
                "subform" | "field" | "draw" | "pageSet" | "pageArea" | "subformSet" | "area"
            ) {
                let child_id = self.parse_element(&child, &child_data_path)?;
                children.push(child_id);
            }
        }

        // Create the node with adjusted occur to reflect actual instance count
        let adjusted_occur = Occur::repeating(occur.min, occur.max, instance_count as u32);

        let id = self.tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: box_model.clone(),
            layout: *layout,
            children,
            occur: adjusted_occur,
            font: FontMetrics::default(),
            calculate: calculate.clone(),
            validate: validate.clone(),
            column_widths: column_widths.to_vec(),
            col_span: 1,
        });

        Ok(id)
    }

    fn parse_field(&mut self, element: &roxmltree::Node, data_path: &[&str]) -> Result<FormNodeId> {
        let name = element.attribute("name").unwrap_or("").to_string();
        let box_model = parse_box_model(element);
        let font = parse_font_metrics(element);
        let calculate = find_script(element, "calculate");
        let validate = find_script(element, "validate");
        let col_span = element
            .attribute("colSpan")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(1);

        // Get default value from template
        let mut value = find_field_value(element);

        // Override with data value if available
        if !name.is_empty() {
            if let Some(data_val) = self.lookup_data(&name, data_path) {
                value = data_val;
            }
        }

        let id = self.tree.add_node(FormNode {
            name,
            node_type: FormNodeType::Field { value },
            box_model,
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font,
            calculate,
            validate,
            column_widths: vec![],
            col_span,
        });

        Ok(id)
    }

    fn parse_draw(&mut self, element: &roxmltree::Node, _data_path: &[&str]) -> Result<FormNodeId> {
        let name = element.attribute("name").unwrap_or("").to_string();
        let box_model = parse_box_model(element);
        let font = parse_font_metrics(element);
        let col_span = element
            .attribute("colSpan")
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(1);

        let content = find_draw_content(element);

        let id = self.tree.add_node(FormNode {
            name,
            node_type: FormNodeType::Draw { content },
            box_model,
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font,
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span,
        });

        Ok(id)
    }

    fn parse_page_set(
        &mut self,
        element: &roxmltree::Node,
        data_path: &[&str],
    ) -> Result<FormNodeId> {
        let mut children = Vec::new();
        for child in element.children().filter(|n| n.is_element()) {
            if child.tag_name().name() == "pageArea" {
                let child_id = self.parse_page_area(&child, data_path)?;
                children.push(child_id);
            }
        }

        let id = self.tree.add_node(FormNode {
            name: "pageSet".to_string(),
            node_type: FormNodeType::PageSet,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::TopToBottom,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        Ok(id)
    }

    fn parse_page_area(
        &mut self,
        element: &roxmltree::Node,
        data_path: &[&str],
    ) -> Result<FormNodeId> {
        let name = element.attribute("name").unwrap_or("pageArea").to_string();

        // Parse content areas
        let mut content_areas = Vec::new();
        let mut children = Vec::new();

        for child in element.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            match tag {
                "contentArea" => {
                    let ca = parse_content_area(&child);
                    content_areas.push(ca);
                }
                "subform" | "field" | "draw" => {
                    let child_id = self.parse_element(&child, data_path)?;
                    children.push(child_id);
                }
                _ => {}
            }
        }

        if content_areas.is_empty() {
            // Default US Letter content area
            content_areas.push(ContentArea::default());
        }

        let id = self.tree.add_node(FormNode {
            name,
            node_type: FormNodeType::PageArea { content_areas },
            box_model: parse_box_model(element),
            layout: LayoutStrategy::TopToBottom,
            children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        Ok(id)
    }

    /// Look up a field's data value from the datasets.
    fn lookup_data(&self, field_name: &str, data_path: &[&str]) -> Option<String> {
        let doc = self.data_doc?;
        let data_root = find_data_root(doc)?;
        let mut full_path: Vec<&str> = data_path.to_vec();
        full_path.push(field_name);
        lookup_data_value(&data_root, &full_path)
    }
}

// ── Attribute parsing helpers ────────────────────────────────────

/// Parse box model dimensions from element attributes.
fn parse_box_model(element: &roxmltree::Node) -> BoxModel {
    let mut margins = Insets::default();

    // Look for margin/border/edge child elements
    for child in element.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "margin" => {
                margins.top = parse_measurement(child.attribute("topInset"));
                margins.right = parse_measurement(child.attribute("rightInset"));
                margins.bottom = parse_measurement(child.attribute("bottomInset"));
                margins.left = parse_measurement(child.attribute("leftInset"));
            }
            "para" => {
                if let Some(space) = child.attribute("spaceAbove") {
                    margins.top = parse_measurement(Some(space));
                }
                if let Some(space) = child.attribute("spaceBelow") {
                    margins.bottom = parse_measurement(Some(space));
                }
            }
            _ => {}
        }
    }

    BoxModel {
        x: parse_measurement(element.attribute("x")),
        y: parse_measurement(element.attribute("y")),
        width: parse_measurement_opt(element.attribute("w")),
        height: parse_measurement_opt(element.attribute("h")),
        margins,
        ..Default::default()
    }
}

/// Parse the layout strategy from a subform element.
fn parse_layout_strategy(element: &roxmltree::Node) -> LayoutStrategy {
    match element.attribute("layout") {
        Some("tb") => LayoutStrategy::TopToBottom,
        Some("lr-tb") => LayoutStrategy::LeftToRightTB,
        Some("rl-tb") => LayoutStrategy::RightToLeftTB,
        Some("position") => LayoutStrategy::Positioned,
        Some("table") => LayoutStrategy::Table,
        Some("row") => LayoutStrategy::Row,
        _ => LayoutStrategy::Positioned, // Default per XFA spec
    }
}

/// Parse occurrence rules from a subform element.
fn parse_occur(element: &roxmltree::Node) -> Occur {
    for child in element.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "occur" {
            let min = child
                .attribute("min")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1);
            let max = child.attribute("max").and_then(|s| {
                if s == "-1" {
                    None
                } else {
                    s.parse::<u32>().ok()
                }
            });
            let initial = child
                .attribute("initial")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(min);
            return Occur::repeating(min, max, initial);
        }
    }
    Occur::once()
}

/// Parse column widths from a table-layout subform.
fn parse_column_widths(element: &roxmltree::Node) -> Vec<f64> {
    element
        .attribute("columnWidths")
        .map(|s| {
            s.split_whitespace()
                .filter_map(|w| {
                    if w == "*" {
                        Some(-1.0)
                    } else {
                        parse_measurement_val(w)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse font metrics from a field/draw element.
fn parse_font_metrics(element: &roxmltree::Node) -> FontMetrics {
    let mut fm = FontMetrics::default();
    for child in element.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "font" {
            if let Some(size) = child.attribute("size") {
                fm.size = parse_measurement_val(size).unwrap_or(10.0);
            }
        }
    }
    fm
}

/// Find a FormCalc script (calculate or validate) in a field element.
fn find_script(element: &roxmltree::Node, script_type: &str) -> Option<String> {
    for child in element.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == script_type {
            // The script may be in a <script> child or directly as text
            for script_child in child.children() {
                if script_child.is_element() && script_child.tag_name().name() == "script" {
                    if let Some(text) = script_child.text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
                if script_child.is_text() {
                    if let Some(text) = script_child.text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find the default value of a field element.
fn find_field_value(element: &roxmltree::Node) -> String {
    for child in element.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "value" {
            // Value element can contain <text>, <integer>, <float>, etc.
            for value_child in child.children().filter(|n| n.is_element()) {
                if let Some(text) = value_child.text() {
                    return text.to_string();
                }
            }
            // Or it might contain direct text
            if let Some(text) = child.text() {
                return text.trim().to_string();
            }
        }
    }
    String::new()
}

/// Find the text content of a draw element.
fn find_draw_content(element: &roxmltree::Node) -> String {
    for child in element.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "value" {
            for value_child in child.children().filter(|n| n.is_element()) {
                let tag = value_child.tag_name().name();
                if tag == "text" || tag == "exData" {
                    // Collect all text content, including nested elements
                    return collect_text(&value_child);
                }
            }
            if let Some(text) = child.text() {
                return text.trim().to_string();
            }
        }
    }
    String::new()
}

/// Recursively collect all text content from an element.
fn collect_text(node: &roxmltree::Node) -> String {
    let mut result = String::new();
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                result.push_str(text);
            }
        } else if child.is_element() {
            result.push_str(&collect_text(&child));
        }
    }
    result
}

/// Parse a content area element.
fn parse_content_area(element: &roxmltree::Node) -> ContentArea {
    ContentArea {
        name: element
            .attribute("name")
            .unwrap_or("contentArea")
            .to_string(),
        x: parse_measurement(element.attribute("x")),
        y: parse_measurement(element.attribute("y")),
        width: parse_measurement_val(element.attribute("w").unwrap_or("612pt")).unwrap_or(612.0),
        height: parse_measurement_val(element.attribute("h").unwrap_or("792pt")).unwrap_or(792.0),
        leader: None,
        trailer: None,
    }
}

/// Parse a measurement string (e.g., "72pt", "1in", "25.4mm") to points.
fn parse_measurement(attr: Option<&str>) -> f64 {
    attr.and_then(parse_measurement_val).unwrap_or(0.0)
}

/// Parse a measurement string to an optional value in points.
fn parse_measurement_opt(attr: Option<&str>) -> Option<f64> {
    attr.and_then(parse_measurement_val)
}

/// Parse a single measurement value with unit suffix to points.
fn parse_measurement_val(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = if let Some(n) = s.strip_suffix("pt") {
        (n, "pt")
    } else if let Some(n) = s.strip_suffix("in") {
        (n, "in")
    } else if let Some(n) = s.strip_suffix("mm") {
        (n, "mm")
    } else if let Some(n) = s.strip_suffix("cm") {
        (n, "cm")
    } else if let Some(n) = s.strip_suffix("px") {
        (n, "px")
    } else if let Some(n) = s.strip_suffix("em") {
        (n, "em")
    } else {
        (s, "pt") // Default unit is points
    };

    let value: f64 = num_str.trim().parse().ok()?;

    let points = match unit {
        "pt" => value,
        "in" => value * 72.0,
        "mm" => value * 72.0 / 25.4,
        "cm" => value * 72.0 / 2.54,
        "px" => value * 0.75, // 96dpi → 72dpi
        "em" => value * 12.0, // Approximate: 1em = 12pt
        _ => value,
    };

    Some(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_template() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="Name">
                    <value><text>Default Name</text></value>
                </field>
                <field name="Amount"/>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let root_node = tree.get(root);
        assert!(matches!(root_node.node_type, FormNodeType::Root));
        assert_eq!(root_node.children.len(), 1); // form1

        let form1 = tree.get(root_node.children[0]);
        assert_eq!(form1.name, "form1");
        assert!(matches!(form1.node_type, FormNodeType::Subform));
        assert_eq!(form1.children.len(), 2);

        let name_field = tree.get(form1.children[0]);
        assert_eq!(name_field.name, "Name");
        if let FormNodeType::Field { value } = &name_field.node_type {
            assert_eq!(value, "Default Name");
        } else {
            panic!("Expected Field");
        }
    }

    #[test]
    fn parse_with_data_merge() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="Name"/>
                <field name="City"/>
            </subform>
        </template>"#;

        let datasets = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
            <xfa:data>
                <form1>
                    <Name>Alice</Name>
                    <City>Amsterdam</City>
                </form1>
            </xfa:data>
        </xfa:datasets>"#;

        let (tree, root) = parse_template(template, Some(datasets)).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let name_field = tree.get(form1.children[0]);
        let city_field = tree.get(form1.children[1]);

        if let FormNodeType::Field { value } = &name_field.node_type {
            assert_eq!(value, "Alice");
        } else {
            panic!("Expected Field");
        }
        if let FormNodeType::Field { value } = &city_field.node_type {
            assert_eq!(value, "Amsterdam");
        } else {
            panic!("Expected Field");
        }
    }

    #[test]
    fn parse_nested_subforms() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <subform name="Address">
                    <field name="Street"/>
                    <field name="Zip"/>
                </subform>
            </subform>
        </template>"#;

        let datasets = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
            <xfa:data>
                <form1><Address><Street>Main St</Street><Zip>12345</Zip></Address></form1>
            </xfa:data>
        </xfa:datasets>"#;

        let (tree, root) = parse_template(template, Some(datasets)).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let address = tree.get(form1.children[0]);
        assert_eq!(address.name, "Address");

        let street = tree.get(address.children[0]);
        if let FormNodeType::Field { value } = &street.node_type {
            assert_eq!(value, "Main St");
        } else {
            panic!("Expected Field");
        }
    }

    #[test]
    fn parse_draw_elements() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <draw name="Label1">
                    <value><text>Hello World</text></value>
                </draw>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let draw = tree.get(form1.children[0]);

        assert_eq!(draw.name, "Label1");
        if let FormNodeType::Draw { content } = &draw.node_type {
            assert_eq!(content, "Hello World");
        } else {
            panic!("Expected Draw");
        }
    }

    #[test]
    fn parse_box_model_attributes() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="F1" x="72pt" y="36pt" w="200pt" h="25pt"/>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let field = tree.get(form1.children[0]);

        assert!((field.box_model.x - 72.0).abs() < 0.01);
        assert!((field.box_model.y - 36.0).abs() < 0.01);
        assert_eq!(field.box_model.width, Some(200.0));
        assert_eq!(field.box_model.height, Some(25.0));
    }

    #[test]
    fn parse_measurement_units() {
        assert!((parse_measurement_val("72pt").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_measurement_val("1in").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_measurement_val("25.4mm").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_measurement_val("2.54cm").unwrap() - 72.0).abs() < 0.01);
        assert!((parse_measurement_val("100").unwrap() - 100.0).abs() < 0.01);
        assert!(parse_measurement_val("").is_none());
    }

    #[test]
    fn parse_layout_strategy_attribute() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1" layout="tb">
                <field name="F1"/>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        assert!(matches!(form1.layout, LayoutStrategy::TopToBottom));
    }

    #[test]
    fn parse_occur_element() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <subform name="Item">
                    <occur min="0" max="-1" initial="2"/>
                    <field name="Desc"/>
                </subform>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let item = tree.get(form1.children[0]);

        assert_eq!(item.occur.min, 0);
        assert_eq!(item.occur.max, None); // unlimited
        assert!(item.occur.is_repeating());
    }

    #[test]
    fn parse_font_metrics_from_element() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="F1">
                    <font typeface="Helvetica" size="14pt" weight="bold"/>
                </field>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let field = tree.get(form1.children[0]);

        assert!((field.font.size - 14.0).abs() < 0.01);
    }

    #[test]
    fn parse_calculate_script() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="Total">
                    <calculate><script>Subtotal * 1.21</script></calculate>
                </field>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        let total = tree.get(form1.children[0]);

        assert_eq!(total.calculate, Some("Subtotal * 1.21".to_string()));
    }

    #[test]
    fn invalid_template_xml_errors() {
        let result = parse_template("<<<not xml>>>", None);
        assert!(result.is_err());
    }

    #[test]
    fn empty_template() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/>"#;
        let (tree, root) = parse_template(template, None).unwrap();
        let root_node = tree.get(root);
        assert!(matches!(root_node.node_type, FormNodeType::Root));
        assert_eq!(root_node.children.len(), 0);
    }

    #[test]
    fn page_set_parsing() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <pageSet>
                    <pageArea name="Page1">
                        <contentArea x="36pt" y="36pt" w="540pt" h="720pt"/>
                    </pageArea>
                </pageSet>
                <field name="F1"/>
            </subform>
        </template>"#;

        let (tree, root) = parse_template(template, None).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);
        // pageSet + field = 2 children
        assert_eq!(form1.children.len(), 2);

        let page_set = tree.get(form1.children[0]);
        assert!(matches!(page_set.node_type, FormNodeType::PageSet));
    }

    #[test]
    fn data_merge_with_missing_fields() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="Exists"/>
                <field name="Missing">
                    <value><text>default</text></value>
                </field>
            </subform>
        </template>"#;

        let datasets = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
            <xfa:data>
                <form1><Exists>found</Exists></form1>
            </xfa:data>
        </xfa:datasets>"#;

        let (tree, root) = parse_template(template, Some(datasets)).unwrap();
        let form1 = tree.get(tree.get(root).children[0]);

        // Exists should have data value
        if let FormNodeType::Field { value } = &tree.get(form1.children[0]).node_type {
            assert_eq!(value, "found");
        } else {
            panic!("Expected Field");
        }

        // Missing should keep its default
        if let FormNodeType::Field { value } = &tree.get(form1.children[1]).node_type {
            assert_eq!(value, "default");
        } else {
            panic!("Expected Field");
        }
    }
}
