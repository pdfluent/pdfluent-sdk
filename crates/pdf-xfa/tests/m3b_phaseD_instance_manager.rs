#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D integration tests for XFA instanceManager host bindings.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    HostBindings, QuickJsRuntime, MAX_INSTANCES_PER_SUBFORM, MAX_MUTATIONS_PER_DOC,
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

fn add_row(
    tree: &mut FormTree,
    root: FormNodeId,
    min: u32,
    max: Option<u32>,
) -> (FormNodeId, FormNodeId) {
    let row = add_child(tree, root, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(min, max, 1);
    let value = add_field(tree, row, "Value", "");
    (row, value)
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

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

fn live_children_named(tree: &FormTree, parent: FormNodeId, name: &str) -> Vec<FormNodeId> {
    tree.get(parent)
        .children
        .iter()
        .copied()
        .filter(|child_id| tree.get(*child_id).name == name)
        .collect()
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn one_row_form(min: u32, max: Option<u32>) -> (FormTree, FormNodeId, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let (row, _value) = add_row(&mut tree, root, min, max);
    let out = add_field(&mut tree, root, "Out", "");
    (tree, root, row, out)
}

#[test]
fn instance_manager_count_single_prototype_is_one() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Row.instanceManager.count;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_instance_writes, 0);
}

#[test]
fn set_instances_three_updates_count() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var count = Row.instanceManager.setInstances(3);
Out.rawValue = count + ":" + Row.instanceManager.count;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "3:3");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 3);
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn set_instances_zero_clamps_to_min_occur() {
    let (mut tree, root, _row, out) = one_row_form(1, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"Out.rawValue = Row.instanceManager.setInstances(0);"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn set_instances_one_reduces_existing_run() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
Row.instanceManager.setInstances(3);
Out.rawValue = Row.instanceManager.setInstances(1);
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 2);
}

#[test]
fn set_instances_clamps_to_safety_cap() {
    let (mut tree, root, _row, out) = one_row_form(0, None);
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"Out.rawValue = Row.instanceManager.setInstances(1000);"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        field_value(&tree, out),
        MAX_INSTANCES_PER_SUBFORM.to_string()
    );
    assert_eq!(
        live_children_named(&tree, root, "Row").len(),
        MAX_INSTANCES_PER_SUBFORM as usize
    );
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn add_instance_increments_count_and_returns_new_handle() {
    let (mut tree, root, _row, out) = one_row_form(0, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var added = Row.instanceManager.addInstance();
added.Value.rawValue = "cloned";
Out.rawValue = Row.instanceManager.count + ":" + added.index;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    let rows = live_children_named(&tree, root, "Row");
    let cloned_value = tree.get(rows[1]).children[0];

    assert_eq!(field_value(&tree, out), "2:1");
    assert_eq!(rows.len(), 2);
    assert_eq!(tree.get(rows[1]).name, "Row");
    assert_eq!(field_value(&tree, cloned_value), "cloned");
    assert_eq!(outcome.js_instance_writes, 1);
}

#[test]
fn remove_instance_decrements_and_refuses_at_min() {
    let (mut tree, root, _row, out) = one_row_form(1, Some(10));
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
Row.instanceManager.setInstances(2);
var removed = Row.instanceManager.removeInstance(1);
var refused = Row.instanceManager.removeInstance(0);
Out.rawValue = Row.instanceManager.count + ":" + removed + ":" + refused;
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1:true:false");
    assert_eq!(live_children_named(&tree, root, "Row").len(), 1);
    assert_eq!(outcome.js_instance_writes, 2);
    assert_eq!(outcome.js_binding_errors, 1);
}

#[test]
fn mutation_cap_blocks_excessive_instance_writes() {
    let (mut tree, root, row, _out) = one_row_form(0, None);
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    for _ in 0..MAX_MUTATIONS_PER_DOC {
        host.reset_per_script(row, Some("calculate"));
        assert_eq!(host.instance_set(row, 1), Ok(1));
    }
    host.reset_per_script(row, Some("calculate"));
    assert_eq!(host.instance_set(row, 1), Err(()));

    let metadata = host.take_metadata();
    assert_eq!(metadata.instance_writes, MAX_MUTATIONS_PER_DOC);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn activity_gate_blocks_instance_mutation_outside_allowed_activity() {
    let (mut tree, root, row, _out) = one_row_form(0, Some(10));
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(row, Some("click"));

    assert_eq!(host.instance_add(row), Err(()));

    let metadata = host.take_metadata();
    assert_eq!(metadata.instance_writes, 0);
    assert_eq!(metadata.binding_errors, 1);
}

#[test]
fn node_index_reports_first_and_last_same_name_sibling() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let (first, _first_value) = add_row(&mut tree, root, 0, Some(10));
    let (_second, _second_value) = add_row(&mut tree, root, 0, Some(10));
    let (last, _last_value) = add_row(&mut tree, root, 0, Some(10));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        first,
        "calculate",
        r#"Out.rawValue = this.index;"#,
    );
    add_js_script(
        &mut tree,
        last,
        "calculate",
        r#"Out.rawValue = Out.rawValue + ":" + this.index + ":" + this.instanceManager.count;"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "0:2:3");
    assert_eq!(outcome.js_instance_writes, 0);
    assert_eq!(outcome.js_binding_errors, 0);
}

#[test]
fn underscore_shorthand_resolves_before_handle_property_deferral() {
    // Regression: the `_<child>` underscore shorthand on a subform proxy must
    // run *before* `shouldDeferHandleProperty`, otherwise every underscore-
    // prefixed access (e.g. `parent._Row.count`) returns `undefined` and
    // throws on the next chained property read.
    //
    // Pre-fix, `Time_Confirmation._BodyRow.count` failed with
    // "cannot read property 'count' of undefined" on real corpus docs even
    // though the form tree had `Time_Confirmation.BodyRow` populated.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let parent = add_child(&mut tree, root, "Parent", FormNodeType::Subform);
    let row = add_child(&mut tree, parent, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(1, Some(10), 1);
    let _row_value = add_field(&mut tree, row, "Value", "");
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = Parent._Row.count;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}
