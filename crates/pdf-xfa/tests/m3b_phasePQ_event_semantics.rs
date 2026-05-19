#![cfg(feature = "xfa-js-sandboxed")]

//! Track C (Product Quality Wave 1) — event execution semantics tests.
//!
//! Companion to
//! `benchmarks/runs/xfa_enterprise_plan/product_quality_track/TRACK_JS_RUNTIME_SEMANTICS.md`
//! and the decision matrix in
//! `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md`.
//!
//! TESTS-ONLY. No engine behaviour change.
//!
//! These tests pin the CURRENT event execution semantics of the sandboxed JS
//! runtime so that future refactors cannot silently mutate the allowlist /
//! denylist contract. The matrix today is:
//!
//! Allowlist (executed during static flatten):
//!   - `initialize`
//!   - `calculate`
//!   - `validate`
//!   - `docReady`
//!   - `layoutReady`
//!
//! Denylist (silently skipped, recorded as `js_skipped`):
//!   - `preSave` (operator-decision-pending; today: deny)
//!   - `preSubmit`
//!   - `click`
//!   - all UI events (`enter` / `exit` / `mouseEnter` / `mouseExit` / `change`)
//!
//! See `INST_MGR_ACTIVITY_POLICY.md` v3 for the consolidated policy.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    activity_allowed_for_sandbox, QuickJsRuntime, SANDBOX_ACTIVITY_ALLOWLIST,
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

fn add_js_script(tree: &mut FormTree, node_id: FormNodeId, activity: &str, script: &str) {
    tree.meta_mut(node_id).event_scripts.push(EventScript::new(
        script.to_string(),
        ScriptLanguage::JavaScript,
        Some(activity.to_string()),
        None,
        None,
    ));
}

fn basic_form(activity: &str, script: &str) -> (FormTree, FormNodeId, FormNodeId) {
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
    add_js_script(&mut tree, field, activity, script);
    (tree, root, field)
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

// ---------------------------------------------------------------------------
// Allowlist — every allowed activity executes the script body.
// ---------------------------------------------------------------------------

/// Pin the constant: the allowlist is the canonical 5-tuple. Any drift would
/// either drop a customer-required activity (regression) or introduce one
/// without policy review (semantic drift).
#[test]
fn allowlist_constant_matches_documented_five_activities() {
    let mut sorted: Vec<&str> = SANDBOX_ACTIVITY_ALLOWLIST.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        vec![
            "calculate",
            "docReady",
            "initialize",
            "layoutReady",
            "validate",
        ],
        "SANDBOX_ACTIVITY_ALLOWLIST has drifted from the policy doc; \
         update INST_MGR_ACTIVITY_POLICY.md before changing this constant"
    );
}

/// Each of the five allowlist activities, when attached to a Field script,
/// executes (js_executed = 1, js_skipped = 0). This is the positive contract
/// equivalent to the negative `click` test in `inst_mgr_real_mutations.rs`.
#[test]
fn every_allowlist_activity_executes_during_flatten() {
    for activity in SANDBOX_ACTIVITY_ALLOWLIST {
        let (mut tree, root, _f) = basic_form(activity, "1 + 1;");
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(
            outcome.js_executed, 1,
            "activity {activity} must execute under sandbox dispatch"
        );
        assert_eq!(
            outcome.js_skipped, 0,
            "activity {activity} must not be skipped"
        );
        assert_eq!(
            outcome.js_runtime_errors, 0,
            "activity {activity} must not raise runtime errors on trivial body"
        );
    }
}

/// Denylist coverage: every UI / save / submit activity skips the runtime.
/// These activities never fire during static flatten — they are interactive
/// or transport-time only. Skipped is silent (no runtime error).
#[test]
fn every_denylist_activity_is_silently_skipped_during_flatten() {
    let denylist = [
        // Save / submit
        "preSave",
        "preSubmit",
        "postSave",
        "postSubmit",
        // UI events
        "click",
        "change",
        "enter",
        "exit",
        "mouseEnter",
        "mouseExit",
        "mouseDown",
        "mouseUp",
        // Print / open hooks
        "prePrint",
        "postPrint",
        "preOpen",
        // Adobe spelling variants
        "ready",
        "full",
    ];
    for activity in denylist {
        let (mut tree, root, _f) = basic_form(activity, "throw new Error('must not run');");
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(
            outcome.js_executed, 0,
            "activity {activity} must NOT execute during flatten"
        );
        assert!(
            outcome.js_skipped >= 1,
            "activity {activity} must be recorded as js_skipped"
        );
        assert_eq!(
            outcome.js_runtime_errors, 0,
            "activity {activity}: skipped scripts must not surface runtime errors \
             (defence-in-depth: body never reaches QuickJS)"
        );
    }
}

/// `activity_allowed_for_sandbox` is the canonical helper the dispatch layer
/// uses. Pin its behaviour explicitly so a refactor away from the const
/// cannot silently relax the gate.
#[test]
fn activity_allowed_helper_matches_allowlist_membership() {
    for allowed in SANDBOX_ACTIVITY_ALLOWLIST {
        assert!(
            activity_allowed_for_sandbox(Some(allowed)),
            "{allowed} must be allowed"
        );
    }
    for denied in [
        "preSave",
        "preSubmit",
        "click",
        "change",
        "enter",
        "ready",
        "Initialize", // case-sensitive
        "INITIALIZE", // case-sensitive
        "",           // empty string
    ] {
        assert!(
            !activity_allowed_for_sandbox(Some(denied)),
            "{denied} must not be allowed"
        );
    }
    // None (no activity attribute) is denied — scripts without an explicit
    // activity must not run, because we cannot reason about ordering.
    assert!(!activity_allowed_for_sandbox(None));
}

// ---------------------------------------------------------------------------
// Execution order — initialize → calculate → validate → docReady → layoutReady
// ---------------------------------------------------------------------------

/// Execution order: when one node carries scripts for several allowlist
/// activities, all of them run, each exactly once. We do not assert a
/// strict ordering of XFA event phases here (that is enforced by the
/// FormCalc dispatch path); we only pin that each allowed activity is
/// honoured even when the same node has multiple events attached.
#[test]
fn multiple_allowlist_activities_on_same_node_all_execute() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "x".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field];

    // Five scripts, one per allowlist activity, all on the same node.
    for activity in SANDBOX_ACTIVITY_ALLOWLIST {
        add_js_script(
            &mut tree, field, activity,
            // Simple side-effect-free body so the order doesn't matter for
            // correctness — we only care that all five fire.
            "1 + 1;",
        );
    }

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 5,
        "all five allowlist scripts on the same node must run"
    );
    assert_eq!(outcome.js_skipped, 0);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// Mixed allow / deny on the same node: allowed scripts run, denied scripts
/// are skipped, errors stay clean (denied script body never reaches QuickJS).
#[test]
fn mixed_allow_and_deny_on_same_node_partitions_correctly() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "x".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field];

    // 3 allowed × 4 denied.
    for activity in ["initialize", "calculate", "validate"] {
        add_js_script(&mut tree, field, activity, "1 + 1;");
    }
    for activity in ["click", "preSubmit", "mouseEnter", "preSave"] {
        // If these ever ran, the throw would surface as a runtime error.
        add_js_script(&mut tree, field, activity, "throw new Error('forbidden');");
    }

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 3, "3 allowlist scripts must execute");
    assert!(
        outcome.js_skipped >= 4,
        "4 denied scripts must be skipped (saw {})",
        outcome.js_skipped
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "denied scripts must never reach QuickJS, so throws cannot count"
    );
}

// ---------------------------------------------------------------------------
// Post-execution state extraction
// ---------------------------------------------------------------------------

/// An allowed-activity script that writes to a field value via the documented
/// host binding (`Field1.rawValue = "X"`) is observable on the FormTree
/// post-flatten (`js_mutations` counter ≥ 1). This pins the contract that
/// the sandbox is not a silent no-op for the allowed surface.
#[test]
fn allowlist_calculate_script_mutation_is_observable_post_execution() {
    let (mut tree, root, _f) = basic_form("calculate", "Field1.rawValue = 'mutated';");
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert!(
        outcome.js_mutations >= 1,
        "calculate-time rawValue assignment must count as a mutation"
    );
}

/// Inverse contract: a denied-activity script that *would* mutate is silently
/// skipped — `js_mutations` stays 0 because the body never executes.
#[test]
fn denylist_click_script_mutation_does_not_apply_to_form_tree() {
    let (mut tree, root, _f) = basic_form("click", "Field1.rawValue = 'should-not-apply';");
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(
        outcome.js_mutations, 0,
        "click handler must not be allowed to mutate field values during flatten"
    );
    assert!(outcome.js_skipped >= 1);
}

/// Script-less activities count as no-op: zero js_executed, zero js_skipped,
/// zero js_runtime_errors. This is the cleanest baseline.
#[test]
fn no_scripts_means_zero_counters_across_the_board() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "x".to_string(),
        },
    );
    tree.get_mut(root).children = vec![_field];

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.js_skipped, 0);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_mutations, 0);
    assert!(
        !outcome.js_present,
        "absence of JS scripts must leave js_present false"
    );
}
