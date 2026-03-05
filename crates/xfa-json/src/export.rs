//! Export FormTree to JSON.
//!
//! Walks the FormTree recursively and produces a flat `FormData` with
//! SOM-style dotted paths as keys and coerced typed values.

use crate::coerce::coerce_value;
use crate::types::{FieldValue, FormData};
use indexmap::IndexMap;
use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree};

/// Convert a FormTree into a JSON-friendly `FormData` structure.
///
/// Walks the tree starting at `root`, collecting field values keyed
/// by their SOM-style dotted path (e.g., `"form1.Customer.Name"`).
/// Repeating subforms become arrays.
pub fn form_tree_to_json(tree: &FormTree, root: FormNodeId) -> FormData {
    let mut fields = IndexMap::new();
    let node = tree.get(root);

    // Skip root-level structural nodes and dive into content
    match &node.node_type {
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for &child_id in &node.children {
                walk_node(tree, child_id, "", &mut fields);
            }
        }
        _ => {
            walk_node(tree, root, "", &mut fields);
        }
    }

    FormData { fields }
}

/// Convert a FormTree into a raw `serde_json::Value`.
///
/// Convenience wrapper that serializes `FormData` directly.
pub fn form_tree_to_value(tree: &FormTree, root: FormNodeId) -> serde_json::Value {
    let data = form_tree_to_json(tree, root);
    serde_json::to_value(data).unwrap_or(serde_json::Value::Null)
}

/// Recursively walk the FormTree, collecting fields into the flat map.
fn walk_node(
    tree: &FormTree,
    node_id: FormNodeId,
    parent_path: &str,
    fields: &mut IndexMap<String, FieldValue>,
) {
    let node = tree.get(node_id);
    let path = if parent_path.is_empty() {
        node.name.clone()
    } else {
        format!("{}.{}", parent_path, node.name)
    };

    match &node.node_type {
        FormNodeType::Field { value } => {
            fields.insert(path, coerce_value(value));
        }
        FormNodeType::Draw { content } => {
            if !content.is_empty() {
                fields.insert(path, FieldValue::Text(content.clone()));
            }
        }
        FormNodeType::Subform => {
            if node.occur.is_repeating() {
                // Repeating subform: collect siblings with the same name as an array.
                // The caller handles this via collect_repeating_siblings.
                // For a single instance in a repeating group, still emit as array.
                let mut instance = IndexMap::new();
                for &child_id in &node.children {
                    walk_node_into_map(tree, child_id, &mut instance);
                }
                // Merge into existing array if present, or create new one
                match fields.get_mut(&path) {
                    Some(FieldValue::Array(arr)) => {
                        arr.push(instance);
                    }
                    _ => {
                        fields.insert(path, FieldValue::Array(vec![instance]));
                    }
                }
            } else {
                // Non-repeating subform: recurse with extended path
                for &child_id in &node.children {
                    walk_node(tree, child_id, &path, fields);
                }
            }
        }
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for &child_id in &node.children {
                walk_node(tree, child_id, &path, fields);
            }
        }
    }
}

/// Walk a node into a flat map (used for repeating section instances).
fn walk_node_into_map(
    tree: &FormTree,
    node_id: FormNodeId,
    map: &mut IndexMap<String, FieldValue>,
) {
    let node = tree.get(node_id);

    match &node.node_type {
        FormNodeType::Field { value } => {
            map.insert(node.name.clone(), coerce_value(value));
        }
        FormNodeType::Draw { content } => {
            if !content.is_empty() {
                map.insert(node.name.clone(), FieldValue::Text(content.clone()));
            }
        }
        FormNodeType::Subform => {
            if node.occur.is_repeating() {
                let mut instance = IndexMap::new();
                for &child_id in &node.children {
                    walk_node_into_map(tree, child_id, &mut instance);
                }
                match map.get_mut(&node.name) {
                    Some(FieldValue::Array(arr)) => {
                        arr.push(instance);
                    }
                    _ => {
                        map.insert(node.name.clone(), FieldValue::Array(vec![instance]));
                    }
                }
            } else {
                // Nested non-repeating: prefix with subform name
                for &child_id in &node.children {
                    let child = tree.get(child_id);
                    let key = format!("{}.{}", node.name, child.name);
                    match &child.node_type {
                        FormNodeType::Field { value } => {
                            map.insert(key, coerce_value(value));
                        }
                        FormNodeType::Draw { content } => {
                            if !content.is_empty() {
                                map.insert(key, FieldValue::Text(content.clone()));
                            }
                        }
                        _ => {
                            walk_node_into_map(tree, child_id, map);
                        }
                    }
                }
            }
        }
        _ => {
            for &child_id in &node.children {
                walk_node_into_map(tree, child_id, map);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn simple_form_export() {
        let mut tree = FormTree::new();
        let name = make_field(&mut tree, "Name", "Acme Corp");
        let amount = make_field(&mut tree, "Amount", "42.50");
        let active = make_field(&mut tree, "Active", "true");

        let form = make_subform(
            &mut tree,
            "form1",
            vec![name, amount, active],
            Occur::once(),
        );
        let root = make_root(&mut tree, vec![form]);

        let data = form_tree_to_json(&tree, root);

        assert_eq!(
            data.fields.get("form1.Name"),
            Some(&FieldValue::Text("Acme Corp".to_string()))
        );
        assert_eq!(
            data.fields.get("form1.Amount"),
            Some(&FieldValue::Number(42.50))
        );
        assert_eq!(
            data.fields.get("form1.Active"),
            Some(&FieldValue::Boolean(true))
        );
    }

    #[test]
    fn nested_subforms() {
        let mut tree = FormTree::new();
        let street = make_field(&mut tree, "Street", "123 Main St");
        let city = make_field(&mut tree, "City", "Springfield");

        let address = make_subform(&mut tree, "Address", vec![street, city], Occur::once());
        let form = make_subform(&mut tree, "form1", vec![address], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let data = form_tree_to_json(&tree, root);

        assert_eq!(
            data.fields.get("form1.Address.Street"),
            Some(&FieldValue::Text("123 Main St".to_string()))
        );
        assert_eq!(
            data.fields.get("form1.Address.City"),
            Some(&FieldValue::Text("Springfield".to_string()))
        );
    }

    #[test]
    fn repeating_subforms_as_arrays() {
        let mut tree = FormTree::new();

        let desc1 = make_field(&mut tree, "Description", "Widget A");
        let qty1 = make_field(&mut tree, "Qty", "10");
        let item1 = make_subform(
            &mut tree,
            "Item",
            vec![desc1, qty1],
            Occur::repeating(0, None, 2),
        );

        let desc2 = make_field(&mut tree, "Description", "Widget B");
        let qty2 = make_field(&mut tree, "Qty", "5");
        let item2 = make_subform(
            &mut tree,
            "Item",
            vec![desc2, qty2],
            Occur::repeating(0, None, 2),
        );

        let form = make_subform(&mut tree, "form1", vec![item1, item2], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let data = form_tree_to_json(&tree, root);

        let items = data.fields.get("form1.Item").unwrap();
        match items {
            FieldValue::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert_eq!(
                    arr[0].get("Description"),
                    Some(&FieldValue::Text("Widget A".to_string()))
                );
                assert_eq!(arr[0].get("Qty"), Some(&FieldValue::Number(10.0)));
                assert_eq!(
                    arr[1].get("Description"),
                    Some(&FieldValue::Text("Widget B".to_string()))
                );
                assert_eq!(arr[1].get("Qty"), Some(&FieldValue::Number(5.0)));
            }
            _ => panic!("Expected Array, got {items:?}"),
        }
    }

    #[test]
    fn empty_fields_are_null() {
        let mut tree = FormTree::new();
        let empty = make_field(&mut tree, "Empty", "");
        let form = make_subform(&mut tree, "form1", vec![empty], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let data = form_tree_to_json(&tree, root);
        assert_eq!(data.fields.get("form1.Empty"), Some(&FieldValue::Null));
    }

    #[test]
    fn form_tree_to_value_produces_valid_json() {
        let mut tree = FormTree::new();
        let name = make_field(&mut tree, "Name", "Test");
        let form = make_subform(&mut tree, "form1", vec![name], Occur::once());
        let root = make_root(&mut tree, vec![form]);

        let value = form_tree_to_value(&tree, root);
        assert!(value.is_object());

        let fields = value.get("fields").unwrap();
        assert_eq!(fields.get("form1.Name").unwrap(), "Test");
    }

    #[test]
    fn type_coercion_in_export() {
        let mut tree = FormTree::new();
        let num = make_field(&mut tree, "Total", "112.50");
        let flag = make_field(&mut tree, "Checked", "0");
        let text = make_field(&mut tree, "Note", "See attached");
        let null_field = make_field(&mut tree, "Blank", "");

        let form = make_subform(
            &mut tree,
            "form1",
            vec![num, flag, text, null_field],
            Occur::once(),
        );
        let root = make_root(&mut tree, vec![form]);

        let data = form_tree_to_json(&tree, root);

        assert_eq!(
            data.fields.get("form1.Total"),
            Some(&FieldValue::Number(112.50))
        );
        assert_eq!(
            data.fields.get("form1.Checked"),
            Some(&FieldValue::Boolean(false))
        );
        assert_eq!(
            data.fields.get("form1.Note"),
            Some(&FieldValue::Text("See attached".to_string()))
        );
        assert_eq!(data.fields.get("form1.Blank"), Some(&FieldValue::Null));
    }
}
