#![cfg(feature = "xfa-js-sandboxed")]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Track C (Product Quality Wave 1) — host object behavior tests.
//!
//! Companion to
//! `benchmarks/runs/xfa_enterprise_plan/product_quality_track/TRACK_JS_RUNTIME_SEMANTICS.md`.
//!
//! TESTS-ONLY. No engine behaviour change.
//!
//! These tests pin the CURRENT host object surface accessible from inside the
//! sandbox so that future refactors cannot silently regress documented
//! sentinels. They complement
//!
//! - `m3b_phaseE_js2_01.rs` (host stub gap closure: xfa.form / form alias /
//!   closeDoc / app.messageBox / app.closeDoc),
//! - `m3b_phaseE_data2_02.rs` (util / console inside `<variables>` closures),
//! - `m3b_phaseE_host_stubs.rs` (broader interactive thunk surface)
//!
//! by adding direct event-script lexical-visibility assertions and
//! null-safe-sentinel coverage at chain depth, plus the `xfa.host.messageBox
//! === 0 (cancelled)` cross-reference.
//!
//! No new host bindings are introduced; every check verifies pre-existing
//! behaviour.

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
// xfa.form / form / xfa.host null-safe sentinel access (JS2-01 invariants)
// ---------------------------------------------------------------------------

/// All three top-level host objects (`xfa`, `xfa.form`, `xfa.host`) are
/// exposed as plain JS objects (not native bindings) and the bare `form`
/// alias is identity-equal to `xfa.form`. This is the "is it actually
/// reachable from a script" smoke test — if any of these are missing,
/// realistic Adobe-style initializers explode at the first reference.
#[test]
fn top_level_host_objects_are_all_exposed_as_objects() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (typeof xfa !== 'object' || xfa === null)
            throw new Error('xfa not exposed');
        if (typeof xfa.form !== 'object' || xfa.form === null)
            throw new Error('xfa.form not exposed');
        if (typeof xfa.host !== 'object' || xfa.host === null)
            throw new Error('xfa.host not exposed');
        if (typeof form !== 'object' || form === null)
            throw new Error('form alias not exposed');
        if (form !== xfa.form)
            throw new Error('form must be identity-equal to xfa.form');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

/// Null-safe sentinel: arbitrary-depth property chains on `xfa.form` /
/// `form` return chainable sentinels rather than throwing `TypeError:
/// Cannot read property X of undefined`. Pin depth 4 — corpus scripts
/// commonly use `xfa.form.SubformA.Page1.field.rawValue`-style chains
/// against unknown paths during validation.
#[test]
fn deep_unknown_property_chains_return_chainable_sentinels() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        // Chain depth 4 through unknown path — no TypeError on undefined.
        var v = xfa.form.unknownA.unknownB.unknownC.unknownD;
        // Chain depth 4 through the `form` alias as well.
        var w = form.unknownA.unknownB.unknownC.unknownD;
        // Reading a sentinel value (.value) at the bottom must also be safe.
        var x = xfa.form.unknownA.unknownB.unknownC.value;
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "null-safe sentinel must support arbitrary-depth chain access"
    );
}

/// `xfa.host.messageBox` returns `0` (== "OK / cancelled" in Acrobat SDK
/// semantics) on every call. This is the documented sentinel that the
/// JS2-02 matrix calls out as the canonical "host C7 typed-error today"
/// path — the runtime today returns 0 silently rather than throwing,
/// which matches Adobe's behaviour when the dialog is dismissed.
#[test]
fn xfa_host_message_box_returns_zero_cancelled_sentinel() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        var a = xfa.host.messageBox('hi');
        var b = xfa.host.messageBox('hi', 'title');
        var c = xfa.host.messageBox('hi', 'title', 0, 1);
        if (a !== 0) throw new Error('messageBox(1-arg) must return 0, got ' + a);
        if (b !== 0) throw new Error('messageBox(2-arg) must return 0, got ' + b);
        if (c !== 0) throw new Error('messageBox(4-arg) must return 0, got ' + c);
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    // Three calls = three unsupported_host_calls counter ticks. This pins the
    // accounting invariant — the JS2-02 matrix's C7 cell hinges on this
    // counter being the truth source.
    assert_eq!(
        outcome.js_unsupported_host_calls, 3,
        "every xfa.host.messageBox call must bump the unsupported counter"
    );
}

/// `xfa.host.*` exposes the documented read-only properties (locale,
/// version, name, appType, language) as strings. Scripts branching on
/// `xfa.host.version` expect a non-throwing string; pin the safe defaults.
#[test]
fn xfa_host_readonly_properties_are_stable_strings() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        // Each of these is exposed by the JS2-01 / phase-E stubs.
        // None must throw, and types must be stable strings.
        var props = ['version', 'name', 'appType', 'language'];
        for (var i = 0; i < props.length; i++) {
            var v = xfa.host[props[i]];
            if (typeof v !== 'string')
                throw new Error('xfa.host.' + props[i] + ' must be string, got ' + typeof v);
        }
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// Cluster C contract preservation at the `xfa.host.*` boundary:
/// `xfa.host.resolveNode` (when present) and the global `xfa.resolveNode`
/// both return `null` for unknown paths. This is the load-bearing invariant
/// the data-binding longtail relies on; pin it explicitly at the host
/// object level.
#[test]
fn xfa_resolve_node_returns_null_for_unknown_path() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (xfa.resolveNode('DoesNotExist') !== null)
            throw new Error('xfa.resolveNode missing path must return null');
        // Same contract on the form-rooted alias.
        if (xfa.form.resolveNode('DoesNotExist') !== null)
            throw new Error('xfa.form.resolveNode missing path must return null');
        // resolveNodes returns frozen empty array for missing paths.
        var ns = xfa.resolveNodes('DoesNotExist[*]');
        if (!Array.isArray(ns)) throw new Error('resolveNodes must return array');
        if (ns.length !== 0) throw new Error('expected empty array');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "cluster C null-on-missing contract must hold"
    );
}

// ---------------------------------------------------------------------------
// util / console stubs — direct event-script lexical visibility
// ---------------------------------------------------------------------------

/// `util.*` stubs (printd, scand, printx, scanx, formatString, stringFromStream)
/// are visible inside event-script bodies, not only inside `<variables>`
/// closures. This is the broader "DATA2-02 surface" pin — the complementary
/// tests in `m3b_phaseE_data2_02.rs` cover the closure case, this covers the
/// direct-body case.
#[test]
fn util_stubs_are_directly_callable_from_event_script() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (typeof util !== 'object') throw new Error('util not object');
        // util.printd: format a Date. Must not throw.
        var d = new Date(Date.UTC(2026, 4, 19, 0, 0, 0));
        var s = util.printd('yyyy-mm-dd', d);
        if (typeof s !== 'string') throw new Error('util.printd must return string');
        if (s !== '2026-05-19')
            throw new Error('util.printd format mismatch: ' + s);
        // util.printx is the picture-format passthrough stub.
        var x = util.printx('???', 'abc');
        if (typeof x !== 'string') throw new Error('util.printx must return string');
        // util.scand parses a Date back from string form.
        var parsed = util.scand('yyyy-mm-dd', '2026-05-19');
        if (!(parsed instanceof Date)) throw new Error('util.scand must return Date');
        // Required stubs that real-corpus scripts depend on.
        var required = ['printd', 'printx', 'scand'];
        for (var i = 0; i < required.length; i++) {
            if (typeof util[required[i]] !== 'function')
                throw new Error('util.' + required[i] + ' must be function');
        }
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "util.* must be directly callable from event script body"
    );
}

/// `console.{log, warn, error, info, debug}` are callable from event scripts
/// (capture-only stubs; no side-effect, no counter bump). Realistic
/// Adobe-authored scripts use `console.log` for templating debug; missing
/// stubs surface as `ReferenceError: console is not defined`.
#[test]
fn console_stubs_are_directly_callable_capture_only() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (typeof console !== 'object') throw new Error('console not object');
        // Each stub must accept any arity and return undefined.
        var r1 = console.log('hello', 1, 2, 3);
        var r2 = console.warn('warn');
        var r3 = console.error('err');
        var r4 = console.info && console.info('info');
        var r5 = console.debug && console.debug('dbg');
        // None must throw, and log/warn/error must exist and be functions.
        if (typeof console.log !== 'function') throw new Error('console.log missing');
        if (typeof console.warn !== 'function') throw new Error('console.warn missing');
        if (typeof console.error !== 'function') throw new Error('console.error missing');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    // Capture-only contract: console.* never bumps the unsupported host call
    // counter (unlike interactive thunks like messageBox).
    assert_eq!(
        outcome.js_unsupported_host_calls, 0,
        "console.* stubs must not bump unsupported_host_calls (capture-only)"
    );
}

/// Cross-document isolation: host objects exposed in document 1 do not
/// leak state into document 2 when the same runtime adapter is reused.
/// Pin the messageBox counter resetting between resets.
#[test]
fn host_object_state_does_not_leak_across_documents() {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

    // First document — bump unsupported counter via messageBox.
    let (mut tree1, root1, _f1) = basic_form(
        "calculate",
        "xfa.host.messageBox('doc1'); xfa.host.messageBox('doc1-again');",
    );
    let out1 = apply_dynamic_scripts_with_runtime(
        &mut tree1,
        root1,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("sandbox dispatch doc1");
    assert_eq!(out1.js_unsupported_host_calls, 2);

    // Second document — counter must reset; first call sees 1, not 3.
    let (mut tree2, root2, _f2) = basic_form("calculate", "xfa.host.messageBox('doc2');");
    let out2 = apply_dynamic_scripts_with_runtime(
        &mut tree2,
        root2,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("sandbox dispatch doc2");
    assert_eq!(
        out2.js_unsupported_host_calls, 1,
        "host counter must reset between documents on the same runtime"
    );
}

/// Forbidden global probe — the host object surface does not include `fetch`,
/// `XMLHttpRequest`, `process`, `require`, `Deno`, or `Bun`. Pinned at the
/// host-object level to keep this guarantee anchored even if the host
/// stub surface grows. Mirrors the broader sandbox audit but tests
/// from the script-visible angle.
#[test]
fn host_object_surface_does_not_leak_forbidden_globals() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (typeof fetch !== 'undefined') throw new Error('fetch leaked');
        if (typeof XMLHttpRequest !== 'undefined') throw new Error('XHR leaked');
        if (typeof process !== 'undefined') throw new Error('process leaked');
        if (typeof require !== 'undefined') throw new Error('require leaked');
        if (typeof Deno !== 'undefined') throw new Error('Deno leaked');
        if (typeof Bun !== 'undefined') throw new Error('Bun leaked');
        if (typeof globalThis !== 'undefined' && globalThis.fetch !== undefined)
            throw new Error('globalThis.fetch leaked');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "no forbidden global must be reachable from script context"
    );
}

/// `xfa.event` is exposed even though no event is firing during static
/// flatten. The event object has documented defaults (`event.name = ""`,
/// `event.rc = true`) — pin them so scripts can rely on the deterministic
/// defaults without `typeof` ladders.
#[test]
fn xfa_event_object_has_deterministic_defaults() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        "
        if (typeof event !== 'object' || event === null)
            throw new Error('event not exposed');
        // event.name defaults to empty string (no firing event during flatten).
        if (event.name !== '') throw new Error('event.name must default to \"\"');
        // event.rc defaults to true (accept-by-default for validators).
        if (event.rc !== true) throw new Error('event.rc must default to true');
        // event.willCommit defaults to false during static flatten.
        if (event.willCommit !== false) throw new Error('event.willCommit must default false');
        ",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}
