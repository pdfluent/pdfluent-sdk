#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! D5 — occur resolution + mutation capture tests.
//!
//! Cover: `node.occur` resolves (no longer undefined), `occur.min`/`occur.max`
//! reads return structural template values, writes are CAPTURED (counted, not
//! applied to layout), and unsupported occur properties are not captured
//! (fail-closed). `occur_mutations_applied` stays 0 (capture-only).

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

/// `node.occur` resolves and `occur.min` reads the structural value (1 for
/// `Occur::once`) instead of throwing.
#[test]
fn occur_child_resolves_and_min_reads() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = String(this.occur.min);",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "1");
    assert!(outcome.occur_lookups_total >= 1);
    assert!(outcome.occur_lookup_successes >= 1);
    assert!(outcome.occur_property_reads >= 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// `occur.min = N` is captured (not applied) and no longer throws.
#[test]
fn occur_min_write_captured_not_applied() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.occur.min = 5; this.rawValue = \"done\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "done");
    assert_eq!(outcome.occur_min_writes, 1);
    assert_eq!(outcome.occur_property_writes, 1);
    assert_eq!(outcome.occur_mutations_captured, 1);
    assert_eq!(
        outcome.occur_mutations_applied, 0,
        "D5 is capture-only: no occur mutation may be applied to layout"
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "occur.min write must no longer throw 'cannot set property min of undefined'"
    );
}

/// `occur.max = N` is captured.
#[test]
fn occur_max_write_captured() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.occur.max = 3; this.rawValue = \"ok\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "ok");
    assert_eq!(outcome.occur_max_writes, 1);
    assert_eq!(outcome.occur_mutations_captured, 1);
    assert_eq!(outcome.occur_mutations_applied, 0);
}

/// An unsupported occur property is NOT captured (fail-closed): only
/// `min`/`max` writes feed the capture counters.
#[test]
fn unsupported_occur_property_not_captured() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.occur.min = 2; this.occur.zzz = 9; this.rawValue = \"x\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "x");
    assert_eq!(
        outcome.occur_property_writes, 1,
        "only the supported occur.min write is captured; occur.zzz is not"
    );
    assert_eq!(outcome.occur_min_writes, 1);
    assert_eq!(outcome.occur_max_writes, 0);
    assert_eq!(outcome.occur_mutations_applied, 0);
}
