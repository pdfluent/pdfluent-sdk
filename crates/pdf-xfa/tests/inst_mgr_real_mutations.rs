#![cfg(feature = "xfa-js-sandboxed")]

//! XFA-INST-MGR-REAL-MUTATIONS (2026-05-17)
//!
//! End-to-end regression tests proving that non-click `instanceManager`
//! calls — `addInstance`, `removeInstance`, `setInstances` — invoked from
//! every allowed XFA activity (`initialize`, `calculate`, `validate`,
//! `docReady`, `layoutReady`) reach the **layout DOM**, not just the
//! `js_instance_writes` counter.
//!
//! These tests are the contractual proof that the script -> form -> layout
//! pipeline is honest: a recorded instance write must produce an additional
//! laid-out node, and a removed instance must vanish from the layout pass.
//!
//! Companion tests assert the negative contract: click-event mutations are
//! sandboxed away during flatten (no layout effect, no cross-document leak),
//! and rollback-triggering scripts undo *structural* mutations too — not
//! merely the field values they touched.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{HostBindings, QuickJsRuntime};
use pdf_xfa::JsExecutionMode;
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::layout::LayoutEngine;
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

fn build_row_form(min: u32, max: Option<u32>) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let row = add_child(&mut tree, root, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(min, max, 1);
    let _value = add_field(&mut tree, row, "Value", "");
    (tree, root)
}

/// Count distinct laid-out form-node ids reachable through `LayoutPage.nodes`
/// (recursively descending into children). This is the load-bearing observable
/// for "did the layout DOM see the mutation?".
fn layout_form_node_count(dom: &xfa_layout_engine::layout::LayoutDom) -> usize {
    fn walk(node: &xfa_layout_engine::layout::LayoutNode, seen: &mut Vec<FormNodeId>) {
        if !seen.contains(&node.form_node) {
            seen.push(node.form_node);
        }
        for child in &node.children {
            walk(child, seen);
        }
    }
    let mut seen = Vec::new();
    for page in &dom.pages {
        for node in &page.nodes {
            walk(node, &mut seen);
        }
    }
    seen.len()
}

fn count_layout_subforms_named(
    dom: &xfa_layout_engine::layout::LayoutDom,
    form: &FormTree,
    name: &str,
) -> usize {
    fn walk(
        node: &xfa_layout_engine::layout::LayoutNode,
        form: &FormTree,
        name: &str,
        n: &mut usize,
    ) {
        if form.get(node.form_node).name == name
            && matches!(form.get(node.form_node).node_type, FormNodeType::Subform)
        {
            *n += 1;
        }
        for child in &node.children {
            walk(child, form, name, n);
        }
    }
    let mut n = 0;
    for page in &dom.pages {
        for node in &page.nodes {
            walk(node, form, name, &mut n);
        }
    }
    n
}

/// Drive a script then run the layout engine over the resulting FormTree.
/// Asserts the script returned a sandbox outcome with no runtime errors.
fn script_then_layout(
    mut tree: FormTree,
    root: FormNodeId,
) -> (
    FormTree,
    pdf_xfa::DynamicScriptOutcome,
    xfa_layout_engine::layout::LayoutDom,
) {
    let outcome = run_sandbox(&mut tree, root);
    let dom = LayoutEngine::new(&tree)
        .layout(root)
        .expect("layout succeeds after scripts");
    (tree, outcome, dom)
}

// -- Allowed activities × instanceManager methods × layout DOM proof. --------

#[test]
fn initialize_add_instance_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(1, Some(10));
    // Drive the addInstance from a sibling field whose event_scripts run at
    // `initialize`. We do NOT attach the script to `Row` itself, because doing
    // so would scope `Row` as `this` and bind `instanceManager` to that single
    // node; we want the implicit-resolver path that real corpus scripts use
    // (`Row.instanceManager.addInstance(1)`).
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "initialize",
        "Row.instanceManager.addInstance();",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(
        outcome.js_instance_writes, 1,
        "initialize addInstance must record an instance write"
    );
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        count_layout_subforms_named(&dom, &tree, "Row"),
        2,
        "layout DOM must observe the clone: 1 prototype + 1 addInstance"
    );
}

#[test]
fn initialize_set_instances_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "initialize",
        "Row.instanceManager.setInstances(3);",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.js_instance_writes, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        count_layout_subforms_named(&dom, &tree, "Row"),
        3,
        "setInstances(3) must produce 3 laid-out Row instances"
    );
}

#[test]
fn initialize_remove_instance_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    // Grow to 3 then drop the middle instance — net 2 in layout.
    add_js_script(
        &mut tree,
        driver,
        "initialize",
        "Row.instanceManager.setInstances(3); Row.instanceManager.removeInstance(1);",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.js_instance_writes, 2);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        count_layout_subforms_named(&dom, &tree, "Row"),
        2,
        "removeInstance(1) after setInstances(3) must leave 2 laid-out Rows"
    );
}

#[test]
fn calculate_add_instance_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(1, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        "Row.instanceManager.addInstance();",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert!(outcome.js_instance_writes >= 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert!(
        count_layout_subforms_named(&dom, &tree, "Row") >= 2,
        "calculate addInstance must reach the layout DOM"
    );
}

#[test]
fn validate_set_instances_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "validate",
        "Row.instanceManager.setInstances(2);",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.js_instance_writes, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(count_layout_subforms_named(&dom, &tree, "Row"), 2);
}

#[test]
fn doc_ready_set_instances_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "docReady",
        "Row.instanceManager.setInstances(2);",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.js_instance_writes, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(count_layout_subforms_named(&dom, &tree, "Row"), 2);
}

#[test]
fn layout_ready_set_instances_mutates_layout_dom() {
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "layoutReady",
        "Row.instanceManager.setInstances(2);",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.js_instance_writes, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(count_layout_subforms_named(&dom, &tree, "Row"), 2);
}

// -- Sandbox / activity enforcement. -----------------------------------------

#[test]
fn click_activity_instance_mutation_is_sandboxed_at_dispatch() {
    // `click` is not in the SANDBOX_ACTIVITY_ALLOWLIST: the dispatch layer
    // skips the script entirely (no host call, no runtime error). Layout
    // must observe the original prototype only.
    let (mut tree, root) = build_row_form(1, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "click",
        "Row.instanceManager.addInstance(); Row.instanceManager.addInstance();",
    );

    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(
        outcome.js_instance_writes, 0,
        "click-event instance mutations must be sandboxed away during flatten"
    );
    assert_eq!(outcome.js_executed, 0);
    assert!(outcome.js_skipped >= 1);
    assert_eq!(
        count_layout_subforms_named(&dom, &tree, "Row"),
        1,
        "layout must see only the prototype Row when click handler is skipped"
    );
}

#[test]
fn click_activity_instance_mutation_is_blocked_at_host_layer() {
    // Even if a malicious or future caller forwards a `click` activity past
    // the dispatch boundary, the host policy must refuse the mutation.
    // Defence-in-depth for the activity allowlist.
    let (mut tree, root) = build_row_form(0, Some(10));
    let row = tree.get(root).children[0];
    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    for forbidden_activity in ["click", "preSubmit", "mouseEnter", "ready"] {
        host.reset_per_script(row, Some(forbidden_activity));
        assert_eq!(
            host.instance_add(row),
            Err(()),
            "host must refuse addInstance on activity={forbidden_activity}"
        );
        assert_eq!(host.instance_set(row, 3), Err(()));
        assert_eq!(host.instance_remove(row, 0), Err(()));
    }
    let metadata = host.take_metadata();
    assert_eq!(
        metadata.instance_writes, 0,
        "no instance writes should be recorded across the four forbidden activities"
    );
}

// -- Rollback symmetry: structural mutations must roll back too. -------------

#[test]
fn rollback_undoes_structural_instance_writes() {
    // A pass that produces both populated-field-blanking AND structural
    // clones must roll BOTH back when the rollback heuristic fires.  Before
    // XFA-INST-MGR the rollback only restored field values; this test now
    // pins the symmetric behaviour.
    //
    // Construction: two populated fields + a Row prototype. The initialize
    // script (a) addInstance on Row and (b) clears both populated fields.
    // The rollback heuristic fires because >50% of populated fields go
    // empty; layout must see the ORIGINAL Row count after restore.
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field_a = add_field(&mut tree, root, "FieldA", "alpha");
    let field_b = add_field(&mut tree, root, "FieldB", "beta");
    let row = add_child(&mut tree, root, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(0, Some(10), 1);
    let _row_value = add_field(&mut tree, row, "Value", "");
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "initialize",
        "Row.instanceManager.addInstance(); FieldA.rawValue = ''; FieldB.rawValue = '';",
    );

    let initial_node_count = tree.nodes.len();
    let outcome = run_sandbox(&mut tree, root);

    // Rollback must have fired (changes wiped); layout must see 1 Row.
    assert_eq!(outcome.changes, 0, "rollback must zero out changes");
    let dom = LayoutEngine::new(&tree)
        .layout(root)
        .expect("layout after rollback");
    assert_eq!(
        count_layout_subforms_named(&dom, &tree, "Row"),
        1,
        "rolled-back addInstance must NOT survive in the layout DOM",
    );
    // FieldA/FieldB are restored.
    match &tree.get(field_a).node_type {
        FormNodeType::Field { value } => assert_eq!(value, "alpha"),
        _ => panic!("expected field"),
    }
    match &tree.get(field_b).node_type {
        FormNodeType::Field { value } => assert_eq!(value, "beta"),
        _ => panic!("expected field"),
    }
    // And the form tree is structurally back to its pre-script shape.
    assert_eq!(
        tree.nodes.len(),
        initial_node_count,
        "rollback must truncate any runtime-cloned nodes",
    );
}

#[test]
fn successful_pass_keeps_instance_writes_visible_to_layout() {
    // Counterpart to the rollback test: when the heuristic does NOT fire,
    // structural mutations remain visible to layout. This guards against
    // over-aggressive rollback regressions sneaking in.
    let (mut tree, root) = build_row_form(0, Some(10));
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "initialize",
        "Row.instanceManager.setInstances(4);",
    );

    let initial_node_count = tree.nodes.len();
    let (tree, outcome, dom) = script_then_layout(tree, root);

    assert_eq!(outcome.changes, 1);
    assert_eq!(outcome.js_instance_writes, 1);
    assert!(
        tree.nodes.len() > initial_node_count,
        "kept clones must enlarge the form tree node vec"
    );
    assert_eq!(count_layout_subforms_named(&dom, &tree, "Row"), 4);
    // Sanity: layout itself produced strictly more nodes than the prototype-
    // only baseline would have. Lower bound proves end-to-end propagation.
    assert!(layout_form_node_count(&dom) >= 4);
}

// -- Cross-document isolation: per-document reset must scrub clones. ---------

#[test]
fn second_document_does_not_inherit_first_document_clones() {
    // Run two independent documents through the SAME QuickJs runtime
    // adapter. Document A grows its Row count; document B must start from
    // its own prototype with no leakage from A.
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

    // Document A: setInstances(3).
    let (mut tree_a, root_a) = build_row_form(0, Some(10));
    let driver_a = add_field(&mut tree_a, root_a, "Driver", "");
    add_js_script(
        &mut tree_a,
        driver_a,
        "initialize",
        "Row.instanceManager.setInstances(3);",
    );
    let outcome_a = apply_dynamic_scripts_with_runtime(
        &mut tree_a,
        root_a,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("doc A dispatch");
    assert_eq!(outcome_a.js_instance_writes, 1);
    let dom_a = LayoutEngine::new(&tree_a)
        .layout(root_a)
        .expect("doc A layout");
    assert_eq!(count_layout_subforms_named(&dom_a, &tree_a, "Row"), 3);

    // Document B: no instanceManager call, must remain at the prototype.
    let (mut tree_b, root_b) = build_row_form(1, Some(10));
    let driver_b = add_field(&mut tree_b, root_b, "Driver", "");
    add_js_script(
        &mut tree_b,
        driver_b,
        "initialize",
        "Driver.rawValue = 'B';",
    );
    let outcome_b = apply_dynamic_scripts_with_runtime(
        &mut tree_b,
        root_b,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("doc B dispatch");
    assert_eq!(
        outcome_b.js_instance_writes, 0,
        "document B must not inherit clones from document A"
    );
    let dom_b = LayoutEngine::new(&tree_b)
        .layout(root_b)
        .expect("doc B layout");
    assert_eq!(
        count_layout_subforms_named(&dom_b, &tree_b, "Row"),
        1,
        "document B's Row count is unaffected by document A's mutations"
    );
}
