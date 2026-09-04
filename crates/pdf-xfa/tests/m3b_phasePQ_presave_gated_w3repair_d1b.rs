#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Wave 3 Repair — Agent D1.B preSave policy closure tests.
//!
//! Companion to:
//! - `docs/INST_MGR_ACTIVITY_POLICY.md` v5
//! - `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_02_EVENT_POLICY_MATRIX.md` (D1.B)
//! - `benchmarks/runs/xfa_enterprise_plan/product_quality_track/D1B_POLICY_CLOSURE_REPORT.md`
//!
//! These tests close the **D1.B gated allow** loop. The implementation
//! gates the `preSave` dispatch path behind the `XFA_PRESAVE_DURING_FLATTEN`
//! environment variable (default OFF) and threads the same gate into the
//! host-binding layer so both legs of defence-in-depth stay in lock-step.
//!
//! Stop-rules pinned here (any future "tolerant" or "broader" gate trips
//! these tests):
//!
//! 1. **Default OFF.** Without `XFA_PRESAVE_DURING_FLATTEN=1`, behaviour is
//!    byte-identical to v4 (W3-B closure). The 7 W3-B closure tests still
//!    PASS unchanged — see `m3b_phasePQ_event_policy_closure_w3b.rs`.
//! 2. **Only preSave.** When the gate is ON, `preSubmit` / `click` / every
//!    other denylist activity MUST stay denied at BOTH the dispatch layer
//!    AND the host-binding layer.
//! 3. **No silent flip.** The variable must equal exactly `"1"`; every other
//!    value (`"true"`, `"yes"`, casing variants, surrounding whitespace)
//!    keeps the gate OFF.
//!
//! The tests use a serial mutex around env-var manipulation because cargo
//! test runs in-process and `std::env` is global. Each test takes the lock,
//! sets the variable, performs the dispatch, captures the outcome, and
//! restores the previous value before dropping the lock.

use std::sync::{Mutex, MutexGuard, OnceLock};

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{
    activity_allowed_for_sandbox_with_gate, presave_during_flatten_enabled, HostBindings,
    QuickJsRuntime, ENV_PRESAVE_DURING_FLATTEN,
};
use pdf_xfa::JsExecutionMode;
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

// ---------------------------------------------------------------------------
// Env-var serial guard
// ---------------------------------------------------------------------------

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// RAII guard that sets `XFA_PRESAVE_DURING_FLATTEN` to `value` for the
/// duration of the guard's lifetime and restores the previous value on drop.
struct PresaveEnvGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<String>,
}

impl PresaveEnvGuard {
    fn set(value: &str) -> Self {
        let lock = env_lock();
        let previous = std::env::var(ENV_PRESAVE_DURING_FLATTEN).ok();
        // SAFETY: test-only env mutation guarded by `env_lock()` so no
        // other test can read/write the variable concurrently.
        std::env::set_var(ENV_PRESAVE_DURING_FLATTEN, value);
        Self {
            _lock: lock,
            previous,
        }
    }

    fn unset() -> Self {
        let lock = env_lock();
        let previous = std::env::var(ENV_PRESAVE_DURING_FLATTEN).ok();
        std::env::remove_var(ENV_PRESAVE_DURING_FLATTEN);
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for PresaveEnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(prev) => std::env::set_var(ENV_PRESAVE_DURING_FLATTEN, prev),
            None => std::env::remove_var(ENV_PRESAVE_DURING_FLATTEN),
        }
    }
}

// ---------------------------------------------------------------------------
// Form-builder helpers (mirrored from event_policy_closure_w3b.rs)
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
// Test 1 — Default-OFF behaviour matches v4 W3-B closure exactly
// ---------------------------------------------------------------------------

/// With `XFA_PRESAVE_DURING_FLATTEN` unset, the dispatch gate MUST deny
/// `preSave` (D1.A status quo). This is the load-bearing default contract.
/// If the default were ever to silently flip, this test would catch it
/// before the W3-B closure suite does.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 (D1.B default OFF).
#[test]
fn d1b_default_off_keeps_presave_denied_at_dispatch() {
    let _guard = PresaveEnvGuard::unset();
    assert!(
        !presave_during_flatten_enabled(),
        "env-var helper must report OFF when variable is unset"
    );
    let (mut tree, root) = basic_form(
        "preSave",
        // Throw body: if preSave ever runs, js_runtime_errors becomes >0.
        "throw new Error('preSave must not execute with gate OFF');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 0,
        "preSave must NOT execute with gate OFF"
    );
    assert!(
        outcome.js_skipped >= 1,
        "preSave must be recorded as js_skipped"
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "preSave body must not reach QuickJS"
    );
    assert_eq!(outcome.js_mutations, 0, "preSave must not mutate values");
    assert_eq!(outcome.js_instance_writes, 0);
}

// ---------------------------------------------------------------------------
// Test 2 — Flag-ON executes preSave script body
// ---------------------------------------------------------------------------

/// With `XFA_PRESAVE_DURING_FLATTEN=1`, the dispatch gate accepts `preSave`
/// and the script body reaches QuickJS. We use a mutation to prove the
/// body actually executed (rawValue moves from "initial" to "presave-ran").
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 (D1.B opt-in).
#[test]
fn d1b_flag_on_executes_only_presave() {
    let _guard = PresaveEnvGuard::set("1");
    assert!(
        presave_during_flatten_enabled(),
        "env-var helper must report ON when variable is exactly \"1\""
    );
    let (mut tree, root) = basic_form(
        "preSave",
        // Mutate Field1.rawValue so we can prove the body executed.
        "Field1.rawValue = 'presave-ran';",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 1,
        "preSave MUST execute exactly once with gate ON (saw {} executed)",
        outcome.js_executed
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "preSave body must run cleanly when allowed"
    );
    assert!(
        outcome.js_mutations >= 1,
        "preSave body must have mutated Field1.rawValue (saw {} mutations)",
        outcome.js_mutations
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Flag-ON keeps preSubmit / click denied (hard stop)
// ---------------------------------------------------------------------------

/// Hard-stop invariant: with `XFA_PRESAVE_DURING_FLATTEN=1`, only `preSave`
/// is unlocked. Every other denylist activity MUST stay denied at the
/// dispatch gate. We verify three representatives: `preSubmit` (D2.A),
/// `click` (D3.A), and `mouseEnter` (UI). Each carries a throw body —
/// any leak surfaces as a non-zero `js_runtime_errors`.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 hard-stop list.
#[test]
fn d1b_flag_on_still_denies_presubmit_and_click_and_mouse() {
    let _guard = PresaveEnvGuard::set("1");
    for denied in ["preSubmit", "click", "mouseEnter"] {
        let (mut tree, root) = basic_form(
            denied,
            "throw new Error('must stay denied even with gate ON');",
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(
            outcome.js_executed, 0,
            "{denied} MUST NOT execute even with D1.B gate ON",
        );
        assert!(
            outcome.js_skipped >= 1,
            "{denied} must be recorded as js_skipped (saw {})",
            outcome.js_skipped
        );
        assert_eq!(
            outcome.js_runtime_errors, 0,
            "{denied} body must never reach QuickJS",
        );
    }
}

// ---------------------------------------------------------------------------
// Test 4 — Env-var parsing: only "1" enables the gate
// ---------------------------------------------------------------------------

/// Pins the env-var parsing contract: any value other than the exact string
/// `"1"` keeps the gate OFF. Tolerant variants (`"true"`, `"yes"`, casing
/// variants, surrounding whitespace) would constitute a silent default
/// drift and are explicitly rejected.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §6.1 default-OFF contract.
#[test]
fn d1b_env_var_parsing_only_one_enables_gate() {
    // Values that MUST keep the gate OFF.
    for off_value in [
        "", "0", "true", "TRUE", "yes", "YES", "on", "ON", "TRUE", " 1", "1 ", " 1 ", "01", "1.0",
        "2",
    ] {
        let _guard = PresaveEnvGuard::set(off_value);
        assert!(
            !presave_during_flatten_enabled(),
            "value {off_value:?} must keep the gate OFF",
        );
    }
    // The ONE value that enables the gate.
    let _guard_on = PresaveEnvGuard::set("1");
    assert!(presave_during_flatten_enabled());
}

// ---------------------------------------------------------------------------
// Test 5 — Host-layer defence-in-depth mirrors the dispatch gate
// ---------------------------------------------------------------------------

/// Even if the dispatch path forwards a `preSave` activity to the host-
/// binding layer, the host MUST refuse mutations unless the same gate is
/// installed. With the gate ON, mutating host calls for `preSave` succeed
/// (subject to other invariants); calls for `preSubmit` / `click` / etc.
/// still refuse — defence-in-depth (§2 of the policy doc).
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §2 + §6.1.
#[test]
fn d1b_host_layer_gate_mirrors_dispatch_for_presave_only() {
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

    // --- Gate OFF: preSave refused at host layer (v4 W3-B baseline) ---
    host.set_presave_gate(false);
    host.reset_per_script(row, Some("preSave"));
    assert_eq!(
        host.instance_add(row),
        Err(()),
        "host must refuse instance_add for preSave when gate OFF"
    );

    // --- Gate ON: preSave accepted at host layer, others still refused ---
    host.set_presave_gate(true);
    assert!(host.presave_gate(), "gate setter must be observable");

    // preSave: accepted. instance_add returns Ok with the new instance idx.
    host.reset_per_script(row, Some("preSave"));
    assert!(
        host.instance_add(row).is_ok(),
        "host must accept instance_add for preSave when gate ON"
    );

    // Hard-stop: preSubmit, click, mouseEnter MUST stay refused even with
    // the D1.B gate ON. A broader "gate" implementation would trip here.
    for still_denied in ["preSubmit", "click", "mouseEnter", "exit", "postSave"] {
        host.reset_per_script(row, Some(still_denied));
        assert_eq!(
            host.instance_add(row),
            Err(()),
            "{still_denied} must stay refused at host layer even with D1.B gate ON",
        );
        assert_eq!(
            host.instance_set(row, 3),
            Err(()),
            "{still_denied} must stay refused at host layer (instance_set) even with D1.B gate ON",
        );
    }
}

// ---------------------------------------------------------------------------
// Test 6 — reset_per_document clears the gate
// ---------------------------------------------------------------------------

/// Cross-document isolation: the D1.B gate MUST clear on
/// `reset_per_document` so a previous-document decision cannot leak into
/// the next document's host bindings. The dispatch path re-installs the
/// gate before any script runs for the next document.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §5 + §6.1.
#[test]
fn d1b_gate_resets_across_documents() {
    let mut host = HostBindings::new();
    host.set_presave_gate(true);
    assert!(host.presave_gate());
    host.reset_per_document();
    assert!(
        !host.presave_gate(),
        "reset_per_document must clear the D1.B gate so prior decisions \
         cannot leak across the cross-document isolation boundary",
    );
}

// ---------------------------------------------------------------------------
// Test 7 — Activity-helper-with-gate matches policy table exactly
// ---------------------------------------------------------------------------

/// `activity_allowed_for_sandbox_with_gate` is the canonical helper the
/// dispatch path consults. This test pins the FINAL v5 policy table cell
/// values (preSave under both gate states; preSubmit / click / others
/// unchanged).
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §1.3.
#[test]
fn d1b_activity_helper_matches_v5_policy_table() {
    // Gate OFF column == v4 table.
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("preSave"),
        false
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("preSubmit"),
        false
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("click"),
        false
    ));
    assert!(activity_allowed_for_sandbox_with_gate(
        Some("initialize"),
        false
    ));
    assert!(activity_allowed_for_sandbox_with_gate(
        Some("calculate"),
        false
    ));

    // Gate ON column: preSave flips to allow; everything else unchanged.
    assert!(activity_allowed_for_sandbox_with_gate(
        Some("preSave"),
        true
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("preSubmit"),
        true
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(Some("click"), true));
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("postSave"),
        true
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(
        Some("mouseEnter"),
        true
    ));
    assert!(!activity_allowed_for_sandbox_with_gate(None, true));

    // Casing variants of "preSave" MUST NOT bypass the canonical match.
    for variant in ["PRESAVE", "PreSave ", " preSave", "presave"] {
        assert!(
            !activity_allowed_for_sandbox_with_gate(Some(variant), true),
            "casing/whitespace variant {variant:?} must not be treated as preSave",
        );
    }
}

// ---------------------------------------------------------------------------
// Test 8 — Mixed dispatch under gate ON partitions correctly
// ---------------------------------------------------------------------------

/// With the gate ON, a single field carrying preSave + preSubmit + click +
/// initialize partitions cleanly: initialize and preSave both execute,
/// preSubmit / click stay denied. The two executed scripts each contribute
/// to `js_mutations`.
///
/// Cross-ref: `INST_MGR_ACTIVITY_POLICY.md` v5 §6.1.
#[test]
fn d1b_flag_on_mixed_dispatch_partitions_correctly() {
    let _guard = PresaveEnvGuard::set("1");

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

    // initialize + preSave: should both execute.
    add_js_script(
        &mut tree,
        field,
        "initialize",
        "Field1.rawValue = 'init-ok';",
    );
    add_js_script(
        &mut tree,
        field,
        "preSave",
        "Field1.rawValue = 'presave-ok';",
    );
    // preSubmit + click: must remain denied.
    add_js_script(
        &mut tree,
        field,
        "preSubmit",
        "throw new Error('preSubmit must stay denied');",
    );
    add_js_script(
        &mut tree,
        field,
        "click",
        "throw new Error('click must stay denied');",
    );

    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_executed, 2,
        "initialize + preSave must execute exactly twice (saw {} executed)",
        outcome.js_executed
    );
    assert!(
        outcome.js_skipped >= 2,
        "preSubmit + click must be recorded as js_skipped (saw {})",
        outcome.js_skipped
    );
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "denied scripts must never reach QuickJS — no throws can count"
    );
}
