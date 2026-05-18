#![cfg(feature = "xfa-js-sandboxed")]

//! JS2-01 (Sprint 2 Batch B) — host stub gap closure tests.
//!
//! Companion to the inventory in
//! `benchmarks/runs/xfa_enterprise_plan/sprint2_batchB/JS2_01_HOST_STUB_GAP_REPORT.md`.
//!
//! These tests pin the five new low-risk host stubs added by JS2-01:
//!
//! 1. `xfa.form` namespace — Proxy that forwards `resolveNode`/`resolveNodes`
//!    to the existing `xfa.resolveNode`/`xfa.resolveNodes` pair and absorbs
//!    unknown writes. Cluster C contract (`resolveNode("DoesNotExist") ===
//!    null`) preserved.
//! 2. `form` top-level global — identity-equal alias of `xfa.form`.
//!    Adobe Reader exposes both spellings; templates such as
//!    `60df78fe_pdf_0012` reference bare `form.X`.
//! 3. `xfa.host.closeDoc(...)` — interactive thunk; safe default `undefined`,
//!    bumps `js_unsupported_host_calls`.
//! 4. `app.messageBox(...)` — Acrobat alias of `app.alert(...)`; safe default
//!    `0` (= OK pressed), bumps `js_unsupported_host_calls`.
//! 5. `app.closeDoc(...)` — Acrobat alias of `xfa.host.closeDoc(...)`; safe
//!    default `undefined`, bumps `js_unsupported_host_calls`.
//!
//! Plus the ten new event scalar properties promised by Phase E security
//! audit §3.9 but not previously installed (`name`, `type`, `shift`,
//! `modifier`, `commitKey`, `willCommit`, `rc`, `keyDown`, `value`,
//! `reenter`).
//!
//! Sandbox security: see `benchmarks/JS_SANDBOX_SECURITY_AUDIT.md` §3.
//! No new Rust closure was registered; every stub is pure JavaScript
//! that delegates to the existing `unsupportedHostCall` counter for
//! accounting only.

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
// Stub 1: xfa.form namespace
// ---------------------------------------------------------------------------

/// `xfa.form` is exposed and is a non-null object (Adobe Reader's FormDOM
/// root reference). Reading must not throw and `typeof xfa.form` must be
/// `"object"`.
#[test]
fn xfa_form_namespace_is_exposed_as_object() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "if (typeof xfa.form !== 'object') throw new Error('xfa.form not object: ' + typeof xfa.form); \
         if (xfa.form === null) throw new Error('xfa.form is null');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "xfa.form must be an exposed object"
    );
    assert_eq!(outcome.js_executed, 1);
}

/// **Cluster C contract preservation** — `xfa.form.resolveNode("DoesNotExist")`
/// must return `null`, not a chainable sentinel. This pins the most important
/// JS2-01 invariant: the new `xfa.form` Proxy MUST NOT paper over missing SOM
/// paths, because the dispatch site and downstream tests rely on the null
/// signal to surface real schema mismatches.
#[test]
fn xfa_form_resolve_node_unknown_returns_null() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"if (xfa.form.resolveNode("DoesNotExist") !== null) throw new Error("xfa.form.resolveNode must return null for missing path");"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "cluster C contract on xfa.form.resolveNode must hold"
    );
}

/// `xfa.form.resolveNodes` for an unknown path returns a frozen empty array
/// (consistent with `xfa.resolveNodes`).
#[test]
fn xfa_form_resolve_nodes_unknown_returns_empty_array() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"var ns = xfa.form.resolveNodes("DoesNotExist[*]");
           if (!Array.isArray(ns)) throw new Error("xfa.form.resolveNodes did not return array");
           if (ns.length !== 0) throw new Error("expected empty NodeList, got length " + ns.length);"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// Unknown property reads return a chainable null-safe sentinel so deeper
/// access (`xfa.form.someVendorExt.value`) does not TypeError.
#[test]
fn xfa_form_unknown_property_returns_sentinel() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "var v = xfa.form.someVendorExt; \
         var w = xfa.form.someVendorExt.deeper.value; \
         /* should not throw */",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// Unknown property writes are absorbed silently — no TypeError, no counter
/// bump (writes to FormDOM viewer flags have no flatten side-effect).
#[test]
fn xfa_form_unknown_property_writes_absorb_silently() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "xfa.form.dirty = true; \
         xfa.form.someViewerFlag = false;",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_unsupported_host_calls, 0,
        "unknown viewer writes must NOT bump unsupported counter"
    );
}

/// `xfa.form` exposes `recalculate`, `execValidate`, `execInitialize`,
/// `execCalculate` as no-op functions. These are commonly called by Adobe
/// initializers — without them, scripts trip `not a function`.
#[test]
fn xfa_form_exec_methods_are_callable_no_ops() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "xfa.form.recalculate(true); \
         var ok = xfa.form.execValidate(); \
         if (ok !== true) throw new Error('execValidate should default true'); \
         xfa.form.execInitialize(); \
         xfa.form.execCalculate();",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_unsupported_host_calls, 0,
        "form.recalculate/execValidate/etc. are not interactive — no counter bump"
    );
}

// ---------------------------------------------------------------------------
// Stub 2: `form` top-level global alias
// ---------------------------------------------------------------------------

/// `form` is exposed as a top-level global, identity-equal to `xfa.form`.
/// Adobe Reader templates use bare `form.resolveNode("…")` interchangeably
/// with `xfa.form.resolveNode("…")`; we expose both.
#[test]
fn form_global_alias_is_identity_equal_to_xfa_form() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "if (typeof form !== 'object') throw new Error('form not exposed as global'); \
         if (form !== xfa.form) throw new Error('form must be identity-equal to xfa.form');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// Cluster C contract on the `form` alias: `form.resolveNode("DoesNotExist")`
/// is `null`, not a sentinel.
#[test]
fn form_alias_resolve_node_unknown_returns_null() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"if (form.resolveNode("DoesNotExist") !== null) throw new Error("form.resolveNode must return null for missing path");"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "cluster C contract on form.resolveNode must hold"
    );
}

// ---------------------------------------------------------------------------
// Stub 3: xfa.host.closeDoc — interactive thunk
// ---------------------------------------------------------------------------

/// `xfa.host.closeDoc()` returns `undefined` and bumps `unsupported_host_calls`.
/// No new capability surface — pure JS no-op.
#[test]
fn xfa_host_close_doc_returns_undefined_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "var r = xfa.host.closeDoc(); \
         if (r !== undefined) throw new Error('closeDoc should return undefined');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_unsupported_host_calls, 1,
        "xfa.host.closeDoc must bump unsupported counter"
    );
}

/// `xfa.host.closeDoc(true)` — accept any args; Adobe's signature is
/// `closeDoc(bSaveChanges)` and we must not throw on the extra parameter.
#[test]
fn xfa_host_close_doc_accepts_args() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "xfa.host.closeDoc(true); \
         xfa.host.closeDoc(false); \
         xfa.host.closeDoc();",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 3);
}

// ---------------------------------------------------------------------------
// Stub 4: app.messageBox — Acrobat alias of app.alert
// ---------------------------------------------------------------------------

/// `app.messageBox("hi")` returns `0` (OK pressed) and bumps the
/// unsupported counter. Mirrors `app.alert` Adobe semantics.
#[test]
fn app_message_box_returns_zero_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "var r = app.messageBox('hi'); \
         if (r !== 0) throw new Error('app.messageBox should return 0, got ' + r);",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

/// `app.messageBox` accepts the Acrobat object signature
/// `app.messageBox({cMsg, cTitle, nIcon, nType})` without throwing.
#[test]
fn app_message_box_accepts_object_signature() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "var r = app.messageBox({cMsg: 'hi', cTitle: 'X', nIcon: 1, nType: 0}); \
         if (r !== 0) throw new Error('app.messageBox object form should return 0');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

// ---------------------------------------------------------------------------
// Stub 5: app.closeDoc — Acrobat alias
// ---------------------------------------------------------------------------

/// `app.closeDoc()` mirrors `xfa.host.closeDoc()`.
#[test]
fn app_close_doc_returns_undefined_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "var r = app.closeDoc(); \
         if (r !== undefined) throw new Error('app.closeDoc should return undefined');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

// ---------------------------------------------------------------------------
// Event object — 10 new scalar properties (Phase E audit §3.9 completion)
// ---------------------------------------------------------------------------

/// The event object exposes the full Adobe Acrobat SDK scalar surface.
/// None of these reads must throw, and the types must be stable spec defaults.
#[test]
fn event_object_exposes_full_acrobat_sdk_scalar_surface() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "if (typeof event.name !== 'string') throw new Error('event.name not string'); \
         if (typeof event.type !== 'string') throw new Error('event.type not string'); \
         if (typeof event.shift !== 'boolean') throw new Error('event.shift not bool'); \
         if (typeof event.modifier !== 'boolean') throw new Error('event.modifier not bool'); \
         if (typeof event.commitKey !== 'number') throw new Error('event.commitKey not number'); \
         if (typeof event.willCommit !== 'boolean') throw new Error('event.willCommit not bool'); \
         if (typeof event.rc !== 'boolean') throw new Error('event.rc not bool'); \
         if (typeof event.keyDown !== 'boolean') throw new Error('event.keyDown not bool'); \
         if (typeof event.value !== 'string') throw new Error('event.value not string'); \
         if (typeof event.reenter !== 'boolean') throw new Error('event.reenter not bool');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// `event.rc` defaults to `true` (Adobe accept-by-default behaviour for
/// validate events). Scripts that branch on `event.rc` for late-binding
/// validators expect a deterministic value.
#[test]
fn event_rc_defaults_to_true_for_validate_accept_by_default() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "if (event.rc !== true) throw new Error('event.rc must default to true');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// `event.name` returns an empty string (no firing event known during static
/// flatten). Scripts that compare `event.name === "calculate"` for branching
/// silently take the false branch — the deterministic empty default avoids
/// the "is not defined" / "cannot read property of undefined" stairstep.
#[test]
fn event_name_defaults_to_empty_string() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "if (event.name !== '') throw new Error('event.name must be empty string default');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

// ---------------------------------------------------------------------------
// Cross-stub integration: the canonical Adobe initializer pattern
// ---------------------------------------------------------------------------

/// The combination of stubs lets a realistic Adobe-style initializer run
/// without any js_runtime_errors. Models the kind of preamble seen in
/// `60df78fe_pdf_0012` and the Canadian IMM series: probe `form`, set
/// viewer flags, call `app.alert` / `app.messageBox` / `app.closeDoc`,
/// access `event.name`, all without TypeError.
#[test]
fn realistic_acrobat_initializer_pattern_completes_without_runtime_error() {
    let (mut tree, root, _f) = basic_form(
        "initialize",
        "
        // Pre-validate phase: branch on event metadata
        if (event.name === 'initialize' || event.name === '') {
          xfa.host.calculationsEnabled = false;
          xfa.form.recalculate(true);
        }
        // Status messages — both Acrobat and Reader spellings
        app.messageBox({cMsg: 'starting'});
        // Form alias usage
        var section = form.resolveNode('Section.Missing');
        if (section === null) {
          // Expected — graceful schema fallback
        }
        // Late-binding close hook (no-op during static flatten)
        if (event.willCommit) {
          app.closeDoc(false);
        }
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "realistic Acrobat initializer must complete cleanly"
    );
    assert_eq!(
        outcome.js_unsupported_host_calls, 1,
        "exactly app.messageBox = 1 unsupported call (closeDoc skipped because event.willCommit is false)"
    );
}

// ---------------------------------------------------------------------------
// Sandbox isolation: new stubs never expose Rust types or system globals
// ---------------------------------------------------------------------------

/// The new `form` global and `xfa.form` namespace must not leak Rust types
/// via prototype chain probing. Same invariant as `host_stubs_never_expose_
/// internal_rust_types_via_js` in `m3b_phaseE_host_stubs.rs`, extended to
/// the new surface.
#[test]
fn new_stubs_never_expose_rust_internals_via_prototype_probe() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        // form / xfa.form must be plain JS objects, no Rust constructor leak.
        if (typeof form !== 'object') throw new Error('form not object');
        if (typeof xfa.form !== 'object') throw new Error('xfa.form not object');
        // require / process / Bun must remain undefined.
        if (typeof require !== 'undefined') throw new Error('require leaked');
        if (typeof process !== 'undefined') throw new Error('process leaked');
        if (typeof Bun !== 'undefined') throw new Error('Bun leaked');
        // The new closeDoc and messageBox thunks must be functions, not
        // native bindings with leaked Rust internals.
        if (typeof xfa.host.closeDoc !== 'function') throw new Error('closeDoc not function');
        if (typeof app.closeDoc !== 'function') throw new Error('app.closeDoc not function');
        if (typeof app.messageBox !== 'function') throw new Error('app.messageBox not function');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

// ---------------------------------------------------------------------------
// Counter discipline: per-document isolation preserved with new stubs
// ---------------------------------------------------------------------------

/// The new interactive thunks (`closeDoc`, `app.messageBox`, `app.closeDoc`)
/// bump `unsupported_host_calls` independently and the counter resets
/// between documents on the same runtime — same isolation contract as
/// `unsupported_counter_is_per_document_and_does_not_leak`.
#[test]
fn new_interactive_thunks_count_independently() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "xfa.host.closeDoc(); \
         app.messageBox('a'); \
         app.closeDoc(true);",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_unsupported_host_calls, 3,
        "each thunk must bump the counter independently"
    );
}
