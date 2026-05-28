#![cfg(feature = "xfa-js-sandboxed")]

//! Wave 3 Agent W3-B — Event policy closure tests.
//!
//! Companion to:
//! - `docs/INST_MGR_ACTIVITY_POLICY.md` v4
//! - `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md`
//!   (decisions D1.B/D2.A/D3.A)
//! - `benchmarks/runs/xfa_enterprise_plan/product_quality_track/W3_B_REPORT.md`
//!
//! TESTS-ONLY. No engine behaviour change.
//!
//! These tests close the W3-B "policy closure" loop by pinning the CURRENT
//! engine semantics for the three operator-decision-pending activities:
//!
//! - `preSave`   (D1, status quo: deny during flatten)
//! - `preSubmit` (D2, status quo: deny during flatten)
//! - `click`     (D3, status quo: deny during flatten)
//!
//! Until an operator commits to flip a default, these activities MUST stay
//! denied at BOTH the dispatch layer AND the host-binding layer. This file
//! is the canonical regression net: any future refactor that silently
//! re-classifies one of these activities will trip a test.
//!
//! The matrix entry pinned by each test is documented inline so a reader
//! can trace any assertion back to `INST_MGR_ACTIVITY_POLICY.md` v4 §1.2.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    activity_allowed_for_sandbox, HostBindings, QuickJsRuntime, SANDBOX_ACTIVITY_ALLOWLIST,
};
use pdf_xfa::JsExecutionMode;
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

// ---------------------------------------------------------------------------
// Helpers (mirrored from m3b_phasePQ_event_semantics.rs — kept local so the
// suite stays self-contained and refactor-resilient).
// ---------------------------------------------------------------------------

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

fn basic_form(activity: &str, script: &str) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: "initial".to_string(),
        },
    );
    tree.get_mut(root).children = vec![field];
    add_js_script(&mut tree, field, activity, script);
    (tree, root)
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

// ---------------------------------------------------------------------------
// Test 1 — Allowlist constant pin (closure invariant)
// ---------------------------------------------------------------------------

/// Pins that preSave / preSubmit / click are NOT members of the canonical
/// allowlist. This is the policy-closure invariant: any future PR that adds
/// one of these activities WITHOUT first updating
/// `INST_MGR_ACTIVITY_POLICY.md` (operator decision D1/D2/D3) trips here.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.1 (allowlist) and §1.2
/// (denylist).
#[test]
fn w3b_closure_presave_presubmit_click_are_not_in_allowlist() {
    for denied in ["preSave", "preSubmit", "click"] {
        assert!(
            !SANDBOX_ACTIVITY_ALLOWLIST.contains(&denied),
            "{denied} must NOT be in SANDBOX_ACTIVITY_ALLOWLIST until operator \
             commits to flip the default (see INST_MGR_ACTIVITY_POLICY.md v4 \
             §6.1/§6.2/§6.3)"
        );
        assert!(
            !activity_allowed_for_sandbox(Some(denied)),
            "activity_allowed_for_sandbox({denied}) must return false in current build"
        );
    }
    // Sanity: the canonical 5-tuple is unchanged.
    assert_eq!(
        SANDBOX_ACTIVITY_ALLOWLIST.len(),
        5,
        "allowlist must still hold exactly 5 lifecycle activities"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — preSave dispatch deny (D1.A status quo)
// ---------------------------------------------------------------------------

/// preSave scripts must NOT execute during static flatten. The body must
/// never reach QuickJS — verified by attaching a `throw` body that would
/// raise `js_runtime_errors` if the dispatch gate were silently relaxed.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.2 R06 / §6.1 (D1.A).
#[test]
fn w3b_closure_presave_script_is_skipped_at_dispatch() {
    let (mut tree, root) = basic_form(
        "preSave",
        // If preSave ever runs, this throw bumps js_runtime_errors and the
        // assertion below catches the silent-default-change regression.
        "throw new Error('preSave must not execute during flatten');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 0,
        "preSave must NOT execute during flatten (D1.A status quo)"
    );
    assert!(
        outcome.js_skipped >= 1,
        "preSave must be recorded as js_skipped (saw {})",
        outcome.js_skipped
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "preSave body must never reach QuickJS — defence-in-depth violated if >0"
    );
    assert_eq!(
        outcome.js_mutations, 0,
        "preSave must not mutate any field values during flatten"
    );
    assert_eq!(
        outcome.js_instance_writes, 0,
        "preSave must not produce instanceManager writes during flatten"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — preSubmit dispatch deny (D2.A status quo)
// ---------------------------------------------------------------------------

/// preSubmit scripts must NOT execute during static flatten. Flatten is not
/// a transport operation; D2.A is the recommended permanent decision.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.2 R07 / §6.2 (D2.A).
#[test]
fn w3b_closure_presubmit_script_is_skipped_at_dispatch() {
    let (mut tree, root) = basic_form(
        "preSubmit",
        // Mutation attempt: if preSubmit ever runs, rawValue moves to 'leaked'
        // and js_mutations becomes >= 1; either assertion catches it.
        "Field1.rawValue = 'leaked-from-preSubmit';",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 0,
        "preSubmit must NOT execute during flatten (D2.A confirmed)"
    );
    assert!(
        outcome.js_skipped >= 1,
        "preSubmit must be recorded as js_skipped"
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "preSubmit body must never reach QuickJS"
    );
    assert_eq!(
        outcome.js_mutations, 0,
        "preSubmit must not mutate field values — transport is not flatten"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — click dispatch deny (D3.A status quo)
// ---------------------------------------------------------------------------

/// click scripts must NOT execute during static flatten. UI events have no
/// reproducible ordering in flatten; D3.A is the recommended permanent
/// decision and is cross-validated by `13275420.pdf` (10 pages with all
/// click handlers skipped).
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.2 R10 / §6.3 (D3.A).
#[test]
fn w3b_closure_click_script_is_skipped_at_dispatch() {
    let (mut tree, root) = basic_form(
        "click",
        // Mutation + throw: belt-and-suspenders — any leak surfaces as either
        // a non-zero js_mutations or a non-zero js_runtime_errors.
        "Field1.rawValue = 'clicked'; throw new Error('click must not run');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 0,
        "click must NOT execute during flatten (D3.A confirmed; target doc \
         13275420 depends on this)"
    );
    assert!(
        outcome.js_skipped >= 1,
        "click must be recorded as js_skipped"
    );
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_mutations, 0,
        "click must not mutate field values during flatten"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — Host-layer defence-in-depth for all three activities
// ---------------------------------------------------------------------------

/// Even if a future caller bypasses the dispatch gate and forwards a
/// preSave / preSubmit / click activity directly to the host-binding layer,
/// every mutating host call MUST refuse. This is the second leg of
/// defence-in-depth (§2 of the policy doc).
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §2.
#[test]
fn w3b_closure_host_layer_refuses_mutations_for_all_three_activities() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let row = add_node(&mut tree, "Row", FormNodeType::Subform);
    tree.get_mut(row).occur = Occur::repeating(0, Some(10), 1);
    let _row_value = {
        let v = add_node(
            &mut tree,
            "Value",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(row).children.push(v);
        v
    };
    tree.get_mut(root).children = vec![row];

    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);

    // The three policy-closure activities + a couple of close cousins to
    // anchor the assertion against the broader denylist.
    for denied_activity in ["preSave", "preSubmit", "click", "postSave", "mouseEnter"] {
        host.reset_per_script(row, Some(denied_activity));
        assert_eq!(
            host.instance_add(row),
            Err(()),
            "host must refuse instance_add when activity={denied_activity}"
        );
        assert_eq!(
            host.instance_remove(row, 0),
            Err(()),
            "host must refuse instance_remove when activity={denied_activity}"
        );
        assert_eq!(
            host.instance_set(row, 3),
            Err(()),
            "host must refuse instance_set when activity={denied_activity}"
        );
    }

    let metadata = host.take_metadata();
    assert_eq!(
        metadata.instance_writes, 0,
        "zero instance writes must be recorded across all denied activities"
    );
    assert!(
        metadata.binding_errors >= 15,
        "every refused mutation must increment binding_errors (3 mutating \
         methods × 5 activities = 15+); saw {}",
        metadata.binding_errors
    );
}

// ---------------------------------------------------------------------------
// Test 6 — Mixed-activity partitioning under policy closure
// ---------------------------------------------------------------------------

/// A single field with all THREE policy-closure activities AND one allowed
/// activity attached at once must partition cleanly: allowed runs, denied
/// stays silent, throws never count. Pins the "no cross-talk" property of
/// the dispatch gate.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.2 + §2.
#[test]
fn w3b_closure_mixed_three_denied_plus_one_allowed_partitions_correctly() {
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

    // 1 allowed activity (initialize) + 3 denied activities (preSave,
    // preSubmit, click). If any denied script body reached QuickJS, the
    // throw would surface as js_runtime_errors.
    add_js_script(
        &mut tree,
        field,
        "initialize",
        "Field1.rawValue = 'init-ok';",
    );
    for denied in ["preSave", "preSubmit", "click"] {
        add_js_script(
            &mut tree,
            field,
            denied,
            "throw new Error('forbidden in flatten');",
        );
    }

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 1,
        "only the initialize script must execute (saw {} executed)",
        outcome.js_executed
    );
    assert!(
        outcome.js_skipped >= 3,
        "all 3 denied scripts must be recorded as js_skipped (saw {})",
        outcome.js_skipped
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "denied scripts must never reach QuickJS — no throws can count"
    );
    assert!(
        outcome.js_mutations >= 1,
        "the allowed initialize script must have mutated Field1.rawValue"
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Activity-helper deny for the W3-B trio
// ---------------------------------------------------------------------------

/// Pins `activity_allowed_for_sandbox` returns false for preSave / preSubmit
/// / click under every realistic casing / surrounding-whitespace variant.
/// The helper is the canonical gate the dispatch path consults; any future
/// "tolerant" rewrite (case-insensitive, trim, alias-table) would trip here
/// and demand operator approval first.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v4 §1.1 (code reference).
#[test]
fn w3b_closure_activity_helper_denies_w3b_trio_under_all_casings() {
    let denied_variants = [
        "preSave",
        "preSubmit",
        "click",
        // Casing variants must NOT be treated as aliases of the allowlist:
        "PRESAVE",
        "PreSubmit",
        "CLICK",
        "Click",
        // Surrounding whitespace must NOT be silently trimmed:
        " preSave",
        "preSubmit ",
        "\tclick",
        // Common Adobe-spelling near-misses:
        "preSave ",
        "presubmit",
    ];

    for variant in denied_variants {
        assert!(
            !activity_allowed_for_sandbox(Some(variant)),
            "activity_allowed_for_sandbox({variant:?}) must return false; \
             permissive variants would constitute a silent default change"
        );
    }
}
