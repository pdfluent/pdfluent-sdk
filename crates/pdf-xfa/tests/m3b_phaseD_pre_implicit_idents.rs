#![cfg(feature = "xfa-js-sandboxed")]

//! M3-B Phase D-pre integration tests for implicit JavaScript identifiers.

use pdf_xfa::dynamic::apply_dynamic_scripts_with_runtime;
use pdf_xfa::js_runtime::{HostBindings, QuickJsRuntime, MAX_RESOLVE_CALLS_PER_SCRIPT};
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

fn add_child(
    tree: &mut FormTree,
    parent: FormNodeId,
    name: &str,
    node_type: FormNodeType,
) -> FormNodeId {
    let child = add_node(tree, name, node_type);
    tree.get_mut(parent).children.push(child);
    child
}

fn add_field(tree: &mut FormTree, parent: FormNodeId, name: &str, value: &str) -> FormNodeId {
    add_child(
        tree,
        parent,
        name,
        FormNodeType::Field {
            value: value.to_string(),
        },
    )
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

fn field_value(tree: &FormTree, node_id: FormNodeId) -> &str {
    match &tree.get(node_id).node_type {
        FormNodeType::Field { value } => value,
        _ => panic!("expected field"),
    }
}

fn run_sandbox(tree: &mut FormTree, root: FormNodeId) -> pdf_xfa::DynamicScriptOutcome {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");
    apply_dynamic_scripts_with_runtime(tree, root, JsExecutionMode::SandboxedRuntime, &mut runtime)
        .expect("sandbox dispatch")
}

#[test]
fn bare_ident_resolves_sibling_field_raw_value() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _b1 = add_field(&mut tree, root, "B1", "alpha");
    let total = add_field(&mut tree, root, "Total", "");
    add_js_script(
        &mut tree,
        total,
        "calculate",
        "Total.rawValue = B1.rawValue;",
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, total), "alpha");
    assert_eq!(outcome.js_mutations, 1);
}

#[test]
fn chained_dotted_path_two_segments() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let sub1 = add_child(&mut tree, root, "Sub1", FormNodeType::Subform);
    let field1 = add_field(&mut tree, sub1, "Field1", "");
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        r#"Sub1.Field1.rawValue = "x";"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, field1), "x");
    assert_eq!(outcome.js_mutations, 1);
}

#[test]
fn chained_dotted_path_three_segments() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let sub1 = add_child(&mut tree, root, "Sub1", FormNodeType::Subform);
    let sub2 = add_child(&mut tree, sub1, "Sub2", FormNodeType::Subform);
    let field1 = add_field(&mut tree, sub2, "Field1", "");
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        r#"Sub1.Sub2.Field1.rawValue = "deep";"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, field1), "deep");
    assert_eq!(outcome.js_mutations, 1);
}

#[test]
fn unresolved_bare_ident_returns_undefined_safely() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let driver = add_field(&mut tree, root, "Driver", "unchanged");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        r#"DoesNotExist.rawValue = "x";"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, driver), "unchanged");
    assert_eq!(outcome.js_runtime_errors, 1);
    assert_eq!(outcome.js_resolve_failures, 1);
}

#[test]
fn is_null_getter_true_on_empty() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _empty = add_field(&mut tree, root, "Empty", "");
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        r#"
if (!Empty.isNull) throw new Error("expected null");
this.rawValue = "ok";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, driver), "ok");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn is_null_getter_false_on_populated() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _full = add_field(&mut tree, root, "Full", "value");
    let driver = add_field(&mut tree, root, "Driver", "");
    add_js_script(
        &mut tree,
        driver,
        "calculate",
        r#"
if (Full.isNull) throw new Error("expected value");
this.rawValue = "ok";
"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, driver), "ok");
    assert_eq!(outcome.js_runtime_errors, 0);
}

#[test]
fn arithmetic_sum_via_bare_idents() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _b1 = add_field(&mut tree, root, "B1", "1");
    let _b2 = add_field(&mut tree, root, "B2", "2");
    let _b3 = add_field(&mut tree, root, "B3", "3");
    let total = add_field(&mut tree, root, "Total", "");
    add_js_script(
        &mut tree,
        total,
        "calculate",
        r#"Total.rawValue = Number(B1.rawValue) + Number(B2.rawValue) + Number(B3.rawValue);"#,
    );

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, total), "6");
    assert_eq!(outcome.js_mutations, 1);
}

#[test]
fn mutation_log_records_implicit_writes() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let _b1 = add_field(&mut tree, root, "B1", "source");
    let total = add_field(&mut tree, root, "Total", "");

    let mut host = HostBindings::new();
    host.reset_per_document();
    host.set_form_handle(&mut tree as *mut FormTree, root);
    host.reset_per_script(total, Some("calculate"));
    let generation = host.generation();

    assert_eq!(host.resolve_implicit(total, "B1").map(|id| id.0), Some(1));
    assert!(host.set_raw_value(total, "source".to_string(), generation));

    let log = host.mutation_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].node_id, total);
    assert_eq!(log[0].before, "");
    assert_eq!(log[0].after, "source");
}

#[test]
fn script_with_with_statement_does_not_leak_globals_outside() {
    let mut runtime = QuickJsRuntime::new().expect("quickjs runtime");

    let mut first = FormTree::new();
    let first_root = add_node(&mut first, "root", FormNodeType::Root);
    let first_driver = add_field(&mut first, first_root, "Driver", "");
    add_js_script(
        &mut first,
        first_driver,
        "calculate",
        "LeakedByImplicitProxy = 123;",
    );
    let first_outcome = apply_dynamic_scripts_with_runtime(
        &mut first,
        first_root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("first dispatch");
    assert_eq!(first_outcome.js_runtime_errors, 0);

    let mut second = FormTree::new();
    let second_root = add_node(&mut second, "root", FormNodeType::Root);
    let second_driver = add_field(&mut second, second_root, "Driver", "");
    add_js_script(
        &mut second,
        second_driver,
        "calculate",
        r#"
if (typeof LeakedByImplicitProxy !== "undefined") throw new Error("global leaked");
this.rawValue = "ok";
"#,
    );
    let second_outcome = apply_dynamic_scripts_with_runtime(
        &mut second,
        second_root,
        JsExecutionMode::SandboxedRuntime,
        &mut runtime,
    )
    .expect("second dispatch");

    assert_eq!(field_value(&second, second_driver), "ok");
    assert_eq!(second_outcome.js_runtime_errors, 0);
}

#[test]
fn cap_enforced_at_max_resolve_calls_per_script() {
    let mut tree = FormTree::new();
    let root = add_node(&mut tree, "root", FormNodeType::Root);
    let driver = add_field(&mut tree, root, "Driver", "");
    let script = format!(
        r#"
for (var i = 0; i < {}; i++) {{
  this.MissingChild;
}}
"#,
        MAX_RESOLVE_CALLS_PER_SCRIPT + 5
    );
    add_js_script(&mut tree, driver, "calculate", &script);

    let outcome = run_sandbox(&mut tree, root);

    assert_eq!(field_value(&tree, driver), "");
    assert!(outcome.js_binding_errors >= 1);
}
