#![cfg(feature = "xfa-js-sandboxed")]

//! XFA-DATA2-02 cluster-fix integration tests.
//!
//! These tests pin the contract for the top-3 unresolved-identifier clusters
//! found by DATA2-01 on the 60df78fe / Tier A subset of the corpus:
//!
//! * `.caption` field-stub — `field.caption.value` / `.text` must return an
//!   empty string and never bump `js_resolve_failures`. The sub-element is
//!   absent from the merged FormTree by design; the chainable sentinel keeps
//!   downstream initializer scripts running instead of aborting on TypeError.
//! * `resolveNode` / `resolveNodes` as instance methods — `handle.resolveNode`
//!   must mirror the global `xfa.resolveNode` pair (data-path routing, null
//!   on miss, frozen empty list for the plural). The pre-existing Cluster C
//!   null-return contract for `xfa.resolveNode("DoesNotExist")` is preserved.
//! * `util` and `console` lexical visibility inside `<variables>` script
//!   bodies — functions registered there commonly close over both globals
//!   and previously raised `console is not defined` / `util is not defined`
//!   at first event-script invocation. The factory now injects both into
//!   the wrapper IIFE so the helpers resolve them via lexical scope.
//!
//! See `benchmarks/runs/xfa_enterprise_plan/sprint2/XFA_DATA2_01_CLUSTER_SCAN_REPORT.md`.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::QuickJsRuntime;
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

fn add_js_script(tree: &mut FormTree, node_id: FormNodeId, activity: &str, script: &str) {
    tree.meta_mut(node_id).event_scripts = vec![EventScript::new(
        script.to_string(),
        ScriptLanguage::JavaScript,
        Some(activity.to_string()),
        None,
        None,
    )];
}

fn basic_form(script_activity: &str, script: &str) -> (FormTree, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "hello".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field];
    add_js_script(&mut tree, field, script_activity, script);
    (tree, root, field)
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

// ---------------------------------------------------------------------------
// 1. caption sentinel
// ---------------------------------------------------------------------------

#[test]
fn caption_chain_on_field_returns_sentinel_with_empty_string_value() {
    // 60df78fe's pattern: thousands of `this.caption.value` / `.text` reads
    // on fields whose `<caption>` sub-element was not materialised into the
    // merged FormTree. Before DATA2-02 each access bumped resolve_failures
    // and threw a TypeError on `.value`.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
if (this.caption === undefined) throw new Error("caption must be sentinel, not undefined");
if (this.caption.value !== "") throw new Error("caption.value must be empty string, got " + this.caption.value);
if (this.caption.text  !== "") throw new Error("caption.text must be empty string");
if (this.caption.name  !== "caption") throw new Error("caption.name must be 'caption'");
if (this.caption.isNull !== true) throw new Error("caption.isNull must be true");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0, "caption chain must not throw");
    assert_eq!(
        outcome.js_resolve_failures, 0,
        "caption sentinel must NOT bump resolve_failures"
    );
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn caption_deep_chain_is_null_safe() {
    // Some XFA initializers walk `caption.font.typeface` or
    // `caption.value.text`. The sentinel routes unknown sub-property reads
    // through `makeNullDataHandle` so the deeper chain stays chainable and
    // never TypeErrors. Terminal reads through the null-data sentinel
    // yield primitives (`value`/`rawValue` → null, `length` → 0).
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
// caption.font is the null-data sentinel (chainable, never TypeError).
var font = this.caption.font;
if (font === null || font === undefined)
  throw new Error("caption.font must be a chainable sentinel, got " + font);
// Terminal probe through the sentinel chain: `.value` resolves to null.
if (this.caption.font.value !== null)
  throw new Error("expected null terminal, got " + this.caption.font.value);
// Top-level scalar accessors stay untouched.
var v = this.caption.value;
if (v !== "") throw new Error("caption.value not empty: " + v);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

#[test]
fn caption_writes_are_silently_absorbed() {
    // Some templates execute `this.caption.value = "Label"` defensively
    // before reading it back. The sentinel must absorb writes without error
    // (caption is a viewer-only field, never a flatten mutation channel)
    // and subsequent reads still return the empty default.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
this.caption.value = "ignored";
this.caption.text  = "ignored";
if (this.caption.value !== "") throw new Error("write must not stick");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

// ---------------------------------------------------------------------------
// 2. resolveNode / resolveNodes as instance methods
// ---------------------------------------------------------------------------

#[test]
fn handle_resolve_node_valid_path_returns_field_handle() {
    // 60df78fe pattern: `this.resolveNode("Sibling").rawValue`. Without
    // DATA2-02 the property read fell through to makeChainHandle and missed.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field1 = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "hello".to_string(),
        },
    );
    let field2 = add_node(
        &mut tree,
        "Field2",
        FormNodeType::Field {
            value: "world".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field1, field2];
    add_js_script(
        &mut tree,
        field1,
        "calculate",
        r#"
var f = this.resolveNode("Field2");
if (f === null) throw new Error("expected handle, got null");
if (f.rawValue !== "world") throw new Error("resolveNode rawValue mismatch: " + f.rawValue);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn handle_resolve_node_missing_path_returns_null() {
    // Cluster C contract: a miss STILL returns null, both via xfa.resolveNode
    // and via the new instance method. DATA2-02 must NOT swallow the miss
    // into a sentinel — scripts use the explicit null check to branch.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
if (this.resolveNode("DoesNotExist") !== null)
  throw new Error("missing path must return native null");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    // The host's xfa.resolveNodeId bumps resolve_failures for unknown paths
    // (Cluster C contract). The instance method routes through the same
    // host binding and must keep that observable behaviour.
    assert!(
        outcome.js_resolve_failures >= 1,
        "missing path must bump resolve_failures to preserve Cluster C contract"
    );
}

#[test]
fn handle_resolve_nodes_returns_frozen_array() {
    // Plural form: array of matches, frozen. Empty result is also a frozen
    // array (not null), matching `xfa.resolveNodes` semantics.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var nodes = this.resolveNodes("DoesNotExist");
if (!Array.isArray(nodes)) throw new Error("expected array");
if (nodes.length !== 0) throw new Error("expected empty array, got length " + nodes.length);
if (!Object.isFrozen(nodes)) throw new Error("expected frozen array");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

// ---------------------------------------------------------------------------
// 3. util host stub callable from variables-script closure
// ---------------------------------------------------------------------------

#[test]
fn variables_script_can_call_util_printd_via_closure() {
    // 0bf81862 / 5d6e30ad / ff2723ed / e2bb8995 pattern: helper functions
    // registered via <variables> close over `util` and `console`. Without
    // DATA2-02 the closure raised `util is not defined` at first event
    // invocation. We inject both into the wrapper IIFE so the closure
    // resolves them via lexical scope.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        None,
        "Helpers".into(),
        r#"
function formatToday() {
  // Exercise util.printd to prove `util` is in scope inside the closure.
  var d = new Date(Date.UTC(2026, 4, 18, 0, 0, 0));
  return util.printd("yyyy-mm-dd", d);
}
"#
        .into(),
    ));
    let out = add_node(
        &mut tree,
        "Out",
        FormNodeType::Field {
            value: "".to_string(),
        },
    );
    tree.get_mut(root).children = vec![out];
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = Helpers.formatToday();",
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "util must be lexically visible inside <variables> closure"
    );
    assert_eq!(outcome.js_executed, 1);
}

// ---------------------------------------------------------------------------
// 4. console host stub callable from variables-script closure
// ---------------------------------------------------------------------------

#[test]
fn variables_script_can_call_console_log_via_closure() {
    // Mirror of the util test: `console.log` inside a <variables> helper
    // must be a callable no-op, not a `ReferenceError`.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    tree.variables_scripts.push((
        None,
        "Logger".into(),
        r#"
function announce(msg) {
  // log / warn / error / info are all stubbed; the call must not throw.
  console.log("info:" + msg);
  console.warn("warn:" + msg);
  console.error("err:" + msg);
  return msg;
}
"#
        .into(),
    ));
    let out = add_node(
        &mut tree,
        "Out",
        FormNodeType::Field {
            value: "".to_string(),
        },
    );
    tree.get_mut(root).children = vec![out];
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"this.rawValue = Logger.announce("ok");"#,
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "console must be lexically visible inside <variables> closure"
    );
    assert_eq!(outcome.js_executed, 1);
}

// ---------------------------------------------------------------------------
// 5. Combined integration smoke: caption + resolveNode + util usage in one
//    script body, no resolve_failures, no runtime_errors.
// ---------------------------------------------------------------------------

#[test]
fn combined_data2_02_pattern_runs_cleanly() {
    // Mimics the high-frequency initializer pattern in 60df78fe: walk to a
    // sibling via resolveNode, read its caption, no failures should be
    // counted.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field1 = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "v1".to_string(),
        },
    );
    let field2 = add_node(
        &mut tree,
        "Field2",
        FormNodeType::Field {
            value: "v2".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field1, field2];
    add_js_script(
        &mut tree,
        field1,
        "initialize",
        r#"
var sib = this.resolveNode("Field2");
if (sib === null) throw new Error("sibling lookup failed");
// Touch the caption sub-element on the sibling and on `this`.
var cap1 = this.caption.value;       // sentinel "" — no resolve_failures bump
var cap2 = sib.caption.text;         // sentinel "" — no resolve_failures bump
if (cap1 !== "" || cap2 !== "") throw new Error("caption stub mismatch");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_resolve_failures, 0,
        "caption / resolveNode happy-path must produce zero resolve_failures"
    );
    assert_eq!(outcome.js_executed, 1);
}
