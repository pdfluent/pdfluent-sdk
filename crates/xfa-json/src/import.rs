//! Import JSON data into a FormTree.
//!
//! Merges field values from a `FormData` structure back into an existing
//! FormTree, updating field values by matching SOM-style paths.

use crate::types::{FieldValue, FormData};
use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree};

/// Merge JSON field values into an existing FormTree.
///
/// Walks the tree starting at `root`, matching field paths from `data`
/// and updating field values. Repeating sections are matched by array index.
pub fn json_to_form_tree(data: &FormData, tree: &mut FormTree, root: FormNodeId) {
    let node = tree.get(root);
    let children: Vec<FormNodeId> = node.children.clone();

    match &node.node_type {
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for child_id in children {
                merge_node(data, tree, child_id, "");
            }
        }
        _ => {
            merge_node(data, tree, root, "");
        }
    }
}

/// Recursively merge data into a subtree.
fn merge_node(data: &FormData, tree: &mut FormTree, node_id: FormNodeId, parent_path: &str) {
    let node = tree.get(node_id);
    let name = node.name.clone();
    let path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}.{name}")
    };

    let node_type = node.node_type.clone();
    let children: Vec<FormNodeId> = node.children.clone();
    let is_repeating = node.occur.is_repeating();

    match node_type {
        FormNodeType::Field { .. } => {
            if let Some(value) = data.fields.get(&path) {
                let string_value = field_value_to_string(value);
                tree.get_mut(node_id).node_type = FormNodeType::Field {
                    value: string_value,
                };
            }
        }
        FormNodeType::Draw { .. } => {
            if let Some(value) = data.fields.get(&path) {
                let string_value = field_value_to_string(value);
                tree.get_mut(node_id).node_type = FormNodeType::Draw {
                    content: string_value,
                };
            }
        }
        FormNodeType::Subform => {
            if is_repeating {
                // For repeating subforms, match array elements by index
                if let Some(FieldValue::Array(instances)) = data.fields.get(&path) {
                    // Update this instance if it's within the array bounds
                    // (instance index tracking would need caller context,
                    // so for now we update the first instance only)
                    if let Some(instance_data) = instances.first() {
                        for child_id in &children {
                            let child = tree.get(*child_id);
                            let child_name = child.name.clone();
                            if let Some(value) = instance_data.get(&child_name) {
                                if let FormNodeType::Field { .. } = &child.node_type {
                                    let string_value = field_value_to_string(value);
                                    tree.get_mut(*child_id).node_type = FormNodeType::Field {
                                        value: string_value,
                                    };
                                }
                            }
                        }
                    }
                }
            } else {
                for child_id in children {
                    merge_node(data, tree, child_id, &path);
                }
            }
        }
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for child_id in children {
                merge_node(data, tree, child_id, &path);
            }
        }
    }
}

/// Convert a FieldValue back to a string for storage in FormTree.
fn field_value_to_string(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(s) => s.clone(),
        FieldValue::Number(n) => {
            if *n == n.trunc() && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        FieldValue::Boolean(b) => if *b { "1" } else { "0" }.to_string(),
        FieldValue::Null => String::new(),
        FieldValue::Array(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::form_tree_to_json;
    use indexmap::IndexMap;
    use xfa_layout_engine::form::{FormNode, Occur};
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

    fn make_field(tree: &mut FormTree, name: &str, value: &str) -> FormNodeId {
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
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_subform(
        tree: &mut FormTree,
        name: &str,
        children: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
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
    fn roundtrip_simple_form() {
        let mut tree = FormTree::new();
        let name = make_field(&mut tree, "Name", "Original");
        let amount = make_field(&mut tree, "Amount", "100");
        let form = make_subform(&mut tree, "form1", vec![name, amount]);
        let root = make_root(&mut tree, vec![form]);

        // Export → modify → import
        let mut data = form_tree_to_json(&tree, root);
        data.fields.insert(
            "form1.Name".to_string(),
            FieldValue::Text("Updated".to_string()),
        );
        data.fields
            .insert("form1.Amount".to_string(), FieldValue::Number(200.0));

        json_to_form_tree(&data, &mut tree, root);

        // Verify updated values
        let exported = form_tree_to_json(&tree, root);
        assert_eq!(
            exported.fields.get("form1.Name"),
            Some(&FieldValue::Text("Updated".to_string()))
        );
        assert_eq!(
            exported.fields.get("form1.Amount"),
            Some(&FieldValue::Number(200.0))
        );
    }

    #[test]
    fn import_boolean_as_zero_one() {
        let mut tree = FormTree::new();
        let check = make_field(&mut tree, "Active", "0");
        let form = make_subform(&mut tree, "form1", vec![check]);
        let root = make_root(&mut tree, vec![form]);

        let mut fields = IndexMap::new();
        fields.insert("form1.Active".to_string(), FieldValue::Boolean(true));
        let data = FormData { fields };

        json_to_form_tree(&data, &mut tree, root);

        // Boolean true → "1" in FormTree
        match &tree.get(check).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "1"),
            _ => panic!("Expected Field"),
        }
    }

    #[test]
    fn import_null_clears_field() {
        let mut tree = FormTree::new();
        let field = make_field(&mut tree, "Note", "something");
        let form = make_subform(&mut tree, "form1", vec![field]);
        let root = make_root(&mut tree, vec![form]);

        let mut fields = IndexMap::new();
        fields.insert("form1.Note".to_string(), FieldValue::Null);
        let data = FormData { fields };

        json_to_form_tree(&data, &mut tree, root);

        match &tree.get(field).node_type {
            FormNodeType::Field { value } => assert_eq!(value, ""),
            _ => panic!("Expected Field"),
        }
    }

    #[test]
    fn field_value_to_string_formats() {
        assert_eq!(field_value_to_string(&FieldValue::Number(42.0)), "42");
        assert_eq!(field_value_to_string(&FieldValue::Number(3.14)), "3.14");
        assert_eq!(field_value_to_string(&FieldValue::Boolean(true)), "1");
        assert_eq!(field_value_to_string(&FieldValue::Boolean(false)), "0");
        assert_eq!(field_value_to_string(&FieldValue::Null), "");
        assert_eq!(
            field_value_to_string(&FieldValue::Text("hi".to_string())),
            "hi"
        );
    }
}
