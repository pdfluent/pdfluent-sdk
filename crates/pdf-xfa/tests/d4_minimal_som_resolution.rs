#![cfg(feature = "xfa-js-sandboxed")]

//! D4 — minimal SOM resolution tests.
//!
//! Cover the one resolved pattern (subform-scoped named script-object
//! bare-identifier lookup, unique-name only, fail-closed on ambiguity) and the
//! SOM trace counters. Default-mode neutrality is guaranteed structurally (the
//! resolver lives in the `xfa-js-sandboxed` backend) and is exercised by the
//! existing protected-target suite.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::QuickJsRuntime;
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

/// A unique-name subform-scoped script object now resolves as a bare identifier
/// (the D4 minimal SOM resolution), and is counted as exposed.
#[test]
fn subform_scoped_unique_script_now_resolves() {
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

    assert_eq!(outcome.som_subform_scripts_exposed, 1);
    assert_eq!(outcome.som_lookup_ambiguous, 0);
    assert_eq!(
        field_value(&tree, out),
        "CA",
        "unique-name subform-scoped script must resolve as a bare identifier in D4"
    );
}

/// A subform-scoped script NAME declared by two subforms is ambiguous and is
/// withheld from bare-identifier exposure (fail-closed) + counted ambiguous.
#[test]
fn subform_scoped_duplicate_name_fails_closed() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        Some("a".into()),
        "dupScript".into(),
        "function f() { return \"A\"; }".into(),
    ));
    tree.variables_scripts.push((
        Some("b".into()),
        "dupScript".into(),
        "function f() { return \"B\"; }".into(),
    ));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof dupScript === \"undefined\") ? \"absent\" : dupScript.f();",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        outcome.som_lookup_ambiguous, 1,
        "duplicate subform-script name must be counted ambiguous"
    );
    assert_eq!(
        outcome.som_subform_scripts_exposed, 0,
        "ambiguous name must NOT be exposed (fail-closed)"
    );
    assert_eq!(
        field_value(&tree, out),
        "absent",
        "ambiguous subform-script must not resolve as a bare identifier"
    );
}

/// An unresolved bare identifier increments the SOM failure counter (typed,
/// not silent).
#[test]
fn unresolved_som_lookup_is_traced() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof ghostNodeXYZ === \"undefined\") ? \"absent\" : \"present\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert!(
        outcome.som_lookups_total >= outcome.som_lookup_successes,
        "total lookups must be >= successes"
    );
    // The bare `ghostNodeXYZ` reference must reach the SOM resolver and miss.
    assert!(
        outcome.som_lookup_failures >= 1,
        "unresolved bare identifier must be counted as a SOM failure"
    );
}
