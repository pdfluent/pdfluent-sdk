#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase E (XFA-JS-HOST-STUBS) integration tests.
//!
//! These tests pin the contract for the Adobe-Reader host-object stubs added
//! by the XFA-JS-HOST-STUBS sprint:
//!
//! * `xfa.host.*` exposes read-side defaults, write-absorbing setters, and
//!   safe-default interactive thunks (`messageBox`, `openList`, `beep`,
//!   `print`, ...) that bump `js_unsupported_host_calls` instead of
//!   inflating `js_runtime_errors`.
//! * `app.calculate.override = ...` is silently absorbed (viewer-only).
//! * Interactive `app.alert(...)`, `app.launchURL(...)`, `xfa.signature.sign()`,
//!   `xfa.connection.execute()` return safe defaults and bump the
//!   unsupported counter.
//! * Read-side viewer-only namespaces (`xfa.viewer`, `xfa.appState`,
//!   `xfa.appearanceFilter`, `xfa.aliasNode`) absorb writes and never throw.
//! * Sandbox isolation: a fresh runtime starts with zero counters and
//!   host-stub state never leaks between documents.
//! * Sandbox isolation: stubs never expose Rust types via JS (`typeof
//!   xfa.host` is `"object"`, `typeof app.alert` is `"function"`).
//!
//! Sandbox security claims are documented in
//! `benchmarks/JS_SANDBOX_SECURITY_AUDIT.md`.

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

fn run_with(
    runtime: &mut QuickJsRuntime,
    tree: &mut FormTree,
    root: FormNodeId,
) -> pdf_xfa::DynamicScriptOutcome {
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, runtime)
        .expect("sandbox dispatch")
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    run_with(&mut runtime, tree, root)
}

// ---------------------------------------------------------------------------
// xfa.host read-side properties
// ---------------------------------------------------------------------------

#[test]
fn xfa_host_numpages_read_returns_static_page_count() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        // numPages should be a number (=0 for synthetic forms without a layout),
        // not undefined, and reading it must not throw.
        "if (typeof xfa.host.numPages !== 'number') throw new Error('numPages not numeric');",
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "no TypeError on host.numPages"
    );
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn xfa_host_viewer_scalar_defaults_are_strings() {
    // Tests the read-side static defaults: name, platform, language, version,
    // appType, variation. Scripts use these to dispatch on viewer identity.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
if (typeof xfa.host.name !== "string") throw new Error("host.name not string");
if (typeof xfa.host.language !== "string") throw new Error("host.language not string");
if (typeof xfa.host.platform !== "string") throw new Error("host.platform not string");
if (typeof xfa.host.version !== "string") throw new Error("host.version not string");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

#[test]
fn xfa_host_unknown_property_returns_sentinel_not_typeerror() {
    // Vendor-specific extensions: `xfa.host.someThing.foo` must not throw.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var s = xfa.host.someVendorExt;
if (s === undefined) throw new Error("expected sentinel, got undefined");
// Deep chain must not throw — the sentinel is null-safe and chainable.
var deep = xfa.host.a.b.c.d.value;
if (deep !== null) throw new Error("expected sentinel chain value=null, got " + deep);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
}

// ---------------------------------------------------------------------------
// xfa.host interactive functions
// ---------------------------------------------------------------------------

#[test]
fn xfa_host_message_box_returns_zero_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var r = xfa.host.messageBox("hi", "title", 0, 1);
// 0 == "user pressed OK" in Adobe Reader. Documented safe default.
if (r !== 0) throw new Error("expected 0, got " + r);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(
        outcome.js_unsupported_host_calls, 1,
        "messageBox must bump unsupported counter"
    );
}

#[test]
fn xfa_host_open_list_returns_minus_one_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        // openList expects a list of strings + returns the index of the chosen
        // option (-1 = cancelled). We always return -1 (no user choice).
        r#"
var idx = xfa.host.openList(["a","b","c"]);
if (idx !== -1) throw new Error("expected -1 cancel, got " + idx);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

#[test]
fn xfa_host_interactive_calls_count_independently() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
xfa.host.messageBox("a");
xfa.host.beep();
xfa.host.print();
xfa.host.gotoURL("https://example.com");
xfa.host.exportData("foo.xml", false);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 5);
}

#[test]
fn xfa_host_property_writes_absorb_silently() {
    // Common pattern: scripts set viewer-only flags. The Proxy `set` trap
    // returns true; no TypeError, no metadata change other than executed.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
xfa.host.title = "Form A";
xfa.host.runtimeHighlight = true;
xfa.host.validationsEnabled = false;
xfa.host.calculationsEnabled = false;
xfa.host.someVendorFlag = 42;
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 0);
}

// ---------------------------------------------------------------------------
// app.* (Reader global)
// ---------------------------------------------------------------------------

#[test]
fn app_calculate_override_write_absorbs_silently() {
    // The signal-case from priority matrix XFA-JS-HOST-STUBS. Decision: silent
    // no-op (NOT UnsupportedHostCapability) — see
    // benchmarks/JS_SANDBOX_SECURITY_AUDIT.md §4 for the rationale: this is
    // a viewer cascade hint, not a UI prompt to the user.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
app.calculate.override = true;
app.calculate.suspend = false;
// Read returns the seeded default — confirms the namespace exists.
if (typeof app.calculate.override !== "boolean") throw new Error("override read");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(
        outcome.js_unsupported_host_calls, 0,
        "writing app.calculate.override is silent no-op, not interactive"
    );
}

#[test]
fn app_alert_returns_zero_and_counts_unsupported() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var r = app.alert("hi");
if (r !== 0) throw new Error("expected dialog-OK = 0, got " + r);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

#[test]
fn app_unknown_property_returns_sentinel() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        // `app.fs` is read-side null (file-system gateway intentionally
        // exposed as null), `app.vendorXyz` is a sentinel.
        r#"
if (app.fs !== null) throw new Error("app.fs must be null");
var v = app.vendorXyz;
if (v === undefined) throw new Error("expected sentinel, got undefined");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
}

// ---------------------------------------------------------------------------
// xfa.signature / xfa.connection — interactive sub-namespaces
// ---------------------------------------------------------------------------

#[test]
fn xfa_signature_sign_counts_unsupported_and_returns_false() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var ok = xfa.signature.sign("certName");
if (ok !== false) throw new Error("expected false, got " + ok);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

#[test]
fn xfa_connection_execute_counts_unsupported_and_returns_null() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
var r = xfa.connection.execute();
if (r !== null) throw new Error("expected null, got " + r);
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
}

// ---------------------------------------------------------------------------
// xfa.viewer / xfa.appState / xfa.appearanceFilter / xfa.aliasNode
// ---------------------------------------------------------------------------

#[test]
fn xfa_viewer_namespace_absorbs_writes_and_reads_defaults() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
if (xfa.viewer.zoomType !== "FitWidth") throw new Error("zoomType default");
xfa.viewer.zoomType = "Custom";
xfa.viewer.zoom = 200;
xfa.viewer.toolbar = false;
// Unknown property — sentinel.
var s = xfa.viewer.unknownExt;
if (s === undefined) throw new Error("expected sentinel for unknown viewer prop");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 0);
}

#[test]
fn xfa_appstate_appearancefilter_aliasnode_are_proxy_namespaces() {
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
xfa.appState.highlightRequiredFields = true;
xfa.appearanceFilter.darkMode = false;
xfa.aliasNode.something = "x";
// None of these throw; reads of unknown props return chainable sentinels.
var v = xfa.aliasNode.someChain.deeper.value;
if (v !== null) throw new Error("expected sentinel value=null");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_unsupported_host_calls, 0);
}

// ---------------------------------------------------------------------------
// Sandbox isolation
// ---------------------------------------------------------------------------

#[test]
fn unsupported_counter_is_per_document_and_does_not_leak() {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

    let (mut tree_a, root_a, _f) =
        basic_form("calculate", "app.alert('one'); xfa.host.messageBox('two');");
    let outcome_a = run_with(&mut runtime, &mut tree_a, root_a);
    assert_eq!(outcome_a.js_unsupported_host_calls, 2);
    assert_eq!(outcome_a.js_runtime_errors, 0);

    // Fresh document on the SAME runtime — counter must reset.
    let (mut tree_b, root_b, _f) = basic_form("calculate", "this.rawValue = 'unchanged';");
    let outcome_b = run_with(&mut runtime, &mut tree_b, root_b);
    assert_eq!(
        outcome_b.js_unsupported_host_calls, 0,
        "Doc A's interactive counter must not leak into Doc B"
    );
}

#[test]
fn host_stubs_never_expose_internal_rust_types_via_js() {
    // `Object.prototype.toString.call(...)` is the canonical way to detect
    // host-supplied types. Anything other than "[object Object]" or
    // "[object Function]" would suggest a Rust-typed wrapper leaked through.
    let (mut tree, root, _f) = basic_form(
        "calculate",
        r#"
function ttag(v) { return Object.prototype.toString.call(v); }

if (ttag(xfa.host)    !== "[object Object]")   throw new Error("xfa.host kind: " + ttag(xfa.host));
if (ttag(xfa.viewer)  !== "[object Object]")   throw new Error("xfa.viewer kind: " + ttag(xfa.viewer));
if (ttag(app)         !== "[object Object]")   throw new Error("app kind: " + ttag(app));
if (ttag(xfa.host.messageBox) !== "[object Function]") throw new Error("messageBox kind: " + ttag(xfa.host.messageBox));
if (ttag(app.alert)   !== "[object Function]") throw new Error("app.alert kind: " + ttag(app.alert));

// No `process`, `fs`, `child_process`, `require`, `globalThis.Bun` — sandbox
// only exposes a closed set of host objects.
if (typeof process !== "undefined")    throw new Error("process leak");
if (typeof require !== "undefined")    throw new Error("require leak");
if (typeof Bun !== "undefined")        throw new Error("Bun leak");
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(outcome.js_runtime_errors, 0, "no host-type leakage");
    assert_eq!(outcome.js_executed, 1);
}

// ---------------------------------------------------------------------------
// 13275420-style mixed pattern: viewer flag flip + messageBox + value write
// ---------------------------------------------------------------------------

#[test]
fn realistic_initializer_pattern_completes_without_runtime_error() {
    // Mirrors the canonical Adobe "initialize" script pattern that drove
    // 13275420's 31 js_runtime_errors: flip a few viewer flags, set a few
    // calculation controls, optionally show a status messageBox, then write
    // a field value. None of these should produce a runtime error.
    let (mut tree, root, field) = basic_form(
        "calculate",
        r#"
app.calculate.override = true;
app.runtimeHighlight   = false;
xfa.host.validationsEnabled  = false;
xfa.host.calculationsEnabled = true;
xfa.host.title = "Form A";
// "Pop a status dialog" — interactive, counted but does not throw.
xfa.host.messageBox("ready");
// Then mutate a field.
this.rawValue = "ready";
"#,
    );
    let outcome = run_sandbox(&mut tree, root);
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "the initializer pattern that previously inflated 13275420 must complete cleanly"
    );
    assert_eq!(outcome.js_mutations, 1);
    assert_eq!(outcome.js_unsupported_host_calls, 1);
    let _ = field; // silence unused
}
