#![cfg(feature = "xfa-js-sandboxed")]

//! XFA-PRODUCT-DATA-BINDING-LONGTAIL — residual cluster regression tests.
//!
//! These tests pin the *post*-DATA2-02 state of the data-binding long-tail:
//!
//! - The Cluster C contract (`xfa.resolveNode("DoesNotExist") === null` AND
//!   the call bumps `js_resolve_failures`) MUST remain preserved.
//! - The DATA2-02 caption sentinel + `handle.resolveNode` short-circuit MUST
//!   keep `js_resolve_failures = 0` on the synthetic top-cluster pattern
//!   (60df78fe-style `this.caption.value` chains).
//! - The residual `implicit_function` cluster (top-1 in the Wave-1 Track B
//!   triage) is a *script-execution-order* issue and is intentionally NOT
//!   resolved here. The synthetic exemplar below shows that a forward
//!   reference to a not-yet-declared helper raises `js_runtime_errors` (the
//!   QuickJS `ReferenceError`) but does *not* bump `js_resolve_failures` —
//!   this is the channel separation that lets the triage report rank the
//!   cluster reliably.
//!
//! See `benchmarks/runs/xfa_enterprise_plan/product_quality_track/
//! B_DATA_BINDING_LONGTAIL_TRIAGE.md` for the full triage inventory.

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

fn add_js_script(tree: &mut FormTree, node_id: FormNodeId, activity: &str, script: &str) {
    tree.meta_mut(node_id).event_scripts = vec![EventScript::new(
        script.to_string(),
        ScriptLanguage::JavaScript,
        Some(activity.to_string()),
        None,
        None,
    )];
}

fn basic_form(activity: &str, script: &str) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: String::new(),
        },
    );
    tree.get_mut(root).children = vec![field];
    add_js_script(&mut tree, field, activity, script);
    (tree, root)
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

// ── Cluster C contract — preserved across DATA2-02 + longtail triage ─────

#[test]
fn longtail_cluster_c_contract_resolvenode_missing_path_returns_null() {
    // This duplicates the existing canonical Cluster C test on purpose:
    // the Wave-1 Track B triage explicitly re-runs the contract pre + post
    // and this file is the agreed home for the post-triage rerun. Keep in
    // lockstep with:
    //   * crates/pdf-xfa/tests/m3b_phaseC_bindings.rs
    //     :: resolve_node_missing_path_returns_null_and_counts_failure
    //   * crates/pdf-xfa/tests/m3c_binding_completeness.rs
    //     :: resolve_node_missing_path_returns_null_contract_preserved
    //   * crates/pdf-xfa/tests/m3b_phaseE_data2_02.rs
    //     :: handle_resolve_node_missing_path_returns_null
    let (mut tree, root) = basic_form(
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
    // Two misses → at least two resolve_failures. The host's
    // xfa.resolveNodeId bumps the counter regardless of the access path.
    assert!(
        outcome.js_resolve_failures >= 2,
        "Cluster C contract: miss MUST bump resolve_failures, got {}",
        outcome.js_resolve_failures,
    );
}

// ── DATA2-02 caption sentinel — still silent ─────────────────────────────

#[test]
fn longtail_caption_sentinel_does_not_bump_resolve_failures() {
    // 60df78fe regression guard. Before DATA2-02 this pattern produced
    // thousands of resolve_failures per document; after the fix it must
    // stay at zero. If this ever regresses, the implicit_function cluster
    // ranking in the Track B triage becomes misleading because the
    // dynamic_property cluster would dominate again.
    let (mut tree, root) = basic_form(
        "calculate",
        r#"
// Mirror the 60df78fe initializer pattern.
var c = this.caption;
if (c === undefined) throw new Error("caption sentinel missing");
if (c.value !== "")  throw new Error("caption.value not empty");
if (c.text  !== "")  throw new Error("caption.text not empty");
// Deep-chain read through the null-data sentinel — must not throw.
if (this.caption.font.value !== null)
  throw new Error("caption.font.value must be null terminal");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_resolve_failures, 0,
        "DATA2-02 caption sentinel must keep resolve_failures at 0",
    );
}

// ── Residual cluster #1: implicit_function (script-order) ────────────────

#[test]
fn longtail_implicit_function_forward_reference_surfaces_as_resolve_failure() {
    // This pins the *current* observable channel for the top-1 residual
    // cluster in the Track B triage (`implicit_function`, impact=79).
    // The user-defined helper is referenced before it is defined (the
    // canonical multi-script XFA execution-order issue called out by the
    // DATA2-01 root-cause analysis).
    //
    // The sandbox routes unknown bare identifiers through the SOM
    // resolver, which means the miss surfaces as BOTH:
    //   - `js_resolve_failures += 1` (the SOM `resolve_path:NoMatch`
    //     event the triage scanner uses to classify the cluster)
    //   - `js_runtime_errors   += 1` (the eventual ReferenceError when
    //     the resolved value is `undefined` and the script tries to
    //     invoke it)
    //
    // This dual signal is what allows
    // `scripts/xfa_unresolved_identifier_scan.py` to bucket
    // `validateForm` / `getDeltas` / `CoreFunctions` under the
    // `implicit_function` cluster: the path text is surfaced via
    // `XFA_JS_DEBUG resolve_path:NoMatch`. When a Wave-2 fix lands
    // (collect-then-execute, per XFA §25.3), this test should flip to a
    // clean run — `js_resolve_failures == 0`. That is the deliberate
    // canary signal that the cluster has been closed.
    let (mut tree, root) = basic_form(
        "calculate",
        r#"
// Forward reference: `validateForm` is never declared in this script.
// In real corpora it is declared in a different script block that
// would execute after this one.
try {
  validateForm();
} catch (e) {
  // Re-throw so we can observe js_runtime_errors. The host catches and
  // counts this; the script itself is allowed to rethrow.
  throw e;
}
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert!(
        outcome.js_resolve_failures >= 1,
        "implicit_function cluster MUST surface as resolve_failure so the \
         triage scanner can classify it; got {}",
        outcome.js_resolve_failures,
    );
    assert!(
        outcome.js_runtime_errors >= 1,
        "forward reference must also surface as a runtime error \
         (ReferenceError on the undefined callee), got {}",
        outcome.js_runtime_errors,
    );
}
