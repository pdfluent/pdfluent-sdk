#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! D7 — opt-in presence/visibility retry tests.
//!
//! Retry is gated behind BOTH `XFA_OCCUR_APPLY=1` and `XFA_PRESENCE_RETRY=1`
//! (default OFF). It admits (`Hidden`/`Invisible` -> `Visible`) only nodes that
//! are an occur-applied target or a descendant of one; `Inactive` is skipped
//! (fail-closed). Env mutation is serialized.

use std::sync::{Mutex, MutexGuard, OnceLock};

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::QuickJsRuntime;
use pdf_xfa::{DynamicScriptOutcome, JsExecutionMode};
use xfa_layout_engine::form::{
    EventScript, FormNode, FormNodeId, FormNodeType, FormTree, Occur, Presence, ScriptLanguage,
};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Sets `XFA_OCCUR_APPLY` and `XFA_PRESENCE_RETRY` for the guard's lifetime
/// (each `Some("1")` to enable, `None` to ensure unset), restoring both on drop.
struct FlagsGuard {
    _lock: MutexGuard<'static, ()>,
    prev_occur: Option<String>,
    prev_presence: Option<String>,
}

impl FlagsGuard {
    fn new(occur: Option<&str>, presence: Option<&str>) -> Self {
        let lock = env_lock();
        let prev_occur = std::env::var("XFA_OCCUR_APPLY").ok();
        let prev_presence = std::env::var("XFA_PRESENCE_RETRY").ok();
        match occur {
            Some(v) => std::env::set_var("XFA_OCCUR_APPLY", v),
            None => std::env::remove_var("XFA_OCCUR_APPLY"),
        }
        match presence {
            Some(v) => std::env::set_var("XFA_PRESENCE_RETRY", v),
            None => std::env::remove_var("XFA_PRESENCE_RETRY"),
        }
        Self {
            _lock: lock,
            prev_occur,
            prev_presence,
        }
    }
}

impl Drop for FlagsGuard {
    fn drop(&mut self) {
        match self.prev_occur.take() {
            Some(v) => std::env::set_var("XFA_OCCUR_APPLY", v),
            None => std::env::remove_var("XFA_OCCUR_APPLY"),
        }
        match self.prev_presence.take() {
            Some(v) => std::env::set_var("XFA_PRESENCE_RETRY", v),
            None => std::env::remove_var("XFA_PRESENCE_RETRY"),
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

/// Build a root + one hidden repeating subform that writes `this.occur.min`.
fn hidden_occur_subform(presence: Presence) -> (FormTree, FormNodeId, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let sub = add_child(&mut tree, root, "Row", FormNodeType::Subform);
    tree.meta_mut(sub).presence = presence;
    add_js_script(&mut tree, sub, "calculate", "this.occur.min = 2;");
    (tree, root, sub)
}

/// Both flags ON: a hidden occur-applied target is admitted to Visible.
#[test]
fn presence_retry_admits_hidden_occur_target_with_both_flags() {
    let _g = FlagsGuard::new(Some("1"), Some("1"));
    let (mut tree, root, sub) = hidden_occur_subform(Presence::Hidden);

    let outcome = run_sandbox(&mut tree, root);

    assert!(outcome.presence_retry_enabled);
    assert_eq!(outcome.occur_mutations_applied, 1);
    assert!(outcome.presence_retry_admitted >= 1);
    assert_eq!(
        tree.meta(sub).presence,
        Presence::Visible,
        "hidden occur-applied target must be admitted to Visible"
    );
}

/// `XFA_PRESENCE_RETRY` unset (occur apply on): no retry, node stays hidden.
#[test]
fn presence_retry_off_without_flag() {
    let _g = FlagsGuard::new(Some("1"), None);
    let (mut tree, root, sub) = hidden_occur_subform(Presence::Hidden);

    let outcome = run_sandbox(&mut tree, root);

    assert!(!outcome.presence_retry_enabled);
    assert_eq!(outcome.presence_retry_admitted, 0);
    assert_eq!(
        tree.meta(sub).presence,
        Presence::Hidden,
        "without XFA_PRESENCE_RETRY the node stays hidden"
    );
}

/// `XFA_PRESENCE_RETRY=1` but `XFA_OCCUR_APPLY` unset: retry requires occur
/// application, so no targets and no admission.
#[test]
fn presence_retry_requires_occur_apply() {
    let _g = FlagsGuard::new(None, Some("1"));
    let (mut tree, root, sub) = hidden_occur_subform(Presence::Hidden);

    let outcome = run_sandbox(&mut tree, root);

    assert!(!outcome.presence_retry_enabled);
    assert_eq!(outcome.presence_retry_admitted, 0);
    assert_eq!(tree.meta(sub).presence, Presence::Hidden);
}

/// Both flags ON but the target is `Inactive`: fail closed (skipped, stays).
#[test]
fn presence_retry_skips_inactive() {
    let _g = FlagsGuard::new(Some("1"), Some("1"));
    let (mut tree, root, sub) = hidden_occur_subform(Presence::Inactive);

    let outcome = run_sandbox(&mut tree, root);

    assert!(outcome.presence_retry_enabled);
    assert!(outcome.presence_retry_skipped >= 1);
    assert_eq!(outcome.presence_retry_admitted, 0);
    assert_eq!(
        tree.meta(sub).presence,
        Presence::Inactive,
        "Inactive must not be admitted (fail-closed)"
    );
}
