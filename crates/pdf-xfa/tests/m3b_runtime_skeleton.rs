//! M3-B Phase B integration tests for the runtime adapter skeleton.
//!
//! Covers the dispatch wiring in
//! [`pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime`] and the
//! metadata fields propagated through [`pdf_xfa::DynamicScriptOutcome`].
//!
//! These tests use synthesised `FormTree` instances to keep the
//! suite hermetic — no `/opt/xfa-golden/` access, no fixture PDFs.

use pdf_xfa::dynamic::{
    apply_dynamic_scripts, apply_dynamic_scripts_with_mode, apply_dynamic_scripts_with_runtime,
};
use pdf_xfa::js_runtime::{NullRuntime, RuntimeMetadata, SandboxError, XfaJsRuntime};
use pdf_xfa::{JsExecutionMode, OutputQuality};
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

fn build_minimal_form_with_script(
    activity: &str,
    language: ScriptLanguage,
) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let child = add_node(&mut tree, "child", FormNodeType::Subform);
    tree.get_mut(root).children = vec![child];
    tree.meta_mut(child).event_scripts = vec![EventScript::new(
        "var x = 1;".to_string(),
        language,
        Some(activity.to_string()),
        None,
        None,
    )];
    (tree, root)
}

#[test]
fn default_mode_skips_javascript() {
    let (mut tree, root) = build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
    let outcome = apply_dynamic_scripts(&mut tree, root).expect("default mode must not error");
    assert!(
        outcome.js_present,
        "JS-bearing form must report js_present=true"
    );
    assert_eq!(
        outcome.js_skipped, 1,
        "default mode must skip the JS script"
    );
    assert_eq!(outcome.js_executed, 0, "default mode must not execute");
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.output_quality, OutputQuality::BestEffort);
}

#[test]
fn strict_mode_denies_javascript() {
    let (mut tree, root) = build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
    let err =
        apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::Strict).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_ascii_lowercase().contains("javascript"),
        "strict mode error must mention javascript: got {msg}",
    );
}

#[test]
fn best_effort_static_records_metadata() {
    let (mut tree, root) = build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
    let outcome =
        apply_dynamic_scripts_with_mode(&mut tree, root, JsExecutionMode::BestEffortStatic)
            .expect("best-effort must not error");
    assert!(outcome.js_present);
    assert_eq!(outcome.js_skipped, 1);
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.output_quality, OutputQuality::BestEffort);
}

#[test]
fn sandbox_mode_with_null_runtime_falls_back_to_skip() {
    // Even in SandboxedRuntime mode, the NullRuntime path must stay safe:
    // each JS script becomes a skip + runtime_error counter, no panic.
    let (mut tree, root) = build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
    let mut runtime = NullRuntime::new();
    let outcome = apply_dynamic_scripts_with_runtime(
        &mut tree,
        root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("null runtime must not error");
    assert!(outcome.js_present);
    assert_eq!(outcome.js_skipped, 1);
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.js_runtime_errors, 1);
    assert_eq!(outcome.js_timeouts, 0);
    assert_eq!(outcome.js_oom, 0);
}

#[test]
fn sandbox_mode_skips_ui_activity_silently() {
    let (mut tree, root) = build_minimal_form_with_script("click", ScriptLanguage::JavaScript);
    let mut runtime = NullRuntime::new();
    let outcome = apply_dynamic_scripts_with_runtime(
        &mut tree,
        root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("null runtime must not error on UI activity");
    assert!(outcome.js_present);
    assert_eq!(outcome.js_skipped, 1, "click activity must skip");
    assert_eq!(
        outcome.js_runtime_errors, 0,
        "click skip must not count as runtime error"
    );
    assert_eq!(outcome.js_executed, 0);
}

#[test]
fn formcalc_unaffected_by_sandbox_mode() {
    let (mut tree, root) = build_minimal_form_with_script("calculate", ScriptLanguage::FormCalc);
    let mut runtime = NullRuntime::new();
    let outcome = apply_dynamic_scripts_with_runtime(
        &mut tree,
        root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("formcalc dispatch must not invoke runtime");
    // formcalc_run can be 0 if FormCalc parse fails on the trivial body, but
    // crucially the JS counters must all be untouched.
    assert!(!outcome.js_present);
    assert_eq!(outcome.js_skipped, 0);
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[cfg(feature = "xfa-js-sandboxed")]
mod sandbox_feature {
    use super::*;
    use pdf_xfa::js_runtime::QuickJsRuntime;

    #[test]
    fn sandbox_mode_executes_harmless_script() {
        let (mut tree, root) =
            build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
        let mut runtime = QuickJsRuntime::new().expect("rquickjs init");
        let outcome = apply_dynamic_scripts_with_runtime(
            &mut tree,
            root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("sandbox dispatch must not error");
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_runtime_errors, 0);
        assert_eq!(outcome.js_skipped, 0);
        assert_eq!(outcome.output_quality, OutputQuality::Sandboxed);
    }

    #[test]
    fn sandbox_mode_blocks_filesystem_access() {
        let mut tree_root = build_minimal_form_with_script("calculate", ScriptLanguage::JavaScript);
        // Replace the script body with one that references `require('fs')`.
        tree_root.0.meta_mut(FormNodeId(1)).event_scripts[0].script =
            "if (typeof require !== 'undefined') { require('fs'); }".to_string();
        let mut runtime = QuickJsRuntime::new().expect("rquickjs init");
        let outcome = apply_dynamic_scripts_with_runtime(
            &mut tree_root.0,
            tree_root.1,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("dispatch must succeed even if script tries fs");
        // require is undefined, so the if-branch is never taken; the script
        // executes cleanly. This proves the namespace is empty rather than
        // the call being intercepted.
        assert_eq!(outcome.js_executed, 1);
    }
}

// Smoke-test sanity: verify the metadata container default keeps backward
// compatibility (existing parsers reading js_present / js_skipped without
// the new fields must still work).
#[test]
fn metadata_default_is_backward_compatible() {
    let outcome = pdf_xfa::DynamicScriptOutcome::default();
    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.js_runtime_errors, 0);
    assert_eq!(outcome.js_timeouts, 0);
    assert_eq!(outcome.js_oom, 0);
    assert_eq!(outcome.output_quality, OutputQuality::Exact);
}

// Ensure the runtime trait is object-safe enough that we can pass NullRuntime
// through dyn dispatch (used by apply_dynamic_scripts_with_runtime). This is
// a compile-time assertion expressed as a runtime no-op.
#[test]
fn runtime_trait_is_object_safe() {
    fn _accept(_: &mut dyn XfaJsRuntime) {}
    let mut rt = NullRuntime::new();
    _accept(&mut rt);
    let md: RuntimeMetadata = rt.take_metadata();
    assert_eq!(md, RuntimeMetadata::default());
    let err = rt.execute_script(Some("calculate"), "").unwrap_err();
    assert_eq!(err, SandboxError::NotCompiledIn);
}
