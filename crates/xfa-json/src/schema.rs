//! Schema extraction from FormTree.
//!
//! Generates a `FormSchema` describing the form's field structure,
//! types, validation rules, and repetition constraints.

use crate::types::{FieldSchema, FieldType, FormSchema};
use indexmap::IndexMap;
use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree};

/// Extract a schema from a FormTree.
///
/// Returns a `FormSchema` with an entry for every field and draw node,
/// including type hints, repetition rules, and scripts.
pub fn export_schema(tree: &FormTree, root: FormNodeId) -> FormSchema {
    let mut fields = IndexMap::new();
    let node = tree.get(root);

    match &node.node_type {
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for &child_id in &node.children {
                walk_schema(tree, child_id, "", false, &mut fields);
            }
        }
        _ => {
            walk_schema(tree, root, "", false, &mut fields);
        }
    }

    FormSchema { fields }
}

/// Recursively walk the tree collecting schema entries.
fn walk_schema(
    tree: &FormTree,
    node_id: FormNodeId,
    parent_path: &str,
    parent_repeatable: bool,
    fields: &mut IndexMap<String, FieldSchema>,
) {
    let node = tree.get(node_id);
    let path = if parent_path.is_empty() {
        node.name.clone()
    } else {
        format!("{}.{}", parent_path, node.name)
    };

    let is_repeatable = parent_repeatable || node.occur.is_repeating();

    match &node.node_type {
        FormNodeType::Field { value } => {
            let field_type = infer_field_type(value);
            fields.insert(
                path.clone(),
                FieldSchema {
                    som_path: path,
                    field_type,
                    required: node.occur.min > 0,
                    repeatable: parent_repeatable,
                    max_occurrences: node.occur.max,
                    calculate: node.calculate.clone(),
                    validate: node.validate.clone(),
                },
            );
        }
        FormNodeType::Draw { .. } => {
            fields.insert(
                path.clone(),
                FieldSchema {
                    som_path: path,
                    field_type: FieldType::Static,
                    required: false,
                    repeatable: parent_repeatable,
                    max_occurrences: Some(1),
                    calculate: None,
                    validate: None,
                },
            );
        }
        FormNodeType::Subform => {
            for &child_id in &node.children {
                walk_schema(tree, child_id, &path, is_repeatable, fields);
            }
        }
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for &child_id in &node.children {
                walk_schema(tree, child_id, &path, is_repeatable, fields);
            }
        }
    }
}

/// Infer the field type from the current value.
fn infer_field_type(value: &str) -> FieldType {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return FieldType::Text; // Unknown, default to text
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "false" | "0" | "1" => return FieldType::Boolean,
        _ => {}
    }

    if trimmed.parse::<f64>().is_ok() {
        return FieldType::Numeric;
    }

    FieldType::Text
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::{FormNode, Occur};
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

    fn make_field(
        tree: &mut FormTree,
        name: &str,
        value: &str,
        calculate: Option<&str>,
        validate: Option<&str>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Field {
                value: value.to_string(),
            },
            box_model: BoxModel::default(),
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: calculate.map(|s| s.to_string()),
            validate: validate.map(|s| s.to_string()),
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_subform(
        tree: &mut FormTree,
        name: &str,
        children: Vec<FormNodeId>,
        occur: Occur,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::TopToBottom,
            children,
            occur,
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_root(tree: &mut FormTree, children: Vec<FormNodeId>) -> FormNodeId {
        tree.add_node(FormNode {
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
        })
    }

    #[test]
    fn schema_captures_field_types() {
        let mut tree = FormTree::new();
        let name = make_field(&mut tree, "Name", "Acme", None, None);
        let amount = make_field(&mut tree, "Amount", "42.50", None, None);
        let active = make_field(&mut tree, "Active", "true", None, None);

        let form = make_subform(&mut tree, "form1", vec![name, amount, active], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let schema = export_schema(&tree, root);

        assert_eq!(
            schema.fields.get("form1.Name").unwrap().field_type,
            FieldType::Text
        );
        assert_eq!(
            schema.fields.get("form1.Amount").unwrap().field_type,
            FieldType::Numeric
        );
        assert_eq!(
            schema.fields.get("form1.Active").unwrap().field_type,
            FieldType::Boolean
        );
    }

    #[test]
    fn schema_includes_scripts() {
        let mut tree = FormTree::new();
        let tax = make_field(
            &mut tree,
            "Tax",
            "0",
            Some("Subtotal * 0.21"),
            Some("Tax >= 0"),
        );
        let form = make_subform(&mut tree, "form1", vec![tax], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let schema = export_schema(&tree, root);
        let tax_schema = schema.fields.get("form1.Tax").unwrap();

        assert_eq!(
            tax_schema.calculate,
            Some("Subtotal * 0.21".to_string())
        );
        assert_eq!(tax_schema.validate, Some("Tax >= 0".to_string()));
    }

    #[test]
    fn schema_marks_repeatable_fields() {
        let mut tree = FormTree::new();
        let desc = make_field(&mut tree, "Description", "Item", None, None);
        let item = make_subform(
            &mut tree,
            "Item",
            vec![desc],
            Occur::repeating(0, None, 1),
        );
        let form = make_subform(&mut tree, "form1", vec![item], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let schema = export_schema(&tree, root);
        let desc_schema = schema.fields.get("form1.Item.Description").unwrap();

        assert!(desc_schema.repeatable);
    }

    #[test]
    fn schema_required_field() {
        let mut tree = FormTree::new();
        let req = make_field(&mut tree, "Required", "x", None, None);
        let form = make_subform(&mut tree, "form1", vec![req], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let schema = export_schema(&tree, root);
        assert!(schema.fields.get("form1.Required").unwrap().required);
    }

    #[test]
    fn infer_field_type_works() {
        assert_eq!(infer_field_type("hello"), FieldType::Text);
        assert_eq!(infer_field_type("42"), FieldType::Numeric);
        assert_eq!(infer_field_type("3.14"), FieldType::Numeric);
        assert_eq!(infer_field_type("true"), FieldType::Boolean);
        assert_eq!(infer_field_type("0"), FieldType::Boolean);
        assert_eq!(infer_field_type(""), FieldType::Text);
    }
}
