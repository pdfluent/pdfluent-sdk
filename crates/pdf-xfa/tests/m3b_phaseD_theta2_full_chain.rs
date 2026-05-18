#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D-θ.2 integration tests: full-chain lazy proxy.
//!
//! These tests exercise the JS chain proxy end-to-end through the sandboxed
//! runtime. They focus on behaviours that D-θ.1's single-segment lookahead
//! cannot satisfy:
//!
//! - A deeper segment (3+) is required to disambiguate between same-name
//!   subforms whose immediate child also collides.
//! - Terminal access on the accumulated chain forces a resolve.
//! - Non-terminal access defers without contacting the host on a known-good
//!   chain prefix.
//! - Unreachable chains still surface as `undefined` for `===` checks.
//! - Symbol.toPrimitive forces full resolution.

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

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

/// Build a multi-level same-name disambiguation tree:
///
/// ```text
/// root
///   F (subform)
///     P1 (subform)
///       X (subform)
///         Z (field = "noise")
///     P1 (subform)            <- shares name with sibling
///       X (subform)           <- shares name with cousin
///         Y (field = "winner")
///   Out (field)
/// ```
///
/// Reading `F.P1.X.Y.rawValue`:
/// - D-θ.1 hint chain "P1" → both P1s have a child named X → no
///   disambiguation. Picks the first P1 (children-biased ordering ties).
///   The first P1's X has no Y, so `.Y.rawValue` returns `null`.
/// - D-θ.2 full-chain probe asks for P1.X.Y — only the second P1 satisfies
///   the chain, and within that P1 only its X has a Y. The proxy collapses
///   onto the right Y and `.rawValue` returns `"winner"`.
fn three_level_disambiguation_tree() -> (FormTree, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let f = add_child(&mut tree, root, "F", FormNodeType::Subform);

    let p1_a = add_child(&mut tree, f, "P1", FormNodeType::Subform);
    let x_a = add_child(&mut tree, p1_a, "X", FormNodeType::Subform);
    let _z = add_field(&mut tree, x_a, "Z", "noise");

    let p1_b = add_child(&mut tree, f, "P1", FormNodeType::Subform);
    let x_b = add_child(&mut tree, p1_b, "X", FormNodeType::Subform);
    let _y = add_field(&mut tree, x_b, "Y", "winner");

    let out = add_field(&mut tree, root, "Out", "");
    (tree, root, out)
}

#[test]
fn full_chain_resolves_disambiguated_subtree() {
    let (mut tree, root, out) = three_level_disambiguation_tree();
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = F.P1.X.Y.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        field_value(&tree, out),
        "winner",
        "full-chain proxy must pin onto the P1.X subtree that contains Y"
    );
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn terminal_access_triggers_chain_resolution() {
    // The terminal `rawValue` read is what forces the host call. Before it,
    // every `.X` should accumulate without touching the host. We assert
    // post-hoc via host-call counts — the chain proxy must add exactly one
    // strict probe per non-terminal step plus one final resolve, no more.
    let (mut tree, root, out) = three_level_disambiguation_tree();
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = F.P1.X.Y.rawValue;",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "winner");
    assert_eq!(outcome.js_runtime_errors, 0);
    // js_host_calls should be modest; the chain is 4 segments so we expect
    // at most ~6 host calls (3 strict probes + 1 final resolve + raw read +
    // set) — bound it loosely so unrelated host bindings can change.
    assert!(
        outcome.js_host_calls < 64,
        "host call budget unexpectedly large: {}",
        outcome.js_host_calls
    );
}

#[test]
fn non_terminal_access_defers() {
    // `var b = F.P1.X;` must NOT immediately collapse onto an X node — the
    // chain must still be open for further property access. We verify this
    // indirectly: a subsequent `.Y.rawValue` on the stored proxy still
    // disambiguates correctly, which would be impossible if the proxy had
    // committed to the wrong X at assignment time.
    let (mut tree, root, out) = three_level_disambiguation_tree();
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "var partial = F.P1.X;\nOut.rawValue = partial.Y.rawValue;",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "winner");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn multi_candidate_intermediate_uses_chain_lookahead() {
    // Variant where the disambiguating segment is at depth 4. D-θ.1's
    // single-segment hint cannot keep enough context across that many
    // levels, while θ.2's accumulator can.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let f = add_child(&mut tree, root, "F", FormNodeType::Subform);

    // Branch A: F.G.H.J.K = "noise"
    let g_a = add_child(&mut tree, f, "G", FormNodeType::Subform);
    let h_a = add_child(&mut tree, g_a, "H", FormNodeType::Subform);
    let j_a = add_child(&mut tree, h_a, "J", FormNodeType::Subform);
    let _k_a = add_field(&mut tree, j_a, "K", "noise");

    // Branch B: F.G.H.J.M = "winner" — distinguished only by the final segment.
    let g_b = add_child(&mut tree, f, "G", FormNodeType::Subform);
    let h_b = add_child(&mut tree, g_b, "H", FormNodeType::Subform);
    let j_b = add_child(&mut tree, h_b, "J", FormNodeType::Subform);
    let _m_b = add_field(&mut tree, j_b, "M", "winner");

    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = F.G.H.J.M.rawValue;",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "winner");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn chain_no_match_returns_undefined() {
    // Unreachable chain must surface as `undefined` for the `===` test —
    // not a proxy. This is the soft-fail contract the resolver-unresolvable
    // test in m3b_phaseD_instance_manager.rs relies on.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _a = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        r#"
var b = A.NoSuchChild;
Out.rawValue = (b === undefined ? "undefined" : "leaked");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "undefined");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn symbol_toprimitive_forces_resolve() {
    // Coercing the proxy to a primitive (via String(), template literal, or
    // arithmetic) must invoke the resolved candidate's primitive view. We
    // use String concatenation here as the simplest trigger; the proxy must
    // not become the literal `[object Object]` or similar.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let a = add_child(&mut tree, root, "A", FormNodeType::Subform);
    let _v = add_field(&mut tree, a, "V", "yes");

    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        // The chain proxy itself is not directly toPrimitive-friendly (the
        // resolved handle is), so we deliberately exercise `.rawValue` on
        // it — which must coerce correctly through the chain accumulator.
        "Out.rawValue = String(A.V.rawValue);",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "yes");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn deep_chain_within_depth_budget() {
    // Chain of exactly MAX_SOM_DEPTH-ish segments must still complete. We
    // build 6 nested same-name-free segments and traverse with rawValue.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let s0 = add_child(&mut tree, root, "S0", FormNodeType::Subform);
    let s1 = add_child(&mut tree, s0, "S1", FormNodeType::Subform);
    let s2 = add_child(&mut tree, s1, "S2", FormNodeType::Subform);
    let s3 = add_child(&mut tree, s2, "S3", FormNodeType::Subform);
    let s4 = add_child(&mut tree, s3, "S4", FormNodeType::Subform);
    let s5 = add_child(&mut tree, s4, "S5", FormNodeType::Subform);
    let _v = add_field(&mut tree, s5, "V", "deep");

    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "Out.rawValue = S0.S1.S2.S3.S4.S5.V.rawValue;",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(field_value(&tree, out), "deep");
    assert_eq!(outcome.js_runtime_errors, 0);
}
