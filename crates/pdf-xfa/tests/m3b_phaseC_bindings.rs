//! M3-B Phase C integration tests for field-value host bindings.
//!
//! These use synthetic `FormTree` fixtures only; no corpus PDFs or benchmark
//! artifacts are read or written.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{HostBindings, NullRuntime, MAX_MUTATIONS_PER_DOC};
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

#[allow(dead_code)]
fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
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

#[test]
fn default_feature_null_runtime_phase_c_metadata_stays_zero() {
    let (mut tree, root, _field) = basic_form("calculate", "this.rawValue = 'changed';");
    let mut runtime = NullRuntime::new();
    let outcome = apply_dynamic_scripts_with_runtime(
        &mut tree,
        root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("null runtime dispatch is fail-open");

    assert_eq!(outcome.js_executed, 0);
    assert_eq!(outcome.js_skipped, 1);
    assert_eq!(outcome.js_host_calls, 0);
    assert_eq!(outcome.js_mutations, 0);
    assert_eq!(outcome.js_binding_errors, 0);
    assert_eq!(outcome.js_resolve_failures, 0);
}

#[test]
fn host_binding_cap_records_binding_error_without_quickjs() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let field = add_node(
        &mut tree,
        "Field1",
        FormNodeType::Field {
            value: String::new(),
        },
    );
    tree.get_mut(root).children = vec![field];

    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(field, Some("calculate"));
    let generation = host.generation();

    for idx in 0..MAX_MUTATIONS_PER_DOC {
        assert!(host.set_raw_value(field, idx.to_string(), generation));
    }
    assert!(!host.set_raw_value(field, "overflow".to_string(), generation));
    let metadata = host.take_metadata();
    assert_eq!(metadata.mutations, MAX_MUTATIONS_PER_DOC);
    assert_eq!(metadata.binding_errors, 1);
}

#[cfg(feature = "xfa-js-sandboxed")]
mod sandbox {
    use super::*;
    use pdf_xfa::js_runtime::QuickJsRuntime;
    use pdf_xfa::OutputQuality;

    fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
        let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
        apply_dynamic_scripts_with_runtime(
            tree,
            root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("sandbox dispatch")
    }

    #[test]
    fn raw_value_get_existing_field_returns_value() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
if (f.rawValue !== "hello") throw new Error("rawValue get failed");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_host_calls, 2);
        assert_eq!(outcome.output_quality, OutputQuality::Sandboxed);
    }

    #[test]
    fn raw_value_get_non_field_resolves_to_null() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let section = add_node(&mut tree, "Section", FormNodeType::Subform);
        let driver = add_node(
            &mut tree,
            "Driver",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        tree.get_mut(root).children = vec![section, driver];
        add_js_script(
            &mut tree,
            driver,
            "calculate",
            r#"if (xfa.resolveNode("Section") !== null) throw new Error("expected null");"#,
        );

        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_resolve_failures, 1);
        assert_eq!(outcome.output_quality, OutputQuality::BestEffort);
    }

    #[test]
    fn raw_value_set_under_calculate_mutates_form_tree() {
        let (mut tree, root, field) = basic_form("calculate", "this.rawValue = 42;");
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "42");
        assert_eq!(outcome.js_mutations, 1);
        assert_eq!(outcome.changes, 1);
    }

    #[test]
    fn raw_value_set_under_validate_mutates_form_tree() {
        let (mut tree, root, field) = basic_form("validate", "this.rawValue = 'bad';");
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "bad");
        assert_eq!(outcome.js_mutations, 1);
        assert_eq!(outcome.js_binding_errors, 0);
    }

    #[test]
    fn raw_value_set_under_doc_ready_mutates_form_tree() {
        let (mut tree, root, field) = basic_form("docReady", "this.rawValue = 'bad';");
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "bad");
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_mutations, 1);
        assert_eq!(outcome.js_binding_errors, 0);
    }

    #[test]
    fn resolve_node_missing_path_returns_null_and_counts_failure() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"if (xfa.resolveNode("DoesNotExist") !== null) throw new Error("expected null");"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_resolve_failures, 1);
        assert_eq!(outcome.js_binding_errors, 0);
    }

    #[test]
    fn resolve_node_class_selector_returns_field_handle() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r##"
var f = xfa.resolveNode("#field");
if (f.rawValue !== "hello") throw new Error("class selector failed");
"##,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_resolve_failures, 0);
    }

    #[test]
    fn resolve_nodes_returns_multiple_matches_and_caps_results() {
        let mut tree = FormTree::new();
        let root = add_node(&mut tree, "root", FormNodeType::Root);
        let driver = add_node(
            &mut tree,
            "Driver",
            FormNodeType::Field {
                value: String::new(),
            },
        );
        let mut children = vec![driver];
        for idx in 0..300 {
            children.push(add_node(
                &mut tree,
                "Item",
                FormNodeType::Field {
                    value: idx.to_string(),
                },
            ));
        }
        tree.get_mut(root).children = children;
        add_js_script(
            &mut tree,
            driver,
            "calculate",
            r#"
var items = xfa.resolveNodes("Item[*]");
if (items.length !== 256) throw new Error("resolveNodes cap failed");
if (items[0].rawValue !== "0") throw new Error("first handle failed");
"#,
        );

        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_resolve_failures, 0);
    }

    #[test]
    fn page_count_bindings_return_same_static_value() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
if (xfa.host.numPages !== 0) throw new Error("numPages changed");
if (xfa.layout.pageCount() !== xfa.host.numPages) throw new Error("pageCount mismatch");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_executed, 1);
        assert_eq!(outcome.js_host_calls, 3);
    }

    #[test]
    fn layout_page_stubs_mark_best_effort_without_rollback() {
        let (mut tree, root, field) = basic_form(
            "calculate",
            r#"
this.rawValue = String(xfa.layout.page(this)) + "/" + String(xfa.layout.pageSpan(this));
"#,
        );

        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "1/1");
        assert_eq!(outcome.js_binding_errors, 0);
        assert_eq!(outcome.js_resolve_failures, 2);
        assert_eq!(outcome.output_quality, OutputQuality::BestEffort);
        assert_eq!(outcome.changes, 1);
    }

    #[test]
    fn util_date_subset_is_deterministic_and_safe() {
        let (mut tree, root, field) = basic_form(
            "calculate",
            r#"
var d = util.scand("yyyy-mm-dd HH:MM:SS", "2026-05-05 13:04:09");
var bad = util.scand("yyyy-mm-dd", "2026-02-31");
this.rawValue = util.printd("yyyy-mm-dd HH:MM:SS", d) + "|" +
  (util.printd("yyyy", bad) === "" ? "invalid" : "bad");
"#,
        );

        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_runtime_errors, 0);
        assert_eq!(field_value(&tree, field), "2026-05-05 13:04:09|invalid");
    }

    #[test]
    fn field_handle_is_frozen_and_narrow() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
f.bypass = 1;
if (f.bypass !== undefined) throw new Error("handle accepted new property");
if (f._id !== undefined) throw new Error("_id leaked");
var keys = Object.keys(f);
if (keys.length !== 1 || keys[0] !== "rawValue") throw new Error("handle keys leaked");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_runtime_errors, 0);
        assert_eq!(outcome.js_binding_errors, 0);
    }

    #[test]
    fn xfa_prototype_pollution_does_not_escape() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
xfa.__proto__.bypass = function() { return 1; };
if (xfa.bypass !== undefined) throw new Error("prototype pollution escaped");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_runtime_errors, 0);
    }

    #[test]
    fn instance_manager_count_is_available_on_handles() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
if (f.instanceManager.count !== 1) throw new Error("instanceManager count failed");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_runtime_errors, 0);
    }

    #[test]
    fn phase_b_capability_guards_remain() {
        let (mut tree, root, _field) = basic_form(
            "calculate",
            r#"
if (typeof require !== "undefined") throw new Error("require leaked");
if (typeof fetch !== "undefined") throw new Error("fetch leaked");
if (Date.now() !== 0) throw new Error("Date.now changed");
if (typeof Math.random !== "undefined") throw new Error("Math.random leaked");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.output_quality, OutputQuality::Sandboxed);
    }

    #[test]
    fn repeated_runs_are_deterministic() {
        let (mut left, left_root, left_field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
f.rawValue = "deterministic";
"#,
        );
        let (mut right, right_root, right_field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
f.rawValue = "deterministic";
"#,
        );

        let left_outcome = run_sandbox(&mut left, left_root);
        let right_outcome = run_sandbox(&mut right, right_root);
        assert_eq!(
            field_value(&left, left_field),
            field_value(&right, right_field)
        );
        assert_eq!(left_outcome, right_outcome);
    }

    #[test]
    fn cross_document_handle_invalidates_after_reset() {
        let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

        let (mut first, first_root, _first_field) = basic_form(
            "calculate",
            r#"globalThis.savedField = xfa.resolveNode("Field1");"#,
        );
        let first_outcome = apply_dynamic_scripts_with_runtime(
            &mut first,
            first_root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("first dispatch");
        assert_eq!(first_outcome.js_executed, 1);

        let (mut second, second_root, _second_field) = basic_form(
            "calculate",
            r#"
if (globalThis.savedField.rawValue !== null) {
  throw new Error("stale handle stayed live");
}
"#,
        );
        let second_outcome = apply_dynamic_scripts_with_runtime(
            &mut second,
            second_root,
            JsExecutionMode::SandboxedRuntime,
            &mut runtime,
        )
        .expect("second dispatch");
        assert_eq!(second_outcome.js_runtime_errors, 0);
    }

    #[test]
    fn three_writes_record_three_mutations() {
        let (mut tree, root, field) = basic_form(
            "calculate",
            r#"
var f = xfa.resolveNode("Field1");
f.rawValue = "one";
f.rawValue = "two";
f.rawValue = "three";
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "three");
        assert_eq!(outcome.js_mutations, 3);
    }

    #[test]
    fn mutation_rolls_back_when_script_errors_after_write() {
        let (mut tree, root, field) = basic_form(
            "calculate",
            r#"
this.rawValue = "bad";
throw new Error("boom");
"#,
        );
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(field_value(&tree, field), "hello");
        assert_eq!(outcome.js_runtime_errors, 1);
        assert_eq!(outcome.js_mutations, 1);
        assert_eq!(outcome.changes, 0);
    }

    #[test]
    fn over_depth_resolve_node_returns_null_and_binding_error() {
        let long_path = (0..=pdf_xfa::MAX_SOM_DEPTH)
            .map(|idx| format!("n{idx}"))
            .collect::<Vec<_>>()
            .join(".");
        let script = format!(
            r#"if (xfa.resolveNode("{long_path}") !== null) throw new Error("expected null");"#
        );
        let (mut tree, root, _field) = basic_form("calculate", &script);
        let outcome = run_sandbox(&mut tree, root);
        assert_eq!(outcome.js_binding_errors, 1);
    }
}
