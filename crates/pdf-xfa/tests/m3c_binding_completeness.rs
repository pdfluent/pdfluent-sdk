//! XFA-DATA-M3C binding completeness tests.
//!
//! Verifies the schema-bound identifier resolution surface introduced for
//! Polish PIT and similar template patterns:
//!
//! - Underscore-shorthand `_<Name>` resolves to an instanceManager on both
//!   the implicit global scope and the `<handle>` scope.
//! - Bare `parent` returns the form-tree parent handle.
//! - `<handle>.ui.choiceList`, `<handle>.formattedValue`, and
//!   `<handle>.execEvent(...)` no longer throw.
//! - The negative contract `xfa.resolveNode("DoesNotExist") === null` is
//!   preserved (cluster C: m3b_phaseC_bindings.rs assertions stay green).
//!
//! Synthetic `FormTree` fixtures only — no corpus PDFs are read.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![cfg(feature = "xfa-js-sandboxed")]

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

fn add_subform(tree: &mut FormTree, parent: FormNodeId, name: &str) -> FormNodeId {
    let id = add_node(tree, name, FormNodeType::Subform);
    tree.get_mut(parent).children.push(id);
    id
}

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    let id = add_node(
        tree,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    );
    tree.get_mut(parent).children.push(id);
    id
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

// ── Underscore-shorthand on the implicit global scope ────────────────────

#[test]
fn underscore_global_with_real_subform_returns_instance_manager() {
    // Tree:
    //   root
    //     Container
    //       AdresZagr (subform, occur.initial=1)
    //       Trigger (field, runs the script on calculate)
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let container = add_subform(&mut tree, root, "Container");
    let _addr = add_subform(&mut tree, container, "AdresZagr");
    let trigger = add_field(&mut tree, container, "Trigger", "");

    // Script does the Polish-PIT pattern: `_AdresZagr.setInstances(0)`.
    // The implicit-global underscore branch must resolve `AdresZagr` via
    // the implicit walk and surface an instance manager whose
    // setInstances() call is a chainable no-op (occur.max is 1 by
    // default in our test fixtures; the call may return 0 or 1, but it
    // must NOT throw and must NOT bump resolve_failures).
    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
if (typeof _AdresZagr === "undefined") {
  throw new Error("_AdresZagr should resolve to an instanceManager");
}
if (typeof _AdresZagr.setInstances !== "function") {
  throw new Error("_AdresZagr.setInstances must be a function");
}
// Adobe pattern: count should be a number, not undefined.
if (typeof _AdresZagr.count !== "number") {
  throw new Error("_AdresZagr.count must be numeric");
}
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

#[test]
fn underscore_global_with_unknown_name_stays_undefined() {
    // No same-named child exists anywhere. Implicit walk returns 0
    // candidates, the proxy `has` returns false, and `with (__globals)`
    // falls through to globalThis, where `_NeverDefined` is undefined and
    // a bare read throws ReferenceError. The script catches that to
    // assert the deferral path. No resolve failure is recorded — the
    // probe uses the quiet host variant.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Trigger", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var threw = false;
try { var x = _NeverDefined; }
catch (e) { threw = true; }
if (!threw) throw new Error("_NeverDefined must remain undefined");
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

// ── Underscore-shorthand on a handle ─────────────────────────────────────

#[test]
fn underscore_on_container_handle_with_missing_subform_returns_empty_manager() {
    // Container has NO same-named child. Per XFA-DATA-M3C, the JS proxy
    // surfaces an empty instance manager for container handles so
    // `Container._OptionalChild.setInstances(N)` is a chainable no-op.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _container = add_subform(&mut tree, root, "Container");
    let trigger = add_field(&mut tree, root, "Trigger", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var c = Container;
if (typeof c === "undefined") throw new Error("Container must resolve");
var mgr = c._OptionalChild;
if (typeof mgr === "undefined") throw new Error("_OptionalChild must be a manager");
if (mgr.count !== 0) throw new Error("expected zero-count manager");
mgr.setInstances(0); // no-op
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

#[test]
fn underscore_on_field_handle_stays_undefined() {
    // XFA 3.3 §6.4.3.2 restricts the underscore-shorthand to container
    // node types. Fields must continue to return `undefined` for
    // underscored property reads so the narrow-handle isolation contract
    // from m3b_phaseC_bindings.rs (field_handle_is_frozen_and_narrow)
    // stays intact.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Trigger", "hello");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var f = xfa.resolveNode("Trigger");
if (f === null) throw new Error("Trigger should resolve");
if (f._id !== undefined) throw new Error("field._id must stay undefined");
if (f._whatever !== undefined) throw new Error("field._whatever must stay undefined");
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

// ── `parent` bare global and handle property ─────────────────────────────

#[test]
fn bare_parent_global_returns_form_tree_parent() {
    // Tree:  root → Container → Trigger
    // Script runs on Trigger; `parent` must resolve to Container.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _container = add_subform(&mut tree, root, "Container");
    let _container_id = _container; // silence shadow
    let trigger = add_field(&mut tree, _container, "Trigger", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
if (typeof parent === "undefined") throw new Error("parent must resolve");
// parent.index of a single-instance container is 0.
if (parent.index !== 0) throw new Error("parent.index must be 0");
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

#[test]
fn handle_dot_parent_walks_up_one_level() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let container = add_subform(&mut tree, root, "Outer");
    let inner = add_subform(&mut tree, container, "Inner");
    let trigger = add_field(&mut tree, inner, "Trigger", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var f = xfa.resolveNode("Trigger");
if (f === null) throw new Error("Trigger should resolve");
var p = f.parent;
if (typeof p === "undefined") throw new Error("f.parent must resolve");
// Inner is single-instance, position 0
if (p.index !== 0) throw new Error("f.parent.index must be 0");
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    let _ = container; // suppress unused warning if any
    let _ = inner;
}

// ── Widget-config stubs ──────────────────────────────────────────────────

#[test]
fn handle_ui_choicelist_is_chainable_empty() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Field1", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var f = xfa.resolveNode("Field1");
if (f === null) throw new Error("Field1 should resolve");
var ui = f.ui;
if (typeof ui === "undefined") throw new Error("f.ui must be defined");
var cl = ui.choiceList;
if (!Array.isArray(cl)) throw new Error("ui.choiceList must be an array");
if (cl.length !== 0) throw new Error("ui.choiceList must start empty");
// Write to a deep stub: must not throw.
ui.choiceList.commitOn = "exit";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn handle_formatted_value_returns_raw_string() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Field1", "123.45");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var f = xfa.resolveNode("Field1");
if (f.formattedValue !== "123.45") {
  throw new Error("formattedValue must mirror rawValue under static flatten");
}
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn handle_exec_event_is_chainable_no_op() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Field1", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
var f = xfa.resolveNode("Field1");
if (typeof f.execEvent !== "function") {
  throw new Error("execEvent must be a function");
}
var r = f.execEvent("calculate");
if (r !== undefined) throw new Error("execEvent must return undefined");
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

// ── Negative contract: resolveNode("DoesNotExist") stays null ────────────

#[test]
fn resolve_node_missing_path_returns_null_contract_preserved() {
    // XFA-DATA-M3C must not absorb the existing null contract from
    // m3b_phaseC_bindings::resolve_node_returns_null_for_missing_path.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let trigger = add_field(&mut tree, root, "Field1", "");

    add_js_script(
        &mut tree,
        trigger,
        "calculate",
        r#"
if (xfa.resolveNode("DoesNotExist") !== null) {
  throw new Error("resolveNode must return null for missing paths");
}
if (xfa.resolveNode("Foo.Bar.Baz") !== null) {
  throw new Error("resolveNode must return null for deep missing paths");
}
"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    // 2 resolve_failures because resolve_node MUST keep reporting misses
    // (cluster C verifies this; M3-C only adds quiet variants for
    // underscore-shorthand probes).
    assert_eq!(outcome.js_resolve_failures, 2);
}
