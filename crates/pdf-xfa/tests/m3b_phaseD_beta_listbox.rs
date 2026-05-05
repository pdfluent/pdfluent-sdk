#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D-β integration tests for XFA listbox API (clearItems + addItem).

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    HostBindings, QuickJsRuntime, MAX_ITEMS_PER_LISTBOX, MAX_MUTATIONS_PER_DOC,
};
use pdf_xfa::JsExecutionMode;
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn add_node(tree: &mut FormTree, name: &str, node_type: FormNodeType) -> FormNodeId {
    tree.add_node(FormNode {
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
    })
}

fn add_child(
    tree: &mut FormTree,
    parent: FormNodeId,
    name: &str,
    node_type: FormNodeType,
) -> FormNodeId {
    let child = add_node(tree, name, node_type);
    tree.get_mut(parent).children.push(child);
    child
}

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    add_child(
        tree,
        parent,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    )
}

fn add_js_script(tree: &mut FormTree, node_id: FormNodeId, activity: &str, script: &str) {
    tree.meta_mut(node_id).event_scripts = vec![EventScript::new(
        script.to_string(),
        ScriptLanguage::JavaScript,
        Some(activity.to_string()),
        None,
        None,
    )];
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn one_field_form() -> (FormTree, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_field(&mut tree, root, "State", "");
    (tree, root, field)
}

#[test]
fn clear_items_on_empty_list_succeeds() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(&mut tree, field, "calculate", "this.clearItems();");

    let outcome = run_sandbox(&mut tree, root);

    assert!(tree.meta(field).runtime_listbox_items.is_empty());
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_list_writes, 1);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn add_item_with_display_and_save_adds_tuple() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"this.addItem("California", "CA");"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    let items = &tree.meta(field).runtime_listbox_items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], ("California".to_string(), "CA".to_string()));
    assert_eq!(outcome.js_list_writes, 1);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn add_item_without_save_uses_display_as_save() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"this.addItem("California");"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    let items = &tree.meta(field).runtime_listbox_items;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        ("California".to_string(), "California".to_string())
    );
    assert_eq!(outcome.js_list_writes, 1);
}

#[test]
fn clear_items_after_multiple_adds_leaves_empty() {
    let (mut tree, root, field) = one_field_form();
    add_js_script(
        &mut tree,
        field,
        "calculate",
        r#"
this.addItem("A", "1");
this.addItem("B", "2");
this.clearItems();
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert!(tree.meta(field).runtime_listbox_items.is_empty());
    assert_eq!(outcome.js_list_writes, 3);
}

#[test]
fn add_item_past_max_items_per_listbox_returns_err() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_ITEMS_PER_LISTBOX {
        host.reset_per_script(field, Some("calculate"));
        assert_eq!(
            host.list_add(field, "A".to_string(), Some("1".to_string())),
            Ok(())
        );
    }
    host.reset_per_script(field, Some("calculate"));
    assert_eq!(
        host.list_add(field, "overflow".to_string(), Some("ov".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, MAX_ITEMS_PER_LISTBOX as usize);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn clear_items_on_non_field_returns_err() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _subform = add_child(&mut tree, root, "Sub", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var result = Sub.clearItems();
Out.rawValue = result === false ? "err" : "ok";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.js_list_writes, 0);
    assert_eq!(outcome.js_binding_errors, 1);
}

#[test]
fn activity_gate_blocks_add_item_outside_allowed_activity() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(field, Some("click"));

    assert_eq!(
        host.list_add(field, "A".to_string(), Some("1".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, 0);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn mutation_cap_blocks_add_item_at_max_mutations() {
    let (mut tree, root, field) = one_field_form();
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_MUTATIONS_PER_DOC {
        host.reset_per_script(field, Some("calculate"));
        assert_eq!(
            host.list_add(field, "A".to_string(), Some("1".to_string())),
            Ok(())
        );
    }
    host.reset_per_script(field, Some("calculate"));
    assert_eq!(
        host.list_add(field, "A".to_string(), Some("1".to_string())),
        Err(())
    );

    let metadata = host.take_metadata();
    assert_eq!(metadata.list_writes, MAX_MUTATIONS_PER_DOC);
    assert_eq!(metadata.binding_errors, 1);
}
