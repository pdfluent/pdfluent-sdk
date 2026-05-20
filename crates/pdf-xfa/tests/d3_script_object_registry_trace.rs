#![cfg(feature = "xfa-js-sandboxed")]

//! D3 — script-object registry trace observability tests.
//!
//! These assert the behaviour-neutral D3 trace counters on
//! [`DynamicScriptOutcome`]:
//! - `variables_scripts_collected` / `variables_data_items_collected`
//! - `script_objects_registered` / `script_objects_register_failed`
//! - `script_objects_subform_scoped`
//!
//! They also pin the two gap classes the D3 audit identified:
//! 1. a NESTED-subform `<variables>` script is collected + counted as
//!    subform-scoped but is NOT reachable as a bare identifier from event
//!    scripts (the "scope_hidden" gap);
//! 2. an oversized body fails to register and is counted as
//!    `script_objects_register_failed` (observable, not silent).

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{QuickJsRuntime, MAX_VARIABLES_SCRIPT_BODY_BYTES};
use pdf_xfa::{DynamicScriptOutcome, JsExecutionMode};
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

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    let child = add_node(
        tree,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    );
    tree.get_mut(parent).children.push(child);
    child
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

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

/// A root-scope named script object is collected, registered, and counted.
#[test]
fn root_script_object_collected_and_registered_counts() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        None,
        "soUtil".into(),
        "function helper() { return \"ok\"; }".into(),
    ));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof soUtil === \"undefined\") ? \"absent\" : soUtil.helper();",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.variables_scripts_collected, 1);
    assert_eq!(outcome.script_objects_registered, 1);
    assert_eq!(outcome.script_objects_register_failed, 0);
    assert_eq!(outcome.script_objects_subform_scoped, 0);
    // Root-scope object is bare-ident visible today.
    assert_eq!(field_value(&tree, out), "ok");
}

/// A NESTED-subform script object is collected and counted as subform-scoped,
/// but is NOT reachable as a bare identifier — the "scope_hidden" gap class.
#[test]
fn subform_scoped_script_object_counted_but_not_bare_visible() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        Some("inner".into()),
        "countryScript".into(),
        "function getCountries() { return \"CA\"; }".into(),
    ));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof countryScript === \"undefined\") ? \"absent\" \
         : countryScript.getCountries();",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.variables_scripts_collected, 1);
    assert_eq!(
        outcome.script_objects_subform_scoped, 1,
        "nested-subform script must be counted as subform-scoped"
    );
    // Registration into subformVariables succeeds...
    assert_eq!(outcome.script_objects_registered, 1);
    // ...but the symbol is NOT bare-ident visible (documents the gap).
    assert_eq!(
        field_value(&tree, out),
        "absent",
        "subform-scoped script must NOT resolve as a bare identifier today"
    );
}

/// An oversized variables-script body fails to register and is counted in
/// `script_objects_register_failed` (observable, not silently dropped).
#[test]
fn oversized_script_object_register_failed_counted() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let big = "x".repeat(MAX_VARIABLES_SCRIPT_BODY_BYTES + 1);
    tree.variables_scripts.push((None, "tooBig".into(), big));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(&mut tree, out, "calculate", "this.rawValue = \"done\";");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.variables_scripts_collected, 1);
    assert_eq!(outcome.script_objects_registered, 0);
    assert_eq!(
        outcome.script_objects_register_failed, 1,
        "oversized body must be counted as a registration failure"
    );
}

/// A `<variables>` data item is collected and counted.
#[test]
fn data_item_collected_counted() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_data_items
        .push((None, "globValidatePressed".into(), "false".into()));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(&mut tree, out, "calculate", "this.rawValue = \"done\";");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.variables_data_items_collected, 1);
    assert_eq!(
        outcome.script_objects_registered, 1,
        "data item registration counts toward registered"
    );
}
