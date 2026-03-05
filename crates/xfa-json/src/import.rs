//! Import JSON data into a FormTree.
//!
//! Merges field values from a `FormData` structure back into an existing
//! FormTree, updating field values by matching SOM-style paths.

use crate::types::{FieldValue, FormData};
use indexmap::IndexMap;
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
                // Repeating subforms are handled by the parent via
                // merge_children_with_repeating_groups — skip here.
            } else {
                merge_children_with_repeating_groups(data, tree, &children, &path);
            }
        }
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            merge_children_with_repeating_groups(data, tree, &children, &path);
        }
    }
}

/// Merge children, grouping same-name repeating siblings and assigning
/// array indices so each sibling gets the correct element from the JSON array.
fn merge_children_with_repeating_groups(
    data: &FormData,
    tree: &mut FormTree,
    children: &[FormNodeId],
    parent_path: &str,
) {
    // Track how many times we've seen each repeating name, so we can assign indices.
    let mut repeating_counts: IndexMap<String, usize> = IndexMap::new();

    for &child_id in children {
        let child = tree.get(child_id);
        let is_repeating = child.occur.is_repeating();

        if is_repeating {
            let name = child.name.clone();
            let index = repeating_counts.get(&name).copied().unwrap_or(0);
            repeating_counts.insert(name.clone(), index + 1);

            let path = if parent_path.is_empty() {
                name
            } else {
                format!("{parent_path}.{name}")
            };

            // Look up the array and apply the correct element by index.
            if let Some(FieldValue::Array(instances)) = data.fields.get(&path) {
                if let Some(instance_data) = instances.get(index) {
                    merge_instance(tree, child_id, instance_data);
                }
            }
        } else {
            merge_node(data, tree, child_id, parent_path);
        }
    }
}

/// Apply a single instance map (from a JSON array element) to a repeating
/// subform node, recursively handling nested subforms and draw nodes.
fn merge_instance(
    tree: &mut FormTree,
    node_id: FormNodeId,
    instance_data: &IndexMap<String, FieldValue>,
) {
    let children: Vec<FormNodeId> = tree.get(node_id).children.clone();

    for &child_id in &children {
        let child = tree.get(child_id);
        let child_name = child.name.clone();
        let child_type = child.node_type.clone();

        match child_type {
            FormNodeType::Field { .. } => {
                if let Some(value) = instance_data.get(&child_name) {
                    let string_value = field_value_to_string(value);
                    tree.get_mut(child_id).node_type = FormNodeType::Field {
                        value: string_value,
                    };
                }
            }
            FormNodeType::Draw { .. } => {
                if let Some(value) = instance_data.get(&child_name) {
                    let string_value = field_value_to_string(value);
                    tree.get_mut(child_id).node_type = FormNodeType::Draw {
                        content: string_value,
                    };
                }
            }
            FormNodeType::Subform => {
                // Nested non-repeating subform: look up dotted keys (e.g., "Address.Street")
                let nested_prefix = format!("{child_name}.");
                let nested_data: IndexMap<String, FieldValue> = instance_data
                    .iter()
                    .filter_map(|(k, v)| {
                        k.strip_prefix(&nested_prefix)
                            .map(|rest| (rest.to_string(), v.clone()))
                    })
                    .collect();
                if !nested_data.is_empty() {
                    merge_instance(tree, child_id, &nested_data);
                }
            }
            _ => {}
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
        make_subform_with_occur(tree, name, children, Occur::once())
    }

    fn make_subform_with_occur(
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

    #[test]
    fn import_repeating_by_index_not_always_first() {
        // P1 regression: each repeating sibling must get the correct array element
        let mut tree = FormTree::new();
        let desc1 = make_field(&mut tree, "Desc", "old1");
        let qty1 = make_field(&mut tree, "Qty", "0");
        let item1 = make_subform_with_occur(
            &mut tree,
            "Item",
            vec![desc1, qty1],
            Occur::repeating(0, None, 2),
        );

        let desc2 = make_field(&mut tree, "Desc", "old2");
        let qty2 = make_field(&mut tree, "Qty", "0");
        let item2 = make_subform_with_occur(
            &mut tree,
            "Item",
            vec![desc2, qty2],
            Occur::repeating(0, None, 2),
        );

        let form = make_subform(&mut tree, "form1", vec![item1, item2]);
        let root = make_root(&mut tree, vec![form]);

        // Import: each array element should go to the correct sibling
        let mut instance0 = IndexMap::new();
        instance0.insert("Desc".to_string(), FieldValue::Text("Widget A".to_string()));
        instance0.insert("Qty".to_string(), FieldValue::Number(10.0));
        let mut instance1 = IndexMap::new();
        instance1.insert("Desc".to_string(), FieldValue::Text("Widget B".to_string()));
        instance1.insert("Qty".to_string(), FieldValue::Number(5.0));

        let mut fields = IndexMap::new();
        fields.insert(
            "form1.Item".to_string(),
            FieldValue::Array(vec![instance0, instance1]),
        );
        let data = FormData { fields };

        json_to_form_tree(&data, &mut tree, root);

        // Verify each sibling got the correct data
        match &tree.get(desc1).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Widget A"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(desc2).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Widget B"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(qty1).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "10"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(qty2).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "5"),
            _ => panic!("Expected Field"),
        }
    }

    #[test]
    fn import_nested_subform_inside_repeating() {
        // P2 regression: nested structures inside repeated items must be applied
        let mut tree = FormTree::new();

        // Item[0] has a nested Address subform
        let street1 = make_field(&mut tree, "Street", "old");
        let addr1 = make_subform(&mut tree, "Address", vec![street1]);
        let name1 = make_field(&mut tree, "Name", "old");
        let item1 = make_subform_with_occur(
            &mut tree,
            "Item",
            vec![name1, addr1],
            Occur::repeating(0, None, 2),
        );

        let street2 = make_field(&mut tree, "Street", "old");
        let addr2 = make_subform(&mut tree, "Address", vec![street2]);
        let name2 = make_field(&mut tree, "Name", "old");
        let item2 = make_subform_with_occur(
            &mut tree,
            "Item",
            vec![name2, addr2],
            Occur::repeating(0, None, 2),
        );

        let form = make_subform(&mut tree, "form1", vec![item1, item2]);
        let root = make_root(&mut tree, vec![form]);

        // Import with nested Address.Street keys
        let mut inst0 = IndexMap::new();
        inst0.insert("Name".to_string(), FieldValue::Text("Alice".to_string()));
        inst0.insert(
            "Address.Street".to_string(),
            FieldValue::Text("123 Main St".to_string()),
        );
        let mut inst1 = IndexMap::new();
        inst1.insert("Name".to_string(), FieldValue::Text("Bob".to_string()));
        inst1.insert(
            "Address.Street".to_string(),
            FieldValue::Text("456 Oak Ave".to_string()),
        );

        let mut fields = IndexMap::new();
        fields.insert(
            "form1.Item".to_string(),
            FieldValue::Array(vec![inst0, inst1]),
        );
        let data = FormData { fields };

        json_to_form_tree(&data, &mut tree, root);

        // Verify nested fields were applied
        match &tree.get(name1).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Alice"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(street1).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "123 Main St"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(name2).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "Bob"),
            _ => panic!("Expected Field"),
        }
        match &tree.get(street2).node_type {
            FormNodeType::Field { value } => assert_eq!(value, "456 Oak Ave"),
            _ => panic!("Expected Field"),
        }
    }
}
