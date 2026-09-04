#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! D6 — opt-in `occur.min` layout application tests.
//!
//! Application is gated behind `XFA_OCCUR_APPLY=1` (default OFF, even in
//! sandboxed mode). Captured `occur.min` writes are applied to the form's
//! `Occur` only for live repeatable containers (Subform/Area/ExclGroup) when the
//! gate is on and the script pass did not roll back; everything else fails
//! closed. Env mutation is serialized (cargo runs tests multi-threaded).

use std::sync::{Mutex, MutexGuard, OnceLock};

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::QuickJsRuntime;
use pdf_xfa::{DynamicScriptOutcome, JsExecutionMode};
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

const ENV_OCCUR_APPLY: &str = "XFA_OCCUR_APPLY";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

struct OccurApplyEnvGuard {
    _lock: MutexGuard<'static, ()>,
    previous: Option<String>,
}

impl OccurApplyEnvGuard {
    fn set(value: &str) -> Self {
        let lock = env_lock();
        let previous = std::env::var(ENV_OCCUR_APPLY).ok();
        std::env::set_var(ENV_OCCUR_APPLY, value);
        Self {
            _lock: lock,
            previous,
        }
    }
    fn unset() -> Self {
        let lock = env_lock();
        let previous = std::env::var(ENV_OCCUR_APPLY).ok();
        std::env::remove_var(ENV_OCCUR_APPLY);
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for OccurApplyEnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(prev) => std::env::set_var(ENV_OCCUR_APPLY, prev),
            None => std::env::remove_var(ENV_OCCUR_APPLY),
        }
    }
}

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

fn add_child(tree: &mut FormTree, parent: FormNodeId, name: &str, t: FormNodeType) -> FormNodeId {
    let c = add_node(tree, name, t);
    tree.get_mut(parent).children.push(c);
    c
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

fn occur_min(tree: &FormTree, id: FormNodeId) -> u32 {
    tree.get(id).occur.min
}

/// Gate ON: a captured `occur.min` write on a repeatable subform is applied to
/// the form's Occur.
#[test]
fn occur_min_applied_when_gate_on() {
    let _g = OccurApplyEnvGuard::set("1");
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let sub = add_child(&mut tree, root, "Row", FormNodeType::Subform);
    add_js_script(&mut tree, sub, "calculate", "this.occur.min = 3;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.occur_mutations_captured, 1);
    assert_eq!(outcome.occur_mutations_applied, 1);
    assert_eq!(outcome.occur_application_targets, 1);
    assert_eq!(
        occur_min(&tree, sub),
        3,
        "occur.min must be applied to the form"
    );
    assert!(
        tree.get(sub).occur.initial >= 3,
        "initial must rise to >= min"
    );
}

/// Gate OFF (default): the same write is captured but NOT applied.
#[test]
fn occur_min_not_applied_when_gate_off() {
    let _g = OccurApplyEnvGuard::unset();
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let sub = add_child(&mut tree, root, "Row", FormNodeType::Subform);
    add_js_script(&mut tree, sub, "calculate", "this.occur.min = 3;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.occur_mutations_captured, 1);
    assert_eq!(
        outcome.occur_mutations_applied, 0,
        "gate off: capture-only, nothing applied"
    );
    assert!(outcome.occur_mutations_skipped >= 1);
    assert_eq!(
        occur_min(&tree, sub),
        1,
        "occur.min must be unchanged when the apply gate is off"
    );
}

/// Gate ON but the target is a Field (not repeatable): fail closed.
#[test]
fn occur_apply_fails_closed_on_non_repeatable() {
    let _g = OccurApplyEnvGuard::set("1");
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let fld = add_child(
        &mut tree,
        root,
        "Fld",
        FormNodeType::Field {
            value: String::new(),
        },
    );
    add_js_script(&mut tree, fld, "calculate", "this.occur.min = 3;");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(outcome.occur_mutations_captured, 1);
    assert_eq!(
        outcome.occur_mutations_applied, 0,
        "a non-repeatable Field target must not have occur applied"
    );
    assert!(outcome.occur_application_ambiguous >= 1);
    assert_eq!(occur_min(&tree, fld), 1, "field occur.min unchanged");
}
