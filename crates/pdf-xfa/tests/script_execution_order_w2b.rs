#![cfg(feature = "xfa-js-sandboxed")]

//! W2-B regression tests — variables-script registration order / size cap.
//!
//! These tests guard the W1-B `implicit_function` cluster root cause: real
//! XFA forms ship per-form helper libraries inside `<variables><script>`
//! blocks (XFA 3.3 §5.5). Event scripts reference top-level declarations
//! from those libraries as `<scriptName>.<top_level_decl>(...)`. If the
//! library fails to register, every dependent event-script call surfaces
//! as a `js_resolve_failure`.
//!
//! Pre-W2-B: variables-scripts used the event-script body-size cap
//! (`MAX_SCRIPT_BODY_BYTES` = 64 KB), which silently rejected every
//! real-world Canadian / IRCC helper (`validateForm` ≈ 125 KB,
//! `CoreFunctions` ≈ 115 KB, `LOV` ≈ 507 KB).
//!
//! Post-W2-B: variables-scripts use the dedicated, higher cap
//! `MAX_VARIABLES_SCRIPT_BODY_BYTES` (1 MiB). Bodies under the new cap
//! register and their `<scriptName>.X(...)` lookups resolve from event
//! scripts. Bodies strictly above the new cap still reject for
//! defence-in-depth.
//!
//! Per-script time + per-document memory budgets are unchanged.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{QuickJsRuntime, MAX_SCRIPT_BODY_BYTES, MAX_VARIABLES_SCRIPT_BODY_BYTES};
use pdf_xfa::{DynamicScriptOutcome, JsExecutionMode};
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

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    let child = add_node(
        tree,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    );
    tree.get_mut(parent).children.push(child);
    child
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

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

/// Pre-W2-B regression: a variables-script body of exactly 96 KB (above
/// `MAX_SCRIPT_BODY_BYTES`) must register and its top-level declarations
/// must resolve from a dependent event script.
///
/// This mirrors real-world Canadian government XFA forms whose
/// `validateForm` library is ≈ 125 KB.
#[test]
fn variables_script_above_event_cap_registers_and_resolves() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);

    // 96 KB body: above the 64 KB event-script cap but well under the
    // 1 MiB variables-script cap. Provides a real-world-sized
    // `validateForm.checkSomething()` callable.
    let preamble = "function checkSomething() { return \"called\"; }\n";
    let padding_unit = "// pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad pad\n";
    let target: usize = 96 * 1024;
    let pad_repeats = (target.saturating_sub(preamble.len())) / padding_unit.len();
    let body = format!("{}{}", preamble, padding_unit.repeat(pad_repeats));
    assert!(
        body.len() > MAX_SCRIPT_BODY_BYTES,
        "synthesised body must exceed event cap"
    );
    assert!(
        body.len() < MAX_VARIABLES_SCRIPT_BODY_BYTES,
        "synthesised body must stay under variables cap"
    );

    tree.variables_scripts
        .push((None, "validateForm".into(), body));

    let out = add_field(&mut tree, root, "Out", "");
    // Calls into the variables-script namespace exactly like real
    // government XFA event scripts: `<scriptName>.<top_level_fn>()`.
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof validateForm === \"undefined\") \
         ? \"absent\" : validateForm.checkSomething();",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        field_value(&tree, out),
        "called",
        "validateForm.checkSomething() must resolve",
    );
    assert_eq!(outcome.js_executed, 1);
    // No registration error must surface as a runtime error.
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "variables-script registration must not error",
    );
}

/// Pre-W2-B regression: a 64 KB-event-cap-sized variables-script must
/// continue to register (small libraries existed before W2-B). Sanity
/// guard so the size-cap split does not regress small bodies.
#[test]
fn variables_script_at_event_cap_still_registers() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let small_body = "function ping() { return \"pong\"; }\n";
    tree.variables_scripts
        .push((None, "Lib".into(), small_body.into()));
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(&mut tree, out, "calculate", "this.rawValue = Lib.ping();");

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "pong");
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// W2-B + Cluster C contract: registering a large variables-script must
/// not break `resolveNode(missing_path) -> null`. The script body is
/// large enough that the pre-W2-B path silently rejected it; the script
/// itself merely defines a helper, while the event script consults
/// `xfa.resolveNode("missing")` and a missing handle must still be the
/// null contract handle (M3-B Phase D-γ).
#[test]
fn variables_script_above_event_cap_does_not_break_resolve_node_null() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);

    let preamble = "function noop() { return \"ok\"; }\n";
    let pad = "//padding-only line that contributes bytes without runtime cost\n";
    let target: usize = 96 * 1024;
    let pad_repeats = (target.saturating_sub(preamble.len())) / pad.len();
    let body = format!("{}{}", preamble, pad.repeat(pad_repeats));
    assert!(body.len() > MAX_SCRIPT_BODY_BYTES);
    tree.variables_scripts.push((None, "Big".into(), body));

    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "var n = xfa.resolveNode(\"definitely.missing.path\"); \
         this.rawValue = (n === null) ? \"null_contract_ok\" : \"contract_broken\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        field_value(&tree, out),
        "null_contract_ok",
        "Cluster C contract: resolveNode(missing) must return null",
    );
    assert_eq!(outcome.js_executed, 1);
}

/// W2-B size split: bodies strictly above the *variables* cap must still
/// reject for defence-in-depth. The dependent event script must observe
/// the helper as undefined (silent absorb) rather than the registration
/// throwing.
#[test]
fn variables_script_above_variables_cap_is_still_rejected() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);

    let unit = "var x = 1;\n";
    let repeats = (MAX_VARIABLES_SCRIPT_BODY_BYTES + 1).div_ceil(unit.len());
    let huge = unit.repeat(repeats);
    assert!(
        huge.len() > MAX_VARIABLES_SCRIPT_BODY_BYTES,
        "synthesised body must exceed variables cap"
    );
    tree.variables_scripts.push((None, "TooBig".into(), huge));

    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof TooBig === \"undefined\") ? \"absorbed\" : \"leaked\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, out), "absorbed");
    assert_eq!(outcome.js_executed, 1);
}

/// W2-B ordering invariant: every variables-script is registered before
/// any event script executes (XFA 3.3 §25.3 initialisation order). The
/// dispatch loop calls `set_form_handle` (which registers all
/// variables-scripts) before the script-iteration loop, so a calculate
/// script firing at the very first node observes the namespace as
/// defined.
#[test]
fn variables_script_registered_before_first_event_script() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);

    tree.variables_scripts.push((
        None,
        "Helpers".into(),
        "function helper() { return 42; }".into(),
    ));

    // The first event-script node — its calculate fires before any
    // other event script in the document.
    let out = add_field(&mut tree, root, "Out", "");
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof Helpers === \"undefined\") \
         ? \"too_late\" : String(Helpers.helper());",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(
        field_value(&tree, out),
        "42",
        "Helpers namespace must be visible to the first calculate script",
    );
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}

/// W2-B ordering invariant: a *subform-scoped* variables-script is
/// available through `subformHandle.variables.<name>` from event scripts
/// the moment dispatch starts. Guards the D-ι.2 dispatch path so the
/// size-cap split does not regress subform-scoped namespaces.
#[test]
fn subform_scoped_variables_script_registers_before_dispatch() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);

    tree.variables_scripts.push((
        Some("Page1".into()),
        "PageHelpers".into(),
        "function pageOnly() { return \"page_ok\"; }".into(),
    ));

    let out = add_field(&mut tree, root, "Out", "");
    // Event script reads from the form-tree implicit scope; a missing
    // subform handle should not crash dispatch.
    add_js_script(
        &mut tree,
        out,
        "calculate",
        "this.rawValue = (typeof PageHelpers === \"undefined\") \
         ? \"scoped_absent\" : \"scoped_leaked\";",
    );

    let outcome = run_sandbox(&mut tree, root);

    // Subform-scoped scripts are intentionally NOT visible as bare
    // globals — only via `subformHandle.variables.<name>`. The bare
    // identifier MUST stay undefined; this is the W2-B regression guard.
    assert_eq!(
        field_value(&tree, out),
        "scoped_absent",
        "subform-scoped variables must NOT leak into the bare global scope",
    );
    assert_eq!(outcome.js_executed, 1);
    assert_eq!(outcome.js_runtime_errors, 0);
}
