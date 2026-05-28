//! Integration test: JavaScript calculate scripts dispatched via the layout engine.
//!
//! Exercises the `application/x-javascript` path in `scripting::run_js_calculations`.
//! Uses programmatic FormTree construction — no real XFA PDF needed.

use std::sync::{atomic::AtomicBool, Arc};

use xfa_js_sandboxed::XfaJsRuntime;
use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::scripting::run_js_calculations;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn make_field(tree: &mut FormTree, name: &str, initial: &str) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Field {
            value: initial.to_string(),
        },
        box_model: BoxModel {
            width: Some(100.0),
            height: Some(20.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    })
}

fn field_value(tree: &FormTree, id: FormNodeId) -> &str {
    match &tree.nodes[id.0].node_type {
        FormNodeType::Field { value } => value.as_str(),
        other => panic!("expected Field, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn js_calculate_writes_raw_value() {
    let mut form = FormTree::new();
    let total_id = make_field(&mut form, "Total", "0");

    let mut rt = XfaJsRuntime::new().expect("runtime init");
    let scripts: Vec<(FormNodeId, &str)> = vec![(total_id, "xfa.form.Total.rawValue = 10 + 20;")];
    run_js_calculations(&mut form, &scripts, &mut rt, no_cancel()).unwrap();

    assert_eq!(field_value(&form, total_id), "30");
}

#[test]
fn js_calculate_cross_field_reference() {
    let mut form = FormTree::new();
    let price_id = make_field(&mut form, "Price", "15");
    let qty_id = make_field(&mut form, "Qty", "4");
    let total_id = make_field(&mut form, "Total", "0");

    let script =
        "xfa.form.Total.rawValue = Number(xfa.form.Price.rawValue) * Number(xfa.form.Qty.rawValue);";
    let scripts: Vec<(FormNodeId, &str)> = vec![(total_id, script)];

    let mut rt = XfaJsRuntime::new().expect("runtime init");
    run_js_calculations(&mut form, &scripts, &mut rt, no_cancel()).unwrap();

    assert_eq!(field_value(&form, total_id), "60");
    // Source fields must not change.
    assert_eq!(field_value(&form, price_id), "15");
    assert_eq!(field_value(&form, qty_id), "4");
}

#[test]
fn js_script_error_is_skipped_best_effort() {
    let mut form = FormTree::new();
    let field_id = make_field(&mut form, "V", "original");

    // A script that throws should leave the field untouched.
    let scripts: Vec<(FormNodeId, &str)> = vec![(field_id, "throw new Error('fail');")];

    let mut rt = XfaJsRuntime::new().expect("runtime init");
    run_js_calculations(&mut form, &scripts, &mut rt, no_cancel()).unwrap();

    assert_eq!(field_value(&form, field_id), "original");
}

#[test]
fn multiple_js_scripts_run_in_order() {
    let mut form = FormTree::new();
    let a_id = make_field(&mut form, "A", "0");
    let b_id = make_field(&mut form, "B", "0");
    let c_id = make_field(&mut form, "C", "0");

    let scripts: Vec<(FormNodeId, &str)> = vec![
        (a_id, "xfa.form.A.rawValue = 1;"),
        (
            b_id,
            "xfa.form.B.rawValue = Number(xfa.form.A.rawValue) + 1;",
        ),
        (
            c_id,
            "xfa.form.C.rawValue = Number(xfa.form.B.rawValue) + 1;",
        ),
    ];

    let mut rt = XfaJsRuntime::new().expect("runtime init");
    run_js_calculations(&mut form, &scripts, &mut rt, no_cancel()).unwrap();

    assert_eq!(field_value(&form, a_id), "1");
    assert_eq!(field_value(&form, b_id), "2");
    assert_eq!(field_value(&form, c_id), "3");
}

#[test]
fn cancelled_before_execution_skips_all_scripts() {
    let mut form = FormTree::new();
    let id = make_field(&mut form, "X", "unchanged");

    let cancel = Arc::new(AtomicBool::new(true));
    let scripts: Vec<(FormNodeId, &str)> = vec![(id, "xfa.form.X.rawValue = \"changed\";")];

    let mut rt = XfaJsRuntime::new().expect("runtime init");
    run_js_calculations(&mut form, &scripts, &mut rt, cancel).unwrap();

    // Script was skipped because the runtime returned Cancelled immediately.
    assert_eq!(field_value(&form, id), "unchanged");
}

#[test]
fn formcalc_and_js_coexist() {
    // This test verifies that the existing FormCalc path (run_calculations)
    // and the new JS path (run_js_calculations) can operate on the same
    // FormTree without interfering with each other.
    use xfa_layout_engine::scripting::{run_calculations, run_js_calculations};

    let mut form = FormTree::new();
    let fc_id = make_field(&mut form, "FormCalcField", "0");
    let js_id = make_field(&mut form, "JsField", "0");

    // Give fc_id a FormCalc calculate script via the existing field.
    form.nodes[fc_id.0].calculate = Some("7 * 6".to_string());

    // Run FormCalc.
    run_calculations(&mut form).unwrap();
    assert_eq!(field_value(&form, fc_id), "42");

    // Run JS.
    let mut rt = XfaJsRuntime::new().expect("runtime init");
    let scripts: Vec<(FormNodeId, &str)> = vec![(js_id, "xfa.form.JsField.rawValue = 99;")];
    run_js_calculations(&mut form, &scripts, &mut rt, no_cancel()).unwrap();
    assert_eq!(field_value(&form, js_id), "99");

    // FormCalc result should still be intact.
    assert_eq!(field_value(&form, fc_id), "42");
}
